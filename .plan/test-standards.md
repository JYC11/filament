# Filament — Test Standards

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
**Test:**
- `typed_id!` generation uniqueness, Display/FromStr round-trip
- `TryFrom` validation: reject invalid entity types, empty names, bad status transitions
- Enum serialization: `EntityType`, `RelationType`, `EntityStatus` → TEXT round-trip
- `AgentResult` deserialization from sample JSON payloads
- `StructuredError` serialization: codes, hints, retryable flags, JSON format
- Boundary values: max-length names, empty key_facts, priority bounds

**Skip:** anything requiring a database or async runtime

### 2. Store — SQL Correctness
**Infra:** In-memory SQLite (`sqlite::memory:`), fresh DB per test via `test_db()`
**Test:**
- Schema constraints: UNIQUE on entity names (if applicable), CHECK constraints (status lifecycle, expires_at > created_at), FK cascades (delete entity → delete relations)
- `with_transaction` semantics: auto-commit on Ok, auto-rollback on Err
- `mutate()` pipeline: event recording, blocked cache rebuild triggers
- Complex queries: filtered entity listing, ready_tasks SQL ordering (priority + created_at)
- Reservation SQL: acquire exclusive, conflict detection, TTL expiry, stale cleanup
- Message queries: unread by agent, status transitions (unread → read)
- Blocked entity cache: rebuild correctness after dependency changes

**Skip:** CRUD happy paths (covered by CLI integration tests), graph traversal logic

### 3. Graph — In-Memory Graph Logic
**Infra:** In-memory SQLite + `KnowledgeGraph` hydrated from store
**Test:**
- Hydration: load entities/relations from store, verify node/edge counts match
- BFS/DFS traversal with depth limits
- Context query: summaries within N hops
- `ready_tasks()`: ordering by priority, respects dependency chains
- `critical_path()`: longest chain calculation, updates on graph changes
- `impact_score()`: in-degree + transitive dependent count
- Cycle detection: adding cyclic dependency returns `CycleDetected` error
- Graph sync: node/edge upserts after store mutations stay consistent

**Skip:** SQL details (covered by store tests), CLI output format

### 4. CLI Integration — Command Contract
**Infra:** Temp directory with `filament init`, run CLI binary as subprocess or via `assert_cmd`
**Test:**
- `filament init`: creates `.filament/` dir with db, runs migrations
- CRUD workflows: `entity add` → `entity list` → `entity show` → `entity delete`
- Task workflow: create tasks → add dependency → `task ready` → close → verify cascade
- Reservation workflow: `reserve` → conflict on overlap → `release` → verify freed
- Message workflow: `msg send` → `msg list --unread` → verify delivery
- `--json` flag: verify machine-readable JSON output on success and `StructuredError` on errors
- Exit codes: 0 for success, categorized non-zero per `FilamentError::exit_code()`
- Error output: human-readable by default, structured JSON with `--json`

**Skip:** SQL constraint details (store tests), graph intelligence logic (graph tests), model validation rules (model tests)

### 5. Daemon Integration — Socket & MCP Contract
**Infra:** Start daemon in test, connect via Unix socket
**Test:**
- Socket lifecycle: start, accept connections, shutdown cleanly
- JSON-RPC protocol: request → response round-trip, error responses
- MCP tool exposure: each tool callable, response matches schema
- Concurrency: multiple readers simultaneously (no corruption)
- Write serialization: two clients writing, verify consistency
- Auto-start: CLI detects running daemon via socket, routes through it
- Stale reservation cleanup on daemon tick

**Skip:** CRUD logic (store tests), CLI output format (CLI tests)

### 6. Dispatch Integration — Agent Lifecycle
**Infra:** Mock `claude -p` with shell script emitting `AgentResult` JSON
**Test:**
- Dispatch → result parsing → message routing → task status update
- Batch dispatch with dependency chain: A blocks B, complete A, verify B launches
- Agent death: kill subprocess, verify reservations released + status updated
- Reservation conflict: two agents dispatched to overlapping files, verify detection
- Role-based context budget: verify budget percentage passed to agent

**Skip:** subprocess spawning details (tokio handles), CLI output (CLI tests)

### 7. TUI — Snapshot Tests
**Infra:** ratatui test helpers (terminal backend mock)
**Test:**
- Task list view rendering with sample data
- Agent status view rendering
- Reservation view rendering
- Layout correctness at different terminal sizes

**Skip:** data loading (store tests), business logic (all other layers)

## Test Infrastructure

### Core Test Helper (feature-gated)

```rust
// filament-core/src/test_utils.rs
#[cfg(feature = "test-utils")]
pub mod test_utils {
    use crate::store::FilamentStore;

    /// Fresh in-memory DB per test — migrations applied, zero shared state
    pub async fn test_db() -> FilamentStore {
        FilamentStore::new("sqlite::memory:").await.unwrap()
    }

    /// Fixture factories
    pub fn sample_entity() -> EntityCreateReq { ... }
    pub fn sample_task() -> EntityCreateReq { ... }
    pub fn sample_relation(source: &str, target: &str) -> RelationCreateReq { ... }
    pub fn sample_message(from: &str, to: &str) -> MessageCreateReq { ... }
    pub fn sample_reservation(agent: &str, glob: &str) -> ReservationCreateReq { ... }
}
```

### Feature flag in Cargo.toml

```toml
[features]
default = []
test-utils = []  # test helpers, fixture factories — never in prod

[dev-dependencies]
filament-core = { path = ".", features = ["test-utils"] }
```

### CLI Integration Test Helper

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

When adding a new feature or module, write tests in this order:

1. **Model unit tests** (fast feedback, no infra)
2. **Store tests** for SQL-specific concerns (constraints, complex queries)
3. **Graph tests** if feature involves traversal or intelligence
4. **CLI integration tests** for all commands (canonical happy-path + output contract)
5. **Daemon/dispatch tests** only when those phases are implemented
6. **Verify no test duplicates an assertion at another layer**

## Anti-Patterns

- **Don't test CRUD happy paths in store tests** — CLI integration tests cover the full stack
- **Don't test SQL constraints in CLI tests** — store tests own constraint verification
- **Don't test model validation in store or CLI tests** — model unit tests own validation
- **Don't mock the store** — in-memory SQLite IS the mock (from koupang lesson)
- **Don't share DB state between tests** — fresh `test_db()` per test
- **Don't test internal error messages in CLI tests** — test exit codes and `--json` structure
