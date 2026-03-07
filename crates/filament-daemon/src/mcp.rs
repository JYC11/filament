use std::collections::HashSet;

use filament_core::connection::FilamentConnection;
use filament_core::dto::{CreateEntityRequest, CreateRelationRequest, SendMessageRequest};
use filament_core::error::{FilamentError, StructuredError};
use filament_core::models::{ReservationMode, TtlSeconds};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use tokio::sync::Mutex;
use tracing::debug;

use crate::dto::{
    AddParams, ContextParams, DeleteParams, InspectParams, ListParams, MessageInboxParams,
    MessageReadParams, MessageSendParams, RelateParams, ReleaseParams, ReservationsParams,
    ReserveParams, TaskCloseParams, TaskReadyParams, UnrelateParams, UpdateParams,
};

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// Number of MCP tools exposed by the server.
pub const TOOL_COUNT: usize = 16;

#[derive(Clone)]
pub struct FilamentMcp {
    conn: std::sync::Arc<Mutex<FilamentConnection>>,
    tool_router: ToolRouter<Self>,
    /// When set, only these tool names are allowed. Others return an error.
    allowed_tools: Option<HashSet<String>>,
}

fn map_err(e: &FilamentError) -> String {
    let structured = StructuredError::from(e);
    serde_json::to_string_pretty(&structured).unwrap_or_else(|_| e.to_string())
}

