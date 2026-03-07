# Typed Entity DTOs — ADT Refactor Plan

**Goal**: Replace flat `CreateEntityRequest` and `EntityChangeset` structs with typed ADTs that enforce per-entity-type field requirements at compile time.

**Fixes**: m2 (content_path cannot be cleared), plus prevents invalid entity creation (Doc without content_path, Lesson without key_facts).

## Field Analysis Per Entity Type

### Creation — which fields are required?

| Field | Task | Module | Service | Agent | Plan | Doc | Lesson |
|-------|------|--------|---------|-------|------|-----|--------|
| name | REQ | REQ | REQ | REQ | REQ | REQ | REQ |
| summary | opt (default "") | opt | opt | opt | opt | opt | auto (= learned) |
| priority | opt (default cfg) | opt | opt | opt | opt | opt | opt |
| key_facts | opt | opt | opt | opt | opt | opt | **REQ** (LessonFields) |
| content_path | **NO** | opt | opt | opt | **REQ** | **REQ** | **NO** |

- **REQ** = must be provided at creation time, error otherwise
- **opt** = optional, has a sensible default
- **NO** = field not accepted; providing one is an error

### Update (changeset) — what can be changed?

| Field | Task | Module | Service | Agent | Plan | Doc | Lesson |
|-------|------|--------|---------|-------|------|-----|--------|
| name | change | change | change | change | change | change | change |
| summary | change | change | change | change | change | change | change |
| status | change | change | change | change | change | change | change |
| priority | change | change | change | change | change | change | change |
| key_facts | change | change | change | change | change | change | change |
| content_path | **clearable** | **clearable** | **clearable** | **clearable** | **change-only** | **change-only** | **clearable** |

- **change** = `Option<T>`: None=don't touch, Some(v)=set to v
- **change-only** = `Option<String>`: None=don't touch, Some(v)=change to v (clear is compile-time impossible)
- **clearable** = `Option<Option<String>>`: None=don't touch, Some(None)=clear, Some(Some(v))=set

### Observations

1. **content_path** is the only field that varies across types for both create and update
2. **key_facts** is structurally required for Lesson (LessonFields) but generic JSON for others
3. All other fields (name, summary, status, priority) are uniform across types
4. Natural grouping emerges: 3 content policies, not 7 unique structs

## Design

### Grouping: 7 variants, 3 inner types

Mirror the Entity ADT pattern: enum with 7 variants for type identity, but only 3 distinct inner struct types based on content policy.

### Creation DTOs

```rust
/// Shared fields for all entity creation requests.
pub struct CreateCommon {
    pub name: String,
    pub summary: Option<String>,
    pub priority: Option<Priority>,
    pub key_facts: Option<serde_json::Value>,
}

/// Entity types that never have content_path.
pub struct CreateNoContent {
    pub common: CreateCommon,
}

/// Entity types where content_path is optional.
pub struct CreateContentOptional {
    pub common: CreateCommon,
    pub content_path: Option<String>,
}

/// Entity types where content_path is required.
pub struct CreateContentRequired {
    pub common: CreateCommon,
    pub content_path: String,  // non-optional
}

pub enum CreateEntityRequest {
    Task(CreateNoContent),
    Module(CreateContentOptional),
    Service(CreateContentOptional),
    Agent(CreateContentOptional),
    Plan(CreateContentRequired),
    Doc(CreateContentRequired),
    Lesson(CreateNoContent),       // key_facts validated in TryFrom
}
```

`CreateEntityRequest::common()` accessor extracts `&CreateCommon` from any variant.
`CreateEntityRequest::content_path()` returns `Option<&str>` from any variant.
`CreateEntityRequest::entity_type()` returns the `EntityType` matching the variant.

`ValidCreateEntityRequest` already exists — its `TryFrom` impl will additionally:
- Error if Lesson's key_facts doesn't parse as `LessonFields`
- Content_path guaranteed present for Doc/Plan by the type system (no runtime check needed)

### Changeset DTOs

