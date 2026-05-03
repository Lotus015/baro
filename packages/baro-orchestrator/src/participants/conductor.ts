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

    constructor(opts: ConductorOptions) {
        super()
        this.opts = {
            parallel: 0,
            timeoutSecs: 600,
            defaultModel: "sonnet",
            ...opts,
        }
    }

    async onContextItem(): Promise<void> {
        // Conductor pulls outcomes via direct await on StoryAgent.done;
        // no bus reactions needed in this phase.
    }

    async run(environment: AgenticEnvironment): Promise<ConductorRunSummary> {
        this.envRef = environment

        let prd = loadPrd(this.opts.prdPath)
        this.emit(new ConductorStateItem("loading", `${prd.userStories.length} stories`))

        const levels = buildDag(prd.userStories, { onlyIncomplete: true })
        if (levels.length === 0) {
            this.emit(new ConductorStateItem("done", "nothing to do"))
            return {
                completedStories: prd.userStories.filter((s) => s.passes).map((s) => s.id),
                failedStories: [],
                totalDurationSecs: 0,
                totalAttempts: 0,
            }
        }

        const completed: string[] = []
        const failed: string[] = []
        const startedAt = Date.now()
        let totalAttempts = 0

        for (let levelIdx = 0; levelIdx < levels.length; levelIdx++) {
            const level = levels[levelIdx]
            this.emit(
                new ConductorStateItem(
                    "running_level",
                    undefined,
                    levelIdx + 1,
                    levels.length,
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
                } else {
                    failed.push(outcome.storyId)
                }
            }

            this.emit(
                new ConductorStateItem(
                    "level_complete",
                    `passed ${outcomes.filter((o) => o.success).length}/${outcomes.length}`,
                    levelIdx + 1,
                    levels.length,
                    level.storyIds,
                ),
            )

            // If every story in the level failed terminally, abort the
            // remaining levels — there's likely nothing usable to build
            // on top of, and dependent levels can't run anyway.
            const anySuccess = outcomes.some((o) => o.success)
            if (!anySuccess && outcomes.length > 0) {
                this.emit(
                    new ConductorStateItem(
                        "failed",
                        "all stories in level failed; aborting remaining levels",
                        levelIdx + 1,
                        levels.length,
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

        return {
            completedStories: completed,
            failedStories: failed,
            totalDurationSecs,
            totalAttempts,
        }
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
        const prompt = this.resolvePrompt(story)

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
