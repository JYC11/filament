# TUI Enhancement Epic

Status: Refined v5
Created: 2026-03-06
Moved from: `.plan/agent-hardening.md` Task 6 (TUI integration)
Reference: `~/Desktop/code/workout-util/src/db/pagination_support.rs` — QueryBuilder + keyset paging pattern

## Motivation

The TUI currently has 5 tabs (Tasks, Agents, Reservations, Messages, Graph) with minimal interactivity — single-value status cycle filter, task close, and no detail view. It shows only tasks (not all entity types), has no paging, and the graph view is a flat ASCII tree that doesn't properly visualize graph topology.

This epic makes the TUI a read-only power-user dashboard with multi-select filtering, detail views, and keyset paging. All mutations stay in the CLI. Graph visualization is deferred to a separate future epic.

---

## Filter Model (shared across Entity table)

All filters are **multi-select toggle sets**. Each filter is a `HashSet` of allowed values. Toggling a value adds/removes it from the set. An empty set means "show all" (no filter).

### Type Filter

- Backing type: `HashSet<EntityType>`
- Default: `{Task}` (shows tasks on startup)
- Key `t` opens a **filter bar** showing all types as toggleable chips:
  `[Task] Module Service Agent Plan Doc Lesson`
- While filter bar is visible, pressing `1`-`7` toggles individual types
- Press `t` again or `Esc` to dismiss
- Press `0` = clear all (show all types)

### Status Filter

- Backing type: `HashSet<EntityStatus>`
- Default: `{Open}` (shows open entities on startup)
- Key `f` opens filter bar: `[Open] InProgress Blocked Closed`
- `1`-`4` toggles individual statuses, `0` = clear all

### Priority Filter

- Backing type: `HashSet<Priority>`
- Default: empty (all priorities shown)
- Key `P` opens filter bar: `P0 P1 P2 P3 P4`
- `0`-`4` toggles individual priorities

### Ready-Only Filter

- Backing type: `bool`
- Default: `false`
- Key `F` toggles "ready only" mode
- When active: forces `types={Task}`, `statuses={Open}`, additionally excludes blocked tasks (uses `ready_tasks()` query which filters by unblocked + ordered by priority)
- Priority filter still composes on top (e.g., ready P0 tasks only)
- Type and status filter bars are disabled while ready-only is active (their values are implied)

### Key Binding Disambiguation

Number keys `1`-`6` serve dual purpose: **tab jump** (when no filter bar is open) vs **filter toggle** (when a filter bar is open). The `active_bar: Option<FilterBar>` state determines which behavior fires. When a filter bar is open, number keys are captured by the bar. Pressing `Esc` or the filter key again closes the bar, restoring number keys to tab navigation.

### Filter Display

Active filters shown in the tab title:

```
Entities [task,module | open,in_progress | P0,P1 | page →]
Entities [ready | P0]
```

### Implementation

```rust
struct FilterState {
    types: HashSet<EntityType>,       // empty = all
    statuses: HashSet<EntityStatus>,  // empty = all
    priorities: HashSet<Priority>,    // empty = all
    ready_only: bool,
    active_bar: Option<FilterBar>,    // which bar is open for toggling
}

enum FilterBar {
    Type,
    Status,
    Priority,
}
```

Changing any filter resets paging to page 1.

---

## Phase 1: Entity Table View — Filtering + Paging + Detail

### Task 1.1: Multi-Select Filter Model + Unified Entity Table

**Current:** Tasks tab shows only `EntityType::Task` with a single status cycle filter. `c` key closes tasks (write operation). Graph tab exists but is being removed.

**Target:** Replace the Tasks tab with a general Entities tab. Add multi-select type, status, and priority filters with toggleable filter bar UI. Add ready-only toggle. Remove write operations. Remove Graph tab.

**Design:**

- Implement `FilterState` as described in Filter Model section above
- Keys `t`/`f`/`P` open the respective filter bar; number keys toggle values
- Key `F` toggles ready-only mode
- **Remove** `close_selected_task()` from `App` and `c` key binding from `event.rs` (TUI is read-only)
- **Remove** `TaskFilter` struct (replaced by `FilterState`)
- **Remove** `Graph` variant from `Tab` enum, remove `views/graph.rs`, remove `graph_data` from `App`
- **Remove** `refresh_graph()` from `App` (graph view deferred to future epic)
- Columns adapt per active type filter:
  - Single type `Task`: Slug, Name, Status, Priority, Blocked, Impact
  - Single type `Lesson`: Slug, Name, Pattern, Status, Priority
  - Multiple/all types: Slug, Name, Type, Status, Priority
- Filter bar renders as a horizontal row of styled chips between tab bar and table
- Selected values shown in highlight color, unselected in dim

