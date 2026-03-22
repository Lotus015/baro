import React, { useState, useCallback, useEffect } from "react"
import { Box, Text, useInput } from "ink"
import Spinner from "ink-spinner"
import { Planner } from "../core/planner.js"
import { ClaudePlanner } from "../core/claude-planner.js"
import type { PrdV2 } from "../core/prd.js"
import type { PlannerMode } from "../App.js"

interface Props {
    plannerMode: PlannerMode
    onPlanReady: (prd: PrdV2) => void
    onQuit: () => void
}

export function PlanScreen({ plannerMode, onPlanReady, onQuit }: Props) {
    const [input, setInput] = useState("")
    const [loading, setLoading] = useState(false)
    const [elapsed, setElapsed] = useState(0)
    const [toolCalls, setToolCalls] = useState<string[]>([])
    const [error, setError] = useState("")
    const [planner] = useState(() => {
        if (plannerMode === "openai") {
            return new Planner({
                cwd: process.cwd(),
                onToken: () => {},
                onToolCall: (name: string, args: any) => {
                    let label = name
                    if (name === "read_file") label = `Reading ${args?.path ?? "..."}`
                    else if (name === "grep") label = `Searching for "${args?.pattern ?? "..."}"`
                    else if (name === "list_files") label = `Listing ${args?.path || "root"}`
                    else if (name === "file_tree") label = "Scanning project structure"
                    setToolCalls((prev) => [...prev.slice(-8), label])
                },
            })
        }
        return new ClaudePlanner({
            cwd: process.cwd(),
            onLog: (line: string) => {
                setToolCalls((prev) => [...prev.slice(-8), line.slice(0, 80)])
            },
        })
    })

    // Elapsed timer while loading
    useEffect(() => {
        if (!loading) return
        const interval = setInterval(() => setElapsed((e) => e + 1), 1000)
        return () => clearInterval(interval)
    }, [loading])

    const submit = useCallback(async () => {
        const goal = input.trim()
        if (!goal) return

        setLoading(true)
        setElapsed(0)
        setToolCalls([])
        setError("")

        try {
            const prd = await planner.send(goal)
            onPlanReady(prd)
        } catch (err: any) {
            setError(err.message ?? String(err))
            setLoading(false)
        }
    }, [input, planner, onPlanReady])

    useInput((ch, key) => {
        if (key.escape) return onQuit()
        if (loading) return

        if (key.return) {
            submit()
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

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Text bold color="cyan">▸ baro plan</Text>
            <Text dimColor>Describe what you want to build</Text>
            <Box marginTop={1} />

            {loading ? (
                <Box flexDirection="column">
                    <Box>
                        <Text color="cyan"><Spinner type="dots" /></Text>
                        <Text> {toolCalls.length > 0 ? "Exploring codebase..." : "Generating plan..."} </Text>
                        <Text dimColor>({elapsed}s)</Text>
                    </Box>
                    {toolCalls.map((tc, i) => (
                        <Text key={i} dimColor>  ⚙ {tc}</Text>
                    ))}
                </Box>
            ) : (
                <Box>
                    <Text dimColor>{"❯ "}</Text>
                    <Text>{input}</Text>
                    <Text color="cyan">█</Text>
                </Box>
            )}

            {error ? (
                <Box marginTop={1}>
                    <Text color="red">{error}</Text>
                </Box>
            ) : null}

            <Box marginTop={1}>
                <Text dimColor>Enter to generate · Esc to quit</Text>
            </Box>
        </Box>
    )
}
