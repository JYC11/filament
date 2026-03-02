# ADR-004: Graph-native task management

**Date:** 2026-03-02
**Status:** Accepted

## Context

beads_rust implements task management as flat records with dependency tracking via a separate SQL table and hand-rolled cycle detection. Filament's unified graph model means tasks are already graph nodes. The question is whether to port beads_rust's task features or reimplement them as graph operations.

## Decision

Reimplement task management as graph operations, not a beads_rust port. Tasks are entities, dependencies are relations, and task intelligence (critical path, impact scoring, ready-to-work detection) comes from graph traversal via petgraph rather than SQL queries.

## Consequences

- Task operations benefit from the full graph — "what knowledge entities relate to this task?" is a single traversal
- Critical path analysis, cycle detection, and topological sorting use petgraph's algorithms instead of hand-rolled SQL
- No compatibility with beads_rust's data format (intentional — different model)
- Task features may take longer to reach parity with beads_rust's mature implementation
- beads_rust's blocked cache pattern is unnecessary — petgraph can compute blocked status on demand
