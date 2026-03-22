#!/usr/bin/env node
/**
 * Execution orchestrator - reads prd.json, runs stories, outputs JSON events to stdout.
 * This is piped into baro-tui for the dashboard.
 */

import * as path from "path"
import * as fs from "fs"
import { CliTask } from "./cli-task.js"
import { buildDag } from "./dag.js"

interface PrdV2 {
    project: string
    userStories: {
        id: string; title: string; description: string; dependsOn: string[];
        retries: number; acceptance: string[]; tests: string[];
        passes: boolean; completedAt: string | null; durationSecs: number | null; priority: number;
    }[]
}

function emit(event: any) {
    process.stdout.write(JSON.stringify(event) + "\n")
}

async function main() {
    const cwd = process.cwd()
    const prdPath = path.join(cwd, "prd.json")
    const prd: PrdV2 = JSON.parse(fs.readFileSync(prdPath, "utf-8"))
    const incomplete = prd.userStories.filter((s) => !s.passes)

    if (incomplete.length === 0) {
        emit({ type: "done", total_time_secs: 0, stats: { stories_completed: prd.userStories.length, stories_skipped: 0, total_commits: 0, files_created: 0, files_modified: 0 } })
        return
    }

    emit({ type: "init", project: prd.project, stories: prd.userStories.map((s) => ({ id: s.id, title: s.title, depends_on: s.dependsOn })) })

    const levels = buildDag(prd.userStories)
    emit({ type: "dag", levels: levels.map((l) => l.stories.map((s) => ({ id: s.id, title: s.title }))) })

    const startTime = Date.now()
    let completed = 0
    let skipped = 0
    const total = incomplete.length

    for (const level of levels) {
        await Promise.allSettled(level.stories.map(async (story) => {
            const maxAttempts = story.retries + 1
            for (let attempt = 1; attempt <= maxAttempts; attempt++) {
                emit({ type: "story_start", id: story.id, title: story.title })

                try {
                    const prompt = buildPrompt(story, cwd)
                    const task = new CliTask({
                        id: story.id,
                        command: "claude",
                        args: ["--dangerously-skip-permissions", "--output-format", "json", "-p", prompt],
                        cwd,
                        onStdout: (line) => emit({ type: "story_log", id: story.id, line }),
                        onStderr: (line) => emit({ type: "story_log", id: story.id, line }),
                    })

                    const result = await task.execute()
                    const dur = Math.round(result.durationMs / 1000)
                    completed++

                    // Mark complete in prd.json
                    const raw = JSON.parse(fs.readFileSync(prdPath, "utf-8"))
                    for (const s of raw.userStories) {
                        if (s.id === story.id) { s.passes = true; s.completedAt = new Date().toISOString(); s.durationSecs = dur }
                    }
                    fs.writeFileSync(prdPath, JSON.stringify(raw, null, 2) + "\n")

                    emit({ type: "story_complete", id: story.id, duration_secs: dur, files_created: 0, files_modified: 0 })
                    emit({ type: "progress", completed, total, percentage: Math.round((completed / total) * 100) })
                    return
                } catch (err: any) {
                    emit({ type: "story_error", id: story.id, error: err.message, attempt, max_retries: maxAttempts })
                    if (attempt < maxAttempts) {
                        emit({ type: "story_retry", id: story.id, attempt: attempt + 1 })
                    } else {
                        skipped++
                        emit({ type: "progress", completed, total, percentage: Math.round((completed / total) * 100) })
                    }
                }
            }
        }))
    }

    emit({ type: "done", total_time_secs: Math.round((Date.now() - startTime) / 1000), stats: { stories_completed: completed, stories_skipped: skipped, total_commits: completed, files_created: 0, files_modified: 0 } })
}

function buildPrompt(story: any, cwd: string): string {
    const templatePath = path.join(cwd, "prompt.md")
    let template: string
    if (fs.existsSync(templatePath)) {
        template = fs.readFileSync(templatePath, "utf-8")
    } else {
        template = [
            "You are working on story STORY_ID: STORY_TITLE", "", "STORY_DESCRIPTION", "",
            "ACCEPTANCE CRITERIA:", "ACCEPTANCE_CRITERIA", "",
            "Run tests: TEST_COMMANDS",
            'If tests pass, commit: git add . && git commit -m "feat(STORY_ID): STORY_TITLE"',
        ].join("\n")
    }
    return template
        .replace(/STORY_ID/g, story.id).replace(/STORY_TITLE/g, story.title)
        .replace(/STORY_DESCRIPTION/g, story.description)
        .replace(/ACCEPTANCE_CRITERIA/g, story.acceptance.map((a: string) => "- " + a).join("\n"))
        .replace(/TEST_COMMANDS/g, story.tests.join(" && "))
}

main().catch((err) => { process.stderr.write("Fatal: " + err + "\n"); process.exit(1) })
