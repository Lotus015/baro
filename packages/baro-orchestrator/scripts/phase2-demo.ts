#!/usr/bin/env tsx
/**
 * Phase 2 demo: Librarian + Sentry value showcase.
 *
 * Runs the SAME 2-level PRD twice on a fresh temp repo:
 *   • Pass A — withLibrarian: false (control)
 *   • Pass B — withLibrarian: true  (treatment)
 *
 * In both passes, S1 explores the codebase and S2 (deps on S1) writes
 * a summary file. With Librarian on, S2's prompt is augmented with
 * S1's findings, so S2 should issue fewer redundant Read/Grep tool
 * calls. We tally tool-calls per story from each run's audit log.
 *
 * Sentry runs in both passes; if both stories happen to touch the same
 * file you'll see CoordinationItem warnings in the audit log.
 *
 * Run with:
 *   npm run phase2
 */

import { execFileSync } from "child_process"
import {
    existsSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    writeFileSync,
    rmSync,
    cpSync,
} from "fs"
import { tmpdir } from "os"
import { join } from "path"

import { orchestrate } from "../src/orchestrate.js"
import type { PrdFile } from "../src/main.js"

function git(cwd: string, args: string[]): string {
    return execFileSync("git", args, { cwd, encoding: "utf8" })
}

function setupRepo(): string {
    const cwd = mkdtempSync(join(tmpdir(), "baro-phase2-"))
    git(cwd, ["init", "-q", "-b", "main"])
    git(cwd, ["config", "user.email", "phase2@baro.test"])
    git(cwd, ["config", "user.name", "Phase 2"])

    writeFileSync(
        join(cwd, "README.md"),
        "# baro-phase2-demo\n\nTiny project. Has a CLI under src/cli.ts and a worker under src/worker.ts.\n",
    )
    writeFileSync(
        join(cwd, "package.json"),
        JSON.stringify(
            {
                name: "baro-phase2-demo",
                version: "0.1.0",
                bin: { tinycli: "src/cli.ts" },
                main: "src/index.ts",
                scripts: { build: "echo build" },
            },
            null,
            2,
        ),
    )
    mkdirSync(join(cwd, "src"), { recursive: true })
    writeFileSync(
        join(cwd, "src/cli.ts"),
        "export const cli = (args: string[]) => console.log('cli args:', args)\n",
    )
    writeFileSync(
        join(cwd, "src/worker.ts"),
        "export const work = (n: number) => n * 2\n",
    )
    writeFileSync(
        join(cwd, "src/index.ts"),
        "export * from './cli'\nexport * from './worker'\n",
    )

    git(cwd, ["add", "."])
    git(cwd, ["commit", "-q", "-m", "initial"])
    return cwd
}

