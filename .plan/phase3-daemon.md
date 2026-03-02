# Phase 3: Daemon (filament-daemon)

**Goal**: daemon process that serves CLI requests over Unix socket + MCP, enables multi-agent access.

**Master plan**: [filament-v1.md](filament-v1.md)
**Depends on**: [Phase 1](phase1-core.md)

---

## 3.1 — Daemon binary setup

- File: `filament-daemon/src/main.rs`
- `filament serve [--foreground] [--socket-path <path>] [--mcp-port 8765]`
- Creates `.filament/filament.sock`, writes `.filament/filament.pid`
- Graceful shutdown on SIGTERM/SIGINT
- Hydrates petgraph from SQLite on startup
- Periodic stale reservation cleanup (every 60s)
- Blocked by: 1.5, 1.6, 1.8

## 3.2 — Socket server

- File: `filament-daemon/src/server.rs`
- `tokio::net::UnixListener` on `.filament/filament.sock`
- Per-connection task: read newline-delimited JSON requests, dispatch to FilamentStore, write responses
- `FilamentStore` behind `Arc<FilamentStore>` (store internally uses `RwLock` on the graph)
- Blocked by: 1.5, 1.6, 1.8, 3.1

## 3.3 — MCP server

- File: `filament-daemon/src/mcp.rs`
- HTTP MCP server (localhost only) exposing filament operations as MCP tools
- Tools exposed:
  - `filament_task_ready` — get ranked actionable tasks
  - `filament_task_close` — mark task complete
  - `filament_context` — get graph context around a node
  - `filament_message_send` — send targeted message
  - `filament_message_inbox` — check inbox
  - `filament_reserve` — acquire file reservation
  - `filament_release` — release file reservation
  - `filament_reservations` — check active reservations
- Each tool has JSON Schema input/output definitions (from schemars derives on models)
- This is how agents discover and call filament — standard MCP protocol
- Blocked by: 1.5, 1.7, 1.8, 3.1

## 3.4 — CLI socket client path

- Update `filament-core/src/connection.rs`: `Socket` variant sends JSON-RPC over UnixStream
- CLI transparently switches to socket mode when daemon is running
- Blocked by: 1.7, 3.2

## 3.5 — Daemon auto-start

- CLI detects missing socket → optionally starts daemon in background
- `filament serve --background` daemonizes (or just `&` + disown)
- Blocked by: 3.1, 3.4

## 3.6 — Tests for Phase 3

- Start daemon in test, connect via socket, run operations, verify responses
- Concurrent reader test: multiple clients querying simultaneously
- Write serialization test: two clients writing, verify no corruption
- MCP tool test: call each exposed tool, verify response schema
- Stale reservation cleanup test: create expired reservation, verify auto-cleaned
- Blocked by: 3.2, 3.3, 3.4

---

## Task Dependency Graph

```
3.1 (daemon setup)
 ├──→ 3.2 (socket server) ──→ 3.4 (CLI client)
 └──→ 3.3 (MCP server)

3.1, 3.4 ──→ 3.5 (auto-start)
3.2, 3.3, 3.4 ──→ 3.6 (tests)
```
