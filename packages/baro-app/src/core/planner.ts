/**
 * AI Planner with streaming output.
 * Direct OpenAI streaming with fixed JSON schema for structured output.
 */

import { z } from "zod"
import { zodToJsonSchema } from "zod-to-json-schema"
import type { PrdV2 } from "./prd.js"
import { streamCompletion } from "./stream.js"
import { createCodebaseTools } from "./tools.js"
import type { ToolDef } from "./stream.js"
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

IMPORTANT: Before generating a plan, USE YOUR TOOLS to explore the existing codebase:
1. Call file_tree to see the project structure
2. Call read_file on key files (package.json, README, entry points, configs)
3. Call grep to find relevant patterns
4. THEN generate a plan that fits the existing code

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
    onToolCall?: (name: string, args: any) => void
}

export class Planner {
    private messages: { role: string; content: string }[]
    private model: string
    private onToken?: (token: string) => void
    private onToolCall?: (name: string, args: any) => void
    private tools: ToolDef[]

    constructor(options: PlannerOptions = {}) {
        this.model = options.model ?? "gpt-5.4"
        this.onToken = options.onToken
        this.onToolCall = options.onToolCall
        this.tools = options.cwd ? createCodebaseTools(options.cwd) : []
        this.messages = [{ role: "system", content: SYSTEM_PROMPT }]
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
            tools: this.tools.length > 0 ? this.tools : undefined,
            onToken: this.onToken ?? (() => {}),
            onToolCall: this.onToolCall,
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

