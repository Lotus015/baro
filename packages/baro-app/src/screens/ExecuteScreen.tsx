import React, { useEffect } from "react"
import { Box, Text, useApp } from "ink"
import { spawn, execSync } from "child_process"
import * as fs from "fs"
import * as path from "path"
import { fileURLToPath } from "url"

interface Props {
    prd: any
    onDone: () => void
}

function findBinary(name: string, searchFrom: string): string | null {
    // 1. bin/ next to dist/ (published npm package layout)
    const pkgBin = path.resolve(searchFrom, "..", "bin", name)
    if (existsExec(pkgBin)) return pkgBin

    // 2. Cargo target (local dev - walk up to find Cargo.toml)
    let dir = searchFrom
    for (let i = 0; i < 10; i++) {
        const cargoTarget = path.join(dir, "target", "release", name)
        if (existsExec(cargoTarget)) return cargoTarget
        const parent = path.dirname(dir)
        if (parent === dir) break
        dir = parent
    }

    // 3. In PATH
    try {
        const which = execSync(`which ${name}`, { encoding: "utf-8" }).trim()
        if (which) return which
    } catch {}

    return null
}

function existsExec(p: string): boolean {
    try {
        fs.accessSync(p, fs.constants.X_OK)
        return true
    } catch {
        return false
    }
}

export function ExecuteScreen({ prd, onDone }: Props) {
    const app = useApp()

    useEffect(() => {
        setTimeout(() => {
            app.exit()

            const thisDir = path.dirname(fileURLToPath(import.meta.url))
            const executorPath = path.join(thisDir, "core", "executor.js")
            const tuiBinary = findBinary("baro-tui", thisDir)

            if (!tuiBinary) {
                console.error("\nError: baro-tui binary not found.")
                console.error("Build it: cd <baro-repo> && cargo build --release")
                console.error("Or install: npm install -g @baro-ai/cli\n")
                process.exit(1)
            }

            const executor = spawn("node", [executorPath], {
                cwd: process.cwd(),
                stdio: ["ignore", "pipe", "inherit"],
                env: { ...process.env },
            })

            const tui = spawn(tuiBinary, [], {
                cwd: process.cwd(),
                stdio: [executor.stdout, "inherit", "inherit"],
            })

            tui.on("error", (err) => {
                console.error("\nFailed to start baro-tui:", err.message)
                process.exit(1)
            })

            tui.on("close", () => process.exit(0))
        }, 100)
    }, [app])

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Text bold color="cyan">▸ baro</Text>
            <Text dimColor>Starting execution dashboard...</Text>
        </Box>
    )
}
