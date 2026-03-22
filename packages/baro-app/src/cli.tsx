#!/usr/bin/env node
import React from "react"
import { render, Box } from "ink"
import { App } from "./App.js"

// Clear screen before starting
process.stdout.write("\x1b[2J\x1b[H")

render(
    <Box width="100%" flexDirection="column" alignItems="flex-start">
        <App />
    </Box>
)
