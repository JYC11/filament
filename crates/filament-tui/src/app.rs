use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::TableState;

use filament_core::connection::FilamentConnection;
use filament_core::dto::Escalation;
use filament_core::models::{AgentRun, Entity, EntityStatus, EntityType, Priority, Reservation};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Entities,
    Agents,
    Reservations,
    Messages,
}

impl Tab {
    pub const ALL: [Self; 4] = [
        Self::Entities,
        Self::Agents,
        Self::Reservations,
        Self::Messages,
    ];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Entities => Self::Agents,
            Self::Agents => Self::Reservations,
            Self::Reservations => Self::Messages,
            Self::Messages => Self::Entities,
        }
    }

    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Entities => Self::Messages,
            Self::Agents => Self::Entities,
            Self::Reservations => Self::Agents,
            Self::Messages => Self::Reservations,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Entities => "Entities",
            Self::Agents => "Agents",
            Self::Reservations => "Reservations",
            Self::Messages => "Messages",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Entities => 0,
            Self::Agents => 1,
            Self::Reservations => 2,
            Self::Messages => 3,
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
    pub last_refresh: DateTime<Utc>,
    pub status_message: Option<String>,
    pub escalation_count: usize,
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
            last_refresh: Utc::now(),
            status_message: None,
            escalation_count: 0,
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

        self.entities = rows;
        self.clamp_entity_selection();
    }

    pub async fn refresh_agents(&mut self) {
        match self.conn.list_running_agents().await {
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

    pub async fn refresh_messages(&mut self) {
        if let Ok(items) = self.conn.list_pending_escalations().await {
            self.escalation_count = items.len();
            self.messages = items;
        } else {
            self.escalation_count = 0;
            self.messages = Vec::new();
        }
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

    const fn current_list_len(&self) -> usize {
        match self.active_tab {
            Tab::Entities => self.entities.len(),
            Tab::Agents => self.agent_runs.len(),
            Tab::Reservations => self.reservations.len(),
            Tab::Messages => self.messages.len(),
        }
    }

    const fn current_table_state_mut(&mut self) -> &mut TableState {
        match self.active_tab {
            Tab::Entities => &mut self.entity_table_state,
            Tab::Agents => &mut self.agent_table_state,
            Tab::Reservations => &mut self.reservation_table_state,
            Tab::Messages => &mut self.message_table_state,
        }
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
}
