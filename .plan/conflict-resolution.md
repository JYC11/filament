# Conflict Resolution Implementation Plan

ADR: `.plan/adr/022-optimistic-conflict-resolution.md`
Task: `9vteyuvr`

## NON-BREAKING CONSTRAINT
The local `.filament/` DB has live task data. All migrations must be **additive only**:
- New columns must have DEFAULT values
- Existing columns must NOT be dropped or renamed
- New `diff` column coexists with existing `old_value`/`new_value` (deprecated but kept)

## Tasks

### 1. Migration: add version + diff columns
- **File**: `migrations/004_version_and_diff.sql`
- `ALTER TABLE entities ADD COLUMN version INTEGER NOT NULL DEFAULT 0;`
- `ALTER TABLE events ADD COLUMN diff TEXT;` (JSON, nullable — old events won't have it)
- Keep existing `old_value`/`new_value` columns (non-breaking)
- New code writes both `diff` JSON and legacy `old_value`/`new_value` for backward compat
- Deps: none

### 2. Add `EscalationKind::Conflict` variant
- **File**: `crates/filament-core/src/dto.rs`
- Add `Conflict` to `EscalationKind` enum
- Deps: none

### 3. Add `VersionConflict` error variant
- **File**: `crates/filament-core/src/error.rs`
- New variant with ALL conflicting fields:
  ```rust
  VersionConflict {
      entity_id: String,
      current_version: i64,
      conflicts: Vec<FieldConflict>,
  }
  ```
- New struct:
  ```rust
  pub struct FieldConflict {
      pub field: String,
      pub your_value: String,
      pub their_value: String,
  }
  ```
- `error_code()` → `"VERSION_CONFLICT"`, `is_retryable()` → `true`
- Hint: "Re-read the entity or resolve conflicts with `filament resolve <slug>`"
- Deps: none

### 4. Add `EntityChangeset` struct
- **File**: `crates/filament-core/src/dto.rs`
- Struct capturing which fields are being changed:
  ```rust
  pub struct EntityChangeset {
      pub summary: Option<String>,
      pub status: Option<EntityStatus>,
      pub priority: Option<Priority>,
      pub key_facts: Option<String>,
      pub content_path: Option<String>,
      pub name: Option<NonEmptyString>,
      pub expected_version: Option<i64>,
  }
  ```
- Method: `changed_field_names() -> Vec<&str>` — returns names of non-None fields
- Deps: none

### 5. JSON diff helpers
- **File**: `crates/filament-core/src/dto.rs` (or small `diff.rs` module)
- `EventDiff` type alias for `serde_json::Value`
- Builder: `DiffBuilder` to construct update diffs:
  ```rust
  DiffBuilder::new()
      .field("summary", old_val, new_val)
      .field("status", old_val, new_val)
      .build()
  // → { "summary": { "old": "x", "new": "y" }, "status": { "old": "a", "new": "b" } }
  ```
- Builder for create diffs (flat key-value):
  ```rust
  DiffBuilder::create()
      .value("name", val)
      .value("summary", val)
      .build()
  // → { "name": "x", "summary": "y" }
  ```
- Parser: `fields_in_diff(diff_json) -> HashSet<String>` — extracts field names from a diff
- Deps: none

### 6. Store: unified `update_entity` with version check
- **File**: `crates/filament-core/src/store.rs`
- New function: `update_entity(conn, id, changeset) -> Result<Entity>`
- Logic:
  1. Begin transaction
  2. `SELECT version, summary, status, priority, ... FROM entities WHERE id = ?` (current state)
  3. If `expected_version` is None → apply all changes, bump version (backward compat LWW)
  4. If `expected_version == current_version` → apply all changes, bump version
  5. If mismatch → call merge logic (task 7)
  6. Build JSON diff from old→new values using `DiffBuilder`
  7. Record event with both `diff` JSON and legacy `old_value`/`new_value`
  8. Return updated entity with new version
- Keep existing `update_entity_summary` and `update_entity_status` as thin wrappers
- Deps: 1, 3, 4, 5

### 7. Auto-merge + conflict detection logic
- **File**: `crates/filament-core/src/store.rs`
- Called from `update_entity` when version mismatch detected:
  1. Query events for this entity where diff is not null, ordered by created_at,
     starting after the caller's expected_version
  2. For each event diff, extract field names via `fields_in_diff()`
  3. Union all touched fields → `remotely_changed: HashSet<String>`
  4. Intersect with `changeset.changed_field_names()` → `conflicts`
  5. If `conflicts` is empty → apply changeset onto current DB values, bump version, success
  6. If conflicts exist → build `Vec<FieldConflict>` with all overlapping fields,
     return `VersionConflict` error
- Deps: 5, 6

### 8. Backfill: record diffs on existing event-producing operations
- **File**: `crates/filament-core/src/store.rs`
- Update `create_entity` to record a create-style diff in the event
- Update `record_event` signature or callers to accept optional `diff` JSON
- Events recorded before migration will have `diff = NULL` — merge logic handles this
  by treating NULL-diff events conservatively (assume all fields changed)
- Deps: 1, 5

### 9. Daemon handler: thread version through
- **File**: `crates/filament-daemon/src/handler/entity.rs`
- Replace `UpdateStatusParam`/`UpdateSummaryParam` with unified `UpdateEntityParam`:
  ```rust
  pub struct UpdateEntityParam {
      pub id: String,
      pub changeset: EntityChangeset,
  }
  ```
- Keep old param structs as deprecated aliases if needed for protocol compat
- On `VersionConflict` error → create escalation message (kind=Conflict) to "user"
  with JSON body containing all `FieldConflict` entries
- Deps: 2, 6

### 10. CLI: `--version` flag + conflict display
- **File**: `crates/filament-cli/src/commands/entity.rs`
- Add `--version` flag to `UpdateArgs` (optional i64)
- On `VersionConflict` error → print table of all conflicts:
  ```
  Conflict on entity abc123de (version 5):
    Field     Yours          Theirs
    summary   "my change"    "their change"
    status    in_progress    closed

  Resolve with: filament resolve abc123de
  ```
- Deps: 6

### 11. CLI: `filament resolve <slug>` command
- **File**: `crates/filament-cli/src/commands/entity.rs` (new subcommand)
- Interactive conflict resolution:
  1. Show all pending conflicts (from escalation messages of kind=Conflict)
  2. For each conflicting field, prompt user:
     - `[t]heirs` — keep the current DB value
     - `[y]ours` — apply the original intended value
     - `[e]dit` — open editor / type new value
  3. Apply resolved changeset with current version (no expected_version, LWW)
  4. Mark the Conflict escalation message as resolved
- Deps: 10

### 12. CLI: show version in inspect output
- **File**: `crates/filament-cli/src/commands/entity.rs` (inspect command)
- Include `version: N` in entity display
- Deps: 1

### 13. Tests
- **File**: `crates/filament-core/tests/conflict_test.rs`
- Test cases:
  - Update bumps version from 0 → 1
  - Update with matching expected_version succeeds
  - Update with mismatched version + non-overlapping fields → auto-merge, version bumps
  - Update with mismatched version + overlapping fields → VersionConflict with ALL conflicts
  - Update without expected_version → LWW (backward compat), version bumps
  - JSON diff correctly built for updates (old/new pairs)
  - JSON diff correctly built for creates (flat values)
  - `fields_in_diff()` extracts correct field names
  - NULL-diff events treated conservatively in merge
  - Escalation created on conflict in daemon mode
  - Version displayed in inspect output
  - DiffBuilder produces correct JSON
- Deps: 6, 7, 8, 9

## Dependency Graph

```
1 (migration) ────────────┐
2 (escalation kind) ──────┤
3 (error + FieldConflict) ┤
4 (changeset) ────────────┼──→ 6 (store update) ──→ 7 (auto-merge)
5 (diff helpers) ─────────┘         │                     │
                                    ├──→ 8 (backfill diffs on create/events)
                                    │                     │
                                    ├──→ 9 (daemon handler) ←─────────┘
                                    ├──→ 10 (CLI --version + display)
                                    │         │
                                    │         └──→ 11 (CLI resolve command)
                                    └──→ 12 (CLI inspect version)
                                                          │
                                                     13 (tests)
```

## Parallelizable groups

| Phase | Tasks | Can parallelize |
|-------|-------|-----------------|
| A     | 1, 2, 3, 4, 5 | All independent — do in parallel |
| B     | 6, 7, 8 | Sequential (6 → 7, 6 → 8) |
| C     | 9, 10, 11, 12 | 9 and 12 parallel; 10 after 6; 11 after 10 |
| D     | 13 | After all above |

## Follow-up tasks (post-merge, separate PRs)

### 14. Backfill script: migrate old_value/new_value → diff JSON
- **File**: `util-scripts/migrate-event-diffs.sh` (or Rust one-shot binary)
- Reads all events where `diff IS NULL` and `old_value IS NOT NULL`
- Converts `old_value`/`new_value` pairs into `{ "field": { "old": ..., "new": ... } }` format
- Problem: legacy events don't record which field changed — only `event_type` hints at it
  (e.g., `StatusChange` → field is `status`, `EntityUpdated` → field is `summary`)
- Best-effort heuristic based on event_type; mark ambiguous events with `{ "_unknown": true }`
- Deps: all above merged and stable

### 15. Drop legacy columns migration
- **File**: `migrations/005_drop_legacy_event_columns.sql`
- Drop `old_value` and `new_value` columns from `events` table
- Remove legacy write paths in store code
- Only after backfill script has run on all local DBs
- Deps: 14
