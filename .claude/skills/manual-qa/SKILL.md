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

## Testing Mindset: Aggressive, Malicious, Stupid

QA is not about confirming the software works — it's about proving it doesn't. Adopt three
personas simultaneously:

**Aggressive** — Push every boundary. Feed maximum-length strings, flood with rapid
commands, chain operations in unexpected orders, hit the same endpoint 50 times in a row.
Don't just test the documented limits — exceed them. If a field accepts a name, paste in
2000 characters. If a command takes an ID, pass 50 IDs. Run `update` before `add`. Run
`remove` twice. Nest operations that shouldn't nest.

**Malicious** — Think like an attacker trying to corrupt data or crash the process. Inject
SQL fragments, shell metacharacters (`$(rm -rf /)`), null bytes, control characters, unicode
edge cases (RTL marks, zero-width joiners, emoji sequences). Try to escape quoted strings.
Pass `--flag=value` where positional args are expected and vice versa. Feed valid JSON with
wrong schemas. Send commands mid-transaction. Try to create circular dependencies. Try to
reference entities across project boundaries.

**Stupid** — Be the user who read nothing and assumes everything. Omit required flags. Pass
a status where a priority is expected. Spell commands wrong and check if the suggestion is
helpful. Use the wrong subcommand. Give a slug where a UUID is expected. Leave trailing
whitespace in values. Pass empty strings. Use `--json` on commands that don't support it.
Pass `--help` mid-argument list. Type numbers where strings go and strings where numbers go.

### What This Looks Like in Practice

Every test group should include at least 2-3 "break it" cases alongside the happy path:

| Happy path TC | Aggressive TC | Malicious TC | Stupid TC |
|---------------|---------------|--------------|-----------|
| Add entity with valid name | Add entity with 500-char name | Add entity with name `'; DROP TABLE entities;--` | Add entity with no name at all |
| List all entities | List after creating 100+ entities | List with `--type` set to `../../etc/passwd` | List with `--type` set to a misspelled value |
| Update a field | Update every field at once | Update with JSON payload in a text field | Update with `--status` and `--priority` swapped |
| Remove entity | Remove entity then inspect it | Remove same entity in two concurrent calls | Remove with a slug that looks like a flag (`--abc`) |

### Bug Severity from Aggressive Testing

