---
name: filament-bug-report-format
description: >
  Create an anonymized snapshot of the local fl.db state along with structured
  bug reproduction steps. The snapshot strips user-identifiable content while
  preserving schema, entity structure, relations, and the conditions that
  triggered the bug. Output is a self-contained directory ready to attach to
  a bug report.
  Use when: the user says "bug report", "report a bug", "create bug snapshot",
  "anonymize db", "submit bug", "reproduction steps", or wants to share their
  local state for debugging.
  Triggers on: "bug report", "report bug", "bug snapshot", "anonymize",
  "reproduction steps", "create bug report".
---

# Bug Report Format — Anonymized Snapshot + Reproduction Steps

## Purpose

Users hit bugs in their real projects. To help us reproduce, they need to share:
1. Their database state (anonymized — no real project names, summaries, or content)
2. Exact reproduction steps
3. Environment info

This skill creates a self-contained bug report directory.

## Workflow

### 1. Gather Bug Information

Ask the user for:
- **What happened?** (the bug symptom)
- **What did you expect?** (correct behavior)
- **What commands triggered it?** (exact CLI commands)
- **Is it reproducible?** (always / sometimes / once)

### 2. Create Bug Report Directory

```bash
TIMESTAMP=$(date +%Y%m%dT%H%M%S)
BUG_DIR="/tmp/fl-bug-$TIMESTAMP"
mkdir -p "$BUG_DIR"
```

### 3. Capture Environment

```bash
cat > "$BUG_DIR/environment.txt" << EOF
date: $(date -u +%Y-%m-%dT%H:%M:%SZ)
os: $(uname -srm)
shell: $SHELL ($(${SHELL} --version 2>&1 | head -1))
rust: $(rustc --version 2>/dev/null || echo "not installed")
fl_version: $(fl --version 2>/dev/null || echo "unknown")
fl_binary: $(which fl 2>/dev/null || echo "not found")
db_path: $(ls .fl/fl.db 2>/dev/null && echo ".fl/fl.db" || echo "not found")
db_size: $(wc -c < .fl/fl.db 2>/dev/null || echo "0") bytes
config: $(cat fl.toml 2>/dev/null || echo "no config")
EOF
```

### 4. Create Anonymized Database Snapshot

The anonymization replaces user content while preserving structure, types, relations,
and entity counts. This is critical — the schema and relationship graph often reveal
the bug, but the actual content is private.

```bash
# Copy the database
cp .fl/fl.db "$BUG_DIR/fl-anonymized.db"

# Anonymize entity names and summaries
sqlite3 "$BUG_DIR/fl-anonymized.db" << 'SQL'
-- Replace names with type + sequential number
UPDATE entities SET name = entity_type || '-' || CAST(rowid AS TEXT);

-- Replace summaries with placeholder preserving length
UPDATE entities SET summary = 'Summary for ' || entity_type || ' entity #' || CAST(rowid AS TEXT);

-- Null out content paths (files are not included)
UPDATE entities SET content_path = NULL;

-- Anonymize key_facts: preserve JSON structure but replace values
-- Keep keys (they reveal schema patterns), replace string values
UPDATE entities SET key_facts = CASE
  WHEN key_facts IS NULL THEN NULL
  WHEN key_facts = '{}' THEN '{}'
  ELSE json_object('_note', 'key_facts anonymized', '_key_count',
    json_array_length(CASE WHEN json_type(key_facts) = 'object' THEN json_array(key_facts) ELSE '[]' END))
END;

-- Anonymize messages
UPDATE messages SET body = 'Anonymized message #' || CAST(rowid AS TEXT)
  || ' (type: ' || message_type || ')';

-- Anonymize relation summaries
UPDATE relations SET summary = CASE
  WHEN summary IS NOT NULL THEN 'Relation summary #' || CAST(rowid AS TEXT)
  ELSE NULL
END;

-- Preserve: entity_type, status, priority, slugs, UUIDs, timestamps,
--           relation types, message types, created_at/updated_at
-- These are structural and needed for reproduction.

-- Verify anonymization
SELECT 'Entities: ' || COUNT(*) FROM entities;
SELECT 'Relations: ' || COUNT(*) FROM relations;
SELECT 'Messages: ' || COUNT(*) FROM messages;
SQL
```

