/**
 * Claude Code CLI planner - uses claude CLI to generate plans.
 * No API key needed - uses the same claude CLI as execution.
 * This is the default planner.
 */

import { CliTask } from "./cli-task.js"
import type { PrdV2 } from "./prd.js"
import * as fs from "fs"
import * as path from "path"

const SYSTEM_PROMPT = `You are an expert software architect. Break down the user's project goal into concrete user stories that form a dependency DAG.

You MUST explore the existing codebase first using your tools (read files, list directories, etc.) before generating the plan.

Output ONLY valid JSON matching this exact schema (no markdown, no explanation, just JSON):
{
  "project": "short project name",
  "branchName": "kebab-case-branch-name",
  "description": "one-line description",
  "userStories": [
    {
      "id": "S1",
      "priority": 1,
      "title": "short title",
      "description": "what to implement",
      "dependsOn": [],
      "retries": 2,
      "acceptance": ["testable criterion"],
      "tests": ["npm test"]
    }
  ]
}

Rules:
- Each story: single focused unit of work for one AI agent
- Use dependsOn for dependencies; same-priority stories with no deps run IN PARALLEL
- Keep stories small (15-60 min of work each)
- Include testable acceptance criteria and test commands
- No circular dependencies
- Start with foundational stories, build up
- IDs: S1, S2, S3...
- Build on existing code, don't recreate what exists
- Output ONLY the JSON, nothing else`

export interface ClaudePlannerOptions {
    cwd?: string
    onLog?: (line: string) => void
}

export class ClaudePlanner {
    private cwd: string
    private onLog?: (line: string) => void

    constructor(options: ClaudePlannerOptions = {}) {
        this.cwd = options.cwd ?? process.cwd()
        this.onLog = options.onLog
    }

    async send(userMessage: string): Promise<PrdV2> {
        const prompt = `${SYSTEM_PROMPT}\n\nUser goal: ${userMessage}`

        const task = new CliTask({
            id: "planner",
            command: "claude",
            args: [
                "--dangerously-skip-permissions",
                "--output-format", "json",
                "-p", prompt,
            ],
            cwd: this.cwd,
            onStdout: (line) => this.onLog?.(line),
            onStderr: (line) => this.onLog?.(line),
        })

        const result = await task.execute()

        // Claude --output-format json wraps result in {"result": "..."}
        let jsonText = result.stdout.trim()
        try {
            const wrapper = JSON.parse(jsonText)
            if (wrapper.result) {
                jsonText = wrapper.result
            }
        } catch {}

        // Extract JSON from potential markdown code blocks
        const jsonMatch = jsonText.match(/```(?:json)?\s*([\s\S]*?)```/)
        if (jsonMatch) {
            jsonText = jsonMatch[1].trim()
        }

        let prd: any
        try {
            prd = JSON.parse(jsonText)
        } catch {
            throw new Error("Claude didn't return valid JSON. Try again with a clearer goal.")
        }

        if (!prd.project || !prd.userStories) {
            throw new Error("Invalid plan format. Missing project or userStories.")
        }

        return {
            ...prd,
            userStories: prd.userStories.map((s: any) => ({
                id: s.id ?? "S?",
                priority: s.priority ?? 0,
                title: s.title ?? "",
                description: s.description ?? "",
                dependsOn: s.dependsOn ?? [],
                retries: s.retries ?? 2,
                acceptance: s.acceptance ?? [],
                tests: s.tests ?? [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            })),
        }
    }
}
