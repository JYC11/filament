use serde::{Deserialize, Serialize};
use thiserror::Error;

/// All errors in the filament system.
#[derive(Error, Debug)]
pub enum FilamentError {
    #[error("Entity not found: {id}")]
    EntityNotFound { id: String },

    #[error("Relation not found: {id}")]
    RelationNotFound { id: String },

    #[error("Message not found: {id}")]
    MessageNotFound { id: String },

    #[error("Agent run not found: {id}")]
    AgentRunNotFound { id: String },

    #[error("Cycle detected: {path}")]
    CycleDetected { path: String },

    #[error("File reserved by {agent}: {glob}")]
    FileReserved { agent: String, glob: String },

    #[error("Reservation expired")]
    ReservationExpired,

    #[error("Validation: {0}")]
    Validation(String),

    #[error("Database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Protocol: {0}")]
    Protocol(String),

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

impl FilamentError {
    /// Machine-readable error code.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::EntityNotFound { .. } => "ENTITY_NOT_FOUND",
            Self::RelationNotFound { .. } => "RELATION_NOT_FOUND",
            Self::MessageNotFound { .. } => "MESSAGE_NOT_FOUND",
            Self::AgentRunNotFound { .. } => "AGENT_RUN_NOT_FOUND",
            Self::CycleDetected { .. } => "CYCLE_DETECTED",
            Self::FileReserved { .. } => "FILE_RESERVED",
            Self::ReservationExpired => "RESERVATION_EXPIRED",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Protocol(_) => "PROTOCOL_ERROR",
            Self::Io(_) => "IO_ERROR",
        }
    }

    /// Whether this error is retryable.
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Database(_) | Self::Io(_))
    }

    /// Agent-friendly hint for resolving the error.
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::EntityNotFound { id } => {
                Some(format!("Check entity ID '{id}' exists with `filament entity list`"))
            }
            Self::MessageNotFound { id } => {
                Some(format!("Check message ID '{id}' exists with `filament msg inbox`"))
            }
            Self::AgentRunNotFound { id } => {
                Some(format!("Check agent run ID '{id}' exists with `filament agent list`"))
            }
            Self::RelationNotFound { id } => {
                Some(format!("Check relation ID '{id}' exists with `filament relation list`"))
            }
            Self::CycleDetected { .. } => {
                Some("Remove one dependency edge to break the cycle".to_string())
            }
            Self::FileReserved { agent, glob } => Some(format!(
                "Wait for agent '{agent}' to release '{glob}', or force-release with `filament reserve release`"
            )),
            Self::ReservationExpired => {
                Some("Re-acquire the reservation before proceeding".to_string())
            }
            Self::Validation(msg) => Some(format!("Fix input: {msg}")),
            _ => None,
        }
    }

    /// Process exit code by error category.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Database(_) => 2,
            Self::EntityNotFound { .. }
            | Self::RelationNotFound { .. }
            | Self::MessageNotFound { .. }
            | Self::AgentRunNotFound { .. } => 3,
            Self::Validation(_) | Self::Protocol(_) => 4,
            Self::CycleDetected { .. } => 5,
            Self::FileReserved { .. } | Self::ReservationExpired => 6,
            Self::Io(_) => 7,
        }
    }
}

/// JSON-serializable error for agent/MCP consumers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredError {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub retryable: bool,
}

impl From<&FilamentError> for StructuredError {
    fn from(err: &FilamentError) -> Self {
        Self {
            code: err.error_code().to_string(),
            message: err.to_string(),
            hint: err.hint(),
            retryable: err.is_retryable(),
        }
    }
}

/// Convenience alias used throughout the codebase.
pub type Result<T> = std::result::Result<T, FilamentError>;
