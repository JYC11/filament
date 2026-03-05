# Rust Persistent Job Queue Research

**Date:** 2026-03-05
**Purpose:** Evaluate crates for filament's persistent-job-queue epic (`mt0dfxvu`)
**Context:** Filament uses sqlx + SQLite (runtime-tokio). Need embeddable, async, priority-aware job queue.

---

## Executive Summary

| Crate | Backend | Stars | Downloads | Last Update | Verdict |
|-------|---------|-------|-----------|-------------|---------|
| **apalis + apalis-sqlite** | SQLite (sqlx) | 1104 | 615K + 3.4K | 2026-02-21 | **Best fit** |
| **effectum** | SQLite (rusqlite) | 46 | 16K | 2024-07-23 | Strong alternative |
| qoxide | SQLite (rusqlite?) | 0 | 208 | 2025-12-05 | Too immature |
| disk_backed_queue | SQLite | 0 | 43 | 2025-12-01 | Too immature |
| aide-de-camp-sqlite | SQLite | ? | 4.8K | 2022-12-18 | Abandoned |
| sqlxmq | PostgreSQL only | 158 | 161K | 2025-05-25 | Wrong backend |
| backie | PostgreSQL (Diesel) | 47 | 65K | 2023-07-12 | Archived |
| faktory | External server | 237 | 85K | 2026-02-28 | Wrong model |
| rusty-celery | Redis/AMQP | 861 | ? | 2024-06-17 | Wrong backend |
| hammerwork | Postgres/MySQL | ? | 17K | 2025-08-29 | Wrong backend |
| background-jobs | Pluggable (sled) | ? | ? | ? | No SQLite backend |

**Recommendation:** apalis + apalis-sqlite is the clear winner for filament. Effectum is a viable alternative if we want fewer dependencies or hit issues with apalis. Building custom on raw sqlx is the fallback.

---

## Tier 1: SQLite-Backed (Viable for Filament)

### 1. apalis + apalis-sqlite

**Version:** 1.0.0-rc.4 (pre-release but actively developed)
**GitHub:** https://github.com/apalis-dev/apalis + https://github.com/apalis-dev/apalis-sqlite
**Stars:** 1,104 | **Downloads:** 615K (core) + 3.4K (sqlite)
**Last Commit:** 2026-02-21 (sqlite), 2026-02-24 (core)
**License:** MIT

**Architecture:**
- Core framework (`apalis`) with pluggable backends (redis, postgres, mysql, sqlite, amqp, nats, libsql)
- `apalis-sqlite` is a separate repo using `sqlx` 0.8.6 with SQLite
- `apalis-sql` provides shared SQL abstractions

**Key Features:**
- Priorities (INTEGER, DESC ordering)
- Max attempts with configurable retries
- Scheduled execution (`run_at` timestamp)
- Worker registration and heartbeat tracking
- Orphaned job re-enqueueing (via `reenqueue_orphaned.sql`)
- Job locking (optimistic, via UPDATE ... WHERE + RETURNING)
- Multiple storage modes: polling-based, event-driven (SQLite update hooks), shared
- Middleware/layer support (tower-like)
- Workflow engine (`apalis-workflow`)
- Custom codecs for serialization
- Cron scheduling (`apalis-cron`)
- Dashboard/board (`apalis-board`)

