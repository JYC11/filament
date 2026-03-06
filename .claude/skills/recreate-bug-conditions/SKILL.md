---
name: recreate-bug-conditions
description: >
  Recreate bug conditions from a bug report snapshot created by the bug-report-format
  skill. Loads the anonymized database, replays the reproduction steps, and verifies
  the bug is reproducible in an isolated environment. Used by maintainers to validate
  and debug issues.
  Use when: the user says "recreate bug", "reproduce bug", "load bug report",
  "replay bug", "verify bug report", or provides a bug report .tar.gz file.
  Triggers on: "recreate bug", "reproduce bug", "load bug report", "replay bug",
  "verify bug report", "debug bug report".
---

# Recreate Bug Conditions — From Bug Report Snapshot

## Purpose

When a user submits a bug report (created by the `bug-report-format` skill), this skill
loads the anonymized snapshot and recreates the conditions so we can reproduce, debug,
and fix the bug.

## Input Format

A bug report is a `.tar.gz` containing:

```
fl-bug-TIMESTAMP/
  BUG-REPORT.md          # Description, expected behavior, reproduction steps
  environment.txt        # Reporter's OS, shell, fl version, db size
  fl-anonymized.db       # Anonymized copy of the database
  anonymized-export.json # JSON export of anonymized state
  recent-fl-commands.txt # Optional: recent fl commands from shell history
```

## Workflow

### 1. Unpack the Bug Report

```bash
BUG_ARCHIVE="$1"  # Path to the .tar.gz provided by user
WORK_DIR="/tmp/fl-bug-repro"
rm -rf "$WORK_DIR" && mkdir -p "$WORK_DIR"
tar xzf "$BUG_ARCHIVE" -C "$WORK_DIR"
BUG_DIR=$(ls -d "$WORK_DIR"/fl-bug-* | head -1)
echo "Unpacked bug report to: $BUG_DIR"
```

### 2. Review the Bug Report

Read and summarize for the maintainer:

```bash
cat "$BUG_DIR/BUG-REPORT.md"
cat "$BUG_DIR/environment.txt"
```

Present:
- **Summary**: What the bug is
- **Reporter's environment**: OS, fl version, db size
- **Reproduction steps**: The exact commands
- **Environment delta**: Differences between reporter's env and ours

### 3. Recreate the Database State

Load the anonymized database into a fresh project:

```bash
REPRO_DIR="/tmp/fl-bug-repro/project"
mkdir -p "$REPRO_DIR" && cd "$REPRO_DIR"
fl init

# Replace the fresh database with the anonymized one
cp "$BUG_DIR/fl-anonymized.db" .fl/fl.db

# Verify the state loaded correctly
echo "=== Entity counts ==="
fl list --json | jq 'group_by(.entity_type) | map({type: .[0].entity_type, count: length})'

echo "=== Task statuses ==="
fl task list --status all --json | jq 'group_by(.status) | map({status: .[0].status, count: length})'

echo "=== Relation count ==="
fl list --json | jq length
# Note: no direct relation count command, use export
fl export | jq '.relations | length'
```

### Alternative: Recreate from JSON Export

If the anonymized DB doesn't load (version mismatch), use the JSON export:

```bash
REPRO_DIR="/tmp/fl-bug-repro/project"
mkdir -p "$REPRO_DIR" && cd "$REPRO_DIR"
fl init
fl import --input "$BUG_DIR/anonymized-export.json"
fl list --json | jq length
```

### 4. Replay Reproduction Steps

Extract and execute the reproduction steps from BUG-REPORT.md:

```bash
# Parse reproduction steps (between ```bash fences in the Reproduction Steps section)
# Execute each step, capturing output

cd "$REPRO_DIR"

# Run each reproduction command from the bug report
# Example:
# fl update <slug> --status in_progress
# fl relate <src> blocks <tgt>
# fl task ready  # <-- this is where the bug might manifest
```

For each step:
1. Run the command
2. Capture stdout, stderr, and exit code
3. Compare against expected behavior from the report
4. Flag any divergence

### 5. Verify Reproduction

After replaying steps, determine:

| Outcome | Next Step |
|---------|-----------|
| Bug reproduces exactly | Proceed to debugging |
| Bug reproduces differently | Note the difference, may be env-dependent |
| Bug does not reproduce | Check environment delta, try with reporter's fl version |
| Crash/panic | Capture backtrace with `RUST_BACKTRACE=1` |

```bash
# If the bug involves a specific command, run it with debug output:
RUST_BACKTRACE=1 fl -vv <failing-command> 2>&1 | tee "$BUG_DIR/debug-output.txt"
```

### 6. Debugging Aids

#### Inspect the Database Directly

```bash
# Check schema version
sqlite3 .fl/fl.db "SELECT * FROM _sqlx_migrations ORDER BY version DESC LIMIT 5;"

# Check for data anomalies
sqlite3 .fl/fl.db "SELECT entity_type, status, COUNT(*) FROM entities GROUP BY entity_type, status;"
sqlite3 .fl/fl.db "SELECT relation_type, COUNT(*) FROM relations GROUP BY relation_type;"
sqlite3 .fl/fl.db "SELECT message_type, COUNT(*) FROM messages GROUP BY message_type;"

# Check for orphan relations
sqlite3 .fl/fl.db "SELECT r.id FROM relations r LEFT JOIN entities e ON r.source_id = e.id WHERE e.id IS NULL;"
sqlite3 .fl/fl.db "SELECT r.id FROM relations r LEFT JOIN entities e ON r.target_id = e.id WHERE e.id IS NULL;"

# Check entity versions (for conflict bugs)
sqlite3 .fl/fl.db "SELECT slug, version, updated_at FROM entities ORDER BY version DESC LIMIT 20;"
```

#### Compare with Fresh State

```bash
# Create a fresh project and run the same reproduction steps from scratch
CLEAN_DIR="/tmp/fl-bug-repro/clean"
mkdir -p "$CLEAN_DIR" && cd "$CLEAN_DIR"
fl init
# Run reproduction steps on clean state
# Compare behavior with loaded-state behavior
```

### 7. Document Findings

```bash
cat > "$BUG_DIR/REPRODUCTION-RESULT.md" << 'EOF'
# Reproduction Result

## Reproduced: YES / NO / PARTIAL

## Our Environment
- OS: $(uname -srm)
- fl version: $(fl --version)
- Rust: $(rustc --version)

## Steps Executed
<!-- List each step and its result -->

## Observations
<!-- What we found during reproduction -->

## Root Cause (if identified)
<!-- Technical explanation -->

## Proposed Fix
<!-- Code change description -->
EOF
```

### 8. Cleanup

After debugging is complete:

```bash
rm -rf /tmp/fl-bug-repro
```

## Tips

- Always check the **environment delta** first — version mismatches cause many "works for me" situations
- If the anonymized DB has a different schema version, run migrations: `fl init` in the repro dir will migrate
- The anonymized data preserves entity types, statuses, priorities, and relation structure — this is usually enough to reproduce graph/query bugs
- For timing-related bugs (race conditions, daemon issues), the static DB snapshot may not capture the issue — ask the reporter for additional steps
- Shell history in `recent-fl-commands.txt` often reveals the real sequence of commands (vs what the reporter remembers)
