# Phase 1: Foundation (filament-core + SQLite)

**Goal**: single-crate library with graph + storage that compiles and passes tests.

**Master plan**: [filament-v1.md](filament-v1.md)
**Local benchmarks**: [benchmarks-local.md](benchmarks-local.md) (workout-util + koupang patterns)

---

## Patterns to Use (from local benchmarks)

These are solved problems — copy the patterns, don't reinvent them.

### From workout-util (SQLite + sqlx)

- **`SqliteExec` trait alias** — lets repo functions accept pool OR transaction:
  ```rust
  pub type SqliteTx<'a> = sqlx::Transaction<'a, Sqlite>;
  pub trait SqliteExec<'e>: sqlx::Executor<'e, Database = Sqlite> {}
  impl<'e, T: sqlx::Executor<'e, Database = Sqlite>> SqliteExec<'e> for T {}
  ```
- **Write ops take `&mut SqliteConnection`**, read ops take `impl SqliteExec<'e>`
- **`sqlx::Type` derive** for enums stored as TEXT in SQLite
- **`Json<T>`** wrapper for complex columns (key_facts, metadata)
- **In-memory SQLite for tests** — `sqlite::memory:`, fresh DB per test, migrations applied
- **`QueryBuilder`** for dynamic filter queries
- **Embed migrations** with `sqlx::migrate!()` (not filesystem-based)

### From koupang (sophisticated transactions)

- **`TxContext` + `with_transaction`** — Option<Transaction> wrapper, auto-commit on Ok, auto-rollback on Err
- **Free-function repositories** — not struct methods, not traits. In-memory SQLite replaces mocks
- **Validated DTOs via `TryFrom`** — raw input → validated newtypes at boundary
- **`typed_id!` macro** — prevents mixing EntityId/RelationId at compile time
- **Feature-gated test-utils** — `#[cfg(feature = "test-utils")]` keeps test infra out of prod
- **Three-layer test org** — store tests, graph tests, CLI integration tests

### Pool initialization (improving on both)

```rust
SqlitePoolOptions::new()
    .after_connect(|conn, _meta| Box::pin(async move {
        conn.execute("PRAGMA journal_mode=WAL").await?;
        conn.execute("PRAGMA foreign_keys=ON").await?;
        conn.execute("PRAGMA busy_timeout=5000").await?;
        conn.execute("PRAGMA synchronous=NORMAL").await?;
        Ok(())
    }))
    .connect(url).await?;
```

---

## 1.1 — Cargo workspace setup

- File: `filament/Cargo.toml` (workspace), `filament/crates/filament-core/Cargo.toml`
- Dependencies: sqlx (sqlite, runtime-tokio, chrono, json), petgraph, tokio, serde, serde_json, thiserror, schemars, tracing, blake3, chrono
- Workspace lint config + release profile defined in master plan
- Blocked by: nothing

## 1.2 — Data models

- File: `filament-core/src/models.rs`
- All types derive `Serialize, Deserialize, JsonSchema` (schemars for MCP/agent integration)
- Enums derive `sqlx::Type` for TEXT storage (from workout-util)
- Use `typed_id!` macro for `EntityId`, `RelationId` (from koupang)
- Use `Json<serde_json::Value>` for `key_facts` and `metadata` columns (from workout-util)
- Types:

  ```rust
  typed_id!(EntityId);
  typed_id!(RelationId);

  #[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize, JsonSchema)]
  pub enum EntityType { Task, Module, Service, Agent, Message, Plan, Doc }

  #[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize, JsonSchema)]
  pub enum RelationType { Blocks, DependsOn, Produces, Owns, RelatesTo, AssignedTo }

  #[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize, JsonSchema)]
  pub enum EntityStatus { Open, InProgress, Closed, Blocked }

  // DB row struct (FromRow)
  #[derive(Debug, Clone, FromRow, Serialize, Deserialize, JsonSchema)]
  pub struct Entity {
      pub id: String,
      pub name: String,
      pub entity_type: EntityType,
      pub summary: String,
      pub key_facts: Json<serde_json::Value>,
      pub content_path: Option<String>,
      pub content_hash: Option<String>,
      pub status: EntityStatus,
      pub priority: i32,
      pub created_at: DateTime<Utc>,
      pub updated_at: DateTime<Utc>,
  }

  #[derive(Debug, Clone, FromRow, Serialize, Deserialize, JsonSchema)]
  pub struct Relation { ... }

  // Agent protocol
  AgentResult { status, task_id, summary, artifacts, messages, blockers, questions }
  AgentStatus: Running, Completed, Blocked, Failed, NeedsInput

  // File reservations (advisory leases)
  Reservation { id, agent_name, file_glob, exclusive, expires_at, created_at }
  ```

