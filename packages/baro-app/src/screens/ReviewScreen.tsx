import React from "react"
import { Box, Text, useInput } from "ink"
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
    useInput((ch, key) => {
        if (key.escape) return onQuit()
        if (key.return || ch === "y") {
            const prdPath = path.join(process.cwd(), "prd.json")
            fs.writeFileSync(prdPath, JSON.stringify(prd, null, 2) + "\n")
            onAccept()
            return
        }
        if (ch === "r") return onRefine()
        if (ch === "q") return onQuit()
    })

    return (
        <Box flexDirection="column" paddingX={2} paddingY={1}>
            <Text bold color="cyan">▸ baro plan</Text>
            <Box marginTop={1} />

            <Box>
                <Text bold color="white">{prd.project}</Text>
                <Text dimColor> ({prd.branchName})</Text>
            </Box>
            <Text dimColor>{prd.description}</Text>
            <Box marginTop={1} />

            {prd.userStories.map((story) => {
                const deps = story.dependsOn.length > 0
                    ? ` ← ${story.dependsOn.join(", ")}`
                    : ""
                return (
                    <Box key={story.id} flexDirection="column" marginBottom={1}>
                        <Box>
                            <Text color="yellow">{story.id}</Text>
                            <Text> {story.title}</Text>
                            <Text dimColor>{deps}</Text>
                        </Box>
                        {story.acceptance.map((ac, i) => (
                            <Text key={i} dimColor>  ✓ {ac}</Text>
                        ))}
                    </Box>
                )
            })}

            <Text dimColor>
                {prd.userStories.length} stories · {new Set(prd.userStories.map(s => s.priority)).size} priority levels
            </Text>
            <Box marginTop={1}>
                <Text>
                    <Text bold color="green">Enter</Text><Text dimColor> accept · </Text>
                    <Text bold>r</Text><Text dimColor> refine · </Text>
                    <Text bold>q</Text><Text dimColor> quit</Text>
                </Text>
            </Box>
        </Box>
    )
}
