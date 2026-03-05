# Persistent Job Queue Plan

**Epic:** `mt0dfxvu` — persistent-job-queue
**Status:** Planning
**Date:** 2026-03-05
**Research:** `.plan/research-job-queues.md`

## Problem

Filament's current dispatch is fire-and-forget: `dispatch_agent()` spawns a subprocess, `monitor_agent()` watches it, auto-dispatch chains newly unblocked tasks. This works for interactive use but breaks down for:

1. **CI/build jobs** — need reliable retry, backoff, timeout, dead-letter
2. **Batch dispatch** — `dispatch-all` is eager (spawns all at once), no rate limiting
3. **Daemon restart** — running agents are orphaned; only reconciliation marks them failed
4. **Scheduling** — no way to schedule future or recurring jobs
5. **Observability** — agent_runs table tracks runs but not queue depth, wait time, throughput

## Use Cases

| Use case | Priority | Notes |
|----------|----------|-------|
| Reliable agent dispatch with retry | P1 | Core: enqueue task, worker claims + dispatches, retry on failure |
| CI/build job execution | P1 | User's primary use case: run build/test commands with retry |
| Priority ordering | P1 | High-priority jobs processed first |
| Backoff on failure | P1 | Exponential backoff to avoid hammering a broken build |
| Concurrency limit | P1 | Don't spawn 20 agents at once |
| Scheduled jobs | P2 | Delayed execution (retry backoff), future scheduling |
| Recurring jobs | P3 | Periodic cleanup, health checks — nice to have |
| Dead letter queue | P2 | Jobs that exhaust retries go somewhere visible |
| Job timeout | P2 | Kill jobs that run too long |

## Decision: Custom Implementation

### Why not apalis

Despite being the best external option, apalis has several friction points for filament:

1. **RC instability** — 1.0.0-rc.4 means breaking API changes are expected
2. **Migration coordination** — apalis-sqlite runs its own migrations (Jobs + Workers tables) via `SqliteStorage::setup()`, separate from filament's sqlx migration system
3. **No built-in backoff** — the one feature we need most for CI/build jobs requires custom middleware
4. **Abstraction mismatch** — apalis is a general-purpose job framework with tower layers, codecs, and abstractions we don't need. Filament already has a daemon, dispatch, monitoring, and subprocess management
5. **Dependency weight** — apalis + apalis-sql + apalis-sqlite pulls in ulid, futures, tower, and more
6. **Low SQLite usage** — 3.4K downloads on apalis-sqlite vs 615K on core suggests the SQLite backend is not well battle-tested

### Why custom works

Filament already has 80% of the infrastructure:

| Component | Already exists | What to add |
|-----------|---------------|-------------|
| SQLite + sqlx + migrations | YES | Job queue table + indexes |
| Daemon process | YES | Worker loop inside daemon |
| Subprocess spawning | YES (`dispatch_agent`) | Wire through job queue |
| Process monitoring | YES (`monitor_agent`) | Report results back to queue |
| Task priority model | YES (0-4 integer) | Map to job priority |
| Stale run cleanup | YES (`reconcile_stale_agent_runs`) | Extend to job queue |
| Config system | YES (`filament.toml`) | Add queue settings |
| Error types | YES (`FilamentError`) | Add queue error variants |
| Socket notifications | YES (`filament watch`) | Emit job events |
| CLI framework | YES (clap) | Add `job` subcommand group |

Estimated effort: **3-4 days** for a solid implementation. The atomic claim pattern in SQLite is well-understood (single `UPDATE ... RETURNING`).

## Design

### Schema (migration 009 or 010)

