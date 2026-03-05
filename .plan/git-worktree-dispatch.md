# Git Worktree Dispatch Plan

**Epic:** `dsxj4re0` — git-worktree-dispatch
**Status:** Planning (revised 2026-03-05)
**Date:** 2026-03-05

## Problem

When multiple agents are dispatched concurrently, they all work in the same working directory.
This causes conflicts in three categories:

1. **File edits** — two agents modify the same file (advisory reservations help but don't prevent)
2. **Build artifacts** — concurrent builds corrupt shared `target/`, `node_modules/`, etc.
3. **Git state** — agents can't commit to separate branches from the same checkout

Reservations solve (1) for simple edits. Only worktrees solve (2) and (3).

## Solution

Two dispatch modes based on task scope, not project structure:

| Mode | Flag | When to use | Cost |
|------|------|-------------|------|
| **Shared workspace** | (default) | Quick edits, docs, single-file changes, review | Zero |
| **Worktree isolation** | `--worktree` | Features, bug fixes, anything needing build/test/commit | Disk + cold build |

Shared mode uses existing reservations. Worktree mode gives each agent an isolated checkout
on a dedicated branch with full autonomy to build, test, and commit.

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
  8. Update task status -> in_progress
  9. tokio::spawn(monitor_agent)
  10. Return run_id
```

Step 5 changes: if `use_worktree`, subprocess cwd becomes the worktree path.
Steps 9-10 change: monitor_agent handles worktree cleanup on completion.

## Design Decisions

### Worktree location
`.filament/worktrees/{run_id}/` — inside the filament data dir, gitignored, easy to find/clean.

### Branch naming
`filament/{run_id}` — namespaced, unique, no conflicts. Created from HEAD at dispatch time.

### .filament/ access from worktrees
Worktrees are separate checkouts. `.filament/` is gitignored so it does NOT appear in worktrees.
All MCP config and socket paths must use **absolute paths** pointing back to the main repo's
`.filament/` directory. The MCP server connects to the same `filament.db` — agents in worktrees
talk to the same daemon/DB as agents in the main repo.

### Branch preservation on cleanup
Agent work must not be silently destroyed:
- **Successful run with commits**: remove worktree dir, **keep branch** (user merges manually or via future `filament agent merge`)
- **Successful run, no commits**: remove worktree dir, delete branch
- **Failed/timed-out run**: remove worktree dir, delete branch
- Always log the branch tip SHA before any branch deletion

The `agent_runs` row stores `worktree_branch` so the user can find agent branches after cleanup.

### Cleanup safety
`git worktree remove --force` destroys uncommitted changes. Before force-removing:
1. Check `git -C <worktree> status --porcelain` for uncommitted changes
2. If changes exist, log a warning with the worktree path (don't silently destroy)
3. Force-remove anyway (agent is done, changes were not committed = not important enough to save)

### Opt-in vs default
Start as **opt-in**: `filament agent dispatch <task> --worktree`. Default behavior unchanged.
Projects can set `dispatch.use_worktree = true` in `filament.toml` to make it the default.

### Concurrent worktree creation
Git locks `.git/worktrees` during `git worktree add`. Two concurrent dispatches may race.
`create_worktree()` retries once after a short delay on lock failure, then errors.

### Detached HEAD guard
If the main repo is in detached HEAD state (mid-rebase, bisect), `create_worktree()` warns
and creates the branch from the detached commit. This is intentional — agents should work
from whatever state the repo is in.

## Tasks

### Phase 1: Core worktree management (no dispatch integration yet)

1. **[T1] Worktree lifecycle module** — `crates/filament-daemon/src/worktree.rs`
   - `create_worktree(project_root, run_id) -> Result<WorktreeInfo>`
     - `git worktree add .filament/worktrees/{run_id} -b filament/{run_id}`
     - Returns `WorktreeInfo { path, branch }`
     - Retry once on lock failure
   - `remove_worktree(project_root, run_id, preserve_branch: bool) -> Result<CleanupReport>`
     - Check for uncommitted changes (log warning if found)
     - `git worktree remove .filament/worktrees/{run_id} --force`
     - If `!preserve_branch`: `git branch -D filament/{run_id}` (log tip SHA first)
     - Returns `CleanupReport { had_uncommitted_changes, branch_preserved, tip_sha }`
   - `has_commits_ahead(project_root, branch) -> Result<bool>`
     - `git log HEAD..filament/{run_id} --oneline` — any output = commits exist
   - `cleanup_stale_worktrees(project_root, active_run_ids: &[String]) -> Result<Vec<String>>`
     - `git worktree list --porcelain` -> find `filament/*` worktrees not in active list
     - Remove each, preserve branches with commits, delete empty branches
   - Tests: create, remove, cleanup stale, idempotent remove, branch preservation, uncommitted changes warning
   - **Deps:** none
   - **Files:** `crates/filament-daemon/src/worktree.rs`, `crates/filament-daemon/src/lib.rs`

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
     - Store worktree info in DispatchContext (passed to monitor)
   - Migration 009: add `worktree_path` and `worktree_branch` columns to `agent_runs` (nullable TEXT)
   - **Deps:** T1, T2
   - **Files:** `crates/filament-daemon/src/dispatch.rs`, `crates/filament-core/src/store.rs`, `crates/filament-core/src/models.rs`, `migrations/0009_agent_run_worktree.sql`

4. **[T4] Worktree cleanup in monitor_agent()**
   - After `route_result()` or `finish_run_failed()`, if worktree path is set:
     1. Check `has_commits_ahead()` for the branch
     2. Call `remove_worktree(preserve_branch: has_commits)`
     3. Update `agent_runs` row with `CleanupReport` info (tip SHA, branch preserved)
   - Handle cleanup failure gracefully (log warning, don't fail result routing)
   - **Deps:** T3
   - **Files:** `crates/filament-daemon/src/dispatch.rs`

### Phase 3: Robustness

5. **[T5] Stale worktree cleanup on daemon startup**
   - In `serve_with_dispatch()`, after `reconcile_stale_agent_runs()`:
     - Query `agent_runs` for active run_ids
     - Call `cleanup_stale_worktrees(project_root, &active_run_ids)`
   - **Deps:** T3 (needs worktree_path column, not T4)
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
   - Document two dispatch modes and when to use each
   - **Deps:** T7

## Dependency Graph

```
T1 (worktree module) --+
                       +-- T3 (dispatch integration) -- T4 (cleanup in monitor)
T2 (CLI + protocol) ---+                            +-- T5 (startup cleanup)
                                                     +-- T6 (dispatch-all)
                                                     +-- T7 (config) -- T8 (docs)
```

Note: T5 depends on T3 (not T4) — startup cleanup only needs the DB column, not monitor cleanup.
T4, T5, T6, T7 can all proceed in parallel after T3.

## Risk / Open Questions

### Disk usage (large repos)
Each worktree gets its own build artifacts (`target/`, `node_modules/`, etc.).
For a large Rust project this can be 10-20GB per worktree.
- Mitigations: recommend `sccache` + shared `CARGO_TARGET_DIR` in docs (not enforced by filament)
- Future: `--max-worktrees N` guard to prevent accidental disk exhaustion
- First-time `--worktree` use could log a one-time advisory about disk cost

### Build cache (out of scope for v1)
Cold builds in worktrees are slow. Possible future optimizations:
- Shared `CARGO_TARGET_DIR` with `sccache` (Rust)
- Symlinked `node_modules/` (JS — risky with concurrent installs)
- These are project-specific and should be documented recommendations, not filament features

### Uncommitted changes in main repo
`git worktree add` creates from HEAD. If the main repo has uncommitted changes, the worktree
won't have them. This is desirable — agents work from clean committed state.

### Merge conflicts (out of scope for v1)
Agents work on branches; the human merges. Could add `filament agent merge <run_id>` later.
For now, `git merge filament/{run_id}` works manually.

### Non-git projects
Worktree dispatch requires a git repo. For non-git projects, `--worktree` should error
with a clear message: "Worktree dispatch requires a git repository."
