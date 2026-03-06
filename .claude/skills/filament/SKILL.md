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
  "create a task", "gotcha", "lesson", "record a lesson", "search before solving",
  "capture lesson", "search knowledge", "relate these", "what blocks",
  "critical path", "ready tasks", "project graph", "knowledge graph", "file reservation",
  "agent message", "what's next", "dispatch agent", "escalations", "export", "import",
  "daemon", "tui".
---

# Filament CLI — Project Knowledge Management

Local-only knowledge graph + task manager + multi-agent orchestrator. Data lives in `.filament/` (created by `filament init`).

## Identity: Slugs

Every entity gets a unique **8-char slug** (`[a-z0-9]`), auto-generated on creation. All commands accept slugs (or UUIDs).

## Search Before Solving, Capture After Solving

**Every task should follow this protocol.** The knowledge base becomes self-reinforcing.

```bash
# 1. Pick a task
filament task ready
filament update <SLUG> --status in_progress

# 2. Search for existing knowledge before solving
filament search "relevant error or topic"
filament search --type lesson "connection pool"   # filter by type
filament lesson show <SLUG>                        # read a found lesson

# 3. Do the work

# 4. Capture if you learned something non-obvious
filament lesson add "descriptive title" \
  --problem "what was failing" \
  --solution "how to fix it" \
  --learned "key insight for next time" \
  --pattern "optional-pattern-name"

# 5. Close the task
filament task close <SLUG>
```

**Capture when:** surprising errors, unexpected library behavior, non-obvious conventions, debugging sessions > few minutes, anything that saves a future agent time.
**Skip when:** obvious fixes (typos, missing imports), already documented, language basics.

## Command Reference

### Entity CRUD

| Command | Description |
|---------|-------------|
| `filament add <NAME> --type <TYPE> --summary "..."` | Create entity. Optional: `--priority 0-4`, `--facts '{"k":"v"}'`, `--content path/to/file` |
| `filament inspect <SLUG>` | Show details + relations |
| `filament read <SLUG>` | Show full content file |
| `filament list [--type TYPE] [--status STATUS]` | List entities |
| `filament update <SLUG> --summary "..." --status STATUS` | Update (status: open\|closed\|in_progress\|blocked) |
| `filament remove <SLUG>` | Delete (cascades relations) |

Entity types: `task`, `module`, `service`, `agent`, `plan`, `doc`, `lesson`

### Tasks

| Command | Description |
|---------|-------------|
| `filament task add <TITLE> --summary "..." [--priority N] [--blocks SLUG] [--depends-on SLUG]` | Create task |
| `filament task list [--status STATUS] [--unblocked]` | List tasks |
| `filament task ready [--limit N]` | Unblocked tasks ranked by priority |
| `filament task show <SLUG>` | Details + relations |
| `filament task close <SLUG>` | Set status=closed |
| `filament task assign <SLUG> --to <AGENT>` | Assign to agent |
| `filament task critical-path <SLUG>` | Longest dependency chain |

### Lessons

| Command | Description |
|---------|-------------|
| `filament lesson add <TITLE> --problem "..." --solution "..." --learned "..." [--pattern NAME]` | Create lesson |
| `filament lesson list [--pattern NAME] [--status STATUS]` | List lessons |
| `filament lesson show <SLUG>` | Structured problem/solution/pattern/learned display |

**Gotchas and solutions are ALWAYS lessons** (not docs). Pattern names enable cross-project knowledge transfer.

### Search (FTS5 + BM25)

| Command | Description |
|---------|-------------|
| `filament search <QUERY> [--type TYPE] [--limit N] [--json]` | Full-text search across names, summaries, key_facts |

Supports: words, phrases (`"like this"`), `OR`, `NOT` operators.

### Relations

| Command | Description |
|---------|-------------|
| `filament relate <SRC> <TYPE> <TGT> [--summary "..." --weight N]` | Create relation |
| `filament unrelate <SRC> <TYPE> <TGT>` | Remove relation |

