/**
 * StoryAgent — story-level wrapper that drives a ClaudeCliParticipant
 * through a single piece of work, with retries and timeout.
 *
 * Lifecycle:
 *   idle ─► starting ─► running ─► done | failed
 *                               ╰► retrying ─► running ─► …
 *
 * Each attempt spawns a fresh ClaudeCliParticipant. On `result:success`
 * (no error, exit 0) the StoryAgent emits a StoryResultItem and resolves
 * its `done` promise. On Claude failure or timeout, retry up to
 * `retries` times before giving up.
 *
 * Library-grade: doesn't import PRD types. Caller passes a StorySpec
 * with prompt + acceptance criteria + retry budget.
 */

import { setTimeout as setTimeoutPromise } from "timers/promises"

import {
    AgenticEnvironment,
    ContextItem,
    Participant,
} from "@mozaik-ai/core"

import {
    AgentPhase,
    AgentStateItem,
    AgentTargetedMessageItem,
    ClaudeResultItem,
} from "../types.js"
import {
    ClaudeCliParticipant,
    ClaudeRunSummary,
} from "./claude-cli-participant.js"

export interface StorySpec {
    /** Story ID, used as agentId for observer attribution. */
    id: string
    /** The prompt sent to Claude as the initial user message. */
    prompt: string
    /** Working directory for Claude. */
    cwd: string
    /** Optional model override (e.g. "sonnet", "opus", "haiku"). */
    model?: string
    /** Retry budget (number of *additional* attempts after the first). */
    retries?: number
    /** Per-attempt timeout in seconds. Default: 600. */
    timeoutSecs?: number
    /** Delay between retries in milliseconds. Default: 1500. */
    retryDelayMs?: number
}

export interface StoryOutcome {
    storyId: string
    success: boolean
    attempts: number
    durationSecs: number
    finalSummary: ClaudeRunSummary | null
    error: string | null
}

export class StoryResultItem extends ContextItem {
    readonly type = "story_result"

    constructor(
        public readonly storyId: string,
        public readonly success: boolean,
        public readonly attempts: number,
        public readonly durationSecs: number,
        public readonly error: string | null,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            storyId: this.storyId,
            success: this.success,
            attempts: this.attempts,
            durationSecs: this.durationSecs,
            error: this.error,
        }
    }
}

export class StoryAgent extends Participant {
    private readonly spec: Required<
        Pick<StorySpec, "retries" | "timeoutSecs" | "retryDelayMs">
    > &
        StorySpec

    private envRef: AgenticEnvironment | null = null
    private currentClaude: ClaudeCliParticipant | null = null
    private currentPhase: AgentPhase = "idle"
    private startedAt: number | null = null
    private resolveDone!: (outcome: StoryOutcome) => void
    public readonly done: Promise<StoryOutcome>

    constructor(spec: StorySpec) {
        super()
        this.spec = {
            retries: 2,
            timeoutSecs: 600,
            retryDelayMs: 1500,
            ...spec,
        }
        this.done = new Promise<StoryOutcome>((res) => {
            this.resolveDone = res
        })
    }

    get id(): string {
        return this.spec.id
    }

    /** Mark "agentId" so observer helpers can attribute events. */
    get agentId(): string {
        return this.spec.id
    }

    getPhase(): AgentPhase {
        return this.currentPhase
    }

    getCurrentClaude(): ClaudeCliParticipant | null {
        return this.currentClaude
    }

    /**
     * Begin executing the story. Idempotent. Returns the `done` promise
     * for the caller's convenience.
     */
    run(environment: AgenticEnvironment): Promise<StoryOutcome> {
        if (this.startedAt != null) {
            return this.done
        }
        this.envRef = environment
        this.startedAt = Date.now()
        this.transition("starting", "story queued")
        void this.executeAllAttempts()
        return this.done
    }

    /**
     * Forward bus messages targeted at this story to its current Claude
     * process. Critic/Librarian/etc inject feedback this way.
     */
    async onContextItem(source: Participant, item: ContextItem): Promise<void> {
        if (source === this) return
        if (item instanceof AgentTargetedMessageItem) {
            if (item.recipientId !== this.spec.id) return
            const claude = this.currentClaude
            if (claude && claude.getPhase() === "running") {
                claude.sendUserMessage(item.text)
            }
        }
    }

    /** Abort the story, killing the running Claude (if any). */
    abort(): void {
        this.currentClaude?.abort()
        this.transition("aborted", "external abort")
    }

