# TUI Messages Tab Enhancement

Status: Refined v1
Created: 2026-03-09
Depends on: `.plan/tui-enhancement.md` (Phase 1+2 complete)

## Motivation

The messages tab currently shows only **pending escalations** (unread blockers/questions to "user") via the `Escalation` type. This means:
- No way to see inter-agent messages
- No way to see read/resolved messages
- No detail view — long message bodies are truncated
- No filtering, sorting, or paging

The entities tab already has all these features. This plan brings messages to parity.

---

## Current State

**Data**: `App.messages: Vec<Escalation>` via `conn.list_pending_escalations()`
**Rendering**: Fixed table — Kind | Agent | Body (truncated) | Task | Time
**Interaction**: j/k navigation, no Enter/detail, no filters

**Available data layer APIs**:
| Function | Layer | Notes |
|----------|-------|-------|
| `store::list_all_messages()` | Store only | Returns `Vec<Message>`, NOT exposed in connection/daemon |
| `store::get_inbox(agent)` | Store + Connection + Daemon | Unread messages for one agent |
| `store::list_pending_escalations()` | Store + Connection + Daemon | Unread blockers/questions + blocked agent runs |
| `store::send_message(req)` | Store + Connection + Daemon | Send a message |
| `store::mark_message_read(id)` | Store + Connection + Daemon | Mark read |

**Gap**: `list_all_messages()` is store-only. Must be plumbed through connection + daemon for TUI to work in both direct and daemon modes.

**Entity tab gap**: Entity paging/filtering/sorting is entirely in-memory — `list_entities()` returns ALL matching entities, `.retain()` applies multi-value filters, `sort_rows()` sorts the full vec, `visible_entities()` slices a page. This must be refactored to SQL-level before building the message equivalent, establishing a shared pattern.

---

## Task 0: Keyset Pagination Module + Entity SQL Paging (P1)

**Goal**: Create a shared `pagination.rs` module with keyset pagination helpers, then refactor entity tab from in-memory to SQL-level filtering/sorting/paging. Messages (Task 1) will reuse the same helpers.

Design adapted from `.plan/tui-enhancement.md` Task 1.2 (specced but never implemented) and `workout-util/src/db/pagination_support.rs`.

### Current (in-memory)

```
list_entities(type?, status?)     → Vec<Entity>  (all matching)
  → .retain() multi-value filters   (in-memory)
  → sort_rows()                      (in-memory)
  → visible_entities() slice         (in-memory paging)
```

### Target (SQL-level with keyset cursors)

```
list_entities_paged(req)          → PagedResult<Entity>  (one page, filtered+sorted+cursored)
```

### Part A: Shared Pagination Module (`crates/filament-core/src/pagination.rs`)

Three reusable components — used by both entity and message queries.

#### Component 1: `PaginationState` (TUI-side cursor tracking)

```rust
pub struct PaginationState {
    pub limit: u32,                         // default 50
    pub next_cursor: Option<String>,        // cursor for forward paging
    pub prev_cursor: Option<String>,        // cursor for backward paging
    pub current_cursor: Option<String>,     // cursor used for current query
    pub direction: PaginationDirection,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PaginationDirection { Forward, Backward }

impl PaginationState {
    pub fn new(limit: u32) -> Self { ... }
    pub fn reset(&mut self) { /* clear cursors, direction = Forward */ }
    pub fn has_next(&self) -> bool { self.next_cursor.is_some() }
    pub fn has_previous(&self) -> bool { self.prev_cursor.is_some() }
    pub fn go_forwards(&mut self) {
        self.current_cursor = self.next_cursor.clone();
        self.direction = PaginationDirection::Forward;
    }
    pub fn go_backwards(&mut self) {
        self.current_cursor = self.prev_cursor.clone();
        self.direction = PaginationDirection::Backward;
    }
    pub fn to_params(&self) -> PaginationParams {
        PaginationParams {
            limit: self.limit,
            cursor: self.current_cursor.clone(),
            direction: self.direction,
        }
    }
    /// Update cursors after receiving a page result.
    pub fn update_cursors(&mut self, next: Option<String>, prev: Option<String>) {
        self.next_cursor = next;
        self.prev_cursor = prev;
    }
}
```

