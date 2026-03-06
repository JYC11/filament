use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;

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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, Serialize, Deserialize, JsonSchema,
)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Task,
    Module,
    Service,
    Agent,
    Plan,
    Doc,
    Lesson,
}

impl_enum_str!(EntityType {
    Task => "task",
    Module => "module",
    Service => "service",
    Agent => "agent",
    Plan => "plan",
    Doc => "doc",
    Lesson => "lesson",
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, Serialize, Deserialize, JsonSchema,
)]
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
