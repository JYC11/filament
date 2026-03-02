# Filament — Master Plan v1.1

Local-only Rust tool for multi-agent orchestration, knowledge graph, task management, inter-agent communication, and TUI.

**Prior art**: `.plan/knowledge-graph-cli-plan.md`, `.plan/multi-agent-orchestration.md`
**Benchmarks**: `.plan/benchmarks.md` (beads_rust + Flywheel), `.plan/benchmarks-local.md` (workout-util + koupang)
**Test standards**: `.plan/test-standards.md` (layered test strategy adapted from koupang)

---

## Decisions Log

| Decision            | Choice                              | Rationale                                                                  |
| ------------------- | ----------------------------------- | -------------------------------------------------------------------------- |
| Architecture        | Hybrid daemon                       | Direct SQLite for single-user, daemon for multi-agent                      |
| Communication       | Unix socket + MCP + SQLite          | Socket for internal RPC, MCP for agent tool discovery, SQLite for durable async |
| Data model          | Unified graph                       | Tasks, knowledge, agents, messages = nodes; deps, relations, comms = edges |
| Task management     | Reimplemented as graph ops          | Not a br port — graph-native task features                                 |
| Agent dispatching   | Subprocess only                     | `claude -p` with structured JSON protocol                                  |
| Storage             | Per-project `.filament/` + SQLite (WAL) | `filament init` in any dir, data local to project                       |
| TUI                 | Start minimal, build up             | Task list + agent status first                                             |
| Name                | `filament`                          | Connecting agents/tasks/knowledge like threads (available on crates.io)    |
| Error design        | Structured errors (thiserror)       | Machine-readable codes, hints, retryable flags for agent consumers (from beads_rust) |
| File coordination   | Advisory reservations with TTL      | No worktrees — leases expire on agent death (from Flywheel)                |
| Agent resilience    | Design for death                    | TTL leases, no ringleaders, no single points of failure (from Flywheel)    |
| Messaging           | Targeted only (no broadcast)        | Agents must address specific recipients to prevent context pollution (from Flywheel) |
| Agent interface     | MCP server on daemon                | Ecosystem standard — agents discover tools via MCP (from Flywheel)         |
| Lint config         | `unsafe_code = "forbid"`, pedantic  | Strict safety baseline (from beads_rust)                                   |
| Schema invariants   | DB-level CHECK constraints          | Enforce lifecycle rules in SQLite, not just app code (from beads_rust)     |
| Toolchain           | Stable Rust                         | Avoid nightly-only features (lesson from beads_rust)                       |

---

## Project Structure

Single binary (`filament`) — see [ADR-017](adr/017-single-binary-distribution.md).

```
filament/
├── Cargo.toml                  # workspace root (lint config, profiles)
├── crates/
│   ├── filament-core/          # library: graph, storage, models, errors
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs       # Entity, Relation, AgentResult, Reservation, Message (non-graph)
│   │       ├── error.rs        # FilamentError (thiserror) + StructuredError
│   │       ├── schema.rs       # SQLite DDL, migrations, CHECK constraints
│   │       ├── store.rs        # FilamentStore + MutationContext
│   │       ├── graph.rs        # petgraph wrapper, traversal, graph intelligence
│   │       ├── connection.rs   # Connection enum: Direct | Socket
│   │       └── protocol.rs     # JSON-RPC types + MCP tool definitions
│   ├── filament-cli/           # the single binary (clap), depends on core + daemon + tui
│   ├── filament-daemon/        # library: Unix socket server, MCP server, serve() entrypoint
│   └── filament-tui/           # library: ratatui app, run_tui() entrypoint
├── migrations/
│   └── 001_init.sql
└── .filament/                  # per-project runtime data (created by `filament init`)
    ├── filament.db
    ├── filament.sock
    ├── filament.pid
    └── content/
```

### Distribution

- `cargo install filament` — publishes filament-cli crate as `filament` binary
- GitHub Releases — pre-built binaries per platform (CI cross-compile)
- `curl -fsSL https://filament.dev/install.sh | sh` — install script
- Homebrew (future): `brew install filament`

---

## Phase Index

| Phase | Sub-plan | Goal | Depends on |
|-------|----------|------|------------|
| 1 | [phase1-core.md](phase1-core.md) | filament-core library: models, errors, schema, store, graph, connection, protocol | — |
| 2 | [phase2-cli.md](phase2-cli.md) | CLI binary: entity, task, relation, query, message, reserve commands | Phase 1 |
| 3 | [phase3-daemon.md](phase3-daemon.md) | Daemon: Unix socket server, MCP server, auto-start | Phase 1 |
| 4 | [phase4-dispatch.md](phase4-dispatch.md) | Agent dispatching: subprocess management, roles, safety, batch dispatch | Phase 3 |
| 5 | [phase5-tui.md](phase5-tui.md) | TUI: task list, agent status, reservation views | Phase 1 |
| 6 | [phase6-integration.md](phase6-integration.md) | Integration: context bundles, escalation, export/import, docs | All |

```
Phase 1 (core)
  ├──→ Phase 2 (CLI)         ← self-tracking milestone: when task CRUD works, import this plan
  ├──→ Phase 3 (daemon + MCP)
  │       └──→ Phase 4 (dispatching + reservations)
  └──→ Phase 5 (TUI)
                └──→ Phase 6 (integration)
```

Phases 2, 3, and 5 can progress in parallel once Phase 1 is done.
Phase 4 requires Phase 3 (agents need MCP to call filament tools).
Phase 6 requires everything.

---

## Crate Dependencies

### filament-core

```toml
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
petgraph = "0.7"
tokio = { version = "1", features = ["sync", "net", "io-util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
schemars = "1"
tracing = "0.1"
blake3 = "1"
chrono = { version = "0.4", features = ["serde"] }
```

### filament-cli (the single binary)

```toml
filament-core = { path = "../filament-core" }
filament-daemon = { path = "../filament-daemon" }
filament-tui = { path = "../filament-tui" }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

### filament-daemon (library)

```toml
filament-core = { path = "../filament-core" }
tokio = { version = "1", features = ["full"] }
# MCP server — evaluate rmcp or implement minimal HTTP+JSON manually
```

### filament-tui (library)

```toml
filament-core = { path = "../filament-core" }
ratatui = "0.29"
crossterm = "0.28"
tokio = { version = "1", features = ["full"] }
```

### Workspace Cargo.toml

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
must_use_candidate = "allow"

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

---

## Open Items (defer to later iterations)

- Change notifications over socket (subscribe/push model)
- Semantic search with embeddings
- Graph visualization in TUI (ASCII graph rendering)
- Config file (`~/.filament/config.toml`) for defaults
- `filament seed` — auto-populate graph from CLAUDE.md, git log, existing br data
- Conflict resolution policies for concurrent entity updates
- Git audit trail — dual persistence (SQLite + Git) for resilience (from Flywheel)
- Intent detection / fuzzy matching for CLI args (from beads_rust)
- Shell completions with dynamic entity ID completion (from beads_rust)
- Agent context budget monitoring — track % used, auto-clear when low (from Flywheel)
- Pre-commit hook that checks file reservations before allowing commits (from Flywheel)
- Graph analytics: PageRank for high-impact entities, betweenness centrality (from beads_viewer)
