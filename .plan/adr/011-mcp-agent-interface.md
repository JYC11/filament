# ADR-011: MCP server as the standard agent interface

**Date:** 2026-03-02
**Status:** Accepted

## Context

Every tool in the Flywheel ecosystem exposes MCP tools. This is how agents discover and call infrastructure — it's the ecosystem standard. A custom JSON-RPC protocol would work but requires every agent to learn a filament-specific API.

## Decision

The daemon exposes filament operations as MCP tools. Agents discover available operations (add entity, query graph, send message, acquire reservation, etc.) through the standard MCP protocol. The daemon acts as an MCP server that agents connect to.

Internal CLI-to-daemon communication still uses the Unix socket with JSON-RPC for performance. MCP is the external-facing agent interface.

## Consequences

- Agents need zero filament-specific code — they use standard MCP tool calling
- New filament features are automatically discoverable by agents
- MCP protocol adds some overhead vs raw JSON-RPC (but agents aren't latency-sensitive)
- Must maintain MCP tool definitions (`schemars` JSON Schema derives on all input/output types)
- Two protocol layers to maintain: internal JSON-RPC + external MCP
