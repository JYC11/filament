# v1.0.0 Code Review — Consolidated Findings

**Date**: 2026-03-07
**Reviewer**: Claude (automated, 4 parallel agents)
**Scope**: All 4 crates, ~25k lines of Rust source code
**CI Status**: GREEN (515 tests, 0 clippy warnings)

## Summary

| Severity | Count | Fixed |
|----------|-------|-------|
| Critical | 0     | —     |
| Major    | 5     | 5/5 FIXED |
| Minor    | 18    | 9/18 FIXED |
| Nit      | 10    | 4/10 FIXED |

---

## Major Findings (ALL FIXED)

### M1. `impact_score` graph traversal direction inverted [filament-core]
- **File**: `crates/filament-core/src/graph.rs`
- **Issue**: Counts upstream dependencies instead of downstream dependents.
- **Fix**: Use `Outgoing` for `Blocks`, `Incoming` for `DependsOn`.
- **Tests**: `impact_score_with_depends_on_edges`, `impact_score_leaf_node_has_zero_impact`

### M2. Optimistic locking UPDATE lacks `WHERE version = ?` guard [filament-core]
- **File**: `crates/filament-core/src/store.rs`
- **Issue**: Version check is in application code but UPDATE doesn't include version guard. TOCTOU race.
- **Fix**: Added `AND version = ?` to UPDATE WHERE clause + rows_affected check.
- **Tests**: `update_with_matching_version_bumps_version`

### M3. Shell completions hardcode `"filament"` instead of `"fl"` [filament-cli]
- **File**: `crates/filament-cli/src/commands/completions.rs`
- **Issue**: Binary was renamed to `fl` but completions still register for `filament`.
- **Fix**: Changed binary name string to `"fl"`.
- **Tests**: `completions_bash_outputs_valid_script` (updated assertion)

### M4. Timeout orphans `spawn_blocking` thread [filament-daemon]
- **File**: `crates/filament-daemon/src/dispatch.rs`
- **Issue**: On timeout, the blocking thread is dropped without awaiting, leaking a thread pool slot.
- **Fix**: Rewrote `await_agent_output` to use `tokio::select!` so blocking thread is reaped after timeout kill.

### M5. `lesson show` makes two redundant DB round-trips [filament-cli]
- **File**: `crates/filament-cli/src/commands/lesson.rs`
- **Issue**: Calls both `resolve_lesson` and `resolve_entity` for same entity.
- **Fix**: Resolve once, type-check with match.

---

## Minor Findings

### FIXED (9/18)

| ID | Issue | Fix |
|----|-------|-----|
| m1 | No validation on `default_priority` from config | `.min(4)` clamp. **Test**: `out_of_range_priority_clamped_to_max` |
| m3 | `try_auto_merge` fetches ALL events | Changed to `ORDER BY created_at DESC LIMIT ?` |
| m5 | `reconcile_stale_agent_runs` doesn't bump entity version | Added `version = version + 1`. **Test**: assertion in `reconcile_stale_runs_marks_running_as_failed` |
| m6 | `DaemonClient::list_all_agent_runs` silently swallows errors | Changed to `Self::parse_result(result)` |
| m8 | `EntityChangeset` doc comment stale | Fixed doc comment |
| m9 | Version-conflict double-printing in entity update | Exit directly after printing conflict |
| m11 | Config template says priority `1-5` but range is `0-4` | Fixed range string |
| m14 | Reservations leaked on `NeedsInput` status | Always release when subprocess exits |
| m15 | MCP `message_send` blocks "user" recipient | Added "user" special-case |
| m18 | Priority filter bar missing key labels | Added key labels and "0:Clear" |

### DEFERRED (9/18)

| ID | Issue | Why deferred |
|----|-------|-------------|
| m2 | `content_path` cannot be cleared once set | Needs `Option<Option<String>>` pattern through full stack — design decision |
| m4 | Import does N individual INSERTs | Performance optimization, not correctness bug |
| m7 | `__all__` sentinel for list-all-agent-runs | API smell but functional, needs protocol redesign |
| m10 | Pagerank JSON output order | Already fixed (Vec preserves insertion order) |
| m12 | `--claude-md` flag is effectively a no-op | Fixed — renamed to `--no-claude-md` |
| m16 | `finish_run_failed` uses 3 separate transactions | Fixed — combined into single atomic transaction |
| m17 | N+1 queries in TUI `refresh_analytics` | Performance optimization, not correctness bug |

**Note**: m10, m12, m16 were actually fixed but listed as deferred in the earlier version. Updated.

---

## Nit Findings

### FIXED (4/10)

| ID | Issue | Fix |
|----|-------|-----|
| n1 | Modulo bias in slug generation | Rejection sampling (accept values < 252) |
| n3 | Inconsistent JSON output helpers | Standardized to `output_json` helper |
| n4 | PID file read error silenced | Added `eprintln!("warning: ...")` |
| n5 | Seed error message says "CSV" | Changed to "file list" |
| n10 | `format_seconds` doesn't handle negative input | Clamp to 0 |

### DEFERRED (6/10)

| ID | Issue | Why deferred |
|----|-------|-------------|
| n2 | `unwrap_or_default` inconsistency in store | Style nit, no functional impact |
| n6 | MCP config file named with throwaway run ID | Cosmetic, works correctly |
| n7 | `TOOL_COUNT` constant could drift | Already caught by `mcp_lists_all_tools` test |
| n8 | Duplicated `status_color`/`status_style` in TUI | Cosmetic duplication, ~10 lines |
| n9 | Duplicated `truncate` wrapper in TUI | Cosmetic duplication, ~5 lines |

---

## Additional Fixes (user-requested)

- Added `expected_version: Option<i64>` param to `delete_entity` through full stack
- DELETE uses `WHERE id = ? AND version = ?` when version provided
- Distinguishes not-found vs version-mismatch errors
- **Tests**: `delete_entity_with_version_mismatch_returns_conflict`, `delete_entity_with_correct_version_succeeds`

---

## Regression Tests Added (7 new, 510 → 515+)

1. `impact_score_with_depends_on_edges` — verifies DependsOn traversal direction
2. `impact_score_leaf_node_has_zero_impact` — verifies leaf nodes have 0 impact
3. `delete_entity_with_version_mismatch_returns_conflict` — verifies version guard on delete
4. `delete_entity_with_correct_version_succeeds` — verifies happy path with version
5. `update_with_matching_version_bumps_version` — verifies WHERE version guard on update
6. `out_of_range_priority_clamped_to_max` — verifies config priority clamping
7. Version bump assertion in `reconcile_stale_runs_marks_running_as_failed` — verifies m5 fix

---

## Deferred Items for Review

These items need planning and discussion before implementation:

### Needs Design Decision
- **m2**: `content_path` cannot be cleared once set — needs `Option<Option<String>>` or sentinel pattern through CLI → handler → store stack
- **m7**: `__all__` sentinel string for listing all agent runs — needs clean protocol method

### Performance Optimizations (correctness is fine)
- **m4**: Import does N individual INSERTs instead of batch — could use multi-row INSERT or transaction batching
- **m17**: N+1 queries in TUI `refresh_analytics` — could batch-fetch entity names

### Cosmetic / Style
- **n2**: `unwrap_or_default` inconsistency in store error handling
- **n6**: MCP config file named with throwaway run ID
- **n8/n9**: Duplicated helpers in TUI (status_color, truncate)
