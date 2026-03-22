import React, { useState } from "react"
import { Box, Text, useInput, useStdout } from "ink"
import * as fs from "fs"
import * as path from "path"
import type { PrdV2 } from "../core/prd.js"

interface Props {
    prd: PrdV2
    onAccept: () => void
    onRefine: () => void
    onQuit: () => void
}

export function ReviewScreen({ prd, onAccept, onRefine, onQuit }: Props) {
    const { stdout } = useStdout()
    const termHeight = stdout?.rows ?? 40
    const [scrollOffset, setScrollOffset] = useState(0)

    // Build all lines for the plan
    const lines: { text: string; color?: string; dim?: boolean; bold?: boolean }[] = []

    lines.push({ text: "" })
    lines.push({ text: `${prd.project} (${prd.branchName})`, bold: true })
    lines.push({ text: prd.description, dim: true })
    lines.push({ text: "" })

    for (const story of prd.userStories) {
        const deps = story.dependsOn.length > 0 ? ` ← ${story.dependsOn.join(", ")}` : ""
        lines.push({ text: `${story.id} ${story.title}${deps}`, color: "yellow" })
        for (const ac of story.acceptance) {
            lines.push({ text: `  ✓ ${ac}`, dim: true })
        }
        lines.push({ text: "" })
    }

    lines.push({ text: `${prd.userStories.length} stories · ${new Set(prd.userStories.map(s => s.priority)).size} priority levels`, dim: true })

    // Visible window: terminal height minus header (3) and footer (3)
    const visibleHeight = termHeight - 6
    const maxOffset = Math.max(0, lines.length - visibleHeight)
    const visibleLines = lines.slice(scrollOffset, scrollOffset + visibleHeight)
    const canScrollDown = scrollOffset < maxOffset
    const canScrollUp = scrollOffset > 0

    useInput((ch, key) => {
        if (key.escape || ch === "q") return onQuit()
        if (key.return || ch === "y") {
            const prdPath = path.join(process.cwd(), "prd.json")
            fs.writeFileSync(prdPath, JSON.stringify(prd, null, 2) + "\n")
            onAccept()
            return
        }
        if (ch === "r") return onRefine()
        if (key.downArrow || ch === "j") setScrollOffset((o) => Math.min(o + 1, maxOffset))
        if (key.upArrow || ch === "k") setScrollOffset((o) => Math.max(o - 1, 0))
        if (key.pageDown) setScrollOffset((o) => Math.min(o + 10, maxOffset))
        if (key.pageUp) setScrollOffset((o) => Math.max(o - 10, 0))
    })

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Box>
                <Text bold color="cyan">▸ baro plan</Text>
                {canScrollUp && <Text dimColor>  ↑ scroll up</Text>}
            </Box>

            {visibleLines.map((line, i) => (
                <Text
                    key={i + scrollOffset}
                    color={line.color as any}
                    dimColor={line.dim}
                    bold={line.bold}
                >{line.text}</Text>
            ))}

            {canScrollDown && <Text dimColor>  ↓ more ({lines.length - scrollOffset - visibleHeight} lines)</Text>}

            <Box marginTop={1}>
                <Text>
                    <Text bold color="green">Enter</Text><Text dimColor> accept · </Text>
                    <Text bold>r</Text><Text dimColor> refine · </Text>
                    <Text bold>q</Text><Text dimColor> quit · </Text>
                    <Text bold>↑↓</Text><Text dimColor> scroll</Text>
                </Text>
            </Box>
        </Box>
    )
}
