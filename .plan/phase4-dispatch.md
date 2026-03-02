# Phase 4: Agent Dispatching

**Goal**: orchestrate `claude -p` subprocesses from filament, with resilience for agent death.

**Master plan**: [filament-v1.md](filament-v1.md)
**Depends on**: [Phase 3](phase3-daemon.md) (agents need MCP to call filament tools)

---

## 4.1 — Agent dispatch engine

- File: `filament-daemon/src/dispatch.rs`
- `dispatch_agent(task_id, role) -> AgentRun`
- Spawns `claude -p` with:
  - `--systemPrompt <role-specific prompt>`
  - `--output-format json`
  - `--allowedTools <role-restricted tools + filament MCP tools>`
  - MCP server URL injected so agent can call filament tools directly
- Captures stdout → parses AgentResult JSON
- Records run in `agent_runs` table
- **Agent death resilience**:
  - Monitor subprocess PID — detect unexpected exit
  - On death: release file reservations held by agent, update run status to Failed, log event
  - No single agent is a ringleader — loss of any agent doesn't block the system
- Blocked by: 1.2, 1.5, 3.3

## 4.2 — Agent roles + prompts

- Stored as entities in the graph (`entity_type = "agent_role"`) or as config files in `.filament/prompts/`
- Built-in roles: coder, reviewer, dockeeper, planner
- Each role defines: system prompt, allowed tools, output schema expectations
- Agent prompt includes:
  - MCP server URL for filament tool access
  - Instruction to use `filament_reserve` before modifying files
  - Instruction to use `filament_message_send` (targeted, not broadcast) for inter-agent comms
  - Instruction to use `filament_task_close` when done
- Blocked by: 4.1

## 4.3 — Pre-dispatch safety

- File: `filament-daemon/src/safety.rs`
- Before dispatching, verify:
  - Task exists and is unblocked
  - No file reservation conflicts with the task's expected file scope
  - Agent role is valid
- Optional integration with `destructive_command_guard` (dcg) if installed:
  - Detect dcg binary in PATH
  - Configure agent's `--allowedTools` to route through dcg
- Blocked by: 4.1

## 4.4 — Result collection + routing

- Parse AgentResult from subprocess stdout
- Route messages: write to `messages` table (targeted only), push via socket if daemon is running
- Update task status based on agent result
- Register artifacts: create entities + relations from `artifacts` field
- Release file reservations held by the completing agent
- Handle escalation: `needs_input` → notify user, `blocked` → log blocker
- Blocked by: 4.1, 4.3

## 4.5 — Batch dispatch

- `filament agent dispatch-all [--max-parallel N] [--role coder]`
- Queries `ready_tasks()` from graph (ranked by priority + impact_score)
- Launches up to N agents in parallel (tokio::JoinSet)
- As agents complete, checks for newly unblocked tasks, launches more
- On agent death: clean up reservations, optionally retry task
- Blocked by: 4.3, 4.4

## 4.6 — Tests for Phase 4

- Mock `claude -p` with a shell script that outputs valid AgentResult JSON
- Test dispatch → result parsing → message routing → task status update
- Test batch dispatch with dependency chain: A blocks B, complete A, verify B launches
- Agent death test: kill subprocess mid-run, verify reservations released + status updated
- Reservation conflict test: two agents dispatched to overlapping files, verify conflict detection
- Blocked by: 4.4, 4.5

---

## Task Dependency Graph

```
4.1 (dispatch engine)
 ├──→ 4.2 (roles + prompts)
 ├──→ 4.3 (safety) ──→ 4.4 (result routing) ──→ 4.5 (batch)
 └──→ 4.4 (result routing)

4.4, 4.5 ──→ 4.6 (tests)
```
