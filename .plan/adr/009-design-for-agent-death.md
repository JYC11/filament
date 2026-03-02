# ADR-009: Design for agent death

**Date:** 2026-03-02
**Status:** Accepted

## Context

The Flywheel ecosystem's core design principle: agents die constantly. Context overflow, crashes, memory wipes, and timeouts are normal operating conditions, not edge cases. Any design that assumes agents are long-lived or reliable will fail at scale.

## Decision

Design every subsystem assuming agents will die mid-operation:

- **TTL leases** — all reservations, assignments, and registrations expire automatically
- **No ringleader agents** — no single agent is critical to system operation
- **Auto-cleanup** — daemon detects dead agents (PID check / heartbeat timeout) and reclaims resources
- **Idempotent operations** — agents can restart and re-execute without side effects
- **State in filament, not in agents** — all coordination state lives in SQLite, not in agent memory

## Consequences

- System remains functional even when agents crash frequently
- No manual intervention needed to recover from agent failures
- TTL tuning becomes important — too short causes spurious expirations, too long delays recovery
- Operations must be designed to be resumable / idempotent (more careful implementation)
- No complex multi-step agent protocols that require all steps to complete atomically