#### Component 2: `keyset_paginate()` (store-side query builder helper)

Extended from the established pattern in workout-util/koupang to support custom sort columns. The cursor remains just the id (matching the established `HasId` pattern). Custom sort ordering uses SQLite tuple comparison with a subquery to resolve the sort value at the cursor point.

```rust
pub struct PaginationParams {
    pub limit: u32,
    pub cursor: Option<String>,       // still just the id, same as established pattern
    pub direction: PaginationDirection,
}

/// Appends keyset pagination to an existing QueryBuilder.
///
/// Extension of the workout-util/koupang pattern: adds `sort_column`,
/// `sort_direction`, and `table` parameters for custom sort support.
/// The cursor is still just the id — sort value at cursor is resolved
/// via subquery using SQLite tuple comparison.
///
/// Established pattern (sort by id only):
///   keyset_paginate(params, alias, qb)
///
/// Extended pattern (sort by custom column + id tiebreaker):
///   keyset_paginate(params, table, sort_column, sort_direction, id_column, qb)
///
/// Generated SQL (forward, sort by priority ASC):
///   AND (priority, id) > (SELECT priority, id FROM entities WHERE id = ?cursor)
///   ORDER BY priority ASC, id ASC
///   LIMIT 51
pub(crate) fn keyset_paginate(
    params: &PaginationParams,
    qb: &mut QueryBuilder<Sqlite>,
    table: &str,           // table name for subquery (e.g., "entities", "messages")
    sort_column: &str,     // e.g., "priority", "created_at", "name"
    sort_direction: &str,  // "ASC" or "DESC"
    id_column: &str,       // e.g., "id" — unique + monotonic tiebreaker
) {
    match params.direction {
        PaginationDirection::Forward => {
            if let Some(ref cursor) = params.cursor {
                qb.push(format!(
                    " AND ({sort_column}, {id_column}) > \
                     (SELECT {sort_column}, {id_column} FROM {table} WHERE {id_column} = "
                ));
                qb.push_bind(cursor.as_str());
                qb.push(")");
            }
            qb.push(format!(" ORDER BY {sort_column} {sort_direction}, {id_column} ASC"));
        }
        PaginationDirection::Backward => {
            if let Some(ref cursor) = params.cursor {
                qb.push(format!(
                    " AND ({sort_column}, {id_column}) < \
                     (SELECT {sort_column}, {id_column} FROM {table} WHERE {id_column} = "
                ));
                qb.push_bind(cursor.as_str());
                qb.push(")");
            }
            let reverse_dir = if sort_direction == "ASC" { "DESC" } else { "ASC" };
            qb.push(format!(" ORDER BY {sort_column} {reverse_dir}, {id_column} DESC"));
        }
    }
    qb.push(" LIMIT ");
    qb.push_bind(params.limit + 1); // +1 to detect has_more
}
```

**Design notes:**
- Cursor is still just the id (`Option<String>`), matching established `HasId`/`get_cursors` pattern exactly
- Sort value at cursor point is resolved via subquery — no composite cursor serialization needed
- SQLite tuple comparison `(a, b) > (SELECT a, b ...)` is supported and uses index-friendly row-value ordering
- `table` parameter needed for the subquery (workout-util/koupang used `alias` for the same table reference)
- `get_cursors()` and `PaginationState` are completely unchanged from the established pattern
- When `sort_column` equals `id_column`, the behavior degrades gracefully to the original id-only pattern
```

#### Component 3: `get_cursors()` (post-fetch cursor extraction)

```rust
pub trait Paginatable {
    fn cursor_id(&self) -> &str;
}

