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
│   ├── filament-daemon/        # library: Unix socket server, handler/{entity,relation,message,reservation,agent_run,graph,event}
│   └── filament-tui/           # library: ratatui app
└── migrations/
```

## Architecture Decisions

Full ADRs with rationale: `.plan/adr/` (001–020). Key choices:

- **Hybrid daemon** — direct SQLite single-user, daemon for multi-agent (ADR-001)
- **Unified graph** — all data as Entity nodes + Relation edges (ADR-003)
- **Design for agent death** — TTL leases, no ringleaders, auto-cleanup (ADR-009)
- **Advisory file reservations** — no worktrees (ADR-008)
- **Targeted messaging only** — no broadcast (ADR-010)
- **MCP agent interface** — ecosystem standard (ADR-011)
- **Structured errors** — machine-readable codes, hints, retryable (ADR-007)
- **Value types** — Priority, Weight, NonEmptyString etc. make invalid states unrepresentable (ADR-018)
- **Slug identity** — 8-char base36 slugs replace name-based lookup (ADR-019)
- **Entity ADT** — tagged enum replaces flat struct, compile-time type safety (ADR-020)

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

- **Entity model**: `Entity` is a tagged enum (`Task | Module | Service | Agent | Plan | Doc`) wrapping `EntityCommon`. Each entity has a unique 8-char slug (`[a-z0-9]`) for human-facing identity, plus a UUID for internal use. Resolution: slug first, UUID fallback.
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

## Dual-Track Project Management

This project uses **both** traditional `.md` files and filament's own knowledge graph. Keep both in sync.

| Concern | Old way (.md files) | New way (filament CLI) |
|---------|--------------------|-----------------------|
| Plans & phases | `.plan/filament-v1.md`, `phase*.md` | `filament list --type plan` |
| Tasks & deps | Manual tracking in MEMORY.md | `filament task ready`, `filament task critical-path` |
| Architecture | `.plan/adr/*.md` | `filament list --type doc`, `filament context --around <adr>` |
| Code structure | This file's Project Layout section | `filament list --type module`, `filament context --around <module>` |
| What's next | MEMORY.md "Next Steps" | `filament task ready` |
| Gotchas | `.plan/gotchas.md` | `filament inspect gotchas`, `filament read gotchas` |

**Rules:**
- When creating/closing tasks, do it in filament AND update MEMORY.md
- When adding ADRs or plans, create both the `.md` file and a filament entity with `--content` pointing to it
- When finishing a phase, `filament task close <phase-task>` and update Current Status below
- `.filament/` is gitignored (local per-user DB) — `.md` files remain the committed source of truth
- Use `filament task ready` to decide what to work on next

## Current Status

**Phases 1–4 complete** (2026-03-03):
- Phase 1: Core library — models, errors, schema, store, graph, connection, protocol
- Phase 2: CLI — entity, task, relation, query, message, reserve commands (54 integration tests)
- Phase 3: Daemon — NDJSON Unix socket server + MCP server (16 tools via `rmcp`)
- Phase 4: Agent dispatching — dispatch engine, roles, CLI commands, death cleanup
- `filament serve [--foreground]` / `filament stop` / `filament mcp`
- `filament agent dispatch|dispatch-all|status|list|history`
- CLI routes through daemon when running (falls back to direct DB access)
- **Agent roles**: Coder, Reviewer, Planner, Dockeeper with compiled-in prompts and tool whitelists
- **Dispatch engine**: spawn subprocess via `std::process`, monitor via `tokio::spawn` + `spawn_blocking`, parse `AgentResult` JSON, route messages, death cleanup (revert task, release reservations, refresh graph)
- **No server-side batch dispatch** — CLI `dispatch-all` loops individual `dispatch_agent` RPCs to avoid child process reaping races
- **Slug-based identity** (ADR-019): 8-char `[a-z0-9]` slugs replace name-based lookup
- **Entity ADT** (ADR-020): `Entity` enum with typed variants, `TypeMismatch` error, compile-time type safety
- 212 tests (105 core + 58 CLI + 39 daemon + 10 MCP), zero clippy warnings
- **Next**: Phase 5 — TUI

## References

- beads_rust (task management + error patterns): https://github.com/Dicklesworthstone/beads_rust
- Flywheel ecosystem (multi-agent orchestration): https://github.com/Dicklesworthstone
- Claude Code orchestration patterns:
  - https://github.com/affaan-m/everything-claude-code
  - https://github.com/VoltAgent/awesome-claude-code-subagents
  - https://github.com/obra/superpowers
