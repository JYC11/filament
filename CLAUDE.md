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
│   └── adr/                    # architecture decision records (001–016)
├── crates/
│   ├── filament-core/          # shared library: graph, storage, models, errors
│   ├── filament-cli/           # CLI binary (clap)
│   ├── filament-daemon/        # daemon binary (Unix socket + MCP server)
│   └── filament-tui/           # TUI binary (ratatui)
└── migrations/
```

## Architecture Decisions

Full ADRs with rationale: `.plan/adr/` (001–016). Key choices:

- **Hybrid daemon** — direct SQLite single-user, daemon for multi-agent (ADR-001)
- **Unified graph** — all data as Entity nodes + Relation edges (ADR-003)
- **Design for agent death** — TTL leases, no ringleaders, auto-cleanup (ADR-009)
- **Advisory file reservations** — no worktrees (ADR-008)
- **Targeted messaging only** — no broadcast (ADR-010)
- **MCP agent interface** — ecosystem standard (ADR-011)
- **Structured errors** — machine-readable codes, hints, retryable (ADR-007)

## Stack

- Rust (cargo workspace, 4 crates, stable toolchain)
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

## Current Status

**Phase: Planning complete (v1.1 with benchmark revisions), implementation not started.**

## References

- beads_rust (task management + error patterns): https://github.com/Dicklesworthstone/beads_rust
- Flywheel ecosystem (multi-agent orchestration): https://github.com/Dicklesworthstone
- Claude Code orchestration patterns:
  - https://github.com/affaan-m/everything-claude-code
  - https://github.com/VoltAgent/awesome-claude-code-subagents
  - https://github.com/obra/superpowers
