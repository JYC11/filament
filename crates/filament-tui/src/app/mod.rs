pub mod entities;
pub mod messages;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::TableState;

use filament_core::config::{FilamentConfig, OutputFormat};
use filament_core::connection::FilamentConnection;
use filament_core::dto::{Escalation, SendMessageRequest};
use filament_core::models::{AgentRun, Message, Reservation};
use filament_core::pagination::PaginationState;

pub use entities::{EntityRow, FilterBar, FilterState, SortState};
pub use messages::{
    MessageFilterBar, MessageFilterState, MessageParticipantFilter, MessageSortState, ReplyState,
};

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

pub struct App {
    pub conn: FilamentConnection,
    pub active_tab: Tab,
    pub should_quit: bool,
    // Entity state
    pub entities: Vec<EntityRow>,
    pub entity_table_state: TableState,
    pub filter: FilterState,
    pub sort: SortState,
    pub entity_pagination: PaginationState,
    pub detail: Option<DetailData>,
    pub detail_scroll: u16,
    // Agent state
    pub agent_runs: Vec<AgentRun>,
    pub agent_table_state: TableState,
    pub agent_show_history: bool,
    // Reservation state
    pub reservations: Vec<Reservation>,
    pub reservation_table_state: TableState,
    // Message state
    pub messages: Vec<Message>,
    pub escalations: Vec<Escalation>,
    pub escalation_count: usize,
    pub message_table_state: TableState,
    pub msg_filter: MessageFilterState,
    pub msg_sort: MessageSortState,
    pub msg_pagination: PaginationState,
    pub message_detail: Option<MessageDetailData>,
    pub message_detail_scroll: u16,
    pub reply: Option<ReplyState>,
    // Config state
    pub config_rows: Vec<(String, String, String)>,
    pub config_table_state: TableState,
    // Analytics state
    pub analytics: AnalyticsData,
    // Global state
    pub last_refresh: DateTime<Utc>,
    pub status_message: Option<String>,
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
            entity_table_state: TableState::default(),
            filter: FilterState::default(),
            sort: SortState::default(),
            entity_pagination: PaginationState::new(DEFAULT_PAGE_SIZE),
            detail: None,
            detail_scroll: 0,
            agent_runs: Vec::new(),
            agent_table_state: TableState::default(),
            agent_show_history: false,
            reservations: Vec::new(),
            reservation_table_state: TableState::default(),
            messages: Vec::new(),
            escalations: Vec::new(),
            escalation_count: 0,
            message_table_state: TableState::default(),
            msg_filter: MessageFilterState::default(),
            msg_sort: MessageSortState::default(),
            msg_pagination: PaginationState::new(DEFAULT_PAGE_SIZE),
            message_detail: None,
            message_detail_scroll: 0,
            reply: None,
            config_rows: Vec::new(),
            config_table_state: TableState::default(),
            analytics: AnalyticsData::default(),
            last_refresh: Utc::now(),
            status_message: None,
            has_cycle: false,
            last_tick: Instant::now(),
        }
    }

    // -----------------------------------------------------------------------
    // Refresh
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Entity operations
    // -----------------------------------------------------------------------

    pub async fn refresh_entities(&mut self) {
        if self.filter.ready_only {
            self.refresh_ready_tasks().await;
            return;
        }

        let req = entities::build_entity_request(&self.filter, self.sort, &self.entity_pagination);

        match self.conn.list_entities_paged(&req).await {
            Ok(result) => {
                self.entity_pagination
                    .update_cursors(result.next_cursor, result.prev_cursor);
                self.entities = entities::build_entity_rows(&mut self.conn, result.items).await;
                entities::clamp_selection(&mut self.entity_table_state, self.entities.len());
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    async fn refresh_ready_tasks(&mut self) {
        match self.conn.ready_tasks().await {
            Ok(mut tasks) => {
                if !self.filter.priorities.is_empty() {
                    tasks.retain(|e| self.filter.priorities.contains(&e.priority()));
                }
                self.entities = entities::build_entity_rows(&mut self.conn, tasks).await;
                entities::sort_entities_in_place(&mut self.entities, self.sort);
                entities::clamp_selection(&mut self.entity_table_state, self.entities.len());
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {e}"));
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
        if let Some(data) =
            entities::open_detail(&mut self.conn, &self.entities, &self.entity_table_state).await
        {
            self.detail = Some(data);
            self.detail_scroll = 0;
        }
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
    // Agent operations
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Reservation operations
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Message operations
    // -----------------------------------------------------------------------

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
        let req =
            messages::build_message_request(&self.msg_filter, self.msg_sort, &self.msg_pagination);
        match self.conn.list_messages_paged(&req).await {
            Ok(result) => {
                self.msg_pagination
                    .update_cursors(result.next_cursor, result.prev_cursor);
                self.messages = result.items;
                entities::clamp_selection(&mut self.message_table_state, self.messages.len());
            }
            Err(_) => {
                self.messages = Vec::new();
            }
        }
    }

    pub async fn cycle_msg_participant(&mut self) {
        messages::cycle_participant(&mut self.conn, &mut self.msg_filter.participant).await;
        self.msg_reset_page();
        self.refresh_messages().await;
    }

    pub async fn open_message_detail(&mut self) {
        if self.active_tab != Tab::Messages {
            return;
        }
        if let Some(data) =
            messages::open_detail(&mut self.conn, &self.messages, &self.message_table_state).await
        {
            self.message_detail = Some(data);
            self.message_detail_scroll = 0;
        }
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

    pub const fn has_msg_next_page(&self) -> bool {
        self.msg_pagination.has_next()
    }

    pub const fn has_msg_prev_page(&self) -> bool {
        self.msg_pagination.has_previous()
    }

    pub async fn msg_next_page(&mut self) {
        if self.msg_pagination.has_next() {
            self.msg_pagination.go_forwards();
            self.message_table_state.select(Some(0));
            self.refresh_messages().await;
        }
    }

    pub async fn msg_prev_page(&mut self) {
        if self.msg_pagination.has_previous() {
            self.msg_pagination.go_backwards();
            self.message_table_state.select(Some(0));
            self.refresh_messages().await;
        }
    }

    pub fn msg_reset_page(&mut self) {
        self.msg_pagination.reset();
        self.message_table_state.select(None);
    }

    // -----------------------------------------------------------------------
    // Reply operations
    // -----------------------------------------------------------------------

    pub fn start_reply(&mut self) {
        if self.active_tab != Tab::Messages {
            return;
        }
        let Some(idx) = self.message_table_state.selected() else {
            return;
        };
        let Some(msg) = self.messages.get(idx) else {
            return;
        };
        // Reply goes to the original sender
        let to = msg.from_agent.as_str().to_string();
        let reply_id = msg.id.as_str().to_string();
        self.reply = Some(ReplyState::new(to, reply_id));
    }

    pub fn cancel_reply(&mut self) {
        self.reply = None;
    }

    pub const fn has_reply(&self) -> bool {
        self.reply.is_some()
    }

    pub async fn send_reply(&mut self) {
        let Some(reply) = self.reply.take() else {
            return;
        };
        if reply.buffer.trim().is_empty() {
            self.status_message = Some("Reply body cannot be empty".to_string());
            return;
        }

        let req = SendMessageRequest {
            from_agent: "user".to_string(),
            to_agent: reply.to_agent,
            body: reply.buffer,
            msg_type: Some(reply.msg_type),
            in_reply_to: Some(reply.in_reply_to),
            task_id: None,
        };

        match self.conn.send_message(req).await {
            Ok(_) => {
                self.status_message = Some("Reply sent".to_string());
                self.refresh_messages().await;
            }
            Err(e) => {
                self.status_message = Some(format!("Send failed: {e}"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Config
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Analytics
    // -----------------------------------------------------------------------

    pub async fn refresh_analytics(&mut self) {
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

    // -----------------------------------------------------------------------
    // Health
    // -----------------------------------------------------------------------

    pub async fn refresh_health(&mut self) {
        self.has_cycle = self.conn.check_cycle().await.unwrap_or(false);
    }

    // -----------------------------------------------------------------------
    // Navigation
    // -----------------------------------------------------------------------

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
            Tab::Analytics => 0,
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
}

fn source_label(from_config: bool) -> String {
    if from_config {
        "config".to_string()
    } else {
        "default".to_string()
    }
}
