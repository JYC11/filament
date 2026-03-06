# v1.0.0 Release QA Plan

**Goal**: Comprehensive manual QA to validate filament is release-ready.

## QA Sessions

### Session 1: Stress Test — Volume & Scale

Test the system under heavy data load. Verify performance doesn't degrade and no silent failures.

**Setup**: Fresh `fl init` in `/tmp/fl-stress-qa`

| ID | Test | Description | Pass criteria |
|----|------|-------------|---------------|
| ST-01 | Bulk entity creation | Create 500 entities (mix of types) in rapid succession | All created, `fl list` shows 500, no errors |
| ST-02 | Bulk relation creation | Create 1000 relations across the 500 entities | All created, `fl context --depth 3` works |
| ST-03 | Bulk message volume | Send 500 messages across 10 agents | `fl message inbox` returns correct counts |
| ST-04 | Large key_facts | Create entity with 50KB JSON key_facts | Stored and retrieved correctly |
| ST-05 | Large content file | Create entity with `--content` pointing to 1MB file | `fl read` returns full content |
| ST-06 | Search at scale | FTS5 search across 500 entities | Returns results in <1s, ranking correct |
| ST-07 | Export at scale | Export 500 entities + 1000 relations | Valid JSON, correct counts |
| ST-08 | Import at scale | Import the exported snapshot into fresh project | All counts match |
| ST-09 | Task ready at scale | 200 tasks with complex dependency graph | `fl task ready` returns correct unblocked set |
| ST-10 | Graph traversal at scale | `fl context --around X --depth 5` on dense graph | Completes without timeout, correct results |
| ST-11 | PageRank at scale | `fl pagerank` on 500-entity, 1000-relation graph | Completes, ranks plausible |
| ST-12 | Concurrent CLI access | 10 parallel `fl list` commands | All succeed, no DB lock errors |
| ST-13 | Rapid create-delete cycle | Create and immediately delete 100 entities | No orphan relations, clean state |
| ST-14 | Long-running lesson list | 200 lessons with large problem/solution fields | `fl lesson list` completes, `fl search` works |
| ST-15 | Export/import round-trip integrity | Export, import to new project, export again, diff | JSON snapshots are semantically identical |

### Session 2: Core CRUD — Happy Path + Edge Cases

Cover every entity type through full lifecycle.

| ID | Test | Description |
|----|------|-------------|
| CR-01 | Create each of 7 entity types | All types: task, module, service, agent, plan, doc, lesson |
| CR-02 | Update each field individually | summary, status, priority, key_facts, content_path |
| CR-03 | Remove entity cascades relations | Create A-blocks-B, remove A, verify B unblocked |
| CR-04 | Unicode entity names | CJK, emoji, RTL, combining chars |
| CR-05 | Empty string handling | Empty summary, empty key_facts |
| CR-06 | Priority boundaries | 0, 4, and out-of-range (5, -1) |
| CR-07 | Status transitions | open -> in_progress -> blocked -> in_progress -> closed |
| CR-08 | Duplicate names | Two entities with same name (should succeed, different slugs) |
| CR-09 | JSON output for all CRUD | `--json` on add, list, inspect, update, remove |

### Session 3: Task Management

| ID | Test | Description |
|----|------|-------------|
| TM-01 | task add with all flags | `--priority`, `--blocks`, `--depends-on`, `--summary` |
| TM-02 | task ready ranking | Multiple unblocked tasks, verify priority ordering |
| TM-03 | task close unblocks dependents | Close blocker, verify dependent appears in `task ready` |
| TM-04 | task blocker-depth | Deep chain (5+ levels), verify correct depth |
| TM-05 | task assign / unassign | Assign to agent, verify in inspect output |
| TM-06 | Circular dependency prevention | A blocks B, B blocks A — should error |

### Session 4: Lessons & Search

| ID | Test | Description |
|----|------|-------------|
| LS-01 | lesson add with all fields | --problem, --solution, --learned, --pattern |
| LS-02 | lesson list by pattern | Filter by pattern name |
| LS-03 | lesson show structured output | Verify problem/solution/pattern/learned display |
| LS-04 | Search basic terms | Single word, phrase, OR, NOT |
| LS-05 | Search by type filter | `--type lesson`, `--type task` |
| LS-06 | Search with no results | Verify graceful empty result |
| LS-07 | Search ranking | BM25 ranking: exact match > partial match |

### Session 5: Relations & Graph

| ID | Test | Description |
|----|------|-------------|
| RG-01 | All relation types | blocks, depends_on, produces, owns, relates_to, assigned_to |
| RG-02 | Unrelate | Remove relation, verify gone from context |
| RG-03 | Context BFS | `--depth 1`, `--depth 2`, `--depth 3` |
| RG-04 | PageRank | Verify high-connectivity nodes rank highest |
| RG-05 | Degree centrality | Verify correct in/out degree counts |
| RG-06 | Relate nonexistent entity | Error with exit code 3 |

