# Local Codebase Benchmarks

Patterns from sibling projects (workout-util, koupang) to adopt in Filament.

---

## workout-util (SQLite + sqlx, single crate)

### Adopt: Entity/Repo/Service Module Pattern

Each domain module follows a consistent 4-file structure:

```
exercise/
  exercise_entity.rs  # DB row struct (FromRow)
  exercise_dto.rs     # Request/Response types
  exercise_repo.rs    # SQL queries (free functions or methods on unit struct)
  exercise_service.rs # Transaction orchestration
```

### Adopt: Executor Trait Alias

Critical abstraction that lets repos accept either pool or transaction:

```rust
pub type SqliteTx<'a> = sqlx::Transaction<'a, Sqlite>;

pub trait SqliteExecutor<'e>: sqlx::Executor<'e, Database = Sqlite> {}
impl<'e, T: sqlx::Executor<'e, Database = Sqlite>> SqliteExecutor<'e> for T {}
```

- **Write operations** take `&mut SqliteTx<'_>` — forces transaction use
- **Read operations** take `impl SqliteExecutor<'e>` — flexible (pool for simple reads, tx for reads-within-transactions)

### Adopt: In-Memory SQLite for Tests

```rust
#[cfg(test)]
pub const IN_MEMORY_DB_URL: &str = "sqlite::memory:";

async fn setup_db() -> SqlitePool {
    init_db(IN_MEMORY_DB_URL).await
}

#[tokio::test]
async fn test_create_and_get() {
    let pool = setup_db().await;
    let mut tx = pool.begin().await.unwrap();
    let repo = ExerciseRepo::new();
    // ... test against tx, commit
}
```

Fresh DB per test, migrations applied, zero shared state.

### Adopt: QueryBuilder for Dynamic Filters

```rust
let mut qb = QueryBuilder::new("SELECT * FROM entities WHERE 1=1");
// conditionally append filters
if let Some(t) = filter.entity_type {
    qb.push(" AND entity_type = ").push_bind(t);
}
keyset_paginate(&pagination, None, &mut qb);
let rows: Vec<Entity> = qb.build_query_as().fetch_all(executor).await?;
```

### Adopt: sqlx::Type for Enum-to-TEXT Mapping

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
pub enum EntityType {
    Task, Module, Service, Agent, Message, Plan, Doc,
}
// Stored as TEXT in SQLite, auto-converted by sqlx
```

### Adopt: Json<T> for Complex Columns

```rust
pub key_facts: Json<serde_json::Value>,
// bind: .bind(Json(value))
// unwrap: entity.key_facts.0
```

### Avoid: String-based Errors

All errors are `Result<T, String>`. Use `thiserror` instead.

### Avoid: No PRAGMA Configuration

No WAL mode, no foreign_keys=ON, no busy_timeout. Must configure explicitly.

### Avoid: Filesystem-based Migrations

Uses `CARGO_MANIFEST_DIR` at runtime — breaks in deployed binaries. Use `sqlx::migrate!()` macro to embed at compile time.

---

## koupang (Postgres + sqlx, workspace, sophisticated transactions)

### Adopt: TxContext + with_transaction Pattern

The core transaction abstraction, adapted for SQLite:

```rust
use sqlx::{SqliteConnection, Pool, Sqlite, Transaction};
use std::ops::DerefMut;

pub struct TxContext<'tx> {
    tx: Option<Transaction<'tx, Sqlite>>,
}

impl<'tx> TxContext<'tx> {
    pub async fn begin(pool: &Pool<Sqlite>) -> Result<Self> {
        Ok(Self { tx: Some(pool.begin().await?) })
    }

    pub async fn commit(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() { tx.commit().await?; }
        Ok(())
    }

    pub async fn rollback(mut self) -> Result<()> {
        if let Some(tx) = self.tx.take() { tx.rollback().await?; }
        Ok(())
    }

    pub fn as_executor(&mut self) -> &mut SqliteConnection {
        self.tx.as_mut().expect("Transaction consumed").deref_mut()
    }
}

pub async fn with_transaction<F, T>(pool: &Pool<Sqlite>, f: F) -> Result<T>
where
    F: for<'a> FnOnce(&'a mut TxContext<'_>) -> Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>,
    T: Send,
{
    let mut tx_ctx = TxContext::begin(pool).await?;
    match f(&mut tx_ctx).await {
        Ok(result) => { tx_ctx.commit().await?; Ok(result) }
        Err(e) => { let _ = tx_ctx.rollback().await; Err(e) }
    }
}
```

**Key design points:**
- `Option<Transaction>` — `.take()` prevents double-use after commit/rollback
- `as_executor()` returns `&mut SqliteConnection` via `DerefMut` — repos accept this
- `with_transaction` auto-commits on Ok, auto-rollbacks on Err
- Nested transactions via `SAVEPOINT` (SQLite supports this)

**Improvement for filament:** koupang has verbose `.map_err(|e| TxError::Other(e.to_string()))` on every call inside transactions. Fix by making `FilamentError` implement `From` for all inner error types, so `?` works directly.

### Adopt: Free-Function Repositories

Repos are free async functions, not struct methods or trait objects:

```rust
pub async fn create_entity<'e>(
    executor: impl SqliteExec<'e>,
    req: ValidEntityReq,
) -> Result<EntityId, FilamentError> { ... }

