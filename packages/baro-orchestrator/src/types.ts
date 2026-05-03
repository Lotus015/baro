/**
 * Canonical custom ContextItem types used by the orchestrator and its
 * participants. Designed to be library-grade — none of them know about
 * baro PRD format or any product-specific concept. They describe agents
 * and Claude CLI events at the domain level, suitable for promotion to a
 * Mozaik library package later.
 */

import { ContextItem } from "@mozaik-ai/core"

// ─── Bus routing ────────────────────────────────────────────────────

/**
 * A user-facing text message addressed to a specific agent in the
 * environment. Other agents see it on the bus but ignore it.
 *
 * This is the canonical "tell agent X to do something" message — emitted by
 * Operator (human input), Conductor (initial story prompt), Critic
 * (review feedback), Surgeon (replan directive), Librarian (knowledge
 * injection), etc.
 */
export class AgentTargetedMessageItem extends ContextItem {
    readonly type = "agent_targeted_message"

    constructor(
        public readonly recipientId: string,
        public readonly text: string,
        public readonly metadata: Readonly<Record<string, unknown>> = {},
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            recipientId: this.recipientId,
            text: this.text,
            metadata: this.metadata,
        }
    }
}

// ─── Agent lifecycle ────────────────────────────────────────────────

export type AgentPhase =
    | "idle"
    | "starting"
    | "running"
    | "waiting"
    | "done"
    | "failed"
    | "aborted"

/**
 * Heartbeat / state-change signal for an agent. Observers (Cartographer,
 * Auditor, Throttler) read these to track who's doing what.
 */
export class AgentStateItem extends ContextItem {
    readonly type = "agent_state"

    constructor(
        public readonly agentId: string,
        public readonly phase: AgentPhase,
        public readonly detail?: string,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            phase: this.phase,
            detail: this.detail,
        }
    }
}

// ─── Claude CLI passthrough types ───────────────────────────────────
//
// These wrap Claude stream-json events that don't map cleanly onto Mozaik's
// built-in ContextItem types (UserMessageItem, ModelMessageItem, etc).
// They're intentionally close to the wire format so observers can do
// detailed inspection, while the mapper still emits typed Mozaik items
// alongside for the events that DO map.

/**
 * Claude `system:*` events — init, status, task_started, task_notification,
 * etc. These describe the Claude session lifecycle, not its content.
 */
export class ClaudeSystemItem extends ContextItem {
    readonly type = "claude_system"

    constructor(
        public readonly agentId: string,
        public readonly subtype: string,
        public readonly raw: Readonly<Record<string, unknown>>,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            subtype: this.subtype,
            raw: this.raw,
        }
    }
}

/**
 * Claude `result` event — emitted once at the end of a turn. Carries
 * session_id (for `--resume`), usage, cost, num_turns, duration. This is
 * the single richest event in the stream and most observers care about it.
 */
export class ClaudeResultItem extends ContextItem {
    readonly type = "claude_result"

    constructor(
        public readonly agentId: string,
        public readonly subtype: string,
        public readonly sessionId: string | null,
        public readonly isError: boolean,
        public readonly resultText: string | null,
        public readonly usage: Readonly<Record<string, unknown>> | null,
        public readonly totalCostUsd: number | null,
        public readonly numTurns: number | null,
        public readonly durationMs: number | null,
        public readonly raw: Readonly<Record<string, unknown>>,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            subtype: this.subtype,
            sessionId: this.sessionId,
            isError: this.isError,
            resultText: this.resultText,
            usage: this.usage,
            totalCostUsd: this.totalCostUsd,
            numTurns: this.numTurns,
            durationMs: this.durationMs,
        }
    }
}

/**
 * Claude `stream_event` — partial token chunks. High volume (~80% of
 * events when --include-partial-messages is on). Most observers should
 * filter these out unless they specifically render a streaming UI.
 */
export class ClaudeStreamChunkItem extends ContextItem {
    readonly type = "claude_stream_chunk"

    constructor(
        public readonly agentId: string,
        public readonly raw: Readonly<Record<string, unknown>>,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            raw: this.raw,
        }
    }
}

/**
 * Claude `rate_limit_event` — informational throttling notice from the
 * Claude API. Throttler participant uses this to back off.
 */
export class ClaudeRateLimitItem extends ContextItem {
    readonly type = "claude_rate_limit"

    constructor(
        public readonly agentId: string,
        public readonly raw: Readonly<Record<string, unknown>>,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            raw: this.raw,
        }
    }
}

/**
 * Fallback for any Claude stream-json event whose `type` we don't yet
 * recognize. Lets us forward-compatibly carry events without dropping
 * them; observers can still inspect them.
 */
export class ClaudeUnknownEventItem extends ContextItem {
    readonly type = "claude_unknown_event"

    constructor(
        public readonly agentId: string,
        public readonly claudeType: string,
        public readonly raw: Readonly<Record<string, unknown>>,
    ) {
        super()
    }

    toJSON(): unknown {
        return {
            type: this.type,
            agentId: this.agentId,
            claudeType: this.claudeType,
            raw: this.raw,
        }
    }
}