### Session 6: Messaging & Escalations

| ID | Test | Description |
|----|------|-------------|
| ME-01 | Send all message types | text, question, blocker, artifact |
| ME-02 | Inbox filtering | By agent, read/unread |
| ME-03 | Escalation creation | Blocker/question to "user" appears in `fl escalations` |
| ME-04 | Escalation resolution | Reply clears escalation |

### Session 7: Infrastructure

| ID | Test | Description |
|----|------|-------------|
| IN-01 | Export/import round-trip | Full snapshot integrity |
| IN-02 | Config file (`fl.toml`) | Create config, verify settings applied |
| IN-03 | Seed from CLAUDE.md | `fl seed` parses sections correctly |
| IN-04 | Completions | `fl completions zsh` outputs valid script |
| IN-05 | Hook install/uninstall | Pre-commit hook installed, check works |
| IN-06 | Double init idempotency | `fl init` twice — no error, no data loss |

### Session 8: Error Handling & Adversarial

| ID | Test | Description |
|----|------|-------------|
| EH-01 | SQL injection attempts | `'; DROP TABLE entities;--` in names |
| EH-02 | Shell metacharacters | `$(rm -rf /)`, backticks, pipes in values |
| EH-03 | Null bytes & control chars | `\x00`, `\x01` in strings |
| EH-04 | Invalid JSON in --facts | Malformed JSON string |
| EH-05 | Nonexistent slug | Operations on `zzzzzzzz` |
| EH-06 | Wrong flag types | String where number expected, vice versa |
| EH-07 | Missing required flags | Each command without its required args |
| EH-08 | `--json` error format | Structured error with code, message, hint, retryable |
| EH-09 | Very long arguments | 10KB name, 100KB summary |
| EH-10 | Concurrent writes | Two processes writing simultaneously |

### Session 9: Daemon & Multi-Agent

| ID | Test | Description |
|----|------|-------------|
| DA-01 | `fl serve` + `fl stop` lifecycle | Start daemon, verify socket, stop cleanly |
| DA-02 | CLI routes through daemon | Start daemon, run commands, verify routed |
| DA-03 | Reservation conflicts via daemon | Two agents claim same file |
| DA-04 | Agent dispatch | `fl agent dispatch` spawns process |
| DA-05 | Agent timeout | Configure short timeout, verify agent killed |
| DA-06 | Dead agent cleanup | Kill agent process, verify reconciliation |

### Session 10: TUI Smoke Test

| ID | Test | Description |
|----|------|-------------|
| TU-01 | Launch and navigate tabs | All 6 tabs render |
| TU-02 | Entity list paging | Page through 50+ entities |
| TU-03 | Detail pane | Select entity, verify details shown |
| TU-04 | Filter entities | Type and status filters |

### Session 11: Idempotency & State Machine

Every mutating operation run twice. Invalid state transitions rejected.

| ID | Test | Description | Pass criteria |
|----|------|-------------|---------------|
| IS-01 | Double `fl init` | Run `fl init` in already-initialized project | No error, no data loss, DB intact |
| IS-02 | Double entity create | `fl add foo --type module` twice | Both succeed (different slugs), or clear error |
| IS-03 | Double relation create | `fl relate A blocks B` twice | Second call: error (exit 6) or silent no-op |
| IS-04 | Double task close | `fl task close <slug>` on already-closed task | Clear error message, not a panic |
| IS-05 | Double reserve same agent | `fl reserve "src/**" --agent X` twice | Refresh/extend, not error |
| IS-06 | Double release | `fl release "src/**" --agent X` twice | Second call: clear error, not panic |
| IS-07 | Double message send | Same `--from --to --body` twice | Both succeed (messages are not deduplicated) |
| IS-08 | Double lesson add | Same title twice | Both succeed (different slugs) |
| IS-09 | Status: closed -> open | `fl update <slug> --status open` on closed entity | Verify behavior (allowed or rejected?) |
| IS-10 | Status: closed -> in_progress | On closed entity | Verify behavior |
| IS-11 | Status: blocked -> closed | Without resolving first | Verify behavior |
| IS-12 | Status: open -> closed (skip in_progress) | Direct close from open | Should succeed (valid shortcut) |
| IS-13 | Status: in_progress -> open | Undo in_progress | Verify behavior |
| IS-14 | Remove then reference | Remove entity, then `fl relate` to it | Exit code 3, not found error |
| IS-15 | Assign closed task | `fl task assign <closed-slug> --to <agent>` | Verify behavior |
| IS-16 | Block open task (not in_progress) | `fl update <open-slug> --status blocked` | Verify behavior |

**Goal**: Document the actual state machine. For any transitions that succeed but probably shouldn't, file bugs.

### Session 12: Docs Accuracy Audit

