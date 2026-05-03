#!/usr/bin/env tsx
/**
 * End-to-end smoke test for the orchestrator. Sets up a fresh temp git
 * repo, drops a 2-story PRD that asks Claude to create simple files
 * and commit them, runs the CLI orchestrator end-to-end, and verifies
 * the resulting git history.
 *
 * Run with:
 *   cd packages/baro-orchestrator && tsx scripts/e2e-test.ts
 */

import { execFileSync } from "child_process"
import { existsSync, mkdtempSync, writeFileSync } from "fs"
import { tmpdir } from "os"
import { join } from "path"

import { orchestrate, type OrchestrateConfig } from "../src/orchestrate.js"

function git(cwd: string, args: string[]): string {
    return execFileSync("git", args, { cwd, encoding: "utf8" })
}

function setupRepo(): { cwd: string; prdPath: string } {
    const cwd = mkdtempSync(join(tmpdir(), "baro-e2e-"))
    process.stderr.write(`[e2e] repo: ${cwd}\n`)

    git(cwd, ["init", "-q", "-b", "main"])
    git(cwd, ["config", "user.email", "e2e@baro.test"])
    git(cwd, ["config", "user.name", "Baro E2E"])
    writeFileSync(
        join(cwd, "README.md"),
        "# baro-e2e\n\nE2E orchestrator test repo.\n",
    )
    git(cwd, ["add", "."])
    git(cwd, ["commit", "-q", "-m", "initial"])

    const prd = {
        project: "baro-e2e",
        branchName: "baro-e2e/test",
        description: "E2E orchestrator smoke test",
        userStories: [
            {
                id: "S1",
                priority: 1,
                title: "Add MIT LICENSE",
                description:
                    "Create a `LICENSE` file at the repo root containing the MIT License text " +
                    "with copyright year 2026 and copyright holder 'baro test'. " +
                    "After creating the file, stage and commit it with the message 'add MIT LICENSE'. " +
                    "Do not modify any other file.",
                dependsOn: [],
                retries: 1,
                acceptance: [
                    "LICENSE file exists at repo root with MIT text",
                    "the commit message is 'add MIT LICENSE'",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
            {
                id: "S2",
                priority: 1,
                title: "Add .gitignore",
                description:
                    "Create a `.gitignore` file at the repo root with the following lines: " +
                    "`node_modules/`, `dist/`, `*.log`, `.env`. " +
                    "After creating the file, stage and commit it with the message 'add .gitignore'. " +
                    "Do not modify any other file.",
                dependsOn: [],
                retries: 1,
                acceptance: [
                    ".gitignore file exists at repo root with required entries",
                    "the commit message is 'add .gitignore'",
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

function verify(cwd: string): { ok: boolean; reasons: string[] } {
    const reasons: string[] = []
    const branch = git(cwd, ["branch", "--show-current"]).trim()
    if (branch !== "baro-e2e/test") {
        reasons.push(`expected branch baro-e2e/test, got '${branch}'`)
    }
    if (!existsSync(join(cwd, "LICENSE"))) {
        reasons.push("LICENSE file missing")
    }
    if (!existsSync(join(cwd, ".gitignore"))) {
        reasons.push(".gitignore file missing")
    }
    const log = git(cwd, ["log", "--oneline", "-10"]).trim()
    if (!/add MIT LICENSE|add LICENSE|MIT LICENSE/i.test(log)) {
        reasons.push("LICENSE commit not found in log")
    }
    if (!/\.gitignore/i.test(log)) {
        reasons.push(".gitignore commit not found in log")
    }
    return { ok: reasons.length === 0, reasons }
}

async function main(): Promise<void> {
    const { cwd, prdPath } = setupRepo()

    const auditLog = join(cwd, "audit.jsonl")
    const config: OrchestrateConfig = {
        prdPath,
        cwd,
        parallel: 2,
        timeoutSecs: 240,
        overrideModel: null,
        defaultModel: "sonnet",
        emitTuiEvents: false, // suppress JSON output, we just want stderr summary
        withGit: true,
        auditLogPath: auditLog,
    }

    process.stderr.write(`[e2e] running orchestrator...\n`)
    const startedAt = Date.now()
    const result = await orchestrate(config)
    const elapsed = Math.round((Date.now() - startedAt) / 1000)

    process.stderr.write(
        `[e2e] orchestrator complete in ${elapsed}s — passed=${result.summary.completedStories.length} failed=${result.summary.failedStories.length}\n`,
    )
    process.stderr.write(`[e2e] audit log: ${auditLog}\n`)
    process.stderr.write(`[e2e] git log:\n`)
    process.stderr.write(git(cwd, ["log", "--oneline", "-10"]).split("\n").map((l) => `  ${l}`).join("\n") + "\n")

    const verdict = verify(cwd)
    if (verdict.ok) {
        process.stderr.write(`[e2e] ✓ PASS — repo state matches expectations\n`)
        process.stderr.write(`[e2e] keep repo: ${cwd}\n`)
    } else {
        process.stderr.write(`[e2e] ✗ FAIL\n`)
        for (const r of verdict.reasons) {
            process.stderr.write(`  - ${r}\n`)
        }
        process.stderr.write(`[e2e] keep repo for debug: ${cwd}\n`)
        process.exit(1)
    }
}

main().catch((e: unknown) => {
    process.stderr.write(`[e2e] fatal: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
