/**
 * Direct OpenAI streaming - bypasses mozaik for real-time token output.
 * Used during planning to show the user what the model is thinking.
 */

export interface StreamOptions {
    model: string
    messages: { role: string; content: string }[]
    task: string
    onToken: (token: string) => void
    onToolCall?: (name: string, args: string) => void
    jsonSchema?: Record<string, any>
}

export async function streamCompletion(opts: StreamOptions): Promise<string> {
    const apiKey = process.env.OPENAI_API_KEY
    if (!apiKey) {
        throw new Error("OPENAI_API_KEY not set")
    }

    const body: any = {
        model: opts.model,
        input: [
            ...opts.messages.map((m) => ({
                role: m.role,
                content: m.content,
            })),
            { role: "user", content: opts.task },
        ],
        stream: true,
    }

    // If structured output requested, use text format with json instruction
    if (opts.jsonSchema) {
        body.text = {
            format: {
                type: "json_schema",
                name: "prd_output",
                schema: opts.jsonSchema,
                strict: true,
            },
        }
    }

    const response = await fetch("https://api.openai.com/v1/responses", {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${apiKey}`,
        },
        body: JSON.stringify(body),
    })

    if (!response.ok) {
        const errText = await response.text()
        throw new Error(`OpenAI API error ${response.status}: ${errText}`)
    }

    if (!response.body) {
        throw new Error("No response body")
    }

    let fullText = ""
    const reader = response.body.getReader()
    const decoder = new TextDecoder()

    let buffer = ""

    while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })

        // Parse SSE events
        const lines = buffer.split("\n")
        buffer = lines.pop() ?? "" // keep incomplete line

        for (const line of lines) {
            if (!line.startsWith("data: ")) continue
            const data = line.slice(6).trim()
            if (data === "[DONE]") continue

            try {
                const event = JSON.parse(data)

                // Handle different event types from Responses API
                if (event.type === "response.output_text.delta") {
                    const delta = event.delta ?? ""
                    fullText += delta
                    opts.onToken(delta)
                } else if (event.type === "response.content.delta") {
                    const delta = event.delta ?? ""
                    fullText += delta
                    opts.onToken(delta)
                } else if (event.type === "response.function_call_arguments.delta") {
                    // Tool call streaming
                } else if (event.type === "response.output_item.added") {
                    if (event.item?.type === "function_call" && opts.onToolCall) {
                        opts.onToolCall(event.item.name, "")
                    }
                }
            } catch {
                // skip unparseable lines
            }
        }
    }

    return fullText
}
