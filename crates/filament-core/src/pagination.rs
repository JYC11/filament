use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite};

use crate::dto::SortDirection;

// ---------------------------------------------------------------------------
// Pagination direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaginationDirection {
    Forward,
    Backward,
}

// ---------------------------------------------------------------------------
// Pagination params (passed to store queries)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub limit: u32,
    pub cursor: Option<String>,
    pub direction: PaginationDirection,
}

// ---------------------------------------------------------------------------
// Pagination state (TUI-side cursor tracking)
// ---------------------------------------------------------------------------

pub struct PaginationState {
    pub limit: u32,
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
    current_cursor: Option<String>,
    direction: PaginationDirection,
}

impl PaginationState {
    #[must_use]
    pub const fn new(limit: u32) -> Self {
        Self {
            limit,
            next_cursor: None,
            prev_cursor: None,
            current_cursor: None,
            direction: PaginationDirection::Forward,
        }
    }

    pub fn reset(&mut self) {
        self.next_cursor = None;
        self.prev_cursor = None;
        self.current_cursor = None;
        self.direction = PaginationDirection::Forward;
    }

    #[must_use]
    pub const fn has_next(&self) -> bool {
        self.next_cursor.is_some()
    }

    #[must_use]
    pub const fn has_previous(&self) -> bool {
        self.prev_cursor.is_some()
    }

    pub fn go_forwards(&mut self) {
        self.current_cursor = self.next_cursor.clone();
        self.direction = PaginationDirection::Forward;
    }

    pub fn go_backwards(&mut self) {
        self.current_cursor = self.prev_cursor.clone();
        self.direction = PaginationDirection::Backward;
    }

    #[must_use]
    pub fn to_params(&self) -> PaginationParams {
        PaginationParams {
            limit: self.limit,
            cursor: self.current_cursor.clone(),
            direction: self.direction,
        }
    }

    pub fn update_cursors(&mut self, next: Option<String>, prev: Option<String>) {
        self.next_cursor = next;
        self.prev_cursor = prev;
    }
}

// ---------------------------------------------------------------------------
// Paged result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Paginatable trait (for cursor extraction)
// ---------------------------------------------------------------------------

