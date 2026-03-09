# QA Session 8: Error Handling & Adversarial

**Date**: 2026-03-09
**Binary**: `fl` (release build)
**Test env**: `/tmp/fl-qa-error` (fresh `fl init`)

## Results

| ID | Test | Result | Notes |
|----|------|--------|-------|
| EH-01 | SQL injection in names | PASS | `'; DROP TABLE entities;--` stored as data |
| EH-02 | Shell metacharacters | PASS | `$(rm -rf /)`, backticks, pipes all stored safely |
| EH-03 | Null bytes & control chars | PASS | Null byte in name accepted; `\x01` in JSON facts rejected (invalid escape) |
| EH-04 | Invalid JSON in --facts | PASS | Exit 4, clear validation error with hint |
| EH-05 | Nonexistent slug | **BUG** | Message says ENTITY_NOT_FOUND but exit code is 4 (validation), should be 3 (not found) |
| EH-06 | Wrong flag types | PASS | `--priority abc` → exit 2, clear clap error |
| EH-07 | Missing required flags | PASS | Exit 2, shows usage hint |
| EH-08 | `--json` error format | PASS | All 4 fields present: code, message, hint, retryable |
| EH-09 | Very long arguments | PASS | 10KB name and 100KB summary both accepted, no crash |
| EH-10 | Concurrent writes | PASS | 10 parallel creates all succeed, no DB lock errors |

## Bugs Found & Fixed

### BUG-QA8-01: Daemon errors lost exit code (all became exit 4) — FIXED

**Severity**: Medium
**Reproduce**: `fl inspect zzzzzzzz` (any nonexistent slug, with daemon auto-start)
**Expected**: Exit code 3 (not found), error code ENTITY_NOT_FOUND
**Actual**: Exit code 4 (validation), error code PROTOCOL_ERROR
**Root cause**: `DaemonClient::call()` wrapped all daemon errors in `FilamentError::Protocol(String)`, losing the original error variant and exit code.
**Fix**: Added `DaemonError` variant to `FilamentError` carrying full structured error metadata (code, message, hint, retryable, exit_code). Added `exit_code` field to `StructuredError` for wire transport. Regression test in `errors.rs`.

### BUG-QA8-02: macOS SIGKILL on installed binary — FIXED

**Severity**: High
**Reproduce**: `make install && fl --version` → exit 137 (SIGKILL)
**Root cause**: macOS code signing enforcement kills ad-hoc signed binaries after `cp` invalidates the signature.
**Fix**: Added `codesign --sign - --force` to `install.sh` after copy.

## Summary

- **Tests**: 10/10 executed
- **Passed**: 10/10 (after fixes)
- **Bugs found**: 2 (both fixed)
- **Panics**: 0
- **Security issues**: 0 (SQL injection, shell injection all safe)
