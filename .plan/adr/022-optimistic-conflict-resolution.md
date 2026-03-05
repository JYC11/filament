# ADR-022: Optimistic Conflict Resolution with Auto-Merge

## Status
Accepted

## Context
In multi-agent mode, multiple agents can update the same entity concurrently via the daemon.
Today, last-write-wins silently — agent B's update can overwrite agent A's changes with no
detection or notification. This is especially dangerous for summary and key_facts fields
where agents accumulate knowledge over time.

## Decision
Hybrid optimistic locking with field-level auto-merge:

1. **Version column**: Add `version INTEGER NOT NULL DEFAULT 0` to `entities` table.
   Every successful update bumps the version.

2. **Version-aware updates**: Update operations accept an optional `expected_version`.
   - If omitted (CLI direct use, backward compat): last-write-wins, version still bumps.
   - If provided: compare against current DB version before applying.

3. **Conflict detection + auto-merge**:
   - Version match → apply changes, bump version.
   - Version mismatch → re-read current entity, compare changed fields:
     - **No field overlap** → auto-merge onto current state, bump version, return success.
     - **Field overlap** → return `VersionConflict` error with ALL conflicting fields.

4. **Conflict escalation**: When a `VersionConflict` is returned in daemon mode, automatically
   create a `Conflict` escalation message to the user agent with details of all conflicting
   fields. This uses a new `EscalationKind::Conflict` variant.

5. **Interactive conflict resolution**: CLI presents all conflicts at once and lets the user
   choose per-field which value to keep (theirs, yours, or manual edit).

6. **Scope**: Entities only. Relations and messages are insert/delete (no concurrent field
   updates). Reservations have their own advisory lock model.

## Event diff format

Events gain a new JSON `diff` column **alongside** the existing `old_value`/`new_value`
columns (which are kept for backward compatibility with existing event data). New code writes
to all three; merge logic reads only from `diff`. Events created before the migration have
`diff = NULL` — merge logic treats these conservatively (assumes all fields changed).

**For updates** (field changes):
```json
{
  "summary": { "old": "previous text", "new": "updated text" },
  "status":  { "old": "open", "new": "in_progress" }
}
```

**For creates** (initial values):
```json
{
  "name": "my-entity",
  "summary": "description here",
  "status": "open",
  "priority": 2
}
```

This format enables:
- Determining which fields changed between any two versions
- Replaying history for merge detection without storing full snapshots
- Rich audit trail (supersedes flat old_value/new_value strings)

## Migration strategy

Three phases:
1. **Now**: Additive migration — add `version` + `diff` columns. New code writes both `diff`
   and legacy `old_value`/`new_value`. Merge logic reads `diff` only, treats NULL conservatively.
2. **Follow-up task**: Backfill script — converts existing `old_value`/`new_value` events into
   `diff` JSON format. Run via `util-scripts/migrate-event-diffs.sh` or similar.
3. **After backfill**: Drop migration — removes `old_value`/`new_value` columns, stops writing
   legacy fields in code.

## Fields tracked for merge
Entity fields eligible for concurrent update: `summary`, `key_facts`, `status`, `priority`,
`content_path`, `name`. Each update specifies which fields it changes. On conflict detection,
only fields in the changeset are compared.

## Consequences

### Positive
- No silent data loss in multi-agent scenarios
- Common case (non-overlapping updates) resolves automatically
- True conflicts surface as escalations with full context — human chooses resolution
- All conflicting fields reported at once (not one-at-a-time)
- Backward compatible: omitting version preserves current behavior
- JSON diff column provides richer audit trail than old_value/new_value strings
- Single migration, no per-field timestamp columns needed

### Negative
- Store layer update functions gain a `version` parameter
- Daemon handlers must thread version through from request params
- Slight overhead: on version mismatch, one extra read + event replay before merge attempt
- Temporary column duplication (diff + legacy old_value/new_value) until follow-up migration script backfills old events and drops legacy columns

### Alternatives Rejected
- **Pure last-write-wins**: silent data loss
- **Full field-level timestamps**: over-engineered for local tool
- **Pure optimistic lock (reject on mismatch)**: too aggressive, most conflicts are non-overlapping
- **Per-field changed_field column**: less flexible than JSON diff
- **Report first conflict only**: forces multiple round-trips to resolve multi-field conflicts