**Files:**
- `app.rs` — add `FilterState`, `FilterBar`, remove `TaskFilter`, `close_selected_task()`, `Graph` tab variant, `graph_data`, `refresh_graph()`
- `views/tasks.rs` → rename to `views/entities.rs`, adapt columns per filter
- `views/filter_bar.rs` — new view for rendering the filter bar chips
- `views/graph.rs` — **delete** (deferred to future epic)
- `views/mod.rs` — add `pub mod filter_bar`, rename `tasks` to `entities`, remove `graph`
- `event.rs` — add `t`/`f`/`P`/`F`, remove `c` key and `5` (Graph), number keys captured when bar open
- `ui.rs` — conditional filter bar row in layout, update tab label, remove Graph render branch

### Task 1.2: Keyset Paging (UUID-based)

**Current:** All entities loaded at once. Works for <100 but doesn't scale.

**Target:** Keyset pagination using UUIDv7 string ordering — load N entities at a time, page forward/backward. Adapted from `workout-util/src/db/pagination_support.rs`.

**Design:**

Follows the three-component pattern from workout-util. All pagination types live in `filament-core/src/pagination.rs` (new module).

#### Component 1: PaginationState (core module, used by TUI)

Adapted from `workout-util::PaginationState`. Changed cursor type from `u32` to `String` (UUIDv7).

```rust
// crates/filament-core/src/pagination.rs

pub struct PaginationState {
    pub limit: u32,                         // default 50
    pub next_cursor: Option<String>,        // UUID of last item → forward
    pub prev_cursor: Option<String>,        // UUID of first item → backward
    pub current_cursor: Option<String>,     // cursor used for current query
    pub direction: PaginationDirection,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PaginationDirection { Forward, Backward }

impl PaginationState {
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
}
```

- Keys: `n` = `go_forwards()` + refresh, `p` = `go_backwards()` + refresh
- Tab title shows: `[< page >]` with `<`/`>` dimmed when no prev/next
- When `ready_only` is active, paging is disabled (ready tasks are typically few)
- Changing any filter calls `pagination.reset()`

#### Component 2: keyset_paginate() (store-side helper)

Adapted from `workout-util::keyset_paginate()`. Uses UUIDv7 string comparison instead of integer ID.

```rust
// crates/filament-core/src/pagination.rs

pub struct PaginationParams {
    pub limit: u32,
    pub cursor: Option<String>,
    pub direction: PaginationDirection,
}

pub(crate) fn keyset_paginate(
    params: &PaginationParams,
    qb: &mut QueryBuilder<Sqlite>,
) {
    match params.direction {
        PaginationDirection::Forward => {
            if let Some(ref cursor) = params.cursor {
                qb.push(" AND id > ");
                qb.push_bind(cursor.as_str());
            }
            qb.push(" ORDER BY id ASC LIMIT ");
        }
        PaginationDirection::Backward => {
            if let Some(ref cursor) = params.cursor {
                qb.push(" AND id < ");
                qb.push_bind(cursor.as_str());
            }
            qb.push(" ORDER BY id DESC LIMIT ");
        }
    }
    qb.push_bind(params.limit + 1); // +1 to detect has_more
}
```

#### Component 3: get_cursors() (store-side helper)

Adapted from `workout-util::get_cursors()`. Uses entity ID (UUIDv7 string) instead of integer.

```rust
// crates/filament-core/src/pagination.rs

pub trait Paginatable {
    fn cursor_id(&self) -> &str;
}

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

#### Component 4: Dynamic filter query builder (store-side)

Uses the `QueryBuilder` + `separated()` pattern from `workout-util::pagination_filters()`.

```rust
// crates/filament-core/src/store.rs (or pagination.rs)

pub struct EntityPageRequest {
    pub types: Vec<EntityType>,
    pub statuses: Vec<EntityStatus>,
    pub priorities: Vec<Priority>,
    pub pagination: PaginationParams,
}

