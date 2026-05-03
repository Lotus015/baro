/**
 * Operator — bridge from external user-input commands (Rust TUI today,
 * web UI later) to the Mozaik bus. The Operator joins the environment
 * but only emits on demand: callers invoke `dispatch(cmd)` when a
 * command arrives over the TUI protocol, and Operator translates it
 * into bus events.
 *
 * Library-grade: knows nothing about TUI specifics, only about the
 * canonical command shape and how to express it as ContextItems.
 */

import {
    AgenticEnvironment,
    ContextItem,
    Participant,
} from "@mozaik-ai/core"

import { AgentTargetedMessageItem } from "../types.js"

export type OperatorCommand =
    | { kind: "redirect"; storyId: string; message: string }
    | { kind: "abort"; storyId: string }
    | { kind: "abort_all" }
    | { kind: "shutdown" }

export interface OperatorHooks {
    /** Called when an `abort` command arrives. */
    onAbort?: (storyId: string) => void
    /** Called when `abort_all` arrives. */
    onAbortAll?: () => void
    /** Called when `shutdown` arrives. */
    onShutdown?: () => void
}

export class Operator extends Participant {
    private envRef: AgenticEnvironment | null = null

    constructor(private readonly hooks: OperatorHooks = {}) {
        super()
    }

    setEnvironment(env: AgenticEnvironment): void {
        this.envRef = env
    }

    async onContextItem(): Promise<void> {
        // Operator is push-only: it emits in response to external
        // commands, never reacts to bus events.
    }

    /** Translate an external command into bus action / hook callback. */
    dispatch(cmd: OperatorCommand): void {
        switch (cmd.kind) {
            case "redirect": {
                this.emit(
                    new AgentTargetedMessageItem(cmd.storyId, cmd.message, {
                        source: "operator",
                    }),
                )
                return
            }
            case "abort": {
                this.hooks.onAbort?.(cmd.storyId)
                return
            }
            case "abort_all": {
                this.hooks.onAbortAll?.()
                return
            }
            case "shutdown": {
                this.hooks.onShutdown?.()
                return
            }
        }
    }

    private emit(item: ContextItem): void {
        this.envRef?.deliverContextItem(this, item)
    }
}