pub(crate) trait Paginatable {
    fn cursor_id(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Keyset pagination query builder
// ---------------------------------------------------------------------------

/// Appends keyset pagination clauses to an existing `QueryBuilder`.
///
/// Uses `SQLite` tuple comparison `(sort_col, id) > (SELECT ...)` for cursor-based
/// paging with a custom sort column and id tiebreaker.
///
/// The cursor is just the id — the sort value at the cursor point is resolved
/// via subquery, so no composite cursor serialization is needed.
pub(crate) fn keyset_paginate(
    params: &PaginationParams,
    qb: &mut QueryBuilder<'_, Sqlite>,
    table: &str,
    sort_column: &str,
    sort_direction: SortDirection,
    id_column: &str,
) {
    // Determine comparison operator and ORDER BY direction based on
    // pagination direction and sort direction.
    //
    // Forward + ASC:  rows after cursor → (col, id) > cursor_pos, ORDER ASC
    // Forward + DESC: rows after cursor → (col, id) < cursor_pos, ORDER DESC
    // Backward + ASC: rows before cursor → (col, id) < cursor_pos, ORDER DESC (reversed)
    // Backward + DESC: rows before cursor → (col, id) > cursor_pos, ORDER ASC (reversed)
    // Same-direction = scan forward with ">"; cross-direction = scan backward with "<".
    let same_direction = params.direction == PaginationDirection::Forward
        && sort_direction == SortDirection::Asc
        || params.direction == PaginationDirection::Backward
            && sort_direction == SortDirection::Desc;

    let (op, order_sort, order_id) = if same_direction {
        (">", "ASC", "ASC")
    } else {
        ("<", "DESC", "DESC")
    };

    if let Some(ref cursor) = params.cursor {
        qb.push(format!(
            " AND ({sort_column}, {id_column}) {op} \
             (SELECT {sort_column}, {id_column} FROM {table} WHERE {id_column} = "
        ));
        qb.push_bind(cursor.clone());
        qb.push(")");
    }

    qb.push(format!(
        " ORDER BY {sort_column} {order_sort}, {id_column} {order_id}"
    ));
    // Fetch one extra row to detect whether more pages exist.
    qb.push(format!(" LIMIT {}", params.limit + 1));
}

// ---------------------------------------------------------------------------
// Cursor extraction
// ---------------------------------------------------------------------------

/// Extracts next/prev cursors from a fetched result set.
///
/// Mutates `rows` in place: pops the extra row if `has_more`, reverses if backward.
/// Returns `(next_cursor, prev_cursor)`.
pub(crate) fn get_cursors<T: Paginatable>(
    params: &PaginationParams,
    rows: &mut Vec<T>,
) -> (Option<String>, Option<String>) {
    let has_more = rows.len() > params.limit as usize;
    if has_more {
        rows.pop();
    }

    if matches!(params.direction, PaginationDirection::Backward) {
        rows.reverse();
    }

    let start_id = rows.first().map(|r| r.cursor_id().to_string());
    let end_id = rows.last().map(|r| r.cursor_id().to_string());

    match params.direction {
        PaginationDirection::Forward => {
            let next = if has_more { end_id } else { None };
            let prev = if params.cursor.is_some() {
                start_id
            } else {
                None
            };
            (next, prev)
        }
        PaginationDirection::Backward => {
            let next = end_id;
            let prev = if has_more { start_id } else { None };
            (next, prev)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRow {
        id: String,
    }

    impl Paginatable for TestRow {
        fn cursor_id(&self) -> &str {
            &self.id
        }
    }

    fn make_rows(ids: &[&str]) -> Vec<TestRow> {
        ids.iter()
            .map(|id| TestRow { id: id.to_string() })
            .collect()
    }

    #[test]
    fn forward_first_page_no_more() {
        let params = PaginationParams {
            limit: 3,
            cursor: None,
            direction: PaginationDirection::Forward,
        };
        let mut rows = make_rows(&["a", "b", "c"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 3);
        assert!(next.is_none(), "no more pages");
        assert!(prev.is_none(), "no previous on first page");
    }

    #[test]
    fn forward_first_page_has_more() {
        let params = PaginationParams {
            limit: 3,
            cursor: None,
            direction: PaginationDirection::Forward,
        };
        // 4 rows = 3 + 1 extra
        let mut rows = make_rows(&["a", "b", "c", "d"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 3);
        assert_eq!(next, Some("c".to_string()), "cursor at last visible row");
        assert!(prev.is_none(), "no previous on first page");
    }

    #[test]
    fn forward_middle_page() {
        let params = PaginationParams {
            limit: 3,
            cursor: Some("c".to_string()),
            direction: PaginationDirection::Forward,
        };
        let mut rows = make_rows(&["d", "e", "f", "g"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 3);
        assert_eq!(next, Some("f".to_string()));
        assert_eq!(prev, Some("d".to_string()));
    }

    #[test]
    fn forward_last_page() {
        let params = PaginationParams {
            limit: 3,
            cursor: Some("f".to_string()),
            direction: PaginationDirection::Forward,
        };
        let mut rows = make_rows(&["g", "h"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 2);
        assert!(next.is_none(), "no more pages");
        assert_eq!(prev, Some("g".to_string()));
    }

    #[test]
    fn backward_reverses_rows() {
        let params = PaginationParams {
            limit: 3,
            cursor: Some("g".to_string()),
            direction: PaginationDirection::Backward,
        };
        // Backward fetch returns rows in reverse order
        let mut rows = make_rows(&["f", "e", "d", "c"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 3);
        // After reverse: d, e, f
        assert_eq!(rows[0].id, "d");
        assert_eq!(rows[2].id, "f");
        assert_eq!(next, Some("f".to_string()));
        assert_eq!(prev, Some("d".to_string()));
    }

    #[test]
    fn backward_first_page_reached() {
        let params = PaginationParams {
            limit: 3,
            cursor: Some("d".to_string()),
            direction: PaginationDirection::Backward,
        };
        let mut rows = make_rows(&["c", "b", "a"]);
        let (next, prev) = get_cursors(&params, &mut rows);
        assert_eq!(rows.len(), 3);
        // After reverse: a, b, c
        assert_eq!(rows[0].id, "a");
        assert_eq!(next, Some("c".to_string()));
        assert!(prev.is_none(), "reached the beginning");
    }

    #[test]
    fn empty_result_set() {
        let params = PaginationParams {
            limit: 3,
            cursor: None,
            direction: PaginationDirection::Forward,
        };
        let mut rows: Vec<TestRow> = Vec::new();
        let (next, prev) = get_cursors(&params, &mut rows);
        assert!(next.is_none());
        assert!(prev.is_none());
    }

    #[test]
    fn pagination_state_lifecycle() {
        let mut state = PaginationState::new(50);
        assert!(!state.has_next());
        assert!(!state.has_previous());

        // Simulate first page result with more available
        state.update_cursors(Some("cursor_a".to_string()), None);
        assert!(state.has_next());
        assert!(!state.has_previous());

        // Go forward
        state.go_forwards();
        let params = state.to_params();
        assert_eq!(params.cursor, Some("cursor_a".to_string()));
        assert_eq!(params.direction, PaginationDirection::Forward);

        // Simulate middle page result
        state.update_cursors(Some("cursor_b".to_string()), Some("cursor_a".to_string()));
        assert!(state.has_next());
        assert!(state.has_previous());

        // Go backward
        state.go_backwards();
        let params = state.to_params();
        assert_eq!(params.cursor, Some("cursor_a".to_string()));
        assert_eq!(params.direction, PaginationDirection::Backward);

        // Reset
        state.reset();
        assert!(!state.has_next());
        assert!(!state.has_previous());
        let params = state.to_params();
        assert!(params.cursor.is_none());
    }
}
