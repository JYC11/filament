# Simulation v3: Advanced Features (Parallel Agents, TTL, Death, Auto-Dispatch, MCP, TUI)

## Context

Simulation v2 exercised the core filament workflow via sequential CLI commands through the daemon.
Six features were deferred because they require real parallel processes or time-based events:

1. **True parallel agents** — separate subprocesses working concurrently via daemon
2. **TTL expiry** — reservation auto-cleanup when time expires
3. **Agent death cleanup** — system reclaims resources from crashed agents
4. **Auto-dispatch** — `FILAMENT_AUTO_DISPATCH=1` chains agent runs automatically
5. **MCP server interaction** — external agent tooling via Model Context Protocol
6. **TUI live observation** — monitoring all the above in the terminal dashboard

## Approach: Mock Agent Scripts + Real Daemon

The key enabler is `FILAMENT_AGENT_COMMAND` — an env var that overrides what subprocess
`filament agent dispatch` spawns (default: `claude`). We write bash scripts that emit
valid `AgentResult` JSON and control their behavior (delay, status, exit code).

**This is exactly how the existing test suite works** (`crates/filament-daemon/tests/dispatch.rs`).

## Prerequisites

- Release binary already built (`make build CRATE=all RELEASE=1`)
- Working directory: `/tmp/filament-sim-v3/`

## Plan

### Phase 1: Setup (mock agent scripts + project init)

**1.1** Create mock agent scripts in `/tmp/filament-sim-v3/scripts/`:

| Script | Behavior | Purpose |
|--------|----------|---------|
| `agent-fast.sh` | 2s delay → emit completed AgentResult → exit 0 | Normal quick completion |
| `agent-slow.sh` | 10s delay → emit completed AgentResult → exit 0 | Long-running agent (for TUI observation) |
| `agent-blocker.sh` | 3s delay → emit blocked AgentResult with blocker message → exit 0 | Agent that raises escalation |
| `agent-crash.sh` | 2s delay → exit 1 (no JSON output) | Simulates agent death |
| `agent-hang.sh` | `sleep 300` (5 min) → never completes | For manual kill (death simulation) |

Each script ignores the `-p` and `--mcp-config` args (which `dispatch` passes) and just emits JSON.

**AgentResult JSON format:**
```json
{"status":"completed","summary":"task done","artifacts":[],"messages":[],"blockers":[],"questions":[]}
```

**1.2** Init project:
```bash
cd /tmp/filament-sim-v3 && filament init
```

**1.3** Seed a small dependency graph (5 tasks, 3 agents):
```
task-A (ready) → task-B → task-C → task-D → task-E
                       ↗ (task-B also blocks task-C)
```
- task-A: "Design API schema" (P0, no deps)
- task-B: "Implement user service" (P1, blocked by A)
- task-C: "Implement order service" (P1, blocked by A)
- task-D: "Integration tests" (P2, blocked by B + C)
- task-E: "Deploy" (P0, blocked by D)

3 agents: alpha, beta, gamma.

### Phase 2: True Parallel Agents (dispatch + concurrent processes)

**2.1** Start daemon with mock agent command:
```bash
FILAMENT_AGENT_COMMAND=/tmp/filament-sim-v3/scripts/agent-fast.sh filament serve
```

**2.2** Dispatch task-A to alpha:
```bash
filament agent dispatch <task-A-slug>
```
The daemon spawns `agent-fast.sh` as a subprocess. The monitor watches it async.

**2.3** Wait 3s, verify completion:
```bash
filament agent list       # should show completed run
filament task ready       # task-B and task-C should now be unblocked
```

**2.4** Dispatch task-B and task-C in parallel (two quick commands):
```bash
filament agent dispatch <task-B-slug>
filament agent dispatch <task-C-slug>
```
Both agents run concurrently as separate PIDs. The daemon handles both.

**2.5** Verify both complete:
```bash
filament agent list       # 3 completed runs
filament task ready       # task-D should be unblocked
```

### Phase 3: TTL Expiry

**3.1** Create a reservation with short TTL (5 seconds):
```bash
filament reserve "src/shared/**" --agent alpha --exclusive --ttl 5
filament reservations     # shows reservation with ~5s remaining
```

**3.2** Wait ~65 seconds (daemon cleanup interval is 60s):
```bash
sleep 65
filament reservations     # reservation should be gone (expired + cleaned)
```

**3.3** Alternative: manual cleanup trigger:
```bash
filament reserve "src/other/**" --agent beta --exclusive --ttl 5
sleep 6
filament reservations --clean    # force cleanup, show cleaned count
filament reservations            # confirm empty
```

### Phase 4: Agent Death Cleanup

**4.1** Stop daemon, restart with crash script:
```bash
filament stop
FILAMENT_AGENT_COMMAND=/tmp/filament-sim-v3/scripts/agent-crash.sh filament serve
```

**4.2** Create a reservation for the agent, then dispatch:
```bash
filament reserve "src/crash-test/**" --agent gamma --exclusive --ttl 3600
filament agent dispatch <task-D-slug>
```

**4.3** Wait for crash (2s + processing):
```bash
sleep 5
filament agent list       # should show FAILED run
filament inspect <task-D> # task should revert to Open (not stuck in InProgress)
filament reservations     # agent's reservations should be released
```

This demonstrates ADR-009: agent dies → task reverts → reservations released → can re-dispatch.

**4.4** Manual kill simulation (hang script):
```bash
filament stop
FILAMENT_AGENT_COMMAND=/tmp/filament-sim-v3/scripts/agent-hang.sh filament serve
filament agent dispatch <task-D-slug>
sleep 2
filament agent list       # shows running agent with PID
# Kill the agent process directly
kill <PID from agent list>
sleep 3
filament agent list       # should show failed (monitor detects exit)
filament inspect <task-D> # reverted to Open
```

