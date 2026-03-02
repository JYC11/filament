# ADR-008: Advisory file reservations with TTL, no worktrees

**Date:** 2026-03-02
**Status:** Accepted

## Context

Multiple agents editing the same files causes conflicts. Two approaches exist in the ecosystem:

1. **Git worktrees** — each agent gets an isolated copy of the repo
2. **Advisory file reservations** — agents claim file globs with TTL leases

The Flywheel ecosystem explicitly rejects worktrees:
> "Worktrees demolish development velocity and create debt you need to pay later when the agents diverge."

## Decision

Use advisory file reservations with TTL. Agents acquire leases on file globs (e.g., `src/store/*.rs`) before modifying them. Leases have a configurable TTL and expire automatically if the agent crashes. A pre-commit guard can block commits touching reserved files at the boundary.

No worktrees. All agents work in the same working directory.

## Consequences

- No merge debt — agents never diverge from each other
- Crashes are safe — TTL leases expire, no stuck locks
- Conflicts surface early through communication rather than late at merge time
- Agents must check reservations before starting work (adds a protocol step)
- Advisory means not enforced at the filesystem level — agents can violate reservations if they ignore the protocol
- Pre-commit guard provides enforcement at the commit boundary (hard stop, not just advisory)