/// Extracts next/prev cursors from a fetched result set.
/// Mutates `rows` in place: pops the extra row if has_more, reverses if backward.
pub(crate) fn get_cursors<T: Paginatable>(
    params: &PaginationParams,
    rows: &mut Vec<T>,
) -> (Option<String>, Option<String>) {  // (next_cursor, prev_cursor)
    let has_more = rows.len() > params.limit as usize;
    if has_more { rows.pop(); }

    if matches!(params.direction, PaginationDirection::Backward) {
        rows.reverse();
    }

    let start_id = rows.first().map(|r| r.cursor_id().to_string());
    let end_id = rows.last().map(|r| r.cursor_id().to_string());

    match params.direction {
        PaginationDirection::Forward => {
            let next = if has_more { end_id } else { None };
            let prev = if params.cursor.is_some() { start_id } else { None };
            (next, prev)
        }
        PaginationDirection::Backward => {
            let next = end_id;
            let prev = if has_more { start_id } else { None };
            (next, prev)
        }
    }
}
```

#### Shared result type

```rust
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
}
```

### Part B: Entity Filtered+Paged Query

New types in `dto.rs`:

```rust
pub struct ListEntitiesRequest {
    pub types: Vec<EntityType>,          // empty = all
    pub statuses: Vec<EntityStatus>,     // empty = all
    pub priorities: Vec<Priority>,       // empty = all
    pub sort_field: EntitySortField,
    pub sort_direction: SortDirection,
    pub pagination: PaginationParams,
}

pub enum EntitySortField {
    Name,       // name column
    Priority,   // priority column (default)
    Status,     // status column
    Updated,    // updated_at column
    Created,    // created_at column
}
```

Note: `Impact` sort is dropped — it's a derived graph value, not a database column. Impact sort would require fetching all entities to compute scores, defeating SQL-level paging.

`SortDirection` already exists in TUI (`app.rs`). Move to `dto.rs` so both store and TUI share it.

### Store (`store.rs`) — Add `list_entities_paged()`

```rust
pub async fn list_entities_paged(
    pool: &Pool<Sqlite>,
    req: &ListEntitiesRequest,
) -> Result<PagedResult<Entity>> {
    let mut qb = QueryBuilder::new("SELECT * FROM entities WHERE 1=1");

    // Multi-value filters using IN (...)
    if !req.types.is_empty() { /* push AND entity_type IN (...) */ }
    if !req.statuses.is_empty() { /* push AND status IN (...) */ }
    if !req.priorities.is_empty() { /* push AND priority IN (...) */ }

    // Keyset pagination (appends cursor clause + ORDER BY + LIMIT)
    let sort_col = match req.sort_field { ... };
    let sort_dir = match req.sort_direction { ... };
    keyset_paginate(&req.pagination, &mut qb, "entities", sort_col, sort_dir, "id");

    let mut rows: Vec<EntityRow> = qb.build_query_as().fetch_all(pool).await?;

    // Implement Paginatable for EntityRow
    let (next, prev) = get_cursors(&req.pagination, &mut rows);
    let entities = rows.into_iter().map(Entity::from).collect();

    Ok(PagedResult { items: entities, next_cursor: next, prev_cursor: prev })
}
```

### Connection + Daemon + Client plumbing

- `connection.rs` — Add `list_entities_paged()`, route direct/socket
- `handler/entity.rs` — Add `list_paged` handler, parse request from JSON
- `client.rs` — Add `list_entities_paged()` RPC
- `server.rs` — Register `"entity.list_paged"` route

### TUI changes (`app.rs`)

- Replace `page: usize` / `page_size: usize` with `entity_pagination: PaginationState`
- Remove `sort_rows()`, `visible_entities()`, `total_pages()`, `has_next_page()`, `has_prev_page()` (all in-memory)
- `refresh_entities()` builds `ListEntitiesRequest` from `FilterState` + `SortState` + `entity_pagination.to_params()`, calls `conn.list_entities_paged()`, stores result in `self.entities`, updates cursors via `entity_pagination.update_cursors()`
- `next_page()` calls `entity_pagination.go_forwards()` then `refresh_entities()`
- `prev_page()` calls `entity_pagination.go_backwards()` then `refresh_entities()`
- Changing any filter calls `entity_pagination.reset()` then `refresh_entities()`
- Tab title: `[< page >]` with `<`/`>` dimmed when no prev/next cursor
- `SortField` → `EntitySortField` (move to dto.rs, remove `Impact` variant)
- `SortDirection` → move to dto.rs
- Keep `blocked_by_count` and `impact` enrichment in `build_entity_rows()` — computed after fetch for current page only (cheap for 50 rows)

### Backward compatibility

`list_entities(type?, status?)` is unchanged — used by CLI, export, graph, MCP, seed, tests. The new `list_entities_paged()` is additive.

### `ready_tasks()` path

`ready_only` still calls `conn.ready_tasks()` — ready tasks are few and already optimized. No change needed.

**Files**: `pagination.rs` (new), `lib.rs`, `dto.rs`, `store.rs`, `connection.rs`, `handler/entity.rs`, `client.rs`, `server.rs`, `app.rs`
**Tests**: Pagination module unit tests (cursor extraction, direction, has_more detection). Store integration test — paged listing returns correct subset, order, and cursors.

---

## Task 1: Filtered Message Query in Data Layer (P1)

**Goal**: Add a filtered message listing through store → connection → daemon, replacing the unfiltered `list_all_messages()`.

**New types** (in `dto.rs`):
```rust
pub struct ListMessagesRequest {
    pub msg_types: Vec<MessageType>,          // empty = all
    pub read_status: Option<MessageStatus>,   // None = all
    pub participant: Option<String>,          // None = all, Some("user") or Some(slug)
    pub sort_field: MessageSortField,
    pub sort_direction: SortDirection,        // shared from pagination module
    pub pagination: PaginationParams,         // keyset cursor from pagination module
}

