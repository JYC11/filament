use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;

// Re-export all value types so existing `use filament_core::models::*` imports keep working.
pub use crate::types::*;

// ---------------------------------------------------------------------------
// Enum string macro
// ---------------------------------------------------------------------------

/// Generate `as_str()`, `Display`, and `FromStr` for enums with `snake_case` string mapping.
macro_rules! impl_enum_str {
    ($name:ident { $($variant:ident => $str:literal),+ $(,)? }) => {
        impl $name {
            #[must_use]
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $str,)+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = FilamentError;
            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                match s {
                    $(s if s.eq_ignore_ascii_case($str) => Ok(Self::$variant),)+
                    _ => Err(FilamentError::Validation(format!(
                        "invalid {}: '{}'", stringify!($name), s
                    ))),
                }
            }
        }
    };
}

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

impl_enum_str!(EntityType {
    Task => "task",
    Module => "module",
    Service => "service",
    Agent => "agent",
    Plan => "plan",
    Doc => "doc",
});

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

impl_enum_str!(RelationType {
    Blocks => "blocks",
    DependsOn => "depends_on",
    Produces => "produces",
    Owns => "owns",
    RelatesTo => "relates_to",
    AssignedTo => "assigned_to",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    Open,
    InProgress,
    Closed,
    Blocked,
}

impl_enum_str!(EntityStatus {
    Open => "open",
    InProgress => "in_progress",
    Closed => "closed",
    Blocked => "blocked",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Text,
    Question,
    Blocker,
    Artifact,
}

impl_enum_str!(MessageType {
    Text => "text",
    Question => "question",
    Blocker => "blocker",
    Artifact => "artifact",
});

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Unread,
    Read,
}

impl_enum_str!(MessageStatus {
    Unread => "unread",
    Read => "read",
});

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

impl_enum_str!(AgentStatus {
    Running => "running",
    Completed => "completed",
    Blocked => "blocked",
    Failed => "failed",
    NeedsInput => "needs_input",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Coder,
    Reviewer,
    Planner,
    Dockeeper,
}

impl_enum_str!(AgentRole {
    Coder => "coder",
    Reviewer => "reviewer",
    Planner => "planner",
    Dockeeper => "dockeeper",
});

impl AgentRole {
    /// All available roles.
    pub const ALL: &'static [Self] = &[Self::Coder, Self::Reviewer, Self::Planner, Self::Dockeeper];
}

/// Whether a file reservation is exclusive or shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReservationMode {
    Exclusive,
    Shared,
}

impl ReservationMode {
    pub const fn is_exclusive(self) -> bool {
        matches!(self, Self::Exclusive)
    }
}

impl From<bool> for ReservationMode {
    fn from(exclusive: bool) -> Self {
        if exclusive {
            Self::Exclusive
        } else {
            Self::Shared
        }
    }
}

impl From<ReservationMode> for bool {
    fn from(mode: ReservationMode) -> Self {
        mode.is_exclusive()
    }
}

impl std::fmt::Display for ReservationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exclusive => write!(f, "exclusive"),
            Self::Shared => write!(f, "shared"),
        }
    }
}

impl std::str::FromStr for ReservationMode {
    type Err = crate::error::FilamentError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "exclusive" => Ok(Self::Exclusive),
            "shared" => Ok(Self::Shared),
            _ => Err(crate::error::FilamentError::Validation(format!(
                "invalid ReservationMode: '{s}'"
            ))),
        }
    }
}

// SQLite stores exclusive as INTEGER (1 = exclusive, 0 = shared)
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for ReservationMode {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> std::result::Result<Self, sqlx::error::BoxDynError> {
        let v = <bool as sqlx::Decode<'r, sqlx::Sqlite>>::decode(value)?;
        Ok(Self::from(v))
    }
}

impl sqlx::Encode<'_, sqlx::Sqlite> for ReservationMode {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'_>>,
    ) -> std::result::Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <bool as sqlx::Encode<'_, sqlx::Sqlite>>::encode_by_ref(&self.is_exclusive(), args)
    }
}

impl sqlx::Type<sqlx::Sqlite> for ReservationMode {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <bool as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &<sqlx::Sqlite as sqlx::Database>::TypeInfo) -> bool {
        <bool as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, JsonSchema)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    EntityCreated,
    EntityUpdated,
    EntityDeleted,
    StatusChange,
    RelationCreated,
    RelationDeleted,
    MessageSent,
    MessageRead,
    ReservationAcquired,
    ReservationReleased,
    AgentStarted,
    AgentFinished,
}

