use filament_core::models::{EntityType, MessageType, Priority, RelationType};
use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskReadyParams {
    /// Maximum number of tasks to return.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskCloseParams {
    /// Entity slug (or ID) to close.
    pub slug: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ContextParams {
    /// Entity slug (or ID) to explore around.
    pub slug: String,
    /// BFS depth (default: 2).
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MessageSendParams {
    /// Sender agent name.
    pub from_agent: String,
    /// Recipient agent name.
    pub to_agent: String,
    /// Message body.
    pub body: String,
    /// Message type: text, question, blocker, artifact (default: text).
    pub msg_type: Option<MessageType>,
    /// Message ID this is replying to (optional).
    pub in_reply_to: Option<String>,
    /// Related task entity ID (optional).
    pub task_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MessageReadParams {
    /// Message ID to mark as read.
    pub message_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MessageInboxParams {
    /// Agent name whose inbox to check.
    pub agent: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReserveParams {
    /// File glob pattern to reserve.
    pub file_glob: String,
    /// Agent name acquiring the reservation.
    pub agent: String,
    /// Exclusive lock (default: false).
    pub exclusive: Option<bool>,
    /// TTL in seconds (default: 300).
    pub ttl_secs: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReleaseParams {
    /// Reservation ID to release.
    pub reservation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationsParams {
    /// Filter by agent name (optional).
    pub agent: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectParams {
    /// Entity slug (or ID) to inspect.
    pub slug: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListParams {
    /// Filter by entity type: task, module, service, agent, plan, doc.
    pub entity_type: Option<EntityType>,
    /// Filter by status: open, `in_progress`, closed, blocked.
    pub status: Option<filament_core::models::EntityStatus>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddParams {
    /// Entity name.
    pub name: String,
    /// Entity type: task, module, service, agent, plan, doc.
    pub entity_type: EntityType,
    /// Short summary.
    pub summary: String,
    /// Priority 0-4 (0=highest, default: 2).
    pub priority: Option<Priority>,
    /// Structured key facts (JSON object).
    pub key_facts: Option<serde_json::Value>,
    /// Path to full content file.
    pub content_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateParams {
    /// Entity slug (or ID) to update.
    pub slug: String,
    /// New summary (optional).
    pub summary: Option<String>,
    /// New status: open, `in_progress`, closed, blocked (optional).
    pub status: Option<filament_core::models::EntityStatus>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RelateParams {
    /// Source entity slug (or ID).
    pub source: String,
    /// Relation type: blocks, `depends_on`, produces, owns, `relates_to`, `assigned_to`.
    pub relation_type: RelationType,
    /// Target entity slug (or ID).
    pub target: String,
    /// Optional relation summary.
    pub summary: Option<String>,
    /// Optional weight (numeric).
    pub weight: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UnrelateParams {
    /// Source entity slug (or ID).
    pub source: String,
    /// Relation type to remove.
    pub relation_type: RelationType,
    /// Target entity slug (or ID).
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteParams {
    /// Entity slug (or ID) to delete.
    pub slug: String,
}
