# ADR-012: Strict lint configuration

**Date:** 2026-03-02
**Status:** Accepted

## Context

beads_rust uses `unsafe_code = "forbid"` and denies clippy pedantic + nursery lints. This catches a wide class of issues at compile time. Filament has no need for unsafe code — it's a high-level orchestration tool.

## Decision

Workspace-level lint configuration:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

Release profile optimized for binary size:

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

## Consequences

- No unsafe code anywhere in the codebase — eliminates entire class of memory safety bugs
- Clippy pedantic catches style issues and potential bugs early
- Some clippy pedantic lints are noisy (hence the selective allows)
- `panic = "abort"` in release means no unwinding — smaller binaries but panics terminate immediately
- LTO + single codegen unit = slower release builds but smaller/faster binaries
