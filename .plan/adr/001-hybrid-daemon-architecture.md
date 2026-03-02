# ADR-001: Hybrid daemon architecture

**Date:** 2026-03-02
**Status:** Accepted

## Context

Filament needs to support two usage modes: a single developer using the CLI directly, and multiple concurrent agents coordinating through shared state. A pure daemon architecture adds latency and complexity for the single-user case. A pure direct-access architecture can't handle concurrent writes safely.

## Decision

Use a hybrid architecture. The CLI works directly against SQLite in single-user mode (no daemon needed). When `filament serve` starts the daemon, the CLI auto-detects the Unix socket and routes through it for concurrent multi-agent access. The connection type is determined at runtime via a `Connection` enum (`Direct | Socket`).

## Consequences

- Single-user experience has zero overhead — no daemon process, no socket latency
- Multi-agent mode gets proper write serialization through the daemon
- CLI code needs to abstract over both connection types (some duplication in command handling)
- Auto-detection logic adds a small amount of complexity (check for `.filament/filament.sock`)
