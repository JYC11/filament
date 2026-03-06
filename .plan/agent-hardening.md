# Agent Hardening Sprint

Status: Draft v1
Inspired by: [openai/symphony](https://github.com/openai/symphony)
Created: 2026-03-06

## Motivation

Five targeted improvements to filament's agent dispatch and daemon infrastructure.
All are self-contained, no cross-dependencies, no new architectural concepts.
Total estimate: ~8 tasks across existing modules.

---

## Task 1: Periodic Reconciliation Timer

**Problem:** Filament only reconciles stale agent runs on daemon startup and during
explicit `agent list` calls. If an agent process crashes between those events, the
`agent_runs` row stays `running` and the task stays `in_progress` until someone
happens to query.

**What Symphony does:** Every poll tick (default 30s), the orchestrator checks all
running agents against actual process liveness and cancels stale ones.

**Design:**

Add a second periodic task to the daemon's `serve_with_dispatch` loop, alongside the
existing `expire_stale_reservations` timer.

```
// In crates/filament-daemon/src/lib.rs, inside serve_with_dispatch:
// Existing: reservation cleanup timer (cleanup_interval_secs)
// New: agent reconciliation timer (reconciliation_interval_secs)
```

Every tick:
1. Call `list_running_agents(pool)` — get all `status='running'` rows
2. For each, check if `pid` is alive (`kill(pid, 0)` via `nix::sys::signal` or
   `std::process::Command("kill").args(["-0", &pid.to_string()])`)
3. If dead: call `finish_agent_run(conn, id, Failed, None)`, revert task to `Open`,
   release reservations, refresh graph, emit notification
4. Log reconciled count at info level

**Config:** `reconciliation_interval_secs` in `FilamentConfig` (default 30).
Resolve via `FILAMENT_RECONCILIATION_INTERVAL` env var.

**Files touched:**
- `crates/filament-core/src/config.rs` — add field + resolver
- `crates/filament-daemon/src/lib.rs` — add timer + reconciliation logic
- `crates/filament-daemon/src/state.rs` — add `reconcile_dead_agents()` method on SharedState

**Tests:**
- Unit: mock a dead PID, verify run is marked failed and task reverted
- Integration: start daemon, dispatch agent, kill the child, wait for reconciliation tick

---

## Task 2: Token Usage Tracking

**Problem:** No visibility into how many tokens each agent run consumes. The
`AgentResult` struct has no token fields, and `agent_runs` has no columns for it.

**What Symphony does:** Tracks `input_tokens`, `output_tokens`, `total_tokens` per
session and aggregates globally.

**Design:**

### 2a. Extend AgentResult

Add optional token fields to the `AgentResult` DTO:

```rust
// crates/filament-core/src/dto.rs
pub struct AgentResult {
    // ... existing fields ...
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}
```

These are optional — old agent outputs without them still parse fine (serde defaults
to `None`).

### 2b. Add columns to agent_runs

New migration (`009_agent_token_tracking.sql`):

```sql
ALTER TABLE agent_runs ADD COLUMN input_tokens INTEGER;
ALTER TABLE agent_runs ADD COLUMN output_tokens INTEGER;
ALTER TABLE agent_runs ADD COLUMN total_tokens INTEGER;
```

Update `AgentRun` struct in `models.rs` to include the new fields.

### 2c. Store tokens on completion

In `dispatch.rs`'s `route_agent_result`, after parsing `AgentResult`, write token
counts to the `agent_runs` row. Extend `finish_agent_run` to accept optional token
counts, or do a separate UPDATE.

### 2d. Display tokens

- `filament agent status <id>` — show token counts if present
- `filament agent history <task>` — show per-run tokens + total
- TUI agents tab — add tokens column

**Files touched:**
- `migrations/009_agent_token_tracking.sql` — new migration
- `crates/filament-core/src/dto.rs` — extend AgentResult
- `crates/filament-core/src/models.rs` — extend AgentRun
- `crates/filament-core/src/store.rs` — extend finish_agent_run
- `crates/filament-daemon/src/dispatch.rs` — pass tokens through
- `crates/filament-cli/src/commands/agent.rs` — display tokens
- `crates/filament-tui/src/app.rs` — tokens column (optional)

**Tests:**
- Unit: parse AgentResult with and without token fields
- Unit: finish_agent_run stores tokens, verify via get_agent_run
- Integration: dispatch mock agent that emits tokens, verify storage

---

## Task 3: Agent Timeout

**Problem:** No way to cap how long an agent subprocess can run. A stuck agent runs
forever, holding file reservations and blocking the task.

**What Symphony does:** Configurable timeout per agent run; when exceeded, the agent
is killed and the run is marked failed.

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

## Task 4: Dispatch Lifecycle Hooks

**Problem:** No way to customize the environment before/after an agent runs. Users
may want to set up a branch, install dependencies, collect metrics, or clean up
after the agent finishes.

**What Symphony does:** Four hook points (`after_create`, `before_run`, `after_run`,
`before_remove`) defined as shell scripts in WORKFLOW.md, each with a timeout.

**Design:**

Filament needs only two hooks (we don't have workspace creation/deletion):

- `before_dispatch` — runs before agent subprocess starts. Failure aborts dispatch.
- `after_dispatch` — runs after agent subprocess finishes (any status). Failure logged
  but ignored.

Add to `filament.toml`:

```toml
[hooks]
before_dispatch = "scripts/setup-agent-env.sh"
after_dispatch = "scripts/collect-metrics.sh"
hook_timeout_secs = 60
```

Add to `FilamentConfig`:

```rust
pub before_dispatch_hook: Option<String>,
pub after_dispatch_hook: Option<String>,
pub hook_timeout_secs: Option<u64>,  // default 60
```

Execution in `dispatch_agent` (dispatch.rs):

```
1. Resolve task, validate, build prompt (existing)
2. Run before_dispatch hook (if configured)
   - Command::new("bash").args(["-c", hook_script])
   - Set cwd to project_root
   - Set env: FILAMENT_TASK_SLUG, FILAMENT_TASK_NAME, FILAMENT_AGENT_ROLE
   - Apply hook_timeout_secs via tokio::time::timeout
   - If hook fails or times out: return error, do NOT spawn agent
3. Spawn agent subprocess (existing)
```

In `monitor_agent`, after the agent finishes:

```
1. Parse result, update run, route messages (existing)
2. Run after_dispatch hook (if configured)
   - Same env vars + FILAMENT_AGENT_STATUS, FILAMENT_RUN_ID
   - Failure logged at warn level, not propagated
```

**Files touched:**
- `crates/filament-core/src/config.rs` — add hook fields
- `crates/filament-daemon/src/state.rs` — add to DispatchConfig
- `crates/filament-daemon/src/dispatch.rs` — run hooks before/after

**Tests:**
- Unit: config parsing with hooks present and absent
- Integration: before_dispatch hook that creates a marker file, verify file exists
  after dispatch
- Integration: before_dispatch hook that exits 1, verify dispatch is aborted
- Integration: after_dispatch hook receives correct env vars

---

## Task 5: Role-Based Concurrency Limits

**Problem:** `dispatch-all` has a single `--max-parallel N` cap. All roles compete
for the same slots. You might want 3 coders but only 1 reviewer running
simultaneously.

**What Symphony does:** `max_concurrent_agents_by_state` — per-state concurrency
limits alongside the global cap.

**Design:**

Filament's natural axis is **role** (coder/reviewer/planner/dockeeper), not issue
state. Add per-role limits to `filament.toml`:

```toml
[agent]
max_parallel = 5

[agent.max_parallel_by_role]
coder = 3
reviewer = 1
planner = 1
dockeeper = 1
```

Add to `FilamentConfig`:

```rust
pub max_parallel_by_role: Option<HashMap<String, usize>>,
```

In `dispatch_all` (the CLI command handler), before dispatching each task:

1. Query `list_running_agents(pool)` — group by role
2. For each ready task + role combination, check:
   - Global: `running_count < max_parallel`
   - Per-role: `running_count_for_role < max_parallel_by_role[role]` (if configured)
3. Skip if either limit is hit

The per-role map is optional. If absent, only the global limit applies. Unknown
role keys in config are ignored with a warning.

**Files touched:**
- `crates/filament-core/src/config.rs` — add `max_parallel_by_role` field
- `crates/filament-daemon/src/state.rs` — add to DispatchConfig
- `crates/filament-daemon/src/dispatch.rs` — check role limits in dispatch_all loop
- `crates/filament-cli/src/commands/agent.rs` — `dispatch-all` passes role limits

**Tests:**
- Unit: config parsing with and without per-role limits
- Unit: dispatch_all respects role limit (mock 3 running coders, limit=3, verify
  no new coder dispatched but reviewer still dispatched)

---

## Task Summary

| # | Task | Files | Depends On |
|---|------|-------|------------|
| 1 | Periodic reconciliation timer | daemon lib, config, state | — |
| 2 | Token usage tracking | migration, dto, models, store, dispatch, CLI, TUI | — |
| 3 | Agent timeout | config, state, dispatch | — |
| 4 | Dispatch lifecycle hooks | config, state, dispatch | — |
| 5 | Role-based concurrency limits | config, state, dispatch, CLI | — |

No cross-dependencies. All five can be done in any order.

Tasks 1, 3, 5 are small (config + logic change in existing functions).
Tasks 2, 4 are medium (new migration / new execution path).

## Future: Dispatch Coordinator Channel (mpsc actor-lite)

**Status:** Follow-up after Tasks 1-5. Evaluate once hardening is complete.

**Problem:** Tasks 1, 3, and 5 each add independent logic that touches dispatch
state (running agent counts, timeouts, reconciliation). Today these are scattered:
- `monitor_agent` is a detached tokio task holding `Arc<SharedState>`
- `dispatch_all` queries running agents, applies limits, then spawns
- Reconciliation would be a third independent timer scanning the same data
- Auto-dispatch chains are triggered inside `monitor_agent`

As these grow, the dispatch module becomes a web of concurrent tasks all reaching
into `SharedState` through separate code paths. Race windows multiply.

**Proposal:** Consolidate into a single `dispatch_coordinator` tokio task that owns
all dispatch state and receives messages via `tokio::mpsc`:

```rust
enum DispatchMessage {
    // External requests
    DispatchTask { slug: String, role: AgentRole, reply: oneshot::Sender<Result<AgentRunId>> },
    DispatchAll { role: AgentRole, max: usize, reply: oneshot::Sender<Result<Vec<AgentRunId>>> },

    // Internal lifecycle events
    AgentFinished { run_id: AgentRunId, result: AgentResult },
    AgentTimedOut { run_id: AgentRunId },
    MonitorPanicked { run_id: AgentRunId },

    // Periodic ticks
    ReconcileTick,
    TimeoutCheck,

    // Config
    ReloadConfig,
}
```

**Coordinator state (owned, not shared):**

```rust
struct DispatchCoordinator {
    // Owned state — no Arc/RwLock needed
    running: HashMap<AgentRunId, RunningAgent>,
    monitors: JoinSet<(AgentRunId, Result<AgentResult>)>,
    config: DispatchConfig,

    // Shared resources (still Arc'd, but accessed only from this task)
    store: FilamentStore,
    graph: Arc<RwLock<KnowledgeGraph>>,
    notify_tx: broadcast::Sender<Notification>,

    // Channel
    rx: mpsc::Receiver<DispatchMessage>,
}

struct RunningAgent {
    pid: u32,
    role: AgentRole,
    task_slug: String,
    started_at: Instant,
    timeout: Duration,
}
```

**Main loop:**

```rust
impl DispatchCoordinator {
    async fn run(mut self) {
        let mut reconcile = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => self.handle(msg).await,
                _ = reconcile.tick() => self.reconcile().await,
                Some(result) = self.monitors.join_next() => {
                    self.handle_monitor_result(result).await;
                }
            }
        }
    }
}
```

**What this consolidates:**

| Current (scattered) | Coordinator (centralized) |
|---------------------|--------------------------|
| `dispatch_agent()` checks running count via DB query | Coordinator checks `self.running.len()` — no DB round-trip |
| `monitor_agent` is a detached task, panics are invisible | `JoinSet` catches panics, coordinator handles cleanup |
| Reconciliation timer scans DB for stale runs | Coordinator knows all running agents, checks PIDs directly |
| Timeout is per-monitor via `tokio::time::timeout` | Coordinator tracks `started_at` + `timeout` per agent |
| Role limits require DB query + grouping | Coordinator counts by role from `self.running` |
| Auto-dispatch is triggered inside monitor_agent | `AgentFinished` handler checks unblocked tasks |

**Benefits:**
- Single source of truth for running agent state (no TOCTOU between DB and reality)
- Monitor panics are caught and handled (via JoinSet)
- No race between reconciliation, timeout, and monitor — they're sequential in the
  same select loop
- Role-based limits are O(1) lookups, not DB queries
- Testable in isolation — feed messages, assert state transitions

**Risks:**
- Refactor of dispatch.rs — not a bolt-on
- Single point of failure (if the coordinator task panics, all dispatch stops).
  Mitigate: wrap in a supervisor loop that restarts + reconciles from DB.
- Adds a message-passing indirection layer. For <10 agents this is pure overhead
  from a performance standpoint — the value is in correctness, not throughput.

**When to do it:** After Tasks 1-5 are done and working. If the hardening tasks
feel clean and the dispatch module isn't getting tangled, skip this. If Tasks 1+3+5
create uncomfortable interactions, this is the cleanup.

**Not recommended:**
- Full actor framework (actix, ractor) — too heavy for <10 agents
- Actor-per-agent-run — subprocess lifecycle is an OS concern, not an actor concern
- Making the store an actor — SQLite already serializes writes, adding a mailbox
  just adds latency

---

## Open Questions

1. **Token source:** Claude's `-p` (print) mode outputs a summary line with token
   counts. Do we parse that, or do we rely on the agent subprocess to include tokens
   in its `AgentResult` JSON? The latter is cleaner but requires agent cooperation.
   Recommendation: support both — parse from AgentResult first, fall back to scanning
   stdout for Claude's token summary line.

2. **Hook env vars:** Should hooks receive the full task JSON (via a temp file or env
   var), or just slug + name + role? Start minimal, extend later.

3. **Reconciliation vs. monitor:** Task 1's reconciliation timer is a safety net for
   the case where `monitor_agent` fails to fire (e.g., daemon restart). It should NOT
   duplicate monitor_agent's routing logic — it only marks runs as failed and reverts
   tasks. The full result routing only happens through the monitor path.

4. **Timeout vs. reconciliation interaction:** If an agent times out (task 3), the
   monitor handles cleanup immediately. The reconciliation timer (task 1) should skip
   runs that are already being handled by an active monitor. Use the `pid` field —
   if pid is dead, reconcile; if pid is alive and within timeout, skip.

---

## Task 6: TUI Integration (Moved)

**Moved to:** `.plan/tui-enhancement.md` (Phase 3: Tasks 3.1–3.3)

The TUI informational tabs (Config, Analytics, Lessons) are now part of the
broader TUI Enhancement Epic which also covers entity CRUD, filtering, paging,
and graph view improvements.
