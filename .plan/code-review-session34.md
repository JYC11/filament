# Code Review — Session 34

Comprehensive audit of bugs, code smells, and structural issues.
Organized into independent tasks that can be tackled in any order.

---

## Task 1: Fix Critical Bug — Child Process Leak on Failed Dispatch

**Priority: P0 (critical)**
**Files:** `crates/filament-daemon/src/dispatch.rs:113-188`

**Bug:** The child process is spawned at line 162 *before* the transaction that creates
the agent run (lines 173-188). If the transaction fails (e.g., `AgentAlreadyRunning`),
`std::process::Child` is dropped — but Rust's `Drop` for `Child` does NOT kill the process.
The orphaned subprocess runs indefinitely. The MCP config file (line 137) is also leaked.

**Fix options (pick one):**
1. Move the running-agent check BEFORE spawning (pre-flight check outside transaction,
   keep the in-transaction check as a race guard)
2. Add a guard struct that kills the child + cleans up MCP config on drop unless
   explicitly disarmed after successful transaction

**Acceptance:**
- Write a test that verifies the child is killed when the transaction fails
- MCP config file is cleaned up on failure path

---

## Task 2: Fix Bug — `blocked_by_counts` Ignores `blocks` Relations

**Priority: P1 (high)**
**Files:** `crates/filament-core/src/store.rs:907-922`

**Bug:** The SQL only counts `WHERE relation_type = 'depends_on'`, but the system treats
both `blocks` AND `depends_on` as blockers (documented in gotchas, implemented in
`rebuild_blocked_cache`). Tasks blocked via `blocks` relations show 0 in the TUI.

The correct count for entity X is:
- Number of `depends_on` relations where X is `source_id` (X depends on something)
- Number of `blocks` relations where X is `target_id` (something blocks X)

**Fix:** Rewrite the SQL to a UNION or two-query approach:
```sql
SELECT entity_id, COUNT(*) FROM (
  SELECT source_id AS entity_id FROM relations WHERE relation_type = 'depends_on'
  UNION ALL
  SELECT target_id AS entity_id FROM relations WHERE relation_type = 'blocks'
) GROUP BY entity_id
```

**Acceptance:**
- Existing `blocked_by_counts` tests still pass
- Add test: create A blocks B, verify B appears in counts
- TUI "Blocked" column shows correct count for both relation types

---

## Task 3: Fix Bug — Graph-DB Divergence on Silently Discarded Error

**Priority: P1 (high)**
**Files:** `crates/filament-daemon/src/handler/relation.rs:27`

**Bug:** `let _ = state.graph_write().await.add_edge_from_relation(&rel);` discards the
result. If the graph rejects the edge, the DB has the relation but the graph doesn't,
causing incorrect results for `critical_path`, `impact_score`, `ready_tasks`, etc.

**Fix:** Log the error at `warn!` level and trigger a full graph re-hydration on failure:
```rust
if let Err(e) = state.graph_write().await.add_edge_from_relation(&rel) {
    tracing::warn!("graph edge add failed, re-hydrating: {e}");
    state.graph_write().await.hydrate(state.store.pool()).await?;
}
```

**Acceptance:**
- No silent error swallowing
- Graph stays consistent with DB

---

## Task 4: Fix Bug — TUI `event::poll()` Blocks Async Runtime

**Priority: P1 (high)**
**Files:** `crates/filament-tui/src/event.rs:17`

**Bug:** `crossterm::event::poll()` is synchronous, blocking the Tokio thread for up to
100ms per iteration. This prevents data-refresh tasks from making progress.

**Fix:** Use `crossterm::event::EventStream` (async) or wrap the poll in
`tokio::task::spawn_blocking`. The `EventStream` approach is cleaner:
```rust
use crossterm::event::EventStream;
use futures::StreamExt;
let mut reader = EventStream::new();
// In the event loop:
if let Some(Ok(event)) = reader.next().await { ... }
```

Note: May need `crossterm` `event-stream` feature enabled.