**SQLite Schema (key tables):**
```sql
-- Jobs table
CREATE TABLE Jobs (
    job TEXT NOT NULL,           -- serialized job payload
    id TEXT NOT NULL UNIQUE,     -- job ID
    job_type TEXT NOT NULL,      -- queue/job type name
    status TEXT NOT NULL DEFAULT 'Pending',  -- Pending|Queued|Running|Done|Failed|Killed
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 25,
    run_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    priority INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    lock_at INTEGER,
    lock_by TEXT,                -- worker ID
    done_at INTEGER,
    FOREIGN KEY(lock_by) REFERENCES Workers(id)
);

-- Workers table
CREATE TABLE Workers (
    id TEXT NOT NULL UNIQUE,
    worker_type TEXT NOT NULL,
    storage_name TEXT NOT NULL,
    layers TEXT,
    last_seen INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

**Fetch query (atomic claim):**
```sql
UPDATE Jobs SET status = 'Queued', lock_by = ?1, lock_at = strftime('%s', 'now')
WHERE ROWID IN (
    SELECT ROWID FROM Jobs
    WHERE job_type = ?2
      AND ((status = 'Pending' AND lock_by IS NULL) OR (status = 'Failed' AND attempts < max_attempts))
      AND (run_at IS NULL OR run_at <= strftime('%s', 'now'))
    ORDER BY priority DESC, run_at ASC, id ASC
    LIMIT ?3
) RETURNING *
```

**API Style:** Async, Tokio-compatible. Builder pattern.
```rust
let pool = SqlitePool::connect(":memory:").await?;
SqliteStorage::setup(&pool).await?;
let mut backend = SqliteStorage::new(&pool);

// Push a job
let task = Task::builder(my_job_data)
    .run_after(Duration::from_secs(60))
    .priority(5)
    .max_attempts(3)
    .build();
backend.push(task).await?;

// Build and run worker
let worker = WorkerBuilder::new("worker-1")
    .backend(backend)
    .build(my_handler);
worker.run().await?;
```

**Dependencies:** sqlx 0.8.6 (same as filament!), tokio, serde, futures, thiserror 2, ulid

**Pros:**
- Uses sqlx (same as filament) -- can potentially share the connection pool
- Actively maintained, frequent commits (Feb 2026)
- 1,100+ stars, largest community
- Rich feature set: priorities, retries, scheduling, worker tracking, orphan recovery
- Event-driven mode via SQLite update hooks (low latency)
- Middleware/layer architecture (tower-like)
- Workflow engine for complex job chains
- Separate cron scheduler
- Dashboard UI available

**Cons:**
- Still RC (1.0.0-rc.4), API may change
- apalis-sqlite has only 3.4K downloads (niche compared to redis/postgres backends)
- 5 open issues on sqlite repo (mostly dep bumps, 1 Windows bug)
- Two separate repos to track (core + sqlite)
- Job payload stored as TEXT (JSON), not typed
- No built-in exponential backoff (retries are immediate re-queue on next poll)
- Heavy dependency tree (full apalis ecosystem)

**Gotchas:**
- `apalis-sqlite` uses its own migration system -- would need to coordinate with filament's migrations
- Event-driven mode (`SqliteStorageWithHook`) requires its own connection (can't share pool easily)
- Worker heartbeat is polling-based (last_seen updates)

---

### 2. effectum

**Version:** 0.7.0
**GitHub:** https://github.com/dimfeld/effectum
**Stars:** 46 | **Downloads:** 16K
**Last Commit:** 2024-07-23 (1.5 years ago)
**License:** MIT OR Apache-2.0

**Architecture:**
- Embeddable SQLite-only job queue
- Uses `rusqlite` (NOT sqlx) with `deadpool-sqlite` for async
- Single crate, self-contained

**Key Features:**
- Priorities (integer, DESC ordering)
- Job weight (for concurrency budgeting)
- Retry with exponential backoff (configurable multiplier, randomization, initial interval)
- Scheduled execution (`run_at`)
- Job timeout (`expires_at` with heartbeat extension)
- Checkpointing (resume failed jobs from checkpoint)
- Recurring/cron jobs
- Cancel or modify pending jobs
- Job status tracking
- Max concurrency per worker

**SQLite Schema:**
```sql
CREATE TABLE jobs (
    job_id INTEGER PRIMARY KEY,
    external_id blob not null,
    job_type text not null,
    priority int not null default 0,
    weight int not null default 1,
    status text,
    orig_run_at bigint not null,
    payload blob,
    checkpointed_payload blob,
    current_try int not null default 0,
    max_retries int not null,
    backoff_multiplier real not null,
    backoff_randomization real not null,
    backoff_initial_interval int not null,
    default_timeout int not null,
    heartbeat_increment int not null,
    ...
);

