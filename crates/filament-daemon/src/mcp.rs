use filament_core::connection::FilamentConnection;
use filament_core::error::{FilamentError, StructuredError};
use filament_core::models::{
    CreateEntityRequest, CreateRelationRequest, SendMessageRequest, TtlSeconds,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler, ServiceExt};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::debug;

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

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
    pub msg_type: Option<String>,
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
    pub entity_type: Option<String>,
    /// Filter by status: open, `in_progress`, closed, blocked.
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddParams {
    /// Entity name.
    pub name: String,
    /// Entity type: task, module, service, agent, plan, doc.
    pub entity_type: String,
    /// Short summary.
    pub summary: String,
    /// Priority 0-4 (0=highest, default: 2).
    pub priority: Option<u8>,
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
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RelateParams {
    /// Source entity slug (or ID).
    pub source: String,
    /// Relation type: blocks, `depends_on`, produces, owns, `relates_to`, `assigned_to`.
    pub relation_type: String,
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
    pub relation_type: String,
    /// Target entity slug (or ID).
    pub target: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteParams {
    /// Entity slug (or ID) to delete.
    pub slug: String,
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// Number of MCP tools exposed by the server.
pub const TOOL_COUNT: usize = 16;

#[derive(Clone)]
pub struct FilamentMcp {
    conn: std::sync::Arc<Mutex<FilamentConnection>>,
    tool_router: ToolRouter<Self>,
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
        }
    }

    // -- Agent workflow tools --

    /// Get ranked actionable tasks (unblocked, priority-sorted).
    #[tool(name = "filament_task_ready")]
    async fn task_ready(
        &self,
        Parameters(p): Parameters<TaskReadyParams>,
    ) -> Result<String, String> {
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
    #[tool(name = "filament_task_close")]
    async fn task_close(
        &self,
        Parameters(p): Parameters<TaskCloseParams>,
    ) -> Result<String, String> {
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
    #[tool(name = "filament_context")]
    async fn context(&self, Parameters(p): Parameters<ContextParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        let depth = p.depth.unwrap_or(2);
        let summaries = conn
            .context_summaries(entity.id().as_str(), depth)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&summaries).expect("JSON"))
    }

    /// Send a targeted message to another agent.
    #[tool(name = "filament_message_send")]
    async fn message_send(
        &self,
        Parameters(p): Parameters<MessageSendParams>,
    ) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        // Validate recipient exists and is an agent
        conn.resolve_agent(&p.to_agent)
            .await
            .map_err(|e| map_err(&e))?;
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
    #[tool(name = "filament_message_inbox")]
    async fn message_inbox(
        &self,
        Parameters(p): Parameters<MessageInboxParams>,
    ) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let msgs = conn.get_inbox(&p.agent).await.map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&msgs).expect("JSON"))
    }

    /// Acquire an advisory file lock.
    #[tool(name = "filament_reserve")]
    async fn reserve(&self, Parameters(p): Parameters<ReserveParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let exclusive = p.exclusive.unwrap_or(false);
        let ttl_val = p.ttl_secs.unwrap_or(300);
        let ttl = TtlSeconds::new(ttl_val).map_err(|e| map_err(&e))?;
        let id = conn
            .acquire_reservation(&p.agent, &p.file_glob, exclusive, ttl)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(format!("Reservation acquired: {id}"))
    }

    /// Release a file reservation.
    #[tool(name = "filament_release")]
    async fn release(&self, Parameters(p): Parameters<ReleaseParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        conn.release_reservation(&p.reservation_id)
            .await
            .map_err(|e| map_err(&e))?;
        Ok("Reservation released".to_string())
    }

    /// List active file reservations.
    #[tool(name = "filament_reservations")]
    async fn reservations(
        &self,
        Parameters(p): Parameters<ReservationsParams>,
    ) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let reservations = conn
            .list_reservations(p.agent.as_deref())
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&reservations).expect("JSON"))
    }

    // -- Entity CRUD tools --

    /// Get entity details and its relations.
    #[tool(name = "filament_inspect")]
    async fn inspect(&self, Parameters(p): Parameters<InspectParams>) -> Result<String, String> {
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
    #[tool(name = "filament_list")]
    async fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let entity_type: Option<filament_core::models::EntityType> = p
            .entity_type
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(|e: FilamentError| map_err(&e))?;
        let status: Option<filament_core::models::EntityStatus> = p
            .status
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(|e: FilamentError| map_err(&e))?;
        let entities = conn
            .list_entities(entity_type, status)
            .await
            .map_err(|e| map_err(&e))?;
        Ok(serde_json::to_string_pretty(&entities).expect("JSON"))
    }

    /// Create a new entity (task, doc, module, etc.).
    #[tool(name = "filament_add")]
    async fn add(&self, Parameters(p): Parameters<AddParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let req = CreateEntityRequest {
            name: p.name,
            entity_type: p.entity_type,
            summary: Some(p.summary),
            key_facts: p.key_facts,
            content_path: p.content_path,
            priority: p.priority,
        };
        let (id, slug) = conn.create_entity(req).await.map_err(|e| map_err(&e))?;
        Ok(format!("Created: {slug} ({id})"))
    }

    /// Update entity summary and/or status.
    #[tool(name = "filament_update")]
    async fn update(&self, Parameters(p): Parameters<UpdateParams>) -> Result<String, String> {
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

        if let Some(ref summary) = p.summary {
            conn.update_entity_summary(id, summary)
                .await
                .map_err(|e| map_err(&e))?;
        }
        if let Some(ref status_str) = p.status {
            let status: filament_core::models::EntityStatus =
                status_str.parse().map_err(|e: FilamentError| map_err(&e))?;
            conn.update_entity_status(id, status)
                .await
                .map_err(|e| map_err(&e))?;
        }

        let mut parts = Vec::new();
        if p.summary.is_some() {
            parts.push("summary");
        }
        if p.status.is_some() {
            parts.push("status");
        }
        Ok(format!(
            "Updated {} for: {} ({})",
            parts.join(" and "),
            entity.name(),
            entity.slug()
        ))
    }

    /// Delete an entity and its relations.
    #[tool(name = "filament_delete")]
    async fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let entity = conn
            .resolve_entity(&p.slug)
            .await
            .map_err(|e| map_err(&e))?;
        conn.delete_entity(entity.id().as_str())
            .await
            .map_err(|e| map_err(&e))?;
        Ok(format!("Deleted: {} ({})", entity.name(), entity.slug()))
    }

    /// Create a relation between two entities.
    #[tool(name = "filament_relate")]
    async fn relate(&self, Parameters(p): Parameters<RelateParams>) -> Result<String, String> {
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
    #[tool(name = "filament_unrelate")]
    async fn unrelate(&self, Parameters(p): Parameters<UnrelateParams>) -> Result<String, String> {
        let mut conn = self.conn.lock().await;
        let source = conn
            .resolve_entity(&p.source)
            .await
            .map_err(|e| map_err(&e))?;
        let target = conn
            .resolve_entity(&p.target)
            .await
            .map_err(|e| map_err(&e))?;
        conn.delete_relation(source.id().as_str(), target.id().as_str(), &p.relation_type)
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
    #[tool(name = "filament_message_read")]
    async fn message_read(
        &self,
        Parameters(p): Parameters<MessageReadParams>,
    ) -> Result<String, String> {
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
///
/// # Errors
///
/// Returns an error if the server fails to start or the transport closes unexpectedly.
pub async fn run_mcp_stdio(conn: FilamentConnection) -> filament_core::error::Result<()> {
    debug!("starting MCP stdio server");
    let server = FilamentMcp::new(conn);
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
    let server = FilamentMcp::new(conn);
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
