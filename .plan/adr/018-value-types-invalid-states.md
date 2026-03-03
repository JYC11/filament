# ADR-018: Value types to make invalid states unrepresentable

**Date:** 2026-03-03
**Status:** Accepted

## Context

Phase 1 used primitive types (`i32` for priority, `f64` for weight, `String` for names) in domain structs. Validation happened only at the DTO boundary (`TryFrom`), meaning code past the boundary could still construct invalid state: negative priorities, NaN weights, empty entity names. The graph silently dropped edges when endpoint nodes were missing, and duplicate edges were undetected.

## Decision

Replace primitives with validated newtypes throughout `filament-core`:

| Type | Wraps | Invariant | Replaces |
|------|-------|-----------|----------|
| `Priority` | `u8` | 0–4 | `i32` |
| `Weight` | `f64` | non-negative, finite | raw `f64` |
| `BudgetPct` | `f64` | 0.0–1.0, finite | raw `f64` |
| `NonEmptyString` | `String` | trimmed, non-empty | `String` |
| `TtlSeconds` | `u32` | > 0 | `i64` |
| `EventType` | enum | 12 known variants | `String` |

Each type has:
- Fallible `new()` returning `Result<Self, FilamentError>`
- sqlx `Encode`/`Decode`/`Type` (with `compatible()` override)
- serde `Serialize`/`Deserialize` via `#[serde(try_from, into)]` — rejects invalid values during deserialization
- `Display`, and `PartialEq<&str>` where useful

Graph edge insertion (`add_edge_from_relation`) now returns `Result`:
- Errors on missing endpoint nodes (was silent no-op)
- Rejects duplicate edges (same source, target, relation_type)

## Consequences

- Invalid state is impossible to construct without `unsafe` or direct field access (fields are still `pub` on newtypes — could tighten later)
- serde deserialization of agent JSON rejects invalid data early (e.g., `"priority": 99` fails at deserialize, not deep in business logic)
- Test helpers use `.unwrap()` on known-valid values — acceptable since test panic is the correct behavior for bad test data
- sqlx custom types require `fn compatible()` override on `Type` impl — without it, `FromRow` decode fails at runtime even when `type_info()` is correct (SQLite type affinity issue)
- `CreateEntityRequest.priority` changed from `Option<i32>` to `Option<u8>` — negative values rejected at serde level before reaching validation
- Adding new `EventType` variants requires a code change (enum, not string) — this is intentional friction
