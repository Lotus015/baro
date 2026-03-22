#!/usr/bin/env node
/**
 * OpenAI planner entry point - called from Rust binary when --planner openai.
 * Usage: node openai-planner.js "goal" --cwd /path/to/project
 * Outputs PRD JSON to stdout.
 */

import { Planner } from "./planner.js"

const args = process.argv.slice(2)
let goal = ""
let cwd = process.cwd()

for (let i = 0; i < args.length; i++) {
    if (args[i] === "--cwd" && args[i + 1]) {
        cwd = args[++i]
    } else if (!goal) {
        goal = args[i]
    }
}

if (!goal) {
    console.error("Usage: openai-planner <goal> [--cwd <path>]")
    process.exit(1)
}

async function main() {
    const planner = new Planner({
        cwd,
        onToken: () => {},
        onToolCall: (name, args) => {
            process.stderr.write(`[openai] tool: ${name}\n`)
        },
    })

    try {
        const prd = await planner.send(goal)
        process.stdout.write(JSON.stringify(prd, null, 2) + "\n")
    } catch (err: any) {
        console.error(`OpenAI planner error: ${err.message}`)
        process.exit(1)
    }
}

main()
