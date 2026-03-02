# ADR-015: Project name "filament"

**Date:** 2026-03-02
**Status:** Accepted

## Context

The project was originally named "loom" but that name is taken on crates.io (Tokio's concurrency testing tool). A new name was needed that reflects the tool's purpose of connecting agents, tasks, and knowledge.

## Decision

Name the project "filament" — connecting agents, tasks, and knowledge like threads/filaments. The name is available on crates.io. Internal crate names: `filament_core`, `filament_cli`, `filament_daemon`, `filament_tui`. Runtime directory: `.filament/`.

## Consequences

- Available on crates.io — no naming conflicts
- Metaphor is clear: filaments connect things, which is what the tool does
- Short enough for CLI usage (`filament task add ...`)
- All internal naming follows the `filament_*` / `filament-*` convention consistently