### Phase 5: Auto-Dispatch

**5.1** Stop daemon, restart with auto-dispatch + fast agent:
```bash
filament stop
FILAMENT_AGENT_COMMAND=/tmp/filament-sim-v3/scripts/agent-fast.sh \
  FILAMENT_AUTO_DISPATCH=1 \
  filament serve
```

**5.2** Reset task-D to Open (if needed from Phase 4):
```bash
filament update <task-D-slug> --status open
```

**5.3** Dispatch task-D only:
```bash
filament agent dispatch <task-D-slug>
```

**5.4** Wait and observe chain reaction:
```bash
sleep 10
filament agent list       # should show task-D completed, AND task-E auto-dispatched + completed
filament task list --status closed  # task-D and task-E both closed automatically
```

The system detected task-E was unblocked when task-D completed, and auto-dispatched an agent.

### Phase 6: MCP Server Interaction (Full)

**6.1** Write an MCP client helper script (`/tmp/filament-sim-v3/scripts/mcp-client.sh`):
```bash
#!/usr/bin/env bash
# Sends JSON-RPC messages to filament mcp via named pipe
# Usage: mcp-client.sh <request.jsonl>
#
# The script starts `filament mcp`, sends each line from the input file
# as a JSON-RPC message, reads responses, and prints them.

FIFO_IN=$(mktemp -u /tmp/mcp-in.XXXXXX)
mkfifo "$FIFO_IN"

# Start MCP server with named pipe as stdin
filament mcp < "$FIFO_IN" &
MCP_PID=$!

# Open the pipe for writing (keeps it open)
exec 3>"$FIFO_IN"

# Send each line from input, wait for response
while IFS= read -r line; do
    echo "$line" >&3
    sleep 0.5  # give server time to respond
done < "$1"

# Cleanup
exec 3>&-
wait $MCP_PID 2>/dev/null
rm -f "$FIFO_IN"
```

**6.2** Write MCP request sequences (`/tmp/filament-sim-v3/scripts/mcp-requests.jsonl`):
```jsonl
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"sim-v3","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"notifications/initialized"}
{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"filament_task_ready","arguments":{}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"filament_list","arguments":{"entity_type":"agent"}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"filament_message_send","arguments":{"from_agent":"alpha","to_agent":"beta","body":"Hello from MCP!","msg_type":"text"}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"filament_message_inbox","arguments":{"agent":"beta"}}}
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"filament_context","arguments":{"slug":"<task-E-slug>","depth":3}}}
```

**6.3** Run the MCP session:
```bash
cd /tmp/filament-sim-v3
bash scripts/mcp-client.sh scripts/mcp-requests.jsonl 2>/dev/null
```

**6.4** Verify from MCP output:
- `tools/list` returns 16 tools with JSON Schema parameter descriptions
- `filament_task_ready` returns the correct unblocked tasks
- `filament_message_send` creates a message (verify with `filament message inbox beta`)
- `filament_context` returns the graph neighborhood
- All responses have proper `jsonrpc: "2.0"` structure

### Phase 7: TUI Live Observation

**7.1** While daemon is running with auto-dispatch, open TUI:
```bash
filament tui
```

**7.2** Describe what each tab shows:
- **Tasks tab**: task statuses, blocked counts, impact scores
- **Agents tab**: running/completed agent PIDs, durations
- **Reservations tab**: active reservations with time-left countdown

**7.3** Since TUI is interactive (takes over terminal), we demonstrate it by:
1. Starting TUI in the foreground
2. Describing what we see (we're the narrator)
3. Quitting with `q`

Note: We can't programmatically screenshot TUI, but we can run it and describe the state.
The 5-second auto-refresh will show changes as agents complete work.

### Phase 8: Cleanup + Log

**8.1** Stop daemon:
```bash
filament stop
```

**8.2** Export final state:
```bash
filament export --output /tmp/filament-sim-v3/snapshot.json
```

**8.3** Write structured log to `.qa/simulation-v3-log-<date>.md`

**8.4** Clean up temp directory

## Key Files

| File | Purpose |
|------|---------|
| `crates/filament-daemon/src/dispatch.rs` | Agent dispatch + monitor + auto-dispatch logic |
| `crates/filament-daemon/src/state.rs` | DispatchConfig (agent_command, auto_dispatch) |
| `crates/filament-daemon/src/lib.rs` | Daemon startup, cleanup task spawning |
| `crates/filament-daemon/src/mcp.rs` | MCP server (16 tools) |
| `crates/filament-tui/src/app.rs` | TUI app state + refresh |
| `crates/filament-daemon/tests/dispatch.rs` | Existing mock agent pattern to follow |

## Verification

Each phase verifies itself via CLI commands:
- `filament agent list` — confirms dispatch, completion, failure states
- `filament task ready` / `task list --status closed` — confirms dependency chain advancement
- `filament reservations` — confirms TTL expiry and death cleanup
- `filament escalations` — confirms blocker routing
- MCP JSON-RPC responses — confirms tool exposure
- TUI visual inspection — confirms live monitoring works

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Mock scripts need to accept `-p` and `--mcp-config` args | Scripts use `$@` or ignore extra args |
| 60s cleanup interval is long to wait | Use `reservations --clean` for manual trigger |
| MCP stdio is hard to interact with ad-hoc | Use pipe/heredoc approach, or skip if too complex |
| TUI takes over terminal | Run briefly, describe state, quit with `q` |
| Agent PID from `agent list` may not be parseable | Use `--json` output for reliable parsing |
