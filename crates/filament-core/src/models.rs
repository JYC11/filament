use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;

// Re-export all value types and enums so existing `use filament_core::models::*` imports keep working.
pub use crate::enums::*;
pub use crate::types::*;

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
    pub version: i64,
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
    pub diff: Option<String>,
    pub created_at: DateTime<Utc>,
}
