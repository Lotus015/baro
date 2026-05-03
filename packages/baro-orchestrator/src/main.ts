/**
 * @baro/orchestrator — TypeScript Mozaik orchestrator that replaces baro's
 * Rust executor. This module is the public entry point of the package.
 *
 * Phase 1 milestone A: exports the building blocks needed to run a single
 * story end-to-end (ClaudeCliParticipant, Auditor, Cartographer, custom
 * ContextItem types, DAG helpers). Conductor / TUI bridge / Operator
 * land in milestone B.
 */

export {
    AgentTargetedMessageItem,
    AgentStateItem,
    type AgentPhase,
    ClaudeSystemItem,
    ClaudeResultItem,
    ClaudeStreamChunkItem,
    ClaudeRateLimitItem,
    ClaudeUnknownEventItem,
} from "./types.js"

export { mapClaudeEvent, type MapResult } from "./stream-json-mapper.js"

export {
    ClaudeCliParticipant,
    type ClaudeCliParticipantOptions,
    type ClaudeRunSummary,
} from "./participants/claude-cli-participant.js"

export { Auditor, type AuditorOptions } from "./participants/auditor.js"

export {
    Cartographer,
    type CartographerOptions,
    type Frame,
} from "./participants/cartographer.js"

export {
    StoryAgent,
    StoryResultItem,
    type StorySpec,
    type StoryOutcome,
} from "./participants/story-agent.js"

export {
    Conductor,
    ConductorStateItem,
    type ConductorOptions,
    type ConductorRunSummary,
} from "./participants/conductor.js"

export {
    type PrdFile,
    type PrdStory,
    loadPrd,
    savePrd,
    normalizePrd,
    markStoryPassed,
    buildDefaultStoryPrompt,
} from "./prd.js"

export {
    buildDag,
    type DagNode,
    type DagLevel,
    type BuildOptions as DagBuildOptions,
} from "./dag.js"
