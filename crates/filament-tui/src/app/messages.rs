use std::collections::HashSet;

use filament_core::connection::FilamentConnection;
use filament_core::dto::{ListMessagesRequest, MessageSortField, SortDirection};
use filament_core::models::{EntityType, Message, MessageStatus, MessageType};
use filament_core::pagination::PaginationState;
use ratatui::widgets::TableState;

use crate::views::messages::MessageDetailData;

// ---------------------------------------------------------------------------
// Message filter state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFilterBar {
    Type,
    Status,
    Sort,
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

// ---------------------------------------------------------------------------
// Message sort state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct MessageSortState {
    pub field: MessageSortField,
    pub direction: SortDirection,
}

impl Default for MessageSortState {
    fn default() -> Self {
        Self {
            field: MessageSortField::Time,
            direction: SortDirection::Desc,
        }
    }
}

impl MessageSortState {
    pub fn set_field(&mut self, field: MessageSortField) {
        if self.field == field {
            self.direction = self.direction.flip();
        } else {
            self.field = field;
            self.direction = SortDirection::Desc;
        }
    }

    pub fn label(&self) -> String {
        format!("{}{}", self.field.label(), self.direction.arrow())
    }
}

// ---------------------------------------------------------------------------
// Reply state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReplyState {
    pub to_agent: String,
    pub in_reply_to: String,
    pub msg_type: MessageType,
    pub buffer: String,
    pub cursor: usize,
}

impl ReplyState {
    pub const fn new(to_agent: String, in_reply_to: String) -> Self {
        Self {
            to_agent,
            in_reply_to,
            msg_type: MessageType::Text,
            buffer: String::new(),
            cursor: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            self.buffer.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            let next = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.buffer.len(), |(i, _)| self.cursor + i);
            self.buffer.drain(self.cursor..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.buffer[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor = self.buffer[self.cursor..]
                .char_indices()
                .nth(1)
                .map_or(self.buffer.len(), |(i, _)| self.cursor + i);
        }
    }

    pub const fn home(&mut self) {
        self.cursor = 0;
    }

    pub const fn end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub const fn cycle_type(&mut self) {
        self.msg_type = match self.msg_type {
            MessageType::Text => MessageType::Question,
            MessageType::Question => MessageType::Blocker,
            MessageType::Blocker | MessageType::Artifact => MessageType::Text,
        };
    }
}

// ---------------------------------------------------------------------------
// Message state operations
// ---------------------------------------------------------------------------

/// Build a `ListMessagesRequest` from filter/sort/pagination state.
pub fn build_message_request(
    filter: &MessageFilterState,
    sort: MessageSortState,
    pagination: &PaginationState,
) -> ListMessagesRequest {
    let participant = match &filter.participant {
        MessageParticipantFilter::All => None,
        MessageParticipantFilter::Mine => Some("user".to_string()),
        MessageParticipantFilter::Agent(slug) => Some(slug.clone()),
    };
    ListMessagesRequest {
        msg_types: filter.msg_types.iter().cloned().collect(),
        read_status: filter.read_status.clone(),
        participant,
        sort_field: sort.field,
        sort_direction: sort.direction,
        pagination: pagination.to_params(),
    }
}

/// Cycle participant filter: All → Mine → (each agent slug) → All
pub async fn cycle_participant(
    conn: &mut FilamentConnection,
    filter: &mut MessageParticipantFilter,
) {
    let agents = known_agent_slugs(conn).await;
    *filter = match filter {
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
}

async fn known_agent_slugs(conn: &mut FilamentConnection) -> Vec<String> {
    conn.list_entities(Some(EntityType::Agent), None)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.slug().to_string())
        .collect()
}

/// Open message detail pane — resolves names, task, and parent message.
pub async fn open_detail(
    conn: &mut FilamentConnection,
    messages: &[Message],
    table_state: &TableState,
) -> Option<MessageDetailData> {
    let idx = table_state.selected()?;
    let msg = messages.get(idx)?.clone();

    let from_name = resolve_participant_name(conn, msg.from_agent.as_str()).await;
    let to_name = resolve_participant_name(conn, msg.to_agent.as_str()).await;

    let task_name = if let Some(ref task_id) = msg.task_id {
        conn.resolve_entity(task_id.as_str())
            .await
            .ok()
            .map(|e| format!("[{}] {}", e.slug(), e.name()))
    } else {
        None
    };

    let reply_to = if let Some(ref reply_id) = msg.in_reply_to {
        conn.get_message(reply_id.as_str()).await.ok()
    } else {
        None
    };

    Some(MessageDetailData {
        message: msg,
        from_name,
        to_name,
        task_name,
        reply_to,
    })
}

async fn resolve_participant_name(conn: &mut FilamentConnection, slug_or_user: &str) -> String {
    if slug_or_user.eq_ignore_ascii_case("user") {
        return "user".to_string();
    }
    conn.resolve_entity(slug_or_user).await.ok().map_or_else(
        || slug_or_user.to_string(),
        |e| format!("[{}] {}", e.slug(), e.name()),
    )
}
