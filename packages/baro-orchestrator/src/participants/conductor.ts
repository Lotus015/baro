/**
 * Conductor — top-level orchestrator participant. Reads a PRD, builds a
 * DAG, releases stories level by level (respecting `parallel` cap),
 * persists PRD updates as stories pass, emits run-level state events.
 *
 * This is the baro-specific glue: knows about PRD format, story IDs,
 * stories-as-DAG-nodes, and the on-disk prd.json. The participants it
 * spawns (StoryAgent → ClaudeCliParticipant) remain library-grade.
 */

import { existsSync, readFileSync } from "fs"
import { join } from "path"

import {
    AgenticEnvironment,
    ContextItem,
    Participant,
} from "@mozaik-ai/core"

import { buildDag } from "../dag.js"
import {
    PrdFile,
    PrdStory,
    buildDefaultStoryPrompt,
    loadPrd,
    markStoryPassed,
    savePrd,
} from "../prd.js"
import { ReplanItem } from "../types.js"
import { StoryAgent, StoryOutcome } from "./story-agent.js"

export interface ConductorOptions {
    /** Path to prd.json (read + write). */
    prdPath: string
    /** Working directory passed to each story's Claude. */
    cwd: string
    /** Max stories running concurrently within a level. 0 = unlimited. */
    parallel?: number
    /** Per-story timeout in seconds. Default: 600. */
    timeoutSecs?: number
    /** Override model for every story (ignores per-story PRD model). */
    overrideModel?: string
    /** Default model when neither overrideModel nor PRD model is set. */
    defaultModel?: string
    /**
     * Optional CLAUDE.md / context content to prepend to the system
     * prompt. Currently ignored at the orchestrator level since Claude
     * CLI auto-loads CLAUDE.md from the cwd; passing it through is left
     * for a future enhancement.
     */
    contextContent?: string
    /** Optional path-builder for per-story prompt template overrides. */
    promptTemplatePath?: string
    /**
     * Optional callback fired after each story passes. Used by callers
     * to tack on side-effects like git push or file-stat capture.
     * Errors thrown here become non-fatal warnings.
     */
    onStoryPassed?: (storyId: string) => Promise<void> | void
    /**
     * Optional callback fired before any story runs (after PRD load).
     * Useful for setting up git branches.
     */
    onRunStart?: (prd: PrdFile) => Promise<void> | void
    /**
     * Optional callback fired immediately before each story's
     * StoryAgent is launched. Returns extra context (e.g. cross-agent
     * findings from Librarian) to prepend to the story's prompt.
     * Returning `null`/`undefined` leaves the prompt unchanged.
     */
    onBeforeStoryLaunch?: (
        storyId: string,
        story: PrdStory,
    ) => Promise<string | null | undefined> | string | null | undefined
    /**
     * Optional callback fired after the entire run completes
     * (regardless of success).
     */
    onRunComplete?: (
        summary: ConductorRunSummary,
    ) => Promise<void> | void
}

export interface ConductorRunSummary {
    completedStories: string[]
    failedStories: string[]
    totalDurationSecs: number
    totalAttempts: number
}

export class ConductorStateItem extends ContextItem {
    readonly type = "conductor_state"

    constructor(
        public readonly phase: "loading" | "running_level" | "level_complete" | "done" | "failed",
        public readonly detail?: string,
        public readonly currentLevel?: number,
        public readonly totalLevels?: number,
        public readonly storyIds?: readonly string[],
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            phase: this.phase,
            detail: this.detail,
            currentLevel: this.currentLevel,
            totalLevels: this.totalLevels,
            storyIds: this.storyIds,
        }
    }
}

export class Conductor extends Participant {
    private readonly opts: Required<
        Pick<ConductorOptions, "parallel" | "timeoutSecs" | "defaultModel">
    > &
        ConductorOptions

    private envRef: AgenticEnvironment | null = null

    /**
     * ReplanItem-s emitted while a level is in flight are buffered here
     * and applied at the next level boundary. Mid-level mutation would
     * leave running StoryAgents orphaned; level boundaries are the safe
     * apply points.
     */
    private readonly pendingReplans: ReplanItem[] = []

    constructor(opts: ConductorOptions) {
        super()
        this.opts = {
            parallel: 0,
            timeoutSecs: 600,
            defaultModel: "sonnet",
            ...opts,
        }
    }

    async onContextItem(_source: Participant, item: ContextItem): Promise<void> {
        if (item instanceof ReplanItem) {
            this.pendingReplans.push(item)
        }
    }

