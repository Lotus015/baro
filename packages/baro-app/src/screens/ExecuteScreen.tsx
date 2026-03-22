import React, { useEffect } from "react"
import { Box, Text, useApp } from "ink"
import { spawn } from "child_process"
import * as path from "path"

interface Props {
    prd: any
    onDone: () => void
}

export function ExecuteScreen({ prd, onDone }: Props) {
    const app = useApp()

    useEffect(() => {
        // Exit Ink, then launch ratatui TUI
        // Small delay to let Ink cleanup
        setTimeout(() => {
            app.exit()

            // Find baro-tui binary
            const appDir = path.dirname(new URL(import.meta.url).pathname)
            const possiblePaths = [
                path.join(appDir, "..", "..", "..", "crates", "baro-tui", "target", "release", "baro-tui"),
                path.join(appDir, "..", "bin", "baro-tui"),
                "baro-tui", // in PATH
            ]

            // Find executor
            const executorPath = path.join(appDir, "core", "executor.js")

            const tuiBinary = possiblePaths.find((p) => {
                try { require("fs").accessSync(p); return true } catch { return false }
            }) ?? possiblePaths[possiblePaths.length - 1]

            // Spawn: node executor.js | baro-tui
            const executor = spawn("node", [executorPath], {
                cwd: process.cwd(),
                stdio: ["ignore", "pipe", "inherit"],
                env: { ...process.env },
            })

            const tui = spawn(tuiBinary, [], {
                cwd: process.cwd(),
                stdio: [executor.stdout, "inherit", "inherit"],
            })

            tui.on("close", () => process.exit(0))
            executor.on("close", () => {
                // Give TUI a moment to process remaining events
                setTimeout(() => {}, 1000)
            })
        }, 100)
    }, [app])

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Text bold color="cyan">▸ baro</Text>
            <Text dimColor>Starting execution dashboard...</Text>
        </Box>
    )
}