- Validated DTOs via `TryFrom` (from koupang):
  ```rust
  pub struct EntityCreateReq { pub name: String, pub entity_type: String, ... }
  pub struct ValidEntityCreateReq { pub name: String, pub entity_type: EntityType, ... }
  impl TryFrom<EntityCreateReq> for ValidEntityCreateReq { ... }
  ```

- Blocked by: 1.1

## 1.3 — Structured error types

- File: `filament-core/src/error.rs`
- `FilamentError` enum via `thiserror` with categorized variants
- Implements `From<sqlx::Error>` so `?` works directly in repo functions and transaction closures (fixing koupang's `.map_err` boilerplate)
  ```rust
  #[derive(Error, Debug)]
  pub enum FilamentError {
      #[error("Entity not found: {id}")] EntityNotFound { id: String },
      #[error("Relation not found: {source} -> {target}")] RelationNotFound { source: String, target: String },
      #[error("Cycle detected: {path}")] CycleDetected { path: String },
      #[error("File reserved by {agent}: {glob}")] FileReserved { agent: String, glob: String },
      #[error("Reservation expired")] ReservationExpired,
      #[error("Validation: {0}")] Validation(String),
      #[error("Database: {0}")] Database(#[from] sqlx::Error),
      #[error("Protocol: {0}")] Protocol(String),
  }
  ```
- Each variant provides:
  - `error_code() -> &'static str` — machine-readable (`"ENTITY_NOT_FOUND"`, `"CYCLE_DETECTED"`)
  - `is_retryable() -> bool`
  - `hint() -> Option<String>` — agent-friendly fix suggestion
  - `exit_code() -> i32` — categorized (2=database, 3=entity, 4=validation, 5=dependency, 6=reservation)
- `StructuredError` wrapper for JSON output:
  ```rust
  #[derive(Serialize)]
  pub struct StructuredError { pub code: String, pub message: String, pub hint: Option<String>, pub retryable: bool }
  impl From<&FilamentError> for StructuredError { ... }
  ```
- Blocked by: 1.1

## 1.4 — SQLite schema + migrations

- File: `filament/migrations/001_init.sql`, `filament-core/src/schema.rs`
- Migrations embedded via `sqlx::migrate!()` macro (not filesystem — from workout-util lesson)
- Pool init with PRAGMAs via `SqlitePoolOptions::after_connect` (improving on workout-util)
- Tables:
  ```sql
  entities (id TEXT PRIMARY KEY, name TEXT NOT NULL, entity_type TEXT NOT NULL,
            summary TEXT NOT NULL DEFAULT '', key_facts TEXT NOT NULL DEFAULT '{}',
            content_path TEXT, content_hash TEXT,
            status TEXT NOT NULL DEFAULT 'open', priority INTEGER NOT NULL DEFAULT 2,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            CHECK ((status = 'closed' AND updated_at IS NOT NULL) OR (status != 'closed')))

  relations (id TEXT PRIMARY KEY, source_id TEXT NOT NULL, target_id TEXT NOT NULL,
             relation_type TEXT NOT NULL, weight REAL NOT NULL DEFAULT 1.0,
             summary TEXT NOT NULL DEFAULT '', metadata TEXT NOT NULL DEFAULT '{}',
             created_at TEXT NOT NULL,
             FOREIGN KEY (source_id) REFERENCES entities(id) ON DELETE CASCADE,
             FOREIGN KEY (target_id) REFERENCES entities(id) ON DELETE CASCADE)

  messages (id TEXT PRIMARY KEY, from_agent TEXT NOT NULL, to_agent TEXT NOT NULL,
            msg_type TEXT NOT NULL DEFAULT 'text', body TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'unread', created_at TEXT NOT NULL, read_at TEXT,
            CHECK (from_agent != '' AND to_agent != ''))

  agent_runs (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, agent_role TEXT NOT NULL,
              pid INTEGER, status TEXT NOT NULL, result_json TEXT,
              context_budget_pct REAL, started_at TEXT NOT NULL, finished_at TEXT)

  file_reservations (id TEXT PRIMARY KEY, agent_name TEXT NOT NULL, file_glob TEXT NOT NULL,
                     exclusive INTEGER NOT NULL DEFAULT 1,
                     created_at TEXT NOT NULL, expires_at TEXT NOT NULL,
                     CHECK (expires_at > created_at))

  blocked_entities_cache (entity_id TEXT PRIMARY KEY, blocker_ids_json TEXT NOT NULL,
                          updated_at TEXT NOT NULL)

  events (id TEXT PRIMARY KEY, entity_id TEXT, event_type TEXT NOT NULL,
          actor TEXT NOT NULL DEFAULT '', old_value TEXT, new_value TEXT,
          created_at TEXT NOT NULL)
  ```
- Indexes: relations(source_id), relations(target_id), entities(entity_type, status), messages(to_agent, status), file_reservations(agent_name), file_reservations(expires_at)
- Partial index: `idx_entities_ready ON entities(status, priority, created_at) WHERE entity_type = 'task' AND status IN ('open', 'in_progress')`
- Blocked by: 1.1

## 1.5 — FilamentStore: SQLite operations

- File: `filament-core/src/store.rs`
- `FilamentStore` struct holds `Pool<Sqlite>` (like koupang service pattern)
- Type aliases (from workout-util):
  ```rust
  pub type SqliteTx<'a> = sqlx::Transaction<'a, Sqlite>;
  pub trait SqliteExec<'e>: sqlx::Executor<'e, Database = Sqlite> {}
  impl<'e, T: sqlx::Executor<'e, Database = Sqlite>> SqliteExec<'e> for T {}
  ```
- **`TxContext` + `with_transaction`** (from koupang, adapted for SQLite):
  - `TxContext` wraps `Option<Transaction<'tx, Sqlite>>`
  - `with_transaction(pool, closure)` — auto-commit on Ok, auto-rollback on Err
  - `FilamentError` implements `From<sqlx::Error>` so `?` works inside closures without `.map_err`
- **`mutate()` method** wraps `with_transaction` + event logging + blocked cache rebuild (from beads_rust plan)
- **Free-function repo layer** (from koupang):
  - `create_entity(executor, req) -> Result<EntityId>`
  - `get_entity(executor, id) -> Result<Entity>`
  - `update_entity(tx, id, req) -> Result<()>`
  - `delete_entity(tx, id) -> Result<()>`
  - Reads take `impl SqliteExec<'e>`, writes take `&mut SqliteConnection`
- CRUD for entities, relations, messages, reservations, agent runs
- File reservation: acquire/release/check/expire-stale
- Blocked entity cache: rebuild on dependency or status changes
- Database init: create pool with PRAGMAs, run embedded migrations
- Blocked by: 1.2, 1.3, 1.4

## 1.6 — KnowledgeGraph: petgraph wrapper

- File: `filament-core/src/graph.rs`
- Hydrate from SQLite on startup
- Traversal: BFS/DFS from a node with depth limit
- Context query: return tier-1 summaries within N hops
- Node lookup by name (HashMap<String, NodeIndex>)
- Upsert node/edge (keep graph in sync with SQLite writes)
- **Graph intelligence** (from beads_viewer pattern):
  - `ready_tasks()` — unblocked tasks ranked by priority
  - `critical_path(task_id)` — longest dependency chain to completion
  - `impact_score(entity_id)` — in-degree + transitive dependents count
  - Cycle detection (petgraph `is_cyclic_directed`)
- Blocked by: 1.2

## 1.7 — Connection abstraction

- File: `filament-core/src/connection.rs`
- `enum FilamentConnection { Direct(FilamentStore), Socket(UnixStream) }`
- Detect daemon: check for `.filament/filament.sock` → Socket, else → Direct
- All operations go through this enum — CLI doesn't know which mode it's in
- Blocked by: 1.5

## 1.8 — Protocol types

- File: `filament-core/src/protocol.rs`
- JSON-RPC style: `Request { id, method, params }`, `Response { id, result?, error? }`
- Method enum matching all store operations (including reservation ops)
- Error responses use `StructuredError` format
- Serde derives for wire format
- Blocked by: 1.2, 1.3

## 1.9 — Tests for Phase 1

**Full test strategy**: [test-standards.md](test-standards.md) — layered approach adapted from koupang.

### Test Infrastructure

- Feature-gated `test-utils` module: `#[cfg(feature = "test-utils")]`
- `test_db()` → fresh in-memory SQLite per test, migrations applied, zero shared state
- Fixture factories: `sample_entity()`, `sample_task()`, `sample_relation()`, `sample_message()`, `sample_reservation()`

### Layer 1: Model Unit Tests (sync, no I/O)

- `typed_id!` uniqueness, Display/FromStr round-trip
- `TryFrom` validation: reject invalid types, empty names, bad transitions
- Enum serde: `EntityType`, `RelationType`, `EntityStatus` → TEXT round-trip
- `AgentResult` deserialization from sample JSON
- `StructuredError` serialization: codes, hints, retryable, JSON format

### Layer 2: Store Tests (in-memory SQLite)

- Schema constraints: CHECK (status lifecycle, expires_at > created_at), FK cascades
- `with_transaction`: auto-commit on Ok, auto-rollback on Err
- `mutate()` pipeline: event recording, blocked cache rebuild
- Reservation SQL: acquire, conflict, TTL expiry, stale cleanup
- **Skip**: CRUD happy paths (CLI integration tests cover these)

### Layer 3: Graph Tests (store + petgraph)

- Hydration: verify node/edge counts match store
- `ready_tasks()` ordering, `critical_path()`, `impact_score()`
- Cycle detection: returns `CycleDetected` error
- Graph sync after store mutations

### Protocol Tests (sync)

- JSON-RPC request/response round-trip serialization
- Method enum coverage, StructuredError in error responses

### Anti-Patterns (from test standards)
- Don't mock the store — in-memory SQLite IS the mock
- Don't test CRUD happy paths in store tests — CLI tests cover full stack
- Don't share DB state between tests — fresh `test_db()` per test

- Blocked by: 1.5, 1.6, 1.8

---

## Task Dependency Graph

```
1.1 (workspace)
 ├──→ 1.2 (models)
 │     ├──→ 1.5 (store) ──→ 1.7 (connection)
 │     ├──→ 1.6 (graph)
 │     └──→ 1.8 (protocol)
 ├──→ 1.3 (errors)
 │     ├──→ 1.5 (store)
 │     └──→ 1.8 (protocol)
 └──→ 1.4 (schema)
       └──→ 1.5 (store)

1.5, 1.6, 1.8 ──→ 1.9 (tests)
```