**Acceptance:**
- TUI remains responsive
- Auto-refresh works while waiting for input
- Existing TUI tests pass

---

## Task 5: Investigate — `critical_path` Edge Direction Semantics

**Priority: P1 (high, but needs design decision)**
**Files:** `crates/filament-core/src/graph.rs:276-315`

**Issue:** The DFS follows outgoing `Blocks` AND outgoing `DependsOn` edges. These go in
opposite semantic directions:
- Outgoing `DependsOn` from A: finds things A depends on (upstream)
- Outgoing `Blocks` from A: finds things A blocks (downstream)

The result mixes upstream dependencies and downstream dependents in the same path.

**Before fixing, decide the intended semantics:**
- **Option A: "What must complete before this task?"** (upstream)
  → Follow outgoing `DependsOn` + incoming `Blocks`
- **Option B: "What chain of work does this task kick off?"** (downstream)
  → Follow outgoing `Blocks` + incoming `DependsOn`
- **Option C: "Longest reachable chain in any direction"** (current, rename to `dependency_reach`)

**Acceptance:**
- Pick a direction, document it, update tests
- Add gotcha explaining the semantics

---

## Task 6: Remove Dead Code

**Priority: P2 (medium)**

**6a. `pre_dispatch_checks()` in `dispatch.rs:42-49`**
Never called — the running-agent check happens inside the transaction instead. Delete it.

**6b. `MessageStatus::Archived` in `models.rs:706-716`**
Defined but never used anywhere. Remove the variant.

**6c. Empty "Executor abstraction" section header in `store.rs:59-61`**
Vestigial comment block with nothing inside. Remove it.

**Acceptance:**
- `make ci` passes after removal
- No dead code warnings

---

## Task 7: Type-Strengthen Request DTOs and Handler Params

**Priority: P2 (medium, high-value refactor)**

Replace raw `String` fields with their corresponding enum types across the wire boundary.
Serde handles validation automatically, eliminating manual parse/match blocks.

**7a. Core request structs (`models.rs`):**
- `CreateEntityRequest.entity_type: String` → `EntityType`
- `CreateRelationRequest.relation_type: String` → `RelationType`
- `SendMessageRequest.msg_type: Option<String>` → `Option<MessageType>`
- `CreateEntityRequest.priority: Option<u8>` → `Option<Priority>`
  (needs `Deserialize` for `Priority` via `serde(try_from)`)

**7b. Daemon handler param structs:**
- `UpdateStatusParam.status` → `EntityStatus`
- `FinishAgentRunParam.status` → `AgentStatus`
- `DeleteRelationParam.relation_type` → `RelationType`
- `DispatchAgentParam.role` → `AgentRole` (blocked on Task 8)
- `ListEntitiesParam.{entity_type, status}` → typed enums
- `AcquireReservationParam.ttl_secs` → `Option<TtlSeconds>`

**7c. MCP param structs (`mcp.rs`):**
- All `ListParams`, `AddParams`, `UpdateParams`, `RelateParams`, `UnrelateParams`,
  `MessageSendParams`, `ReserveParams` — same pattern as 7b.

**7d. Remove now-dead `TryFrom` match blocks:**
- `TryFrom<CreateEntityRequest>` entity_type matching (models.rs:985-997)
- `TryFrom<CreateRelationRequest>` relation_type matching (models.rs:1055-1067)
- `TryFrom<SendMessageRequest>` msg_type matching (models.rs:1113-1123)
- Handler-side `serde_json::from_value(Value::String(...))` patterns (Finding 2B)

**Acceptance:**
- Invalid enum values rejected at deserialization, not deep in handler logic
- All 224+ tests pass
- No manual string-to-enum parsing remains in handlers

---

## Task 8: Move `AgentRole` to `filament-core`

**Priority: P2 (medium, enables Task 7 and Task 9)**
**Files:** `crates/filament-daemon/src/roles.rs` → `crates/filament-core/src/models.rs`

Move the `AgentRole` enum definition, `FromStr`, `Display`, and `name()` to
`filament-core::models`. Keep daemon-specific data (system prompts, tool whitelists)
in `filament-daemon` as functions/trait impls keyed on the enum.

