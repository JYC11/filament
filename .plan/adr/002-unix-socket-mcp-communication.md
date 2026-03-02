# ADR-002: Unix socket + MCP server + SQLite communication

**Date:** 2026-03-02
**Status:** Accepted

## Context

Agents need to call filament operations. The Flywheel ecosystem uses HTTP-based MCP servers as the standard interface for agent tool discovery and invocation. Internal CLI-to-daemon communication needs something lower-latency than HTTP.

## Decision

Three communication channels:
1. **Unix socket** — internal CLI-to-daemon RPC (JSON-RPC over socket, low latency)
2. **MCP server** — agent-facing tool interface (agents discover and call filament tools via standard MCP protocol)
3. **SQLite messages table** — durable async messaging between agents (targeted only, stored persistently)

## Consequences

- Agents use the ecosystem-standard MCP protocol — no custom client needed
- CLI-to-daemon communication is fast (no HTTP overhead, no port allocation)
- Messages survive agent crashes (persisted in SQLite, not just in-memory)
- Three communication paths means three things to maintain and test
- Unix sockets are not available on Windows (acceptable — local-only tool, macOS/Linux focus)
