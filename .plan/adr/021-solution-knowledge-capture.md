# ADR-021: Solution Knowledge Capture

**Status:** Accepted
**Date:** 2026-03-04
**Inspired by:** [sleeplesslord/runes](https://github.com/sleeplesslord/runes)

## Context

Filament tracks **doing** (tasks, dependencies, agents) but not **knowing** (lessons learned, reusable patterns, solved problems). When an agent solves a tricky issue — auth timeout, SQLite locking, graph cycle — that knowledge evaporates after the session ends. Agents re-discover the same solutions in future sessions.

Runes (a Go CLI by sleeplesslord) demonstrates a focused approach: structured solution records with `problem/solution/pattern/learned` fields, BM25-ranked search, content similarity, and dual-scope storage (global + local). Its key insight is that solutions are a distinct data shape from tasks.

## Decision

Incorporate solution knowledge capture into filament across four work items:

### 1. Lesson Entity Variant

Add a `Lesson` variant to the `Entity` ADT with structured fields:

- `problem` — what was failing (symptoms, error messages)
- `solution` — specific steps taken to fix
- `pattern` — optional reusable pattern name (e.g., "circuit-breaker", "n-plus-one-fix")
- `learned` — key insight for next time

This extends the existing `Task | Module | Service | Agent | Plan | Doc` enum. `summary` and `key_facts` remain available on `EntityCommon` for compatibility.

### 2. Text Search with Ranking

Replace substring matching with relevance-ranked search. Options:

- **SQLite FTS5** — built into SQLite, minimal deps, good enough for our scale
- **In-memory BM25** — like Runes does, with field weights (name 3x, pattern 2.5x, summary 1.5x)

Prefer FTS5 since we already depend on SQLite. Add a `filament search <query>` command.

### 3. Global Knowledge Scope

Add `~/.filament/` as a global store for cross-project knowledge. When querying:

- Local `.filament/` is primary
- Global `~/.filament/` supplements (merged results, lower priority)
- `filament lesson add --global` writes to global scope
- `filament search` queries both by default, `--scope local|global` to filter

### 4. Agent Skill Workflow

Update the filament agent skill with a "search before solving" protocol:

- Before starting a task: `filament search "relevant terms"` to check for existing solutions
- After completing: `filament lesson add "title" --problem "..." --solution "..." --learned "..."`
- This makes the knowledge base self-reinforcing as agents work

## Consequences

- Entity ADT grows from 6 to 7 variants — manageable
- New migration for FTS5 virtual table
- Global scope introduces a second database connection — need clear merge semantics
- Agents become smarter over time as the lesson corpus grows
- Pattern names enable cross-project knowledge transfer via global scope

## Alternatives Considered

- **Separate tool** (like Runes + Saga split) — rejected because filament already has the entity infrastructure, and a separate tool means agents need two MCP connections
- **Freeform notes via existing Doc entity** — rejected because unstructured text loses the problem/solution/learned discipline that makes knowledge retrievable
- **Auto-similarity without explicit search** — deferred to a future iteration; start with explicit search, add suggestions later
