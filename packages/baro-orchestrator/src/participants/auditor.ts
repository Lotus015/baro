/**
 * Auditor — passive observer that persists every ContextItem to a JSONL
 * file. Used for replay, post-mortem debugging, and (later) `--resume`
 * by replaying the log into a fresh environment.
 *
 * Library-grade: knows nothing about specific agent types or domains.
 */

import { appendFileSync, mkdirSync } from "fs"
import { dirname } from "path"

import { ContextItem, Participant } from "@mozaik-ai/core"

import { ClaudeStreamChunkItem } from "../types.js"

export interface AuditorOptions {
    /** Path to the JSONL log file. Parent directories are created if needed. */
    path: string
    /**
     * If true, ClaudeStreamChunkItem events are skipped. Default: true,
     * because partial-message chunks dominate volume and rarely add audit
     * value.
     */
    skipStreamChunks?: boolean
    /**
     * Optional custom filter. If provided, an item is written iff this
     * returns true. Runs after `skipStreamChunks`.
     */
    filter?: (source: Participant, item: ContextItem) => boolean
}

export class Auditor extends Participant {
    private readonly path: string
    private readonly skipStreamChunks: boolean
    private readonly filter?: (source: Participant, item: ContextItem) => boolean

    constructor(opts: AuditorOptions) {
        super()
        this.path = opts.path
        this.skipStreamChunks = opts.skipStreamChunks ?? true
        this.filter = opts.filter
        mkdirSync(dirname(this.path), { recursive: true })
    }

    async onContextItem(source: Participant, item: ContextItem): Promise<void> {
        if (this.skipStreamChunks && item instanceof ClaudeStreamChunkItem) {
            return
        }
        if (this.filter && !this.filter(source, item)) {
            return
        }
        const entry = {
            ts: new Date().toISOString(),
            source: this.sourceLabel(source),
            item: item.toJSON(),
        }
        appendFileSync(this.path, JSON.stringify(entry) + "\n")
    }

    private sourceLabel(source: Participant): string {
        const ctor = source.constructor.name
        const idCandidate = (source as unknown as { agentId?: string }).agentId
        return typeof idCandidate === "string" ? `${ctor}:${idCandidate}` : ctor
    }
}
