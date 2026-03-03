# ADR-020: Entity algebraic data type

**Date:** 2026-03-03
**Status:** Accepted

## Context

The `Entity` struct was a flat struct with an `entity_type: EntityType` field. Commands like `task close`, `task assign`, and `message send` needed to verify the entity was the right type, leading to 6+ call sites with manual `if entity.entity_type != EntityType::Task` checks. Missing a check meant accepting invalid operations silently.

## Decision

Replace the flat `Entity` struct with a tagged enum (algebraic data type):

```rust
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum Entity {
    Task(EntityCommon),
    Module(EntityCommon),
    Service(EntityCommon),
    Agent(EntityCommon),
    Plan(EntityCommon),
    Doc(EntityCommon),
}
```

### Internal structure

- **`EntityRow`** (private): flat `#[derive(sqlx::FromRow)]` struct for DB queries
- **`EntityCommon`** (public): shared fields (id, slug, name, summary, status, priority, etc.)
- **`Entity`** (public): tagged enum, the domain type returned by all public APIs
- **`From<EntityRow> for Entity`**: automatic conversion at the store boundary

### Accessor methods

`Entity` provides: `common()`, `id()`, `slug()`, `name()`, `entity_type()`, `status()`, `priority()`, `summary()`, `is_task()`, `is_agent()`, `into_common()`.

### Type-safe error

```rust
TypeMismatch { expected: EntityType, actual: EntityType, slug: Slug }
```

Error code `TYPE_MISMATCH`, exit code 4, with hint: `'{slug}' is not a {expected}. Use filament inspect {slug} to check its type`.

## Consequences

- **Compile-time type safety**: pattern matching on `Entity::Task(_)` vs runtime string checks
- **Exhaustive matching**: adding a new entity type requires handling it everywhere
- **No runtime overhead**: serde's internally-tagged representation (`entity_type` field) is the same wire format
- **Breaking change**: same as ADR-019 — `.filament/` must be re-initialized
- **14 new tests**: Slug (9), Entity ADT (5), TypeMismatch (added to existing error tests)
