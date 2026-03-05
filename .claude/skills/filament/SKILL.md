---
name: filament
description: >
  Use the filament CLI for project knowledge management, task tracking, lesson capture,
  and inter-agent coordination. Filament stores entities (tasks, modules, services, agents,
  plans, docs, lessons), relations between them, messages, and file reservations in a local
  SQLite database. Use when: managing project context, tracking tasks, recording gotchas
  and lessons, recording architecture decisions, creating knowledge graph entries,
  coordinating agent work, or querying project structure.
  Triggers on: "filament", "add entity", "add task", "add lesson", "track this",
  "create a task", "gotcha", "lesson", "record a lesson", "relate these", "what blocks",
  "critical path", "ready tasks", "project graph", "knowledge graph", "file reservation",
  "agent message", "what's next", "dispatch agent", "escalations", "export", "import",
  "daemon", "tui".
---

# Filament CLI — Project Knowledge Management

Filament is a local-only knowledge graph + task manager + multi-agent orchestrator. All data
lives in `.filament/` within the project root. Initialize with `filament init` before any
other command.

## Identity: Slugs

Every entity gets a unique **8-char slug** (`[a-z0-9]`) auto-generated on creation. Slugs are
the primary human-facing identifier. All commands accept slugs (or UUIDs) wherever an entity
reference is needed. Use `filament list` or `filament inspect` to find slugs.

## Quick Reference

### Project Setup
```bash
filament init                              # creates .filament/ with SQLite DB
```

### Entity CRUD (the core building block)

Entities are nodes in the knowledge graph. Types: `task`, `module`, `service`, `agent`, `plan`, `doc`, `lesson`.

```bash
# Create — returns the entity's slug
filament add <NAME> --type <TYPE> --summary "..." [--priority 0-4] [--facts '{"k":"v"}'] [--content path/to/file]

# Read
filament inspect <SLUG>                    # details + relations
filament read <SLUG>                       # full content file
filament list [--type TYPE] [--status STATUS]

# Update
filament update <SLUG> --summary "new"     # update summary
filament update <SLUG> --status closed     # update status (open|closed|in_progress|blocked)
filament update <SLUG> --summary "new" --status in_progress  # both at once

# Delete
filament remove <SLUG>                     # cascades relations
```

### Relations (edges in the graph)

Relations connect entities. Types: `blocks`, `depends_on`, `produces`, `owns`, `relates_to`, `assigned_to`.

```bash
filament relate <SOURCE> <TYPE> <TARGET> [--summary "..." --weight N]
filament unrelate <SOURCE> <TYPE> <TARGET>
```

### Tasks (specialized entity workflow)

Tasks are entities with type=task. The `task` subcommand provides workflow shortcuts.

```bash
filament task add <TITLE> --summary "..." [--priority N] [--blocks SLUG] [--depends-on SLUG]
filament task list [--status open|closed|in_progress|all]
filament task list --unblocked             # open tasks with no open blockers
filament task ready [--limit N]            # unblocked tasks ranked by priority (graph-based)
filament task show <SLUG>                  # details + relations
filament task close <SLUG>                 # sets status=closed
filament task assign <SLUG> --to <AGENT>   # creates assigned_to relation
filament task critical-path <SLUG>         # longest dependency chain
```

### Lessons (knowledge capture — gotchas, patterns, solutions)

Lessons are entities with type=lesson. They capture reusable knowledge with structured fields.
**Gotchas, recurring problems, and solutions should ALWAYS be recorded as Lesson entities** (not Doc).

```bash
filament lesson add <TITLE> --problem "what was failing" --solution "how to fix" --learned "key insight" [--pattern "pattern-name"] [--priority N]
filament lesson list [--pattern NAME] [--status all|open|closed]
filament lesson show <SLUG>                # structured display of problem/solution/pattern/learned
```

Lesson fields are stored in `key_facts` JSON. The `--learned` value is also used as the entity summary.
Pattern names enable cross-project knowledge transfer (e.g., "n-plus-one-fix", "circuit-breaker").

