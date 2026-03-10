# Filament — Master Plan v1.1

Local-only Rust tool for multi-agent orchestration, knowledge graph, task management, inter-agent communication, and TUI.

**ADRs**: `.plan/adr/` (001–023)

---

## Decisions Log

| Decision            | Choice                              | Rationale                                                                  |
| ------------------- | ----------------------------------- | -------------------------------------------------------------------------- |
| Architecture        | Hybrid daemon                       | Direct SQLite for single-user, daemon for multi-agent                      |
| Communication       | Unix socket + MCP + SQLite          | Socket for internal RPC, MCP for agent tool discovery, SQLite for durable async |
| Data model          | Unified graph                       | Tasks, knowledge, agents, messages = nodes; deps, relations, comms = edges |
| Task management     | Reimplemented as graph ops          | Not a br port — graph-native task features                                 |
| Agent dispatching   | Subprocess only                     | `claude -p` with structured JSON protocol                                  |
| Storage             | Per-project `.fl/` + SQLite (WAL)   | `fl init` in any dir, data local to project                                |
| Error design        | Structured errors (thiserror)       | Machine-readable codes, hints, retryable flags for agent consumers         |
| File coordination   | Advisory reservations with TTL      | No worktrees — leases expire on agent death                                |
| Agent resilience    | Design for death                    | TTL leases, no ringleaders, no single points of failure                    |
| Messaging           | Targeted only (no broadcast)        | Agents must address specific recipients to prevent context pollution        |
| Agent interface     | MCP server on daemon                | Ecosystem standard — agents discover tools via MCP                         |

---

## Phases (all complete)

| Phase | Goal | Completed |
|-------|------|-----------|
| 1 | filament-core: models, errors, schema, store, graph, connection, protocol | 2026-02-27 |
| 2 | CLI: entity, task, relation, query, message, reserve commands | 2026-02-28 |
| 3 | Daemon: Unix socket server, MCP server (16 tools via rmcp) | 2026-03-03 |
| 4 | Agent dispatching: subprocess management, roles, safety | 2026-03-03 |
| 5 | TUI: 6-tab dashboard (entities, agents, reservations, messages, config, analytics) | 2026-03-03 |
| 6 | Integration: context bundles, escalation, export/import | 2026-03-04 |
| 7 | Small features: config, watch, analytics, hooks, seed, audit, completions | 2026-03-07 |

---

## Open Items (future iterations)

- Semantic search with embeddings
- Graph visualization in TUI (ASCII graph rendering)
- Intent detection / fuzzy matching for CLI args
- Agent context budget monitoring — track % used, auto-clear when low
