# Refactor: Slug-Based Identity + Entity ADT

**Status**: Planned
**Breaking**: Yes (DB migration, API change, CLI change)
**Scope**: All 4 crates

## Motivation

Two problems with the current entity model:

### Problem 1: Name collisions
Entities are resolved by name (`resolve_entity` tries `get_entity_by_name` first). Names are
non-unique — two agents could create entities with the same name, and the wrong one gets
returned. Beads_rust solves this with randomly generated slugs as stable identifiers.

### Problem 2: Runtime type checking
`resolve_entity()` returns a generic `Entity` struct. Callers must check `.entity_type` at
runtime to guard operations (e.g., `task_close` checks `entity_type == "task"`, `message_send`
checks `entity_type == "agent"`). This is error-prone — every new call site must remember to
add the guard. An ADT approach moves these checks to the type system.

## Design

### Slug: 8-character base36 identifier

```rust
/// Human-friendly, randomly generated entity identifier.
/// 8 chars of [a-z0-9] = 2.8 trillion combinations.
/// Example: "a3f7k2m9"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Slug(String);

impl Slug {
    pub fn new() -> Self { /* generate random 8-char base36 */ }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

- Generated at entity creation time, stored in DB
- UNIQUE constraint in SQLite — retry on collision (astronomically unlikely)
- `name` stays as a human-readable display label (can duplicate across entities)
- Resolution order: slug first → ID fallback (name removed from resolution)
- CLI: `filament inspect a3f7k2m9` instead of `filament inspect auth-module`
- CLI output: shows slug prominently (e.g., `[a3f7k2m9] auth-module (module, open)`)

### Entity ADT: enum with typed variants

```rust
// DB row struct (flat, for sqlx::FromRow)
#[derive(sqlx::FromRow)]
pub(crate) struct EntityRow {
    pub id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub entity_type: EntityType,  // kept for DB storage
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub content_hash: Option<String>,
    pub status: EntityStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Shared fields across all entity types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntityCommon {
    pub id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub content_hash: Option<String>,
    pub status: EntityStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// The domain-level ADT
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum Entity {
    Task(EntityCommon),
    Module(EntityCommon),
    Service(EntityCommon),
    Agent(EntityCommon),
    Plan(EntityCommon),
    Doc(EntityCommon),
}

impl Entity {
    /// Access common fields regardless of variant.
    pub fn common(&self) -> &EntityCommon { ... }
    pub fn common_mut(&mut self) -> &mut EntityCommon { ... }
    pub fn entity_type(&self) -> EntityType { ... }
    pub fn id(&self) -> &EntityId { &self.common().id }
    pub fn slug(&self) -> &Slug { &self.common().slug }
    pub fn name(&self) -> &NonEmptyString { &self.common().name }
    pub fn status(&self) -> &EntityStatus { &self.common().status }
    pub fn priority(&self) -> Priority { self.common().priority }
}

// Conversion from DB row to domain ADT
impl From<EntityRow> for Entity {
    fn from(row: EntityRow) -> Self {
        let common = EntityCommon { /* fields from row */ };
        match row.entity_type {
            EntityType::Task => Entity::Task(common),
            EntityType::Module => Entity::Module(common),
            EntityType::Service => Entity::Service(common),
            EntityType::Agent => Entity::Agent(common),
            EntityType::Plan => Entity::Plan(common),
            EntityType::Doc => Entity::Doc(common),
        }
    }
}
```

### Typed resolution functions

```rust
// Generic resolution (returns Entity ADT — caller can match)
pub async fn resolve_entity(conn, slug_or_id: &str) -> Result<Entity>

// Type-specific resolution (returns the variant or error)
pub async fn resolve_task(conn, slug_or_id: &str) -> Result<EntityCommon> {
    match resolve_entity(conn, slug_or_id).await? {
        Entity::Task(common) => Ok(common),
        other => Err(FilamentError::TypeMismatch {
            expected: EntityType::Task,
            actual: other.entity_type(),
            slug: other.slug().clone(),
        })
    }
}

pub async fn resolve_agent(conn, slug_or_id: &str) -> Result<EntityCommon> { ... }
// etc. for Module, Service, Plan, Doc
```

### New error variant

```rust
pub enum FilamentError {
    // ... existing variants ...

    #[error("Expected {expected}, but '{slug}' is a {actual}")]
    TypeMismatch {
        expected: EntityType,
        actual: EntityType,
        slug: Slug,
    },
}
```

## Tasks

Dependencies: tasks within the same phase are independent unless noted.

### Phase A: Models + DB (filament-core) — foundation

| # | Task | Files | Depends |
|---|------|-------|---------|
| A1 | Add `Slug` value type | `models.rs` | — |
| A2 | Add `slug` column to entities table, UNIQUE constraint, index | `migrations/003_slug.sql` | — |
| A3 | Add `EntityRow` (internal, `sqlx::FromRow`) | `models.rs` | A1 |
| A4 | Add `EntityCommon` struct | `models.rs` | A1 |
| A5 | Refactor `Entity` from struct → enum ADT | `models.rs` | A3, A4 |
| A6 | Add `Entity` accessor methods (`common()`, `id()`, `slug()`, etc.) | `models.rs` | A5 |
| A7 | Add `From<EntityRow> for Entity` conversion | `models.rs` | A5 |
| A8 | Add `TypeMismatch` error variant | `error.rs` | A1 |
| A9 | Update `CreateEntityRequest` / `ValidCreateEntityRequest` — no slug in input (auto-generated) | `models.rs` | A1 |
| A10 | Update `GraphNode` — add `slug: Slug` field | `graph.rs` | A1 |

### Phase B: Store layer (filament-core) — data access

| # | Task | Files | Depends |
|---|------|-------|---------|
| B1 | Update `create_entity` — generate slug, insert slug column | `store.rs` | A1–A9 |
| B2 | Replace `get_entity_by_name` with `get_entity_by_slug` | `store.rs` | A5, A7 |
| B3 | Update `get_entity` (by ID) — query `EntityRow`, convert to `Entity` | `store.rs` | A5, A7 |
| B4 | Update `list_entities` — return `Vec<Entity>` via `EntityRow` conversion | `store.rs` | A5, A7 |
| B5 | Update `ready_tasks` — return `Vec<EntityCommon>` (already known to be tasks) | `store.rs` | A5 |
| B6 | Update `update_entity_summary`, `update_entity_status`, `update_entity_key_facts` | `store.rs` | A2 |
| B7 | Update `delete_entity` | `store.rs` | — |
| B8 | Add typed resolution helpers: `resolve_task`, `resolve_agent` | `store.rs` or new `resolve.rs` | A5, A8, B2, B3 |
| B9 | Update `KnowledgeGraph::add_node_from_entity` — use `Entity` ADT, populate slug | `graph.rs` | A10 |
| B10 | Update `KnowledgeGraph::ready_tasks` — match on `Entity::Task` variant | `graph.rs` | A5 |

### Phase C: Connection + daemon (filament-core, filament-daemon)

| # | Task | Files | Depends |
|---|------|-------|---------|
| C1 | Update `FilamentConnection` — replace `get_entity_by_name` with `get_entity_by_slug`, update return types | `connection.rs` | B2–B5 |
| C2 | Update `DaemonClient` — slug-based methods | `daemon_client.rs` | C1 |
| C3 | Update NDJSON protocol `Method` enum — rename `GetEntityByName` → `GetEntityBySlug` | `protocol.rs` | C1 |
| C4 | Update daemon handlers — use `Entity` ADT | `handler/entity.rs` | C1 |
| C5 | Update MCP `resolve_entity` — slug-first resolution | `mcp.rs` | C1 |
| C6 | Update MCP `task_close` — use `resolve_task()` instead of manual type check | `mcp.rs` | B8 |
| C7 | Update MCP `message_send` — use `resolve_agent()` instead of manual type check | `mcp.rs` | B8 |
| C8 | Update MCP tool param names — `name` → `slug` where applicable | `mcp.rs` | C5 |
| C9 | Update MCP `create` tool — return slug in response | `mcp.rs` | B1 |

### Phase D: CLI (filament-cli)

| # | Task | Files | Depends |
|---|------|-------|---------|
| D1 | Update `resolve_entity` helper — slug-first, ID fallback (remove name lookup) | `helpers.rs` | C1 |
| D2 | Add `resolve_task`, `resolve_agent` helpers | `helpers.rs` | B8 |
| D3 | Update `task show/close/assign` — use typed resolution | `task.rs` | D2 |
| D4 | Update `inspect`, `read`, `update`, `remove` — use `Entity` ADT accessors | `entity.rs` | D1 |
| D5 | Update `relate`, `unrelate` — use slug resolution | `relation.rs` | D1 |
| D6 | Update `context` — use slug resolution | `query.rs` | D1 |
| D7 | Update CLI arg names — `<name>` → `<slug>` in help text | `entity.rs`, `task.rs`, `query.rs` | D1 |
| D8 | Update display formatting — show slug in output (e.g., `[a3f7k2m9] auth-module`) | `helpers.rs` | D1 |
| D9 | Update `add` command — print generated slug on creation | `entity.rs`, `task.rs` | B1 |

### Phase E: Tests

| # | Task | Files | Depends |
|---|------|-------|---------|
| E1 | Update core unit tests — `Entity` ADT construction, `Slug` validation, `TypeMismatch` error | `models.rs` tests | A1–A8 |
| E2 | Update store tests — slug-based creation/lookup, collision retry | `store.rs` tests | B1–B8 |
| E3 | Update graph tests — `GraphNode` with slug | `graph.rs` tests | B9, B10 |
| E4 | Update CLI integration tests — slug in commands | CLI tests | D1–D9 |
| E5 | Update daemon integration tests | daemon tests | C1–C9 |
| E6 | Update MCP integration tests | MCP tests | C5–C9 |

### Phase F: Documentation + cleanup

| # | Task | Files | Depends |
|---|------|-------|---------|
| F1 | Write ADR-019: Slug-based entity identification | `.plan/adr/019-slug-identity.md` | — |
| F2 | Write ADR-020: Entity ADT over struct + type field | `.plan/adr/020-entity-adt.md` | — |
| F3 | Update CLAUDE.md — new entity model, key concepts, updated test count | `CLAUDE.md` | E1–E6 |
| F4 | Update MEMORY.md — session handoff, current state | `MEMORY.md` | E1–E6 |
| F5 | Update `.plan/filament-v1.md` — reflect refactor in master plan | `.plan/filament-v1.md` | — |
| F6 | Remove filament task tracking data (`.filament/` DB will be rebuilt) | — | — |
| F7 | Update `.plan/gotchas.md` — add slug/ADT gotchas discovered during implementation | `.plan/gotchas.md` | E1–E6 |

## Migration Strategy

Since `.filament/` is gitignored and per-user:
1. Add `migrations/003_slug.sql` with `ALTER TABLE entities ADD COLUMN slug TEXT`
2. Backfill: `UPDATE entities SET slug = lower(hex(randomblob(4)))` (8 hex chars as seed)
3. Add UNIQUE index: `CREATE UNIQUE INDEX idx_entities_slug ON entities(slug)`
4. sqlx migration runs automatically on next `filament init` or daemon start

For existing users: just delete `.filament/` and re-init. Breaking change is acceptable.

## Scope of Breaking Changes

| What breaks | Why | Mitigation |
|-------------|-----|------------|
| CLI commands: `filament inspect my-task` | Name resolution removed | Use slug: `filament inspect a3f7k2m9` |
| MCP tools: `name` parameter | Renamed to `slug` | Update MCP tool schemas |
| NDJSON protocol: `GetEntityByName` | Replaced by `GetEntityBySlug` | Protocol version bump |
| Existing `.filament/` databases | New column + ADT | Delete and re-init |
| All code importing `Entity` struct | Now an enum | Update match patterns |
| Serialized JSON (`entity_type` field) | Now `#[serde(tag)]` discriminant | Wire format is compatible (tagged enum) |
| `resolve_entity` return type | Was `Entity` struct, now `Entity` enum | Update callers to match or use accessors |

## Open Questions

1. **Slug charset**: `[a-z0-9]` (36 chars, 8 length = 2.8T) vs `[a-z]` only (26 chars, 8 length = 208B) — base36 recommended for density
2. **Name in CLI**: Should `filament add` still show name in output alongside slug? Yes — name is the display label
3. **Search by name**: Should there be a `filament search <name>` command for fuzzy name lookup? Defer to later — out of scope for this refactor
4. **EntityCommon vs separate structs per type**: Currently all types share identical fields. If type-specific fields emerge later (e.g., `Task.assignee`), the ADT is ready for it. For now, all variants wrap `EntityCommon`