Open every documentation file and execute every example command literally. Copy-paste, don't retype.

| ID | Test | Description | Pass criteria |
|----|------|-------------|---------------|
| DOC-01 | README.md examples | Execute every `fl` command in README | All produce expected output |
| DOC-02 | CLAUDE.md command table | Execute every command in the Command Reference table | All work, correct flags |
| DOC-03 | Filament skill file | Execute every command in `~/.claude/skills/filament/SKILL.md` | All work |
| DOC-04 | `fl --help` accuracy | Compare `--help` output to docs for every subcommand | Flags match |
| DOC-05 | Error code table | Trigger each exit code (0, 2, 3, 4, 5, 6, 7) | Matches documented meaning |
| DOC-06 | Stale `filament` references | Grep all docs for old `filament` command name | Zero occurrences (all should be `fl`) |
| DOC-07 | Skill scenario schema | Validate all 6 scenario JSON files against the schema in SKILL.md | All fields present, no undocumented fields |
| DOC-08 | Bug report skill | Run the bug-report-format skill workflow | Produces valid .tar.gz with expected contents |
| DOC-09 | Manual QA skill | Run manual-qa skill's "Quick Sanity Check" section | All commands succeed |
| DOC-10 | Entity type table | Create one of each type listed in docs | All types accepted, no undocumented types |

**Goal**: Every example in every doc file must work when copy-pasted. Any stale content is a bug.

### Session 13: Exploratory Testing (Unscripted)

Set a 30-minute timer. No plan. Try to break things.

**Guidelines:**
- Use the tool as if you've never read the docs
- Do things in the wrong order (relate before add, close before start)
- Interrupt operations: Ctrl+C during `fl export`, kill during `fl serve`
- Try the same command 10 times rapidly
- Mix entity types where they shouldn't go (assign a module to a task)
- Use the `--json` flag on every command — does it always produce valid JSON?
- Try `fl` with no subcommand, with unknown subcommand, with `--version`
- Run `fl task ready` with zero tasks
- Run `fl escalations` with zero messages
- Pipe binary data into commands that accept stdin
- Try commands in a directory with no `.fl/`

**Capture**: For every unexpected behavior found, note the command and result. File as bug if it's a panic, silent wrong answer, or confusing error.

### Session 14: Data Integrity & Recovery

Test what happens when things go wrong at the data layer.

| ID | Test | Description | Pass criteria |
|----|------|-------------|---------------|
| DI-01 | Corrupt DB | Truncate `.fl/fl.db` to 50% size, run `fl list` | Graceful error, not panic |
| DI-02 | Delete DB | Remove `.fl/fl.db`, run `fl list` | Clear "not initialized" error |
| DI-03 | Read-only DB | `chmod 444 .fl/fl.db`, run `fl add` | Clear permission error |
| DI-04 | Delete socket mid-daemon | Remove `.fl/fl.sock` while daemon runs | Daemon detects and recovers, or clear error |
| DI-05 | Stale PID file | Create `.fl/fl.pid` with fake PID, run `fl serve` | Daemon starts (detects stale PID) |
| DI-06 | Import over existing data | `fl import` into project with existing entities | Clear behavior (merge, replace, or error) |
| DI-07 | Import malformed JSON | `fl import --input /dev/null` | Graceful error, DB unchanged |
| DI-08 | Import from future version | Edit export JSON with extra unknown fields | Ignores unknown fields or clear error |
| DI-09 | Disk full simulation | Create export on full filesystem | Partial write doesn't corrupt DB |
| DI-10 | Concurrent daemon + direct | Run `fl serve`, then direct `fl` in another terminal | Either routes through daemon or clear error |

## Execution Order

1. **Session 8** (Error handling) — catch crashes early
2. **Session 14** (Data integrity) — verify recovery before anything else
3. **Session 1** (Stress test) — verify scale before detailed testing
4. **Session 2** (Core CRUD) — foundation
5. **Session 11** (Idempotency) — verify state machine alongside CRUD
6. **Session 3** (Tasks) — key workflow
7. **Session 4** (Lessons & Search) — knowledge capture
8. **Session 5** (Relations & Graph) — graph intelligence
9. **Session 6** (Messaging) — agent coordination
10. **Session 7** (Infrastructure) — supporting features
11. **Session 9** (Daemon) — multi-agent mode
12. **Session 10** (TUI) — visual verification
13. **Session 12** (Docs accuracy) — verify all documentation matches reality
14. **Session 13** (Exploratory) — final unscripted pass, find what scripts missed

## Pass Criteria for v1.0.0

- All Critical and High severity bugs fixed
- Zero panics or crashes on any input
- All error messages are human-readable (no raw SQL/stack traces)
- Export/import round-trip preserves all data
- Stress test: 500 entities + 1000 relations handled without degradation
- Medium bugs documented as known issues (acceptable for v1.0.0)
