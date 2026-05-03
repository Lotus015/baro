#!/usr/bin/env tsx
/**
 * Phase 1 milestone-B demo (orchestration only, no Rust/no TUI):
 * Conductor + StoryAgents + Auditor + Cartographer running a tiny
 * PRD with two parallel stories.
 *
 * Success: both stories run concurrently inside a single
 * AgenticEnvironment, frames from both interleave on the Cartographer
 * sink, prd.json gets updated to passes:true for both, audit log
 * captures the full event tape.
 */

import {
    AgenticEnvironment,
    Participant,
} from "@mozaik-ai/core"
import {
    mkdirSync,
    mkdtempSync,
    writeFileSync,
} from "fs"
import { tmpdir } from "os"
import { join } from "path"

import {
    Auditor,
    Cartographer,
    Conductor,
    type Frame,
    type PrdFile,
} from "../src/main.js"

class Originator extends Participant {
    async onContextItem(): Promise<void> {
        return
    }
}

void Originator

function setupTestRepo(): { cwd: string; prdPath: string } {
    const cwd = mkdtempSync(join(tmpdir(), "baro-multistory-demo-"))
    writeFileSync(
        join(cwd, "README.md"),
        "# Demo repo\n\nA tiny repo for the multi-story orchestration demo.\n",
    )
    writeFileSync(
        join(cwd, "package.json"),
        JSON.stringify({ name: "demo-multi", version: "0.1.0" }, null, 2),
    )
    writeFileSync(
        join(cwd, "index.ts"),
        "export const greet = (n: string) => `Hello, ${n}!`\n",
    )

    const prd: PrdFile = {
        project: "demo-multi",
        branchName: "demo-multi",
        description: "Two-story orchestration demo",
        userStories: [
            {
                id: "S1",
                priority: 1,
                title: "Inspect README",
                description:
                    "Use the Read tool to read README.md and tell me its first heading. Do not modify any files. Keep your final answer to one sentence.",
                dependsOn: [],
                retries: 0,
                acceptance: [
                    "you have read README.md",
                    "you have stated the first heading",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
            {
                id: "S2",
                priority: 1,
                title: "Inspect package.json",
                description:
                    "Use the Read tool to read package.json and tell me the value of the `name` field. Do not modify any files. Keep your final answer to one sentence.",
                dependsOn: [],
                retries: 0,
                acceptance: [
                    "you have read package.json",
                    "you have stated the value of the name field",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
        ],
    }
    const prdPath = join(cwd, "prd.json")
    writeFileSync(prdPath, JSON.stringify(prd, null, 2) + "\n")
    return { cwd, prdPath }
}

function frameSummary(frame: Frame): string {
    switch (frame.kind) {
        case "agent_state":
            return `state[${frame.agentId}] → ${frame.phase}${frame.detail ? ` (${frame.detail})` : ""}`
        case "user_message":
            return `user[${frame.agentId ?? "?"}] ${truncate(frame.text)}`
        case "model_message":
            return `assistant[${frame.agentId ?? "?"}] ${truncate(frame.text)}`
        case "tool_call":
            return `tool_call[${frame.agentId ?? "?"}] ${frame.name}(${truncate(frame.args, 60)})`
        case "tool_result":
            return `tool_result[${frame.agentId ?? "?"}] ${frame.callId.slice(0, 12)} ${truncate(frame.output.replace(/\n/g, "⏎"), 60)}`
        case "result":
            return `result[${frame.agentId}] ok=${!frame.isError} turns=${frame.numTurns} dur=${frame.durationMs}ms cost=$${frame.costUsd?.toFixed(4) ?? "?"}`
        case "rate_limit":
            return `rate_limit[${frame.agentId}]`
        case "system":
            return `system[${frame.agentId}] ${frame.subtype}`
        case "stream_chunk":
            return `stream_chunk[${frame.agentId}]`
        case "unknown":
            return `unknown ${frame.sourceLabel} ${frame.itemType}`
    }
}

function truncate(s: string, max = 80): string {
    return s.length > max ? `${s.slice(0, max)}…` : s
}

async function main(): Promise<void> {
    const { cwd, prdPath } = setupTestRepo()
    process.stderr.write(`[demo] test repo: ${cwd}\n`)
    process.stderr.write(`[demo] prd: ${prdPath}\n`)

    const logsDir = join(import.meta.dirname, "demo-logs")
    mkdirSync(logsDir, { recursive: true })
    const logPath = join(logsDir, `demo-multi-${Date.now()}.jsonl`)

    const env = new AgenticEnvironment()
    const auditor = new Auditor({ path: logPath })
    const cartographer = new Cartographer({
        sink: (frame) => process.stdout.write(`[frame] ${frameSummary(frame)}\n`),
    })
    const conductor = new Conductor({
        prdPath,
        cwd,
        parallel: 0,
        timeoutSecs: 120,
        defaultModel: "sonnet",
    })

    auditor.join(env)
    cartographer.join(env)
    conductor.join(env)

    const summary = await conductor.run(env)

    process.stderr.write(
        `[demo] run complete: ${summary.completedStories.length} passed, ${summary.failedStories.length} failed in ${summary.totalDurationSecs}s (${summary.totalAttempts} total attempts)\n`,
    )
    process.stderr.write(
        `[demo] passed: ${summary.completedStories.join(", ") || "(none)"}\n`,
    )
    if (summary.failedStories.length > 0) {
        process.stderr.write(
            `[demo] failed: ${summary.failedStories.join(", ")}\n`,
        )
    }
    process.stderr.write(`[demo] audit log: ${logPath}\n`)
    process.stderr.write(`[demo] updated prd: ${prdPath}\n`)
}

main().catch((e: unknown) => {
    process.stderr.write(`[demo] fatal: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
