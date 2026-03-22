/**
 * AI Planner with streaming output.
 * Direct OpenAI streaming with fixed JSON schema for structured output.
 */

import { z } from "zod"
import { zodToJsonSchema } from "zod-to-json-schema"
import type { PrdV2 } from "./prd.js"
import { streamCompletion } from "./stream.js"
import * as fs from "fs"
import * as path from "path"

const StorySchema = z.object({
    id: z.string().describe("Short ID like S1, S2, S3"),
    priority: z.number().describe("Priority level: lower = earlier"),
    title: z.string().describe("Short title for the story"),
    description: z.string().describe("What needs to be implemented"),
    dependsOn: z.array(z.string()).describe("IDs of stories this depends on"),
    retries: z.number().describe("Retry attempts if story fails (usually 2)"),
    acceptance: z.array(z.string()).describe("Testable acceptance criteria"),
    tests: z.array(z.string()).describe("Test commands (e.g. ['npm test'])"),
})

const PrdSchema = z.object({
    project: z.string().describe("Short project name"),
    branchName: z.string().describe("Git branch name (kebab-case)"),
    description: z.string().describe("One-line project description"),
    userStories: z.array(StorySchema).describe("User stories forming a DAG"),
})

const SYSTEM_PROMPT = `You are an expert software architect. Break down project goals into concrete user stories that form a dependency DAG.

Rules:
- Each story: single focused unit of work for one AI agent
- Use dependsOn for dependencies; same-priority stories with no deps run IN PARALLEL
- Keep stories small (15-60 min of work)
- Include testable acceptance criteria and test commands
- No circular dependencies
- Start with foundational stories, build up
- retries: 2-3 for most stories
- IDs: S1, S2, S3...
- branchName: kebab-case
- Build on existing code, don't recreate what exists`

/** Recursively add additionalProperties: false to all objects in schema (OpenAI strict mode requires it) */
function fixSchemaForOpenAI(schema: any): any {
    if (!schema || typeof schema !== "object") return schema
    if (schema.type === "object" && schema.properties) {
        schema.additionalProperties = false
    }
    if (schema.properties) {
        for (const key of Object.keys(schema.properties)) {
            fixSchemaForOpenAI(schema.properties[key])
        }
    }
    if (schema.items) fixSchemaForOpenAI(schema.items)
    // Remove $schema if present
    delete schema.$schema
    return schema
}

export interface PlannerOptions {
    model?: string
    cwd?: string
    onToken?: (token: string) => void
}

export class Planner {
    private messages: { role: string; content: string }[]
    private model: string
    private onToken?: (token: string) => void

    constructor(options: PlannerOptions = {}) {
        this.model = options.model ?? "gpt-5.4"
        this.onToken = options.onToken
        this.messages = [{ role: "system", content: SYSTEM_PROMPT }]

        if (options.cwd) {
            const context = gatherContext(options.cwd)
            if (context) {
                this.messages.push({
                    role: "system",
                    content: `Existing codebase:\n${context}`,
                })
            }
        }
    }

    async send(userMessage: string): Promise<PrdV2> {
        const raw = zodToJsonSchema(PrdSchema, "prd")
        const schema = fixSchemaForOpenAI(
            (raw as any).definitions?.prd ?? raw
        )

        const fullText = await streamCompletion({
            model: this.model,
            messages: this.messages,
            task: userMessage,
            jsonSchema: schema,
            reasoning: { effort: "high" },
            onToken: this.onToken ?? (() => {}),
        })

        let prd: z.infer<typeof PrdSchema>
        try {
            prd = JSON.parse(fullText)
        } catch {
            throw new Error("Failed to parse plan output as JSON")
        }

        this.messages.push(
            { role: "user", content: userMessage },
            { role: "assistant", content: fullText },
        )

        return {
            ...prd,
            userStories: prd.userStories.map((s) => ({
                ...s,
                passes: false,
                completedAt: null,
                durationSecs: null,
            })),
        }
    }
}

function gatherContext(cwd: string): string | null {
    const parts: string[] = []
    const pkgPath = path.join(cwd, "package.json")
    if (fs.existsSync(pkgPath)) {
        try {
            const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"))
            parts.push(`Project: ${pkg.name ?? "unknown"}`)
            if (pkg.dependencies) parts.push(`Deps: ${Object.keys(pkg.dependencies).join(", ")}`)
        } catch {}
    }

    const files: string[] = []
    const ignore = new Set(["node_modules", ".git", "dist", "build", ".next", "coverage", "target"])
    function walk(dir: string, prefix: string, depth: number) {
        if (depth > 3 || files.length > 50) return
        try {
            for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
                if (ignore.has(entry.name) || entry.name.startsWith(".")) continue
                const rel = prefix ? `${prefix}/${entry.name}` : entry.name
                if (entry.isDirectory()) { files.push(rel + "/"); walk(path.join(dir, entry.name), rel, depth + 1) }
                else files.push(rel)
            }
        } catch {}
    }
    walk(cwd, "", 0)
    if (files.length > 0) parts.push(`\nFiles:\n${files.join("\n")}`)
    return parts.length > 0 ? parts.join("\n") : null
}
