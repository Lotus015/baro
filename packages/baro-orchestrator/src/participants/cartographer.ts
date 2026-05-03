/**
 * Cartographer — observer that translates Mozaik bus events into a
 * coarser-grained "frame stream" suitable for downstream UI rendering.
 *
 * In Phase 1 the consumer is the Rust TUI (line-delimited JSON over
 * stdout). The same Cartographer will later serve a web UI by
 * subscribing a different sink. The protocol is one event object per
 * call to `sink`, no buffering.
 *
 * Library-grade: emits semantic frames (state changes, tool calls,
 * messages, results) regardless of who produced them.
 */

import {
    ContextItem,
    FunctionCallItem,
    FunctionCallOutputItem,
    ModelMessageItem,
    Participant,
    UserMessageItem,
} from "@mozaik-ai/core"

import {
    AgentStateItem,
    AgentTargetedMessageItem,
    ClaudeRateLimitItem,
    ClaudeResultItem,
    ClaudeStreamChunkItem,
    ClaudeSystemItem,
    ClaudeUnknownEventItem,
} from "../types.js"

export type Frame =
    | { kind: "agent_state"; agentId: string; phase: string; detail?: string }
    | { kind: "user_message"; agentId: string | null; text: string }
    | { kind: "model_message"; agentId: string | null; text: string }
    | { kind: "tool_call"; agentId: string | null; callId: string; name: string; args: string }
    | { kind: "tool_result"; agentId: string | null; callId: string; output: string }
    | { kind: "result"; agentId: string; isError: boolean; text: string | null; durationMs: number | null; costUsd: number | null; numTurns: number | null; sessionId: string | null }
    | { kind: "rate_limit"; agentId: string; raw: unknown }
    | { kind: "system"; agentId: string; subtype: string }
    | { kind: "stream_chunk"; agentId: string }
    | { kind: "unknown"; sourceLabel: string; itemType: string }

export interface CartographerOptions {
    /** Where each frame is written. */
    sink: (frame: Frame) => void
    /**
     * Whether to emit `stream_chunk` frames. Default: false (high volume,
     * mainly useful for live token-streaming UIs).
     */
    emitStreamChunks?: boolean
}

export class Cartographer extends Participant {
    private readonly sink: (frame: Frame) => void
    private readonly emitStreamChunks: boolean

    constructor(opts: CartographerOptions) {
        super()
        this.sink = opts.sink
        this.emitStreamChunks = opts.emitStreamChunks ?? false
    }

    async onContextItem(source: Participant, item: ContextItem): Promise<void> {
        const agentId = this.extractAgentId(source, item)

        if (item instanceof AgentStateItem) {
            this.sink({
                kind: "agent_state",
                agentId: item.agentId,
                phase: item.phase,
                detail: item.detail,
            })
            return
        }

        if (item instanceof UserMessageItem) {
            const json = item.toJSON() as { content: Array<{ text: string }> }
            const text = json.content?.[0]?.text ?? ""
            this.sink({ kind: "user_message", agentId, text })
            return
        }

        if (item instanceof AgentTargetedMessageItem) {
            this.sink({
                kind: "user_message",
                agentId: item.recipientId,
                text: item.text,
            })
            return
        }

        if (item instanceof ModelMessageItem) {
            const json = item.toJSON() as { content: Array<{ text: string }> }
            const text = json.content?.[0]?.text ?? ""
            this.sink({ kind: "model_message", agentId, text })
            return
        }

        if (item instanceof FunctionCallItem) {
            this.sink({
                kind: "tool_call",
                agentId,
                callId: item.callId,
                name: item.name,
                args: item.args,
            })
            return
        }

        if (item instanceof FunctionCallOutputItem) {
            const json = item.toJSON() as { call_id: string; output: Array<{ text: string }> }
            const output = json.output?.[0]?.text ?? ""
            this.sink({ kind: "tool_result", agentId, callId: json.call_id, output })
            return
        }

        if (item instanceof ClaudeResultItem) {
            this.sink({
                kind: "result",
                agentId: item.agentId,
                isError: item.isError,
                text: item.resultText,
                durationMs: item.durationMs,
                costUsd: item.totalCostUsd,
                numTurns: item.numTurns,
                sessionId: item.sessionId,
            })
            return
        }

        if (item instanceof ClaudeRateLimitItem) {
            this.sink({ kind: "rate_limit", agentId: item.agentId, raw: item.raw })
            return
        }

        if (item instanceof ClaudeSystemItem) {
            this.sink({
                kind: "system",
                agentId: item.agentId,
                subtype: item.subtype,
            })
            return
        }

        if (item instanceof ClaudeStreamChunkItem) {
            if (this.emitStreamChunks) {
                this.sink({ kind: "stream_chunk", agentId: item.agentId })
            }
            return
        }

        if (item instanceof ClaudeUnknownEventItem) {
            this.sink({
                kind: "unknown",
                sourceLabel: source.constructor.name,
                itemType: item.claudeType,
            })
            return
        }

        // Anything else falls through into an "unknown" frame so the sink
        // sees that something happened.
        this.sink({
            kind: "unknown",
            sourceLabel: source.constructor.name,
            itemType: (item as { type?: string }).type ?? "unspecified",
        })
    }

    private extractAgentId(source: Participant, item: ContextItem): string | null {
        const fromItem = (item as unknown as { agentId?: string }).agentId
        if (typeof fromItem === "string") return fromItem
        const fromSource = (source as unknown as { agentId?: string }).agentId
        return typeof fromSource === "string" ? fromSource : null
    }
}