pub async fn get_entity_by_id<'e>(
    executor: impl SqliteExec<'e>,
    id: &str,
) -> Result<Entity, FilamentError> { ... }
```

In-memory SQLite replaces mocks — no trait indirection needed.

### Adopt: Validated DTOs via TryFrom

Raw input → validated types at the boundary:

```rust
pub struct EntityCreateReq {
    pub name: String,
    pub entity_type: String,
    pub summary: String,
}

pub struct ValidEntityCreateReq {
    pub name: EntityName,       // validated
    pub entity_type: EntityType, // parsed enum
    pub summary: String,
}

impl TryFrom<EntityCreateReq> for ValidEntityCreateReq {
    type Error = FilamentError;
    fn try_from(req: EntityCreateReq) -> Result<Self, Self::Error> {
        Ok(Self {
            name: EntityName::new(&req.name)?,
            entity_type: req.entity_type.parse()?,
            summary: req.summary,
        })
    }
}
```

### Adopt: Newtype ID Pattern

Prevents mixing entity IDs at compile time:

```rust
macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);
        impl $name {
            pub fn new(id: impl Into<String>) -> Self { Self(id.into()) }
            pub fn as_str(&self) -> &str { &self.0 }
            pub fn into_inner(self) -> String { self.0 }
        }
        impl std::fmt::Display for $name { ... }
    };
}

typed_id!(EntityId);
typed_id!(RelationId);
```

### Adopt: Feature-Gated Test Utils

```toml
[features]
test-utils = []  # gates test helpers so they don't leak into production
```

### Adopt: Three-Layer Test Organization

```
tests/
  integration.rs        # top-level: mod common; mod store; mod graph;
  common/mod.rs         # test_db(), sample_entity(), fixtures
  store/
    mod.rs
    entity_test.rs      # repo/store function tests
    relation_test.rs
    reservation_test.rs
  graph/
    mod.rs
    traversal_test.rs
    intelligence_test.rs
```

---

## Combined Pattern Summary for Filament

### Database Layer (filament-core/src/store.rs)

```
┌─────────────────────────────────────────────┐
│  FilamentStore (holds Pool<Sqlite>)          │
│                                              │
│  pub async fn mutate<F,R>(&self, f) -> R     │  ← MutationContext from plan
│    wraps with_transaction + event logging    │    + koupang TxContext pattern
│    + blocked cache rebuild                   │
│                                              │
│  Internal: with_transaction(pool, closure)   │  ← koupang pattern
│  Internal: TxContext { Option<Transaction> } │  ← koupang pattern
├─────────────────────────────────────────────┤
│  Free functions (repo layer):                │  ← koupang pattern
│                                              │
│  create_entity(executor, req) -> EntityId    │
│  get_entity(executor, id) -> Entity          │
│  update_entity(tx, id, req) -> ()            │
│  delete_entity(tx, id) -> ()                 │
│                                              │
│  Reads:  impl SqliteExec<'e>                 │  ← workout-util pattern
│  Writes: &mut SqliteConnection               │  ← workout-util pattern
├─────────────────────────────────────────────┤
│  Types:                                      │
│                                              │
│  SqliteTx<'a> = Transaction<'a, Sqlite>      │  ← workout-util alias
│  trait SqliteExec: Executor<Sqlite>          │  ← workout-util alias
│  Entity (FromRow, Serialize, JsonSchema)     │
│  EntityType (sqlx::Type, stored as TEXT)     │  ← workout-util pattern
│  Json<Value> for key_facts column            │  ← workout-util pattern
│  ValidEntityReq via TryFrom<EntityReq>       │  ← koupang pattern
│  typed_id!(EntityId)                         │  ← koupang pattern
└─────────────────────────────────────────────┘
```

### Testing Layer

```
In-memory SQLite per test (workout-util pattern)
  + test_db() helper that runs migrations
  + sample_entity() / sample_task() fixture factories
  + Three layers: store tests, graph tests, CLI integration tests (koupang pattern)
  + Feature-gated test-utils (koupang pattern)
```

### Error Layer

```
FilamentError enum (thiserror) with:
  - From<sqlx::Error> for database errors
  - Structured codes/hints for agent consumption (beads_rust pattern)
  - Direct ? usage in transaction closures (fixing koupang's .map_err boilerplate)
```
