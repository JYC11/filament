use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::TableState;

use filament_core::config::{FilamentConfig, OutputFormat};
use filament_core::connection::FilamentConnection;
use filament_core::dto::Escalation;
use filament_core::models::{AgentRun, Entity, EntityStatus, EntityType, Priority, Reservation};

use crate::views::analytics::AnalyticsData;
use crate::views::detail::DetailData;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Entities,
    Agents,
    Reservations,
    Messages,
    Config,
    Analytics,
}

impl Tab {
    pub const ALL: [Self; 6] = [
        Self::Entities,
        Self::Agents,
        Self::Reservations,
        Self::Messages,
        Self::Config,
        Self::Analytics,
    ];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Entities => Self::Agents,
            Self::Agents => Self::Reservations,
            Self::Reservations => Self::Messages,
            Self::Messages => Self::Config,
            Self::Config => Self::Analytics,
            Self::Analytics => Self::Entities,
        }
    }

    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Entities => Self::Analytics,
            Self::Agents => Self::Entities,
            Self::Reservations => Self::Agents,
            Self::Messages => Self::Reservations,
            Self::Config => Self::Messages,
            Self::Analytics => Self::Config,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Entities => "Entities",
            Self::Agents => "Agents",
            Self::Reservations => "Reservations",
            Self::Messages => "Messages",
            Self::Config => "Config",
            Self::Analytics => "Analytics",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Entities => 0,
            Self::Agents => 1,
            Self::Reservations => 2,
            Self::Messages => 3,
            Self::Config => 4,
            Self::Analytics => 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityRow {
    pub entity: Entity,
    pub blocked_by_count: usize,
    pub impact: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterBar {
    Type,
    Status,
    Priority,
    Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Priority,
    Status,
    Updated,
    Created,
    Impact,
}

impl SortField {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Priority => "priority",
            Self::Status => "status",
            Self::Updated => "updated",
            Self::Created => "created",
            Self::Impact => "impact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub const fn flip(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub const fn arrow(self) -> &'static str {
        match self {
            Self::Asc => "↑",
            Self::Desc => "↓",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SortState {
    pub field: SortField,
    pub direction: SortDirection,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            field: SortField::Priority,
            direction: SortDirection::Asc,
        }
    }
}

impl SortState {
    pub fn set_field(&mut self, field: SortField) {
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

pub struct App {
    pub conn: FilamentConnection,
    pub active_tab: Tab,
    pub should_quit: bool,
    pub entities: Vec<EntityRow>,
    pub agent_runs: Vec<AgentRun>,
    pub reservations: Vec<Reservation>,
    pub messages: Vec<Escalation>,
    pub entity_table_state: TableState,
    pub agent_table_state: TableState,
    pub reservation_table_state: TableState,
    pub message_table_state: TableState,
    pub filter: FilterState,
    pub sort: SortState,
    pub page: usize,
    pub page_size: usize,
    pub detail: Option<DetailData>,
    pub detail_scroll: u16,
    pub last_refresh: DateTime<Utc>,
    pub status_message: Option<String>,
    pub escalation_count: usize,
    pub config_rows: Vec<(String, String, String)>,
    pub config_table_state: TableState,
    pub analytics: AnalyticsData,
    pub agent_show_history: bool,
    pub has_cycle: bool,
    last_tick: Instant,
}

impl App {
    #[must_use]
    pub fn new(conn: FilamentConnection) -> Self {
        Self {
            conn,
            active_tab: Tab::Entities,
            should_quit: false,
            entities: Vec::new(),
            agent_runs: Vec::new(),
            reservations: Vec::new(),
            messages: Vec::new(),
            entity_table_state: TableState::default(),
            agent_table_state: TableState::default(),
            reservation_table_state: TableState::default(),
            message_table_state: TableState::default(),
            filter: FilterState::default(),
            sort: SortState::default(),
            page: 0,
            page_size: DEFAULT_PAGE_SIZE,
            detail: None,
            detail_scroll: 0,
            last_refresh: Utc::now(),
            status_message: None,
            escalation_count: 0,
            config_rows: Vec::new(),
            config_table_state: TableState::default(),
            analytics: AnalyticsData::default(),
            agent_show_history: false,
            has_cycle: false,
            last_tick: Instant::now(),
        }
    }

    pub fn should_auto_refresh(&self) -> bool {
        self.last_tick.elapsed() >= REFRESH_INTERVAL
    }

    pub async fn refresh_all(&mut self) {
        self.refresh_entities().await;
        self.refresh_agents().await;
        self.refresh_reservations().await;
        self.refresh_messages().await;
        self.refresh_health().await;
        self.last_refresh = Utc::now();
        self.last_tick = Instant::now();
    }

    pub async fn refresh_entities(&mut self) {
        if self.filter.ready_only {
            self.refresh_ready_tasks().await;
            return;
        }

        let type_filter = if self.filter.types.len() == 1 {
            self.filter.types.iter().next().copied()
        } else {
            None
        };

        let status_filter = if self.filter.statuses.len() == 1 {
            self.filter.statuses.iter().next().copied()
        } else {
            None
        };

        let result = self.conn.list_entities(type_filter, status_filter).await;

        match result {
            Ok(mut entities) => {
                // Apply multi-value filters that list_entities can't handle
                if self.filter.types.len() > 1 {
                    entities.retain(|e| self.filter.types.contains(&e.entity_type()));
                }
                if self.filter.statuses.len() > 1 {
                    entities.retain(|e| self.filter.statuses.contains(e.status()));
                }
                if !self.filter.priorities.is_empty() {
                    entities.retain(|e| self.filter.priorities.contains(&e.priority()));
                }

                self.build_entity_rows(entities).await;
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    async fn refresh_ready_tasks(&mut self) {
        match self.conn.ready_tasks().await {
            Ok(mut entities) => {
                if !self.filter.priorities.is_empty() {
                    entities.retain(|e| self.filter.priorities.contains(&e.priority()));
                }
                self.build_entity_rows(entities).await;
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    async fn build_entity_rows(&mut self, entities: Vec<Entity>) {
        let entity_count = entities.len();
        let use_impact = entity_count <= 50;

        let blocked_counts = self.conn.blocked_by_counts().await.unwrap_or_default();

        let entity_ids: Vec<String> = if use_impact {
            entities
                .iter()
                .map(|e| e.id().as_str().to_string())
                .collect()
        } else {
            Vec::new()
        };
        let impact_scores = if use_impact {
            self.conn
                .batch_impact_scores(&entity_ids)
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

        self.sort_rows(&mut rows);
        self.entities = rows;
        self.clamp_entity_selection();
    }

    pub async fn refresh_agents(&mut self) {
        let result = if self.agent_show_history {
            self.conn.list_all_agent_runs(100).await
        } else {
            self.conn.list_running_agents().await
        };
        match result {
            Ok(runs) => {
                self.agent_runs = runs;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub async fn refresh_reservations(&mut self) {
        match self.conn.list_reservations(None).await {
            Ok(res) => {
                self.reservations = res;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    pub fn load_config(&mut self, project_root: Option<&std::path::Path>) {
        let config = project_root.map_or_else(FilamentConfig::default, FilamentConfig::load);
        let output_fmt = match config.output_format {
            Some(OutputFormat::Json) => "json",
            Some(OutputFormat::Text) | None => "text",
        };

        self.config_rows = vec![
            (
                "default_priority".to_string(),
                config.resolve_default_priority().to_string(),
                source_label(config.default_priority.is_some()),
            ),
            (
                "output_format".to_string(),
                output_fmt.to_string(),
                source_label(config.output_format.is_some()),
            ),
            (
                "agent_command".to_string(),
                config.resolve_agent_command(),
                source_label(config.agent_command.is_some()),
            ),
            (
                "auto_dispatch".to_string(),
                config.resolve_auto_dispatch().to_string(),
                source_label(config.auto_dispatch.is_some()),
            ),
            (
                "context_depth".to_string(),
                config.resolve_context_depth().to_string(),
                source_label(config.context_depth.is_some()),
            ),
            (
                "max_auto_dispatch".to_string(),
                config.resolve_max_auto_dispatch().to_string(),
                source_label(config.max_auto_dispatch.is_some()),
            ),
            (
                "cleanup_interval_secs".to_string(),
                config.resolve_cleanup_interval_secs().to_string(),
                source_label(config.cleanup_interval_secs.is_some()),
            ),
            (
                "idle_timeout_secs".to_string(),
                config.resolve_idle_timeout_secs().to_string(),
                source_label(config.idle_timeout_secs.is_some()),
            ),
        ];
    }

    pub async fn refresh_analytics(&mut self) {
        // Single batch query for name lookup instead of N individual get_entity calls
        let name_map: HashMap<String, String> = self
            .conn
            .list_entities(None, None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.common().id.to_string(), e.name().to_string()))
            .collect();

        let mut pagerank_data = Vec::new();
        if let Ok(scores) = self.conn.pagerank(None, None).await {
            for (id, score) in &scores {
                let name = name_map
                    .get(id.as_str())
                    .cloned()
                    .unwrap_or_else(|| id.to_string());
                pagerank_data.push((id.clone(), name, *score));
            }
            pagerank_data
                .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            pagerank_data.truncate(20);
        }

        let mut degree_data = Vec::new();
        if let Ok(degrees) = self.conn.degree_centrality().await {
            for (id, (in_deg, out_deg, total)) in &degrees {
                let name = name_map
                    .get(id.as_str())
                    .cloned()
                    .unwrap_or_else(|| id.to_string());
                degree_data.push((id.clone(), name, *in_deg, *out_deg, *total));
            }
            degree_data.sort_by(|a, b| b.4.cmp(&a.4));
            degree_data.truncate(20);
        }

        self.analytics = AnalyticsData {
            pagerank: pagerank_data,
            degree: degree_data,
            calculated: true,
        };
    }

    pub async fn refresh_messages(&mut self) {
        if let Ok(items) = self.conn.list_pending_escalations().await {
            self.escalation_count = items.len();
            self.messages = items;
        } else {
            self.escalation_count = 0;
            self.messages = Vec::new();
        }
    }

    pub async fn refresh_health(&mut self) {
        self.has_cycle = self.conn.check_cycle().await.unwrap_or(false);
    }

    pub fn select_next(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let state = self.current_table_state_mut();
        let i = state
            .selected()
            .map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 });
        state.select(Some(i));
    }

    pub fn select_prev(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let state = self.current_table_state_mut();
        let i = state
            .selected()
            .map_or(0, |i| if i == 0 { len - 1 } else { i - 1 });
        state.select(Some(i));
    }

    fn current_list_len(&self) -> usize {
        match self.active_tab {
            Tab::Entities => self.visible_entities().len(),
            Tab::Agents => self.agent_runs.len(),
            Tab::Reservations => self.reservations.len(),
            Tab::Messages => self.messages.len(),
            Tab::Config => self.config_rows.len(),
            Tab::Analytics => 0, // Analytics has no selectable rows
        }
    }

    const fn current_table_state_mut(&mut self) -> &mut TableState {
        match self.active_tab {
            Tab::Entities => &mut self.entity_table_state,
            Tab::Agents => &mut self.agent_table_state,
            Tab::Reservations => &mut self.reservation_table_state,
            Tab::Messages => &mut self.message_table_state,
            Tab::Config | Tab::Analytics => &mut self.config_table_state,
        }
    }

    fn sort_rows(&self, rows: &mut [EntityRow]) {
        let dir = self.sort.direction;
        rows.sort_by(|a, b| {
            let cmp = match self.sort.field {
                SortField::Name => a.entity.name().as_str().cmp(b.entity.name().as_str()),
                SortField::Priority => a
                    .entity
                    .priority()
                    .value()
                    .cmp(&b.entity.priority().value()),
                SortField::Status => a.entity.status().as_str().cmp(b.entity.status().as_str()),
                SortField::Updated => a
                    .entity
                    .common()
                    .updated_at
                    .cmp(&b.entity.common().updated_at),
                SortField::Created => a
                    .entity
                    .common()
                    .created_at
                    .cmp(&b.entity.common().created_at),
                SortField::Impact => a.impact.cmp(&b.impact),
            };
            match dir {
                SortDirection::Asc => cmp,
                SortDirection::Desc => cmp.reverse(),
            }
        });
    }

    fn clamp_entity_selection(&mut self) {
        let len = self.visible_entities().len();
        if let Some(idx) = self.entity_table_state.selected() {
            if len == 0 {
                self.entity_table_state.select(None);
            } else if idx >= len {
                self.entity_table_state.select(Some(len - 1));
            }
        }
    }

    pub fn visible_entities(&self) -> &[EntityRow] {
        let start = self.page * self.page_size;
        if start >= self.entities.len() {
            return &[];
        }
        let end = (start + self.page_size).min(self.entities.len());
        &self.entities[start..end]
    }

    pub const fn total_pages(&self) -> usize {
        if self.entities.is_empty() || self.page_size == 0 {
            return 1;
        }
        self.entities.len().div_ceil(self.page_size)
    }

    pub const fn has_next_page(&self) -> bool {
        self.page + 1 < self.total_pages()
    }

    pub const fn has_prev_page(&self) -> bool {
        self.page > 0
    }

    pub fn next_page(&mut self) {
        if self.has_next_page() {
            self.page += 1;
            self.entity_table_state.select(Some(0));
        }
    }

    pub fn prev_page(&mut self) {
        if self.has_prev_page() {
            self.page -= 1;
            self.entity_table_state.select(Some(0));
        }
    }

    pub fn reset_page(&mut self) {
        self.page = 0;
        self.entity_table_state.select(None);
    }

    pub async fn open_detail(&mut self) {
        if self.active_tab != Tab::Entities {
            return;
        }
        let Some(idx) = self.entity_table_state.selected() else {
            return;
        };
        let visible = self.visible_entities();
        let Some(row) = visible.get(idx) else {
            return;
        };

        let entity_id = row.entity.id().as_str().to_string();
        let entity = row.entity.clone();

        let relations = self
            .conn
            .list_relations(&entity_id)
            .await
            .unwrap_or_default();

        let events = self
            .conn
            .get_entity_events(&entity_id)
            .await
            .unwrap_or_default();

        let blocker_depth = if entity.entity_type() == EntityType::Task {
            self.conn.blocker_depth(&entity_id).await.unwrap_or(0)
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
            self.conn
                .batch_get_entities(&ref_ids)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id, e)| (id, format!("[{}] {}", e.slug(), e.name())))
                .collect()
        };

        self.detail = Some(DetailData {
            entity,
            relations,
            events,
            blocker_depth,
            name_map,
        });
        self.detail_scroll = 0;
    }

    pub fn close_detail(&mut self) {
        self.detail = None;
        self.detail_scroll = 0;
    }

    pub const fn has_detail(&self) -> bool {
        self.detail.is_some()
    }

    pub const fn scroll_detail_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(1);
    }

    pub const fn scroll_detail_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }
}

fn source_label(from_config: bool) -> String {
    if from_config {
        "config".to_string()
    } else {
        "default".to_string()
    }
}