```rust
/// Shared changeset fields. All are Option (None = don't change).
pub struct ChangesetCommon {
    pub name: Option<NonEmptyString>,
    pub summary: Option<String>,
    pub status: Option<EntityStatus>,
    pub priority: Option<Priority>,
    pub key_facts: Option<String>,
    pub expected_version: i64,
}

/// Entity types where content_path can be cleared.
pub struct ContentClearableChangeset {
    pub common: ChangesetCommon,
    /// None = don't touch, Some(None) = clear, Some(Some(v)) = set
    pub content_path: Option<Option<String>>,
}

/// Entity types where content_path can change but never be cleared.
pub struct ContentRequiredChangeset {
    pub common: ChangesetCommon,
    /// None = don't touch, Some(v) = change to v
    pub content_path: Option<String>,
}

pub enum EntityChangeset {
    Task(ContentClearableChangeset),
    Module(ContentClearableChangeset),
    Service(ContentClearableChangeset),
    Agent(ContentClearableChangeset),
    Plan(ContentRequiredChangeset),
    Doc(ContentRequiredChangeset),
    Lesson(ContentClearableChangeset),
}
```

`EntityChangeset::common()` / `common_mut()` accessors.
`EntityChangeset::content_path_for_sql()` returns the resolved value for SQL:
- ContentClearable: `None` → keep existing, `Some(None)` → NULL, `Some(Some(v))` → v
- ContentRequired: `None` → keep existing, `Some(v)` → v

`EntityChangeset::changed_field_names()` delegates to common + content_path check.
`EntityChangeset::is_empty()` delegates similarly.

### Why Task/Lesson use ContentClearableChangeset (not a NoContentChangeset)

Even though Task/Lesson don't get content_path at creation, an entity might have been imported or upgraded from a previous version. Allowing clearable on update is safer — it doesn't add content_path at creation (that's the Create ADT's job), but it can clean up existing data.

If we wanted to be stricter, we could use a third `NoContentChangeset` variant that doesn't have the field at all. But that blocks updates on entities that already have content_path set from import or migration — too rigid for a local tool.

## Tasks

### 1. Define new types in `dto.rs`

**File**: `crates/filament-core/src/dto.rs`

- Add `CreateCommon`, `CreateNoContent`, `CreateContentOptional`, `CreateContentRequired`
- Refactor `CreateEntityRequest` from struct → enum with 7 variants
- Add `impl CreateEntityRequest { fn common(), fn content_path(), fn entity_type() }`
- Update `ValidCreateEntityRequest::try_from` — destructure enum, validate per-type
- Add `ChangesetCommon`, `ContentClearableChangeset`, `ContentRequiredChangeset`
- Refactor `EntityChangeset` from struct → enum with 7 variants
- Add `impl EntityChangeset { fn common(), fn content_path_for_sql(), fn changed_field_names(), fn is_empty() }`

### 2. Update store create path

**File**: `crates/filament-core/src/store.rs`

- `create_entity()` already takes `ValidCreateEntityRequest` — update to extract content_path via method
- Minimal change: the SQL stays the same, just how content_path is accessed changes

### 3. Update store update path

**File**: `crates/filament-core/src/store.rs`

- `update_entity()` — replace `changeset.content_path.as_deref().or(row.content_path.as_deref())` with `changeset.content_path_for_sql()` which handles the 3 states
- Conflict detection in `detect_conflicts()` — adapt pattern matching for the new content_path access

### 4. Update CLI entity commands

**File**: `crates/filament-cli/src/commands/entity.rs`

- `add()` — construct the correct `CreateEntityRequest` variant based on `args.r#type`
  - Doc/Plan: require `--content` flag, error without it
  - Task/Lesson: reject `--content` flag, error if provided
  - Module/Service/Agent: accept `--content` optionally
- `update()` — add `--content` and `--clear-content` flags to `UpdateArgs`
  - Read entity type first, construct correct `EntityChangeset` variant
  - `--clear-content` on Doc/Plan → validation error
- `resolve()` — construct correct changeset variant based on entity type

### 5. Update CLI task/lesson commands

**Files**: `crates/filament-cli/src/commands/task.rs`, `lesson.rs`

- `task add` → `CreateEntityRequest::Task(CreateNoContent { ... })`
- `lesson add` → `CreateEntityRequest::Lesson(CreateNoContent { ... })`
  - Validate LessonFields at construction time (already done, just use new type)

### 6. Simplify seed command

**File**: `crates/filament-cli/src/commands/seed.rs`

