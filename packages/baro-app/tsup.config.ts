import { defineConfig } from "tsup"

/**
 * Two bundles ship inside the npm `baro-ai` package's `dist/` directory:
 *
 *   1. `openai-planner.js` — the existing standalone planner subprocess.
 *   2. `cli.mjs` — the Mozaik orchestrator CLI bundled with all its TS
 *      sources (including the @mozaik-ai/core framework). Spawned by
 *      crates/baro-tui/src/orchestrator_client.rs after a user runs
 *      `npm install -g baro-ai`. The Rust client looks for it at
 *      `node_modules/baro-ai/dist/cli.mjs`.
 *
 * The orchestrator entry point lives in the sibling workspace package
 * `@baro/orchestrator`. tsup follows the imports across the workspace
 * boundary, bundles everything (including @mozaik-ai/core) into a
 * single ESM file with a node shebang.
 */
export default defineConfig([
    {
        entry: { "openai-planner": "src/core/openai-planner.ts" },
        format: ["esm"],
        outDir: "dist",
        clean: false,
        sourcemap: true,
    },
    {
        entry: { cli: "../baro-orchestrator/scripts/cli.ts" },
        format: ["esm"],
        outDir: "dist",
        outExtension: () => ({ js: ".mjs" }),
        target: "node20",
        platform: "node",
        bundle: true,
        // Force the Mozaik framework + the workspace orchestrator code
        // *into* the bundle so the published package is self-contained
        // (no runtime dependency on @mozaik-ai/core or @baro/orchestrator).
        noExternal: [
            /^@mozaik-ai\//,
            /^@baro\//,
        ],
        clean: false,
        sourcemap: true,
        banner: { js: "#!/usr/bin/env node" },
    },
])
