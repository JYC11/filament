# Build Serialization for Multi-Agent Coordination

> **SUPERSEDED (2026-03-23):** Claude Code's built-in `isolation: "worktree"` on the Agent tool
> gives each subagent its own repo copy, eliminating shared `target/` contention entirely.
> Worktree guidance added to the filament skill instead. This plan is kept for reference only.

## Problem

When running multiple Claude agents in tmux, concurrent `make fmt/build/check/test` causes:

1. **Concurrent `cargo fmt`** → source file race conditions (REAL corruption risk)
2. **`cargo fmt` during build/check** → build reads inconsistent source (transient errors)
3. **Concurrent builds** → cargo's internal file lock serializes them (safe but slow)

Only #1 is a true correctness bug. #2 causes transient build failures (agent retries). #3 is just a performance issue cargo already handles.

## Approach

Add a portable read/write file lock to util-scripts. Transparent to agents — they use `make` as before, locking happens inside the scripts.

## Design Decisions

### Lock semantics: shared/exclusive (read/write lock)

- `fmt.sh` → **exclusive** lock (it modifies source files)
- `build.sh`, `check.sh`, `test.sh` → **shared** lock (they read source; cargo handles target dir serialization internally)

This means:
- Multiple builds/checks/tests can run concurrently (cargo serializes compilation internally, test execution runs in parallel)
- `cargo fmt` waits for all active builds to finish, and blocks new builds while formatting
- Two `cargo fmt` processes never run simultaneously

The shared/exclusive distinction matters: Agent A running `make test` (5 min) shouldn't block Agent B from running `make build` (30 sec). Without shared locks, Agent B waits 5 minutes for no reason.

### Portability: Perl-based flock shim

macOS doesn't ship the `flock` CLI (it's a util-linux tool). Options considered:

| Option | Pros | Cons |
|--------|------|------|
| `brew install flock` | Simple | External dependency |
| `mkdir` + PID file | Portable, no deps | No shared/exclusive support |
| Perl `Fcntl::flock` | Ships with macOS, supports shared/exclusive | Perl dependency (but always present) |
| Try `flock` CLI, fall back to `mkdir` | Best of both worlds | Complex, two code paths |

**Decision**: Perl `Fcntl::flock`. Perl ships with macOS and Linux. Gives us real shared/exclusive kernel-level locks with automatic cleanup on process death (no stale locks). One code path, no fallback complexity.

### Lock file location

`/tmp/filament-build.lock` — project-specific enough. Multiple filament checkouts would share the lock, which is actually desirable (they share the same cargo target dir by default).

### Stale lock handling

Not needed — kernel `flock(2)` releases automatically when the process dies (lock is on the file descriptor, not the file). No PID files, no cleanup scripts.

### Agent transparency

Agents don't need to know about locking. No changes to agent prompts or preamble needed for the mechanism itself. Just a note in the preamble that builds serialize automatically.

## Implementation

### Task 1: Create `util-scripts/buildlock.sh` — portable lock helper

Source-able helper that provides `acquire_build_lock` function.

```bash
#!/bin/bash
# Build lock for multi-agent coordination.
# Uses flock(2) via Perl for portable shared/exclusive file locking.
#
# Usage (source this, then call):
#   source util-scripts/buildlock.sh
#   acquire_build_lock --shared    # for build/check/test
#   acquire_build_lock --exclusive  # for fmt
#
# Lock is held until the process exits (fd-based, auto-released on death).

BUILDLOCK_FILE="/tmp/filament-build.lock"
BUILDLOCK_FD=9

acquire_build_lock() {
    local mode="${1:---shared}"
    local flock_flag="LOCK_SH"  # shared by default
    if [ "$mode" == "--exclusive" ]; then
        flock_flag="LOCK_EX"
    fi

    # Open lock file on fd 9
    eval "exec ${BUILDLOCK_FD}>${BUILDLOCK_FILE}"

    # Acquire lock via Perl (portable flock(2) syscall)
    if ! perl -e "
        use Fcntl qw(:flock);
        open(my \$fh, '>&=', ${BUILDLOCK_FD}) or die 'cannot dup fd: \$!';
        flock(\$fh, ${flock_flag}) or die 'flock failed: \$!';
    "; then
        echo "ERROR: Failed to acquire build lock" >&2
        return 1
    fi

    # The lock is now held on fd 9 and will be released when the
    # process exits (or when fd 9 is explicitly closed).
}
```

**Key detail**: The file descriptor stays open in the parent shell process, so the lock persists for the entire script execution. When the script exits, the fd closes and the lock releases.

### Task 2: Update util-scripts to acquire locks

Each script gets two lines added near the top (after argument parsing, before the cargo loop):

**`fmt.sh`** — exclusive lock:
```bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/buildlock.sh"
acquire_build_lock --exclusive
```

**`build.sh`**, **`check.sh`**, **`test.sh`** — shared lock:
```bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/buildlock.sh"
acquire_build_lock --shared
```

### Task 3: Update agent-preamble.md

Add a short note under "Common Mistakes to Avoid":

```
- Build commands (`make build/check/test/fmt`) automatically serialize via file locks.
  Multiple agents can build/check/test concurrently, but `make fmt` waits for exclusive access.
  No manual coordination needed.
```

### Task 4: Manual verification

Test in two terminal windows:
1. Terminal 1: `make test CRATE=all` (holds shared lock ~30s)
2. Terminal 2: `make fmt CRATE=all` (should block until tests finish)
3. Verify fmt starts only after tests complete
4. Reverse: fmt first, then test — test should wait for fmt

Also verify: two concurrent `make build CRATE=all` both proceed (shared locks don't block each other).

### Task 5: Capture lesson

```bash
fl lesson add "Build serialization for multi-agent tmux" \
  --problem "Multiple Claude agents running make build/check/test/fmt concurrently causes source file races, especially with cargo fmt" \
  --solution "Added flock(2)-based shared/exclusive locking to util-scripts via Perl shim. fmt takes exclusive lock, build/check/test take shared locks. Transparent to agents." \
  --learned "macOS lacks flock CLI but Perl Fcntl provides portable flock(2) access. Shared/exclusive locks matter — simple mutex would unnecessarily serialize independent builds." \
  --pattern "multi-agent-build-coordination"
```

## Task Summary

| # | Task | Scope | Depends on |
|---|------|-------|-----------|
| 1 | Create `util-scripts/buildlock.sh` | New file | — |
| 2 | Add lock calls to `fmt.sh`, `build.sh`, `check.sh`, `test.sh` | 4 file edits | 1 |
| 3 | Update `agent-preamble.md` | 1 file edit | — |
| 4 | Manual verification (two-terminal test) | Testing | 2 |
| 5 | Capture lesson entity | Documentation | 4 |

## Risks & Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|-----------|
| Perl not available on some system | Very low (ships with macOS + most Linux) | Document in README; could add `mkdir` fallback later |
| fd 9 conflicts with something else | Very low | Use a high fd number (200+) if needed |
| Lock file on /tmp cleared by OS | Low (only on reboot) | Lock is recreated on next `exec 9>` — no issue |
| Performance: agents wait too long | Medium | Shared locks minimize this; upgrade to worktrees if it becomes a bottleneck |
| `make ci` runs fmt then check then test sequentially | None (each acquires/releases its own lock per invocation) | Already correct — fmt exclusive, then check shared, etc. |
