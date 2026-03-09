# QA-07: Infrastructure

**Date**: 2026-03-09 (Session 85)
**Environment**: `/tmp/fl-qa-infra`, fresh `fl init`
**Result**: 6/6 PASS, 0 bugs

## Results

| ID | Test | Result | Notes |
|----|------|--------|-------|
| IN-01 | Export/import round-trip | PASS | 11 entities, 7 relations, 6 messages — all counts match after import |
| IN-02 | Config file (`fl.toml`) | PASS | Config at `.fl/config.toml`; `default_priority=1` correctly applied to new entities |
| IN-03 | Seed from file | PASS | `--file` creates Doc; `--files` reads file list; duplicates correctly skipped |
| IN-04 | Completions | PASS | `fl completions zsh` produces 2379-line valid script (BrokenPipe panic on `| head` is upstream clap_complete issue) |
| IN-05 | Hook install/uninstall | PASS | Install creates pre-commit hook, check runs clean, uninstall removes it |
| IN-06 | Double init idempotency | PASS | "Already initialized" message, no error, no data loss (14 entities preserved) |

## Observations

- Config file lives at `.fl/config.toml` (not `fl.toml` in project root) — `fl config path` confirms
- `fl completions zsh | head` triggers BrokenPipe panic in clap_complete (upstream crate issue, not our code)
- `fl seed --files` expects a file containing paths (one per line), not comma-separated paths
