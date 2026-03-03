# Filament

Local-only Rust tool for multi-agent orchestration, knowledge graph, task management, inter-agent communication, CLI + TUI.

## Project Layout

```
filament/
├── CLAUDE.md
├── Makefile                    # make fmt/check/build/test/run/migration/adr/ci
├── util-scripts/               # shell scripts backing Makefile targets
├── .plan/
│   ├── filament-v1.md          # master plan v1.1 (6 phases, 30+ tasks)
│   ├── phase1-core.md … phase6-integration.md
│   ├── test-standards.md       # layered test strategy
│   ├── benchmarks.md           # beads_rust + Flywheel analysis
│   ├── benchmarks-local.md     # workout-util + koupang patterns
│   ├── gotchas.md              # pitfalls & solutions (sqlx, thiserror, petgraph, etc.)
│   └── adr/                    # architecture decision records (001–018)
├── crates/
│   ├── filament-core/          # library: graph, storage, models, errors
│   ├── filament-cli/           # the single binary (clap), depends on core + daemon + tui
│   ├── filament-daemon/        # library: Unix socket server, MCP server
│   └── filament-tui/           # library: ratatui app
└── migrations/
```

## Architecture Decisions

Full ADRs with rationale: `.plan/adr/` (001–018). Key choices:

- **Hybrid daemon** — direct SQLite single-user, daemon for multi-agent (ADR-001)
- **Unified graph** — all data as Entity nodes + Relation edges (ADR-003)
- **Design for agent death** — TTL leases, no ringleaders, auto-cleanup (ADR-009)
- **Advisory file reservations** — no worktrees (ADR-008)
- **Targeted messaging only** — no broadcast (ADR-010)
- **MCP agent interface** — ecosystem standard (ADR-011)
- **Structured errors** — machine-readable codes, hints, retryable (ADR-007)
- **Value types** — Priority, Weight, NonEmptyString etc. make invalid states unrepresentable (ADR-018)

## Stack

- Rust (cargo workspace, 4 crates, single binary, stable toolchain) — see ADR-017
- sqlx (sqlite, runtime-tokio) — persistent storage
- petgraph — in-memory graph traversal + intelligence
- tokio — async runtime, process spawning, Unix socket server
- clap (derive) — CLI argument parsing
- thiserror — structured error types
- schemars — JSON Schema for MCP/agent integration
- ratatui + crossterm — TUI
- serde + serde_json — serialization, JSON-RPC protocol
- tracing — structured logging
- blake3 — content file change detection
- chrono — timestamps

## Key Concepts

- **Three-tier content**: summary (cheap traversal) → key_facts (LLM reasoning) → content_path (full reference material on disk)
- **AgentResult protocol**: subprocesses (`claude -p`) emit JSON with status, artifacts, messages, blockers, questions. Filament parses and routes.
- **Per-project storage**: `filament init` creates `.filament/` with SQLite DB, Unix socket, PID file, content dir.

## Implementation Plan

- Master plan (phases, tasks, deps, file paths): `.plan/filament-v1.md`
- Phase sub-plans: `.plan/phase1-core.md` … `.plan/phase6-integration.md`
- Benchmark analysis: `.plan/benchmarks.md`, `.plan/benchmarks-local.md`
- Test standards: `.plan/test-standards.md`
- Architecture decisions: `.plan/adr/` (use `make adr TITLE="..."` to add new ones)

## Gotchas

See `.plan/gotchas.md` for the full list. Top hits:

- sqlx custom newtypes need `fn compatible()` override, not just `type_info()`
- `thiserror` v2 treats fields named `source` as error sources
- `with_transaction` requires `|conn| Box::pin(async move { ... })`
- petgraph 0.7 requires `use petgraph::visit::EdgeRef` for edge methods

## Current Status

**Phase 2 complete** (2026-03-03). Full CLI binary with all command groups:
- `filament init` — project initialization
- `filament add/remove/update/inspect/read/list` — entity CRUD (top-level)
- `filament relate/unrelate` — relation management
- `filament task add/list/ready/show/close/assign/critical-path` — task subgroup
- `filament context --around <name> --depth N` — graph neighborhood query
- `filament message send/inbox/read` — inter-agent messaging
- `filament reserve/release/reservations` — file reservation management
- Global flags: `--json`, `-v`/`-q` verbosity, structured error output
- 140 tests (83 core + 57 CLI integration), zero clippy warnings
- 3 code reviews + 2 manual QA rounds (65 test cases, results in `.qa/`)

## References

- beads_rust (task management + error patterns): https://github.com/Dicklesworthstone/beads_rust
- Flywheel ecosystem (multi-agent orchestration): https://github.com/Dicklesworthstone
- Claude Code orchestration patterns:
  - https://github.com/affaan-m/everything-claude-code
  - https://github.com/VoltAgent/awesome-claude-code-subagents
  - https://github.com/obra/superpowers
