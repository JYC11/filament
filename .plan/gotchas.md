# Gotchas

Pitfalls discovered during implementation. Check here before debugging mysterious failures.

## sqlx

- **Custom newtypes need `fn compatible()`** — implementing `sqlx::Type` with just `type_info()` is not enough; override `compatible()` to delegate to the inner type (e.g., `<i32 as Type<Sqlite>>::compatible(ty)`), or `FromRow` decode fails at runtime with "mismatched types". This applies to both `impl_sqlx_newtype!` and `typed_id!`.
- **`chrono` feature required** — sqlx needs `features = ["chrono"]` for `DateTime<Utc>` encode/decode. schemars needs `features = ["chrono04"]` for `DateTime<Utc>` JsonSchema.
- **Raw SQL timestamps** — must use ISO 8601 format (`2024-01-01T00:00:00Z`), not bare dates like `2024-01-01`. SQLite stores timestamps as TEXT.
- **`with_transaction` closure** — requires `|conn| Box::pin(async move { ... })` for lifetime correctness. The boxed future is mandatory; you cannot use a plain async closure.
- **Clone IDs before `with_transaction`** — if you need an `id` after the closure, clone it first (`let tx_id = id.clone()`), because the closure moves captured variables.
- **Reborrow pattern for event wiring** — use `.execute(&mut *conn)` then `record_event(conn, ...)` to reuse `&mut SqliteConnection` across multiple queries in a transaction.

## thiserror

- **v2 treats fields named `source` as error sources** — thiserror v2 auto-wraps any field named `source` with `#[source]`. Rename to `source_id`/`target_id` or similar to avoid unexpected behavior.

## petgraph

- **v0.7 requires `use petgraph::visit::EdgeRef`** — calling `.source()` or `.target()` on edge references requires this import. Without it, you get a confusing "method not found" error.

## Value types (ADR-018)

- **`Priority`/`Weight` are `Copy`** — pass by value to `.bind()`. Clippy warns on needless `&` for generic args.
- **`NonEmptyString` trims on construction** — `NonEmptyString::new("  hello  ")` produces `"hello"`. Intentional but can surprise.
- **`NonEmptyString` implements `PartialEq<&str>`** — enables `entity.name == "foo"` comparisons.
- **`MessageType` does NOT implement `PartialEq<&str>`** — use `.as_str()` for comparison (e.g., `msg.msg_type.as_str() == "question"`).
- **Serde `try_from`/`into` on newtypes** — all value types use `#[serde(try_from = "T", into = "T")]` so deserialization rejects invalid values at the serde layer.

## Graph / Store behavior

- **Entity names are NOT unique** — no UNIQUE constraint on `entities.name`. Multiple entities can share a name.
- **`critical_path()` always returns at least 1 node** — even with no outgoing deps, the starting node is included.
- **`critical_path()` on closed tasks** — may return only 1 node; don't assert `>= 2` on closed task chains.
- **`context_summaries()` excludes the starting node** — BFS returns only neighbors within N hops, not the root itself.
- **BFS uses manual queue** — `neighbors_directed()` in both directions, not petgraph's `Bfs` (which is outgoing-only).
- **Both `blocks` AND `depends_on` block tasks** — `ready_tasks()` treats both relation types as blockers.
- **`ready_tasks()` takes `&mut SqliteConnection`** — not `&Pool<Sqlite>`, and it auto-rebuilds the blocked cache.
- **`release_reservation` is idempotent** — no error on double-release. `mark_message_read` returns NotFound on already-read.
- **`finish_agent_run` returns NotFound for nonexistent runs** — does a SELECT before UPDATE to get task_id/agent_role for event recording.
- **`delete_relation` (by id) does NOT record events** — only `delete_relation_by_endpoints` does (the daemon handler only uses the latter).
- **String truncation** — must use char boundaries, not byte slicing. Use `truncate_with_ellipsis()`.

## Daemon

- **Tests need `#[tokio::test(flavor = "multi_thread")]`** — single-threaded runtime deadlocks on concurrent connections.
- **`CancellationToken` for shutdown** — `tokio_util::sync::CancellationToken` is the clean shutdown mechanism.
- **NDJSON safety** — `serde_json::to_string()` (compact) never emits newlines, so it's safe for the newline-delimited protocol.
- **`SharedState.graph` is `RwLock<KnowledgeGraph>`** — write ops must update the in-memory graph after DB commit.
- **ALL mutating handlers must refresh the graph** — call `graph_write().add_node_from_entity()` after DB commit for any field stored in the graph (summary, status, etc.).
- **Handler sub-modules are private** — `mod entity;` etc. Only `dispatch()` in `mod.rs` is `pub`.
- **Handler `exclusive` default is `false`** — was incorrectly `true` before session 18 fix.
- **Multi-agent race conditions** — concurrent `ready_tasks()` calls can race; check readiness before spawning concurrent agents.

## MCP / rmcp

- **Tool return type must be `Result<String, String>`** — `CallToolResult` does NOT implement `IntoCallToolResult`.
- **`#[tool_handler]` macro uses 2-arg `Result<T, E>`** — don't import `filament_core::error::Result` (1-arg alias) in mcp.rs.
- **`Parameters(p): Parameters<T>`** — extracts tool params; `#[tool_router]` on impl block, `#[tool(name = "...")]` on methods.
- **`Content` is `Annotated<RawContent>`** — extract text with `c.raw.as_text().map(|t| t.text.as_str())`.
- **`CallToolRequestParams` has no `Default`** — build manually with all fields in tests.
- **MCP tests need `rmcp` with `client` feature** in dev-dependencies.
- **MCP mode must redirect tracing to stderr** — `stdout` is JSON-RPC transport. Use `.with_writer(std::io::stderr).with_ansi(false)`.

## CLI

- **CLI avoids sqlx dep** — uses `FilamentStore` directly, not `Pool<Sqlite>`.
- **`filament relate` arg order** — `<SOURCE> <RELATION_TYPE> <TARGET>`.
- **`filament update` requires at least one flag** — `--summary` or `--status`.
- **`message inbox` takes positional arg** — not `--agent` flag.
- **`filament serve --foreground`** runs inline; bare `filament serve` re-execs detached.
- **Exclusive reservation semantics** — exclusive conflicts with ALL other-agent reservations; non-exclusive only conflicts with exclusive.

## Clippy / Rust

- **`pub(crate)` in private modules** — triggers `clippy::redundant_pub_crate`; use plain `pub` (module privacy is the real access control).
- **`assert_cmd::Command::cargo_bin` is deprecated** — needs `#[allow(deprecated)]`; no non-deprecated alternative.
- **`Cow<'static, str>.as_str()`** — requires unstable `str_as_str`; use `.as_ref()` instead.
- **`.filament/` is gitignored** — per-user local database, not committed.

## Tests

- **`#![allow(dead_code)]` in `tests/common/mod.rs`** — each test binary only uses a subset of helpers.
- **`filament-core` dev-dep needs `features = ["test-utils"]`** — for `init_test_pool()` access.