Bugs found through aggressive testing get elevated severity:
- **Panic/crash** → always Critical (the binary must never crash on bad input)
- **Data corruption** → always Critical (silent data loss or mangled state)
- **Leaked internal errors** (raw SQL, stack traces) → High (information leak + bad UX)
- **Wrong exit code** → Medium (breaks scripting and CI pipelines)
- **Missing/bad error message** → Medium (user can't self-diagnose)

## Stress Test QA — Large Volume Data Handling

When the user asks for "stress test", "load test", "volume test", or "scale test", run this
extended protocol that pushes the system under heavy data loads.

### Setup

```bash
cargo build --release
cd /tmp && rm -rf <project>-stress-qa && mkdir <project>-stress-qa && cd <project>-stress-qa
<binary> init
```

### Phase 1: Bulk Entity Creation

Create a large number of entities rapidly using a shell loop. Capture timing.

```bash
R="$PROJECT_ROOT/.qa/stress-qa-results.md"
echo "## Bulk Entity Creation" >> "$R"
START=$(date +%s)
for i in $(seq 1 500); do
  TYPE=$(echo "task module service agent plan doc lesson" | tr ' ' '\n' | shuf -n1)
  $BINARY add "entity-$i" --type "$TYPE" --summary "Stress test entity $i with type $TYPE" 2>&1 | tail -1
done
END=$(date +%s)
echo "Created 500 entities in $((END-START)) seconds" >> "$R"
```

Verify: `$BINARY list | wc -l` should show 500 entities.

### Phase 2: Bulk Relation Creation

Create relations between random entity pairs:

```bash
# Get all slugs
SLUGS=($($BINARY list --json | jq -r '.[].slug'))
echo "## Bulk Relation Creation" >> "$R"
START=$(date +%s)
TYPES=("relates_to" "depends_on" "produces" "owns")
for i in $(seq 1 1000); do
  SRC=${SLUGS[$((RANDOM % ${#SLUGS[@]}))]}
  TGT=${SLUGS[$((RANDOM % ${#SLUGS[@]}))]}
  REL=${TYPES[$((RANDOM % ${#TYPES[@]}))]}
  $BINARY relate "$SRC" "$REL" "$TGT" 2>/dev/null
done
END=$(date +%s)
echo "Attempted 1000 relations in $((END-START)) seconds" >> "$R"
```

### Phase 3: Bulk Message Volume

```bash
# Create 10 agents, send 50 messages each
AGENTS=()
for i in $(seq 1 10); do
  SLUG=$($BINARY add "stress-agent-$i" --type agent --summary "Stress agent $i" --json | jq -r '.slug')
  AGENTS+=("$SLUG")
done
START=$(date +%s)
for i in $(seq 1 500); do
  FROM=${AGENTS[$((RANDOM % 10))]}
  TO=${AGENTS[$(( (RANDOM + 1) % 10))]}
  $BINARY message send --from "$FROM" --to "$TO" --body "Stress message $i: $(head -c 200 /dev/urandom | base64 | head -c 100)" --type text 2>/dev/null
done
END=$(date +%s)
echo "Sent 500 messages in $((END-START)) seconds" >> "$R"
```

### Phase 4: Performance Queries

Time critical operations at scale:

```bash
echo "## Performance at Scale" >> "$R"

# List all
time $BINARY list > /dev/null 2>&1

# Search
time $BINARY search "stress entity" > /dev/null 2>&1

# Graph operations
SLUG=${SLUGS[0]}
time $BINARY context --around "$SLUG" --depth 3 > /dev/null 2>&1
time $BINARY pagerank > /dev/null 2>&1
time $BINARY degree > /dev/null 2>&1

# Task operations
time $BINARY task ready > /dev/null 2>&1
```

### Phase 5: Export/Import at Scale

```bash
# Export full state
time $BINARY export --output /tmp/<project>-stress-qa/stress-snapshot.json
SIZE=$(wc -c < /tmp/<project>-stress-qa/stress-snapshot.json)
echo "Export size: $SIZE bytes" >> "$R"

# Import into fresh project
mkdir -p /tmp/<project>-stress-qa-2 && cd /tmp/<project>-stress-qa-2
$BINARY init
time $BINARY import --input /tmp/<project>-stress-qa/stress-snapshot.json

# Verify counts match
ORIG_COUNT=$($BINARY list --json 2>/dev/null | jq length)
cd /tmp/<project>-stress-qa-2
IMPORT_COUNT=$($BINARY list --json 2>/dev/null | jq length)
echo "Original: $ORIG_COUNT, Imported: $IMPORT_COUNT" >> "$R"
```

### Phase 6: Concurrent Access

```bash
# Fire 10 parallel list commands
echo "## Concurrent Access" >> "$R"
for i in $(seq 1 10); do
  $BINARY list > /dev/null 2>&1 &
done
wait
echo "10 parallel list commands: all succeeded" >> "$R"

# Fire 10 parallel writes
for i in $(seq 1 10); do
  $BINARY add "concurrent-$i" --type doc --summary "Concurrent write $i" > /dev/null 2>&1 &
done
wait
echo "10 parallel writes: check for errors" >> "$R"
```

### Pass/Fail Criteria

| Metric | Pass | Fail |
|--------|------|------|
| 500 entity creation | <60s | >120s or errors |
| 1000 relation creation | <120s | >240s or errors |
| List at 500 entities | <2s | >5s |
| Search at 500 entities | <2s | >5s |
| PageRank at 500+1000 | <5s | >15s |
| Export 500+1000 | <10s | >30s |
| Import full snapshot | <30s | >60s |
| Concurrent reads | 0 errors | Any DB lock error |
| Concurrent writes | 0 corruption | Any data loss |
| Export/import round-trip | Counts match | Mismatch |

### Results File

Save to `.qa/stress-qa-YYYY-MM-DDTHHMM.md` alongside regular QA results.

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