Types: `blocks` (A blocks B), `depends_on`, `produces`, `owns`, `relates_to`, `assigned_to`

### Agent Dispatch

| Command | Description |
|---------|-------------|
| `filament agent dispatch <TASK> [--role coder\|reviewer\|planner\|dockeeper]` | Dispatch agent to task |
| `filament agent dispatch-all [--max-parallel N]` | Dispatch all ready tasks |
| `filament agent status <RUN_ID>` / `list` / `history <TASK>` | Monitor agents |

### Messaging

| Command | Description |
|---------|-------------|
| `filament message send --from <A> --to <B> --body "..." [--type text\|question\|blocker\|artifact]` | Send message |
| `filament message inbox <AGENT>` / `read <MSG_ID>` | Read messages |
| `filament escalations` | Show pending blockers/questions |

Send `--type blocker` or `--type question` TO `user` to create escalations.

### Graph Queries

| Command | Description |
|---------|-------------|
| `filament context --around <SLUG> --depth N` | BFS neighborhood |
| `filament pagerank` / `filament degree` | Graph analytics |

### File Reservations

| Command | Description |
|---------|-------------|
| `filament reserve <GLOB> --agent <NAME> [--exclusive] [--ttl SECS]` | Acquire lock |
| `filament release <GLOB> --agent <NAME>` | Release lock |
| `filament reservations [--agent NAME] [--clean]` | List reservations |

### Infrastructure

| Command | Description |
|---------|-------------|
| `filament serve [--foreground]` / `filament stop` | Daemon (multi-agent mode) |
| `filament mcp` | MCP stdio server (16 tools for AI agents) |
| `filament tui` | Interactive ratatui dashboard |
| `filament export [--output PATH]` / `filament import [--input PATH]` | Snapshot/restore |
| `filament config show` / `init` / `path` | Configuration (`filament.toml`) |
| `filament watch [--events ...]` | Real-time change notifications |
| `filament hook install` / `uninstall` / `check` | Git pre-commit reservation checks |
| `filament seed [--file PATH] [--files PATH] [--dry-run]` | Parse CLAUDE.md into Doc entities |
| `filament audit [--branch NAME]` | Snapshot graph to git branch |
| `filament completions bash\|zsh\|fish` | Shell completions |

### Global Flags

`--json` (machine-readable) | `-v` (debug) | `-vv` (trace) | `-q` (quiet)

## Entity Types

| Type | Purpose | When to use |
|------|---------|-------------|
| `task` | Work items with status workflow, priority, deps | Bugs, features, work to track |
| `module` | Code structure (crates, files) | Relate tasks to code they touch |
| `service` | Runtime components | Databases, servers, infrastructure |
| `agent` | Actors (required for assign/message/reserve) | AI or human workers |
| `plan` | Planning docs (use `--content path.md`) | Group tasks via `owns` |
| `doc` | Reference material (use `--content path.md`) | ADRs, specs, runbooks |
| `lesson` | Knowledge capture (use `filament lesson add`) | Gotchas, patterns, solutions |

## Structured Errors

| Exit | Meaning |
|------|---------|
| 0 | Success |
| 2 | CLI argument error |
| 3 | Not found |
| 4 | Validation error |
| 5 | Database error |
| 6 | Resource conflict |
| 7 | I/O error |

With `--json`, errors include `code`, `message`, `hint`, `retryable` fields.

## Tips

- Priority: 0 = highest, 4 = lowest (default 2)
- `blocks` direction: `A blocks B` means B can't start until A closes
- `task list --status` and `--unblocked` cannot be combined
- Daemon routes all CLI commands through Unix socket for concurrent access
- `FILAMENT_AUTO_DISPATCH=1` chains agent runs on newly-unblocked tasks
- Dual-track: keep CLAUDE.md/MEMORY.md in sync with filament entities
