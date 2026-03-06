# Plan: Daemon Auto-Start + Blocker Depth

## Task 1: Auto-start daemon + idle TTL

### Changes
1. **`ServeConfig`** (`filament-daemon/src/config.rs`): Add `idle_timeout_secs: Option<u64>` field, default 1800 (30 min). Resolve from `filament.toml` `idle_timeout_secs` key and `FILAMENT_IDLE_TIMEOUT` env var.
2. **`FilamentConfig`** (`filament-core/src/config.rs`): Add `idle_timeout_secs: Option<u64>` + `resolve_idle_timeout_secs()`.
3. **Daemon idle timer** (`filament-daemon/src/lib.rs`): Track last activity timestamp in `SharedState` (AtomicU64 or similar). In the accept loop, check idle timeout alongside cancel token. On timeout, cancel and shutdown.
4. **`SharedState`** (`filament-daemon/src/state.rs`): Add `last_activity: AtomicI64` (epoch secs). Add `touch()` method called on each request. Add `idle_since()` for the timer to check.
5. **`server.rs`**: Call `state.touch()` on each incoming request.
6. **Auto-start** (`filament-core/src/connection.rs`): In `auto_detect`, if socket doesn't exist/connect, spawn `filament serve` as background process, wait briefly for socket, retry connect. Fall back to Direct if spawn fails.
7. **TUI config tab**: Show `idle_timeout_secs` in config display.

### Edge cases
- Auto-start must not loop (try once, fall back to Direct)
- Daemon already running but socket stale → clean up and restart
- Idle timer must reset on every client connection, not just request

### Risk: low
- Auto-start is just spawning the existing `serve` command
- Idle timer is a simple check in the existing select loop

---

## Task 2: Replace critical_path with blocker_depth

### What changes
Replace `critical_path(entity_id) -> Vec<EntityId>` (DFS, exponential worst case) with `blocker_depth(entity_id) -> usize` (BFS, O(N+E)).

### Algorithm
BFS from entity, following upstream edges (outgoing `DependsOn` + incoming `Blocks`), only through non-Closed nodes. Return the max depth reached. This tells you "how many layers of blockers must clear before this task is unblocked."

### Files to change

**filament-core:**
- `graph.rs`: Replace `critical_path()` + `dfs_longest_path()` with `blocker_depth()` (BFS). Replace `critical_path_names()` with depth in `ContextBundle`. Update `ContextBundle` struct: `critical_path: Vec<String>` → `blocker_depth: usize`.
- `connection.rs`: Replace `critical_path()` method with `blocker_depth()`, Direct path still hydrates (for now).
- `client.rs`: Replace `critical_path()` with `blocker_depth()`.

**filament-daemon:**
- `handler/graph.rs`: Replace `critical_path` handler with `blocker_depth`.
- `handler/mod.rs`: Update method dispatch.

**filament-cli:**
- `commands/task.rs`: Replace `critical-path` subcommand with `blocker-depth` (or keep name, change output).

**filament-tui:**
- `views/detail.rs`: Replace "Critical Path" section with "Blocker Depth: N". Remove `critical_path` from `DetailData`, add `blocker_depth: usize`. Remove `name_map` entries for critical path IDs.
- `app.rs`: Call `blocker_depth()` instead of `critical_path()`.

**Tests (update, don't weaken):**
- `filament-core/tests/graph.rs`: ~7 critical_path tests → blocker_depth tests (same setups, assert depth numbers).
- `filament-cli/tests/task.rs`: 3 critical_path CLI tests → blocker_depth.
- `filament-daemon/tests/daemon.rs`: 2 critical_path tests → blocker_depth.

### Protocol change
- Method name: `CriticalPath` → `BlockerDepth` in protocol enum
- Response: `Vec<EntityId>` → `usize`

---

## Execution order
1. Task 2 first (smaller blast radius, self-contained in graph logic)
2. Task 1 second (touches connection layer, daemon config)
3. Run full test suite after each
