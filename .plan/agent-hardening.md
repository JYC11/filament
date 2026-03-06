# Agent Hardening Sprint

Status: **Complete** (2026-03-06). Commit: e4169bb
Inspired by: [openai/symphony](https://github.com/openai/symphony)
Created: 2026-03-06
Revised: 2026-03-06

## Motivation

Two targeted improvements to filament's agent dispatch infrastructure.
Both are self-contained, no cross-dependencies, no new architectural concepts.

---

## Task 1: Periodic Reconciliation Timer

**Problem:** Filament only reconciles stale agent runs on daemon startup and during
explicit `agent list` calls. If an agent process crashes between those events, the
`agent_runs` row stays `running` and the task stays `in_progress` until someone
happens to query.

**Design:**

Add a second periodic task to the daemon's `serve_with_dispatch` loop, alongside the
existing `expire_stale_reservations` timer.

Every tick:
1. Call `list_running_agents(pool)` — get all `status='running'` rows
2. For each, check if `pid` is alive via `std::process::Command("kill").args(["-0", &pid.to_string()])`
   (project forbids `unsafe` — no `nix` crate)
3. If dead: call `finish_agent_run(conn, id, Failed, None)`, revert task to `open`
4. Log reconciled count at info level

**Scope note:** Reconciliation is a **safety net** — it only marks dead runs as failed
and reverts task status. It does NOT duplicate `monitor_agent`'s full routing logic
(message routing, auto-dispatch chaining, etc.). The full result path only runs
through the monitor.

**Interaction with timeout (Task 2):** If an agent is still alive but past its timeout,
reconciliation skips it — the monitor's `tokio::time::timeout` handles that case.
Reconciliation only acts on dead PIDs.

**Config:** `reconciliation_interval_secs` in `FilamentConfig` (default 30).
Resolve via `FILAMENT_RECONCILIATION_INTERVAL` env var.

**Files touched:**
- `crates/filament-core/src/config.rs` — add field + resolver
- `crates/filament-daemon/src/lib.rs` — add timer + reconciliation logic
- `crates/filament-daemon/src/state.rs` — add `reconcile_dead_agents()` method on SharedState

**Tests:**
- Unit: mock a dead PID, verify run is marked failed and task reverted
- Integration: start daemon, create a fake running agent_run with a dead PID, wait for reconciliation tick, verify cleanup

---

## Task 2: Agent Timeout

**Problem:** No way to cap how long an agent subprocess can run. A stuck agent runs
forever, holding file reservations and blocking the task.

**Design:**

Add `agent_timeout_secs` to `FilamentConfig` (default: 3600 = 1 hour, 0 = no timeout).
Resolve via `FILAMENT_AGENT_TIMEOUT` env var.

In `monitor_agent` (dispatch.rs), wrap the `spawn_blocking(waitpid)` call with
`tokio::time::timeout`:

```rust
let timeout_duration = Duration::from_secs(config.agent_timeout_secs);
match tokio::time::timeout(timeout_duration, blocking_wait).await {
    Ok(exit_result) => { /* existing logic */ },
    Err(_elapsed) => {
        // Kill the child
        child.kill();
        // Mark run as failed with timeout message
        finish_agent_run(conn, run_id, AgentStatus::Failed, Some("timeout")).await;
        // Revert task, release reservations, etc.
    }
}
```

Pass timeout through `DispatchConfig`.

**Files touched:**
- `crates/filament-core/src/config.rs` — add field + resolver
- `crates/filament-daemon/src/state.rs` — add to DispatchConfig
- `crates/filament-daemon/src/dispatch.rs` — add timeout to monitor_agent

**Tests:**
- Unit: verify timeout config resolution (env > config > default)
- Integration: dispatch a `sleep 999` subprocess with 2s timeout, verify it's killed
  and run is marked failed

---

## Task Summary

| # | Task | Priority | Files | Depends On |
|---|------|----------|-------|------------|
| 1 | Periodic reconciliation timer | P2 | daemon lib, config, state | — |
| 2 | Agent timeout | P2 | config, state, dispatch | — |

No cross-dependencies. Either can be done first.

---

## Dropped Tasks (with rationale)

### Token Usage Tracking (was Task 2) — Deferred

No reliable token source. Claude's `-p` output format is not a stable API. The
`AgentResult` protocol exists but no agents populate it today. Adding a migration,
model fields, CLI columns, and TUI columns for data that will always be `None` is
dead code. Revisit when agent dispatch has real users and a reliable token source.

### Dispatch Lifecycle Hooks (was Task 4) — Dropped

YAGNI. No one is using dispatch today. The examples (set up branches, collect metrics)
are speculative. Users can wrap `filament agent dispatch` in their own shell scripts
for the same effect with more flexibility. Filament already has git hooks via
`filament hook install` — a second hook system creates confusion.

### Role-Based Concurrency Limits (was Task 5) — Dropped

Premature optimization. `dispatch-all --max-parallel N` covers the simple case.
Per-role limits assume a scale of agent dispatching that hasn't been reached. If a
future dispatch coordinator is built, role limits would be trivial to add there.

### Dispatch Coordinator Channel — Deferred

With only 2 tasks (reconciliation timer + timeout), the coordinator actor pattern is
overkill. Two timers in the daemon loop don't need an mpsc channel. Revisit if
dispatch complexity grows.

---

## Open Questions

1. **Reconciliation vs. monitor:** Task 1's reconciliation timer is a safety net for
   the case where `monitor_agent` fails to fire (e.g., daemon restart). It should NOT
   duplicate monitor_agent's routing logic — it only marks runs as failed and reverts
   tasks. The full result routing only happens through the monitor path.

2. **Timeout vs. reconciliation interaction:** If an agent times out (task 2), the
   monitor handles cleanup immediately. The reconciliation timer (task 1) should skip
   runs that are already being handled by an active monitor. Use the `pid` field —
   if pid is dead, reconcile; if pid is alive and within timeout, skip.
