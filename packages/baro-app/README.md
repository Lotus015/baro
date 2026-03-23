# baro

Background agent runtime orchestrator. Breaks down a goal into stories, builds a dependency DAG, and runs them in parallel — each story gets its own Claude agent.

```
npm install -g baro-ai
```

## How it works

1. You describe a goal
2. Claude explores your codebase and plans the work as a dependency graph
3. You review and accept the plan
4. Stories execute in parallel with live TUI dashboard
5. Each story auto-commits and pushes on completion

## Usage

```bash
# Interactive - opens welcome screen
baro

# Direct - skip to planning
baro "Add authentication with JWT and role-based access control"

# Use OpenAI for planning instead of Claude
baro --planner openai "Add WebSocket support"

# Specify working directory
baro --cwd ~/projects/myapp "Add unit tests"
```

## Features

- **Parallel execution** — independent stories run simultaneously, respecting dependency order
- **DAG engine** — topological sort with level grouping, cycle detection
- **Live TUI** — dashboard with story status, live agent logs, DAG view, stats
- **Git coordination** — mutex-protected commits, auto-push with retry, pull --rebase before each story, conflict detection
- **Retry logic** — failed stories retry automatically (configurable per story)
- **Completion screen** — summary overlay with stats when all stories finish
- **Claude + OpenAI** — Claude as default planner/executor, OpenAI as alternative planner

## Requirements

- [Claude CLI](https://docs.anthropic.com/en/docs/claude-cli) installed and authenticated
- macOS (arm64/x64) or Linux (x64/arm64)
- Node.js 18+ (only if using `--planner openai`)

## Architecture

Rust binary distributed via npm. TUI built with ratatui, async execution with tokio, one Claude CLI process per story.

## License

MIT
