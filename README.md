# Filament

Local-only multi-agent orchestration, knowledge graph, and task management for software projects.

Filament gives AI coding agents (and humans) a shared project brain — a knowledge graph of tasks, modules, services, plans, and docs with dependency tracking, file reservations, inter-agent messaging, and a real-time TUI dashboard. Everything runs locally with a SQLite database per project.

## Key Features

- **Knowledge graph** — entities (tasks, modules, services, agents, plans, docs, lessons) connected by typed relations (blocks, depends_on, produces, owns, relates_to, assigned_to)
- **Task management** — priority-ranked ready queues, critical path analysis, dependency tracking, impact scoring
- **Lesson capture** — structured knowledge capture of gotchas, solutions, and recurring patterns with problem/solution/learned fields
- **Full-text search** — FTS5-powered search across all entities with BM25 relevance ranking
- **Multi-agent orchestration** — dispatch AI agents with specialized roles (coder, reviewer, planner, dockeeper), monitor runs, handle agent death gracefully
- **Inter-agent messaging** — targeted message passing between agents with typed messages (text, question, blocker, artifact)
- **Advisory file reservations** — TTL-based file locks to coordinate concurrent edits across agents
- **Graph analytics** — PageRank and degree centrality to identify high-impact entities and bottlenecks
- **MCP server** — 16-tool Model Context Protocol interface for AI agent integration
- **TUI dashboard** — real-time terminal UI showing tasks, agent runs, and reservations
- **Hybrid architecture** — direct SQLite for single-user, Unix socket daemon for multi-agent
- **Project config** — layered `fl.toml` configuration with environment variable overrides
- **Real-time watch** — live event stream from the daemon for monitoring agent activity
- **Git integration** — pre-commit hooks for reservation conflict checks, audit snapshots to git branches
- **Seed command** — bootstrap the knowledge graph from existing CLAUDE.md documentation

## Installation

**macOS or Linux** required (uses Unix sockets for daemon mode). SQLite is bundled — no system install needed.

### Quick Install (prebuilt binary)

```bash
curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh
```

Options:

```bash
# Custom install directory
curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh -s -- --to /usr/local/bin

# Specific version
curl -fsSL https://raw.githubusercontent.com/JYC11/filament/main/install.sh | sh -s -- --version v1.0.0
```

### Build from Source

