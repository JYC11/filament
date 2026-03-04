use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::FilamentError;
use crate::models::{
    AgentStatus, Entity, EntityId, EntityType, Event, Message, MessageId, MessageType,
    NonEmptyString, Priority, Relation, Weight,
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
}

impl std::fmt::Display for EscalationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocker => write!(f, "blocker"),
            Self::Question => write!(f, "question"),
            Self::NeedsInput => write!(f, "needs_input"),
        }
    }
}

// ---------------------------------------------------------------------------
// Validated DTOs (boundary validation via TryFrom)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntityRequest {
    pub name: String,
    pub entity_type: EntityType,
    pub summary: Option<String>,
    pub key_facts: Option<serde_json::Value>,
    pub content_path: Option<String>,
    pub priority: Option<Priority>,
}

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
        let name = NonEmptyString::new(&req.name)
            .map_err(|_| FilamentError::Validation("name cannot be empty".to_string()))?;

        Ok(Self {
            name,
            entity_type: req.entity_type,
            summary: req.summary.unwrap_or_default(),
            key_facts: req.key_facts.unwrap_or_else(|| serde_json::json!({})),
            content_path: req.content_path,
            priority: req.priority.unwrap_or(Priority::DEFAULT),
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
