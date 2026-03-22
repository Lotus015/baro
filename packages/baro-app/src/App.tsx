import React, { useState } from "react"
import { Box, Text, useApp, useInput } from "ink"
import { ApiKeyScreen } from "./screens/ApiKeyScreen.js"
import { PlanScreen } from "./screens/PlanScreen.js"
import { ReviewScreen } from "./screens/ReviewScreen.js"
import { ExecuteScreen } from "./screens/ExecuteScreen.js"
import type { PrdV2 } from "./core/prd.js"

type Screen = "apikey" | "plan" | "review" | "execute"

export function App() {
    const app = useApp()
    const [screen, setScreen] = useState<Screen>(() => {
        // Skip API key if already set
        if (process.env.OPENAI_API_KEY || process.env.ANTHROPIC_API_KEY) {
            return "plan"
        }
        // Check ~/.baro/.env
        try {
            const fs = require("fs")
            const home = process.env.HOME ?? ""
            const envPath = `${home}/.baro/.env`
            if (fs.existsSync(envPath)) {
                const content = fs.readFileSync(envPath, "utf-8")
                for (const line of content.split("\n")) {
                    const [key, val] = line.split("=")
                    if (key && val) process.env[key.trim()] = val.trim()
                }
                if (process.env.OPENAI_API_KEY || process.env.ANTHROPIC_API_KEY) {
                    return "plan"
                }
            }
        } catch {}
        return "apikey"
    })
    const [prd, setPrd] = useState<PrdV2 | null>(null)

    useInput((input, key) => {
        if (key.escape && screen === "execute") {
            app.exit()
        }
    })

    if (screen === "apikey") {
        return (
            <ApiKeyScreen
                onComplete={() => setScreen("plan")}
                onQuit={() => app.exit()}
            />
        )
    }

    if (screen === "plan") {
        return (
            <PlanScreen
                onPlanReady={(plan) => {
                    setPrd(plan)
                    setScreen("review")
                }}
                onQuit={() => app.exit()}
            />
        )
    }

    if (screen === "review") {
        return (
            <ReviewScreen
                prd={prd!}
                onAccept={() => setScreen("execute")}
                onRefine={() => setScreen("plan")}
                onQuit={() => app.exit()}
            />
        )
    }

    return (
        <ExecuteScreen
            prd={prd!}
            onDone={() => app.exit()}
        />
    )
}
