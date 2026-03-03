---
name: filament
description: >
  Use the filament CLI for project knowledge management, task tracking, and inter-agent
  coordination. Filament stores entities (tasks, modules, services, agents, plans, docs),
  relations between them, messages, and file reservations in a local SQLite database.
  Use when: managing project context, tracking tasks, recording architecture decisions,
  creating knowledge graph entries, coordinating agent work, or querying project structure.
  Triggers on: "filament", "add entity", "add task", "track this", "create a task",
  "relate these", "what blocks", "critical path", "ready tasks", "project graph",
  "knowledge graph", "file reservation", "agent message", "what's next".
---

# Filament CLI — Project Knowledge Management

Filament is a local-only knowledge graph + task manager. All data lives in `.filament/`
within the project root. Initialize with `filament init` before any other command.

## Quick Reference

### Project Setup
```bash
filament init                              # creates .filament/ with SQLite DB
```

### Entity CRUD (the core building block)

Entities are nodes in the knowledge graph. Types: `task`, `module`, `service`, `agent`, `plan`, `doc`.

```bash
# Create
filament add <NAME> --type <TYPE> --summary "..." [--priority 0-4] [--facts '{"k":"v"}'] [--content path/to/file]

# Read
filament inspect <NAME>                    # details + relations
filament read <NAME>                       # full content file
filament list [--type TYPE] [--status STATUS]

# Update
filament update <NAME> --summary "new"     # update summary
filament update <NAME> --status closed     # update status (open|closed|in_progress|blocked)
filament update <NAME> --summary "new" --status in_progress  # both at once

# Delete
filament remove <NAME>                     # cascades relations
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
filament task add <TITLE> --summary "..." [--priority N] [--blocks NAME] [--depends-on NAME]
filament task list [--status open|closed|in_progress|all]
filament task list --unblocked             # open tasks with no open blockers
filament task ready [--limit N]            # unblocked tasks ranked by priority (graph-based)
filament task show <NAME>                  # details + relations
filament task close <NAME>                 # sets status=closed
filament task assign <NAME> --to <AGENT>   # creates assigned_to relation
filament task critical-path <NAME>         # longest dependency chain
```

### Context Queries (graph traversal)

```bash
filament context --around <NAME> --depth N [--limit N]  # BFS neighborhood
```

### Inter-Agent Messaging

```bash
filament message send --from <AGENT> --to <AGENT> --body "..." [--type text|question|blocker|artifact]
filament message inbox <AGENT>             # unread messages
filament message read <MSG_ID>             # mark as read
```

### File Reservations (advisory locking)

```bash
filament reserve <GLOB> --agent <NAME> [--exclusive] [--ttl SECS]
filament release <GLOB> --agent <NAME>
filament reservations [--agent NAME] [--clean]
```

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

### Query project structure

```bash
filament list --type module                # all modules
filament list --type task --status open    # open tasks
filament task ready                        # what to work on next
filament context --around filament-core --depth 2  # what connects to core
```

## Entity Types and When to Use Them

| Type | Use For | Examples |
|------|---------|---------|
| `task` | Work items with lifecycle | "implement daemon", "fix bug #42" |
| `module` | Code modules/crates | "filament-core", "store.rs" |
| `service` | Running services/components | "sqlite-db", "unix-socket-server" |
| `agent` | AI agents or human actors | "planner-agent", "code-reviewer" |
| `plan` | Planning documents | "phase-3-plan", "architecture-overview" |
| `doc` | Reference documentation | "adr-003", "api-spec", "gotchas" |

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

- Entity names are NOT unique — use descriptive, specific names to avoid ambiguity
- Names can be used in place of IDs for all commands (resolved by first match)
- `--json` output is suitable for piping to `jq` or parsing in scripts
- Priority 0 = highest urgency, 4 = lowest (default is 2)
- Reservations use string-equality for glob matching (not pattern overlap)
- `task close` and `task assign` validate the entity is actually a task
- `task list --status` and `--unblocked` cannot be combined
