# QA Session 02: Core CRUD — Happy Path + Edge Cases

**Date**: 2026-03-09
**Project**: `/tmp/fl-qa-crud`
**Result**: 9/9 PASS (1 bug found and fixed during session)

## Results

| ID | Test | Result | Notes |
|----|------|--------|-------|
| CR-01 | Create each of 7 entity types | PASS | task, module, service, agent, plan (--content), doc (--content), lesson — all created |
| CR-02 | Update each field individually | PASS | summary, status, priority, facts, content all work (after bug fix) |
| CR-03 | Remove entity cascades relations | PASS | Removing blocker cascades relation, blocked entity appears in `task ready` |
| CR-04 | Unicode entity names | PASS | CJK (测试模块), emoji (🔥🎯🚀), RTL (مرحبا), combining chars (café résumé naïve) |
| CR-05 | Empty string handling | PASS | Empty summary accepted (omitted from display), empty `{}` facts accepted |
| CR-06 | Priority boundaries | PASS | 0 and 4 accepted, 5 rejected (exit 4 validation), -1 rejected at CLI parse |
| CR-07 | Status transitions | PASS | open -> in_progress -> blocked -> in_progress -> closed all succeed |
| CR-08 | Duplicate names | PASS | Same name creates two entities with different slugs |
| CR-09 | JSON output for all CRUD | PASS | add, list, inspect, update, remove all produce valid JSON with --json |

## Bugs Found & Fixed

### Bug 1: `fl update` missing `--priority`, `--facts`, `--content` flags (FIXED)

**Severity**: Medium
**Description**: `fl update` only supported `--summary` and `--status`. Priority, key_facts, and content_path were not updatable via CLI despite being supported in the store layer.
**Fix**: Added `--priority`, `--facts`, `--content` flags to `UpdateArgs` struct. Priority validated through `Priority::new()` newtype, facts validated as JSON, content validated for file existence. 6 new tests added.
**Files changed**: `crates/filament-cli/src/commands/entity.rs`, `crates/filament-cli/tests/entity.rs`

### Bug 2: Dispatch daemon tests checking `err.to_string()` for error codes (FIXED)

**Severity**: Low (test-only, introduced by session 81 daemon error exit code fix)
**Description**: `dispatch_closed_task_fails` and `dispatch_agent_already_running` tests checked `err.to_string().contains("AGENT_DISPATCH_FAILED")` but `DaemonError` Display renders the message, not the code. After session 81's error protocol changes, the error flows through as `DaemonError` with the code in a separate field.
**Fix**: Changed assertions to use `err.error_code() == "AGENT_DISPATCH_FAILED"` instead of string matching on Display output.
**Files changed**: `crates/filament-daemon/tests/dispatch.rs`

## Test Count

- Before: 530 tests
- After: 536 tests (+6 new entity update tests)
- All pass, zero clippy warnings
