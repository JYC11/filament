# Gotchas

Pitfalls discovered during implementation. Check here before debugging mysterious failures.

## sqlx

- **Custom newtypes need `fn compatible()`** — implementing `sqlx::Type` with just `type_info()` is not enough; override `compatible()` to delegate to the inner type (e.g., `<i32 as Type<Sqlite>>::compatible(ty)`), or `FromRow` decode fails at runtime with "mismatched types" even though `type_info()` returns the correct type. This is a SQLite type affinity issue.
- **`chrono` feature required** — sqlx needs `features = ["chrono"]` for `DateTime<Utc>` encode/decode. schemars needs `features = ["chrono04"]` for `DateTime<Utc>` JsonSchema.
- **Raw SQL timestamps** — must use ISO 8601 format (`2024-01-01T00:00:00Z`), not bare dates like `2024-01-01`. SQLite stores timestamps as TEXT.
- **`with_transaction` closure** — requires `|conn| Box::pin(async move { ... })` for lifetime correctness. The boxed future is mandatory; you cannot use a plain async closure.

## thiserror

- **v2 treats fields named `source` as error sources** — thiserror v2 auto-wraps any field named `source` with `#[source]`. Rename to `source_id`/`target_id` or similar to avoid unexpected behavior.

## petgraph

- **v0.7 requires `use petgraph::visit::EdgeRef`** — calling `.source()` or `.target()` on edge references requires this import. Without it, you get a confusing "method not found" error.

## Value types (ADR-018)

- **`Priority`/`Weight` are `Copy`** — pass by value to `.bind()`. Clippy warns on needless `&` for generic args (`clippy::needless_borrows_for_generic_args`).
- **`NonEmptyString` trims on construction** — `NonEmptyString::new("  hello  ")` produces `"hello"`. This is intentional but can surprise if you expect whitespace preservation.
- **Serde `try_from`/`into` on newtypes** — all value types use `#[serde(try_from = "T", into = "T")]` so deserialization rejects invalid values. This means JSON like `{"priority": 99}` fails at the serde layer, not at business logic validation.

## Tests

- **`#![allow(dead_code)]` in `tests/common/mod.rs`** — each test binary only uses a subset of helpers; without this, every test file gets dead code warnings for unused helpers.
- **`filament-core` dev-dep needs `features = ["test-utils"]`** — the dev-dependencies self-reference must include this feature for `init_test_pool()` access.