### Agent Dispatch (subprocess management)

Dispatch AI agents to work on tasks. Agents run as subprocesses and report results.

```bash
filament agent dispatch <TASK_SLUG> [--role coder|reviewer|planner|dockeeper]
filament agent dispatch-all [--max-parallel N] [--role ROLE]   # dispatch all ready tasks
filament agent status <RUN_ID>             # check a specific agent run
filament agent list                        # all agent runs
filament agent history <TASK_SLUG>         # past runs for a task
```

Roles determine the system prompt and tool access:
- `coder` (default) — writes code, runs tests
- `reviewer` — reviews code, suggests improvements
- `planner` — creates plans and breaks down work
- `dockeeper` — writes documentation

### Escalations

Escalations surface blockers, questions, and needs-input from agents to the user.

```bash
filament escalations                       # show all pending escalations
```

Escalations are created automatically when agents send `--type blocker` or `--type question`
messages TO `user`. They appear in the TUI escalation indicator and in `filament escalations`.

### Context Queries (graph traversal)

```bash
filament context --around <SLUG> --depth N [--limit N]  # BFS neighborhood
filament pagerank [--damping 0.85] [--iterations 50] [--limit N]  # PageRank scores
filament degree [--limit N]                              # degree centrality (in/out/total)
```

### Inter-Agent Messaging

```bash
filament message send --from <AGENT> --to <AGENT> --body "..." [--type text|question|blocker|artifact]
filament message inbox <AGENT>             # unread messages
filament message read <MSG_ID>             # mark as read
```

Message types:
- `text` — general communication (default)
- `question` — agent needs a decision (creates escalation if sent to `user`)
- `blocker` — agent is blocked and cannot proceed (creates escalation if sent to `user`)
- `artifact` — agent produced a deliverable

### File Reservations (advisory locking)

```bash
filament reserve <GLOB> --agent <NAME> [--exclusive] [--ttl SECS]
filament release <GLOB> --agent <NAME>
filament reservations [--agent NAME] [--clean]
```

### Export / Import

```bash
filament export [--output PATH] [--no-events]    # export all data to JSON
filament import [--input PATH] [--no-events]     # import from JSON
```

Export creates a complete snapshot (entities, relations, messages, reservations, events).
Import performs upserts — existing entities are updated, new ones are inserted.

### Configuration

```bash
filament config show                       # display resolved configuration
filament config init                       # create filament.toml with defaults
filament config path                       # print config file path
```

Config file (`filament.toml`) supports layered resolution: defaults → config → env → CLI.

### Change Notifications

```bash
filament watch [--events entity_created,entity_updated,...]  # real-time change stream
```

Subscribes to daemon push notifications for entity/relation/message changes.

### Git Hooks

```bash
filament hook install                      # install pre-commit reservation check
filament hook uninstall                    # remove the hook
filament hook check [--agent NAME]         # run the check manually
```

### Seed (auto-populate from project files)

```bash
filament seed [--dry-run]                  # parse CLAUDE.md sections into Doc entities
```

### Audit Trail (git-backed snapshots)

```bash
filament audit [--branch NAME] [--message "..."]  # snapshot graph to git branch
```

### Shell Completions

```bash
filament completions bash|zsh|fish|elvish|powershell
```

### Daemon (multi-agent mode)

```bash
filament serve [--foreground] [--socket-path PATH]  # start Unix socket server
filament stop                                       # stop running daemon
```

When the daemon is running, all CLI commands route through it via Unix socket instead of
accessing SQLite directly. This enables concurrent multi-agent access.

### TUI (interactive dashboard)

```bash
filament tui                               # launch ratatui terminal UI
```

The TUI shows: task list, agent status, file reservations, messages, graph view, and an escalation indicator.

### MCP Server (AI agent integration)

```bash
filament mcp                               # start MCP stdio server
```

Exposes 16 tools via the Model Context Protocol for AI agent integration. Agents connect
via stdio and can create/read/update entities, send messages, manage tasks, etc.