**Why:** The CLI, daemon, protocol, and store all deal with agent roles but can't use the
typed enum because it's trapped in `filament-daemon`. After this move:
- CLI args can use `AgentRole` directly via Clap's `FromStr` support
- Store functions can accept `AgentRole` instead of `&str`
- `AgentRun.agent_role` can be `AgentRole` instead of `NonEmptyString`

**Acceptance:**
- `filament-core` has `AgentRole` enum with `FromStr`/`Display`/`Serialize`/`Deserialize`
- `filament-daemon` imports from core, no duplication
- CLI `DispatchArgs.role` and `DispatchAllArgs.role` use `AgentRole`

---

## Task 9: Type-Strengthen CLI Args

**Priority: P2 (medium, depends on Task 7 partially, Task 8 fully)**

Replace raw `String` CLI args with typed enums. Clap uses `FromStr` automatically.

| Arg | File | Change to |
|-----|------|-----------|
| `AddArgs.r#type` | `entity.rs:16` | `EntityType` |
| `UpdateArgs.status` | `entity.rs:46` | `Option<EntityStatus>` |
| `ListArgs.{r#type, status}` | `entity.rs:64,68` | `Option<EntityType>`, `Option<EntityStatus>` |
| `RelateArgs.relation_type` | `relation.rs:13` | `RelationType` |
| `UnrelateArgs.relation_type` | `relation.rs:29` | `RelationType` |
| `MessageSendArgs.r#type` | `message.rs:47` | `MessageType` |
| `DispatchArgs.role` | `agent.rs:45` | `AgentRole` |
| `DispatchAllArgs.role` | `agent.rs:55` | `AgentRole` |
| `TaskAddArgs.priority` | `task.rs:57` | `Option<Priority>` |

**Acceptance:**
- Invalid enum values produce Clap's built-in error messages
- No manual `.parse()` calls remain in command handlers
- `make test CRATE=filament-cli` passes

---

## Task 10: Deduplicate `inspect` / `show` Relation Rendering

**Priority: P2 (medium)**
**Files:** `crates/filament-cli/src/commands/entity.rs:186-214`,
         `crates/filament-cli/src/commands/task.rs:256-286`

**Issue:** ~29 identical lines: collect other_ids, batch_get_entities, print relations
with direction arrows.

**Fix:** Extract to `helpers.rs`:
```rust
pub async fn print_relations(
    conn: &mut FilamentConnection,
    entity_id: &EntityId,
    entity_name: &str,
    relations: &[Relation],
) -> Result<()> { ... }
```

Also fix `task::show()` which unnecessarily unwraps `Entity::Task(c)` then rewraps
`Entity::Task(c.clone())` for JSON serialization. Use `resolve_entity()` + type check instead.

**Acceptance:**
- No duplicated relation-rendering code
- `entity inspect` and `task show` produce identical output to before

---

## Task 11: Add `Entity::into_task()` / `Entity::into_agent()` Methods

**Priority: P2 (medium)**
**Files:** `crates/filament-core/src/models.rs`, `crates/filament-core/src/store.rs`,
         `crates/filament-core/src/connection.rs`

**Issue:** Four identical 7-line match blocks in `store.rs` and `connection.rs` for
extracting typed variants.

**Fix:** Add methods on `Entity`:
```rust
impl Entity {
    pub fn into_task(self) -> Result<EntityCommon> {
        match self {
            Self::Task(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch { ... }),
        }
    }
    pub fn into_agent(self) -> Result<EntityCommon> { ... }
}
```

Then `resolve_task` in both `store.rs` and `connection.rs` becomes:
```rust
resolve_entity(pool, slug_or_id).await?.into_task()
```

**Acceptance:**
- No duplicated match blocks
- All existing tests pass

---

## Task 12: Add `parse_result` Helper to `DaemonClient`

**Priority: P3 (low)**
**Files:** `crates/filament-core/src/client.rs`

