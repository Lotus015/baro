/**
 * High-level entry: build a Mozaik environment with all the standard
 * baro participants, run a PRD to completion, return a summary.
 *
 * Used by:
 *   - the Rust orchestrator client (via run-orchestrator.ts)
 *   - direct TS callers (tests, demos)
 */

import { mkdirSync } from "fs"
import { dirname } from "path"

import {
    AgenticEnvironment,
    ContextItem,
    FunctionCallItem,
    FunctionCallOutputItem,
    ModelMessageItem,
    Participant,
} from "@mozaik-ai/core"

import {
    GitGate,
    createOrCheckoutBranch,
    getGitFileStats,
    getHeadSha,
    gitPushWithRetry,
    isInsideGitRepo,
    safePullRebase,
} from "./git.js"
import { Auditor } from "./participants/auditor.js"
import {
    Conductor,
    ConductorRunSummary,
    ConductorStateItem,
} from "./participants/conductor.js"
import { Operator } from "./participants/operator.js"
import { StoryResultItem, type StoryAgent } from "./participants/story-agent.js"
import { PrdFile, loadPrd } from "./prd.js"
import {
    AgentStateItem,
    ClaudeResultItem,
    ClaudeSystemItem,
} from "./types.js"
import { emit } from "./tui-protocol.js"

export interface OrchestrateConfig {
    prdPath: string
    cwd: string
    parallel?: number
    timeoutSecs?: number
    overrideModel?: string | null
    defaultModel?: string
    /** Optional path for the audit JSONL log. If omitted, no Auditor joins. */
    auditLogPath?: string
    /**
     * If true, BaroEvents are emitted to stdout for a TUI consumer.
     * Default: true.
     */
    emitTuiEvents?: boolean
    /**
     * Whether to perform git lifecycle operations (branch create, push,
     * pull --rebase between stories). If undefined, auto-detected from
     * whether `cwd` is a git working tree.
     */
    withGit?: boolean
    /** Hooks for receiving Operator commands externally (Rust TUI). */
    operatorHooks?: {
        onAbort?: (storyId: string) => void
        onAbortAll?: () => void
        onShutdown?: () => void
    }
}

export interface OrchestrateResult {
    summary: ConductorRunSummary
    operator: Operator
    /** Active StoryAgents indexed by id, exposed for outside abort/inspection. */
    storyAgents: Map<string, StoryAgent>
}

/**
 * Build, run, and tear down the orchestration environment for a single
 * PRD execution.
 */
export async function orchestrate(
    config: OrchestrateConfig,
): Promise<OrchestrateResult> {
    const env = new AgenticEnvironment()
    const emitTui = config.emitTuiEvents ?? true

    // Optional audit log (resume + post-mortem).
    if (config.auditLogPath) {
        mkdirSync(dirname(config.auditLogPath), { recursive: true })
        new Auditor({ path: config.auditLogPath }).join(env)
    }

    // BaroEvent forwarder: watch the bus, translate to TUI protocol on stdout.
    if (emitTui) {
        new BaroEventForwarder().join(env)
    }

    // Operator listens for external commands (wired from caller).
    const operator = new Operator(config.operatorHooks ?? {})
    operator.setEnvironment(env)
    operator.join(env)

    const useGit = config.withGit ?? (await isInsideGitRepo(config.cwd))
    const gitGate = new GitGate()
    let baseSha: string | null = null

    // Conductor — the work driver.
    const conductor = new Conductor({
        prdPath: config.prdPath,
        cwd: config.cwd,
        parallel: config.parallel ?? 0,
        timeoutSecs: config.timeoutSecs ?? 600,
        overrideModel: config.overrideModel ?? undefined,
        defaultModel: config.defaultModel ?? "sonnet",
        onRunStart: useGit
            ? async (prd) => {
                  baseSha = await getHeadSha(config.cwd)
                  if (prd.branchName) {
                      await createOrCheckoutBranch(
                          config.cwd,
                          prd.branchName,
                          (line) => emitTui && emit({ type: "story_log", id: "_git", line }),
                      )
                  }
              }
            : undefined,
        onStoryPassed: useGit
            ? async (storyId) => {
                  await safePullRebase(config.cwd, (line) =>
                      emitTui && emit({ type: "story_log", id: storyId, line }),
                  )
                  try {
                      await gitPushWithRetry(gitGate, {
                          cwd: config.cwd,
                          onLog: (line) =>
                              emitTui &&
                              emit({ type: "story_log", id: storyId, line }),
                      })
                      if (emitTui) {
                          emit({
                              type: "push_status",
                              id: storyId,
                              success: true,
                              error: null,
                          })
                      }
                  } catch (e) {
                      if (emitTui) {
                          emit({
                              type: "push_status",
                              id: storyId,
                              success: false,
                              error: (e as Error)?.message ?? String(e),
                          })
                      }
                  }
              }
            : undefined,
    })
    conductor.join(env)

    // Emit `init` early so the TUI can render the story list before any
    // Claude process spawns.
    if (emitTui) {
        const prd = loadPrd(config.prdPath)
        emit({
            type: "init",
            project: prd.project,
            stories: prd.userStories.map((s) => ({
                id: s.id,
                title: s.title,
                depends_on: s.dependsOn,
            })),
        })
    }

    const summary = await conductor.run(env)

    let filesCreated = 0
    let filesModified = 0
    if (useGit && baseSha) {
        const stats = await getGitFileStats(config.cwd, baseSha)
        filesCreated = stats.created
        filesModified = stats.modified
    }

    if (emitTui) {
        emit({
            type: "done",
            total_time_secs: summary.totalDurationSecs,
            stats: {
                stories_completed: summary.completedStories.length,
                stories_skipped: 0,
                total_commits: 0,
                files_created: filesCreated,
                files_modified: filesModified,
            },
        })
    }

    return {
        summary,
        operator,
        storyAgents: new Map(),
    }
}

