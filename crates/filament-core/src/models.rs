use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Typed ID macro
// ---------------------------------------------------------------------------

/// Generate a newtype wrapper around `String` for type-safe IDs.
macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        pub struct $name(pub String);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(uuid_v7())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        // sqlx decode/encode as TEXT
        impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for $name {
            fn decode(
                value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
            ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
                let s = <String as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
                Ok(Self(s))
            }
        }

        impl sqlx::Encode<'_, sqlx::Sqlite> for $name {
            fn encode_by_ref(
                &self,
                args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
            ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <String as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&self.0, args)
            }
        }

        impl sqlx::Type<sqlx::Sqlite> for $name {
            fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
                <String as sqlx::Type<sqlx::Sqlite>>::type_info()
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
    };
}

/// Generate a UUID v7 (time-ordered) as a string.
#[allow(clippy::cast_possible_truncation)]
fn uuid_v7() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX epoch");
    let millis = now.as_millis() as u64; // safe: won't overflow for ~500k years

    let mut bytes = [0u8; 16];

    // Timestamp (48 bits) — truncation to u8 is intentional (extracting individual bytes)
    bytes[0] = (millis >> 40) as u8;
    bytes[1] = (millis >> 32) as u8;
    bytes[2] = (millis >> 24) as u8;
    bytes[3] = (millis >> 16) as u8;
    bytes[4] = (millis >> 8) as u8;
    bytes[5] = millis as u8;

    // Random bits for the rest
    let rand_bytes: [u8; 10] = std::array::from_fn(|_| fastrand_u8());
    bytes[6..16].copy_from_slice(&rand_bytes);

    // Version 7
    bytes[6] = (bytes[6] & 0x0F) | 0x70;
    // Variant 10xx
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Simple random byte using thread-local state (no external dep).
#[allow(clippy::cast_possible_truncation)]
fn fastrand_u8() -> u8 {
    use std::cell::Cell;
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    thread_local! {
        static RNG: Cell<u64> = Cell::new(RandomState::new().build_hasher().finish());
    }
    RNG.with(|cell| {
        // xorshift64
        let mut s = cell.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        cell.set(s);
        s as u8
    })
}

// ---------------------------------------------------------------------------
// IDs
// ---------------------------------------------------------------------------

typed_id!(EntityId);
typed_id!(RelationId);
typed_id!(MessageId);
typed_id!(ReservationId);
typed_id!(AgentRunId);
typed_id!(EventId);

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Task,
    Module,
    Service,
    Agent,
    Plan,
    Doc,
}

impl EntityType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Module => "module",
            Self::Service => "service",
            Self::Agent => "agent",
            Self::Plan => "plan",
            Self::Doc => "doc",
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    Blocks,
    DependsOn,
    Produces,
    Owns,
    RelatesTo,
    AssignedTo,
}

impl RelationType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Blocks => "blocks",
            Self::DependsOn => "depends_on",
            Self::Produces => "produces",
            Self::Owns => "owns",
            Self::RelatesTo => "relates_to",
            Self::AssignedTo => "assigned_to",
        }
    }
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    Open,
    InProgress,
    Closed,
    Blocked,
}

impl EntityStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Closed => "closed",
            Self::Blocked => "blocked",
        }
    }
}

impl std::fmt::Display for EntityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Text,
    Question,
    Blocker,
    Artifact,
}

impl MessageType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Question => "question",
            Self::Blocker => "blocker",
            Self::Artifact => "artifact",
        }
    }
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Unread,
    Read,
    Archived,
}

impl MessageStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Read => "read",
            Self::Archived => "archived",
        }
    }
}

impl std::fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Completed,
    Blocked,
    Failed,
    NeedsInput,
}

