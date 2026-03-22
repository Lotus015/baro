# baro

Autonomous parallel coding engine. Give it a goal, it breaks it into stories, builds a dependency DAG, and executes them in parallel using AI agents.

```
npm install -g baro-ai
```

## How it works

1. **You describe a goal** - "Add authentication with JWT and role-based access control"
2. **AI plans the work** - Claude (or OpenAI) explores your codebase and creates a dependency graph of user stories
3. **You review the plan** - Scrollable plan review with accept/refine/quit
4. **Stories execute in parallel** - Independent stories run simultaneously, each with its own Claude agent
5. **Live TUI dashboard** - Watch progress, logs, DAG visualization, and stats in real-time

## Usage

### Interactive mode

```bash
baro
```

Opens the welcome screen where you type your goal and choose a planner.

### Direct mode

```bash
baro "Add a REST API for user management"
```

Skips the welcome screen and starts planning immediately.

### Options

```
baro [goal] [options]

Arguments:
  goal                    Project goal (opens welcome screen if omitted)

Options:
  --planner <planner>     Planner to use: claude or openai (default: claude)
  --cwd <path>            Working directory (default: current directory)
  -h, --help              Print help
```

### Examples

```bash
# Interactive - opens welcome screen
baro

# Plan and execute with Claude (default)
baro "Refactor the database layer to use connection pooling"

# Use OpenAI for planning
baro --planner openai "Add WebSocket support"

# Run in a specific directory
baro --cwd ~/projects/myapp "Add unit tests for all API endpoints"
```

## TUI Screens

### Welcome

ASCII art logo, goal text input, and planner toggle (Claude/OpenAI).

### Planning

Animated spinner showing planning progress with elapsed timer. The selected AI explores your codebase and generates a structured plan.

### Review

Scrollable list of all planned stories with descriptions and dependencies. Navigate with arrow keys, accept with Enter, or quit with q.

### Execution Dashboard

Three tabs while stories execute:

- **Dashboard** - Story list with status icons + live logs from the active agent
- **DAG** - Dependency graph visualization showing levels and connections
- **Stats** - Summary table with times, file counts, and completion stats

Keybinds: `1/2/3` switch tabs, `Tab/Shift+Tab` switch log panels, `q` quit.

## Requirements

- [Claude CLI](https://docs.anthropic.com/en/docs/claude-cli) installed and authenticated (for Claude planner/executor)
- Node.js 18+ (only needed if using `--planner openai`)
- macOS (arm64/x64) or Linux (x64/arm64)

## Architecture

Baro is a Rust binary distributed via npm:

- **TUI** - ratatui-based terminal UI with 4 screens
- **Planner** - Spawns Claude CLI or OpenAI (via Node.js bridge) to generate a PRD
- **DAG Engine** - Kahn's algorithm for topological sort with level grouping
- **Executor** - Parallel story execution via tokio, one Claude agent per story
- **npm package** - `postinstall` downloads the prebuilt binary for your platform

## Development

```bash
# Build the Rust binary
cargo build -p baro-tui --release

# Run locally
./target/release/baro "your goal"

# The binary is in crates/baro-tui/
```

## License

MIT
