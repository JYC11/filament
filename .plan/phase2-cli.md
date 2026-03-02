# Phase 2: CLI (filament-cli)

**Goal**: usable CLI that works in direct mode (no daemon). Once task CRUD works, import this plan into filament itself for self-tracking.

**Master plan**: [filament-v1.md](filament-v1.md)
**Depends on**: [Phase 1](phase1-core.md)

---

## 2.1 — CLI skeleton + clap setup

- File: `filament-cli/src/main.rs`, `filament-cli/src/commands/mod.rs`
- Top-level commands: `entity`, `task`, `relate`, `context`, `inspect`, `read`, `list`, `agent`, `message`, `reserve`, `serve`, `tui`, `init`
- Global flags:
  ```rust
  #[arg(long, global = true)] json: bool,      // JSON output for agent consumption
  #[arg(long, global = true)] no_daemon: bool,  // Force direct mode
  #[arg(short, long, action = Count, global = true)] verbose: u8,
  #[arg(short, long, global = true)] quiet: bool,
  ```
- `filament init` creates `.filament/` directory and database
- Error output: human-friendly by default, `--json` emits `StructuredError` JSON to stderr
- Blocked by: 1.1

## 2.2 — Entity commands

- File: `filament-cli/src/commands/entity.rs`
- `filament add <name> --type <type> --summary "..." [--facts '{}'] [--content ./path.md]`
- `filament remove <name>`
- `filament update <name> [--summary "..."] [--facts '{}']`
- `filament inspect <name> [<name2> ...]` — tier-2 key_facts
- `filament read <name>` — tier-3 full content
- Blocked by: 1.5, 1.7, 2.1

## 2.3 — Relation commands

- File: `filament-cli/src/commands/relation.rs`
- `filament relate <source> <relation_type> <target> [--summary "..."] [--weight 1.0]`
- `filament unrelate <source> <relation_type> <target>`
- Blocked by: 1.5, 1.7, 2.1

## 2.4 — Task commands

- File: `filament-cli/src/commands/task.rs`
- `filament task add <title> --summary "..." [--priority N] [--blocks <other>] [--depends-on <other>]`
- `filament task list [--status open|closed|all] [--unblocked]`
- `filament task ready [--limit N]` — ranked unblocked tasks (uses graph intelligence)
- `filament task show <name>`
- `filament task close <name>`
- `filament task assign <name> --to <agent-name>`
- `filament task critical-path <name>` — show dependency chain
- Tasks are entities with `entity_type = "task"`, dependencies are `blocks`/`depends_on` relations
- `--unblocked` filter: tasks where no `depends_on` target has `status != "closed"`
- Blocked by: 1.5, 1.6, 1.7, 2.1

## 2.5 — Query commands

- File: `filament-cli/src/commands/query.rs`
- `filament context --around <name> --depth <N> [--type <type>] [--limit 20]`
- `filament list [--type <type>] [--status <status>]`
- JSON output to stdout, errors to stderr
- Blocked by: 1.5, 1.6, 1.7, 2.1

## 2.6 — Message commands

- File: `filament-cli/src/commands/message.rs`
- `filament message send --from <agent> --to <agent> --body "..."` (targeted only — `--to` required)
- `filament message inbox <agent> [--unread]`
- `filament message outbox <agent>`
- No broadcast endpoint — agents must address specific recipients
- Blocked by: 1.5, 1.7, 2.1

## 2.7 — Reservation commands

- File: `filament-cli/src/commands/reserve.rs`
- `filament reserve <glob> --agent <name> [--exclusive] [--ttl 3600]` — acquire advisory lease
- `filament release <glob> --agent <name>` — release lease
- `filament reservations [--agent <name>]` — list active reservations
- `filament reservations --expired --clean` — remove stale reservations
- Blocked by: 1.5, 1.7, 2.1

## 2.8 — Tests for Phase 2

- Integration tests: run CLI commands against temp SQLite, verify output
- Task workflow test: create tasks, add dependencies, list unblocked, close, verify cascade
- Reservation workflow test: reserve, conflict detection, release, expiry
- `--json` output test: verify StructuredError format on errors
- Blocked by: 2.2–2.7

---

## Self-tracking milestone

Once tasks 2.1 + 2.4 are working (`filament init`, `filament task add/list/close/ready`), import all remaining plan tasks into filament's own `.filament/` directory. From that point forward, track the rest of the build using the tool itself.

---

## Task Dependency Graph

```
2.1 (skeleton + init)
 ├──→ 2.2 (entity cmds)
 ├──→ 2.3 (relation cmds)
 ├──→ 2.4 (task cmds)      ← self-tracking milestone
 ├──→ 2.5 (query cmds)
 ├──→ 2.6 (message cmds)
 └──→ 2.7 (reservation cmds)

2.2–2.7 ──→ 2.8 (tests)
```

All of 2.2–2.7 depend on Phase 1 (store, connection) + 2.1 (CLI skeleton).
