use serde::{Deserialize, Serialize};

use crate::error::StructuredError;

// ---------------------------------------------------------------------------
// JSON-RPC style protocol types
// ---------------------------------------------------------------------------

/// A request from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub method: Method,
    pub params: serde_json::Value,
}

/// A response from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<StructuredError>,
}

impl Response {
    /// Create a success response.
    #[must_use]
    pub const fn success(id: String, result: serde_json::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    #[must_use]
    pub const fn error(id: String, err: StructuredError) -> Self {
        Self {
            id,
            result: None,
            error: Some(err),
        }
    }
}

/// All operations supported by the protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    // Entity operations
    CreateEntity,
    GetEntity,
    GetEntityBySlug,
    ListEntities,
    UpdateEntitySummary,
    UpdateEntityStatus,
    DeleteEntity,

    // Relation operations
    CreateRelation,
    ListRelations,
    DeleteRelation,

    // Message operations
    SendMessage,
    GetInbox,
    MarkMessageRead,

    // Reservation operations
    AcquireReservation,
    FindReservation,
    ListReservations,
    ReleaseReservation,
    ExpireStaleReservations,

    // Agent run operations
    CreateAgentRun,
    FinishAgentRun,
    ListRunningAgents,

    // Graph operations
    ReadyTasks,
    CriticalPath,
    ImpactScore,
    ContextQuery,
    CheckCycle,

    // Event operations
    GetEntityEvents,
}
