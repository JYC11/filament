use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::TableState;

use filament_core::config::{FilamentConfig, OutputFormat};
use filament_core::connection::FilamentConnection;
use filament_core::dto::{
    EntitySortField, Escalation, ListEntitiesRequest, ListMessagesRequest, MessageSortField,
    SortDirection,
};
use filament_core::models::{
    AgentRun, Entity, EntityStatus, EntityType, Message, MessageStatus, MessageType, Priority,
    Reservation,
};
use filament_core::pagination::PaginationState;

use crate::views::analytics::AnalyticsData;
use crate::views::detail::DetailData;
use crate::views::messages::MessageDetailData;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_PAGE_SIZE: u32 = 50;

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
// Message filter state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFilterBar {
    Type,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageParticipantFilter {
    All,
    Mine,
    Agent(String),
}

#[derive(Debug, Clone)]
pub struct MessageFilterState {
    pub msg_types: HashSet<MessageType>,
    pub read_status: Option<MessageStatus>,
    pub participant: MessageParticipantFilter,
    pub active_bar: Option<MessageFilterBar>,
}

impl Default for MessageFilterState {
    fn default() -> Self {
        Self {
            msg_types: HashSet::new(),
            read_status: None,
            participant: MessageParticipantFilter::Mine,
            active_bar: None,
        }
    }
}

impl MessageFilterState {
    pub fn toggle_type(&mut self, t: MessageType) {
        if !self.msg_types.remove(&t) {
            self.msg_types.insert(t);
        }
    }

    pub fn clear_types(&mut self) {
        self.msg_types.clear();
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();

        // Participant
        match &self.participant {
            MessageParticipantFilter::All => parts.push("all".to_string()),
            MessageParticipantFilter::Mine => parts.push("mine".to_string()),
            MessageParticipantFilter::Agent(slug) => parts.push(format!("agent:{slug}")),
        }

        // Type filter
        if !self.msg_types.is_empty() {
            let mut types: Vec<&str> = self.msg_types.iter().map(MessageType::as_str).collect();
            types.sort_unstable();
            parts.push(types.join(","));
        }

        // Read status
        if let Some(ref status) = self.read_status {
            parts.push(status.as_str().to_string());
        }

        parts.join(" | ")
    }
}

pub struct App {
    pub conn: FilamentConnection,
    pub active_tab: Tab,
    pub should_quit: bool,
    pub entities: Vec<EntityRow>,
    pub agent_runs: Vec<AgentRun>,
    pub reservations: Vec<Reservation>,
    pub messages: Vec<Message>,
    pub escalations: Vec<Escalation>,
    pub entity_table_state: TableState,
    pub agent_table_state: TableState,
    pub reservation_table_state: TableState,
    pub message_table_state: TableState,
    pub filter: FilterState,
    pub sort: SortState,
    pub entity_pagination: PaginationState,
    pub msg_filter: MessageFilterState,
    pub detail: Option<DetailData>,
    pub detail_scroll: u16,
    pub message_detail: Option<MessageDetailData>,
    pub message_detail_scroll: u16,
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
            escalations: Vec::new(),
            entity_table_state: TableState::default(),
            agent_table_state: TableState::default(),
            reservation_table_state: TableState::default(),
            message_table_state: TableState::default(),
            filter: FilterState::default(),
            sort: SortState::default(),
            entity_pagination: PaginationState::new(DEFAULT_PAGE_SIZE),
            msg_filter: MessageFilterState::default(),
            detail: None,
            detail_scroll: 0,
            message_detail: None,
            message_detail_scroll: 0,
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

        let req = ListEntitiesRequest {
            types: self.filter.types.iter().copied().collect(),
            statuses: self.filter.statuses.iter().copied().collect(),
            priorities: self.filter.priorities.iter().copied().collect(),
            sort_field: self.sort.field,
            sort_direction: self.sort.direction,
            pagination: self.entity_pagination.to_params(),
        };

