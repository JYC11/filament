use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;
use crate::models::{
    AgentStatus, Entity, EntityId, EntityStatus, EntityType, Event, LessonFields, Message,
    MessageId, MessageType, NonEmptyString, Priority, Relation, Weight,
};

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
    pub to_agent: NonEmptyString,
    pub body: NonEmptyString,
    pub msg_type: MessageType,
}

// ---------------------------------------------------------------------------
// Export / Import
// ---------------------------------------------------------------------------

/// Full graph export: all entities, relations, messages, and events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExportData {
    /// Schema version (1 for forward compat).
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
    pub messages: Vec<Message>,
    pub events: Vec<Event>,
}

/// Summary of an import operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportResult {
    pub entities_imported: usize,
    pub relations_imported: usize,
    pub messages_imported: usize,
    pub events_imported: usize,
}

// ---------------------------------------------------------------------------
// Escalation
// ---------------------------------------------------------------------------

/// Something requiring human attention.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Escalation {
    pub kind: EscalationKind,
    pub agent_name: String,
    pub task_id: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Classification of an escalation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EscalationKind {
    Blocker,
    Question,
    NeedsInput,
    Conflict,
}

impl std::fmt::Display for EscalationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocker => write!(f, "blocker"),
            Self::Question => write!(f, "question"),
            Self::NeedsInput => write!(f, "needs_input"),
            Self::Conflict => write!(f, "conflict"),
        }
    }
}

// ---------------------------------------------------------------------------
// Search result
// ---------------------------------------------------------------------------

/// An entity with its BM25 relevance rank from full-text search.
/// Lower rank values indicate higher relevance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub entity: Entity,
    pub rank: f64,
}

// ---------------------------------------------------------------------------
// Entity changeset (optimistic conflict resolution)
// ---------------------------------------------------------------------------

/// Shared changeset fields for all entity types.
///
/// `None` means "don't change this field"; `Some(v)` means "set to v".
/// `expected_version` is always required — callers must read the entity first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesetCommon {
    pub name: Option<NonEmptyString>,
    pub summary: Option<String>,
    pub status: Option<EntityStatus>,
    pub priority: Option<Priority>,
    pub key_facts: Option<String>,
    pub expected_version: i64,
}

/// Changeset for entity types where `content_path` can be set or cleared
/// (Task, Module, Service, Agent, Lesson).
///
/// - `None` → don't touch
/// - `Some(None)` → clear to NULL
/// - `Some(Some(v))` → set to v
#[derive(Debug, Clone)]
pub struct ContentClearableChangeset {
    pub common: ChangesetCommon,
    pub content_path: Option<Option<String>>,
}

/// Changeset for entity types where `content_path` can be changed but never cleared
/// (Doc, Plan — content is always required).
///
/// - `None` → don't touch
/// - `Some(v)` → change to v
#[derive(Debug, Clone)]
pub struct ContentRequiredChangeset {
    pub common: ChangesetCommon,
    pub content_path: Option<String>,
}

/// Typed entity changeset — enforces `content_path` policy per entity type.
#[derive(Debug, Clone)]
pub enum EntityChangeset {
    Task(ContentClearableChangeset),
    Module(ContentClearableChangeset),
    Service(ContentClearableChangeset),
    Agent(ContentClearableChangeset),
    Plan(ContentRequiredChangeset),
    Doc(ContentRequiredChangeset),
    Lesson(ContentClearableChangeset),
}

impl EntityChangeset {
    /// Access the common changeset fields.
    #[must_use]
    pub const fn common(&self) -> &ChangesetCommon {
        match self {
            Self::Task(v)
            | Self::Module(v)
            | Self::Service(v)
            | Self::Agent(v)
            | Self::Lesson(v) => &v.common,
            Self::Plan(v) | Self::Doc(v) => &v.common,
        }
    }