```sql
CREATE TABLE jobs (
    id            TEXT PRIMARY KEY,           -- UUID
    job_type      TEXT NOT NULL,              -- 'agent_dispatch' | 'shell_command' | 'cleanup'
    payload       TEXT NOT NULL,              -- JSON: type-specific data
    status        TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','claimed','running','done','failed','dead')),
    priority      INTEGER NOT NULL DEFAULT 2, -- 0=highest, 4=lowest (matches entity priority)
    attempts      INTEGER NOT NULL DEFAULT 0,
    max_attempts  INTEGER NOT NULL DEFAULT 3,
    run_at        TEXT NOT NULL DEFAULT (datetime('now')),  -- delayed/backoff scheduling
    claimed_by    TEXT,                       -- worker ID (daemon instance)
    claimed_at    TEXT,
    started_at    TEXT,
    finished_at   TEXT,
    last_error    TEXT,                       -- last failure message
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    -- Backoff: base_backoff_secs * 2^attempts (capped at max_backoff_secs)
    base_backoff_secs INTEGER NOT NULL DEFAULT 5,
    max_backoff_secs  INTEGER NOT NULL DEFAULT 300,
    -- Optional link to filament entity
    entity_id     TEXT,                       -- FK to entities.id (nullable)
    -- Timeout
    timeout_secs  INTEGER                     -- NULL = no timeout
);

-- Priority-ordered fetch of claimable jobs
CREATE INDEX idx_jobs_claimable ON jobs(priority ASC, run_at ASC)
    WHERE status = 'pending' AND run_at <= datetime('now');

-- Lookup by entity (e.g., find jobs for a task)
CREATE INDEX idx_jobs_entity ON jobs(entity_id) WHERE entity_id IS NOT NULL;

-- Cleanup queries (find dead/done jobs older than X)
CREATE INDEX idx_jobs_status ON jobs(status, finished_at);
```

Note: priority ASC because filament uses 0=highest (unlike apalis which uses DESC).

### Job Types

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JobPayload {
    /// Dispatch an agent to work on a task
    AgentDispatch {
        task_slug: String,
        role: String,        // coder, reviewer, planner, dockeeper
        use_worktree: bool,
    },
    /// Run a shell command (CI/build)
    ShellCommand {
        command: String,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: HashMap<String, String>,
    },
    /// Internal maintenance
    Cleanup {
        kind: CleanupKind,   // StaleWorktrees, ExpiredReservations, OldJobs
    },
}
```

### Atomic Claim (core query)

```sql
UPDATE jobs
SET status = 'claimed',
    claimed_by = ?1,
    claimed_at = datetime('now'),
    attempts = attempts + 1
WHERE id = (
    SELECT id FROM jobs
    WHERE status = 'pending'
      AND run_at <= datetime('now')
    ORDER BY priority ASC, run_at ASC
    LIMIT 1
)
RETURNING *;
```

Single statement, atomic in SQLite. No SKIP LOCKED needed (SQLite serializes writes).

### Worker Loop

Runs inside the daemon process (no separate binary):

```
loop {
    if active_jobs < max_concurrent {
        job = claim_next_job(worker_id)
        if job:
            tokio::spawn(execute_job(job))
        else:
            sleep(poll_interval)  // 1-2 seconds
    } else {
        sleep(poll_interval)
    }
}
```

### Job Lifecycle

```
                  enqueue
                    │
                    ▼
              ┌─────────┐
              │ pending  │◄──── retry (with backoff)
              └────┬─────┘
                   │ claim
                   ▼
              ┌─────────┐
              │ claimed  │
              └────┬─────┘
                   │ start execution
                   ▼
              ┌─────────┐
              │ running  │──── timeout? ──► fail + retry/dead
              └────┬─────┘
                   │
              ┌────┴────┐
              │         │
              ▼         ▼
         ┌────────┐ ┌────────┐
         │  done  │ │ failed │──── attempts < max? ──► pending (with run_at += backoff)
         └────────┘ └────┬───┘
                         │ attempts >= max
                         ▼
                    ┌────────┐
                    │  dead  │  (dead letter)
                    └────────┘