#[tool_router]
#[allow(clippy::significant_drop_tightening)]
impl FilamentMcp {
    pub fn new(conn: FilamentConnection) -> Self {
        Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
            tool_router: Self::tool_router(),
            allowed_tools: None,
        }
    }

    pub fn new_filtered(conn: FilamentConnection, allowed: &[&str]) -> Self {
        Self {
            conn: std::sync::Arc::new(Mutex::new(conn)),
            tool_router: Self::tool_router(),
            allowed_tools: Some(allowed.iter().map(|s| (*s).to_string()).collect()),
        }
    }

    fn check_allowed(&self, tool_name: &str) -> Result<(), String> {
        if let Some(ref allowed) = self.allowed_tools {
            if !allowed.contains(tool_name) {
                return Err(format!(
                    "tool '{tool_name}' is not allowed for this agent role"
                ));
            }
        }
        Ok(())
    }

    // -- Agent workflow tools --

    /// Get ranked actionable tasks (unblocked, priority-sorted).
    #[tool(name = "fl_task_ready")]
    async fn task_ready(
        &self,
        Parameters(p): Parameters<TaskReadyParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_task_ready")?;
        let mut conn = self.conn.lock().await;
        match conn.ready_tasks().await {
            Ok(mut tasks) => {
                if let Some(limit) = p.limit {
                    tasks.truncate(limit);
                }
                Ok(serde_json::to_string_pretty(&tasks).expect("JSON"))
            }
            Err(e) => Err(map_err(&e)),
        }
    }

    /// Mark a task as closed/complete.
    #[tool(name = "fl_task_close")]
    async fn task_close(
        &self,
        Parameters(p): Parameters<TaskCloseParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_task_close")?;
        let mut conn = self.conn.lock().await;
        let task = conn.resolve_task(&p.slug).await.map_err(|e| map_err(&e))?;
        conn.update_entity_status(
            task.id.as_str(),
            filament_core::models::EntityStatus::Closed,
        )
        .await
        .map_err(|e| map_err(&e))?;
        Ok(format!("Closed: {} ({})", task.name, task.slug))
    }

    /// Get graph neighborhood summaries around an entity.
    #[tool(name = "fl_context")]
    async fn context(&self, Parameters(p): Parameters<ContextParams>) -> Result<String, String> {
        self.check_allowed("fl_context")?;
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        let depth = p.depth.unwrap_or(2).min(10);
        let summaries = conn
            .context_summaries(entity.id().as_str(), depth)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&summaries).expect("JSON"))
    }

    /// Send a targeted message to another agent.
    #[tool(name = "fl_message_send")]
    async fn message_send(
        &self,
        Parameters(p): Parameters<MessageSendParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_message_send")?;
        let mut conn = self.conn.lock().await;
        // Validate recipient — allow "user" for escalations, otherwise must be an agent entity
        if p.to_agent != "user" {
            conn.resolve_agent(&p.to_agent)
                .await
                .map_err(|e| map_err(&e))?;
        }
        let req = SendMessageRequest {
            from_agent: p.from_agent,
            to_agent: p.to_agent,
            body: p.body,
            msg_type: p.msg_type,
            in_reply_to: p.in_reply_to,
            task_id: p.task_id,
        };
        let id = conn.send_message(req).await.map_err(|e| map_err(&e))?;
        Ok(format!("Message sent: {id}"))
    }

    /// Check unread messages for an agent.
    #[tool(name = "fl_message_inbox")]
    async fn message_inbox(
        &self,
        Parameters(p): Parameters<MessageInboxParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_message_inbox")?;
        let mut conn = self.conn.lock().await;
        let msgs = conn.get_inbox(&p.agent).await.map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&msgs).expect("JSON"))
    }

    /// Acquire an advisory file lock.
    #[tool(name = "fl_reserve")]
    async fn reserve(&self, Parameters(p): Parameters<ReserveParams>) -> Result<String, String> {
        self.check_allowed("fl_reserve")?;
        let mut conn = self.conn.lock().await;
        let mode = ReservationMode::from(p.exclusive.unwrap_or(false));
        let ttl_val = p.ttl_secs.unwrap_or(300);
        let ttl = TtlSeconds::new(ttl_val).map_err(|e| map_err(&e))?;
        let id = conn
            .acquire_reservation(&p.agent, &p.file_glob, mode, ttl)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(format!("Reservation acquired: {id}"))
    }

    /// Release a file reservation.
    #[tool(name = "fl_release")]
    async fn release(&self, Parameters(p): Parameters<ReleaseParams>) -> Result<String, String> {
        self.check_allowed("fl_release")?;
        let mut conn = self.conn.lock().await;
        conn.release_reservation(&p.reservation_id)
            .await
            .map_err(|e| map_err(&e))?;
        Ok("Reservation released".to_string())
    }

    /// List active file reservations.
    #[tool(name = "fl_reservations")]
    async fn reservations(
        &self,
        Parameters(p): Parameters<ReservationsParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_reservations")?;
        let mut conn = self.conn.lock().await;
        let reservations = conn
            .list_reservations(p.agent.as_deref())
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&reservations).expect("JSON"))
    }

    // -- Entity CRUD tools --

    /// Get entity details and its relations.
    #[tool(name = "fl_inspect")]
    async fn inspect(&self, Parameters(p): Parameters<InspectParams>) -> Result<String, String> {
        self.check_allowed("fl_inspect")?;
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        let relations = conn
            .list_relations(entity.id().as_str())
            .await
            .map_err(|e| map_err(&e))?;
        let result = serde_json::json!({
            "entity": entity,
            "relations": relations,
        });
        Ok(serde_json::to_string_pretty(&result).expect("JSON"))
    }

    /// List/filter entities by type and status.
    #[tool(name = "fl_list")]
    async fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<String, String> {
        self.check_allowed("fl_list")?;
        let mut conn = self.conn.lock().await;
        let entities = conn
            .list_entities(p.entity_type, p.status)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&entities).expect("JSON"))
    }

    /// Create a new entity (task, doc, module, etc.).
    #[tool(name = "fl_add")]
    async fn add(&self, Parameters(p): Parameters<AddParams>) -> Result<String, String> {
        self.check_allowed("fl_add")?;
        let mut conn = self.conn.lock().await;
        let req = CreateEntityRequest::from_parts(
            p.entity_type,
            p.name.clone(),
            Some(p.summary.clone()),
            p.priority,
            p.key_facts.clone(),
            p.content_path,
        )
        .map_err(|e| map_err(&e))?;
        let (id, slug) = conn.create_entity(req).await.map_err(|e| map_err(&e))?;
        Ok(format!("Created: {slug} ({id})"))
    }

    /// Update entity summary and/or status.
    #[tool(name = "fl_update")]
    async fn update(&self, Parameters(p): Parameters<UpdateParams>) -> Result<String, String> {
        self.check_allowed("fl_update")?;
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        let id = entity.id().as_str();

        if p.summary.is_none() && p.status.is_none() {
            return Err(map_err(&FilamentError::Validation(
                "at least one of summary or status is required".to_string(),
            )));
        }

        let mut parts = Vec::new();
        if p.summary.is_some() {
            parts.push("summary");
        }
        if p.status.is_some() {
            parts.push("status");
        }

        if let Some(ref summary) = p.summary {
            conn.update_entity_summary(id, summary)
                .await
                .map_err(|e| map_err(&e))?;
        }
        if let Some(status) = p.status {
            conn.update_entity_status(id, status)
                .await
                .map_err(|e| map_err(&e))?;
        }
        Ok(format!(
            "Updated {} for: {} ({})",
            parts.join(" and "),
            entity.name(),
            entity.slug()
        ))
    }

    /// Delete an entity and its relations.
    #[tool(name = "fl_delete")]
    async fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<String, String> {
        self.check_allowed("fl_delete")?;
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        conn.delete_entity(entity.id().as_str(), None)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(format!("Deleted: {} ({})", entity.name(), entity.slug()))
    }

    /// Create a relation between two entities.
    #[tool(name = "fl_relate")]
    async fn relate(&self, Parameters(p): Parameters<RelateParams>) -> Result<String, String> {
        self.check_allowed("fl_relate")?;
        let mut conn = self.conn.lock().await;
        let source = conn
            .resolve_entity(&p.source)
            .await
            .map_err(|e| map_err(&e))?;
        let target = conn
            .resolve_entity(&p.target)
            .await
            .map_err(|e| map_err(&e))?;
        let req = CreateRelationRequest {
            source_id: source.id().to_string(),
            target_id: target.id().to_string(),
            relation_type: p.relation_type,
            weight: p.weight,
            summary: p.summary,
            metadata: None,
        };
        let id = conn.create_relation(req).await.map_err(|e| map_err(&e))?;
        Ok(format!(
            "Related: {} -> {} ({})",
            source.name(),
            target.name(),
            id
        ))
    }

    /// Remove a relation between two entities.
    #[tool(name = "fl_unrelate")]
    async fn unrelate(&self, Parameters(p): Parameters<UnrelateParams>) -> Result<String, String> {
        self.check_allowed("fl_unrelate")?;
        let mut conn = self.conn.lock().await;
        let source = conn
            .resolve_entity(&p.source)
            .await
            .map_err(|e| map_err(&e))?;
        let target = conn
            .resolve_entity(&p.target)
            .await
            .map_err(|e| map_err(&e))?;
        conn.delete_relation(
            source.id().as_str(),
            target.id().as_str(),
            p.relation_type.as_str(),
        )
        .await
        .map_err(|e| map_err(&e))?;
        Ok(format!(
            "Unrelated: {} -/-> {} ({})",
            source.name(),
            target.name(),
            p.relation_type
        ))
    }

    /// Mark a message as read.
    #[tool(name = "fl_message_read")]
    async fn message_read(
        &self,
        Parameters(p): Parameters<MessageReadParams>,
    ) -> Result<String, String> {
        self.check_allowed("fl_message_read")?;
        let mut conn = self.conn.lock().await;
        conn.mark_message_read(&p.message_id)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(format!("Message marked as read: {}", p.message_id))
    }
}

