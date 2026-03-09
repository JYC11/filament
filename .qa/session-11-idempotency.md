# QA-11: Idempotency & State Machine

**Date**: 2026-03-09
**Session**: 84
**Task**: `td52e0h4`
**Result**: 16/16 PASS, 0 bugs found

## Test Results

| # | Test | Expected | Actual | Result |
|---|------|----------|--------|--------|
| 1 | Double `fl init` | Idempotent, no error | "Already initialized", exit 0 | PASS |
| 2 | Double create (same name) | Two entities, different slugs | Created two tasks with distinct slugs | PASS |
| 3 | Double relate (same pair) | Error on duplicate | Exit 4, "relation already exists" | PASS |
| 4 | Double `task close` | Idempotent | Both succeed, exit 0 | PASS |
| 5 | Double reserve (same glob, same agent) | Creates second reservation | Two reservations created, exit 0 | PASS |
| 6 | Double release | Idempotent | Both succeed, exit 0 | PASS |
| 7 | Double message (same body) | Two separate messages | Two messages with different IDs | PASS |
| 8 | Double lesson (same title) | Two entities, different slugs | Created two lessons with distinct slugs | PASS |
| 9 | closed → open | Allowed | Updated, status=open, exit 0 | PASS |
| 10 | closed → in_progress | Allowed | Updated, status=in_progress, exit 0 | PASS |
| 11 | blocked → closed | Allowed | Closed, status=closed, exit 0 | PASS |
| 12 | open → closed | Allowed | Closed, status=closed, exit 0 | PASS |
| 13 | in_progress → open | Allowed | Updated, status=open, exit 0 | PASS |
| 14 | Remove then reference | Not found errors | inspect/update/relate all exit 3 with hint | PASS |
| 15 | Assign closed task | Allowed (permissive) | Assigned, exit 0 | PASS |
| 16 | Block open task | Blocked excluded from ready | Blocked task not in `task ready`, blocker is | PASS |

## Observations

- **Names are not unique identifiers** — multiple entities can share the same name. Slugs provide uniqueness.
- **Status transitions are unrestricted** — any status can move to any other status. No state machine enforcement.
- **Double reserve creates duplicates** — two reservations for the same glob by the same agent. Not a bug (advisory locks), but worth noting.
- **Assigning closed tasks is allowed** — permissive behavior, no validation on task status for assignment.
- **All "not found" errors** return exit code 3 with helpful hints.
- **Duplicate relations are properly rejected** — exit 4 with descriptive error message.

## Environment

- macOS Darwin 25.3.0
- Rust 1.94.0
- fl installed at ~/.local/bin/fl
- Temp dir: /tmp/fl-qa-idempotency