CREATE TABLE active_jobs (
    job_id INTEGER PRIMARY KEY,
    active_worker_id bigint,
    priority int not null default 0,
    run_at bigint not null,
    started_at bigint,
    expires_at bigint
);
-- Priority-aware index for pending jobs
CREATE INDEX active_run_at ON active_jobs(priority desc, run_at) WHERE active_worker_id is null;
```

**API Style:** Async, Tokio-compatible.
```rust
let queue = Queue::new(&PathBuf::from("effectum.db")).await?;

let a_job = JobRunner::builder("remind_me", remind_me_job).build();

let worker = Worker::builder(&queue, context)
    .max_concurrency(10)
    .jobs([a_job])
    .build();

let job_id = Job::builder("remind_me")
    .run_at(time::OffsetDateTime::now_utc() + Duration::from_secs(3600))
    .json_payload(&payload)?
    .priority(5)
    .add_to(&queue)
    .await?;
```

**Dependencies:** rusqlite 0.31, deadpool-sqlite, tokio, serde, backoff, cron, tracing, uuid v7

**Pros:**
- Purpose-built for SQLite, clean design
- Exponential backoff built in (configurable multiplier, randomization)
- Job checkpointing (unique feature -- resume from last checkpoint on failure)
- Job weight for concurrency budgeting
- Heartbeat-based timeout with extension
- Recurring jobs via cron expressions
- Clean, small API surface
- Uses tracing (same as filament)
- Dual-licensed

**Cons:**
- Uses rusqlite, NOT sqlx -- cannot share filament's connection pool
- Last updated July 2024 (1.5 years stale)
- Only 46 stars, small community
- Uses `deadpool-sqlite` (another connection pool to manage)
- `eyre` for errors (filament uses `thiserror`)
- Uses `time` crate (filament uses `chrono`)
- No worker registration/tracking
- No event-driven mode (polling only)
- No dashboard/monitoring
- Missing: sweeper for completed jobs, outbox pattern

**Gotchas:**
- Would need a separate SQLite database file (can't share filament's DB)
- rusqlite and sqlx SQLite may conflict on bundled SQLite versions
- `deadpool-sqlite` adds another async pool layer alongside sqlx's pool

---

### 3. qoxide

**Version:** 1.3.0
**GitHub:** https://github.com/Haizzz/qoxide
**Stars:** 0 | **Downloads:** 208
**Last Commit:** 2025-12-05
**License:** ?

**Key Features:**
- FIFO queue with SQLite persistence
- WAL mode
- Reserve-complete-fail workflow
- Dead letter queue (after N attempts)
- Queue inspection (sizes)

**Pros:**
- Very simple API
- Dead letter queue
- Max attempts

**Cons:**
- Zero stars, 208 downloads -- effectively unused
- Synchronous API (no async)
- No priorities
- No scheduled execution
- No backoff
- Binary payload only (Vec<u8>)
- No worker management
- Too primitive for production use

**Verdict:** Not viable. Too immature and feature-poor.

---

### 4. aide-de-camp + aide-de-camp-sqlite

**Version:** 0.2.0
**GitHub:** Not found (404)
**Downloads:** 7K (core) + 4.8K (sqlite)
**Last Update:** 2022-12-18 (3+ years ago)

**Key Features:**
- Backend-agnostic delayed job queue
- SQLite and MongoDB backends
- Trait-based (pluggable)

**Cons:**
- Abandoned (3+ years, GitHub 404)
- Pre-1.0
- Minimal downloads

**Verdict:** Dead project. Not viable.

---

### 5. disk_backed_queue

**Version:** 0.1.1
**Downloads:** 43
**Last Update:** 2025-12-01

**Verdict:** Essentially a prototype. 43 downloads, no stars. Not viable.

---

## Tier 2: Non-SQLite (Reference/Comparison)

### 6. sqlxmq

**Version:** 0.6.0
**GitHub:** https://github.com/Diggsey/sqlxmq
**Stars:** 158 | **Downloads:** 161K
**Backend:** PostgreSQL ONLY (uses LISTEN/NOTIFY, SKIP LOCKED)

Well-designed, uses sqlx, but PostgreSQL-specific features (LISTEN/NOTIFY, advisory locks, SKIP LOCKED) that have no SQLite equivalent. Not portable.

### 7. faktory

**Version:** 0.13.1
**GitHub:** https://github.com/jonhoo/faktory-rs
**Stars:** 237 | **Downloads:** 85K
**Backend:** External Faktory server (Go-based)

Client bindings for the Faktory work server. Requires running a separate Go process. Wrong model for filament (embedded, local-only).

### 8. rusty-celery

**Version:** (last release 2024)
**GitHub:** https://github.com/rusty-celery/rusty-celery
**Stars:** 861
**Backend:** Redis or AMQP (RabbitMQ)

Rust implementation of Celery protocol. Requires Redis or RabbitMQ. Wrong backend for filament.

### 9. backie

**Version:** 0.9.0
**Stars:** 47 | **Downloads:** 65K
**Backend:** PostgreSQL (Diesel)
**Status:** ARCHIVED

Was a fork of fang. Now archived. Uses Diesel, not sqlx. PostgreSQL only.

### 10. hammerwork

**Version:** 1.15.5
**Downloads:** 17K
**Backend:** PostgreSQL + MySQL

High version number but relatively low downloads. No SQLite support.

### 11. background-jobs

**Version:** 0.20.0
**Backend:** Sled or PostgreSQL

Pluggable backends but no SQLite. Sled backend exists but sled itself is unmaintained.

---

## Tier 3: Build Custom

### Option: Raw sqlx SQLite implementation

Given filament already has:
- sqlx + SQLite with migrations
- Entity/Relation model in a graph
- Daemon with Unix socket
- Agent dispatch infrastructure

A custom job queue on top of existing infrastructure would look like:

```sql
CREATE TABLE job_queue (
    id TEXT PRIMARY KEY,        -- ULID or UUID
    job_type TEXT NOT NULL,     -- discriminant
    payload TEXT NOT NULL,      -- JSON
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','claimed','running','done','failed','dead')),
    priority INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    run_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    claimed_by TEXT,            -- worker/agent ID
    claimed_at INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at INTEGER,
    last_error TEXT,
    backoff_secs INTEGER NOT NULL DEFAULT 5,
    -- backoff: backoff_secs * 2^attempts
    FOREIGN KEY(claimed_by) REFERENCES ...
);
CREATE INDEX idx_jq_fetch ON job_queue(priority DESC, run_at ASC)
    WHERE status IN ('pending','failed') AND (run_at <= strftime('%s','now'));