impl AgentStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::NeedsInput => "needs_input",
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// DB row structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub entity_type: EntityType,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub content_hash: Option<String>,
    pub status: EntityStatus,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    pub id: RelationId,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: f64,
    pub summary: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: MessageId,
    pub from_agent: String,
    pub to_agent: String,
    pub msg_type: MessageType,
    pub body: String,
    pub status: MessageStatus,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Reservation {
    pub id: ReservationId,
    pub agent_name: String,
    pub file_glob: String,
    pub exclusive: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct AgentRun {
    pub id: AgentRunId,
    pub task_id: EntityId,
    pub agent_role: String,
    pub pid: Option<i32>,
    pub status: AgentStatus,
    pub result_json: Option<String>,
    pub context_budget_pct: Option<f64>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub id: EventId,
    pub entity_id: Option<EntityId>,
    pub event_type: String,
    pub actor: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Agent protocol (parsed from subprocess JSON output)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentResult {
    pub status: AgentStatus,
    pub task_id: Option<EntityId>,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub messages: Vec<AgentMessage>,
    pub blockers: Vec<String>,
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentMessage {
    pub to_agent: String,
    pub body: String,
    pub msg_type: MessageType,
}

// ---------------------------------------------------------------------------
// Validated DTOs (boundary validation via TryFrom)
// ---------------------------------------------------------------------------

use crate::error::FilamentError;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEntityRequest {
    pub name: String,
    pub entity_type: String,
    pub summary: Option<String>,
    pub key_facts: Option<serde_json::Value>,
    pub content_path: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ValidCreateEntityRequest {
    pub name: String,
    pub entity_type: EntityType,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub priority: i32,
}

impl TryFrom<CreateEntityRequest> for ValidCreateEntityRequest {
    type Error = FilamentError;

    fn try_from(req: CreateEntityRequest) -> std::result::Result<Self, Self::Error> {
        if req.name.trim().is_empty() {
            return Err(FilamentError::Validation("name cannot be empty".to_string()));
        }

        let entity_type = match req.entity_type.to_lowercase().as_str() {
            "task" => EntityType::Task,
            "module" => EntityType::Module,
            "service" => EntityType::Service,
            "agent" => EntityType::Agent,
            "plan" => EntityType::Plan,
            "doc" => EntityType::Doc,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid entity type: '{other}' (expected: task, module, service, agent, plan, doc)"
                )));
            }
        };

        let priority = req.priority.unwrap_or(2);
        if !(0..=4).contains(&priority) {
            return Err(FilamentError::Validation(format!(
                "priority must be 0-4, got {priority}"
            )));
        }

        Ok(Self {
            name: req.name.trim().to_string(),
            entity_type,
            summary: req.summary.unwrap_or_default(),
            key_facts: req.key_facts.unwrap_or_else(|| serde_json::json!({})),
            content_path: req.content_path,
            priority,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRelationRequest {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub weight: Option<f64>,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ValidCreateRelationRequest {
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: f64,
    pub summary: String,
    pub metadata: serde_json::Value,
}

impl TryFrom<CreateRelationRequest> for ValidCreateRelationRequest {
    type Error = FilamentError;

    fn try_from(req: CreateRelationRequest) -> std::result::Result<Self, Self::Error> {
        if req.source_id.trim().is_empty() {
            return Err(FilamentError::Validation(
                "source_id cannot be empty".to_string(),
            ));
        }
        if req.target_id.trim().is_empty() {
            return Err(FilamentError::Validation(
                "target_id cannot be empty".to_string(),
            ));
        }
        if req.source_id == req.target_id {
            return Err(FilamentError::Validation(
                "source_id and target_id must differ".to_string(),
            ));
        }

        let relation_type = match req.relation_type.to_lowercase().as_str() {
            "blocks" => RelationType::Blocks,
            "depends_on" => RelationType::DependsOn,
            "produces" => RelationType::Produces,
            "owns" => RelationType::Owns,
            "relates_to" => RelationType::RelatesTo,
            "assigned_to" => RelationType::AssignedTo,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid relation type: '{other}' (expected: blocks, depends_on, produces, owns, relates_to, assigned_to)"
                )));
            }
        };

        Ok(Self {
            source_id: EntityId::from(req.source_id),
            target_id: EntityId::from(req.target_id),
            relation_type,
            weight: req.weight.unwrap_or(1.0),
            summary: req.summary.unwrap_or_default(),
            metadata: req.metadata.unwrap_or_else(|| serde_json::json!({})),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageRequest {
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub msg_type: Option<String>,
    pub in_reply_to: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidSendMessageRequest {
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub msg_type: MessageType,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
}

impl TryFrom<SendMessageRequest> for ValidSendMessageRequest {
    type Error = FilamentError;

    fn try_from(req: SendMessageRequest) -> std::result::Result<Self, Self::Error> {
        if req.from_agent.trim().is_empty() {
            return Err(FilamentError::Validation(
                "from_agent cannot be empty".to_string(),
            ));
        }
        if req.to_agent.trim().is_empty() {
            return Err(FilamentError::Validation(
                "to_agent cannot be empty".to_string(),
            ));
        }
        if req.body.trim().is_empty() {
            return Err(FilamentError::Validation(
                "message body cannot be empty".to_string(),
            ));
        }

        let msg_type = match req.msg_type.as_deref().unwrap_or("text") {
            "text" => MessageType::Text,
            "question" => MessageType::Question,
            "blocker" => MessageType::Blocker,
            "artifact" => MessageType::Artifact,
            other => {
                return Err(FilamentError::Validation(format!(
                    "invalid message type: '{other}' (expected: text, question, blocker, artifact)"
                )));
            }
        };

        Ok(Self {
            from_agent: req.from_agent.trim().to_string(),
            to_agent: req.to_agent.trim().to_string(),
            body: req.body,
            msg_type,
            in_reply_to: req.in_reply_to.map(MessageId::from),
            task_id: req.task_id.map(EntityId::from),
        })
    }
}
