# Simulation v2: Daemon Mode + Complex Scenario

**Status:** Planned (not started)
**Prereq:** Simulation v1 log at `.qa/simulation-log-2026-03-04.md`

## What v1 covered (direct CLI, no daemon)
- Linear 8-task dependency chain
- 4 agents, single-threaded simulation (narrator runs all commands sequentially)
- Escalations, reservations, messaging, export/import
- All via `filament <cmd>` → SQLite directly

## What v2 should add

### 1. Daemon mode
- Start daemon: `filament daemon start`
- CLI auto-routes through Unix socket when daemon is running
- Demonstrate: start daemon → run commands → verify they go through socket → stop daemon
- Show PID file, socket file in `.filament/`
- Test: what happens when daemon dies mid-operation (design for agent death, ADR-009)

### 2. Non-linear dependency graph
- v1 was a straight line: A → B → C → D → E → F → G → H
- v2: diamond dependencies, parallel branches that converge
- Example topology:
  ```
  architecture ──┬── backend-api ───┬── integration-tests ── deploy
                 ├── frontend-spa ──┤
                 ├── auth-service ──┘
                 └── infra-setup ──── monitoring ── deploy
  ```
- Multiple tasks ready simultaneously → agents work in parallel

### 3. True parallel agent simulation
- Multiple terminal sessions (or background processes) acting as different agents
- Each agent process talks to the daemon concurrently
- Demonstrate reservation conflicts happening organically (not staged)
- Show message inbox growing in real-time as agents send artifacts

### 4. TTL expiry + death cleanup
- Reserve files with short TTL (e.g., 30s)
- Let TTL expire, show automatic cleanup
- Simulate agent death (kill a background process)
- Show how the system reclaims reservations from dead agents

### 5. Auto-dispatch (FILAMENT_AUTO_DISPATCH=1)
- Close a task → system automatically finds newly-unblocked tasks
- Chain reaction: closing one task triggers agent dispatch on the next
- Demonstrates the "no ringleader" design (ADR-009)

### 6. MCP server interaction
- Start MCP server alongside daemon
- Show tool calls via the MCP protocol (16 tools)
- Demonstrate how an external AI agent would interact with filament

### 7. Context bundles
- `filament context --around <entity> --depth 3` with richer graph
- Show how the bundle captures the full neighborhood for LLM context injection
- Compare bundle sizes at different depths

### 8. TUI observation
- Run TUI in one terminal while agents operate in others
- Show task list updating, agent status changing, escalation indicator lighting up
- Screenshot/describe the TUI state at key moments

## Proposed scenario: "Microservices Migration"

A monolith is being broken into 5 microservices. More realistic than v1:

**Modules (6):**
- monolith (existing, being decomposed)
- user-service, order-service, payment-service, notification-service, api-gateway

**Agents (6):**
- alice (backend, user-service + order-service)
- bob (backend, payment-service)
- carol (frontend, api-gateway)
- dave (infra, deployment + monitoring)
- eve (QA, testing)
- frank (tech lead, architecture + review)

**Tasks (~15) with diamond deps:**
- Phase 1: architecture design, database migration plan (parallel)
- Phase 2: user-service, order-service, payment-service (parallel, all blocked by phase 1)
- Phase 3: notification-service (blocked by user + order), api-gateway (blocked by all services)
- Phase 4: integration tests (blocked by gateway), load tests (blocked by gateway)
- Phase 5: canary deploy (blocked by tests), full deploy (blocked by canary)

**Complications to simulate:**
- Payment service blocked on PCI compliance review (external blocker)
- Two agents try to modify shared proto files (reservation conflict)
- Agent eve raises quality concern → escalation → scope change mid-sprint
- Frank vetoes a design decision → task gets reassigned
- Notification service depends on a task that gets re-opened after a bug is found
- Auto-dispatch chains 3 tasks in sequence without human intervention

## Execution plan

1. Build release binary
2. Init project in `/tmp/filament-sim-v2/`
3. Start daemon (`filament daemon start`)
4. Seed all entities + complex relation graph
5. Run 15+ cycles with daemon, parallel agents, TTL, auto-dispatch
6. Demonstrate MCP tools (if time)
7. Run TUI alongside (describe state)
8. Export → import round-trip
9. Stop daemon, clean up
10. Write structured log to `.qa/simulation-v2-log-<date>.md`

## Session instructions

When starting the next session, say:
```
Let's run simulation v2 — the daemon-mode microservices migration.
See .plan/simulation-v2-daemon.md for the full plan.
```
