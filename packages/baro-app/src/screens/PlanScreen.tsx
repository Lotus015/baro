import React, { useState, useCallback } from "react"
import { Box, Text, useInput } from "ink"
import Spinner from "ink-spinner"
import { Planner } from "../core/planner.js"
import type { PrdV2 } from "../core/prd.js"

interface Props {
    onPlanReady: (prd: PrdV2) => void
    onQuit: () => void
}

export function PlanScreen({ onPlanReady, onQuit }: Props) {
    const [input, setInput] = useState("")
    const [loading, setLoading] = useState(false)
    const [tokenCount, setTokenCount] = useState(0)
    const [error, setError] = useState("")
    const [planner] = useState(() => new Planner({
        cwd: process.cwd(),
        onToken: () => setTokenCount((c) => c + 1),
    }))

    const submit = useCallback(async () => {
        const goal = input.trim()
        if (!goal) return

        setLoading(true)
        setTokenCount(0)
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
                <Box>
                    <Text color="cyan"><Spinner type="dots" /></Text>
                    <Text> Generating plan... </Text>
                    <Text dimColor>({tokenCount} tokens)</Text>
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
