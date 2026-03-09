use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::{EntityType, Slug};

/// A single field where two concurrent changes conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConflict {
    pub field: String,
    pub your_value: String,
    pub their_value: String,
}

/// All errors in the filament system.
#[derive(Error, Debug)]
pub enum FilamentError {
    #[error("Entity not found: {id}")]
    EntityNotFound { id: String },

    #[error("Type mismatch for '{slug}': expected {expected}, got {actual}")]
    TypeMismatch {
        expected: EntityType,
        actual: EntityType,
        slug: Slug,
    },

    #[error("Relation not found: {id}")]
    RelationNotFound { id: String },

    #[error("Message not found: {id}")]
    MessageNotFound { id: String },

    #[error("Message already read: {id}")]
    MessageAlreadyRead { id: String },

    #[error("Agent run not found: {id}")]
    AgentRunNotFound { id: String },

    #[error("Reservation not found: {id}")]
    ReservationNotFound { id: String },

    #[error("Cycle detected: {path}")]
    CycleDetected { path: String },

    #[error("File reserved by {agent}: {glob}")]
    FileReserved { agent: String, glob: String },

    #[error("Reservation expired")]
    ReservationExpired,

    #[error("Agent dispatch failed: {reason}")]
    AgentDispatchFailed { reason: String },

    #[error("Task {task_id} already has a running agent")]
    AgentAlreadyRunning { task_id: String },

    #[error("Version conflict on entity {entity_id} (current version {current_version})")]
    VersionConflict {
        entity_id: String,
        current_version: i64,
        conflicts: Vec<FieldConflict>,
    },

    #[error("Validation: {0}")]
    Validation(String),

    #[error("Database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Protocol: {0}")]
    Protocol(String),

