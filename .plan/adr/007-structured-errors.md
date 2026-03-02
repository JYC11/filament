# ADR-007: Structured errors with machine-readable codes

**Date:** 2026-03-02
**Status:** Accepted

## Context

Filament's errors are consumed by both humans (CLI) and AI agents (MCP tools, subprocess callers). beads_rust demonstrates a strong pattern: every error variant has a machine-readable code (`ISSUE_NOT_FOUND`, `CYCLE_DETECTED`), retryable flag, hint string, and categorized exit code. anyhow-style errors are opaque to programmatic consumers.

## Decision

Use `thiserror` for `FilamentError` enum. Each variant carries:
- **code** — machine-readable string (e.g., `ENTITY_NOT_FOUND`, `CYCLE_DETECTED`)
- **hint** — actionable suggestion for resolution
- **retryable** — bool flag indicating whether the operation can be retried
- **exit code** — categorized (0 = success, 1 = user error, 2 = system error, etc.)

`StructuredError` wraps `FilamentError` for JSON serialization to agents. Implement `From<sqlx::Error>` on `FilamentError` so `?` works directly in transaction closures without the `TxError::Other(e.to_string())` boilerplate (koupang lesson).

## Consequences

- Agents can programmatically decide whether to retry, report, or work around errors
- CLI gets helpful hints alongside error messages
- More boilerplate per error variant than `anyhow` — but the payoff is agent-friendliness
- Exit codes enable shell script integration (`if filament task add ...; then ...`)
- `From<sqlx::Error>` implementation means `?` propagation works cleanly in transaction closures