**Issue:** 28 occurrences of:
```rust
serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
```
and ~8 occurrences of the single-field extraction variant.

**Fix:** Add private helpers:
```rust
fn parse_result<T: DeserializeOwned>(value: serde_json::Value) -> Result<T> { ... }
fn extract_field<T: DeserializeOwned>(value: &serde_json::Value, field: &str) -> Result<T> { ... }
```

**Acceptance:**
- No inline `map_err(|e| FilamentError::Protocol(e.to_string()))` patterns remain
- All daemon/client tests pass

---

## Task 13: Narrow `resolve_entity` Socket Error Matching

**Priority: P3 (low)**
**Files:** `crates/filament-core/src/connection.rs:141-155`

**Issue:** The Socket branch catches `FilamentError::Protocol(_)` as a trigger to fall back
from slug to ID lookup. A Protocol error could mean "connection dropped" — not "slug not found."

**Fix:** Only catch `EntityNotFound`:
```rust
Err(FilamentError::EntityNotFound { .. }) => c.get_entity(slug_or_id).await,
Err(e) => Err(e),
```

**Acceptance:**
- Connection errors propagate immediately instead of triggering fallback
- Slug resolution still works

---

## Task 14: Minor Fixes

**Priority: P3 (low)**

**14a. `Reservation.{agent_name, file_glob}` → `NonEmptyString`**
Files: `models.rs:901-902`, `store.rs:592-636`, daemon handler, CLI, tests.

**14b. `exclusive: bool` → `ReservationMode` enum**
Files: `models.rs:903`, `store.rs:596`, `connection.rs:330`, CLI `reserve.rs:17`.

**14c. `content_path` + `content_hash` → `Option<ContentRef>`**
Files: `models.rs:782-783`, `store.rs` (entity CRUD), CLI/daemon handlers.

**14d. `truncate_with_ellipsis` edge case (max_chars < 3)**
Files: `helpers.rs:73-81`. Return empty or first N chars without ellipsis.

**14e. Move `truncate_with_ellipsis` to `filament-core`**
Deduplicate `helpers.rs:73-81` and `tui/views/agents.rs:94-101`.

**14f. Deduplicate `format_duration` / `format_remaining` in TUI**
Files: `tui/views/agents.rs:67-82`, `tui/views/reservations.rs:92-104`.

**14g. Clamp `context_query` depth to max 10**
Files: daemon `handler/graph.rs:59`, `mcp.rs:234`.

**14h. `mark_message_read` — return specific error for already-read messages**
Files: `store.rs:563-581`.

**14i. Remove pass-through CLI helper wrappers**
Files: `helpers.rs:33-56`. Call `conn.resolve_*()` directly.

---

## Task 15: Break `dispatch ↔ server` Bidirectional Module Dependency

**Priority: P3 (low, design smell)**
**Files:** `crates/filament-daemon/src/dispatch.rs`, `server.rs`

`dispatch` imports `server::SharedState`, `server` imports `dispatch::DispatchConfig`.
Move `SharedState` and `DispatchConfig` to a shared module (e.g., `state.rs` or `config.rs`).

---

## Dependency Graph

```
Task 1  (child leak)         — independent
Task 2  (blocked_by_counts)  — independent
Task 3  (graph-DB diverge)   — independent
Task 4  (TUI event poll)     — independent
Task 5  (critical_path)      — independent (needs design decision)
Task 6  (dead code)          — independent
Task 7  (type DTOs)          — partially blocked by Task 8 (for AgentRole)
Task 8  (move AgentRole)     — independent
Task 9  (type CLI args)      — blocked by Task 7, Task 8
Task 10 (dedup inspect/show) — independent
Task 11 (Entity::into_task)  — independent
Task 12 (parse_result)       — independent
Task 13 (narrow error match) — independent
Task 14 (minor fixes)        — independent (14b pairs with Task 7)
Task 15 (break bidir dep)    — independent
```

Tasks 1-5 are bugs (fix first). Tasks 6-15 are code smells (fix in order of impact).
