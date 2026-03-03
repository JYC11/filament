---
name: manual-qa
description: >
  Run manual QA against a CLI binary using a real temp project. Build the release binary,
  create an isolated test directory, execute commands, capture stdout/stderr/exit codes,
  and produce a structured results file in the project's .qa/ folder.
  Use when: the user asks to "manual test", "manual QA", "QA the CLI", "smoke test",
  "exercise the binary", "end-to-end QA", or wants to verify CLI behavior outside of
  automated tests.
  Triggers on: "manual qa", "manual test", "smoke test", "QA", "exercise the CLI",
  "test the binary", "end-to-end test the CLI".
---

# Manual QA

Manual QA exercises the real release binary end-to-end against a temp project directory.
It catches issues automated tests miss: output formatting, error message quality, exit
codes, cross-command workflows, and CLI argument ergonomics.

## When to Run

- After completing a development phase or code review round
- After fixing bugs found in prior QA
- Before importing real project data (self-tracking milestones)
- When adding new command groups

## Workflow

### 1. Setup

```bash
cargo build --release
cd /tmp && rm -rf <project>-qa && mkdir <project>-qa && cd <project>-qa
<binary> init
```

### 2. Plan Test Cases

Organize by command group with TC-XX IDs. See [references/test-plan-template.md](references/test-plan-template.md).

Typical groups:
- Init (idempotency, double-init)
- CRUD (add, list, inspect/show, update, remove)
- Error handling (invalid input, not-found, missing flags, JSON errors)
- Relations / linking between entities
- Workflows (multi-step sequences, blocking/unblocking)
- Output modes (human-readable vs `--json`)

### 3. Execute and Capture

Write results to `.qa/manual-qa-results.md` in the project root.

For each test case, capture:
- The exact command run
- Full stdout and stderr
- Exit code
- Pass/fail verdict with reasoning

Use shell scripting to append results:
```bash
R="$PROJECT_ROOT/.qa/manual-qa-results.md"
cat >> "$R" <<'EOF'
### TC-XX: Description
EOF
echo '```' >> "$R"
$BINARY command args >> "$R" 2>&1; echo "exit: $?" >> "$R"
echo '```' >> "$R"
```

### 4. Analyze Results

After all tests run:
- Mark each TC as PASS or FAIL with reasoning
- For failures, document expected vs actual
- Create a "Bugs Found" section with severity, description, fix status
- Write a summary table by group

### 5. Fix and Verify

For each bug found:
1. Fix the code
2. Run `make ci` (or equivalent) to ensure no regressions
3. Rebuild release binary
4. Re-run the failing TC to confirm the fix
5. Update the results file with fix status

## Key Things to Verify

- **Output formatting**: alignment, truncation, priority/status display
- **Error messages**: human-readable with hints, correct exit codes
- **JSON mode**: valid JSON, correct field names, null handling
- **Cross-command consistency**: entity created in `add` visible in `list`, `inspect`, `context`
- **Bidirectional behavior**: if graph traversal exists, test both directions
- **Idempotency**: double-init, double-release, etc.
- **Edge cases**: empty lists, no relations, single-element paths

## Results File Structure

See [references/results-template.md](references/results-template.md) for the full template.

Results use datetime in the filename to maintain a log across runs:

```
.qa/
  manual-qa-YYYY-MM-DDTHHMM.md   # e.g. manual-qa-2026-03-03T1430.md
```

Each run produces a new timestamped file. Never overwrite previous results.

## Common Pitfalls

- Hardcoding `**Result**: PASS` — always verify actual output before marking
- Using wrong argument style (positional vs `--flag`) — check `--help` first
- Forgetting to test `--json` mode alongside human output
- Not testing error paths (invalid input, not-found, conflicts)
- Not rebuilding after fixes before re-testing