fn build_entity_page_query(req: &EntityPageRequest) -> QueryBuilder<'_, Sqlite> {
    let mut qb = QueryBuilder::new("SELECT * FROM entities WHERE 1=1");

    if !req.types.is_empty() {
        qb.push(" AND entity_type IN (");
        let mut sep = qb.separated(", ");
        for t in &req.types { sep.push_bind(t.as_str()); }
        sep.push_unseparated(")");
    }

    if !req.statuses.is_empty() {
        qb.push(" AND status IN (");
        let mut sep = qb.separated(", ");
        for s in &req.statuses { sep.push_bind(s.as_str()); }
        sep.push_unseparated(")");
    }

    if !req.priorities.is_empty() {
        qb.push(" AND priority IN (");
        let mut sep = qb.separated(", ");
        for p in &req.priorities { sep.push_bind(p.value()); }
        sep.push_unseparated(")");
    }

    keyset_paginate(&req.pagination, &mut qb);
    qb
}
```

**Files:**
- `crates/filament-core/src/pagination.rs` — **new module**: `PaginationState`, `PaginationParams`, `PaginationDirection`, `Paginatable` trait, `keyset_paginate()`, `get_cursors()`
- `crates/filament-core/src/lib.rs` — add `pub mod pagination`
- `crates/filament-core/src/dto.rs` — add `EntityPageRequest`
- `crates/filament-core/src/store.rs` — add `list_entities_paged()` using `build_entity_page_query`
- `crates/filament-core/src/connection.rs` — expose paged method
- `crates/filament-tui/src/app.rs` — add `PaginationState` field, wire `n`/`p` keys
- `crates/filament-tui/src/event.rs` — `n`/`p` keys
- `crates/filament-tui/src/ui.rs` — page cursor indicators in tab title

### Task 1.3: Detail Pane with Events + Critical Path

**Current:** No way to see entity details (summary, key_facts, content) in the TUI.

**Target:** Press `Enter` on a selected entity to show a read-only detail pane below the table (60/40 vertical split). Includes event history and critical path.

**Design:**

- Add to `App`:
  - `detail_entity: Option<Entity>`
  - `detail_relations: Vec<Relation>`
  - `detail_events: Vec<Event>`
  - `detail_critical_path: Vec<EntityId>`
  - `detail_scroll: u16`
  - `show_detail: bool`
- When `Enter` is pressed on a selected row, fetch:
  - Entity: `conn.get_entity(id)`
  - Relations: `conn.list_relations(id)`
  - Events: `conn.get_entity_events(id)`
  - Critical path: `conn.critical_path(id)` (for tasks only, skip for other types)
- Split the content area: top = table (60%), bottom = detail pane (40%)
- Detail pane sections:
  1. **Header**: `[slug] Name (type, status, P2)`
  2. **Summary** (wrapped text)
  3. **Key facts** (JSON pretty-printed, or for Lessons: Problem/Solution/Learned)
  4. **Critical path** (tasks only): chain of slugs → names
  5. **Relations**: list of (type, direction, target slug, target name)
  6. **Event history**: chronological list (timestamp, event_type, diff summary)
- Press `Esc` to close detail pane
- `j`/`k` in detail mode scroll the detail text (table navigation disabled while detail is open)

**Files:**
- `app.rs` — add detail fields, `open_detail()` async method, `close_detail()` method
- `views/detail.rs` — new view for entity detail rendering (sections 1-6)
- `views/mod.rs` — add `pub mod detail`
- `event.rs` — `Enter` opens detail, `Esc` closes, `j`/`k` scroll in detail mode
- `ui.rs` — conditional split layout when detail is open

---

## Phase 2: Informational Tabs + Enhancements

### Task 2.1: Config Tab

Read-only display of resolved configuration values.

```
┌─ Config ──────────────────────────────────────────┐
│ Key                     Value          Source      │
│ ───────────────────────────────────────────────── │
│ default_priority        2              default     │
│ agent_command           claude         default     │
│ auto_dispatch           true           env         │
│ cleanup_interval_secs   60             default     │
└───────────────────────────────────────────────────┘
```

- Data source: `FilamentConfig::load()` — load once on startup, no refresh
- New Tab variant: `Config`

**Files:**
- `app.rs` — extend Tab enum, add `config_data: Vec<(String, String, String)>` (key, value, source)
- `views/config.rs` — new view, simple `Table` widget
- `views/mod.rs` — add module
- `event.rs` — key `5`
- `ui.rs` — render branch

### Task 2.2: Analytics Tab

Combined PageRank + degree centrality in a split view.

```
┌─ Analytics ───────────────────────────────────────┐
│ PageRank (damping=0.85)                           │
│ ─────────────────────                             │
│ filament-core            0.142356                 │
│                                                   │
│ Degree Centrality                                 │
│ ─────────────────                                 │
│ Name                     In   Out  Total          │
│ filament-core             8     3     11          │
└───────────────────────────────────────────────────┘
```

- Data source: `conn.pagerank()` and `conn.degree_centrality()`
- Refresh on manual `r` only (expensive graph computation, not on auto-tick)
- New Tab variant: `Analytics`

**Files:**
- `app.rs` — extend Tab enum, add `pagerank_data`, `degree_data` fields
- `views/analytics.rs` — new view, two `Table` widgets in vertical split
- `views/mod.rs` — add module
- `event.rs` — key `6`
- `ui.rs` — render branch

### Task 2.3: Agent History Toggle

**Current:** Agents tab only shows running agents (`list_running_agents`).

**Target:** Toggle between "running only" and "all history" views.

**Design:**

- Key `h` on Agents tab toggles between:
  - Running agents (default, current behavior via `list_running_agents`)
  - All agent runs — needs new `list_all_agent_runs(limit)` store method
- History view columns: Run ID, Task Slug, Role, Status, Duration, Started At
- Completed/failed runs shown with dim styling
- Tab title: `Agents [running]` or `Agents [history]`

**Files:**
- `app.rs` — add `agent_show_history: bool`, `agent_history: Vec<AgentRun>`
- `crates/filament-core/src/store.rs` — add `list_all_agent_runs(limit: u32)`
- `crates/filament-core/src/connection.rs` — expose new method
- `views/agents.rs` — adapt columns for history view, dim styling for non-running
- `event.rs` — `h` key toggles

### Task 2.4: Health Indicator in Status Bar

**Current:** Status bar shows connection mode, refresh time, escalation count.

**Target:** Add entity/relation counts and cycle detection warning.

**Design:**

- On each refresh, compute:
  - `entity_count: usize` — from current page data (cheap)
- On **manual refresh only** (`r` key), compute:
  - `has_cycle: bool` via `conn.check_cycle()` (expensive — traverses full graph)
- Display in status bar:
  - Normal: `[direct] 42 entities ✓ refreshed 14:30:05`
  - Cycle: `[direct] 42 entities ⚠ cycle refreshed 14:30:05`
- Cycle warning in red/bold

**Files:**
- `app.rs` — add `has_cycle`, `entity_count` fields
- `ui.rs` — update `draw_status_bar` to include health info

---

## Task Dependencies

```
Phase 1 (Entity Table):
  1.1 Multi-Select Filters + Entity Table ─── no deps
  1.2 Keyset Paging ───────────────────────── depends on 1.1
  1.3 Detail Pane + Events + Critical Path ── depends on 1.1