- **Remove** the section-parsing mode that creates Doc entities from CLAUDE.md sections (no content_path → invalid under new types)
- **Keep only** the file-based mode: each file path in `--files` (or `--file`) becomes a Doc entity with `content_path` set to that file path
- Entity name derived from filename (e.g., `CLAUDE.md`, `gotchas.md`)
- Summary extracted from first meaningful line of the file (existing `extract_summary` logic)
- Remove `--no-claude-md` flag (no longer needed — there's no implicit CLAUDE.md parse)
- This is a simplification: fewer modes, every Doc has a content_path, no partial section extraction

### 7. Update daemon handlers

**Files**: `crates/filament-daemon/src/handler/entity.rs`, `dto.rs`, `mcp.rs`

- `handler/entity.rs`: `UpdateEntityParam.changeset` is now an enum — serde deserialization needs the entity_type tag or the handler resolves the entity first and constructs the right variant
- `dto.rs`: `CreateParams` needs updating — currently flat, needs to construct the right variant
- `mcp.rs`: MCP create/update tools need entity_type to construct correct variant

**Serde consideration**: The daemon protocol is JSON-RPC. The changeset is currently deserialized directly from JSON params. With an enum, serde needs a discriminator. Options:
  - (a) Add `entity_type` field to the JSON params, use `#[serde(tag = "entity_type")]`
  - (b) Handler reads entity first, constructs the right variant manually from flat JSON fields
  - Option (b) is simpler and doesn't change the wire format

### 8. Update tests

**Mechanical updates** (compiler-driven — fix all construction sites):
- `crates/filament-core/tests/` — update all `EntityChangeset { ... }` and `CreateEntityRequest { ... }` constructions to use correct variant
- `crates/filament-daemon/tests/` — update dispatch tests
- `crates/filament-tui/tests/` — update snapshot tests

**Tests REMOVED** (compiler now enforces — "define errors out of existence"):
- ~~Creating Doc without content_path → error~~ — compile-time impossible
- ~~Creating Plan without content_path → error~~ — compile-time impossible
- ~~Clearing content_path on Doc → error~~ — unrepresentable in `ContentRequiredChangeset`
- ~~Task/Lesson created with content_path → error~~ — `CreateNoContent` has no content_path field

**Tests ADDED** (logic the type system doesn't cover):
- `content_path_for_sql()` returns correct value for clearable (keep/clear/set) and required (keep/set)
- Lesson `TryFrom` rejects missing/malformed LessonFields in key_facts
- Serde round-trip for typed changeset via daemon JSON-RPC

**Tests SIMPLIFIED**:
- Conflict detection: one test per content policy (clearable vs required) instead of all 7 types
- Store update merge: covered by `content_path_for_sql()` unit test instead of integration sprawl

### 9. ADR

**File**: `.plan/adr/023-typed-entity-dtos.md`

Document the decision: why ADT, why 3 inner types with 7 variants, what invalid states are now prevented.

## Risks & Edge Cases

1. **Seed simplified** — old section-parsing mode removed; seed now only does file-based inserts where each file becomes a Doc with content_path. Breaking change for users who relied on `fl seed` without `--file`/`--files`, but this is pre-v1 so acceptable.
2. **Import path** — `import_entity()` in store.rs uses `CreateEntityRequest` — imported entities might have any combination. Import should use the flat struct internally or validate per-type. Since import is a bulk restore, it should bypass type-specific validation (the data was valid when exported).
3. **Serde backward compat** — daemon JSON-RPC changeset format changes slightly. Since `.fl/` is local-only and not versioned, this is fine.
4. **Conflict resolution test suite** — conflict_test.rs has many `EntityChangeset` constructions that need updating.
5. **Large blast radius in tests** — ~40+ test sites construct these types. Plan for mechanical refactor with compiler errors guiding.

## Estimated Scope

- ~200 lines new types + accessors in dto.rs
- ~30 lines changed in store.rs (create + update paths)
- ~50 lines changed across CLI commands
- ~30 lines changed in daemon handlers
- ~100+ lines changed in tests (mechanical)
- 1 new ADR

Touches all 4 crates but most changes are mechanical (struct construction sites).

## Sequence

```
[1] dto.rs types         (foundation — everything depends on this)
  |
  +--[2] store create     (depends on 1)
  +--[3] store update     (depends on 1)
  |
  +--[4] CLI entity       (depends on 1)
  +--[5] CLI task/lesson  (depends on 1)
  +--[6] CLI seed         (depends on 1)
  |
  +--[7] daemon handlers  (depends on 1)
  |
  [8] tests               (after all production code, compile-error-driven)
  [9] ADR                 (after implementation is settled)
```

Tasks 2-7 are independent of each other (all depend only on 1). Can be done in parallel or any order, then tests, then ADR.