    /// Error received from the daemon, preserving original exit code and metadata.
    #[error("{message}")]
    DaemonError {
        code: String,
        message: String,
        hint: Option<String>,
        retryable: bool,
        exit_code: i32,
    },

    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

impl FilamentError {
    /// Machine-readable error code.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::EntityNotFound { .. } => "ENTITY_NOT_FOUND",
            Self::TypeMismatch { .. } => "TYPE_MISMATCH",
            Self::RelationNotFound { .. } => "RELATION_NOT_FOUND",
            Self::MessageNotFound { .. } => "MESSAGE_NOT_FOUND",
            Self::MessageAlreadyRead { .. } => "MESSAGE_ALREADY_READ",
            Self::AgentRunNotFound { .. } => "AGENT_RUN_NOT_FOUND",
            Self::ReservationNotFound { .. } => "RESERVATION_NOT_FOUND",
            Self::CycleDetected { .. } => "CYCLE_DETECTED",
            Self::FileReserved { .. } => "FILE_RESERVED",
            Self::ReservationExpired => "RESERVATION_EXPIRED",
            Self::AgentDispatchFailed { .. } => "AGENT_DISPATCH_FAILED",
            Self::AgentAlreadyRunning { .. } => "AGENT_ALREADY_RUNNING",
            Self::VersionConflict { .. } => "VERSION_CONFLICT",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::Protocol(_) => "PROTOCOL_ERROR",
            Self::DaemonError { ref code, .. } => match code.as_str() {
                "ENTITY_NOT_FOUND" => "ENTITY_NOT_FOUND",
                "RELATION_NOT_FOUND" => "RELATION_NOT_FOUND",
                "MESSAGE_NOT_FOUND" => "MESSAGE_NOT_FOUND",
                "MESSAGE_ALREADY_READ" => "MESSAGE_ALREADY_READ",
                "AGENT_RUN_NOT_FOUND" => "AGENT_RUN_NOT_FOUND",
                "RESERVATION_NOT_FOUND" => "RESERVATION_NOT_FOUND",
                "CYCLE_DETECTED" => "CYCLE_DETECTED",
                "FILE_RESERVED" => "FILE_RESERVED",
                "RESERVATION_EXPIRED" => "RESERVATION_EXPIRED",
                "AGENT_DISPATCH_FAILED" => "AGENT_DISPATCH_FAILED",
                "AGENT_ALREADY_RUNNING" => "AGENT_ALREADY_RUNNING",
                "VERSION_CONFLICT" => "VERSION_CONFLICT",
                "VALIDATION_ERROR" => "VALIDATION_ERROR",
                "DATABASE_ERROR" => "DATABASE_ERROR",
                "IO_ERROR" => "IO_ERROR",
                "TYPE_MISMATCH" => "TYPE_MISMATCH",
                _ => "PROTOCOL_ERROR",
            },
            Self::Io(_) => "IO_ERROR",
        }
    }

    /// Whether this error is retryable.
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::DaemonError { retryable, .. } => *retryable,
            Self::Database(_)
            | Self::Io(_)
            | Self::AgentDispatchFailed { .. }
            | Self::VersionConflict { .. } => true,
            _ => false,
        }
    }

    /// Agent-friendly hint for resolving the error.
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::EntityNotFound { id } => {
                Some(format!("Check entity '{id}' exists with `fl list`"))
            }
            Self::TypeMismatch {
                expected, slug, ..
            } => Some(format!(
                "'{slug}' is not a {expected}. Use `fl inspect {slug}` to check its type"
            )),
            Self::MessageNotFound { id } => {
                Some(format!("Check message ID '{id}' with `fl message inbox <agent>`"))
            }
            Self::MessageAlreadyRead { id } => {
                Some(format!("Message '{id}' has already been marked as read"))
            }
            Self::AgentRunNotFound { id } => {
                Some(format!("Agent run '{id}' does not exist or has already finished"))
            }
            Self::RelationNotFound { id } => {
                Some(format!("Relation '{id}' does not exist. Check entity names and relation type"))
            }
            Self::ReservationNotFound { id } => {
                Some(format!("Reservation '{id}' does not exist. Check active reservations with `fl reservations`"))
            }
            Self::CycleDetected { .. } => {
                Some("Remove one dependency edge to break the cycle".to_string())
            }
            Self::FileReserved { agent, glob } => Some(format!(
                "Wait for agent '{agent}' to release '{glob}', or run `fl release '{glob}' --agent {agent}`"
            )),
            Self::ReservationExpired => {
                Some("Re-acquire the reservation before proceeding".to_string())
            }
            Self::AgentDispatchFailed { .. } => {
                Some("Check agent command and task configuration, then retry".to_string())
            }
            Self::AgentAlreadyRunning { task_id } => Some(format!(
                "Wait for the running agent to finish, or check status with `fl agent history {task_id}`"
            )),
            Self::VersionConflict { entity_id, .. } => Some(format!(
                "Re-read the entity or resolve conflicts with `fl resolve {entity_id}`"
            )),
            Self::Validation(msg) => Some(format!("Fix input: {msg}")),
            Self::DaemonError { ref hint, .. } => hint.clone(),
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
            | Self::MessageAlreadyRead { .. }
            | Self::AgentRunNotFound { .. }
            | Self::ReservationNotFound { .. } => 3,
            Self::Validation(_)
            | Self::Protocol(_)
            | Self::TypeMismatch { .. }
            | Self::VersionConflict { .. } => 4,
            Self::CycleDetected { .. } => 5,
            Self::FileReserved { .. } | Self::ReservationExpired => 6,
            Self::Io(_) => 7,
            Self::AgentDispatchFailed { .. } | Self::AgentAlreadyRunning { .. } => 8,
            Self::DaemonError { exit_code, .. } => *exit_code,
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
    #[serde(default)]
    pub exit_code: i32,
}

impl StructuredError {
    /// Reconstruct a [`FilamentError`] from this wire-format error.
    ///
    /// Preserves exit code, error code, hint, and retryable status
    /// so that errors round-trip correctly through the daemon protocol.
    pub fn into_error(self) -> FilamentError {
        FilamentError::DaemonError {
            exit_code: self.exit_code,
            code: self.code,
            message: self.message,
            hint: self.hint,
            retryable: self.retryable,
        }
    }
}

impl From<&FilamentError> for StructuredError {
    fn from(err: &FilamentError) -> Self {
        Self {
            code: err.error_code().to_string(),
            message: err.to_string(),
            hint: err.hint(),
            retryable: err.is_retryable(),
            exit_code: err.exit_code(),
        }
    }
}

/// Convenience alias used throughout the codebase.
pub type Result<T> = std::result::Result<T, FilamentError>;
