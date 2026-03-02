# ADR-014: Stable Rust toolchain

**Date:** 2026-03-02
**Status:** Accepted

## Context

beads_rust requires nightly Rust for `edition = "2024"` (Rust 1.88+). This creates friction for contributors and CI — nightly can break without warning, and specific nightly versions must be pinned.

## Decision

Target stable Rust. Avoid nightly-only features. Use the current stable edition (2021 or 2024 once stable).

## Consequences

- Broader compatibility — anyone with a recent stable toolchain can build
- CI is simpler — no nightly pinning or override files
- May miss some ergonomic nightly features, but nothing filament needs is nightly-only
- If a dependency requires nightly, that's a reason to find an alternative