pub enum MessageSortField {
    Time,    // created_at (default)
    Type,    // msg_type
    From,    // from_agent
    Status,  // read status
}
```

**Store** (`store.rs`) — Add `list_messages_paged()`:
```rust
pub async fn list_messages_paged(
    pool: &Pool<Sqlite>,
    req: &ListMessagesRequest,
) -> Result<PagedResult<Message>> {
    let mut qb = QueryBuilder::new("SELECT * FROM messages WHERE 1=1");

    if !req.msg_types.is_empty() {
        qb.push(" AND msg_type IN (");
        let mut sep = qb.separated(", ");
        for t in &req.msg_types { sep.push_bind(t.as_str()); }
        sep.push_unseparated(")");
    }

    if let Some(ref status) = req.read_status {
        match status {
            MessageStatus::Unread => qb.push(" AND read_at IS NULL"),
            MessageStatus::Read => qb.push(" AND read_at IS NOT NULL"),
        };
    }

    if let Some(ref p) = req.participant {
        qb.push(" AND (from_agent = ");
        qb.push_bind(p.as_str());
        qb.push(" OR to_agent = ");
        qb.push_bind(p.as_str());
        qb.push(")");
    }

    // Keyset pagination (reuses shared helper from pagination.rs)
    let sort_col = match req.sort_field {
        MessageSortField::Time => "created_at",
        MessageSortField::Type => "msg_type",
        MessageSortField::From => "from_agent",
        MessageSortField::Status => "read_at",
    };
    let sort_dir = match req.sort_direction {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    };
    keyset_paginate(&req.pagination, &mut qb, "messages", sort_col, sort_dir, "id");

    let mut rows: Vec<Message> = qb.build_query_as().fetch_all(pool).await?;

    // Implement Paginatable for Message (cursor_id = message id)
    let (next, prev) = get_cursors(&req.pagination, &mut rows);

    Ok(PagedResult { items: rows, next_cursor: next, prev_cursor: prev })
}
```

**Connection** (`connection.rs`) — Add `list_messages_paged()`, returns `PagedResult<Message>`.

**Daemon** (`handler/message.rs`) — Add `list_paged` handler, parse `ListMessagesRequest` from JSON params.

**Client** (`client.rs`) — Add `list_messages_paged()` RPC method.

**Server** (`server.rs`) — Register `"message.list_paged"` route.

**Files**: `dto.rs`, `store.rs`, `connection.rs`, `handler/message.rs`, `client.rs`, `server.rs`
**Tests**: Unit test — paged listing returns correct subset, cursors, and sort order.

---

## Task 2: Message Detail Pane (P1)

**Goal**: Press `Enter` on a selected message to see full body + metadata in a detail pane.

**Design**: Reuse the existing 60/40 split pattern from entities tab.

**Detail pane sections**:
1. **Header**: Message type (color-coded) + read/unread status
2. **Routing**: `From: [slug] name → To: [slug] name` (resolve entity names, "user" stays as "user")
3. **Body**: Full text, word-wrapped
4. **Task link**: If `task_id` is set, show `Task: [slug] name`
5. **Reply chain**: If `in_reply_to` is set, show parent message summary
6. **Timestamps**: Created at, Read at (if read)

**New struct**:
```rust
pub struct MessageDetailData {
    pub message: Message,
    pub from_name: String,     // resolved display name
    pub to_name: String,       // resolved display name
    pub task_name: Option<String>,  // resolved if task_id present
    pub reply_to: Option<Message>,  // parent message if in_reply_to present
}
```

**App changes**:
- Add `message_detail: Option<MessageDetailData>`, `message_detail_scroll: u16`
- `open_message_detail()` — fetch message, resolve names, fetch parent
- `close_message_detail()` — clear

**Event changes**:
- `Enter` on Messages tab → `open_message_detail()`
- `Esc` → close detail
- `j`/`k` in detail mode → scroll

**Files**: `app.rs`, `views/messages.rs` (add detail renderer), `event.rs`, `ui.rs`

---

## Task 3: Message Filtering (P1)

**Goal**: Filter messages by type, participant, and read status — all at the SQL level via `ListMessagesRequest`.

### Filter Model

```rust
pub struct MessageFilterState {
    pub msg_types: HashSet<MessageType>,     // empty = all
    pub read_status: Option<MessageStatus>,  // None = all, Some(Unread), Some(Read)
    pub participant: MessageParticipantFilter,
    pub active_bar: Option<MessageFilterBar>,
}