    /// Returns the resolved `content_path` for SQL:
    /// - `None` → keep existing value
    /// - `Some(None)` → clear to NULL
    /// - `Some(Some(v))` → set to v
    #[must_use]
    pub fn content_path_for_sql(&self) -> Option<Option<&str>> {
        match self {
            Self::Task(v)
            | Self::Module(v)
            | Self::Service(v)
            | Self::Agent(v)
            | Self::Lesson(v) => v.content_path.as_ref().map(|opt| opt.as_deref()),
            Self::Plan(v) | Self::Doc(v) => v.content_path.as_deref().map(Some),
        }
    }

    /// Returns the entity type implied by the variant.
    #[must_use]
    pub const fn entity_type(&self) -> EntityType {
        match self {
            Self::Task(_) => EntityType::Task,
            Self::Module(_) => EntityType::Module,
            Self::Service(_) => EntityType::Service,
            Self::Agent(_) => EntityType::Agent,
            Self::Plan(_) => EntityType::Plan,
            Self::Doc(_) => EntityType::Doc,
            Self::Lesson(_) => EntityType::Lesson,
        }
    }

    /// Returns the names of fields that have values set (i.e., are being changed).
    #[must_use]
    pub fn changed_field_names(&self) -> Vec<&str> {
        let common = self.common();
        let mut fields = Vec::new();
        if common.name.is_some() {
            fields.push("name");
        }
        if common.summary.is_some() {
            fields.push("summary");
        }
        if common.status.is_some() {
            fields.push("status");
        }
        if common.priority.is_some() {
            fields.push("priority");
        }
        if common.key_facts.is_some() {
            fields.push("key_facts");
        }
        if self.content_path_for_sql().is_some() {
            fields.push("content_path");
        }
        fields
    }

    /// Returns true if no fields are being changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let common = self.common();
        common.name.is_none()
            && common.summary.is_none()
            && common.status.is_none()
            && common.priority.is_none()
            && common.key_facts.is_none()
            && self.content_path_for_sql().is_none()
    }

    /// Construct the right changeset variant from flat parts, based on entity type.
    ///
    /// `content_path` of `None` means "don't change"; `Some(v)` means "set to v".
    /// Use variant constructors directly for clearing `content_path` on clearable types.
    pub fn for_type(
        entity_type: EntityType,
        common: ChangesetCommon,
        content_path: Option<String>,
    ) -> Self {
        match entity_type {
            EntityType::Plan => Self::Plan(ContentRequiredChangeset {
                common,
                content_path,
            }),
            EntityType::Doc => Self::Doc(ContentRequiredChangeset {
                common,
                content_path,
            }),
            EntityType::Task => Self::Task(ContentClearableChangeset {
                common,
                content_path: content_path.map(Some),
            }),
            EntityType::Module => Self::Module(ContentClearableChangeset {
                common,
                content_path: content_path.map(Some),
            }),
            EntityType::Service => Self::Service(ContentClearableChangeset {
                common,
                content_path: content_path.map(Some),
            }),
            EntityType::Agent => Self::Agent(ContentClearableChangeset {
                common,
                content_path: content_path.map(Some),
            }),
            EntityType::Lesson => Self::Lesson(ContentClearableChangeset {
                common,
                content_path: content_path.map(Some),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Entity creation (typed per entity type)
// ---------------------------------------------------------------------------

/// Shared fields for all entity creation requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCommon {
    pub name: String,
    pub summary: Option<String>,
    pub priority: Option<Priority>,
    pub key_facts: Option<serde_json::Value>,
}

impl CreateCommon {
    /// Convenience constructor with just a name (other fields default to None).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: None,
            priority: None,
            key_facts: None,
        }
    }
}

/// Creation data for entity types where `content_path` is optional
/// (Task, Module, Service, Agent, Lesson).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContentOptional {
    #[serde(flatten)]
    pub common: CreateCommon,
    pub content_path: Option<String>,
}

/// Creation data for entity types where `content_path` is required (Doc, Plan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContentRequired {
    #[serde(flatten)]
    pub common: CreateCommon,
    pub content_path: String,
}

