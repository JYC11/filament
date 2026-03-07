# Filament

Local-only Rust tool for multi-agent orchestration, knowledge graph, task management, inter-agent communication, CLI + TUI.

## Context Management (MANDATORY)

- **Minimal onboarding.** At session start, read only CLAUDE.md + MEMORY.md + `fl task ready`. Do NOT explore the codebase, read source files, or run extra commands until you have a specific task.
- **Route through docs first.** Before grepping or reading source, check if the answer is already in CLAUDE.md, MEMORY.md, `fl inspect <slug>`, or `fl lesson list`. These are the index — source code is the last resort.
- **Filament is the second brain.** `.md` files are the committed summary; filament's knowledge graph holds the full context (entities, relations, lessons, task deps). Use `fl context --around <slug>` for neighborhood context, `fl search <query>` for free-text lookup.
- **Target <50% context window per task.** If a task will consume more than half the context window (including exploration + implementation + testing), split it into smaller tasks first. Every task should complete with room to spare.
- **No speculative reads.** Do not read files "just in case". Each tool call should have a clear purpose tied to the current task. Minimize exploration — pattern-match from docs and existing modules.

## Project Layout

```
filament/
├── CLAUDE.md
├── Makefile                    # make fmt/check/build/test/run/migration/adr/ci
├── util-scripts/               # shell scripts backing Makefile targets
├── .plan/
│   ├── filament-v1.md          # master plan v1.1 (all 6 phases complete)
│   ├── gotchas.md              # pitfalls & solutions (sqlx, thiserror, petgraph, etc.)
│   └── adr/                    # architecture decision records (001–023)
├── .qa/                        # QA results + simulation logs
├── crates/
│   ├── filament-core/          # library: graph, storage, models, errors
│   ├── filament-cli/           # the single binary (clap), depends on core + daemon + tui
│   ├── filament-daemon/        # library: Unix socket server, handler/{entity,relation,message,reservation,agent_run,graph,event}
│   └── filament-tui/           # library: ratatui app
└── migrations/
```

## Architecture Decisions

Full ADRs with rationale: `.plan/adr/` (001–023). Key choices:

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
- **Lesson knowledge capture** — structured problem/solution/pattern/learned fields (ADR-021)
- **Optimistic conflict resolution** — version checks, auto-merge non-overlapping, field-level resolve (ADR-022)
- **Typed entity DTOs** — CreateEntityRequest/EntityChangeset as ADTs enforce content_path policy per type (ADR-023)

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

- **Entity model**: `Entity` is a tagged enum (`Task | Module | Service | Agent | Plan | Doc | Lesson`) wrapping `EntityCommon`. Each entity has a unique 8-char slug (`[a-z0-9]`) for human-facing identity, plus a UUID for internal use. Resolution: slug first, UUID fallback.
- **Lesson entities**: Gotchas, solutions, and recurring problems go in Lesson entities (not Doc). Structured fields (`problem`, `solution`, `pattern`, `learned`) stored in `key_facts` JSON, accessed via `LessonFields` struct. CLI: `fl lesson add/list/show`.
- **Three-tier content**: summary (cheap traversal) → key_facts (LLM reasoning) → content_path (full reference material on disk)
- **AgentResult protocol**: subprocesses (`claude -p`) emit JSON with status, artifacts, messages, blockers, questions. Filament parses and routes.
- **Per-project storage**: `fl init` creates `.fl/` with SQLite DB, Unix socket, PID file, content dir.

## Plans & References

- Master plan: `.plan/filament-v1.md`
- Gotchas: `.plan/gotchas.md`
- Architecture decisions: `.plan/adr/` (use `make adr TITLE="..."` to add new ones)
- QA results & simulation logs: `.qa/`

## Development Rules

- **Every bug fix must include a test** — if a bug is found, write a regression test that would have caught it before fixing the implementation.
- Tests gate completion — always run `make test CRATE=all` after changes.
- Never weaken or modify a test to make it pass — the bug is in the implementation, not the test.
- **Code Smells**
  - god functions are bad
  - overly fragmented functions are bad
  - circular dependencies are bad, dependencies should be acyclic and unidirectional
  - pass through methods: a method that only invokes another method and does nothing else is bad
- **Development Heuristics**
  - favour using ADTs, value objects, rich domain models to make illegal states unrepresentable
  - threshold for abstraction 4+ (DRY principle), 1-3 times no abstraction (YAGNI)
    - if unsure, repeat a bit more until abstraction becomes very obvious
  - favour efficient SQL queries, the less I/O the better
    - use batch `WHERE IN` to get many records instead of N+1 loops
    - do SQL queries the simple way a couple times before finding patterns you can make efficient
  - modules should be deep (strong functionality but simple interfaces) and minimize unnecessary information from being shown to the user of the modules
  - different layer, different abstraction — each layer in `CLI → handler → store` should operate at a distinct level of abstraction; if two adjacent layers use the same vocabulary, one is probably unnecessary
  - define errors out of existence — prefer validated newtypes and type states so error cases can't happen; handle the remaining errors that types can't prevent
  - in an unsure problem area, employ tactical programming (get things done focus) to figure out patterns then do a clean up of them after with strategic programming (long term maintenance focus)
  - complexity is incremental — each small shortcut compounds; when cleaning up tactical code, fix the small things too
  - too much specialization of purpose can make the code too complicated
  - comment only things that are not obvious from the code