/**
 * Translates bus events into the legacy BaroEvent shape consumed by the
 * Rust TUI. Lives inside this module so callers don't have to wire
 * sinks themselves.
 */
class BaroEventForwarder extends Participant {
    /** Story IDs that have already received a `story_start`. */
    private startedStories = new Set<string>()
    /** Number of in-flight retry attempts per story (for `story_retry`). */
    private retryCounts = new Map<string, number>()
    /** Token-usage tally per story (incrementally updated from results). */
    private tokensByStory = new Map<string, { input: number; output: number }>()

    async onContextItem(source: Participant, item: ContextItem): Promise<void> {
        if (item instanceof ConductorStateItem) {
            this.handleConductorState(item)
            return
        }

        if (item instanceof StoryResultItem) {
            this.handleStoryResult(item)
            return
        }

        if (item instanceof ClaudeResultItem) {
            this.handleClaudeResult(item)
            return
        }

        if (item instanceof AgentStateItem) {
            this.handleAgentState(item)
            return
        }

        if (item instanceof ClaudeSystemItem) {
            // Mostly noise; emit only init transitions (already covered
            // by AgentStateItem) — skip.
            return
        }

        if (item instanceof ModelMessageItem) {
            this.handleModelMessage(source, item)
            return
        }

        if (item instanceof FunctionCallItem) {
            this.handleToolCall(source, item)
            return
        }

        if (item instanceof FunctionCallOutputItem) {
            this.handleToolResult(source, item)
            return
        }
    }

    private handleConductorState(item: ConductorStateItem): void {
        // Mirror conductor lifecycle as a `progress` event the existing
        // Rust TUI understands — it doesn't yet know `conductor_state`.
        if (
            item.phase === "running_level" &&
            item.currentLevel != null &&
            item.totalLevels != null
        ) {
            emit({
                type: "progress",
                completed: item.currentLevel - 1,
                total: item.totalLevels,
                percentage: Math.round(
                    ((item.currentLevel - 1) / Math.max(1, item.totalLevels)) * 100,
                ),
            })
        }
    }

    private handleStoryResult(item: StoryResultItem): void {
        if (item.success) {
            emit({
                type: "story_complete",
                id: item.storyId,
                duration_secs: item.durationSecs,
                files_created: 0,
                files_modified: 0,
            })
        } else {
            emit({
                type: "story_error",
                id: item.storyId,
                error: item.error ?? "unknown error",
                attempt: item.attempts,
                max_retries: item.attempts,
            })
        }
    }

    private handleClaudeResult(item: ClaudeResultItem): void {
        const usage = item.usage as
            | { input_tokens?: number; output_tokens?: number }
            | null
        const inputTokens =
            typeof usage?.input_tokens === "number" ? usage.input_tokens : 0
        const outputTokens =
            typeof usage?.output_tokens === "number"
                ? usage.output_tokens
                : 0
        const tally = this.tokensByStory.get(item.agentId) ?? { input: 0, output: 0 }
        tally.input += inputTokens
        tally.output += outputTokens
        this.tokensByStory.set(item.agentId, tally)
        emit({
            type: "token_usage",
            id: item.agentId,
            input_tokens: inputTokens,
            output_tokens: outputTokens,
        })
    }

    private handleAgentState(item: AgentStateItem): void {
        if (item.phase === "running" && !this.startedStories.has(item.agentId)) {
            this.startedStories.add(item.agentId)
            emit({ type: "story_start", id: item.agentId, title: item.agentId })
        }
        if (item.phase === "waiting" && item.detail?.includes("retrying")) {
            const count = (this.retryCounts.get(item.agentId) ?? 0) + 1
            this.retryCounts.set(item.agentId, count)
            emit({ type: "story_retry", id: item.agentId, attempt: count })
        }
    }

    private handleModelMessage(source: Participant, item: ModelMessageItem): void {
        const agentId = (source as unknown as { agentId?: string }).agentId
        if (typeof agentId !== "string") return
        const json = item.toJSON() as { content: Array<{ text: string }> }
        const text = json.content?.[0]?.text ?? ""
        if (!text.trim()) return
        emit({ type: "story_log", id: agentId, line: text })
    }

    private handleToolCall(source: Participant, item: FunctionCallItem): void {
        const agentId = (source as unknown as { agentId?: string }).agentId
        if (typeof agentId !== "string") return
        emit({
            type: "story_log",
            id: agentId,
            line: `[tool_call] ${item.name} ${item.args}`,
        })
    }

    private handleToolResult(
        source: Participant,
        item: FunctionCallOutputItem,
    ): void {
        const agentId = (source as unknown as { agentId?: string }).agentId
        if (typeof agentId !== "string") return
        const json = item.toJSON() as {
            call_id: string
            output: Array<{ text: string }>
        }
        const text = json.output?.[0]?.text ?? ""
        emit({
            type: "story_log",
            id: agentId,
            line: `[tool_result ${json.call_id}] ${text}`,
        })
    }
}

