# Filament

Local-only multi-agent orchestration, knowledge graph, and task management for software projects.

Filament gives AI coding agents (and humans) a shared project brain — a knowledge graph of tasks, modules, services, plans, and docs with dependency tracking, file reservations, inter-agent messaging, and a real-time TUI dashboard. Everything runs locally with a SQLite database per project.

## Key Features

- **Knowledge graph** — entities (tasks, modules, services, agents, plans, docs) connected by typed relations (blocks, depends_on, produces, owns, relates_to, assigned_to)
- **Task management** — priority-ranked ready queues, critical path analysis, dependency tracking, impact scoring
- **Multi-agent orchestration** — dispatch AI agents with specialized roles (coder, reviewer, planner, dockeeper), monitor runs, handle agent death gracefully
- **Inter-agent messaging** — targeted message passing between agents with typed messages (text, question, blocker, artifact)
- **Advisory file reservations** — TTL-based file locks to coordinate concurrent edits across agents
- **MCP server** — 16-tool Model Context Protocol interface for AI agent integration
- **TUI dashboard** — real-time terminal UI showing tasks, agent runs, and reservations
- **Hybrid architecture** — direct SQLite for single-user, Unix socket daemon for multi-agent

## Installation

### Prerequisites

- **Rust toolchain** — stable channel (edition 2021). Install via [rustup](https://rustup.rs/):
  ```
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **SQLite** — bundled via sqlx (no system install required)
- **macOS or Linux** — uses Unix sockets for daemon mode

### Build from Source

Clone the repository and build the release binary:

```bash
git clone https://github.com/your-org/filament.git
cd filament
make build CRATE=all RELEASE=1
```

The binary is at `target/release/filament`. Copy it to your PATH:

```bash
cp target/release/filament ~/.local/bin/
# or
sudo cp target/release/filament /usr/local/bin/
```

### Development Build

```bash
make build CRATE=all        # debug build
make test CRATE=all          # run all tests
make ci                      # full CI: fmt check + clippy + tests
```

## Setup

<!-- TODO: Document project initialization workflow, .filament/ directory structure,
     daemon configuration, MCP integration with Claude Code / other AI tools,
     and recommended .gitignore additions -->

*Coming soon.*

## Usage

### Initialize a Project

Create a `.filament/` directory with a SQLite database in your project root:

```bash
cd your-project
filament init
```

This creates `.filament/filament.db` and `.filament/content/`. Add `.filament/` to your `.gitignore`.

### Global Flags

Every command supports these flags:

| Flag | Short | Description |
|------|-------|-------------|
| `--json` | | Output JSON instead of human-readable text |
| `--verbose` | `-v` | Increase verbosity (`-v` debug, `-vv` trace) |
| `--quiet` | `-q` | Suppress non-error output |

---

## Entity Management

Filament's core data model is a knowledge graph of **entities** connected by **relations**. Every entity has a unique 8-character slug (e.g., `a3kf92mx`) for human-friendly identification.

### Entity Types

| Type | Purpose |
|------|---------|
| `task` | Work items with priority, status, and dependency tracking |
| `module` | Code modules or components |
| `service` | Services or APIs |
| `agent` | AI or human agents |
| `plan` | Implementation plans |
| `doc` | Documentation, ADRs, reference material |

### Entity Status

All entities have a status: `open`, `in_progress`, `closed`, or `blocked`.

### Add an Entity

```bash
filament add "Authentication Module" --type module --summary "Handles JWT auth" --priority 1

# With key facts (JSON object)
filament add "API Gateway" --type service \
  --summary "Routes requests to microservices" \
  --facts '{"port": 8080, "protocol": "HTTP/2"}'

# With a content file (must exist on disk)
filament add "Architecture Decision" --type doc \
  --summary "ADR-001: Database choice" \
  --content .plan/adr/001-database.md
```

Output: `Created: a3kf92mx (550e8400-e29b-41d4-a716-446655440000)`

### Inspect an Entity

```bash
filament inspect a3kf92mx
```

Shows name, slug, ID, type, status, priority, summary, facts, content path, relations, and timestamps.

### Update an Entity

```bash
filament update a3kf92mx --status in_progress
filament update a3kf92mx --summary "Updated description"
filament update a3kf92mx --status closed --summary "Done"
```

### Read Entity Content

Print the raw content file associated with an entity:

```bash
filament read a3kf92mx
```

### List Entities

```bash
filament list                          # all entities
filament list --type task              # only tasks
filament list --status open            # only open entities
filament list --type module --status in_progress
```

### Remove an Entity

```bash
filament remove a3kf92mx
```

Removes the entity and all its relations.

---

## Relations

Connect entities with typed, directed edges.

### Relation Types

| Type | Meaning |
|------|---------|
| `blocks` | Source blocks target from starting |
| `depends_on` | Source depends on target being done |
| `produces` | Source produces/creates target |
| `owns` | Source owns/contains target |
| `relates_to` | General association |
| `assigned_to` | Agent is assigned to a task |

### Create a Relation

```bash
filament relate abc12345 blocks def67890
filament relate abc12345 depends_on def67890 --summary "needs API first"
filament relate abc12345 owns def67890 --weight 1.0
```

### Remove a Relation

```bash
filament unrelate abc12345 blocks def67890
```

### Explore the Graph

View the neighborhood around an entity (BFS traversal):

```bash
filament context --around abc12345              # default depth 2, limit 20
filament context --around abc12345 --depth 3    # deeper traversal
filament context --around abc12345 --limit 50   # more results
```

---

## Task Management

Tasks are the primary work unit. The `task` subcommand provides specialized task operations beyond basic entity CRUD.

### Add a Task

```bash
filament task add "Implement login endpoint" --summary "POST /auth/login" --priority 0

# With dependency relations
filament task add "Write integration tests" \
  --summary "Test the login flow" \
  --depends-on abc12345 \
  --priority 2

# With blocking relation
filament task add "Deploy to staging" --blocks def67890
```

### List Tasks

```bash
filament task list                     # open tasks (default)
filament task list --status all        # all tasks regardless of status
filament task list --status in_progress
filament task list --status closed
filament task list --unblocked         # only tasks with no blockers
```

Note: `--status` and `--unblocked` are mutually exclusive.

### Show Task Details

```bash
filament task show abc12345
```

### Ready Tasks

Show unblocked tasks ranked by priority and impact score:

```bash
filament task ready               # top 20 by default
filament task ready --limit 5     # top 5 only
```

Impact score counts how many downstream tasks are transitively blocked by each task — higher impact tasks should be done first.

### Critical Path

Show the dependency chain blocking a task:

```bash
filament task critical-path abc12345
```

Output:
```
Critical path (3 steps):
  1. Database schema migration
  2. API endpoint implementation
  3. Frontend integration
```

### Close a Task

```bash
filament task close abc12345
```

### Assign a Task

```bash
filament task assign abc12345 --to agent_slug
```

Creates an `assigned_to` relation from the agent to the task.

---

## Inter-Agent Messaging

Agents communicate through targeted messages. No broadcast — every message has a sender and recipient.

### Message Types

| Type | Purpose |
|------|---------|
| `text` | General communication (default) |
| `question` | Questions needing answers |
| `blocker` | Blocking issues that need resolution |
| `artifact` | Deliverables, code snippets, results |

### Send a Message

```bash
filament message send --from coder-1 --to reviewer-1 \
  --body "Login endpoint ready for review"

filament message send --from reviewer-1 --to coder-1 \
  --body "Missing input validation on email field" \
  --type blocker
```

### Check Inbox

```bash
filament message inbox coder-1
```

Shows unread messages with sender, type, and a preview.

### Mark as Read

```bash
filament message read <message-uuid>
```

Returns an error if the message was already read (distinct from not found).

---

## File Reservations

Advisory file locks to coordinate concurrent edits. Reservations use glob patterns and have a TTL.

### Reserve Files

```bash
# Shared reservation (default) — multiple agents can hold shared locks
filament reserve "src/auth/*.rs" --agent coder-1 --ttl 1800

# Exclusive reservation — only one agent can hold this lock
filament reserve "src/main.rs" --agent coder-1 --exclusive --ttl 3600
```

Default TTL is 3600 seconds (1 hour).

### Release Files

```bash
filament release "src/auth/*.rs" --agent coder-1
```

### List Reservations

```bash
filament reservations                              # all active reservations
filament reservations --agent coder-1              # filter by agent
filament reservations --clean                      # clean expired, then list
```

---

## Daemon

Filament runs in two modes:
- **Direct mode** — CLI talks directly to SQLite (single-user, default)
- **Daemon mode** — CLI connects to a Unix socket server (multi-agent, required for dispatching)

The CLI auto-detects which mode to use based on whether `.filament/filament.sock` exists.

### Start the Daemon

```bash
filament serve                  # background (daemonizes)
filament serve --foreground     # foreground (for debugging)
```

### Stop the Daemon

```bash
filament stop
```

### Custom Socket Path

```bash
filament serve --socket-path /tmp/filament.sock
```

---

## Agent Dispatching

Dispatch AI agents to work on tasks. Requires the daemon to be running.

### Agent Roles

| Role | Purpose | Key Capabilities |
|------|---------|-----------------|
| `coder` | Implement code changes | File reservations, code editing, test running |
| `reviewer` | Review code for correctness and quality | Entity inspection, sending feedback messages |
| `planner` | Break down work and create plans | Entity/relation creation, dependency analysis, critical path |
| `dockeeper` | Maintain documentation | Entity updates, file reservations for docs |

Each role has a compiled-in system prompt and a whitelist of MCP tools it can use.

### Dispatch a Single Agent

```bash
filament agent dispatch abc12345                     # default role: coder
filament agent dispatch abc12345 --role reviewer
filament agent dispatch abc12345 --role planner
```

### Dispatch to All Ready Tasks

```bash
filament agent dispatch-all                          # up to 3 parallel, coder role
filament agent dispatch-all --max-parallel 5 --role reviewer
```

This finds all unblocked tasks and dispatches agents sequentially (one RPC per task) up to the parallel limit. Requires daemon mode.

### Monitor Agent Runs

```bash
filament agent status <run-uuid>       # check a specific run
filament agent list                    # list running agents
filament agent history abc12345        # all runs for a task
```

### Agent Lifecycle

1. Agent is spawned as a subprocess with the task context and role prompt
2. Agent uses MCP tools to interact with the knowledge graph
3. Agent emits a structured `AgentResult` JSON on completion
4. Filament parses the result, routes messages, and updates task status
5. On agent death: task status is reverted, file reservations are released, graph is refreshed

---

## MCP Server

Filament exposes a [Model Context Protocol](https://modelcontextprotocol.io/) server for AI agent integration. The MCP server provides 16 tools over JSON-RPC via stdin/stdout.

### Start the MCP Server

```bash
filament mcp
```

This runs the MCP stdio transport. All logs go to stderr; stdout is reserved for JSON-RPC.

### Available MCP Tools

#### Entity Operations

| Tool | Description |
|------|-------------|
| `filament_add` | Create a new entity |
| `filament_inspect` | Get entity details and relations |
| `filament_update` | Update entity summary and/or status |
| `filament_delete` | Delete an entity and its relations |
| `filament_list` | List/filter entities by type and status |

#### Relation Operations

| Tool | Description |
|------|-------------|
| `filament_relate` | Create a relation between entities |
| `filament_unrelate` | Remove a relation |
| `filament_context` | BFS graph neighborhood query |

#### Task Operations

| Tool | Description |
|------|-------------|
| `filament_task_ready` | Get ranked unblocked tasks |
| `filament_task_close` | Mark a task as closed |

#### Messaging

| Tool | Description |
|------|-------------|
| `filament_message_send` | Send a message to another agent |
| `filament_message_inbox` | Check unread messages |
| `filament_message_read` | Mark a message as read |

#### File Reservations

| Tool | Description |
|------|-------------|
| `filament_reserve` | Acquire an advisory file lock |
| `filament_release` | Release a file reservation |
| `filament_reservations` | List active reservations |

---

## TUI Dashboard

An interactive terminal UI for monitoring tasks, agents, and reservations in real time.

### Launch

```bash
filament tui
```

### Tabs

| Tab | Key | Content |
|-----|-----|---------|
| Tasks | `1` | Task list with status, priority, blocked count, impact score |
| Agents | `2` | Running agent processes with role, PID, duration |
| Reservations | `3` | Active file locks with TTL countdown |

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit |
| `Tab` | Next tab |
| `Shift+Tab` | Previous tab |
| `1` `2` `3` | Jump to tab |
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `r` | Force refresh |
| `f` | Cycle task filter (Tasks tab only): open → in_progress → blocked → closed → all |
| `c` | Close selected task (Tasks tab only) |

### Display Details

- **Tasks**: status is color-coded (green=open, yellow=in_progress, red=blocked, gray=closed). Impact scores are computed for up to 50 tasks.
- **Agents**: shows live duration for running agents. Status colors: green=running, cyan=completed, red=failed, yellow=blocked, magenta=needs_input.
- **Reservations**: TTL countdown with yellow warning under 5 minutes and red "EXPIRED" label. Expired rows are dimmed.

Auto-refreshes every 5 seconds. Status bar shows connection mode (daemon/direct) and last refresh time.

---

## JSON Output

All commands support `--json` for machine-readable output. Errors are also structured:

```json
{"code": "entity_not_found", "message": "No entity with slug 'xyz'", "hint": "Check the slug with 'filament list'", "retryable": false}
```

---

## Project Structure

```
filament/
├── Cargo.toml                  # workspace: 4 crates, single binary
├── Makefile                    # build, test, lint, CI targets
├── migrations/                 # SQLite migrations (sqlx)
├── crates/
│   ├── filament-core/          # library: models, graph, storage, errors
│   ├── filament-cli/           # binary: clap CLI, command handlers
│   ├── filament-daemon/        # library: Unix socket server, MCP, agent roles
│   └── filament-tui/           # library: ratatui terminal UI
└── .plan/                      # plans, ADRs, test standards
```

## Development

```bash
make fmt CRATE=all              # format code
make fmt CRATE=all CHECK=1      # check formatting (CI)
make check CRATE=all CLIPPY=1   # clippy lints
make test CRATE=all             # run all tests
make test CRATE=filament-core   # test a single crate
make ci                         # full CI pipeline
make migration NAME=add_foo     # create a new migration
make adr TITLE="Decision name"  # create a new ADR
```

## Inspiration

Filament was directly inspired by [beads_rust](https://github.com/Dicklesworthstone/beads_rust) (task management and error patterns) and [Flywheel](https://github.com/Dicklesworthstone) (multi-agent orchestration ecosystem), both by [Jeff Emanuel](https://github.com/Dicklesworthstone). Those projects demonstrated powerful ideas across separate tools — Filament consolidates them into a single Rust binary that handles knowledge graph, task management, agent orchestration, messaging, and file coordination all in one place.

## License

MIT License. See [LICENSE](LICENSE) for details.