pub enum MessageParticipantFilter {
    All,                     // no participant filter in query
    Mine,                    // participant = "user"
    Agent(String),           // participant = agent slug
}

pub enum MessageFilterBar {
    Type,       // text, question, blocker, artifact
    Status,     // all, unread, read
}
```

**Defaults**: `msg_types = all`, `read_status = None (all)`, `participant = Mine`

Starting with "Mine" (messages to/from user) preserves the current UX — user sees their escalations first, then can widen to see all.

### Key Bindings (Messages tab)

| Key | Action |
|-----|--------|
| `t` | Open type filter bar: `1:Text 2:Question 3:Blocker 4:Artifact 0:Clear` |
| `f` | Open read status filter bar: `1:All 2:Unread 3:Read` |
| `a` | Cycle participant filter: All → Mine → (each agent) → All |

`a` cycles rather than opening a bar because agent count is variable and a numbered bar doesn't scale. The current participant filter is shown in the tab title.

### Filter → Query Translation

Changing any filter rebuilds a `ListMessagesRequest` and re-fetches from the database. The `MessageFilterState` maps directly to query parameters:

```rust
fn build_message_request(&self) -> ListMessagesRequest {
    ListMessagesRequest {
        msg_types: self.msg_filter.msg_types.iter().cloned().collect(),
        read_status: self.msg_filter.read_status.clone(),
        participant: match &self.msg_filter.participant {
            All => None,
            Mine => Some("user".to_string()),
            Agent(slug) => Some(slug.clone()),
        },
        sort_field: self.msg_sort.field,
        sort_direction: self.msg_sort.direction,
        pagination: self.msg_pagination.to_params(),
    }
}
```

Changing any filter calls `msg_pagination.reset()` then triggers `refresh_messages()`.

### Tab Title

```
Messages [mine | unread | 1/3]
Messages [all | question,blocker | 2/5]
Messages [agent:h5pmjq57 | 1/1]
```

**Files**: `app.rs`, `views/messages.rs`, `views/filter_bar.rs` (add message filter bars), `event.rs`

---

## Task 4: Message Paging + Sorting (P2)

**Goal**: Add paging and sort controls matching entities tab patterns.

### Paging

Keyset pagination via `PaginationState` (shared module from Task 0). TUI tracks `msg_pagination: PaginationState` (limit 50). Cursor presence drives `<`/`>` page indicators.

| Key | Action |
|-----|--------|
| `n` | `msg_pagination.go_forwards()` then `refresh_messages()` (re-fetch) |
| `p` | `msg_pagination.go_backwards()` then `refresh_messages()` (re-fetch) |

### Sorting

`MessageSortField` and `SortDirection` are part of `ListMessagesRequest` (defined in Task 1). The TUI holds a `MessageSortState` that feeds into the query. Changing sort calls `msg_pagination.reset()` then re-fetches.

| Key | Action |
|-----|--------|
| `s` | Open sort bar: `1:Time↓ 2:Type 3:From 4:Status` (same field = flip direction) |

Default: Time descending (newest first).

### Updated Columns

With all messages visible (not just escalations), columns should adapt:

```
Type | From | To | Body | Status | Time
```

- **Type**: color-coded (Red=Blocker, Yellow=Question, Cyan=Text, Magenta=Artifact)
- **From/To**: resolved slug or "user"
- **Body**: truncated preview
- **Status**: "●" (unread, bold) or "○" (read, dim)
- **Time**: relative or absolute

**Files**: `app.rs`, `views/messages.rs`, `event.rs`

**Depends on**: Task 1 (data layer), Task 3 (filtering produces the request to page/sort)

---

## Task 5: Reply from TUI (P4, Nice-to-Have)

**Goal**: Reply to a selected message directly from the TUI without switching to CLI.

**Design**:

This is the first **write operation** in the TUI and requires a text input widget. Approach:

1. Press `R` on a selected message (or in detail view) → opens reply mode
2. Reply mode:
   - Bottom pane becomes a text input area (replaces detail)
   - Pre-fills `from: user`, `to: <original sender>`, `in_reply_to: <message_id>`
   - Type filter defaults to `text` but can cycle with `t` (text/question/blocker)
   - `Enter` sends, `Esc` cancels
3. Uses `conn.send_message()` with `ValidSendMessageRequest`
4. After send: close reply pane, auto-refresh messages, show status bar confirmation

**Text input considerations**:
- Single-line initially (one-liner replies). Multi-line is complex.
- Use ratatui `Paragraph` with cursor tracking
- Store input in `reply_buffer: String`, `reply_cursor: usize`
- Standard editing: left/right, backspace, delete, home/end

**Risk**: Text input in TUI is non-trivial (cursor management, unicode, scrolling). Keep it minimal — single line, no history, no clipboard. If it becomes complex, punt to a future epic.

**Files**: `app.rs` (ReplyState), `views/reply.rs` (new), `event.rs` (reply mode key handling), `ui.rs` (layout with reply pane)

**Depends on**: Task 2 (detail pane — reply builds on it)

---

## Task Dependencies

```
Task 0: Entity SQL paging ──────────────── no deps (establishes pattern)
Task 1: Message data layer ─────────────── depends on Task 0 (follows same pattern)
Task 2: Detail Pane ─────────────────────── depends on Task 1
Task 3: Filtering ──────────────────────── depends on Task 1
Task 4: Paging + Sorting ──────────────── depends on Task 1, Task 3
Task 5: Reply (P4) ─────────────────────── depends on Task 2
```

**Execution order**: Task 0 → Task 1 → Tasks 2+3 (parallel) → Task 4 → Task 5 (if pursued)

## Estimated Scope

| Task | Priority | Files touched | Complexity |
|------|----------|---------------|------------|
| 0. Entity SQL paging | P1 | 7 (core + TUI) | Medium — new store query, TUI refactor |
| 1. Message data layer | P1 | 6 (core) | Low — follows Task 0 pattern |
| 2. Detail pane | P1 | 4 (TUI) | Medium — new view, name resolution |
| 3. Filtering | P1 | 4 (TUI) | Medium — new filter state + bars |
| 4. Paging + sorting | P2 | 3 (TUI) | Low — follows Task 0 pattern |
| 5. Reply | P4 | 4+ (TUI) | High — text input is new territory |

## What NOT to Add

- **Message deletion** — no destructive operations in TUI
- **Mark as read from TUI** — could be a future enhancement but not in this scope
- **Thread view** — showing full reply chains as trees is complex; detail pane shows one level of `in_reply_to`
- **Real-time streaming** — auto-refresh every 5s is sufficient; `fl watch` is the streaming tool
- **Search** — FTS5 search is a CLI operation; TUI filters are sufficient
- **In-memory filtering** — all filtering, sorting, and paging is SQL-level via `ListMessagesRequest`