    private async executeAllAttempts(): Promise<void> {
        const maxAttempts = this.spec.retries + 1
        let lastSummary: ClaudeRunSummary | null = null
        let lastError: string | null = null

        for (let attempt = 1; attempt <= maxAttempts; attempt++) {
            if (attempt > 1) {
                this.transition(
                    "waiting",
                    `retrying (attempt ${attempt}/${maxAttempts})`,
                )
                await setTimeoutPromise(this.spec.retryDelayMs)
            }

            const result = await this.runOneAttempt(attempt)
            lastSummary = result.summary
            lastError = result.error

            if (result.success) {
                const durationSecs = Math.round(
                    (Date.now() - (this.startedAt ?? Date.now())) / 1000,
                )
                this.transition("done", `success on attempt ${attempt}`)
                this.emitStoryResult(true, attempt, durationSecs, null)
                this.resolveDone({
                    storyId: this.spec.id,
                    success: true,
                    attempts: attempt,
                    durationSecs,
                    finalSummary: result.summary,
                    error: null,
                })
                return
            }
        }

        const durationSecs = Math.round(
            (Date.now() - (this.startedAt ?? Date.now())) / 1000,
        )
        this.transition("failed", `exhausted ${maxAttempts} attempts`)
        this.emitStoryResult(false, maxAttempts, durationSecs, lastError)
        this.resolveDone({
            storyId: this.spec.id,
            success: false,
            attempts: maxAttempts,
            durationSecs,
            finalSummary: lastSummary,
            error: lastError,
        })
    }

    private async runOneAttempt(
        attempt: number,
    ): Promise<{ success: boolean; summary: ClaudeRunSummary | null; error: string | null }> {
        if (!this.envRef) {
            return { success: false, summary: null, error: "no environment" }
        }

        this.transition("running", `attempt ${attempt}`)

        const claude = new ClaudeCliParticipant(this.spec.id, {
            cwd: this.spec.cwd,
            model: this.spec.model,
        })
        this.currentClaude = claude
        claude.join(this.envRef)
        claude.start(this.envRef)

        // Claude --print --input-format stream-json doesn't begin
        // emitting events until it has consumed at least one input event
        // OR stdin is closed. Waiting on `claude.ready` first would
        // deadlock — send the prompt up front, then await events.
        try {
            claude.sendUserMessage(this.spec.prompt)
            claude.closeStdin()
        } catch (e) {
            const error = e instanceof Error ? e.message : String(e)
            claude.abort()
            claude.leave(this.envRef)
            this.currentClaude = null
            return { success: false, summary: null, error }
        }

        let summary: ClaudeRunSummary
        try {
            summary = await raceWithTimeout(
                claude.done,
                this.spec.timeoutSecs * 1000,
                `attempt ${attempt} timeout after ${this.spec.timeoutSecs}s`,
            )
        } catch (e) {
            claude.abort()
            const error = e instanceof Error ? e.message : String(e)
            // Wait for the kill to land so subsequent attempts get a clean slate.
            try {
                await claude.done
            } catch {
                // ignore
            }
            claude.leave(this.envRef)
            this.currentClaude = null
            return { success: false, summary: null, error }
        }

        claude.leave(this.envRef)
        this.currentClaude = null

        const success =
            summary.exitCode === 0 &&
            summary.error == null &&
            summary.lastResult != null &&
            !summary.lastResult.isError

        if (!success) {
            const reason = summary.error
                ? summary.error.message
                : summary.lastResult?.isError
                  ? `claude reported isError on result:${summary.lastResult.subtype}`
                  : `non-zero exit ${summary.exitCode}`
            return { success: false, summary, error: reason }
        }

        return { success: true, summary, error: null }
    }

    private emitStoryResult(
        success: boolean,
        attempts: number,
        durationSecs: number,
        error: string | null,
    ): void {
        if (!this.envRef) return
        this.envRef.deliverContextItem(
            this,
            new StoryResultItem(this.spec.id, success, attempts, durationSecs, error),
        )
    }

    private transition(next: AgentPhase, detail?: string): void {
        if (next === this.currentPhase) return
        this.currentPhase = next
        if (this.envRef) {
            this.envRef.deliverContextItem(
                this,
                new AgentStateItem(this.spec.id, next, detail),
            )
        }
    }
}

function raceWithTimeout<T>(
    p: Promise<T>,
    ms: number,
    label: string,
): Promise<T> {
    return Promise.race([
        p,
        new Promise<T>((_, rej) => setTimeout(() => rej(new Error(label)), ms)),
    ])
}
