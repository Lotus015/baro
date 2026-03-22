#!/usr/bin/env node
import React from "react"
import { render, Box } from "ink"
import { App, type PlannerMode } from "./App.js"

// Parse args
const args = process.argv.slice(2)
let plannerMode: PlannerMode = "claude" // default: use claude CLI, no API key needed

for (let i = 0; i < args.length; i++) {
    if (args[i] === "--planner" && args[i + 1]) {
        const val = args[i + 1].toLowerCase()
        if (val === "openai" || val === "gpt" || val.startsWith("gpt-")) {
            plannerMode = "openai"
        }
        i++
    }
    if (args[i] === "--help" || args[i] === "-h") {
        console.log(`
  baro - autonomous parallel coding

  Usage:
    baro                        Plan with Claude Code, execute with Claude Code
    baro --planner openai       Plan with GPT-5.4 (needs OPENAI_API_KEY)

  The default mode requires only Claude Code CLI installed.
  No API keys needed.
`)
        process.exit(0)
    }
}

process.stdout.write("\x1b[2J\x1b[H")

render(
    <Box width="100%" flexDirection="column" alignItems="flex-start">
        <App plannerMode={plannerMode} />
    </Box>
)