/// Typed entity creation request — enforces per-type field requirements at compile time.
///
/// Content policy:
/// - Task, Module, Service, Agent, Lesson: optional `content_path`
/// - Doc, Plan: required `content_path`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "entity_type", rename_all = "snake_case")]
pub enum CreateEntityRequest {
    Task(CreateContentOptional),
    Module(CreateContentOptional),
    Service(CreateContentOptional),
    Agent(CreateContentOptional),
    Plan(CreateContentRequired),
    Doc(CreateContentRequired),
    Lesson(CreateContentOptional),
}

impl CreateEntityRequest {
    /// Access the common fields shared by all creation requests.
    #[must_use]
    pub const fn common(&self) -> &CreateCommon {
        match self {
            Self::Task(v)
            | Self::Module(v)
            | Self::Service(v)
            | Self::Agent(v)
            | Self::Lesson(v) => &v.common,
            Self::Plan(v) | Self::Doc(v) => &v.common,
        }
    }

    /// Consume and return the common fields.
    #[must_use]
    pub fn into_common(self) -> CreateCommon {
        match self {
            Self::Task(v)
            | Self::Module(v)
            | Self::Service(v)
            | Self::Agent(v)
            | Self::Lesson(v) => v.common,
            Self::Plan(v) | Self::Doc(v) => v.common,
        }
    }

    /// Returns the entity type implied by the variant.
    #[must_use]
    pub const fn entity_type(&self) -> EntityType {
        match self {
            Self::Task(_) => EntityType::Task,
            Self::Module(_) => EntityType::Module,
            Self::Service(_) => EntityType::Service,
            Self::Agent(_) => EntityType::Agent,
            Self::Plan(_) => EntityType::Plan,
            Self::Doc(_) => EntityType::Doc,
            Self::Lesson(_) => EntityType::Lesson,
        }
    }

    /// Returns the `content_path` if one is set.
    #[must_use]
    pub fn content_path(&self) -> Option<&str> {
        match self {
            Self::Task(v)
            | Self::Module(v)
            | Self::Service(v)
            | Self::Agent(v)
            | Self::Lesson(v) => v.content_path.as_deref(),
            Self::Plan(v) | Self::Doc(v) => Some(&v.content_path),
        }
    }

    /// Construct the right creation variant from flat parts, based on entity type.
    ///
    /// Returns an error if Doc/Plan is missing `content_path`.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Validation` if a Doc or Plan is created without a `content_path`.
    pub fn from_parts(
        entity_type: EntityType,
        name: String,
        summary: Option<String>,
        priority: Option<Priority>,
        key_facts: Option<serde_json::Value>,
        content_path: Option<String>,
    ) -> std::result::Result<Self, FilamentError> {
        let common = CreateCommon {
            name,
            summary,
            priority,
            key_facts,
        };
        match entity_type {
            EntityType::Task => Ok(Self::Task(CreateContentOptional {
                common,
                content_path,
            })),
            EntityType::Module => Ok(Self::Module(CreateContentOptional {
                common,
                content_path,
            })),
            EntityType::Service => Ok(Self::Service(CreateContentOptional {
                common,
                content_path,
            })),
            EntityType::Agent => Ok(Self::Agent(CreateContentOptional {
                common,
                content_path,
            })),
            EntityType::Plan => {
                let path = content_path.ok_or_else(|| {
                    FilamentError::Validation("plan requires --content path".into())
                })?;
                Ok(Self::Plan(CreateContentRequired {
                    common,
                    content_path: path,
                }))
            }
            EntityType::Doc => {
                let path = content_path.ok_or_else(|| {
                    FilamentError::Validation("doc requires --content path".into())
                })?;
                Ok(Self::Doc(CreateContentRequired {
                    common,
                    content_path: path,
                }))
            }
            EntityType::Lesson => Ok(Self::Lesson(CreateContentOptional {
                common,
                content_path,
            })),
        }
    }
}

