use std::collections::HashSet;

use filament_core::connection::FilamentConnection;
use filament_core::dto::{EntitySortField, ListEntitiesRequest, SortDirection};
use filament_core::models::{Entity, EntityStatus, EntityType, Priority};
use filament_core::pagination::PaginationState;
use ratatui::widgets::TableState;

use crate::views::detail::DetailData;

// ---------------------------------------------------------------------------
// Entity row (enriched for display)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EntityRow {
    pub entity: Entity,
    pub blocked_by_count: usize,
    pub impact: usize,
}

// ---------------------------------------------------------------------------
// Entity filter state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterBar {
    Type,
    Status,
    Priority,
    Sort,
}

#[derive(Debug, Clone)]
pub struct FilterState {
    pub types: HashSet<EntityType>,
    pub statuses: HashSet<EntityStatus>,
    pub priorities: HashSet<Priority>,
    pub ready_only: bool,
    pub active_bar: Option<FilterBar>,
}

impl Default for FilterState {
    fn default() -> Self {
        let mut types = HashSet::new();
        types.insert(EntityType::Task);
        let mut statuses = HashSet::new();
        statuses.insert(EntityStatus::Open);
        Self {
            types,
            statuses,
            priorities: HashSet::new(),
            ready_only: false,
            active_bar: None,
        }
    }
}

impl FilterState {
    pub fn toggle_type(&mut self, t: EntityType) {
        if !self.types.remove(&t) {
            self.types.insert(t);
        }
    }

    pub fn toggle_status(&mut self, s: EntityStatus) {
        if !self.statuses.remove(&s) {
            self.statuses.insert(s);
        }
    }

    pub fn toggle_priority(&mut self, p: Priority) {
        if !self.priorities.remove(&p) {
            self.priorities.insert(p);
        }
    }

    pub fn clear_types(&mut self) {
        self.types.clear();
    }

    pub fn clear_statuses(&mut self) {
        self.statuses.clear();
    }

    pub fn clear_priorities(&mut self) {
        self.priorities.clear();
    }

    pub const fn toggle_ready_only(&mut self) {
        self.ready_only = !self.ready_only;
        if self.ready_only {
            self.active_bar = None;
        }
    }

    pub fn is_single_type(&self, t: EntityType) -> bool {
        self.types.len() == 1 && self.types.contains(&t)
    }

    pub fn label(&self) -> String {
        if self.ready_only {
            let mut parts = vec!["ready".to_string()];
            if !self.priorities.is_empty() {
                let p = self.priority_label();
                parts.push(p);
            }
            return parts.join(" | ");
        }

        let mut parts = Vec::new();

        if !self.types.is_empty() {
            parts.push(self.type_label());
        }
        if !self.statuses.is_empty() {
            parts.push(self.status_label());
        }
        if !self.priorities.is_empty() {
            parts.push(self.priority_label());
        }

        if parts.is_empty() {
            "all".to_string()
        } else {
            parts.join(" | ")
        }
    }

    fn type_label(&self) -> String {
        let mut types: Vec<&str> = self.types.iter().map(EntityType::as_str).collect();
        types.sort_unstable();
        types.join(",")
    }

    fn status_label(&self) -> String {
        let mut statuses: Vec<&str> = self.statuses.iter().map(EntityStatus::as_str).collect();
        statuses.sort_unstable();
        statuses.join(",")
    }

    fn priority_label(&self) -> String {
        let mut pris: Vec<String> = self
            .priorities
            .iter()
            .map(|p| format!("P{}", p.value()))
            .collect();
        pris.sort_unstable();
        pris.join(",")
    }
}

// ---------------------------------------------------------------------------
// Entity sort state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct SortState {
    pub field: EntitySortField,
    pub direction: SortDirection,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            field: EntitySortField::Priority,
            direction: SortDirection::Asc,
        }
    }
}

impl SortState {
    pub fn set_field(&mut self, field: EntitySortField) {
        if self.field == field {
            self.direction = self.direction.flip();
        } else {
            self.field = field;
            self.direction = SortDirection::Asc;
        }
    }

    pub fn label(&self) -> String {
        format!("{}{}", self.field.label(), self.direction.arrow())
    }
}

// ---------------------------------------------------------------------------
// Entity state operations (methods on App via free functions)
// ---------------------------------------------------------------------------

