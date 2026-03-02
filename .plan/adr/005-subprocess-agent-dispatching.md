# ADR-005: Subprocess-only agent dispatching

**Date:** 2026-03-02
**Status:** Accepted

## Context

Filament needs to dispatch work to AI agents. Options include: API calls to LLM providers directly, spawning Claude Code subprocesses, or managing long-running agent processes.

## Decision

Agents are spawned as subprocesses using `claude -p` with structured JSON output. Each agent invocation is a single subprocess that runs to completion and returns an `AgentResult` JSON payload containing: status, artifacts produced, messages to other agents, blockers encountered, and questions for humans.

Agents discover filament's capabilities via MCP tools exposed by the daemon.

## Consequences

- Simple process model — spawn, wait, parse result. No long-lived connections to manage
- Agents can use any Claude Code capabilities (file editing, shell, MCP tools)
- `AgentResult` protocol gives structured feedback vs Flywheel's "observe via heartbeat/tmux" approach
- Each subprocess is isolated — crashes don't affect other agents or the daemon
- Higher overhead per task than long-lived agents (process startup cost)
- Subprocess approach limits agent-to-agent real-time collaboration (must go through filament messaging)
