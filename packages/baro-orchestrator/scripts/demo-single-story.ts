#!/usr/bin/env tsx
/**
 * Phase 1 milestone-A demo: run one story end-to-end through the
 * orchestrator's bus, with no TUI, no DAG, no git, no review.
 *
 * Wires:
 *   ClaudeCliParticipant  ── emits typed Claude events on bus
 *   Auditor                ── persists everything to JSONL
 *   Cartographer           ── translates events to compact frames on stdout
 *   Originator             ── delivers the initial AgentTargetedMessageItem
 *
 * Success: the agent completes the trivial task, Auditor log contains
 * typed Mozaik items (UserMessage, FunctionCall, FunctionCallOutput,
 * ModelMessage, ClaudeResult), and Cartographer prints frames in real
 * time.
 */

import { ContextItem, Participant, AgenticEnvironment } from "@mozaik-ai/core"
import { mkdirSync, mkdtempSync, writeFileSync } from "fs"
import { tmpdir } from "os"
import { join } from "path"

import {
    AgentTargetedMessageItem,
    Auditor,
    Cartographer,
    ClaudeCliParticipant,
    type Frame,
} from "../src/main.js"

class Originator extends Participant {
    async onContextItem(): Promise<void> {
        return
    }
}

function setupTestRepo(): string {
    const dir = mkdtempSync(join(tmpdir(), "baro-orchestrator-demo-"))
    writeFileSync(
        join(dir, "README.md"),
        "# Demo repo\n\nA tiny repo for the orchestrator single-story demo.\n",
    )
    writeFileSync(
        join(dir, "package.json"),
        JSON.stringify({ name: "demo-repo", version: "0.0.0" }, null, 2),
    )
    writeFileSync(join(dir, "index.ts"), "export const greet = (n: string) => `Hello, ${n}!`\n")
    return dir
}

function frameSummary(frame: Frame): string {
    switch (frame.kind) {
        case "agent_state":
            return `state[${frame.agentId}] → ${frame.phase}${frame.detail ? ` (${frame.detail})` : ""}`
        case "user_message":
            return `user[${frame.agentId ?? "?"}] ${frame.text.slice(0, 80)}${frame.text.length > 80 ? "…" : ""}`
        case "model_message":
            return `assistant[${frame.agentId ?? "?"}] ${frame.text.slice(0, 80)}${frame.text.length > 80 ? "…" : ""}`
        case "tool_call":
            return `tool_call[${frame.agentId ?? "?"}] ${frame.name}(${frame.args.slice(0, 60)})`
        case "tool_result":
            return `tool_result[${frame.agentId ?? "?"}] ${frame.callId.slice(0, 12)} ${frame.output.slice(0, 60).replace(/\n/g, "⏎")}`
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

async function main(): Promise<void> {
    const cwd = setupTestRepo()
    process.stderr.write(`[demo] test repo: ${cwd}\n`)

    const logsDir = join(import.meta.dirname, "demo-logs")
    mkdirSync(logsDir, { recursive: true })
    const logPath = join(logsDir, `demo-single-${Date.now()}.jsonl`)

    const env = new AgenticEnvironment()

    const agent = new ClaudeCliParticipant("S1", { cwd })
    const auditor = new Auditor({ path: logPath })
    const cartographer = new Cartographer({
        sink: (frame) => process.stdout.write(`[frame] ${frameSummary(frame)}\n`),
    })
    const originator = new Originator()

    agent.join(env)
    auditor.join(env)
    cartographer.join(env)
    originator.join(env)

    agent.start(env)

    env.deliverContextItem(
        originator,
        new AgentTargetedMessageItem(
            "S1",
            "Use the Read tool to read README.md, then tell me its first heading. Keep your answer short.",
        ),
    )
    agent.closeStdin()

    const summary = await Promise.race([
        agent.done,
        new Promise<never>((_, rej) =>
            setTimeout(() => rej(new Error("demo timeout 120s")), 120_000),
        ),
    ])

    process.stderr.write(
        `[demo] agent done. exit=${summary.exitCode} session=${summary.sessionId} cost=$${summary.lastResult?.totalCostUsd?.toFixed(4) ?? "?"}\n`,
    )
    process.stderr.write(`[demo] audit log: ${logPath}\n`)
}

main().catch((e: unknown) => {
    process.stderr.write(`[demo] fatal: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