Phase 2 (Info Tabs + Enhancements):
  2.1 Config Tab ──────────── no deps
  2.2 Analytics Tab ───────── no deps
  2.3 Agent History Toggle ── no deps
  2.4 Health Indicator ────── no deps
```

**Execution order:**
1. Task 1.1 first (foundation — filters, entity table, remove writes, remove graph)
2. Tasks 1.2, 1.3, 2.1-2.4 all in parallel (all independent after 1.1)

## Updated Tab Bar

| Key | Tab | Type |
|-----|-----|------|
| `1` | Entities (was Tasks) | Table + multi-select filters + paging + detail |
| `2` | Agents | Table + history toggle |
| `3` | Reservations | Table (read-only) |
| `4` | Messages | Table (read-only) |
| `5` | Config | Read-only |
| `6` | Analytics | Read-only |

6 tabs. Lessons are viewed via Entity tab with `type_filter = {Lesson}` + detail pane (shows Problem/Solution/Learned from key_facts). Graph visualization deferred to a separate future epic.

## Key Bindings Summary

| Key | Context | Action |
|-----|---------|--------|
| `q` | Global | Quit |
| `Ctrl+c` | Global | Quit |
| `Tab`/`BackTab` | Global | Next/prev tab |
| `1`-`6` | Global (no filter bar open) | Jump to tab |
| `r` | Global | Manual refresh (also recomputes cycle check) |
| `j`/`k`/`↓`/`↑` | Table | Navigate rows |
| `j`/`k`/`↓`/`↑` | Detail | Scroll viewport |
| `t` | Entities tab | Open type filter bar |
| `f` | Entities tab | Open status filter bar |
| `P` | Entities tab | Open priority filter bar |
| `F` | Entities tab | Toggle ready-only mode |
| `1`-`7` / `0`-`4` | Filter bar open | Toggle filter value / clear all |
| `Esc` | Filter bar / Detail | Close overlay |
| `Enter` | Entities tab | Open detail pane |
| `n` | Entities tab | Next page |
| `p` | Entities tab | Previous page |
| `h` | Agents tab | Toggle running / history |

## What NOT to Add

- **Write operations** — all mutations (create, update, delete, close) stay in the CLI. TUI is read-only.
- **Text input** — no create/edit forms, no search box.
- **Graph view** — removed from this epic. Deferred to a separate future epic with proper design for terminal graph visualization (layered DAG, Canvas-based, or hybrid approach).
- **Separate Lessons tab** — covered by Entity tab with type filter + detail pane.
- **Watch** — streaming CLI command, doesn't fit dashboard model.
- **Seed/Audit** — one-shot operations, stay in CLI.

## Future: Graph View Epic (separate)

The current ASCII tree graph view will be removed in Task 1.1. A proper graph visualization deserves its own epic with dedicated design. Options to explore:

- Layered DAG (Sugiyama-style) with topological sort + layer assignment
- Hybrid layered list with depth indicators and cross-references
- Canvas-based box-and-line rendering (ratatui Canvas widget)
- Enhanced tree with back-reference annotations for multi-parent nodes

This is a non-trivial UI/UX problem that should be designed separately from the dashboard enhancement work.
