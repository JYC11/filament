# Aggressive QA Plan — Break Everything

**Goal**: Find bugs by attacking edge cases, concurrency, state corruption, and resource pressure.
**Previous QA**: 268 test cases (99.6% pass) — but all sequential, single-client, mostly happy-path.

## Structure

| Round | Scenario | Focus |
|-------|----------|-------|
| 1 | CLI-only (no daemon) | Storage, graph, validation, boundary attacks |
| 2 | CLI through daemon | Repeat Round 1 via daemon + add concurrency/lifecycle attacks |
| 3 | TUI alongside Round 2 | Rendering, live refresh, daemon death, concurrent mutation |
| 4 | Scripted stress | Automated bash scripts for parallel/race/load scenarios |

---

## Round 1: Single Agent CLI (Direct DB)

**Setup**: `filament init` in fresh temp dir. No daemon. All operations hit SQLite directly.

### R1-A: Graph Cycle Attacks (TC01-TC05)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 01 | Create A, B, C (tasks). Relate: A depends_on B, B depends_on C, C depends_on A | Relations created (no cycle prevention?) | | |
| 02 | `task critical-path A` on cyclic graph | Terminates (no infinite loop), returns error or partial path | | |
| 03 | `task ready` with all tasks in a cycle | No tasks returned (all blocked) or error — must not hang | | |
| 04 | `context --around A --depth 5` on cyclic graph | Terminates, doesn't revisit nodes | | |
| 05 | Delete one entity in the cycle, then re-run `task ready` | Unblocked tasks now appear | | |

### R1-B: Cascade & Scale Attacks (TC06-TC12)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 06 | Create chain of 50 tasks: T1→T2→...→T50 (each depends_on next) | All created | | |
| 07 | `task critical-path T1` on 50-deep chain | Returns full path (50 nodes), completes in <5s | | |
| 08 | `task ready` on 50-deep chain (only T50 is ready) | Returns only T50 | | |
| 09 | Close T50, then `task ready` — T49 unblocks | Returns T49 | | |
| 10 | Delete T1 (root of 50-chain) — cascade behavior | Cascade deletes relations? Entity persists but relations cleaned? | | |
| 11 | Create 100 entities of mixed types in rapid succession | All created, `list` returns 100 | | |
| 12 | `context --around <entity> --depth 10` on wide graph (1 entity with 30 direct deps) | Returns all neighbors, terminates | | |

### R1-C: Unicode & Boundary Attacks (TC13-TC22)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 13 | Entity name with emoji: `add "🔥fire🔥" --type task` | Created (NonEmptyString allows Unicode) | | |
| 14 | Entity name with ZWJ sequence: `add "👨‍👩‍👧‍👦family" --type task` | Created | | |
| 15 | Entity name with RTL override: `add $'\u202Ereversed' --type task` | Created or rejected — must not corrupt display | | |
| 16 | Entity name all whitespace: `add "   " --type task` | Rejected (NonEmptyString trims → empty) | | |
| 17 | Entity name single char: `add "x" --type task` | Created | | |
| 18 | Summary with 10,000 chars | Created or rejected with clear error | | |
| 19 | `--facts '{"key": null}'` — null value in JSON | Stored or rejected — no panic | | |
| 20 | `--facts 'not json'` — invalid JSON | Exit 4, validation error | | |
| 21 | `--content /nonexistent/path` | Error, no panic, clear message | | |
| 22 | `--content /dev/null` — empty file | Created with empty content or rejected | | |

### R1-D: State Machine Violations (TC23-TC30)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 23 | Close an already-closed task | Error or idempotent — not panic | | |
| 24 | Close a non-task entity (module, service) | Exit 4, type mismatch error | | |
| 25 | Assign a task to nonexistent agent | Error — agent must exist | | |
| 26 | Assign an already-assigned task to a different agent | Reassignment or conflict error | | |
| 27 | Update status to "open" on a closed task (reopen) | Works or error — consistent behavior | | |
| 28 | Delete entity that has messages in its inbox | Entity deleted, messages orphaned or cascade deleted | | |
| 29 | Mark an already-read message as read again | `MessageAlreadyRead` error (per ADR) | | |
| 30 | Send message to deleted agent | Error — recipient not found | | |