```

### Backoff Calculation

```rust
fn next_run_at(job: &Job) -> DateTime<Utc> {
    let backoff = job.base_backoff_secs * 2u64.pow(job.attempts as u32);
    let capped = backoff.min(job.max_backoff_secs);
    Utc::now() + Duration::from_secs(capped)
}
```

Default: 5s → 10s → 20s → 40s → 80s → 160s → 300s (cap)

### Integration with Existing Dispatch

Two modes:

1. **Direct dispatch** (unchanged) — `filament agent dispatch <task>` works as today for interactive use
2. **Queue dispatch** — `filament job enqueue <task>` puts it in the queue, daemon worker picks it up

The queue worker calls the same `dispatch_agent()` internally. The queue adds reliability around it.

For `dispatch-all`, can optionally route through the queue instead of spawning all at once.

### CLI Commands

```
filament job enqueue <TASK_OR_COMMAND> [--priority N] [--max-attempts N] [--delay SECS] [--timeout SECS]
filament job list [--status STATUS] [--limit N]
filament job show <JOB_ID>
filament job cancel <JOB_ID>
filament job retry <JOB_ID>          # re-enqueue a failed/dead job
filament job purge [--status done|dead] [--older-than DAYS]
filament job stats                   # queue depth, throughput, failure rate
```

### ShellCommand Jobs (CI/Build)

```bash
# Enqueue a build job
filament job enqueue --shell "cargo build --release" --max-attempts 3

# Enqueue a test job with higher priority
filament job enqueue --shell "cargo test" --priority 1 --timeout 600

# Enqueue an agent dispatch through the queue
filament job enqueue --task <slug> --role coder

# Chain: build then test (second job depends on first)
filament job enqueue --shell "cargo build" --priority 1
filament job enqueue --shell "cargo test" --priority 2 --delay 0
```

### Config (`filament.toml`)

```toml
[queue]
enabled = true
max_concurrent = 3          # max parallel jobs
poll_interval_ms = 1000     # how often to check for new jobs
default_max_attempts = 3
default_timeout_secs = 3600 # 1 hour
purge_done_after_days = 7   # auto-cleanup completed jobs
```

### Daemon Integration

The worker loop starts inside `serve_with_dispatch()`:

1. On daemon startup: claim orphaned `claimed`/`running` jobs → mark failed (like `reconcile_stale_agent_runs`)
2. Spawn worker loop as a tokio task
3. Worker respects `max_concurrent` from config
4. On daemon shutdown: gracefully stop worker (finish current jobs, don't claim new ones)

## Tasks

### Phase 1: Core queue (storage + worker)

1. **[T1] Job queue schema + store** — migration, model, CRUD
   - Migration: `jobs` table with indexes
   - Model: `Job`, `JobPayload`, `JobStatus` types
   - Store: `enqueue_job`, `claim_next_job`, `finish_job`, `fail_job`, `retry_job`, `get_job`, `list_jobs`, `cancel_job`, `purge_jobs`, `job_stats`
   - Tests: enqueue, claim ordering, retry with backoff, dead letter, cancel, purge
   - **Deps:** none
   - **Files:** `migrations/000N_job_queue.sql`, `crates/filament-core/src/models.rs`, `crates/filament-core/src/store.rs`

2. **[T2] Worker loop in daemon**
   - `JobWorker` struct: poll interval, max_concurrent, active job tracking
   - `run_worker()` async loop: claim → spawn executor → track completion
   - `execute_job()`: match on `JobPayload`, dispatch accordingly
   - `ShellCommand` executor: `Command::new()`, capture output, timeout via `tokio::time::timeout`
   - `AgentDispatch` executor: call existing `dispatch_agent()` internals
   - Graceful shutdown on daemon stop
   - Orphan recovery on startup
   - Tests: worker claims and executes, respects concurrency limit, retries on failure, backoff timing
   - **Deps:** T1
   - **Files:** `crates/filament-daemon/src/worker.rs`, `crates/filament-daemon/src/lib.rs`

### Phase 2: CLI + integration

3. **[T3] CLI `job` subcommand group**
   - `enqueue`, `list`, `show`, `cancel`, `retry`, `purge`, `stats`
   - Protocol messages for each operation
   - Connection/Client methods
   - Tests: CLI integration tests
   - **Deps:** T1
   - **Files:** `crates/filament-cli/src/commands/job.rs`, `crates/filament-core/src/protocol.rs`, `crates/filament-core/src/connection.rs`, `crates/filament-core/src/client.rs`

4. **[T4] Daemon handler for job operations**
   - Handle job protocol messages (enqueue, list, show, cancel, retry, purge, stats)
   - Route through existing daemon handler dispatch
   - **Deps:** T2, T3
   - **Files:** `crates/filament-daemon/src/handler/job.rs`, `crates/filament-daemon/src/handler/mod.rs`

### Phase 3: Config + observability

5. **[T5] Queue config in filament.toml**
   - `[queue]` section in config schema
   - Env vars: `FILAMENT_QUEUE_ENABLED`, `FILAMENT_QUEUE_MAX_CONCURRENT`, etc.
   - Resolution: CLI flag > env > config > defaults
   - **Deps:** T2
   - **Files:** `crates/filament-core/src/config.rs`, `crates/filament-daemon/src/state.rs`

6. **[T6] Job events + watch integration**
   - Emit events when jobs transition states (enqueued, claimed, done, failed, dead)
   - `filament watch --events job_enqueued,job_done,job_failed`
   - **Deps:** T2
   - **Files:** `crates/filament-daemon/src/worker.rs`

### Phase 4: Polish

7. **[T7] TUI job queue view**
   - New tab in TUI showing job queue status
   - Columns: ID, type, status, priority, attempts, created, error
   - **Deps:** T4
   - **Files:** `crates/filament-tui/src/...`

8. **[T8] Docs + skill update**
   - Update CLAUDE.md with job queue commands
   - Update filament skill
   - **Deps:** T7
   - **Files:** CLAUDE.md, `.claude/skills/filament/filament.md`

## Dependency Graph

```
T1 (schema + store) ──┬── T2 (worker loop) ──┬── T4 (daemon handler)  ── T7 (TUI)
                      │                       ├── T5 (config)              │
                      │                       └── T6 (events)             T8 (docs)
                      └── T3 (CLI + protocol) ─┘
