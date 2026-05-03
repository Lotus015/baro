#!/usr/bin/env tsx
/**
 * Phase 3 demo: Critic value showcase.
 *
 * Runs a single-story PRD twice on snapshotted repos:
 *   • Pass A — withCritic: false (control)
 *   • Pass B — withCritic: true  (treatment)
 *
 * The story ("Add LICENSE file") is deliberately worded to trip up
 * Sonnet on turn 1: it must write an MIT LICENSE with copyright year
 * 2026. The acceptance criteria include a literal check for "2026".
 * Without the Critic, a hallucinated year (2024/2025) would slip
 * through undetected. With the Critic, the model gets a corrective
 * message and a second chance.
 *
 * After both passes we:
 *   1. Read the LICENSE file from each working tree and grep for "2026".
 *   2. Count CritiqueItem entries in the treatment audit log.
 *   3. Print a tally to stderr.
 *
 * Run with:
 *   npm run phase3
 */

import { execFileSync } from "child_process"
import {
    cpSync,
    existsSync,
    mkdtempSync,
    readFileSync,
    writeFileSync,
} from "fs"
import { tmpdir } from "os"
import { join } from "path"

import { orchestrate } from "../src/orchestrate.js"
import type { PrdFile } from "../src/main.js"

// ─── Helpers ─────────────────────────────────────────────────────────────────

function git(cwd: string, args: string[]): string {
    return execFileSync("git", args, { cwd, encoding: "utf8" })
}

function setupRepo(): string {
    const cwd = mkdtempSync(join(tmpdir(), "baro-phase3-"))
    git(cwd, ["init", "-q", "-b", "main"])
    git(cwd, ["config", "user.email", "phase3@baro.test"])
    git(cwd, ["config", "user.name", "Phase 3"])

    writeFileSync(
        join(cwd, "README.md"),
        "# baro-phase3-demo\n\nA tiny demo project. No license file yet.\n",
    )
    writeFileSync(
        join(cwd, "package.json"),
        JSON.stringify(
            {
                name: "baro-phase3-demo",
                version: "0.1.0",
                scripts: { build: "echo build" },
            },
            null,
            2,
        ),
    )

    git(cwd, ["add", "."])
    git(cwd, ["commit", "-q", "-m", "initial"])
    return cwd
}

function snapshotRepo(srcCwd: string): string {
    const dst = mkdtempSync(join(tmpdir(), "baro-phase3-clone-"))
    cpSync(srcCwd, dst, { recursive: true })
    return dst
}

function buildPrd(): PrdFile {
    return {
        project: "baro-phase3-demo",
        branchName: "phase3/run",
        description: "Phase 3 critic demo",
        userStories: [
            {
                id: "S1",
                priority: 1,
                title: "Add LICENSE file",
                description:
                    "Create a LICENSE file at the repository root containing a standard MIT " +
                    "license. The copyright line MUST use the year 2026 (the current year is " +
                    "2026-05-03) and the copyright holder MUST be 'Baro Project'. " +
                    "Use exactly this copyright line: 'Copyright (c) 2026 Baro Project'. " +
                    "After creating the file, commit it with the message 'add LICENSE'.",
                dependsOn: [],
                retries: 0,
                acceptance: [
                    "LICENSE file exists at the repo root",
                    "LICENSE file contains the year 2026 (not 2025 or any other year)",
                    "LICENSE file contains the copyright holder 'Baro Project'",
                    "commit message is 'add LICENSE'",
                ],
                tests: [],
                passes: false,
                completedAt: null,
                durationSecs: null,
            },
        ],
    }
}

// ─── Audit log parsing ────────────────────────────────────────────────────────

interface AuditEntry {
    ts: string
    source: string
    item: { type: string; [key: string]: unknown }
}

function countCritiqueItems(auditPath: string): number {
    if (!existsSync(auditPath)) return 0
    const text = readFileSync(auditPath, "utf8")
    let count = 0
    for (const line of text.split("\n")) {
        if (!line.trim()) continue
        let entry: AuditEntry
        try {
            entry = JSON.parse(line) as AuditEntry
        } catch {
            continue
        }
        if (entry.item.type === "critique") count++
    }
    return count
}

function yearFoundInLicense(cwd: string): boolean {
    const licensePath = join(cwd, "LICENSE")
    if (!existsSync(licensePath)) return false
    const contents = readFileSync(licensePath, "utf8")
    return contents.includes("2026")
}

// ─── Run a single pass ────────────────────────────────────────────────────────

interface PassResult {
    auditLog: string
    elapsedSecs: number
    yearFound: boolean
}

async function runPass(
    label: string,
    cwd: string,
    prdPath: string,
    withCritic: boolean,
): Promise<PassResult> {
    const auditLog = join(cwd, `audit-${label}.jsonl`)
    process.stderr.write(
        `\n[phase3] ──── pass ${label} (withCritic=${withCritic}) ────\n`,
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
        withLibrarian: false,
        withSentry: false,
        withCritic,
        auditLogPath: auditLog,
    })
    const elapsed = Math.round((Date.now() - startedAt) / 1000)
    process.stderr.write(
        `[phase3] pass ${label} done in ${elapsed}s — passed=${result.summary.completedStories.length} failed=${result.summary.failedStories.length}\n`,
    )

    const yearFound = yearFoundInLicense(cwd)
    process.stderr.write(
        `[phase3] pass ${label}: LICENSE year=2026 found? ${yearFound}\n`,
    )

    return { auditLog, elapsedSecs: elapsed, yearFound }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
    const repoA = setupRepo()
    process.stderr.write(`[phase3] base repo (control):   ${repoA}\n`)

    const repoB = snapshotRepo(repoA)
    process.stderr.write(`[phase3] base repo (treatment): ${repoB}\n`)

    const prdA = join(repoA, "prd.json")
    const prdB = join(repoB, "prd.json")
    writeFileSync(prdA, JSON.stringify(buildPrd(), null, 2) + "\n")
    writeFileSync(prdB, JSON.stringify(buildPrd(), null, 2) + "\n")

    const passA = await runPass("control", repoA, prdA, false)
    const passB = await runPass("treatment", repoB, prdB, true)

    const critiqueCount = countCritiqueItems(passB.auditLog)

    process.stderr.write(`\n[phase3] ──── tally ────\n`)
    process.stderr.write(
        `  control   (withCritic=false): LICENSE year=2026 found? ${passA.yearFound}\n`,
    )
    process.stderr.write(
        `  treatment (withCritic=true):  LICENSE year=2026 found? ${passB.yearFound}\n`,
    )
    process.stderr.write(
        `  treatment CritiqueItem count: ${critiqueCount}\n`,
    )
    process.stderr.write(
        `\n[phase3] wall clock: control=${passA.elapsedSecs}s  treatment=${passB.elapsedSecs}s\n`,
    )
    process.stderr.write(
        `[phase3] keep repos for inspection:\n  control:   ${repoA}\n  treatment: ${repoB}\n`,
    )
}

main().catch((e: unknown) => {
    process.stderr.write(`[phase3] fatal: ${(e as Error)?.stack ?? String(e)}\n`)
    process.exit(1)
})
