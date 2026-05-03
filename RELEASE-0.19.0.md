# baro-ai 0.19.0 — Mozaik orchestrator release

**Branch:** `mozaik-rework` (pushed to `origin`)

This is the first release that runs every story through the TypeScript
Mozaik orchestrator instead of the in-process Rust executor. The old
executor is gone; the orchestrator is bundled into the npm package as
`dist/cli.mjs`.

## What changed

- **New baro CLI flags:**
  `--with-critic`, `--critic-model`, `--no-librarian`, `--no-sentry`,
  `--with-surgeon`, `--surgeon-use-llm`, `--surgeon-model`. See README.
- **Architecture:** every `baro` invocation now spawns
  `node ~/.baro/bin/cli.mjs` (the bundled Mozaik orchestrator); the
  orchestrator owns story execution end-to-end and emits typed events
  on a shared bus where Librarian / Sentry / Critic / Surgeon
  participants observe and react.
- **Defaults:** Phase 2 observers (Librarian + Sentry) are ON by
  default. Phase 3 (Critic) and Phase 4 (Surgeon) are opt-in.
- **Auth:** every model call goes through the `claude` CLI subprocess,
  so users keep using whatever auth Claude CLI is configured with
  (OAuth session, Bedrock, Vertex). No `ANTHROPIC_API_KEY` required.
- **Rust binary:** ~2400 lines of dead orchestration code excised
  (`executor.rs`, `claude_runner.rs`'s streaming half, `git.rs` mutex
  helpers, `dag.rs`). `cargo build` is clean (0 warnings).
- **Postinstall** now also copies `dist/cli.mjs` to `~/.baro/bin/cli.mjs`
  alongside the Rust binary so the orchestrator is found from any cwd.

## Publish steps

baro-ai's existing release flow assumes:

1. **GitHub Release** at `v<version>` with prebuilt Rust binaries for
   each platform (`baro-darwin-arm64`, `baro-darwin-x64`,
   `baro-linux-x64`, `baro-linux-arm64`, `baro-windows-x64.exe`).
   `scripts/postinstall.js` downloads from
   `https://github.com/Lotus015/baro/releases/download/v0.19.0/baro-<platform>`.
2. **npm publish** of the `baro-ai` package, which ships the bundled
   `dist/cli.mjs` orchestrator alongside `bin/baro` and `postinstall.js`.

Order: cut the GitHub release first (so postinstall has something to
download), then `npm publish`.

### 1. Build Rust binaries for all platforms

If you have a CI workflow or local cross-compile setup, run it. Manual
local build of just this machine's binary:

```
cargo build --release --bin baro
# binary at target/release/baro
```

Cross-compile with `cross`:

```
cross build --release --bin baro --target x86_64-unknown-linux-gnu
cross build --release --bin baro --target aarch64-apple-darwin
cross build --release --bin baro --target x86_64-apple-darwin
cross build --release --bin baro --target aarch64-unknown-linux-gnu
cross build --release --bin baro --target x86_64-pc-windows-gnu
```

Rename each output to match the postinstall expectation
(`baro-<platform>` or `baro-<platform>.exe`).

### 2. Cut a GitHub Release

```
gh release create v0.19.0 \
    --title "baro 0.19.0 — Mozaik orchestrator" \
    --notes-file RELEASE-0.19.0.md \
    target/release/baro-darwin-arm64 \
    target/release/baro-darwin-x64 \
    target/release/baro-linux-x64 \
    target/release/baro-linux-arm64 \
    target/release/baro-windows-x64.exe
```

### 3. Build the npm package

```
cd packages/baro-app
rm -rf dist
npm run build
npm pack --dry-run   # sanity-check what's about to publish
```

You should see ~10 files including `dist/cli.mjs` (~340 KB) and
`dist/openai-planner.js`.

### 4. Publish to npm

```
cd packages/baro-app
npm publish --access public
```

### 5. Smoke-test the published package

In a fresh project:

```
mkdir -p /tmp/baro-test && cd /tmp/baro-test
git init && git commit --allow-empty -m initial
npm install -g baro-ai@0.19.0
# postinstall runs: downloads binary, copies cli.mjs to ~/.baro/bin/

baro --help                            # should show new flags
baro --dry-run "create a HELLO.md file with one line"
ls ~/.baro/bin/                         # should contain `baro` AND `cli.mjs`
```

Then a real run:

```
baro --with-critic "create a HELLO.md file with one line saying hi"
```

Watch the TUI — Librarian / Sentry / Critic frames should appear in the
audit log under `~/.baro/audit/` (when configured) or in `prd.json`.

## Rollback

If post-publish testing surfaces a regression:

```
npm deprecate baro-ai@0.19.0 "regression in <module> — see issue #N"
```

Users on 0.18.1 are unaffected. Drafting fixes on `mozaik-rework` and
publishing 0.19.1 should be straightforward.
