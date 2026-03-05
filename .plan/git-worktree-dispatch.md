# Git Worktree Dispatch Plan

**Epic:** `dsxj4re0` — git-worktree-dispatch
**Status:** Planning
**Date:** 2026-03-05

## Problem

When multiple agents are dispatched concurrently, they all work in the same working directory.
This causes file conflicts even with advisory reservations — agents can still `git checkout`,
run builds, or modify shared files (Cargo.lock, target/, etc.) and step on each other.

## Solution

Use `git worktree` to give each dispatched agent an isolated copy of the repo. Each agent
gets its own checkout on a dedicated branch, works independently, and the results are merged
back when the agent completes.

## Current Dispatch Flow (what changes)

```
dispatch_agent()
  1. Resolve task
  2. Check task status + no running agent
  3. Build MCP config (.filament/mcp-{run_id}.json)
  4. Build system prompt with context bundle
  5. Spawn: Command::new("claude").arg("-p")... [cwd = project_root]  <-- CHANGES
  6. ChildGuard wraps subprocess
  7. Transaction: create agent_run record
  8. Update task status → in_progress
  9. tokio::spawn(monitor_agent)
  10. Return run_id
```

Step 5 changes: subprocess cwd becomes the worktree path instead of project_root.
Steps 9-10 change: monitor_agent handles worktree cleanup on completion.

## Design Decisions

### Worktree location
`.filament/worktrees/{run_id}/` — inside the filament data dir, gitignored, easy to find/clean.

### Branch naming
`filament/{run_id}` — namespaced, unique, no conflicts. Created from HEAD at dispatch time.

### When to create/destroy
- **Create**: In `dispatch_agent()`, after pre-flight checks pass, before subprocess spawn.
- **Destroy**: In `monitor_agent()`, after result routing completes (success or failure).
- **Orphan cleanup**: In `reconcile_stale_agent_runs()` on daemon startup.

### MCP config path
The MCP config currently lives at `.filament/mcp-{run_id}.json` in the main repo.
The worktree shares `.filament/` (it's gitignored), so the MCP config path stays the same.
The MCP server connects to the same `.filament/filament.db` — no change needed.

### What about .filament/ itself?
Worktrees are separate checkouts but share the same `.git` dir. `.filament/` is gitignored
so it won't appear in worktrees. The MCP server path in the config will point to the
main repo's `.filament/` directory (absolute path), so agents in worktrees still talk to
the same daemon/DB.

### Opt-in vs default
Start as **opt-in**: `filament agent dispatch <task> --worktree`. Default behavior unchanged.
Can become default later once proven stable.

## Tasks

### Phase 1: Core worktree management (no dispatch integration yet)

1. **[T1] Worktree lifecycle module** — `crates/filament-daemon/src/worktree.rs`
   - `create_worktree(project_root, run_id) -> Result<PathBuf>`
     - `git worktree add .filament/worktrees/{run_id} -b filament/{run_id}`
   - `remove_worktree(project_root, run_id) -> Result<()>`
     - `git worktree remove .filament/worktrees/{run_id} --force`
     - `git branch -D filament/{run_id}`
   - `cleanup_stale_worktrees(project_root) -> Result<Vec<String>>`
     - `git worktree list --porcelain` → find filament/* worktrees with no matching running agent
     - Remove each + delete branch
   - Tests: create, remove, cleanup stale, idempotent remove
   - **Deps:** none
   - **Files:** `crates/filament-daemon/src/worktree.rs`, `crates/filament-daemon/src/lib.rs` (mod declaration)

### Phase 2: Dispatch integration

2. **[T2] Add --worktree flag to CLI + protocol**
   - CLI: `filament agent dispatch <task> [--worktree] [--role ROLE]`
   - Protocol: add `use_worktree: bool` to DispatchAgent request params
   - Connection/Client: thread the flag through
   - **Deps:** none (parallel with T1)
   - **Files:** `crates/filament-cli/src/commands/agent.rs`, `crates/filament-core/src/protocol.rs`, `crates/filament-core/src/connection.rs`, `crates/filament-core/src/client.rs`

3. **[T3] Integrate worktree into dispatch_agent()**
   - If `use_worktree`:
     - After pre-flight checks, call `create_worktree()`
     - Set subprocess cwd to worktree path
     - Store worktree path in DispatchContext (passed to monitor)
   - Add `worktree_path` column to `agent_runs` table (nullable TEXT) — migration 009
   - **Deps:** T1, T2
   - **Files:** `crates/filament-daemon/src/dispatch.rs`, `crates/filament-core/src/store.rs`, `crates/filament-core/src/models.rs`, `migrations/0009_agent_run_worktree.sql`

4. **[T4] Worktree cleanup in monitor_agent()**
   - After `route_result()` or `finish_run_failed()`, call `remove_worktree()` if path is set
   - Handle cleanup failure gracefully (log warning, don't fail the result routing)
   - **Deps:** T3
   - **Files:** `crates/filament-daemon/src/dispatch.rs`

### Phase 3: Robustness

5. **[T5] Stale worktree cleanup on daemon startup**
   - In `serve_with_dispatch()`, after `reconcile_stale_agent_runs()`:
     - Call `cleanup_stale_worktrees()`
     - Cross-reference with `agent_runs` table to find orphaned worktrees
   - **Deps:** T4
   - **Files:** `crates/filament-daemon/src/lib.rs`

6. **[T6] dispatch-all --worktree support**
   - Thread `use_worktree` flag through `dispatch_all` flow
   - `filament agent dispatch-all [--worktree] [--max-parallel N]`
   - Auto-dispatch also respects the worktree setting (from config or flag)
   - **Deps:** T3
   - **Files:** `crates/filament-cli/src/commands/agent.rs`, `crates/filament-daemon/src/dispatch.rs`

### Phase 4: Config + docs

7. **[T7] Config file support**
   - Add `dispatch.use_worktree = true/false` to `filament.toml` schema
   - Env var: `FILAMENT_USE_WORKTREE=1`
   - Resolution: CLI flag > env var > config > default (false)
   - **Deps:** T3
   - **Files:** `crates/filament-core/src/config.rs`, `crates/filament-daemon/src/state.rs`

8. **[T8] Update skill + CLAUDE.md**
   - Add worktree dispatch to filament skill
   - Update CLAUDE.md with new flag/config
   - **Deps:** T7

## Dependency Graph

```
T1 (worktree module) ──┐
                       ├── T3 (dispatch integration) ── T4 (cleanup in monitor) ── T5 (startup cleanup)
T2 (CLI + protocol) ───┘                            └── T6 (dispatch-all)
                                                     └── T7 (config) ── T8 (docs)
```

## Risk / Open Questions

- **Large repos**: `git worktree add` is fast (no clone), but disk usage doubles per agent. Acceptable for typical projects.
- **Build cache**: Each worktree has its own `target/` — first build in a worktree is cold. Could symlink `target/` but that risks build conflicts. Start without symlink, optimize later if needed.
- **Uncommitted changes**: `git worktree add` creates from HEAD. If the main repo has uncommitted changes, the worktree won't have them. This is actually desirable — agents work from clean state.
- **Merge conflicts**: Out of scope for v1. Agents work on branches; human merges. Could add `filament agent merge <run_id>` later.
