# Phase 5: TUI (Minimal, filament-tui library)

**Goal**: basic terminal UI showing tasks, agent status, and reservations. Invoked via `filament tui` subcommand (single binary — [ADR-017](adr/017-single-binary-distribution.md)).

**Master plan**: [filament-v1.md](filament-v1.md)
**Depends on**: [Phase 1](phase1-core.md) (connection abstraction)

---

## 5.1 — TUI app skeleton

- File: `filament-tui/src/lib.rs` (library, not binary), `filament-tui/src/app.rs`
- Exports `pub async fn run_tui(connection: FilamentConnection) -> Result<()>` entrypoint
- Called by `filament tui` subcommand in filament-cli
- Connects via FilamentConnection (direct or socket)
- Event loop: keyboard input + periodic data refresh
- Blocked by: 1.7

## 5.2 — Task list view

- File: `filament-tui/src/views/tasks.rs`
- Table: name, status, priority, blocked-by count, assigned agent, impact score
- Keyboard: j/k navigate, Enter for detail, c to close, n to create
- Filter bar: status, type
- Blocked by: 5.1

## 5.3 — Agent status view

- File: `filament-tui/src/views/agents.rs`
- Running agents: task name, role, duration, PID, file reservations held
- Recent completions: task, result status, summary
- Blocked by: 5.1

## 5.4 — Reservation view

- File: `filament-tui/src/views/reservations.rs`
- Active reservations: agent, glob, exclusive flag, time remaining
- Expired/stale reservations highlighted
- Blocked by: 5.1

## 5.5 — Tests for Phase 5

- TUI snapshot tests (ratatui test helpers)
- Blocked by: 5.2, 5.3, 5.4

---

## Task Dependency Graph

```
5.1 (skeleton)
 ├──→ 5.2 (task view)
 ├──→ 5.3 (agent view)
 └──→ 5.4 (reservation view)

5.2, 5.3, 5.4 ──→ 5.5 (tests)
```