    async run(environment: AgenticEnvironment): Promise<ConductorRunSummary> {
        this.envRef = environment

        let prd = loadPrd(this.opts.prdPath)
        this.emit(new ConductorStateItem("loading", `${prd.userStories.length} stories`))

        if (this.opts.onRunStart) {
            try {
                await this.opts.onRunStart(prd)
            } catch (e) {
                this.emit(
                    new ConductorStateItem(
                        "failed",
                        `onRunStart hook failed: ${(e as Error)?.message ?? String(e)}`,
                    ),
                )
                throw e
            }
        }

        // Adaptive DAG: each iteration of the while-loop computes the
        // remaining-incomplete DAG fresh from the PRD, runs the next
        // level, then applies any ReplanItem-s buffered during that
        // level. ReplanItem applications happen at level boundaries so
        // running StoryAgents are never orphaned.
        const completed: string[] = []
        const failed: string[] = []
        const startedAt = Date.now()
        let totalAttempts = 0
        let levelOrdinal = 0
        let abortedReason: string | null = null
        let appliedReplans = 0

        while (true) {
            const levels = buildDag(prd.userStories, { onlyIncomplete: true })
            if (levels.length === 0) break

            const level = levels[0]
            levelOrdinal += 1
            const totalLevelsHint = levelOrdinal + levels.length - 1
            this.emit(
                new ConductorStateItem(
                    "running_level",
                    undefined,
                    levelOrdinal,
                    totalLevelsHint,
                    level.storyIds,
                ),
            )

            const outcomes = await this.runLevel(prd, level.storyIds)
            for (const outcome of outcomes) {
                totalAttempts += outcome.attempts
                if (outcome.success) {
                    completed.push(outcome.storyId)
                    prd = markStoryPassed(prd, outcome.storyId, outcome.durationSecs)
                    savePrd(this.opts.prdPath, prd)
                    if (this.opts.onStoryPassed) {
                        try {
                            await this.opts.onStoryPassed(outcome.storyId)
                        } catch (e) {
                            this.emit(
                                new ConductorStateItem(
                                    "running_level",
                                    `onStoryPassed hook for ${outcome.storyId} failed: ${(e as Error)?.message ?? String(e)}`,
                                    levelOrdinal,
                                    totalLevelsHint,
                                ),
                            )
                        }
                    }
                } else {
                    failed.push(outcome.storyId)
                }
            }

            this.emit(
                new ConductorStateItem(
                    "level_complete",
                    `passed ${outcomes.filter((o) => o.success).length}/${outcomes.length}`,
                    levelOrdinal,
                    totalLevelsHint,
                    level.storyIds,
                ),
            )

            // Apply ReplanItem-s buffered during this level (Phase 4).
            // PRD is mutated, persisted, and the next loop iteration
            // recomputes the DAG from the updated PRD.
            if (this.pendingReplans.length > 0) {
                const drained = this.pendingReplans.splice(0)
                for (const replan of drained) {
                    prd = this.applyReplan(prd, replan)
                    appliedReplans += 1
                    this.emit(
                        new ConductorStateItem(
                            "running_level",
                            `replan applied (source=${replan.source}, +${replan.addedStories.length}/-${replan.removedStoryIds.length}): ${replan.reason}`,
                            levelOrdinal,
                        ),
                    )
                }
                savePrd(this.opts.prdPath, prd)
            }

            // If every story in the level failed terminally AND no replan
            // mutated the plan in response, abort the remaining levels.
            const anySuccess = outcomes.some((o) => o.success)
            const replannedThisLevel = appliedReplans > 0
            if (!anySuccess && outcomes.length > 0 && !replannedThisLevel) {
                abortedReason =
                    "all stories in level failed; aborting remaining levels"
                this.emit(
                    new ConductorStateItem(
                        "failed",
                        abortedReason,
                        levelOrdinal,
                        totalLevelsHint,
                    ),
                )
                break
            }
        }

        const totalDurationSecs = Math.round((Date.now() - startedAt) / 1000)
        const phase = failed.length === 0 ? "done" : "failed"
        this.emit(
            new ConductorStateItem(
                phase,
                `${completed.length} passed, ${failed.length} failed in ${totalDurationSecs}s`,
            ),
        )

        const summary: ConductorRunSummary = {
            completedStories: completed,
            failedStories: failed,
            totalDurationSecs,
            totalAttempts,
        }

        if (this.opts.onRunComplete) {
            try {
                await this.opts.onRunComplete(summary)
            } catch (e) {
                this.emit(
                    new ConductorStateItem(
                        "failed",
                        `onRunComplete hook failed: ${(e as Error)?.message ?? String(e)}`,
                    ),
                )
            }
        }

        return summary
    }

