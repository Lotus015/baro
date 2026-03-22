/**
 * OpenAI Responses API streaming with tool calling loop.
 * Model can call tools (read_file, grep, list_files) to explore the codebase,
 * then generates the final output (plan JSON).
 */

export interface ToolDef {
    name: string
    description: string
    parameters: Record<string, any>
    invoke: (args: any) => Promise<string>
}

export interface StreamOptions {
    model: string
    messages: { role: string; content: string }[]
    task: string
    onToken: (token: string) => void
    onToolCall?: (name: string, args: any) => void
    onThinking?: (text: string) => void
    jsonSchema?: Record<string, any>
    reasoning?: { effort: "low" | "medium" | "high" }
    tools?: ToolDef[]
}

const API_URL = "https://api.openai.com/v1/responses"
const MAX_TOOL_ROUNDS = 15

export async function streamCompletion(opts: StreamOptions): Promise<string> {
    const apiKey = process.env.OPENAI_API_KEY
    if (!apiKey) throw new Error("OPENAI_API_KEY not set")

    // Build initial input
    let input: any[] = [
        ...opts.messages.map((m) => ({ role: m.role, content: m.content })),
        { role: "user", content: opts.task },
    ]

    // Tool calling loop - model may request tools multiple times before final output
    for (let round = 0; round < MAX_TOOL_ROUNDS; round++) {
        const body: any = { model: opts.model, input, stream: true }

        if (opts.reasoning) body.reasoning = opts.reasoning
        if (opts.jsonSchema) {
            body.text = {
                format: { type: "json_schema", name: "prd_output", schema: opts.jsonSchema, strict: true },
            }
        }
        if (opts.tools?.length) {
            body.tools = opts.tools.map((t) => ({
                type: "function",
                name: t.name,
                description: t.description,
                parameters: t.parameters,
            }))
        }

        const response = await fetch(API_URL, {
            method: "POST",
            headers: { "Content-Type": "application/json", Authorization: `Bearer ${apiKey}` },
            body: JSON.stringify(body),
        })

        if (!response.ok) {
            const errText = await response.text()
            throw new Error(`OpenAI API error ${response.status}: ${errText}`)
        }

        if (!response.body) throw new Error("No response body")

        // Parse SSE stream
        const { textOutput, toolCalls, responseId } = await parseSSE(response.body, opts)

        // If no tool calls, we have our final text output
        if (toolCalls.length === 0) {
            return textOutput
        }

        // Execute tool calls and feed results back
        const toolResults: any[] = []
        for (const tc of toolCalls) {
            const toolDef = opts.tools?.find((t) => t.name === tc.name)
            if (!toolDef) continue

            opts.onToolCall?.(tc.name, tc.args)

            let result: string
            try {
                const parsed = JSON.parse(tc.args)
                result = await toolDef.invoke(parsed)
            } catch (err: any) {
                result = `Error: ${err.message}`
            }

            toolResults.push({
                type: "function_call_output",
                call_id: tc.callId,
                output: typeof result === "string" ? result : JSON.stringify(result),
            })
        }

        // Next round: just tool results as input (API remembers conversation via response ID)
        input = toolResults
    }

    throw new Error("Too many tool calling rounds")
}

interface ToolCall {
    callId: string
    name: string
    args: string
}

async function parseSSE(
    body: ReadableStream<Uint8Array>,
    opts: StreamOptions
): Promise<{ textOutput: string; toolCalls: ToolCall[]; responseId: string }> {
    let textOutput = ""
    const toolCalls: ToolCall[] = []
    const toolCallArgs = new Map<number, { callId: string; name: string; args: string }>()
    let responseId = ""

    const reader = body.getReader()
    const decoder = new TextDecoder()
    let buffer = ""

    while (true) {
        const { done, value } = await reader.read()
        if (done) break

        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split("\n")
        buffer = lines.pop() ?? ""

        for (const line of lines) {
            if (!line.startsWith("data: ")) continue
            const data = line.slice(6).trim()
            if (data === "[DONE]") continue

            try {
                const ev = JSON.parse(data)

                if (ev.type === "response.created") {
                    responseId = ev.response?.id ?? ""
                } else if (ev.type === "response.output_text.delta") {
                    textOutput += ev.delta ?? ""
                    opts.onToken(ev.delta ?? "")
                } else if (ev.type === "response.reasoning.delta") {
                    opts.onThinking?.(ev.delta ?? "")
                } else if (ev.type === "response.output_item.added") {
                    if (ev.item?.type === "function_call") {
                        toolCallArgs.set(ev.output_index, {
                            callId: ev.item.call_id ?? "",
                            name: ev.item.name ?? "",
                            args: "",
                        })
                    }
                } else if (ev.type === "response.function_call_arguments.delta") {
                    const tc = toolCallArgs.get(ev.output_index)
                    if (tc) tc.args += ev.delta ?? ""
                } else if (ev.type === "response.output_item.done") {
                    if (ev.item?.type === "function_call") {
                        const tc = toolCallArgs.get(ev.output_index)
                        if (tc) {
                            toolCalls.push({ callId: tc.callId, name: tc.name, args: tc.args })
                        }
                    }
                }
            } catch {}
        }
    }

    return { textOutput, toolCalls, responseId }
}