#[tool_handler]
impl ServerHandler for FilamentMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Filament — local multi-agent orchestration, knowledge graph, and task management"
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the MCP server on stdio.
/// If `allowed_tools` is provided, only those tools are accessible.
///
/// # Errors
///
/// Returns an error if the server fails to start or the transport closes unexpectedly.
pub async fn run_mcp_stdio(
    conn: FilamentConnection,
    allowed_tools: Option<&[&str]>,
) -> filament_core::error::Result<()> {
    debug!("starting MCP stdio server");
    let server = if let Some(tools) = allowed_tools {
        debug!(tools = ?tools, "MCP tool filtering enabled");
        FilamentMcp::new_filtered(conn, tools)
    } else {
        FilamentMcp::new(conn)
    };
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| FilamentError::Protocol(format!("MCP server error: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| FilamentError::Protocol(format!("MCP server error: {e}")))?;
    debug!("MCP stdio server stopped");
    Ok(())
}

/// Run the MCP server on a generic async read/write transport (for testing).
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn run_mcp_transport<T>(
    conn: FilamentConnection,
    transport: T,
) -> filament_core::error::Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    run_mcp_transport_filtered(conn, transport, None).await
}

/// Run the MCP server on a generic transport with optional tool filtering.
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn run_mcp_transport_filtered<T>(
    conn: FilamentConnection,
    transport: T,
    allowed_tools: Option<&[&str]>,
) -> filament_core::error::Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let server = if let Some(tools) = allowed_tools {
        FilamentMcp::new_filtered(conn, tools)
    } else {
        FilamentMcp::new(conn)
    };
    let service = server
        .serve(transport)
        .await
        .map_err(|e| FilamentError::Protocol(format!("MCP server error: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| FilamentError::Protocol(format!("MCP server error: {e}")))?;
    Ok(())
}
