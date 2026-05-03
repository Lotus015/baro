#!/usr/bin/env tsx
/**
 * baro-orchestrator CLI — standalone entry for end-user testing.
 *
 * Until the Rust TUI is rewired to spawn this as a subprocess, you can
 * run the orchestrator directly:
 *
 *   tsx packages/baro-orchestrator/scripts/cli.ts \
 *       --prd ./prd.json \
 *       --cwd . \
 *       --parallel 0 \
 *       --timeout 600 \
 *       [--model sonnet|opus|haiku] \
 *       [--no-git] \
 *       [--no-tui-events] \
 *       [--audit-log ./audit.jsonl]
 *
 * Prints BaroEvents to stdout (one JSON per line) by default, plus a
 * compact human summary on stderr.
 */

import { existsSync } from "fs"
import { resolve } from "path"

import { orchestrate, type OrchestrateConfig } from "../src/orchestrate.js"

interface CliArgs {
    prd: string
    cwd: string
    parallel: number
    timeout: number
    model?: string
    noGit: boolean
    noTuiEvents: boolean
    auditLog?: string
    help: boolean
}

function parseArgs(argv: string[]): CliArgs {
    const args: CliArgs = {
        prd: "prd.json",
        cwd: ".",
        parallel: 0,
        timeout: 600,
        noGit: false,
        noTuiEvents: false,
        help: false,
    }
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i]
        switch (a) {
            case "-h":
            case "--help":
                args.help = true
                break
            case "--prd":
                args.prd = required(argv, ++i, "--prd")
                break
            case "--cwd":
                args.cwd = required(argv, ++i, "--cwd")
                break
            case "--parallel":
                args.parallel = parseInt(required(argv, ++i, "--parallel"), 10)
                break
            case "--timeout":
                args.timeout = parseInt(required(argv, ++i, "--timeout"), 10)
                break
            case "--model":
                args.model = required(argv, ++i, "--model")
                break
            case "--no-git":
                args.noGit = true
                break
            case "--no-tui-events":
                args.noTuiEvents = true
                break
            case "--audit-log":
                args.auditLog = required(argv, ++i, "--audit-log")
                break
            default:
                process.stderr.write(`[cli] unknown flag: ${a}\n`)
                process.exit(2)
        }
    }
    return args
}

function required(argv: string[], i: number, flag: string): string {
    const v = argv[i]
    if (v == null) {
        process.stderr.write(`[cli] flag ${flag} requires a value\n`)
        process.exit(2)
    }
    return v
}

function printHelp(): void {
    process.stdout.write(
        [
            "baro-orchestrator CLI",
            "",
            "Usage:",
            "  cli.ts --prd <path> --cwd <path> [options]",
            "",
            "Options:",
            "  --prd <path>          Path to prd.json (default: ./prd.json)",
            "  --cwd <path>          Working directory (default: .)",
            "  --parallel <N>        Max parallel stories per level (0 = unlimited)",
            "  --timeout <secs>      Per-story timeout (default: 600)",
            "  --model <name>        Override model (opus, sonnet, haiku)",
            "  --no-git              Skip git lifecycle (branch / push)",
            "  --no-tui-events       Skip BaroEvent JSON emission",
            "  --audit-log <path>    Persist all bus events to JSONL",
            "  -h, --help            Show this message",
            "",
        ].join("\n"),
    )
}

async function main(): Promise<void> {
    const args = parseArgs(process.argv.slice(2))
    if (args.help) {
        printHelp()
        return
    }

    const cwd = resolve(args.cwd)
    const prdPath = resolve(cwd, args.prd)
    if (!existsSync(prdPath)) {
        process.stderr.write(`[cli] PRD not found: ${prdPath}\n`)
        process.exit(2)
    }

    const config: OrchestrateConfig = {
        prdPath,
        cwd,
        parallel: args.parallel,
        timeoutSecs: args.timeout,
        overrideModel: args.model ?? null,
        defaultModel: args.model ?? "sonnet",
        emitTuiEvents: !args.noTuiEvents,
        withGit: args.noGit ? false : undefined,
        auditLogPath: args.auditLog,
    }

    process.stderr.write(
        `[cli] starting orchestrator: prd=${prdPath} cwd=${cwd} parallel=${args.parallel} timeout=${args.timeout}s\n`,
    )

    const startedAt = Date.now()
    try {
        const result = await orchestrate(config)
        const elapsed = Math.round((Date.now() - startedAt) / 1000)
        const passed = result.summary.completedStories.length
        const failed = result.summary.failedStories.length
        process.stderr.write(
            `[cli] complete in ${elapsed}s — ${passed} passed, ${failed} failed (${result.summary.totalAttempts} attempts)\n`,
        )
        if (failed > 0) {
            process.stderr.write(
                `[cli] failed stories: ${result.summary.failedStories.join(", ")}\n`,
            )
            process.exit(1)
        }
    } catch (e) {
        process.stderr.write(
            `[cli] fatal: ${(e as Error)?.stack ?? String(e)}\n`,
        )
        process.exit(1)
    }
}

main().catch((e: unknown) => {
    process.stderr.write(`[cli] unhandled: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