        match self.conn.list_entities_paged(&req).await {
            Ok(result) => {
                self.entity_pagination
                    .update_cursors(result.next_cursor, result.prev_cursor);
                self.build_entity_rows(result.items).await;
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
                self.sort_entities_in_place();
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
        // Fetch escalations for the status bar badge
        if let Ok(items) = self.conn.list_pending_escalations().await {
            self.escalation_count = items.len();
            self.escalations = items;
        } else {
            self.escalation_count = 0;
            self.escalations = Vec::new();
        }

        // Fetch full messages for the messages tab using current filters
        let req = self.build_message_request();
        match self.conn.list_messages_paged(&req).await {
            Ok(result) => {
                self.messages = result.items;
                self.clamp_message_selection();
            }
            Err(_) => {
                self.messages = Vec::new();
            }
        }
    }

    fn clamp_message_selection(&mut self) {
        let len = self.messages.len();
        if let Some(idx) = self.message_table_state.selected() {
            if len == 0 {
                self.message_table_state.select(None);
            } else if idx >= len {
                self.message_table_state.select(Some(len - 1));
            }
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

    /// In-memory sort for the `ready_only` path (bypasses SQL paging).
    fn sort_entities_in_place(&mut self) {
        let sort = self.sort;
        self.entities.sort_by(|a, b| {
            let cmp = match sort.field {
                EntitySortField::Name => a.entity.name().as_str().cmp(b.entity.name().as_str()),
                EntitySortField::Priority => a
                    .entity
                    .priority()
                    .value()
                    .cmp(&b.entity.priority().value()),
                EntitySortField::Status => {
                    a.entity.status().as_str().cmp(b.entity.status().as_str())
                }
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

    fn clamp_entity_selection(&mut self) {
        let len = self.entities.len();
        if let Some(idx) = self.entity_table_state.selected() {
            if len == 0 {
                self.entity_table_state.select(None);
            } else if idx >= len {
                self.entity_table_state.select(Some(len - 1));
            }
        }
    }

    pub fn visible_entities(&self) -> &[EntityRow] {
        &self.entities
    }

    pub const fn has_next_page(&self) -> bool {
        self.entity_pagination.has_next()
    }

    pub const fn has_prev_page(&self) -> bool {
        self.entity_pagination.has_previous()
    }

    pub async fn next_page(&mut self) {
        if self.entity_pagination.has_next() {
            self.entity_pagination.go_forwards();
            self.entity_table_state.select(Some(0));
            self.refresh_entities().await;
        }
    }

    pub async fn prev_page(&mut self) {
        if self.entity_pagination.has_previous() {
            self.entity_pagination.go_backwards();
            self.entity_table_state.select(Some(0));
            self.refresh_entities().await;
        }
    }

    pub fn reset_page(&mut self) {
        self.entity_pagination.reset();
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

    // -----------------------------------------------------------------------
    // Message filtering
    // -----------------------------------------------------------------------

    fn build_message_request(&self) -> ListMessagesRequest {
        let participant = match &self.msg_filter.participant {
            MessageParticipantFilter::All => None,
            MessageParticipantFilter::Mine => Some("user".to_string()),
            MessageParticipantFilter::Agent(slug) => Some(slug.clone()),
        };
        ListMessagesRequest {
            msg_types: self.msg_filter.msg_types.iter().cloned().collect(),
            read_status: self.msg_filter.read_status.clone(),
            participant,
            sort_field: MessageSortField::Time,
            sort_direction: SortDirection::Desc,
            pagination: filament_core::pagination::PaginationParams {
                cursor: None,
                limit: DEFAULT_PAGE_SIZE,
                direction: filament_core::pagination::PaginationDirection::Forward,
            },
        }
    }

    /// Cycle participant filter: All → Mine → (each agent slug) → All
    pub async fn cycle_msg_participant(&mut self) {
        let agents = self.known_agent_slugs().await;
        self.msg_filter.participant = match &self.msg_filter.participant {
            MessageParticipantFilter::All => MessageParticipantFilter::Mine,
            MessageParticipantFilter::Mine => agents
                .first()
                .map_or(MessageParticipantFilter::All, |slug| {
                    MessageParticipantFilter::Agent(slug.clone())
                }),
            MessageParticipantFilter::Agent(current) => agents
                .iter()
                .position(|s| s == current)
                .map_or(MessageParticipantFilter::All, |pos| {
                    if pos + 1 < agents.len() {
                        MessageParticipantFilter::Agent(agents[pos + 1].clone())
                    } else {
                        MessageParticipantFilter::All
                    }
                }),
        };
        self.message_table_state.select(None);
        self.refresh_messages().await;
    }

    async fn known_agent_slugs(&mut self) -> Vec<String> {
        self.conn
            .list_entities(Some(EntityType::Agent), None)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.slug().to_string())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Message detail pane
    // -----------------------------------------------------------------------

    pub async fn open_message_detail(&mut self) {
        if self.active_tab != Tab::Messages {
            return;
        }
        let Some(idx) = self.message_table_state.selected() else {
            return;
        };
        let Some(msg) = self.messages.get(idx).cloned() else {
            return;
        };

        // Resolve from/to display names
        let from_name = self.resolve_participant_name(msg.from_agent.as_str()).await;
        let to_name = self.resolve_participant_name(msg.to_agent.as_str()).await;

        // Resolve task name if present
        let task_name = if let Some(ref task_id) = msg.task_id {
            self.conn
                .resolve_entity(task_id.as_str())
                .await
                .ok()
                .map(|e| format!("[{}] {}", e.slug(), e.name()))
        } else {
            None
        };

        // Fetch reply parent if present
        let reply_to = if let Some(ref reply_id) = msg.in_reply_to {
            self.conn.get_message(reply_id.as_str()).await.ok()
        } else {
            None
        };

        self.message_detail = Some(MessageDetailData {
            message: msg,
            from_name,
            to_name,
            task_name,
            reply_to,
        });
        self.message_detail_scroll = 0;
    }

    pub fn close_message_detail(&mut self) {
        self.message_detail = None;
        self.message_detail_scroll = 0;
    }

    pub const fn has_message_detail(&self) -> bool {
        self.message_detail.is_some()
    }

    pub const fn scroll_message_detail_down(&mut self) {
        self.message_detail_scroll = self.message_detail_scroll.saturating_add(1);
    }

    pub const fn scroll_message_detail_up(&mut self) {
        self.message_detail_scroll = self.message_detail_scroll.saturating_sub(1);
    }

    async fn resolve_participant_name(&mut self, slug_or_user: &str) -> String {
        if slug_or_user.eq_ignore_ascii_case("user") {
            return "user".to_string();
        }
        self.conn
            .resolve_entity(slug_or_user)
            .await
            .ok()
            .map_or_else(
                || slug_or_user.to_string(),
                |e| format!("[{}] {}", e.slug(), e.name()),
            )
    }
}

fn source_label(from_config: bool) -> String {
    if from_config {
        "config".to_string()
    } else {
        "default".to_string()
    }
}