impl_enum_str!(EventType {
    EntityCreated => "entity_created",
    EntityUpdated => "entity_updated",
    EntityDeleted => "entity_deleted",
    StatusChange => "status_change",
    RelationCreated => "relation_created",
    RelationDeleted => "relation_deleted",
    MessageSent => "message_sent",
    MessageRead => "message_read",
    ReservationAcquired => "reservation_acquired",
    ReservationReleased => "reservation_released",
    AgentStarted => "agent_started",
    AgentFinished => "agent_finished",
});

// ---------------------------------------------------------------------------
// DB row structs
// ---------------------------------------------------------------------------

/// Reference to content stored on disk — groups path + integrity hash.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContentRef {
    pub path: String,
    pub hash: Option<String>,
}

/// Shared fields for all entity types.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EntityCommon {
    pub id: EntityId,
    pub slug: Slug,
    pub name: NonEmptyString,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content: Option<ContentRef>,
    pub status: EntityStatus,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain entity — an algebraic data type with one variant per entity type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum Entity {
    Task(EntityCommon),
    Module(EntityCommon),
    Service(EntityCommon),
    Agent(EntityCommon),
    Plan(EntityCommon),
    Doc(EntityCommon),
}

impl Entity {
    /// Access the common fields shared by all entity types.
    #[must_use]
    pub const fn common(&self) -> &EntityCommon {
        match self {
            Self::Task(c)
            | Self::Module(c)
            | Self::Service(c)
            | Self::Agent(c)
            | Self::Plan(c)
            | Self::Doc(c) => c,
        }
    }

    /// Consume and return the common fields.
    #[must_use]
    pub fn into_common(self) -> EntityCommon {
        match self {
            Self::Task(c)
            | Self::Module(c)
            | Self::Service(c)
            | Self::Agent(c)
            | Self::Plan(c)
            | Self::Doc(c) => c,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &EntityId {
        &self.common().id
    }

    #[must_use]
    pub const fn slug(&self) -> &Slug {
        &self.common().slug
    }

    #[must_use]
    pub const fn name(&self) -> &NonEmptyString {
        &self.common().name
    }

    #[must_use]
    pub const fn entity_type(&self) -> EntityType {
        match self {
            Self::Task(_) => EntityType::Task,
            Self::Module(_) => EntityType::Module,
            Self::Service(_) => EntityType::Service,
            Self::Agent(_) => EntityType::Agent,
            Self::Plan(_) => EntityType::Plan,
            Self::Doc(_) => EntityType::Doc,
        }
    }

    #[must_use]
    pub const fn status(&self) -> &EntityStatus {
        &self.common().status
    }

    #[must_use]
    pub const fn priority(&self) -> Priority {
        self.common().priority
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.common().summary
    }

    /// Consume the entity, returning the inner `EntityCommon` if it is a Task.
    ///
    /// # Errors
    ///
    /// Returns `TypeMismatch` if the entity is not a task.
    pub fn into_task(self) -> Result<EntityCommon, FilamentError> {
        match self {
            Self::Task(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Task,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }

    /// Consume the entity, returning the inner `EntityCommon` if it is an Agent.
    ///
    /// # Errors
    ///
    /// Returns `TypeMismatch` if the entity is not an agent.
    pub fn into_agent(self) -> Result<EntityCommon, FilamentError> {
        match self {
            Self::Agent(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Agent,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Relation {
    pub id: RelationId,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: RelationType,
    pub weight: Weight,
    pub summary: String,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: MessageId,
    pub from_agent: NonEmptyString,
    pub to_agent: NonEmptyString,
    pub msg_type: MessageType,
    pub body: NonEmptyString,
    pub status: MessageStatus,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Reservation {
    pub id: ReservationId,
    pub agent_name: NonEmptyString,
    pub file_glob: NonEmptyString,
    #[sqlx(rename = "exclusive")]
    pub mode: ReservationMode,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct AgentRun {
    pub id: AgentRunId,
    pub task_id: EntityId,
    pub agent_role: NonEmptyString,
    pub pid: Option<i32>,
    pub status: AgentStatus,
    pub result_json: Option<String>,
    pub context_budget_pct: Option<BudgetPct>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub id: EventId,
    pub entity_id: Option<EntityId>,
    pub event_type: EventType,
    pub actor: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub created_at: DateTime<Utc>,
}