- **Error Handling**
  - store/graph layers return `Result<T, FilamentError>` with structured error enums
  - `unwrap`/`expect` only in tests and provably infallible cases (e.g., compiled regex)
  - never silently swallow errors
- **Naming Conventions**

  | Element | Convention | Example |
  |---------|-----------|---------|
  | Entity variants | PascalCase noun | `Task`, `Module`, `Agent`, `Lesson` |
  | CLI commands | verb-noun kebab | `task ready`, `agent dispatch` |
  | Store methods | `verb_noun` snake_case | `create_entity`, `get_entity` |
  | Handler functions | `handle_verb_noun` snake_case | `handle_create_entity` |
  | Domain models | Plain noun PascalCase | `Entity`, `Relation`, `Message` |
  | Value types | PascalCase | `NonEmptyString`, `Slug`, `Priority` |
  | Error variants | PascalCase descriptive | `EntityNotFound`, `TypeMismatch` |
  | Migrations | `NNNN_descriptive_name` | `0001_initial_schema.sql` |

- **Git Workflow**
  - commit after completing a logical unit of work (feature, fix, refactor) — not after every file edit
  - do not push unless explicitly asked
  - branch naming: `feat/short-description`, `fix/short-description`, `refactor/short-description`
  - commit messages: imperative mood, `type: description` (e.g., `feat: add export command`, `fix: prevent duplicate relations`)
- **Refactoring Scope**
  - refactor in the same PR if <30 min of work; otherwise create a follow-up task
  - tactical code is fine during exploration; clean up before the feature is "done"
- if any of the development rules are not clear, ESCALATE = stop and ask the user before proceeding

## Gotchas & Lessons

New gotchas and solutions should be recorded as **Lesson entities** (`fl lesson add`), not Doc entities. Legacy gotchas remain in `.plan/gotchas.md`. Top hits:

- sqlx custom newtypes need `fn compatible()` override, not just `type_info()`
- `thiserror` v2 treats fields named `source` as error sources
- `with_transaction` requires `|conn| Box::pin(async move { ... })`
- petgraph 0.7 requires `use petgraph::visit::EdgeRef` for edge methods
- SQLite cannot ALTER CHECK constraints — must recreate table in migrations

## Task Tracking

This project uses **filament itself** for task management, with `.md` files as the committed source of truth.

- **What's next**: `fl task ready`
- **Start work**: `fl update <slug> --status in_progress`
- **Finish work**: `fl task close <slug>`
- **New tasks**: `fl task add <name> --summary "..." --priority N`
- **Dependencies**: `fl relate <blocker> blocks <blocked>`
- **Full backlog**: `fl task list`
- **Lessons/gotchas**: `fl lesson list`, `fl lesson show <slug>`

**Rules:**
- When creating/closing tasks, do it in filament AND update MEMORY.md
- When adding ADRs or plans, create both the `.md` file and a filament entity with `--content` pointing to it
- `.fl/` is gitignored (local per-user DB) — `.md` files remain the committed source of truth

## Current Status

**All 7 phases complete + agent hardening** (2026-03-07). 529 tests, zero clippy warnings.

| Phase | What | Key details |
|-------|------|-------------|
| 1 | Core library | models, errors, schema, store, graph, connection, protocol |
| 2 | CLI | entity, task, relation, query, message, reserve, export, import, escalations |
| 3 | Daemon | NDJSON Unix socket server + MCP server (16 tools via `rmcp`) |
| 4 | Dispatch | subprocess management, roles (Coder/Reviewer/Planner/Dockeeper), death cleanup |
| 5 | TUI | 6-tab dashboard (entities, agents, reservations, messages, config, analytics), detail pane, paging, filters |
| 6 | Integration | context bundles, auto-dispatch, escalation routing, export/import |
| 7 | Small features | config file, watch, graph analytics, hooks, seed, audit, completions |

Key architectural features:
- **Slug identity** (ADR-019): 8-char `[a-z0-9]` slugs for human-facing identity
- **Entity ADT** (ADR-020): tagged enum with 7 typed variants, compile-time type safety
- **Lesson entities** (ADR-021): structured knowledge capture — gotchas, solutions, patterns
- **CLI routes through daemon** when running (falls back to direct DB access)
- **Auto-dispatch**: `FILAMENT_AUTO_DISPATCH=1` chains agent runs on newly-unblocked tasks
- **Escalations**: blockers/questions from agents routed as messages to "user"
- **Config file** (`fl.toml`): layered resolution (defaults → config → env → CLI)
- **Socket notifications**: pub/sub via `fl watch` for real-time entity change events
- **Graph analytics**: `PageRank` + degree centrality via `fl pagerank`/`fl degree`
- **Pre-commit hooks**: `fl hook install` for reservation conflict checks
- **Seed command**: `fl seed --file PATH` / `--files LIST` creates Doc entities from files
- **Audit trail**: `fl audit` snapshots knowledge graph to a git branch
- **Agent timeout**: `agent_timeout_secs` (default 1h) kills long-running agents via SIGTERM→SIGKILL
- **Dead agent reconciliation**: daemon periodically checks PIDs, cleans up crashed agent runs
