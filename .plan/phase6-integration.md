# Phase 6: Integration + Polish

**Goal**: glue everything together — context generation, escalation, export/import, documentation.

**Master plan**: [filament-v1.md](filament-v1.md)
**Depends on**: All previous phases

---

## 6.1 — Context bundles for agents

- Before dispatching an agent, auto-generate context:
  - `filament context --around <task> --depth 2` output
  - Upstream dependency artifacts
  - Relevant CLAUDE.md content
  - Critical path from this task to project completion
- Injected into the agent's prompt
- Blocked by: 4.1, 1.6

## 6.2 — Escalation routing

- Level 0: agent handles internally (compile error → fix)
- Level 1: agent-to-agent via targeted messages
- Level 2: orchestrator routes (trigger review, unblock next)
- Level 3: escalate to user (TUI notification or terminal output)
- Blocked by: 4.4, 5.1

## 6.3 — Export / Import

- `filament export` → full graph as JSON (entities + relations + events)
- `filament import < graph.json` → bulk upsert
- Blocked by: 1.5

## 6.4 — Documentation

- `filament help <command>` — built into clap
- README.md for the filament/ directory
- Blocked by: all prior phases

---

## Task Dependency Graph

```
6.1 (context bundles) — needs Phase 4 + 1.6
6.2 (escalation)      — needs Phase 4 + Phase 5
6.3 (export/import)   — needs Phase 1
6.4 (docs)            — needs everything
```
