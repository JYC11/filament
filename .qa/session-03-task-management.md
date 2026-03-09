# QA-03: Task Management

**Date**: 2026-03-09
**Session**: 84
**Task**: `3rrqsw65`
**Result**: 5/6 PASS, 1 BUG found and fixed

## Test Results

| # | Test | Expected | Actual | Result |
|---|------|----------|--------|--------|
| 1 | task add with all flags | Creates task with priority, depends-on | P0 task with depends_on relation created, --blocks validates target exists | PASS |
| 2 | task ready ranking | Ordered by priority (P0 first) | P0 → P1 → P2 → P4 correct order | PASS |
| 3 | task close unblocks dependents | Dependent appears in ready after blocker closed | Dependent appeared in `task ready` after `task close` | PASS |
| 4 | blocker-depth 5+ levels | Reports depth 5 for 6-node chain | "blocker depth 5" reported correctly, root shows "no upstream blockers" | PASS |
| 5 | task assign/unassign | Assign creates relation, unrelate removes it | Assign creates `agent -> task (assigned_to)` relation; unrelate removes it. Direction: agent→task | PASS |
| 6 | circular dependency prevention | Should reject cycle-creating relation | **BUG**: Cycle A→B→C→A was allowed (exit 0). All 3 tasks deadlocked — none appear in `task ready` | **FAIL** |

## Bug: Circular Dependency Not Prevented

**Severity**: Medium (data integrity issue, creates unresolvable deadlocks)

**Reproduction**:
```bash
fl init
fl task add "cyc-a" --summary "a"
fl task add "cyc-b" --summary "b"
fl task add "cyc-c" --summary "c"
fl relate <A> blocks <B>    # OK
fl relate <B> blocks <C>    # OK
fl relate <C> blocks <A>    # BUG: succeeds (exit 0), should fail
fl task ready               # None of A, B, C appear — permanent deadlock
```

**Observed behavior**:
- Self-blocks are correctly prevented (exit 4, "source_id and target_id must differ")
- Transitive cycles (A→B→C→A) are NOT prevented — relation is created successfully
- `blocker-depth` reports depth 2 for all three nodes (handles cycle gracefully, doesn't loop)
- All three tasks are permanently blocked — can never become ready

**Expected behavior**: `fl relate C blocks A` should return an error like "relation would create a cycle" with exit 4.

**Impact**: Tasks in a cycle are permanently deadlocked. The only recovery is manually removing one of the cycle edges with `fl unrelate`.

**Fix**: Added cycle detection in `store::create_relation()` using a recursive CTE that traverses the normalized dependency DAG (following `blocks` edges forward and `depends_on` edges backward). Returns `CycleDetected` error with exit code 5 before the INSERT. Covers both direct and daemon code paths.

**Files changed**:
- `crates/filament-core/src/store.rs` — cycle detection query
- `crates/filament-cli/tests/relation.rs` — regression test (`relate_circular_dependency_prevented`)
- `crates/filament-core/tests/graph.rs` — updated `different_relation_types_between_same_entities` to use non-conflicting types, added `contradictory_dependency_types_rejected_as_cycle`

**Tests added**: 2 (CLI regression + core unit test). Total: 538.

## Observations

- `--blocks` and `--depends-on` flags on `task add` validate that the target entity exists before creating
- `assigned_to` relation direction is agent → task (not task → agent)
- `blocker-depth` correctly handles both deep chains and cycles without infinite loops
- Priority ranking in `task ready` works correctly across all 5 priority levels

## Environment

- macOS Darwin 25.3.0
- Rust 1.94.0
- fl installed at ~/.local/bin/fl
- Temp dir: /tmp/fl-qa-taskmgmt