### 5. Export Anonymized Snapshot as JSON

```bash
# Also export as JSON for readability
cd "$BUG_DIR" && mkdir -p temp-project && cd temp-project
fl init
# Import the anonymized DB directly (it's already a valid fl.db)
cp ../fl-anonymized.db .fl/fl.db
fl export --output "$BUG_DIR/anonymized-export.json"
cd "$BUG_DIR" && rm -rf temp-project
```

### 6. Write Reproduction Steps

Create the bug report file. Fill in from user's answers:

```bash
cat > "$BUG_DIR/BUG-REPORT.md" << 'REPORTEOF'
# Bug Report

## Summary
<!-- One-line description of the bug -->

## Environment
See `environment.txt` for full details.

## What Happened
<!-- Describe the bug symptom. Include exact error messages, exit codes. -->

## Expected Behavior
<!-- What should have happened instead -->

## Reproduction Steps

```bash
# Step 1: Setup (if needed)
fl init

# Step 2: Create preconditions
# (commands to set up the state that triggers the bug)

# Step 3: Trigger the bug
# (the exact command that fails)
```

## Reproduction Rate
<!-- always / sometimes / once -->

## Attachments
- `fl-anonymized.db` — anonymized copy of the database at time of bug
- `anonymized-export.json` — JSON export of anonymized state
- `environment.txt` — system and tool versions

## Additional Context
<!-- Any other relevant information: was daemon running? config changes? recent upgrade? -->
REPORTEOF
```

### 7. Capture Recent CLI History (Optional)

If the user consents, capture recent `fl` commands from shell history:

```bash
# Extract recent fl commands (last 50)
grep -E '^\s*fl\s' ~/.zsh_history 2>/dev/null | tail -50 > "$BUG_DIR/recent-fl-commands.txt" || \
grep -E '^\s*fl\s' ~/.bash_history 2>/dev/null | tail -50 > "$BUG_DIR/recent-fl-commands.txt" || \
echo "No shell history found" > "$BUG_DIR/recent-fl-commands.txt"
```

### 8. Package the Report

```bash
cd /tmp
tar czf "fl-bug-$TIMESTAMP.tar.gz" "fl-bug-$TIMESTAMP/"
echo "Bug report created: /tmp/fl-bug-$TIMESTAMP.tar.gz"
echo "Contents:"
ls -la "$BUG_DIR/"
echo ""
echo "Size: $(wc -c < /tmp/fl-bug-$TIMESTAMP.tar.gz) bytes"
```

### 9. Present to User

Tell the user:
- The bug report is at `/tmp/fl-bug-TIMESTAMP.tar.gz`
- Review `BUG-REPORT.md` and fill in the marked sections
- The database is anonymized: names, summaries, messages, and key_facts are replaced
- Preserved: entity types, statuses, priorities, slugs, timestamps, relations, message types
- They can inspect `anonymized-export.json` to verify nothing sensitive leaked
- Submit the `.tar.gz` with their issue

## What Gets Anonymized vs Preserved

| Field | Anonymized | Preserved | Why |
|-------|-----------|-----------|-----|
| Entity name | Yes | | Could contain project names |
| Summary | Yes | | Could contain project details |
| Key facts | Yes (structure only) | | Could contain secrets/details |
| Content path | Yes (NULL) | | File paths leak project structure |
| Message body | Yes | | Could contain anything |
| Relation summary | Yes | | Could contain details |
| Entity type | | Yes | Needed for reproduction |
| Status | | Yes | Needed for reproduction |
| Priority | | Yes | Needed for reproduction |
| Slug | | Yes | Needed for reproduction steps |
| UUID | | Yes | Needed for relation integrity |
| Timestamps | | Yes | Needed for ordering/debugging |
| Relation type | | Yes | Needed for graph structure |
| Message type | | Yes | Needed for escalation bugs |
| Version | | Yes | Needed for conflict bugs |

## Anonymization Verification Checklist

Before sharing, the user should verify:
- [ ] No real project names in entity names
- [ ] No real descriptions in summaries
- [ ] No secrets in key_facts
- [ ] No file paths in content_path
- [ ] No real conversation text in messages
- [ ] `grep -ri "company\|project\|secret" fl-anonymized.db` returns nothing sensitive