### R1-E: Relation Edge Cases (TC31-TC38)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 31 | Create same relation twice (A depends_on B, then again) | Error (duplicate) or idempotent | | |
| 32 | Create A blocks B AND B depends_on A (semantic duplicate) | Both created? Or detected as conflict? | | |
| 33 | Relate entity to itself (A depends_on A) | Rejected or creates self-loop | | |
| 34 | Create relation with invalid type string | Exit 4, clear error | | |
| 35 | Delete a relation by ID, then try to delete again | Error or idempotent | | |
| 36 | Delete endpoint entity — check relation is cascade deleted | Relation removed from DB | | |
| 37 | `inspect` entity with 20+ relations | All relations displayed, no truncation bugs | | |
| 38 | Create relation between two non-task entities (module contains service) | Created (relations aren't task-specific) | | |

### R1-F: Reservation Edge Cases (TC39-TC44)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 39 | Reserve `*.rs` (exclusive), then reserve `src/main.rs` (exclusive) by different agent | Conflict detected (glob overlap) | | |
| 40 | Reserve `*.rs` (shared), then reserve `*.rs` (exclusive) by different agent | Conflict (exclusive vs any) | | |
| 41 | Reserve with empty glob pattern | Exit 4, validation error | | |
| 42 | Reserve with very long glob (1000 chars) | Created or rejected — no panic | | |
| 43 | Release reservation that doesn't exist | Idempotent or clear error | | |
| 44 | Reserve, let TTL expire (if TTL is short enough to test), check auto-cleanup | Reservation gone after TTL | | |

---

## Round 2: Multi-Agent via Daemon

**Setup**: `filament init` + `filament serve` in temp dir. All CLI commands route through daemon socket.

### R2-A: Repeat Round 1 Through Daemon (TC45-TC48)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 45 | Run TC01-TC05 (cycle attacks) through daemon | Same results as Round 1 | | |
| 46 | Run TC13-TC22 (unicode/boundary) through daemon | Same results as Round 1 | | |
| 47 | Run TC23-TC30 (state machine) through daemon | Same results as Round 1 | | |
| 48 | Run TC31-TC38 (relation edge cases) through daemon | Same results as Round 1 | | |

### R2-B: Daemon Lifecycle Violence (TC49-TC56)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 49 | `filament serve`, then `filament serve` again (double start) | Error "already running" or idempotent | | |
| 50 | `kill -9 <daemon_pid>`, then `filament serve` | Detects stale PID, cleans up, starts fresh | | |
| 51 | Delete `.filament/daemon.sock` while daemon runs, then CLI command | Client gets connection error, daemon detects socket loss | | |
| 52 | Write fake PID (99999) to `.filament/daemon.pid`, then `filament serve` | Detects stale PID (process not running), starts normally | | |
| 53 | `filament stop` when no daemon running | Error or idempotent | | |
| 54 | `filament stop`, then immediate `filament serve` | Clean restart, no leftover state | | |
| 55 | `kill -9 <daemon_pid>` during an active `filament add` | Entity either fully created or not — no partial state | | |
| 56 | Start daemon, create entities, `filament stop`, verify data persists on restart | SQLite retains all data across daemon restarts | | |

### R2-C: Concurrency & Race Conditions (TC57-TC66)

**These require scripted parallel execution.**

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 57 | 10 parallel `filament add` commands (bash `&` + `wait`) | All 10 created, no duplicates, no crashes | | |
| 58 | 5 parallel `filament list` during concurrent writes | Each list returns consistent snapshot (no partial reads) | | |
| 59 | 2 agents call `task ready` simultaneously, both try `task assign` on same task | Exactly one succeeds, other gets conflict error | | |
| 60 | 2 agents request exclusive reservation on `*.rs` simultaneously | Exactly one succeeds, other gets conflict | | |
| 61 | `filament add` + `filament delete <same slug>` simultaneously | One completes, other gets not-found or succeeds — no corruption | | |
| 62 | 50 parallel `filament add` in tight loop | All 50 created, `list` returns 50 | | |
| 63 | Parallel: agent A sends message to B while B sends message to A | Both messages delivered, no deadlock | | |
| 64 | Rapid `filament relate` + `filament task ready` interleaved | `ready` always returns consistent results | | |
| 65 | 3 agents: each creates 10 tasks with cross-dependencies, then all call `critical-path` | All critical-path calls terminate with correct results | | |
| 66 | `filament context --around <entity>` while another process deletes that entity | Either returns context or clean "not found" — no panic | | |

### R2-D: Agent Dispatch Attacks (TC67-TC74)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 67 | Dispatch agent, immediately `kill -9` the agent child process | ChildGuard fires, task reverted, reservations released | | |
| 68 | Dispatch agent with role that doesn't exist | Clear error, no dispatch | | |
| 69 | Dispatch agent on already-in_progress task | Error: task already assigned/in-progress | | |
| 70 | Dispatch 3 agents simultaneously, all targeting same task | Exactly one dispatched, others rejected | | |
| 71 | Agent subprocess emits malformed JSON (not valid AgentResult) | Parse error handled, task status reverted | | |
| 72 | Agent subprocess emits empty stdout | Handled gracefully, no panic | | |
| 73 | Restart daemon while agent subprocess is running | Orphaned child behavior — does ChildGuard still clean up? | | |
| 74 | `dispatch-all` with 10 ready tasks | All dispatched sequentially, no races between individual RPCs | | |

### R2-E: Graph/DB Desync Attacks (TC75-TC80)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 75 | Create entity via daemon, verify `context --around` sees it immediately | In-memory graph refreshed after create | | |
| 76 | Delete entity via daemon, verify `context --around` no longer includes it | In-memory graph refreshed after delete | | |
| 77 | Create relation via daemon, immediately query `task ready` | Ready calculation uses updated graph | | |
| 78 | Close task via daemon, immediately `task critical-path` on dependent | Critical path reflects closed task | | |
| 79 | Rapid create-then-query (10x in sequence) — each query must see previous creates | No stale reads from cached graph | | |
| 80 | Create entity directly via `filament add` (CLI bypass when daemon running) — does daemon's graph see it? | CLI routes through daemon, so graph stays in sync. But what if daemon is down and CLI falls back to direct DB? | | |

---

## Round 3: TUI Under Fire

**Setup**: `filament serve` running, `filament tui` in one terminal, CLI in another.

### R3-A: TUI Rendering Stress (TC81-TC88)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 81 | Start TUI with empty database | Shows empty tables, no crash | | |
| 82 | Add 50 entities via CLI while TUI is running, then refresh TUI | TUI shows all 50 after refresh | | |
| 83 | Entity with 200-char name — does table truncate cleanly? | Truncated with ellipsis, no layout break | | |
| 84 | Rapidly switch tabs (Tasks→Agents→Reservations→Messages) in <1s | No flicker, no crash, correct data per tab | | |
| 85 | Resize terminal to 20x10 (tiny) — does TUI survive? | Renders something reasonable or shows "too small" | | |
| 86 | Resize terminal to 300x80 (huge) — does layout scale? | Tables fill space, no rendering artifacts | | |
| 87 | Start TUI, then stop daemon (`filament stop`) — TUI behavior | Error message shown, no crash, graceful degradation | | |
| 88 | Start TUI without daemon running | Connects directly to DB or shows connection error | | |

### R3-B: TUI + Concurrent Mutations (TC89-TC94)

| TC | Test | Expected | Actual | Result |
|----|------|----------|--------|--------|
| 89 | TUI open on Tasks tab. CLI: close a visible task. TUI refresh. | Task status updates in TUI | | |
| 90 | TUI open on Agents tab. CLI: dispatch agent. TUI refresh. | Agent run appears in TUI | | |
| 91 | TUI open on Reservations tab. CLI: reserve file. TUI refresh. | New reservation visible | | |
| 92 | TUI open on Messages tab. CLI: send message. TUI refresh. | New message appears | | |
| 93 | Delete entity currently selected/highlighted in TUI | TUI handles gracefully (deselect, show "deleted") | | |
| 94 | Rapid CLI mutations (20 adds in 2 seconds) while TUI auto-refreshes | TUI doesn't crash, eventually consistent | | |

---

## Round 4: Scripted Stress Tests

Save these as executable scripts in `.qa/scripts/`. Run against fresh `filament init` environments.

### R4-A: Parallel Write Storm

```bash
#!/bin/bash
# R4-TC95: Parallel write storm — 50 concurrent entity creates
set -e
DIR=$(mktemp -d)
cd "$DIR"
filament init
filament serve
sleep 1

for i in $(seq 1 50); do
  filament add "stress-$i" --type task --summary "Stress entity $i" &
done
wait

COUNT=$(filament list --type task --json | grep -c '"type":"task"')
echo "Created: $COUNT (expected: 50)"
[ "$COUNT" -eq 50 ] && echo "PASS" || echo "FAIL"

filament stop
rm -rf "$DIR"
```

### R4-B: Ready-Task Race

```bash
#!/bin/bash
# R4-TC96: Two agents race to assign the same task
set -e
DIR=$(mktemp -d)
cd "$DIR"
filament init
filament serve
sleep 1

filament add "race-task" --type task --summary "Race target"
SLUG=$(filament list --type task --json | jq -r '.[0].slug')

filament add "agent-a" --type agent --summary "Agent A"
filament add "agent-b" --type agent --summary "Agent B"
SLUG_A=$(filament list --type agent --json | jq -r '.[] | select(.name=="agent-a") | .slug')
SLUG_B=$(filament list --type agent --json | jq -r '.[] | select(.name=="agent-b") | .slug')

filament task assign "$SLUG" "$SLUG_A" &
filament task assign "$SLUG" "$SLUG_B" &
wait

filament inspect "$SLUG"
echo "Check: exactly one agent assigned, or last-write-wins"

filament stop
rm -rf "$DIR"
```

### R4-C: Daemon Kill & Recovery

```bash
#!/bin/bash
# R4-TC97: Kill daemon mid-flight, verify recovery
set -e
DIR=$(mktemp -d)
cd "$DIR"
filament init
filament serve
sleep 1

filament add "pre-kill" --type task --summary "Before kill"
DPID=$(cat .filament/daemon.pid)
kill -9 "$DPID"
sleep 1

filament serve  # should recover from stale PID
sleep 1

filament inspect pre-kill  # data should persist
echo "PASS if entity found"

filament stop
rm -rf "$DIR"
```

### R4-D: Message Flood

```bash
#!/bin/bash
# R4-TC98: Flood agent inbox with 200 messages
set -e
DIR=$(mktemp -d)
cd "$DIR"
filament init
filament serve
sleep 1

filament add "flood-agent" --type agent --summary "Victim"
SLUG=$(filament list --type agent --json | jq -r '.[0].slug')

for i in $(seq 1 200); do
  filament message send "$SLUG" "Message $i" --from "system" &
done
wait

COUNT=$(filament message inbox "$SLUG" --json | jq length)
echo "Messages: $COUNT (expected: 200)"
[ "$COUNT" -eq 200 ] && echo "PASS" || echo "FAIL"

filament stop
rm -rf "$DIR"
```

### R4-E: Dependency Chain Stress

```bash
#!/bin/bash
# R4-TC99: 50-deep dependency chain, critical path, cascading close
set -e
DIR=$(mktemp -d)
cd "$DIR"
filament init
filament serve
sleep 1

PREV=""
for i in $(seq 1 50); do
  if [ -z "$PREV" ]; then
    filament task add "chain-$i" --summary "Chain link $i"
  else
    filament task add "chain-$i" --summary "Chain link $i" --depends-on "$PREV"
  fi
  PREV="chain-$i"
done

# Critical path from first task
FIRST_SLUG=$(filament list --type task --json | jq -r '.[] | select(.name=="chain-1") | .slug')
echo "Critical path (should be 50 nodes):"
filament task critical-path "$FIRST_SLUG" | wc -l

# Ready should be only chain-50
echo "Ready tasks (should be 1):"
filament task ready | wc -l

filament stop
rm -rf "$DIR"
```

---

## Execution Notes

- **Build release binary first**: `make build CRATE=all RELEASE=1`
- **Each round uses a fresh temp dir** — no state leakage between rounds
- **Record actual output** in the Result column — don't just write PASS/FAIL
- **Any panic = automatic BUG** — even if the output is correct, panics are bugs
- **Any hang > 10s = automatic BUG** — timeouts indicate infinite loops or deadlocks
- **Capture exit codes** — wrong exit code is a bug even if output looks right
- **TUI tests are visual** — note rendering artifacts, truncation, alignment issues

## Bug Tracking

| BUG ID | TC | Description | Severity | Fixed? |
|--------|-----|------------|----------|--------|
| | | | | |
