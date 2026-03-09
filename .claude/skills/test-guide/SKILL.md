---
name: test-guide
description: >
  Guide for writing tests following the project's layered test standards. Use when the user says
  "write tests", "add tests", "test strategy", "what tests to write", "test this module",
  "where should this test go", or is deciding which test layer to use.
---

# Filament Test Standards

Adapted from koupang's layered test guide. Each test justifies its infrastructure cost. No duplicate assertions across layers.

## Decision Flowchart

```
Input validation, normalization?              → Model unit test
SQL behavior (constraints, FKs, queries)?     → Store test
Graph traversal, intelligence, cycle detect?  → Graph test
CLI output, exit codes, JSON format?          → CLI integration test
Socket protocol, MCP tools, concurrency?      → Daemon integration test
Agent dispatch, result parsing, resilience?   → Dispatch integration test
TUI rendering, layout?                        → TUI snapshot test
Happy-path CRUD workflow?                     → CLI integration test (covers all layers)
```

## Layer Guide

### 1. Models — Pure Unit Tests
**Infra:** None (`#[test]`, sync, no I/O)
**Test:** validation rules (`TryFrom`), normalization, rejection messages, boundary values, enum serde round-trips, `typed_id!` uniqueness, `StructuredError` JSON format
**Skip:** anything requiring a database or async runtime

### 2. Store — SQL Correctness
**Infra:** In-memory SQLite (`sqlite::memory:`), fresh DB per test via `test_db()`
**Test:** schema constraints (UNIQUE, CHECK, FK cascades), `with_transaction` semantics (commit/rollback), `mutate()` pipeline (event recording, cache rebuild), complex queries (ready_tasks ordering, filtered listing), reservation SQL (acquire, conflict, TTL expiry, stale cleanup), message queries (unread by agent)
**Skip:** CRUD happy paths (covered by CLI integration tests)

### 3. Graph — In-Memory Graph Logic
**Infra:** In-memory SQLite + `KnowledgeGraph` hydrated from store
**Test:** hydration correctness, BFS/DFS traversal with depth limits, `ready_tasks()` ordering, `critical_path()` calculation, `impact_score()`, cycle detection, graph sync after store mutations
**Skip:** SQL details (store tests), CLI output format

### 4. CLI Integration — Command Contract
**Infra:** Temp directory with `fl init`, run CLI binary via `assert_cmd`
**Test:** happy-path CRUD workflows (entity, task, relation, message, reservation), exit codes (0 success, categorized non-zero), `--json` flag (machine-readable output + `StructuredError`), multi-step flows (create tasks → add deps → list ready → close → verify cascade)
**Skip:** SQL constraint details (store tests), graph intelligence (graph tests), model validation (model tests)

### 5. Daemon Integration — Socket & MCP Contract
**Infra:** Start daemon in test, connect via Unix socket
**Test:** socket lifecycle, JSON-RPC round-trip, MCP tool exposure, concurrent readers, write serialization, auto-start detection, stale reservation cleanup
**Skip:** CRUD logic (store tests), CLI output format

### 6. Dispatch Integration — Agent Lifecycle
**Infra:** Mock `claude -p` with shell script emitting `AgentResult` JSON
**Test:** dispatch → parse → route → update cycle, batch dispatch with dependency chains, agent death resilience (reservations released), reservation conflict detection
**Skip:** subprocess internals, CLI output

### 7. TUI — Snapshot Tests
**Infra:** ratatui test helpers (terminal backend mock)
**Test:** view rendering with sample data, layout at different terminal sizes
**Skip:** data loading, business logic

## Test Infrastructure

### Feature-gated test-utils (never in prod)

```toml
# filament-core/Cargo.toml
[features]
default = []
test-utils = []

[dev-dependencies]
filament-core = { path = ".", features = ["test-utils"] }
```

```rust
// filament-core/src/test_utils.rs
#[cfg(feature = "test-utils")]
pub mod test_utils {
    use crate::store::FilamentStore;

    pub async fn test_db() -> FilamentStore {
        FilamentStore::new("sqlite::memory:").await.unwrap()
    }

    pub fn sample_entity() -> EntityCreateReq { /* defaults */ }
    pub fn sample_task() -> EntityCreateReq { /* entity_type = Task */ }
    pub fn sample_relation(source: &str, target: &str) -> RelationCreateReq { /* defaults */ }
    pub fn sample_message(from: &str, to: &str) -> MessageCreateReq { /* defaults */ }
    pub fn sample_reservation(agent: &str, glob: &str) -> ReservationCreateReq { /* defaults */ }
}
```

### CLI integration helper

```rust
// filament-cli/tests/common/mod.rs
use assert_cmd::Command;
use tempfile::TempDir;

pub fn filament_cmd(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("filament").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

pub fn init_project(dir: &TempDir) {
    filament_cmd(dir).arg("init").assert().success();
}
```

## Order for New Modules

1. Model unit tests (fast feedback, no infra)
2. Store tests for SQL-specific concerns only
3. Graph tests if feature involves traversal or intelligence
4. CLI integration tests for all commands (canonical happy-path + output contract)
5. Daemon/dispatch tests when those phases are implemented
6. **Verify no test duplicates an assertion at another layer**

## Anti-Patterns

- **Don't test CRUD happy paths in store tests** — CLI integration tests cover the full stack
- **Don't test SQL constraints in CLI tests** — store tests own constraint verification
- **Don't test model validation in store or CLI tests** — model unit tests own validation
- **Don't mock the store** — in-memory SQLite IS the mock
- **Don't share DB state between tests** — fresh `test_db()` per test
- **Don't test internal error messages in CLI tests** — test exit codes and `--json` structure