### Global Flags

| Flag | Effect |
|------|--------|
| `--json` | Machine-readable JSON output |
| `-v` | Debug logging (filament crate) |
| `-vv` | Trace logging (all crates) |
| `-q` | Suppress non-error output |

## Common Workflows

### Import project documentation as knowledge entities

```bash
filament add project-plan --type plan --summary "Master plan v1.1" --content .plan/filament-v1.md
filament add phase-1-core --type plan --summary "Phase 1: Core library" --content .plan/phase1-core.md
filament relate project-plan owns phase-1-core
```

### Record a gotcha / lesson learned

```bash
filament lesson add "SQLite CHECK constraint" \
  --problem "INSERT fails when new entity_type not in CHECK list" \
  --solution "Recreate table with updated CHECK constraint in migration" \
  --learned "SQLite cannot ALTER CHECK constraints — must recreate table" \
  --pattern "sqlite-check-migration"
```

### Track architecture decisions

```bash
filament add adr-003-unified-graph --type doc --summary "All data as Entity nodes + Relation edges" \
  --facts '{"status":"accepted","date":"2026-02-28"}' --content .plan/adr/003-unified-graph.md
filament relate adr-003-unified-graph relates_to filament-core
```

### Create task with dependency chain

```bash
filament task add implement-daemon --summary "Unix socket server + MCP protocol" --priority 1
filament task add implement-tui --summary "Ratatui dashboard" --depends-on implement-daemon
filament task critical-path implement-tui   # shows: implement-daemon -> implement-tui
```

### Agent coordination workflow

```bash
filament add agent-planner --type agent --summary "Planning agent"
filament task assign implement-daemon --to agent-planner
filament reserve "crates/filament-daemon/**" --agent agent-planner --exclusive --ttl 7200
filament message send --from orchestrator --to agent-planner --body "Start phase 3 implementation"
```

### Dispatch agents to ready tasks

```bash
filament task ready                        # see what's unblocked
filament agent dispatch <TASK_SLUG>        # dispatch single task (role=coder)
filament agent dispatch-all --max-parallel 3  # dispatch all ready tasks
filament agent list                        # monitor running agents
filament escalations                       # check for blockers/questions
```

### Handle escalations

```bash
filament escalations                       # see pending blockers & questions
# Respond to the agent that raised the escalation:
filament message send --from user --to <AGENT_SLUG> --body "Answer to your question" --type text
# Then unblock the task:
filament update <TASK_SLUG> --status in_progress
```

### Export, backup, and restore

```bash
filament export --output snapshot.json     # full backup
filament import --input snapshot.json      # restore into another project
```

### Query project structure

```bash
filament list --type module                # all modules
filament list --type task --status open    # open tasks
filament task ready                        # what to work on next
filament context --around <SLUG> --depth 2 # what connects to an entity
```

### Simulation / Roleplay (exercise the full system)

Seed a temp project with example data and manually simulate agent cycles:

```bash
# Setup
cd /tmp && rm -rf filament-sim && mkdir filament-sim && cd filament-sim
filament init

# Create entities
filament add api-gateway --type module --summary "HTTP routing layer"
filament add auth-service --type module --summary "JWT auth + sessions"
filament add alice --type agent --summary "Senior backend coder"
filament add bob --type agent --summary "Frontend specialist"
filament task add design-arch --summary "Design system architecture" --priority 0
filament task add setup-db --summary "PostgreSQL schema + migrations" --priority 1

# Create dependency chain
filament relate <design-arch-slug> blocks <setup-db-slug>

# Simulate agent cycle
filament task ready                        # → design-arch is unblocked
filament task assign <slug> --to <agent-slug>
filament update <slug> --status in_progress
filament message send --from alice --to bob --body "DB schema ready" --type artifact
filament task close <slug>
filament task ready                        # → next task unblocked

# Simulate escalation
filament message send --from alice --to user --body "BLOCKED: need credentials" --type blocker
filament update <slug> --status blocked
filament escalations                       # → shows alice's blocker

# Cleanup
rm -rf /tmp/filament-sim
```

