# ADR-017: Single binary distribution

**Date:** 2026-03-02
**Status:** Accepted

## Context

The original plan had three separate binaries (`filament`, `filament-daemon`, `filament-tui`) from three binary crates. This means three `cargo install` commands and complicates non-cargo distribution (curl-based install scripts, GitHub release binaries, Homebrew formulae).

beads_rust ships as a single `br` binary with all functionality behind subcommands. This is simpler for users and distributors.

## Decision

Ship a single `filament` binary. The daemon and TUI crate become libraries consumed by `filament-cli`:

- `filament-core/` — shared library (unchanged)
- `filament-daemon/` — library crate exporting `serve()` entrypoint
- `filament-tui/` — library crate exporting `run_tui()` entrypoint
- `filament-cli/` — the single binary, depends on all three

Subcommands:
- `filament serve [--background]` — starts daemon (was `filament-daemon` binary)
- `filament tui` — launches TUI (was `filament-tui` binary)
- All other commands — CLI as before

Distribution channels:
- `cargo install filament` (publishes `filament-cli` crate with `filament` binary name)
- GitHub Releases with pre-built binaries (cross-compiled via CI)
- Curl install script: `curl -fsSL https://filament.dev/install.sh | sh`
- Homebrew: `brew install filament` (future)

## Consequences

- One install command, one binary on `$PATH`, one thing to update
- Simpler CI — build one artifact per platform, not three
- Binary size is larger (includes TUI + daemon code even if unused) — mitigated by `opt-level = "z"` + LTO + strip already in the plan
- `filament-daemon` and `filament-tui` can still be published as library crates for programmatic embedding
- Feature gates could exclude TUI or daemon from the binary if size becomes an issue (defer)
