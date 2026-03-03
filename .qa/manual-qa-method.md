# Filament CLI — Manual QA Method

## Purpose

Manual QA complements the automated test suite by exercising the real CLI binary
end-to-end against a temp project, verifying output formatting, error messages,
exit codes, and cross-command workflows that integration tests may not cover.

## Setup

```bash
# Build release binary
cargo build --release

# Create isolated QA directory
cd /tmp && rm -rf filament-qa && mkdir filament-qa && cd filament-qa

# Initialize
/path/to/target/release/filament init
```

## Test Organization

Tests are grouped by command area with IDs for traceability:

| Group | TC Range | What it covers |
|-------|----------|----------------|
| Init | TC-01–02 | `filament init`, double-init idempotency |
| Entity CRUD | TC-03–15 | add, list, inspect, update, remove, read, content files |
| Error Handling | TC-16–20 | Invalid types/status, missing flags, not-found, JSON errors |
| Relations | TC-21–24 | relate, unrelate, inspect relations, invalid entity |
| Tasks | TC-25–36 | add, list, show, close, ready, blocks, assign, critical-path, --unblocked |
| Context | TC-37–39 | Neighborhood query, no-relations, bidirectional (incoming edges) |
| Messages | TC-40–43 | send, inbox, read, inbox-after-read |
| Reservations | TC-44–47 | reserve, list, release, exclusive conflict |
| JSON Output | TC-48–50 | --json add, --json list, --json task ready |

## Running Method

1. **Build**: `cargo build --release`
2. **Fresh project**: Always start with a clean `/tmp/filament-qa`
3. **Setup entities**: Create a realistic graph (services, modules, agents, docs) before testing
4. **Capture output**: Redirect stdout/stderr to a results file with exit codes
5. **Verify manually**: Check output matches expectations, not just exit codes
6. **Mark pass/fail**: Document actual vs expected for failures

## Key Things to Check

- **Output formatting**: Column alignment, truncation, priority display `[P0]`
- **Error messages**: Human-readable with hints, correct exit codes (3=not found, 4=validation, 6=conflict)
- **JSON mode**: Valid JSON, correct field names, null handling
- **Cross-command consistency**: Entity created in `add` shows in `list`, `inspect`, `context`
- **Bidirectional traversal**: Context query finds both outgoing and incoming neighbors
- **Blocking semantics**: Blocked tasks excluded from `ready`, unblocked after blocker closed
- **Reservation conflicts**: Only `--exclusive` reservations trigger conflicts
- **Idempotency**: `init` on existing project, `release` on non-existent reservation

## Results File Format

Results go in `.qa/manual-qa-results.md` with:
- Date, binary path, QA directory
- Test plan (numbered list)
- Each test: header, code block with output, pass/fail verdict
- Bugs found section (with severity, description, expected/actual, fix status)
- Summary table

## When to Run

- After completing a development phase (before self-tracking import)
- After fixing bugs found in code review
- Before major releases
- When adding new command groups

## Gotchas

- `message inbox` takes a positional arg, not `--agent`
- Non-exclusive reservations don't conflict (by design, ADR-008)
- `filament init` returns exit 0 on double-init (idempotent)
- Entity names are NOT unique — `inspect` returns first match by name