```

T1 and T3 can start in parallel (T3 needs T1 for types but can stub).
T2, T5, T6 parallelize after T1.
T4 needs T2 + T3.
T7 needs T4.
T8 needs T7.

## Risk / Open Questions

### Job-Entity relationship
Jobs can optionally link to an entity (task) via `entity_id`. This is a soft reference — deleting the entity doesn't cascade to the job. The job payload contains everything needed to execute.

### Queue vs direct dispatch coexistence
Both modes coexist. Direct dispatch is for interactive use (immediate feedback). Queue dispatch is for batch/CI (reliability). No forced migration — users adopt the queue when they need it.

### ShellCommand security
`ShellCommand` jobs execute arbitrary commands. Since filament is local-only and single-user, this is acceptable (same trust model as `dispatch_agent` spawning `claude -p`). The command runs as the daemon process user.

### SQLite write contention
The worker polls with `UPDATE ... RETURNING` which briefly locks the DB. With a 1-second poll interval and single worker process, this is negligible. If multiple daemon instances exist (they shouldn't — PID file prevents it), SQLite's serialized writes handle it correctly.

### Job output capture
`ShellCommand` jobs capture stdout/stderr. Where to store?
- Short output: in `result_json` column (like agent runs)
- Long output: write to `.filament/job-output/{job_id}.log`, store path in result_json
- Decision: start with result_json, add file output if needed

### Migration numbering
Depends on whether worktree dispatch (migration 009) lands first. Use next available number.

---

## Design Review / Critique (2026-03-05)

**Status: UNRESOLVED — must address before implementation.**

### Issue 1: Dual state machine (CRITICAL)

An `AgentDispatch` job creates two parallel status trackers — `jobs.status` and `agent_runs.status` — for the same work unit:

| Time | `jobs.status` | `agent_runs.status` |
|------|--------------|-------------------|
| enqueued | pending | — |
| worker claims | claimed | — |
| worker starts dispatch | running | running |
| agent finishes | ??? | completed |
| worker notices | done | completed |

Who's authoritative? If the daemon crashes after `agent_runs` is marked `completed` but before `jobs` is marked `done`, orphan recovery could re-queue the job and the agent runs *again*. This is the classic dual-write problem — two tables tracking the same lifecycle with no consistency mechanism.

### Issue 2: ShellCommand is scope creep (HIGH)

Building a general shell executor is a different product. It needs:
- **Output streaming** — builds produce megabytes. `result_json` won't work day one.
- **Process cancellation** — `job cancel` must kill the running process, not just flip a status bit.
- **Signal handling** — SIGTERM, SIGKILL, grace periods.
- **Exit code semantics** — is exit code 1 "failed" or "test failures found"?

Filament already has a battle-tested subprocess manager (`ChildGuard`, `monitor_agent`). `ShellCommand` rebuilds all of that from scratch. Could this be deferred to v2, or should shell commands go through agent dispatch (where the agent runs the build)?

### Issue 3: `claimed` status is unnecessary (MEDIUM)

In distributed systems, `claimed` means "a remote worker reserved this but hasn't started yet." In filament's single-daemon model, the worker claims and starts execution in the same function call. If the daemon crashes between `claimed` and `running`, the job is stuck — requiring yet another recovery path. Simplify to `pending → running → done/failed/dead`.

### Issue 4: No job dependencies (HIGH)

The plan's "chain" example uses priority + delay, which doesn't guarantee sequencing. If `cargo build` fails, `cargo test` still runs. Real CI pipelines need: "run B only after A succeeds." The plan has no job-to-job dependency mechanism — which is exactly what CI/build needs.

### Issue 5: `entity_id` creates silent orphans (MEDIUM)

If you delete an entity, queued jobs for it still execute. If you close a task, queued dispatch jobs still fire. No cascade, no validation. The plan says "soft reference" but this means jobs execute against stale/deleted context.

### Issue 6: Polling wastes cycles (LOW)

The daemon already has a pub/sub event system. A 1-second poll loop burns CPU when the queue is empty (99% of the time). The worker should be notified on enqueue and sleep otherwise.

### Issue 7: Two concurrency limits (MEDIUM)

`dispatch-all --max-parallel N` already exists. `queue.max_concurrent` adds a second, independent limit. If someone sets `max-parallel 5` but `max_concurrent 3`, behavior is confusing. These need to be unified or one needs to supersede the other.

### Issue 8: `Cleanup` job type is YAGNI (LOW)

No current need for scheduled cleanup jobs. The daemon cleans up on startup. Adding this variant "because we might need it" adds a code path with zero users. Cut from v1.

### Issue 9: Backoff params per-job is over-engineering (LOW)

Storing `base_backoff_secs` and `max_backoff_secs` on every job row is 99% wasted — almost all jobs will use the same defaults. This should be config-level with a rare per-job override, not two columns on every row.

### Issue 10: The fundamental question

Before refining tasks, resolve the architectural direction:

**Option A: Enhance existing dispatch** — Add `max_attempts`, `backoff_secs`, `retry_count` columns to `agent_runs`. Retry logic in `monitor_agent`. `dispatch-all` becomes the queue (it already is one, just eager). No new tables, no new CLI commands, no dual state machine. Smallest change, solves the agent reliability problem, but doesn't address shell commands or scheduling.

**Option B: Separate job queue, single source of truth** — The current plan, but jobs *replace* `agent_runs` tracking for queued work (not duplicate it). `AgentDispatch` jobs write to `jobs` table only, not `agent_runs`. Shell commands are a future extension, not v1. Solves the dual-write problem but requires refactoring dispatch to optionally skip `agent_runs`.

**Option C: Job queue for shell commands only** — Agent dispatch stays as-is (it works). The queue is specifically for `filament job run "cargo test"` — serialized shell execution with retry. Completely orthogonal to agent dispatch. Clean separation, but doesn't improve agent dispatch reliability.

**Decision needed before proceeding to tasks.**
