import React, { useState } from "react"
import { Box, Text, useInput } from "ink"
import * as fs from "fs"
import * as path from "path"

interface Props {
    onComplete: () => void
    onQuit: () => void
}

export function ApiKeyScreen({ onComplete, onQuit }: Props) {
    const [input, setInput] = useState("")
    const [error, setError] = useState("")

    useInput((ch, key) => {
        if (key.escape) return onQuit()
        if (key.return) {
            if (!input.trim()) {
                setError("API key is required")
                return
            }
            saveKey(input.trim())
            onComplete()
            return
        }
        if (key.backspace || key.delete) {
            setInput((v) => v.slice(0, -1))
            return
        }
        if (ch && !key.ctrl) {
            setInput((v) => v + ch)
            setError("")
        }
    })

    const masked = input.length <= 8
        ? input
        : input.slice(0, 4) + "•".repeat(Math.min(input.length - 8, 30)) + input.slice(-4)

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Text bold color="cyan">▸ baro</Text>
            <Text dimColor>Autonomous parallel coding</Text>
            <Box marginTop={1} />
            <Text>Paste your API key to get started:</Text>
            <Box marginTop={1}>
                <Text dimColor>{"❯ "}</Text>
                <Text color="cyan">{masked || <Text dimColor>sk-...</Text>}</Text>
                <Text color="cyan">█</Text>
            </Box>
            {error ? (
                <Box marginTop={1}>
                    <Text color="red">{error}</Text>
                </Box>
            ) : null}
            <Box marginTop={1}>
                <Text dimColor>Enter to save · Esc to quit · Saved to ~/.baro/.env</Text>
            </Box>
        </Box>
    )
}

function saveKey(key: string) {
    const home = process.env.HOME ?? ""
    const dir = path.join(home, ".baro")
    fs.mkdirSync(dir, { recursive: true })
    const envPath = path.join(dir, ".env")

    const isAnthropic = key.startsWith("sk-ant-")
    const varName = isAnthropic ? "ANTHROPIC_API_KEY" : "OPENAI_API_KEY"
    fs.writeFileSync(envPath, `${varName}=${key}\n`, { mode: 0o600 })
    process.env[varName] = key
}