function buildPrd(): PrdFile {
    return {
        project: "baro-phase2-demo",
        branchName: "phase2/run",
        description: "Phase 2 librarian/sentry demo",
        userStories: [
            {
                id: "S1",
                priority: 1,
                title: "Map the project structure",
                description:
                    "Inspect the project to identify (a) the project name from package.json, " +
                    "(b) the main entry point listed in package.json, and (c) the bin entries. " +
                    "Use the Read tool on package.json and any relevant source files. " +
                    "Then create a file `MAP.md` at the repo root containing a markdown bullet list " +
                    "with these three findings, and commit it with the message 'add MAP.md'.",
                dependsOn: [],
                retries: 0,
                acceptance: [
                    "MAP.md exists at repo root",
                    "MAP.md mentions the project name, main entry, and bin entries",
                    "commit message is 'add MAP.md'",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
            {
                id: "S2",
                priority: 2,
                title: "Write project summary based on S1 findings",
                description:
                    "Using whatever you already know about the project (you may have it from prior agents — " +
                    "check the [librarian] block at the top of this prompt if present), " +
                    "create a `SUMMARY.md` file at the repo root that contains a one-paragraph project " +
                    "description that mentions the project name and the role of each src/ file. " +
                    "Avoid re-reading files you already have information about. " +
                    "Commit with the message 'add SUMMARY.md'.",
                dependsOn: ["S1"],
                retries: 0,
                acceptance: [
                    "SUMMARY.md exists at repo root",
                    "SUMMARY.md mentions the project name",
                    "commit message is 'add SUMMARY.md'",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
        ],
    }
}

function snapshotRepo(srcCwd: string): string {
    const dst = mkdtempSync(join(tmpdir(), "baro-phase2-clone-"))
    cpSync(srcCwd, dst, { recursive: true })
    return dst
}

function tallyToolCalls(auditPath: string): Map<string, Map<string, number>> {
    const text = readFileSync(auditPath, "utf8")
    const result = new Map<string, Map<string, number>>()
    for (const line of text.split("\n")) {
        if (!line.trim()) continue
        let entry: { source: string; item: { type: string; name?: string } }
        try {
            entry = JSON.parse(line) as typeof entry
        } catch {
            continue
        }
        if (entry.item.type !== "function_call") continue
        const sourceMatch = /^ClaudeCliParticipant:(\S+)$/.exec(entry.source)
        if (!sourceMatch) continue
        const agentId = sourceMatch[1]!
        const tool = entry.item.name ?? "?"
        const byAgent = result.get(agentId) ?? new Map<string, number>()
        byAgent.set(tool, (byAgent.get(tool) ?? 0) + 1)
        result.set(agentId, byAgent)
    }
    return result
}

interface PassResult {
    auditLog: string
    elapsedSecs: number
    toolsByAgent: Map<string, Map<string, number>>
}

async function runPass(
    label: string,
    cwd: string,
    prdPath: string,
    withLibrarian: boolean,
): Promise<PassResult> {
    const auditLog = join(cwd, `audit-${label}.jsonl`)
    process.stderr.write(
        `\n[phase2] ──── pass ${label} (withLibrarian=${withLibrarian}) ────\n`,
    )
    const startedAt = Date.now()
    const result = await orchestrate({
        prdPath,
        cwd,
        parallel: 1,
        timeoutSecs: 240,
        defaultModel: "sonnet",
        emitTuiEvents: false,
        withGit: true,
        withLibrarian,
        withSentry: true,
        auditLogPath: auditLog,
    })
    const elapsed = Math.round((Date.now() - startedAt) / 1000)
    process.stderr.write(
        `[phase2] pass ${label} done in ${elapsed}s — passed=${result.summary.completedStories.length} failed=${result.summary.failedStories.length}\n`,
    )
    return {
        auditLog,
        elapsedSecs: elapsed,
        toolsByAgent: tallyToolCalls(auditLog),
    }
}

function printTally(label: string, tally: Map<string, Map<string, number>>): void {
    process.stderr.write(`[phase2] ${label} tool calls per story:\n`)
    for (const [agentId, tools] of [...tally.entries()].sort()) {
        const total = [...tools.values()].reduce((a, b) => a + b, 0)
        const breakdown = [...tools.entries()]
            .map(([t, c]) => `${t}=${c}`)
            .join(" ")
        process.stderr.write(`  ${agentId}: ${total} total (${breakdown})\n`)
    }
}

async function main(): Promise<void> {
    const repoA = setupRepo()
    process.stderr.write(`[phase2] base repo (control): ${repoA}\n`)

    // Snapshot the same starting state for the second pass so both runs
    // see an identical environment.
    const repoB = snapshotRepo(repoA)
    process.stderr.write(`[phase2] base repo (treatment): ${repoB}\n`)

    // Use a tweaked PRD for each (separate prd.json so they don't
    // collide), but build it from the same blueprint.
    const prdA = join(repoA, "prd.json")
    const prdB = join(repoB, "prd.json")
    writeFileSync(prdA, JSON.stringify(buildPrd(), null, 2) + "\n")
    writeFileSync(prdB, JSON.stringify(buildPrd(), null, 2) + "\n")

    const passA = await runPass("control", repoA, prdA, false)
    const passB = await runPass("treatment", repoB, prdB, true)

    process.stderr.write(`\n[phase2] ──── tally ────\n`)
    printTally("control  (no Librarian)", passA.toolsByAgent)
    printTally("treatment (with Librarian)", passB.toolsByAgent)

    const totalA = sumAll(passA.toolsByAgent)
    const totalB = sumAll(passB.toolsByAgent)
    process.stderr.write(
        `\n[phase2] total tool calls: control=${totalA}  treatment=${totalB}  Δ=${totalA - totalB}\n`,
    )
    process.stderr.write(
        `[phase2] wall clock: control=${passA.elapsedSecs}s  treatment=${passB.elapsedSecs}s\n`,
    )
    process.stderr.write(
        `[phase2] keep repos for inspection:\n  control:   ${repoA}\n  treatment: ${repoB}\n`,
    )

    // Best-effort: also extract the Librarian context that was injected
    // into S2's prompt. We grep the treatment audit log for the
    // AgentTargetedMessageItem-like pattern, but those don't exist —
    // the prompt is sent as a Claude `user` event. Instead, dump the
    // first user_message logged for S2 from the treatment run.
    void existsSync
    void rmSync
}

function sumAll(t: Map<string, Map<string, number>>): number {
    let n = 0
    for (const tools of t.values()) {
        for (const c of tools.values()) n += c
    }
    return n
}

main().catch((e: unknown) => {
    process.stderr.write(`[phase2] fatal: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
