# ADR-016: Start TUI minimal, build up

**Date:** 2026-03-02
**Status:** Accepted

## Context

TUI features are unbounded — graph visualization, real-time agent monitoring, interactive task management, log streaming, etc. Building all of this upfront delays the core value (CLI + daemon + agents).

## Decision

Start the TUI with three views only:
1. **Task list** — filterable task view with status, assignments, dependencies
2. **Agent status** — active agents, their current tasks, context usage, health
3. **Reservations** — active file reservations, TTLs, holders

Build additional views (graph visualization, message streams, log tailing) in later iterations based on actual usage.

## Consequences

- TUI ships earlier — usable for basic monitoring while CLI handles all operations
- Keeps Phase 5 scope manageable (5 tasks vs potentially 15+)
- Users who need more can use the CLI or direct SQLite queries
- View framework (ratatui tabs/panels) should be designed for extensibility from the start
- Risk: minimal TUI may feel underwhelming compared to the CLI's capabilities
