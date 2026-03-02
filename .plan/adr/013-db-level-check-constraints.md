# ADR-013: DB-level CHECK constraints for invariants

**Date:** 2026-03-02
**Status:** Accepted

## Context

beads_rust enforces status/lifecycle invariants at the database level with CHECK constraints:
```sql
CHECK (
    (status = 'closed' AND closed_at IS NOT NULL) OR
    (status = 'tombstone') OR
    (status NOT IN ('closed', 'tombstone') AND closed_at IS NULL)
)
```

This prevents invalid states even if application code has bugs. Application-only validation can be bypassed by direct SQL access (debugging, migrations, manual fixes).

## Decision

Enforce lifecycle invariants in SQLite CHECK constraints, not just application code. Examples:
- Entity status transitions (active entities can't have `archived_at`)
- Relation endpoints must reference valid entity kinds
- Reservation TTL must be positive
- Message sender and recipient must differ

## Consequences

- Invalid data is impossible to write, regardless of how it's written (app, manual SQL, migration)
- Constraint violations produce clear SQLite errors that map to `FilamentError` variants
- Some constraints are awkward to express in SQL CHECK syntax (complex cross-table rules need triggers instead)
- Schema changes require careful migration of CHECK constraints
- Defense in depth — app code validates too, but the DB is the last line of defense
