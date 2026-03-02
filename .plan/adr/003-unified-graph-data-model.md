# ADR-003: Unified graph data model

**Date:** 2026-03-02
**Status:** Accepted

## Context

Filament handles tasks, knowledge, agents, messages, files, and their relationships. Traditional designs use separate tables/schemas per domain (issues table, knowledge base, agent registry). beads_rust uses a flat issues table. The Flywheel ecosystem uses separate tools per concern (beads for tasks, agent_mail for messages, meta_skill for knowledge).

## Decision

Everything is a node (`Entity`) or an edge (`Relation`). Tasks, modules, agents, plans, docs, files — all entity types with a `kind` discriminator. Dependencies, ownership, artifact production, communication links — all relation types. The graph is stored in SQLite and loaded into petgraph for in-memory traversal.

### Three-tier content model
- **summary** — cheap traversal, always loaded (stored inline)
- **key_facts** — LLM reasoning context (JSON array, stored inline)
- **content_path** — full reference material (path to file on disk)

## Consequences

- Cross-domain queries become graph traversals ("what tasks touch this file?" "what does this agent know?")
- Graph intelligence (critical path, impact scoring, PageRank) works across all entity types, not just tasks — validates the approach used by beads_viewer
- Single schema for all data — simpler migrations and storage layer
- Requires discipline to model everything as entities/relations (some things may feel forced)
- petgraph in-memory copy enables fast traversal but must stay in sync with SQLite
- Three-tier content keeps the graph lightweight while allowing deep dives