/// Build enriched entity rows with blocked-by counts and impact scores.
pub async fn build_entity_rows(
    conn: &mut FilamentConnection,
    entities: Vec<Entity>,
) -> Vec<EntityRow> {
    let entity_count = entities.len();
    let use_impact = entity_count <= 50;

    let blocked_counts = conn.blocked_by_counts().await.unwrap_or_default();

    let entity_ids: Vec<String> = if use_impact {
        entities
            .iter()
            .map(|e| e.id().as_str().to_string())
            .collect()
    } else {
        Vec::new()
    };
    let impact_scores = if use_impact {
        conn.batch_impact_scores(&entity_ids)
            .await
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    let mut rows = Vec::with_capacity(entity_count);
    for entity in entities {
        let entity_id = entity.id().as_str();
        rows.push(EntityRow {
            blocked_by_count: blocked_counts.get(entity_id).copied().unwrap_or(0),
            impact: impact_scores.get(entity_id).copied().unwrap_or(0),
            entity,
        });
    }
    rows
}

/// Build a `ListEntitiesRequest` from filter/sort/pagination state.
pub fn build_entity_request(
    filter: &FilterState,
    sort: SortState,
    pagination: &PaginationState,
) -> ListEntitiesRequest {
    ListEntitiesRequest {
        types: filter.types.iter().copied().collect(),
        statuses: filter.statuses.iter().copied().collect(),
        priorities: filter.priorities.iter().copied().collect(),
        sort_field: sort.field,
        sort_direction: sort.direction,
        pagination: pagination.to_params(),
    }
}

/// In-memory sort for the `ready_only` path (bypasses SQL paging).
pub fn sort_entities_in_place(entities: &mut [EntityRow], sort: SortState) {
    entities.sort_by(|a, b| {
        let cmp = match sort.field {
            EntitySortField::Name => a.entity.name().as_str().cmp(b.entity.name().as_str()),
            EntitySortField::Priority => a
                .entity
                .priority()
                .value()
                .cmp(&b.entity.priority().value()),
            EntitySortField::Status => a.entity.status().as_str().cmp(b.entity.status().as_str()),
            EntitySortField::Updated => a
                .entity
                .common()
                .updated_at
                .cmp(&b.entity.common().updated_at),
            EntitySortField::Created => a
                .entity
                .common()
                .created_at
                .cmp(&b.entity.common().created_at),
        };
        match sort.direction {
            SortDirection::Asc => cmp,
            SortDirection::Desc => cmp.reverse(),
        }
    });
}

/// Clamp table selection to stay within bounds after data changes.
pub fn clamp_selection(table_state: &mut TableState, len: usize) {
    if let Some(idx) = table_state.selected() {
        if len == 0 {
            table_state.select(None);
        } else if idx >= len {
            table_state.select(Some(len - 1));
        }
    }
}

/// Open entity detail pane — fetches relations, events, blocker depth, and name map.
pub async fn open_detail(
    conn: &mut FilamentConnection,
    entities: &[EntityRow],
    entity_table_state: &TableState,
) -> Option<DetailData> {
    let idx = entity_table_state.selected()?;
    let row = entities.get(idx)?;

    let entity_id = row.entity.id().as_str().to_string();
    let entity = row.entity.clone();

    let relations = conn.list_relations(&entity_id).await.unwrap_or_default();

    let events = conn.get_entity_events(&entity_id).await.unwrap_or_default();

    let blocker_depth = if entity.entity_type() == EntityType::Task {
        conn.blocker_depth(&entity_id).await.unwrap_or(0)
    } else {
        0
    };

    // Collect all referenced entity IDs for batch name resolution
    let mut ref_ids: Vec<String> = Vec::new();
    for rel in &relations {
        if rel.source_id.as_str() != entity_id {
            ref_ids.push(rel.source_id.to_string());
        }
        if rel.target_id.as_str() != entity_id {
            ref_ids.push(rel.target_id.to_string());
        }
    }
    ref_ids.sort_unstable();
    ref_ids.dedup();

    let name_map = if ref_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        conn.batch_get_entities(&ref_ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, e)| (id, format!("[{}] {}", e.slug(), e.name())))
            .collect()
    };

    Some(DetailData {
        entity,
        relations,
        events,
        blocker_depth,
        name_map,
    })
}