// ---------------------------------------------------------------------------
// Validated creation DTO (boundary validation via TryFrom)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ValidCreateEntityRequest {
    pub name: NonEmptyString,
    pub entity_type: EntityType,
    pub summary: String,
    pub key_facts: serde_json::Value,
    pub content_path: Option<String>,
    pub priority: Priority,
}

impl TryFrom<CreateEntityRequest> for ValidCreateEntityRequest {
    type Error = FilamentError;

    fn try_from(req: CreateEntityRequest) -> std::result::Result<Self, Self::Error> {
        let entity_type = req.entity_type();
        let content_path = req.content_path().map(String::from);
        let common = req.into_common();

        let name = NonEmptyString::new(&common.name)
            .map_err(|_| FilamentError::Validation("name cannot be empty".to_string()))?;

        let key_facts = common.key_facts.unwrap_or_else(|| serde_json::json!({}));

        // Lesson requires structured key_facts (problem, solution, learned).
        if entity_type == EntityType::Lesson && LessonFields::from_key_facts(&key_facts).is_none() {
            return Err(FilamentError::Validation(
                "lesson requires problem, solution, and learned fields in key_facts".to_string(),
            ));
        }

        Ok(Self {
            name,
            entity_type,
            summary: common.summary.unwrap_or_default(),
            key_facts,
            content_path,
            priority: common.priority.unwrap_or(Priority::DEFAULT),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationRequest {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: crate::models::RelationType,
    pub weight: Option<f64>,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ValidCreateRelationRequest {
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub relation_type: crate::models::RelationType,
    pub weight: Weight,
    pub summary: String,
    pub metadata: serde_json::Value,
}

impl TryFrom<CreateRelationRequest> for ValidCreateRelationRequest {
    type Error = FilamentError;

    fn try_from(req: CreateRelationRequest) -> std::result::Result<Self, Self::Error> {
        let source_id = req.source_id.trim().to_string();
        let target_id = req.target_id.trim().to_string();

        if source_id.is_empty() {
            return Err(FilamentError::Validation(
                "source_id cannot be empty".to_string(),
            ));
        }
        if target_id.is_empty() {
            return Err(FilamentError::Validation(
                "target_id cannot be empty".to_string(),
            ));
        }
        if source_id == target_id {
            return Err(FilamentError::Validation(
                "source_id and target_id must differ".to_string(),
            ));
        }

        let weight = Weight::new(req.weight.unwrap_or(1.0))?;

        Ok(Self {
            source_id: EntityId::from(source_id),
            target_id: EntityId::from(target_id),
            relation_type: req.relation_type,
            weight,
            summary: req.summary.unwrap_or_default(),
            metadata: req.metadata.unwrap_or_else(|| serde_json::json!({})),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub from_agent: String,
    pub to_agent: String,
    pub body: String,
    pub msg_type: Option<MessageType>,
    pub in_reply_to: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidSendMessageRequest {
    pub from_agent: NonEmptyString,
    pub to_agent: NonEmptyString,
    pub body: NonEmptyString,
    pub msg_type: MessageType,
    pub in_reply_to: Option<MessageId>,
    pub task_id: Option<EntityId>,
}

impl TryFrom<SendMessageRequest> for ValidSendMessageRequest {
    type Error = FilamentError;

    fn try_from(req: SendMessageRequest) -> std::result::Result<Self, Self::Error> {
        let from_agent = NonEmptyString::new(&req.from_agent)
            .map_err(|_| FilamentError::Validation("from_agent cannot be empty".to_string()))?;
        let to_agent = NonEmptyString::new(&req.to_agent)
            .map_err(|_| FilamentError::Validation("to_agent cannot be empty".to_string()))?;
        let body = NonEmptyString::new(&req.body)
            .map_err(|_| FilamentError::Validation("message body cannot be empty".to_string()))?;

        Ok(Self {
            from_agent,
            to_agent,
            body,
            msg_type: req.msg_type.unwrap_or(MessageType::Text),
            in_reply_to: req.in_reply_to.map(MessageId::from),
            task_id: req.task_id.map(EntityId::from),
        })
    }
}