## Entity Types and When to Use Them

| Type | Purpose | Examples |
|------|---------|---------|
| `task` | **Work items** — the only type with full status workflow (open → in_progress → closed). Has priority, dependency tracking, ready-queue ranking, critical path. | "implement daemon", "fix bug #42" |
| `module` | **Code structure** — crates, files, subsystems. Relate to tasks so agents know which code a task touches. | "filament-core", "store.rs" |
| `service` | **Runtime components** — running infrastructure vs code. | "sqlite-db", "unix-socket-server" |
| `agent` | **Actors** — required for `task assign`, `message send`, `reserve`. | "planner-agent", "code-reviewer" |
| `plan` | **Planning docs** — group tasks via `owns`. Always use `--content path/to/plan.md` to point at the file. | "phase-3-plan", "architecture-overview" |
| `doc` | **Reference material** — ADRs, specs, runbooks. Always use `--content path/to/doc.md` to point at the file. | "adr-003", "api-spec" |
| `lesson` | **Knowledge capture** — gotchas, recurring problems, solutions, patterns. Use `filament lesson add` with `--problem`, `--solution`, `--learned`. | "sqlx-check-gotcha", "n-plus-one-fix" |

**Design principle**: The graph is lightweight — summaries + pointers, not content duplication.
For `doc` and `plan` types, always use `--content` so the physical `.md` file remains the source of truth.
**Gotchas and lessons** should ALWAYS use the `lesson` type (not `doc`) so they have structured problem/solution/learned fields.

## Relation Types and Semantics

| Type | Reads As | Use For |
|------|----------|---------|
| `blocks` | A blocks B | Task dependencies (B can't start until A closes) |
| `depends_on` | A depends on B | Declarative dependency (A needs B) |
| `produces` | A produces B | Output relationships (build step -> artifact) |
| `owns` | A owns B | Containment (plan owns tasks, module owns files) |
| `relates_to` | A relates to B | General association |
| `assigned_to` | A assigned to B | Agent assignment |

## Structured Errors

All errors have machine-readable exit codes:

| Exit Code | Meaning |
|-----------|---------|
| 0 | Success |
| 2 | CLI argument error (clap) |
| 3 | Not found (entity, relation, message) |
| 4 | Validation error |
| 5 | Database error |
| 6 | Resource conflict (file reservation) |
| 7 | I/O error |

With `--json`, errors output structured JSON with `code`, `message`, `hint`, and `retryable` fields.

## Dual-Track Convention

Filament runs alongside traditional .md documentation:
- **CLAUDE.md** and **MEMORY.md** remain the source of truth for session onboarding
- Filament entities mirror and extend the .md content with queryable structure
- When updating project state, update BOTH .md files and filament entities
- Filament adds: graph queries, dependency tracking, ready-task computation, agent coordination

## Tips

- Entities are identified by **slugs** (8-char `[a-z0-9]`), auto-generated on creation
- Use `filament list` or `filament inspect` to find an entity's slug
- `--json` output is suitable for piping to `jq` or parsing in scripts
- Priority 0 = highest urgency, 4 = lowest (default is 2)
- Reservations use string-equality for glob matching (not pattern overlap)
- `task close` and `task assign` validate the entity is actually a task
- `task list --status` and `--unblocked` cannot be combined
- To create escalations: send `--type blocker` or `--type question` messages FROM an agent TO `user`
- Auto-dispatch: set `FILAMENT_AUTO_DISPATCH=1` to chain agent runs on newly-unblocked tasks
- `filament seed` bootstraps the knowledge graph from CLAUDE.md sections
- `filament audit` snapshots the graph to a git branch for disaster recovery
- `filament pagerank` and `filament degree` show graph analytics
- `filament watch` streams real-time change notifications from the daemon
- `filament config init` creates a `filament.toml` for project-level defaults