```

Atomic claim:
```sql
UPDATE job_queue
SET status = 'claimed', claimed_by = ?1, claimed_at = strftime('%s','now')
WHERE id = (
    SELECT id FROM job_queue
    WHERE status IN ('pending', 'failed') AND attempts < max_attempts
      AND run_at <= strftime('%s','now')
    ORDER BY priority DESC, run_at ASC
    LIMIT 1
) RETURNING *;
```

**Pros:**
- Zero new dependencies
- Shares filament's connection pool
- Full control over schema, fits filament's Entity model
- Can integrate with existing daemon notifications (pub/sub)
- Can use filament's existing error types, tracing, etc.

**Cons:**
- Must implement: retry logic, backoff, timeout, orphan detection, worker heartbeat
- More code to write and maintain
- No community battle-testing
- Estimated effort: 2-4 days for basic queue, 1 week for full features

---

## Feature Comparison Matrix

| Feature | apalis-sqlite | effectum | Custom (sqlx) |
|---------|:------------:|:--------:|:-------------:|
| SQLite backend | YES (sqlx) | YES (rusqlite) | YES (sqlx) |
| Share filament pool | YES | NO | YES |
| Async/Tokio | YES | YES | YES |
| Priorities | YES | YES | YES |
| Scheduled jobs | YES (run_at) | YES (run_at) | Easy |
| Max retries | YES (25 default) | YES | Easy |
| Exponential backoff | NO (immediate) | YES (built-in) | Medium |
| Job timeout | Partial (heartbeat) | YES (expires_at) | Medium |
| Checkpointing | NO | YES | Hard |
| Recurring/cron | YES (apalis-cron) | YES (built-in) | Medium |
| Dead letter queue | NO | NO | Easy |
| Worker tracking | YES | NO | Medium |
| Orphan recovery | YES | YES (on restart) | Medium |
| Event-driven fetch | YES (SQLite hooks) | NO | Medium |
| Middleware/layers | YES (tower-like) | NO | N/A |
| Workflow engine | YES (apalis-workflow) | NO | N/A |
| Dashboard | YES (apalis-board) | NO | N/A |
| Same sqlx version | YES (0.8.6) | N/A (rusqlite) | YES |
| Maintenance | Active (Feb 2026) | Stale (Jul 2024) | Self |
| Community | Large (1100 stars) | Small (46 stars) | None |
| Dep weight | Heavy | Medium | Zero |

---

## Recommendation for Filament

### Primary: apalis + apalis-sqlite

**Why:**
1. Same sqlx version (0.8.6) -- can share connection pool
2. Actively maintained with frequent releases
3. Largest Rust job queue community (1100+ stars)
4. Priorities, retries, scheduling, worker tracking all built in
5. Event-driven mode (SQLite update hooks) for low-latency dispatch
6. Middleware architecture for cross-cutting concerns (tracing, metrics)
7. Workflow engine available if needed later

**Integration approach:**
- Share filament's `SqlitePool` with `SqliteStorage::new(&pool)`
- Run apalis migrations alongside filament migrations
- Use apalis workers inside the daemon process
- Map filament task types to apalis job types

**Risks:**
- RC status (API could change before 1.0)
- No built-in exponential backoff (would need middleware or manual retry scheduling)
- apalis-sqlite has only 3.4K downloads (less battle-tested than redis/postgres)

### Fallback: Custom implementation

**When to choose custom:**
- If apalis RC instability causes problems
- If the dependency weight is unacceptable
- If we need deep integration with filament's Entity model (jobs as entities)
- If backoff/timeout behavior needs to be very specific

**Estimated effort:** 3-5 days for feature parity with what filament needs

### Not recommended: effectum

Despite being well-designed, the rusqlite dependency is a dealbreaker. It would mean:
- Two SQLite libraries in one process (sqlx + rusqlite)
- Two connection pools
- Potential SQLite version conflicts
- Cannot share filament's existing pool

---

## Key Design Questions for Filament

1. **Jobs as Entities?** Should jobs be Entity nodes in the graph, or a separate table?
   - Separate table is simpler and matches all existing crate designs
   - Entity integration adds graph traversal but complicates schema

2. **Backoff strategy?** apalis lacks built-in backoff. Options:
   - Custom middleware that sets `run_at` on retry
   - Manual `run_at = now + base * 2^attempts` in retry handler

3. **Concurrency model?** Single worker with N concurrent jobs, or multiple workers?
   - apalis supports both; filament daemon is single-process, so single worker + concurrency limit

4. **Job types?** What kinds of jobs?
   - Agent dispatch (subprocess spawn)
   - Cleanup/maintenance tasks
   - Notification delivery
   - Scheduled recurring tasks (e.g., stale reservation cleanup)

5. **Shared pool or separate DB?**
   - apalis-sqlite can use filament's pool directly
   - Separate DB file isolates job queue from knowledge graph