Requires the [Rust toolchain](https://rustup.rs/) (stable channel).

```bash
git clone https://github.com/JYC11/filament.git
cd filament
make build CRATE=all RELEASE=1
make install                        # installs to ~/.local/bin (default)
make install DEST=/usr/local/bin    # custom destination
```

### Uninstall

```bash
make uninstall                      # removes from ~/.local/bin (default)
make uninstall DEST=/usr/local/bin  # custom destination
```

### Development

```bash
make build CRATE=all        # debug build
make test CRATE=all          # run all tests
make ci                      # full CI: fmt check + clippy + tests
```

## AI Agent Integration

Filament works best when your coding agent (Claude Code, Cursor, etc.) knows its full command set. Rather than memorizing the CLI, create a **skill** or **rule** from this README so your agent can use `fl` commands natively:

```
Create a skill for the filament CLI based on the README at:
https://github.com/JYC11/filament/blob/main/README.md
```

This gives your agent the complete command reference, entity types, relation types, and workflow patterns — no manual lookup needed.

For **real multi-agent orchestration** — multiple Claude instances working concurrently on the same project — use **tmux** to launch parallel `claude -p` sessions. Start the daemon (`fl serve`), then give each agent its own tmux window. Filament coordinates them through file reservations, task dependencies, and inter-agent messaging. See [Agent Dispatching](#agent-dispatching) for dispatch commands.

---

## Usage

### Initialize a Project

Create a `.fl/` directory with a SQLite database in your project root:

```bash
cd your-project
fl init
```

This creates:
- `.fl/fl.db` — SQLite database (WAL mode)
- `.fl/content/` — content file storage

Add `.fl/` to your `.gitignore`.

### Global Flags

Every command supports these flags:

| Flag        | Short | Description                                  |
| ----------- | ----- | -------------------------------------------- |
| `--json`    |       | Output JSON instead of human-readable text   |
| `--verbose` | `-v`  | Increase verbosity (`-v` debug, `-vv` trace) |
| `--quiet`   | `-q`  | Suppress non-error output                    |

---

## Entity Management

Filament's core data model is a knowledge graph of **entities** connected by **relations**. Every entity has a unique 8-character slug (e.g., `a3kf92mx`) for human-friendly identification.

### Entity Types

| Type      | Purpose | Lifecycle | Example |
| --------- | ------- | --------- | ------- |
| `task`    | **Work items** — the only type with full status workflow. Has priority, dependency tracking, ready-queue ranking, critical path, and impact scoring. | `open` → `in_progress` → `closed` (or `blocked`) | "Implement login endpoint", "Fix bug #42" |
| `module`  | **Code structure** — crates, files, subsystems. Relate to tasks with `owns` or `relates_to` so agents know which code areas a task touches. | Structural (no workflow) | "filament-core", "auth module", "store.rs" |
| `service` | **Runtime components** — running infrastructure, external dependencies. Distinguishes what runs from what's code. | Structural (no workflow) | "sqlite-db", "unix-socket-server", "redis-cache" |
| `agent`   | **Actors** — AI agents or human operators. Required for `task assign`, `message send`, `reserve`. The dispatch system tracks who's working on what. | Structural (no workflow) | "planner-agent", "code-reviewer", "alice" |
| `plan`    | **Planning documents** — group tasks via `owns` relations. Always use `--content` to point at the actual `.md` file. The entity summary is a 1-2 sentence description; the file is the source of truth. | Structural (no workflow) | "phase-3-plan", "architecture-overview" |
| `doc`     | **Reference material** — ADRs, specs, runbooks, knowledge. Like plans, always use `--content` to reference the file on disk. The graph adds queryable structure; the file has the content. | Structural (no workflow) | "adr-003-unified-graph", "api-spec", "gotchas" |
| `lesson`  | **Knowledge capture** — gotchas, solutions, recurring patterns. Structured fields: `problem`, `solution`, `learned`, optional `pattern`. Use `fl lesson add` to create. Searchable via FTS5. | Structural (no workflow) | "SQLite CHECK constraint gotcha", "N+1 query fix" |

**Design principle**: The graph is lightweight. Entities store short summaries + pointers to files. For `doc` and `plan` types, always use `--content path/to/file.md` so the physical file remains the source of truth. The graph adds queryable relations, dependency tracking, and context queries on top.

### Entity Status

All entities have a status field (`open`, `in_progress`, `closed`, `blocked`), but only **tasks** use the full status workflow. Other entity types are structural — they exist to give the graph shape for context queries, not to be "completed".

### Add an Entity

```bash
fl add "Authentication Module" --type module --summary "Handles JWT auth" --priority 1

# With key facts (JSON object)
fl add "API Gateway" --type service \
  --summary "Routes requests to microservices" \
  --facts '{"port": 8080, "protocol": "HTTP/2"}'

# With a content file (must exist on disk)
fl add "Architecture Decision" --type doc \
  --summary "ADR-001: Database choice" \
  --content .plan/adr/001-database.md
```

Output: `Created: a3kf92mx (550e8400-e29b-41d4-a716-446655440000)`

### Inspect an Entity

```bash
fl inspect a3kf92mx
```

Shows name, slug, ID, type, status, priority, summary, facts, content path, relations, and timestamps.

### Update an Entity

```bash
fl update a3kf92mx --status in_progress
fl update a3kf92mx --summary "Updated description"
fl update a3kf92mx --status closed --summary "Done"
fl update a3kf92mx --priority 0
fl update a3kf92mx --facts '{"key": "value"}'
fl update a3kf92mx --content path/to/file.md
```

### Read Entity Content

Print the raw content file associated with an entity:

```bash
fl read a3kf92mx
```

### List Entities

```bash
fl list                          # all entities
fl list --type task              # only tasks
fl list --status open            # only open entities
fl list --status all             # all statuses (including closed)
fl list --type module --status in_progress
```

### Remove an Entity

```bash
fl remove a3kf92mx
```

Removes the entity and all its relations.

---

## Relations

Connect entities with typed, directed edges.

### Relation Types

| Type          | Meaning                             |
| ------------- | ----------------------------------- |
| `blocks`      | Source blocks target from starting  |
| `depends_on`  | Source depends on target being done |
| `produces`    | Source produces/creates target      |
| `owns`        | Source owns/contains target         |
| `relates_to`  | General association                 |
| `assigned_to` | Agent is assigned to a task         |

### Create a Relation

```bash
fl relate abc12345 blocks def67890
fl relate abc12345 depends_on def67890 --summary "needs API first"
fl relate abc12345 owns def67890 --weight 1.0
```

### Remove a Relation

```bash
fl unrelate abc12345 blocks def67890
```

### Explore the Graph

View the neighborhood around an entity (BFS traversal):

```bash
fl context --around abc12345              # default depth 2, limit 20
fl context --around abc12345 --depth 3    # deeper traversal
fl context --around abc12345 --limit 50   # more results
```

---

## Task Management

Tasks are the primary work unit. The `task` subcommand provides specialized task operations beyond basic entity CRUD.

### Add a Task

```bash
fl task add "Implement login endpoint" --summary "POST /auth/login" --priority 0

# With dependency relations
fl task add "Write integration tests" \
  --summary "Test the login flow" \
  --depends-on abc12345 \
  --priority 2

# With blocking relation
fl task add "Deploy to staging" --blocks def67890
```

### List Tasks

```bash
fl task list                     # open tasks (default)
fl task list --status all        # all tasks regardless of status
fl task list --status in_progress
fl task list --status closed
fl task list --unblocked         # only tasks with no blockers
```

Note: `--status` and `--unblocked` are mutually exclusive.

### Show Task Details

```bash
fl task show abc12345
```

### Ready Tasks

Show unblocked tasks ranked by priority and impact score:

```bash
fl task ready               # top 20 by default
fl task ready --limit 5     # top 5 only
```

Impact score counts how many downstream tasks are transitively blocked by each task — higher impact tasks should be done first.

### Blocker Depth

Show the longest dependency chain blocking a task:

```bash
fl task blocker-depth abc12345
```

### Close a Task

```bash
fl task close abc12345
```

### Assign a Task

```bash
fl task assign abc12345 --to agent_slug
```

Creates an `assigned_to` relation from the agent to the task.

---

## Lessons

Structured knowledge capture for gotchas, solutions, and recurring patterns. Each lesson has four fields stored in the entity's `key_facts` JSON.

### Add a Lesson

```bash
fl lesson add "SQLite CHECK constraint gotcha" \
  --problem "Cannot ALTER TABLE to modify CHECK constraints" \
  --solution "Recreate the table with the new constraint, copy data, drop old" \
  --learned "SQLite CHECK constraints are immutable after table creation" \
  --pattern "schema-migration-gotcha"
```

### List Lessons

```bash
fl lesson list                              # all lessons
fl lesson list --pattern "migration"        # filter by pattern name
fl lesson list --status open                # filter by status
```

### Show Lesson Details

```bash
fl lesson show abc12345
```

Displays the structured problem/solution/learned/pattern fields.

### Delete a Lesson

```bash
fl lesson delete abc12345
```

---

## Search

Full-text search across all entities using SQLite FTS5 with BM25 relevance ranking. Searches entity names, summaries, and key_facts.

```bash
fl search "migration"                       # search all entities
fl search "connection pool" --type lesson   # filter by entity type
fl search "deployment" --limit 5            # limit results
```

Supports FTS5 query syntax: words, phrases (`"like this"`), `OR`, `NOT`.

---

## Inter-Agent Messaging

Agents communicate through targeted messages. No broadcast — every message has a sender and recipient.

### Message Types

| Type       | Purpose                              |
| ---------- | ------------------------------------ |
| `text`     | General communication (default)      |
| `question` | Questions needing answers            |
| `blocker`  | Blocking issues that need resolution |
| `artifact` | Deliverables, code snippets, results |

### Send a Message

```bash
fl message send --from coder-1 --to reviewer-1 \
  --body "Login endpoint ready for review"

fl message send --from reviewer-1 --to coder-1 \
  --body "Missing input validation on email field" \
  --type blocker
```

### Check Inbox

```bash
fl message inbox coder-1
```

Shows unread messages with sender, type, and a preview.

### Mark as Read

```bash
fl message read <message-uuid>
```

Returns an error if the message was already read (distinct from not found).

---

## File Reservations

Advisory file locks to coordinate concurrent edits. Reservations use glob patterns and have a TTL.

### Reserve Files

```bash
# Shared reservation (default) — multiple agents can hold shared locks
fl reserve "src/auth/*.rs" --agent coder-1 --ttl 1800

# Exclusive reservation — only one agent can hold this lock
fl reserve "src/main.rs" --agent coder-1 --exclusive --ttl 3600
```

Default TTL is 3600 seconds (1 hour).

### Release Files

```bash
fl release "src/auth/*.rs" --agent coder-1
```

### List Reservations

```bash
fl reservations                              # all active reservations
fl reservations --agent coder-1              # filter by agent
fl reservations --clean                      # clean expired, then list
```

---

## Daemon

Filament runs in two modes:

- **Direct mode** — CLI talks directly to SQLite (single-user, default)
- **Daemon mode** — CLI connects to a Unix socket server (multi-agent, required for dispatching)

The CLI auto-detects which mode to use based on whether `.fl/fl.sock` exists.

### Start the Daemon

```bash
fl serve                  # background (daemonizes)
fl serve --foreground     # foreground (for debugging)
```

### Stop the Daemon

```bash
fl stop
```

### Custom Socket Path

```bash
fl serve --socket-path /tmp/fl.sock
```

---

## Agent Dispatching

Dispatch AI agents to work on tasks. Requires the daemon to be running.

### Agent Roles

| Role        | Purpose                                 | Key Capabilities                                             |
| ----------- | --------------------------------------- | ------------------------------------------------------------ |
| `coder`     | Implement code changes                  | File reservations, code editing, test running                |
| `reviewer`  | Review code for correctness and quality | Entity inspection, sending feedback messages                 |
| `planner`   | Break down work and create plans        | Entity/relation creation, dependency analysis, critical path |
| `dockeeper` | Maintain documentation                  | Entity updates, file reservations for docs                   |

Each role has a compiled-in system prompt and a whitelist of MCP tools it can use.

### Dispatch a Single Agent

```bash
fl agent dispatch abc12345                     # default role: coder
fl agent dispatch abc12345 --role reviewer
fl agent dispatch abc12345 --role planner
```

### Dispatch to All Ready Tasks

```bash
fl agent dispatch-all                          # up to 3 parallel, coder role
fl agent dispatch-all --max-parallel 5 --role reviewer
```

This finds all unblocked tasks and dispatches agents sequentially (one RPC per task) up to the parallel limit. Requires daemon mode.

### Monitor Agent Runs

```bash
fl agent status <run-uuid>       # check a specific run
fl agent list                    # list running agents
fl agent history abc12345        # all runs for a task
```

### Agent Lifecycle

1. Agent is spawned as a subprocess with the task context and role prompt
2. Agent uses MCP tools to interact with the knowledge graph
3. Agent emits a structured `AgentResult` JSON on completion
4. Filament parses the result, routes messages, and updates task status
5. On agent death: task status is reverted, file reservations are released, graph is refreshed
6. **Timeout**: agents are killed after `agent_timeout_secs` (default 1 hour, 0 = no limit)
7. **Reconciliation**: daemon periodically checks for dead agent PIDs and cleans up orphaned runs

---

## MCP Server

Filament exposes a [Model Context Protocol](https://modelcontextprotocol.io/) server for AI agent integration. The MCP server provides 16 tools over JSON-RPC via stdin/stdout.

### Start the MCP Server

```bash
fl mcp
```

This runs the MCP stdio transport. All logs go to stderr; stdout is reserved for JSON-RPC.

### Available MCP Tools

#### Entity Operations

| Tool               | Description                             |
| ------------------ | --------------------------------------- |
| `fl_add`     | Create a new entity                     |
| `fl_inspect` | Get entity details and relations        |
| `fl_update`  | Update entity summary and/or status     |
| `fl_delete`  | Delete an entity and its relations      |
| `fl_list`    | List/filter entities by type and status |

#### Relation Operations

| Tool                | Description                        |
| ------------------- | ---------------------------------- |
| `fl_relate`   | Create a relation between entities |
| `fl_unrelate` | Remove a relation                  |
| `fl_context`  | BFS graph neighborhood query       |

#### Task Operations

| Tool                  | Description                |
| --------------------- | -------------------------- |
| `fl_task_ready` | Get ranked unblocked tasks |
| `fl_task_close` | Mark a task as closed      |

#### Messaging

| Tool                     | Description                     |
| ------------------------ | ------------------------------- |
| `fl_message_send`  | Send a message to another agent |
| `fl_message_inbox` | Check unread messages           |
| `fl_message_read`  | Mark a message as read          |

#### File Reservations

| Tool                    | Description                   |
| ----------------------- | ----------------------------- |
| `fl_reserve`      | Acquire an advisory file lock |
| `fl_release`      | Release a file reservation    |
| `fl_reservations` | List active reservations      |

---

## TUI Dashboard

An interactive terminal UI for monitoring the knowledge graph in real time.

### Launch

```bash
fl tui
```

### Tabs

| Tab          | Key | Content                                                          |
| ------------ | --- | ---------------------------------------------------------------- |
| Entities     | `1` | Unified entity table with multi-select type/status filtering     |
| Agents       | `2` | Running agent processes with role, PID, duration                 |
| Reservations | `3` | Active file locks with TTL countdown                             |
| Messages     | `4` | Messages with kind, agent, and body                              |
| Config       | `5` | Read-only view of resolved `fl.toml` configuration values  |
| Analytics    | `6` | PageRank scores and degree centrality for graph entities         |

### Keyboard Shortcuts

| Key            | Action                                                     |
| -------------- | ---------------------------------------------------------- |
| `q` / `Ctrl+C` | Quit                                                       |
| `Tab`          | Next tab                                                   |
| `Shift+Tab`    | Previous tab                                               |
| `1`–`6`        | Jump to tab                                                |
| `j` / `Down`   | Move selection down                                        |
| `k` / `Up`     | Move selection up                                          |
| `Enter`        | Open detail pane (60/40 split with events + critical path) |
| `Esc`          | Close detail pane                                          |
| `n` / `p`      | Next / previous page (Entities tab)                        |
| `t`            | Cycle type filter (Entities tab)                           |
| `s`            | Cycle status filter (Entities tab)                         |
| `h`            | Toggle agent history (Agents tab)                          |
| `r`            | Force refresh + health check                               |

### Display Details

- **Entities**: client-side paging (20 per page), multi-select type/status filters, detail pane with event log and critical path on Enter.
- **Agents**: shows live duration for running agents. Press `h` to toggle between running-only and full history. Status colors: green=running, cyan=completed, red=failed, yellow=blocked, magenta=needs_input.
- **Reservations**: TTL countdown with yellow warning under 5 minutes and red "EXPIRED" label. Expired rows are dimmed.
- **Messages**: escalations color-coded by kind (red=blocker, yellow=question, magenta=needs_input).
- **Config**: read-only display of resolved configuration values.
- **Analytics**: PageRank and degree centrality scores, calculated on demand.

Auto-refreshes every 5 seconds. Status bar shows entity count, connection mode (daemon/direct), health indicator, and last refresh time.

---

## Export / Import

Export the entire knowledge graph (entities, relations, messages, events) as JSON for backup or transfer between projects.

### Export

```bash
fl export                        # print to stdout
fl export --output backup.json   # write to file
fl export --no-events            # exclude event log
```

### Import

```bash
fl import --input backup.json    # read from file
cat backup.json | fl import      # read from stdin
fl import --input backup.json --no-events  # skip events
```

Import reports counts of entities, relations, messages, and events imported.

---

## Escalations

View pending escalations — blockers, questions, and needs-input statuses from agent runs that require human attention.

```bash
fl escalations                   # human-readable table
fl escalations --json            # structured JSON
```

Output shows kind, agent name, message body, and associated task (if any).

---

## Auto-Dispatch

When `FILAMENT_AUTO_DISPATCH=1` is set, the daemon automatically dispatches agents to newly-unblocked tasks. When an agent closes a task, any downstream tasks that become unblocked are queued for dispatch.

```bash
FILAMENT_AUTO_DISPATCH=1 fl serve
```

---

## Shell Completions

Generate shell completions for your preferred shell:

```bash
fl completions bash > ~/.local/share/bash-completion/completions/fl
fl completions zsh > ~/.zfunc/_fl
fl completions fish > ~/.config/fish/completions/fl.fish
```

Supported shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

---

## Configuration

Filament uses a layered config system. Create `.fl/config.toml` in your project:

```bash
fl config init > .fl/config.toml    # generate template
fl config show                       # show resolved values
fl config path                       # show config file path
```

Resolution order: environment variables (`FILAMENT_*`) > config file > defaults.

| Setting | Env Var | Default |
|---------|---------|---------|
| `default_priority` | — | `2` |
| `output_format` | — | `text` |
| `agent_command` | `FILAMENT_AGENT_COMMAND` | `claude` |
| `auto_dispatch` | `FILAMENT_AUTO_DISPATCH` | `false` |
| `context_depth` | `FILAMENT_CONTEXT_DEPTH` | `2` |
| `max_auto_dispatch` | `FILAMENT_MAX_AUTO_DISPATCH` | `3` |
| `cleanup_interval_secs` | `FILAMENT_CLEANUP_INTERVAL` | `60` |
| `idle_timeout_secs` | `FILAMENT_IDLE_TIMEOUT` | `1800` (0 = never, daemon only) |
| `reconciliation_interval_secs` | `FILAMENT_RECONCILIATION_INTERVAL` | `30` (daemon only) |
| `agent_timeout_secs` | `FILAMENT_AGENT_TIMEOUT` | `3600` (0 = no limit, daemon only) |

---

## Watch

Real-time event stream from the daemon. Requires `fl serve` to be running.

```bash
fl watch                                   # all events
fl watch --events entity_created,status_change   # filter by type
```

---

## Graph Analytics

### PageRank

Identify the most connected/important entities in the knowledge graph:

```bash
fl pagerank                                # top 20 by PageRank score
fl pagerank --damping 0.85 --iterations 50 --limit 10
```

### Degree Centrality

Show entities with the most connections:

```bash
fl degree                                  # top 20 by total degree
fl degree --limit 10
```

---

## Git Hooks

Pre-commit hook that blocks commits when staged files conflict with exclusive file reservations held by other agents.

```bash
fl hook install                            # add to .git/hooks/pre-commit
fl hook uninstall                          # remove from pre-commit
fl hook check                              # run the check manually
fl hook check --agent coder-1              # exclude own reservations
```

---

## Seed

Bootstrap the knowledge graph from markdown documentation:

```bash
fl seed --file CLAUDE.md                   # parse a specific markdown file
fl seed --files paths.txt                  # ingest multiple files listed one per line
fl seed --file CLAUDE.md --dry-run         # preview without creating
```

Parses `## Section` headings as Doc entities with summaries from the first content line. Skips duplicates.

The `--files` flag accepts a text file with one markdown path per line (blank lines and `#` comments are skipped). Combine with `--dry-run` to preview before ingesting.

---

## Audit

Snapshot the knowledge graph to a git branch for version-controlled audit trails:

```bash
fl audit                                   # commit to filament-audit branch
fl audit --branch my-audit                 # custom branch name
fl audit --message "milestone snapshot"    # custom commit message
```

Exports entities, relations, messages, and events as JSON. Creates an orphan branch on first run.

---

## JSON Output

All commands support `--json` for machine-readable output. Errors are also structured:

```json
{
  "code": "entity_not_found",
  "message": "No entity with slug 'xyz'",
  "hint": "Check the slug with 'fl list'",
  "retryable": false
}
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

## Why "Filament"?

A filament is a single fine thread that, woven together with others, forms something strong. In the same way, Filament weaves together isolated pieces of project knowledge — tasks, code modules, plans, docs, lessons, agent activity — into a connected graph that's stronger than the sum of its parts. The name also nods to the thin conductive wire in a light bulb: the thing that turns energy into light. Filament turns scattered context into clarity.

## Inspiration

Filament was directly inspired by [beads_rust](https://github.com/Dicklesworthstone/beads_rust) (task management and error patterns) and [Flywheel](https://github.com/Dicklesworthstone#the-agentic-coding-flywheel) (multi-agent orchestration ecosystem), both by [Jeff Emanuel](https://github.com/Dicklesworthstone). Those projects demonstrated powerful ideas across separate tools — Filament consolidates them into a single Rust binary that handles knowledge graph, task management, agent orchestration, messaging, and file coordination all in one place.

The lessons feature — structured knowledge capture of gotchas, solutions, and recurring patterns — was inspired by [runes](https://github.com/sleeplesslord/runes).

## License

MIT License. See [LICENSE](LICENSE) for details.
