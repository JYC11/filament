# Simulation v3 Log: Advanced Features

**Date:** 2026-03-04
**Binary:** `target/release/filament` (all 4 crates)
**Working dir:** `/tmp/filament-sim-v3/`
**Mode:** Daemon with mock agent scripts + direct CLI

## Setup

- 5 tasks: design-api-schema → implement-user-service + implement-order-service → integration-tests → deploy
- 3 agents: alpha, beta, gamma
- 4 mock agent scripts: fast (2s), slow (10s), crash (exit 1), hang (sleep 300)
- Diamond dependency: task-D blocked by BOTH task-B and task-C

## Results by Phase

### Phase 2: True Parallel Agents

| Step | Action | Result |
|------|--------|--------|
| 2.1 | Start daemon with `FILAMENT_AGENT_COMMAND=agent-fast.sh` | daemon PID 24135 |
| 2.2 | Dispatch task-A | Completed in 2s, task auto-closed |
| 2.3 | Verify ready | task-B and task-C both unblocked |
| 2.4 | Dispatch task-B AND task-C simultaneously | Two PIDs (24153, 24156) running concurrently |
| 2.5 | Verify parallel completion | Both completed in ~2s, task-D unblocked |

**Verdict: PASS** — true parallel subprocess execution with correct diamond-dependency unblocking.

### Phase 3: TTL Expiry

| Step | Action | Result |
|------|--------|--------|
| 3.1 | Reserve `src/shared/**` with TTL=5s | Reservation created, shows expiry time |
| 3.2 | Wait 6s, run `reservations --clean` | "Cleaned up 1 expired reservation(s)" |
| 3.3 | Verify empty | No active reservations |

**Verdict: PASS** — TTL-based reservation expiry works correctly.

### Phase 4: Agent Death Cleanup

| Step | Action | Result |
|------|--------|--------|
| 4.1 | Restart daemon with `agent-crash.sh` | daemon PID 24244 |
| 4.2 | Dispatch task-D (agent will crash) | Agent exits 1 after 2s |
| 4.3 | Verify cleanup | Agent run: `failed` (2s). Task reverted to `open`. |
| 4.4 | Manual kill test (hang agent) | Sandbox blocked `kill` command — could not test |
| 4.5 | Daemon stop kills hanging agent | Agent PID 24363 killed, but run stayed `running` in DB |

**Verdict: PARTIAL PASS** — crash cleanup works (task reverts, run marked failed). Manual kill couldn't be tested due to sandbox. Daemon shutdown doesn't finalize in-flight agent runs (expected — daemon is itself shutting down).

**Finding:** Orphan agent processes survive daemon SIGTERM. The stale `running` record in DB blocks future dispatch. Needed manual SQLite fixup. Consider: startup reconciliation that marks stale `running` records as `failed`.

### Phase 5: Auto-Dispatch (Chain Reaction)

| Step | Action | Result |
|------|--------|--------|
| 5.1 | Start daemon with `FILAMENT_AUTO_DISPATCH=1` + `agent-fast.sh` | daemon PID 24519 |
| 5.2 | Reset task-D to open | Done (was still open from Phase 4 cleanup) |
| 5.3 | Dispatch ONLY task-D | Dispatched, completed in 2s |
| 5.4 | Verify chain reaction | task-D completed → task-E auto-dispatched → task-E completed (2s) |
| 5.5 | Final check | ALL 5 tasks closed. `task ready` returns empty. |

**Verdict: PASS** — auto-dispatch correctly detected newly-unblocked task-E and chained execution. Single dispatch cascaded through the rest of the dependency graph.

### Phase 6: MCP Server Interaction

| Step | Action | Result |
|------|--------|--------|
| 6.1 | Send `initialize` | `rmcp 0.17.0`, protocol `2024-11-05` |
| 6.2 | Send `tools/list` | 16 tools returned with full JSON Schema |
| 6.3 | `filament_list` (agents) | 3 agents returned with all metadata fields |
| 6.4 | `filament_context` (deploy, depth=2) | BFS neighborhood: integration-tests, implement-order-service, implement-user-service |
| 6.5 | `filament_message_send` (slug) | Message created, ID returned |
| 6.6 | Verify via CLI `message inbox` | Message visible in beta's inbox |
| 6.7 | `filament_message_send` (name) | Structured error: `ENTITY_NOT_FOUND` with hint |

**Verdict: PASS** — MCP server fully functional. 16 tools exposed. Tool calls work (with slug-based resolution). Structured errors for invalid inputs. CLI-MCP round-trip confirmed.

### Phase 7: Export

| Metric | Count |
|--------|-------|
| Entities | 8 (5 tasks + 3 agents) |
| Relations | 5 (blocking chain) |
| Messages | 1 (MCP-sent) |
| Events | 43 |
| Snapshot size | 18.6 KB |

**Verdict: PASS** — full data export with all entities, relations, messages, and events.

## Summary

| Feature | Status | Notes |
|---------|--------|-------|
| True parallel agents | PASS | 2 concurrent PIDs, diamond-dep unblocking |
| TTL expiry | PASS | 5s TTL + manual cleanup |
| Agent crash cleanup | PASS | Task reverts to open, run marked failed |
| Agent manual kill | SKIPPED | Sandbox blocked `kill` |
| Orphan process handling | FINDING | Stale `running` records need startup reconciliation |
| Auto-dispatch chain | PASS | Single dispatch cascaded D→E automatically |
| MCP server (16 tools) | PASS | Full JSON-RPC, tool calls, structured errors |
| MCP→CLI round-trip | PASS | Message sent via MCP, read via CLI |
| Export | PASS | 18.6 KB snapshot, all data preserved |

## Findings / Future Work

1. **Startup reconciliation**: When daemon starts, it should mark any `running` agent records as `failed` (stale from previous daemon). This prevents "AGENT_ALREADY_RUNNING" errors after unclean shutdowns.
2. **Orphan reaping**: `filament stop` should wait for child processes to exit (or forcefully kill them) before the daemon exits. Currently, child processes can survive as orphans.
3. **MCP agent resolution**: MCP `message_send` requires slug-based agent references, not human names. This is by design (slugs are canonical), but an MCP agent using `filament_list` first to discover slugs is the correct workflow.
4. **TUI phase skipped**: TUI requires interactive terminal — cannot be tested programmatically. Verified manually in previous sessions.
