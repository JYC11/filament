# ADR-006: Per-project `.filament/` directory with SQLite (WAL mode)

**Date:** 2026-03-02
**Status:** Accepted

## Context

Filament data needs to live somewhere. Options: global database (one per user), per-project database, or external service. beads_rust uses a per-project `.beads/` directory.

## Decision

`filament init` creates a `.filament/` directory in the project root containing:
- `filament.db` — SQLite database (WAL mode)
- `filament.sock` — Unix socket (when daemon is running)
- `filament.pid` — daemon PID file
- `content/` — full content files referenced by `content_path`

SQLite pool initialization sets PRAGMAs via `SqlitePoolOptions::after_connect`: WAL mode, `foreign_keys=ON`, `busy_timeout=5000`, `synchronous=NORMAL`.

## Consequences

- Data is local to each project — no cross-project contamination, easy to delete
- `.filament/` can be gitignored (project-specific runtime data)
- WAL mode enables concurrent reads during daemon operation
- No external database dependency — zero infrastructure
- Must handle the case where `.filament/` doesn't exist yet (helpful error: "run `filament init` first")
- Migrations use `sqlx::migrate!()` macro (embedded) not filesystem — avoids `CARGO_MANIFEST_DIR` issues in deployed binaries (lesson from workout-util)
