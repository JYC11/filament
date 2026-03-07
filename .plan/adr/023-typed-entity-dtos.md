# ADR-023: Typed Entity DTOs

**Status**: Accepted
**Date**: 2026-03-07

## Context

`CreateEntityRequest` and `EntityChangeset` were flat structs with all fields optional. This caused two problems:

1. **Doc/Plan could be created without `content_path`** — content is required for these types but the type system didn't enforce it.
2. **`content_path` couldn't be cleared** — `Option<String>` can only represent "don't touch" (None) or "set" (Some), not "clear to NULL".

Different entity types have different content_path policies:
- **Doc, Plan**: content_path is required at creation, can be changed but never cleared
- **Task, Module, Service, Agent, Lesson**: content_path is optional at creation, can be set or cleared

## Decision

Replace both DTOs with tagged enums (ADTs) that encode per-type content_path policy at compile time.

### CreateEntityRequest

```
enum CreateEntityRequest {
    Task(CreateContentOptional),      // content_path: Option<String>
    Module(CreateContentOptional),
    Service(CreateContentOptional),
    Agent(CreateContentOptional),
    Plan(CreateContentRequired),      // content_path: String (non-optional)
    Doc(CreateContentRequired),
    Lesson(CreateContentOptional),
}
```

`from_parts()` factory constructs the right variant from flat data, returning an error for Doc/Plan without content_path.

### EntityChangeset

```
enum EntityChangeset {
    Task(ContentClearableChangeset),   // content_path: Clearable<String>
    Module(ContentClearableChangeset),
    Service(ContentClearableChangeset),
    Agent(ContentClearableChangeset),
    Plan(ContentRequiredChangeset),    // content_path: Option<String>
    Doc(ContentRequiredChangeset),
    Lesson(ContentClearableChangeset),
}
```

`ContentClearableChangeset` uses `Clearable<String>` — a three-state enum: `Keep` (don't touch), `Clear` (set to NULL), `Set(v)` (set to value). This replaces the unreadable `Option<Option<String>>` pattern.
`ContentRequiredChangeset` uses `Option<String>`: None = keep, Some(v) = change (clearing is unrepresentable).

`for_type()` factory constructs the right variant from entity type + common fields + optional content_path.

### Shared fields via composition

Common fields (name, summary, status, priority, key_facts) are extracted into `CreateCommon` and `ChangesetCommon` structs. Accessor methods (`common()`, `content_path_for_sql()`, `entity_type()`, etc.) provide uniform access.

### ValidCreateEntityRequest stays flat

The validated DTO remains a flat struct — type safety is enforced at the construction boundary (the enum), and the store layer doesn't need to know about content policies.

## Consequences

### Positive

- Doc/Plan without content_path is a compile error (not a runtime surprise)
- Clearing content_path on Doc/Plan is unrepresentable in the type system
- Three-state content_path (keep/clear/set) for clearable types — fixes the m2 bug
- `from_parts()` and `for_type()` factories make migration from flat data simple

### Negative

- Test construction sites are more verbose (mitigated by factory methods and helpers)
- Seed command simplified to file-based only (section-parsing removed since it created Docs without content_path)

### Tests removed (compiler enforces)

- Creating Doc without content_path
- Creating Plan without content_path
- Clearing content_path on Doc/Plan
- Task/Lesson created with content_path on types that don't support it

## Alternatives Considered

1. **Runtime validation only** — rejected because it turns compile-time guarantees into test-coverage gambles
2. **7 unique inner structs** — rejected; only 2 content policies exist (required vs clearable), so 2 inner types with 7 variants mirrors the Entity ADT pattern
3. **3 inner types (NoContent + Optional + Required)** — rejected; even Task/Lesson might have content_path from import/migration, so clearable is safer than no-content