    /**
     * Apply a ReplanItem to the in-memory PrdFile (returns a new copy):
     *   - removes stories whose ids are in `removedStoryIds`, *unless*
     *     they have already passed (we don't roll back commit work);
     *   - adds new stories from `addedStories`, skipping ids that already
     *     exist (replans are idempotent on accidental duplicates);
     *   - rewrites `dependsOn` for stories in `modifiedDeps`.
     *
     * This method is pure: persistence is the caller's responsibility.
     */
    private applyReplan(prd: PrdFile, replan: ReplanItem): PrdFile {
        let stories = prd.userStories.slice()

        if (replan.removedStoryIds.length > 0) {
            const removeSet = new Set(replan.removedStoryIds)
            stories = stories.filter((s) => !removeSet.has(s.id) || s.passes)
        }

        if (replan.modifiedDeps.size > 0) {
            stories = stories.map((s) => {
                const newDeps = replan.modifiedDeps.get(s.id)
                if (!newDeps) return s
                return { ...s, dependsOn: [...newDeps] }
            })
        }

        if (replan.addedStories.length > 0) {
            const existing = new Set(stories.map((s) => s.id))
            for (const a of replan.addedStories) {
                if (existing.has(a.id)) continue
                stories.push({
                    id: a.id,
                    priority: a.priority,
                    title: a.title,
                    description: a.description,
                    dependsOn: [...a.dependsOn],
                    retries: a.retries ?? 2,
                    acceptance: a.acceptance ? [...a.acceptance] : [],
                    tests: a.tests ? [...a.tests] : [],
                    passes: false,
                    completedAt: null,
                    durationSecs: null,
                    model: a.model,
                })
            }
        }

        return { ...prd, userStories: stories }
    }

    private async runLevel(
        prd: PrdFile,
        storyIds: readonly string[],
    ): Promise<StoryOutcome[]> {
        const stories = storyIds
            .map((id) => prd.userStories.find((s) => s.id === id))
            .filter((s): s is PrdStory => s !== undefined)
            .filter((s) => !s.passes)

        if (stories.length === 0) return []

        const cap = this.opts.parallel > 0 ? this.opts.parallel : stories.length
        const outcomes: StoryOutcome[] = []
        let cursor = 0

        const runNext = async (): Promise<void> => {
            while (cursor < stories.length) {
                const idx = cursor++
                const story = stories[idx]
                const outcome = await this.runStory(story)
                outcomes.push(outcome)
            }
        }

        const workers = Array.from(
            { length: Math.min(cap, stories.length) },
            () => runNext(),
        )
        await Promise.all(workers)
        // Preserve story-id order in outcomes for deterministic output.
        outcomes.sort(
            (a, b) =>
                stories.findIndex((s) => s.id === a.storyId) -
                stories.findIndex((s) => s.id === b.storyId),
        )
        return outcomes
    }

    private async runStory(story: PrdStory): Promise<StoryOutcome> {
        if (!this.envRef) {
            return {
                storyId: story.id,
                success: false,
                attempts: 0,
                durationSecs: 0,
                finalSummary: null,
                error: "no environment",
            }
        }
        const model =
            this.opts.overrideModel ?? story.model ?? this.opts.defaultModel
        let prompt = this.resolvePrompt(story)

        if (this.opts.onBeforeStoryLaunch) {
            try {
                const extra = await this.opts.onBeforeStoryLaunch(story.id, story)
                if (typeof extra === "string" && extra.trim().length > 0) {
                    prompt = `${extra.trim()}\n\n${prompt}`
                }
            } catch (e) {
                this.emit(
                    new ConductorStateItem(
                        "running_level",
                        `onBeforeStoryLaunch hook for ${story.id} failed: ${(e as Error)?.message ?? String(e)}`,
                    ),
                )
            }
        }

        const agent = new StoryAgent({
            id: story.id,
            prompt,
            cwd: this.opts.cwd,
            model,
            retries: story.retries,
            timeoutSecs: this.opts.timeoutSecs,
        })
        agent.join(this.envRef)
        try {
            const outcome = await agent.run(this.envRef)
            return outcome
        } finally {
            if (this.envRef) agent.leave(this.envRef)
        }
    }

    private resolvePrompt(story: PrdStory): string {
        const candidatePath =
            this.opts.promptTemplatePath ?? join(this.opts.cwd, "prompt.md")
        if (existsSync(candidatePath)) {
            const tpl = readFileSyncSafe(candidatePath)
            if (tpl) {
                return applyTemplate(tpl, story)
            }
        }
        return buildDefaultStoryPrompt(story)
    }

    private emit(item: ContextItem): void {
        this.envRef?.deliverContextItem(this, item)
    }
}

function readFileSyncSafe(path: string): string | null {
    try {
        return readFileSync(path, "utf8")
    } catch {
        return null
    }
}

function applyTemplate(tpl: string, story: PrdStory): string {
    const acceptance = story.acceptance.length
        ? story.acceptance.map((a, i) => `${i + 1}. ${a}`).join("\n")
        : "(none specified)"
    return tpl
        .replace(/STORY_ID/g, story.id)
        .replace(/STORY_TITLE/g, story.title)
        .replace(/STORY_DESCRIPTION/g, story.description)
        .replace(/ACCEPTANCE_CRITERIA/g, acceptance)
}
