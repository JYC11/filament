use std::path::Path;

use tokio::net::UnixStream;

use crate::client::DaemonClient;
use crate::error::{FilamentError, Result};
use crate::graph::KnowledgeGraph;
use crate::models::{
    CreateEntityRequest, CreateRelationRequest, Entity, EntityCommon, EntityId, EntityStatus,
    EntityType, Event, Message, MessageId, Relation, RelationId, Reservation, ReservationId,
    SendMessageRequest, Slug, TtlSeconds, ValidCreateEntityRequest, ValidCreateRelationRequest,
    ValidSendMessageRequest,
};
use crate::schema::init_pool;
use crate::store::{self, FilamentStore};

/// Connection mode: direct `SQLite` or via daemon socket.
pub enum FilamentConnection {
    /// Direct `SQLite` access (single-user mode).
    Direct(FilamentStore),
    /// Connected to daemon via Unix socket (multi-agent mode).
    Socket(DaemonClient),
}

/// Runtime directory name created by `filament init`.
const RUNTIME_DIR: &str = ".filament";
const SOCKET_NAME: &str = "filament.sock";
const DB_NAME: &str = "filament.db";

#[allow(clippy::missing_errors_doc)]
impl FilamentConnection {
    /// Auto-detect connection mode.
    /// If `.filament/filament.sock` exists and is connectable, use Socket.
    /// Otherwise, open a Direct connection to `.filament/filament.db`.
    ///
    /// # Errors
    ///
    /// Returns an error if neither the socket nor database can be opened.
    pub async fn auto_detect(project_root: &Path) -> Result<Self> {
        let runtime_dir = project_root.join(RUNTIME_DIR);
        let sock_path = runtime_dir.join(SOCKET_NAME);

        // Try socket first (daemon mode)
        if sock_path.exists() {
            if let Ok(stream) = UnixStream::connect(&sock_path).await {
                return Ok(Self::Socket(DaemonClient::from_stream(stream)));
            }
        }

        // Fall back to direct mode
        let db_path = runtime_dir.join(DB_NAME);
        let db_str = db_path.to_str().ok_or_else(|| {
            crate::error::FilamentError::Validation(format!(
                "database path is not valid UTF-8: {}",
                db_path.display()
            ))
        })?;
        let pool = init_pool(db_str).await?;
        Ok(Self::Direct(FilamentStore::new(pool)))
    }

    /// Open a direct connection to a specific database path.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Database` if the pool fails to connect.
    pub async fn direct(db_path: &str) -> Result<Self> {
        let pool = init_pool(db_path).await?;
        Ok(Self::Direct(FilamentStore::new(pool)))
    }

    // -----------------------------------------------------------------------
    // Dispatch methods — route through Direct (SQLite) or Socket (daemon)
    // -----------------------------------------------------------------------

    pub async fn create_entity(&mut self, req: CreateEntityRequest) -> Result<(EntityId, Slug)> {
        match self {
            Self::Direct(s) => {
                let valid = ValidCreateEntityRequest::try_from(req)?;
                s.with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::create_entity(conn, &valid).await })
                })
                .await
            }
            Self::Socket(c) => {
                let params = serde_json::to_value(&req)
                    .map_err(|e| FilamentError::Protocol(e.to_string()))?;
                c.create_entity(params).await
            }
        }
    }

    pub async fn get_entity(&mut self, id: &str) -> Result<Entity> {
        match self {
            Self::Direct(s) => store::get_entity(s.pool(), id).await,
            Self::Socket(c) => c.get_entity(id).await,
        }
    }

    pub async fn get_entity_by_slug(&mut self, slug: &str) -> Result<Entity> {
        match self {
            Self::Direct(s) => store::get_entity_by_slug(s.pool(), slug).await,
            Self::Socket(c) => c.get_entity_by_slug(slug).await,
        }
    }

    /// Resolve an entity by slug/ID and verify it is a Task.
    pub async fn resolve_task(&mut self, slug_or_id: &str) -> Result<EntityCommon> {
        let entity = self.resolve_entity(slug_or_id).await?;
        match entity {
            Entity::Task(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Task,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }

    /// Resolve an entity by slug/ID and verify it is an Agent.
    pub async fn resolve_agent(&mut self, slug_or_id: &str) -> Result<EntityCommon> {
        let entity = self.resolve_entity(slug_or_id).await?;
        match entity {
            Entity::Agent(c) => Ok(c),
            other => Err(FilamentError::TypeMismatch {
                expected: EntityType::Agent,
                actual: other.entity_type(),
                slug: other.slug().clone(),
            }),
        }
    }

    /// Resolve entity by slug (first) or UUID fallback.
    pub async fn resolve_entity(&mut self, slug_or_id: &str) -> Result<Entity> {
        match self {
            Self::Direct(s) => store::resolve_entity(s.pool(), slug_or_id).await,
            Self::Socket(c) => {
                // Try slug first, then ID
                match c.get_entity_by_slug(slug_or_id).await {
                    Ok(entity) => Ok(entity),
                    Err(FilamentError::Protocol(_) | FilamentError::EntityNotFound { .. }) => {
                        c.get_entity(slug_or_id).await
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub async fn list_entities(
        &mut self,
        entity_type: Option<EntityType>,
        status: Option<EntityStatus>,
    ) -> Result<Vec<Entity>> {
        match self {
            Self::Direct(s) => {
                store::list_entities(
                    s.pool(),
                    entity_type.as_ref().map(EntityType::as_str),
                    status.as_ref().map(EntityStatus::as_str),
                )
                .await
            }
            Self::Socket(c) => c.list_entities(entity_type, status).await,
        }
    }

    pub async fn update_entity_summary(&mut self, id: &str, summary: &str) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let id = id.to_string();
                let summary = summary.to_string();
                s.with_transaction(|conn| {
                    let id = id.clone();
                    let summary = summary.clone();
                    Box::pin(async move { store::update_entity_summary(conn, &id, &summary).await })
                })
                .await
            }
            Self::Socket(c) => c.update_entity_summary(id, summary).await,
        }
    }

    pub async fn update_entity_status(&mut self, id: &str, status: EntityStatus) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let id = id.to_string();
                s.with_transaction(|conn| {
                    let id = id.clone();
                    let status = status.clone();
                    Box::pin(async move { store::update_entity_status(conn, &id, status).await })
                })
                .await
            }
            Self::Socket(c) => c.update_entity_status(id, status).await,
        }
    }

    pub async fn delete_entity(&mut self, id: &str) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let id = id.to_string();
                s.with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::delete_entity(conn, &id).await })
                })
                .await
            }
            Self::Socket(c) => c.delete_entity(id).await,
        }
    }

    // -----------------------------------------------------------------------
    // Relation dispatch methods
    // -----------------------------------------------------------------------

    pub async fn create_relation(&mut self, req: CreateRelationRequest) -> Result<RelationId> {
        match self {
            Self::Direct(s) => {
                let valid = ValidCreateRelationRequest::try_from(req)?;
                s.with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::create_relation(conn, &valid).await })
                })
                .await
            }
            Self::Socket(c) => {
                let params = serde_json::to_value(&req)
                    .map_err(|e| FilamentError::Protocol(e.to_string()))?;
                c.create_relation(params).await
            }
        }
    }

    pub async fn list_relations(&mut self, entity_id: &str) -> Result<Vec<Relation>> {
        match self {
            Self::Direct(s) => store::list_relations(s.pool(), entity_id).await,
            Self::Socket(c) => c.list_relations(entity_id).await,
        }
    }

    pub async fn delete_relation(
        &mut self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
    ) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let source_id = source_id.to_string();
                let target_id = target_id.to_string();
                let relation_type = relation_type.to_string();
                s.with_transaction(|conn| {
                    let source_id = source_id.clone();
                    let target_id = target_id.clone();
                    let relation_type = relation_type.clone();
                    Box::pin(async move {
                        store::delete_relation_by_endpoints(
                            conn,
                            &source_id,
                            &target_id,
                            &relation_type,
                        )
                        .await
                    })
                })
                .await
            }
            Self::Socket(c) => c.delete_relation(source_id, target_id, relation_type).await,
        }
    }

    // -----------------------------------------------------------------------
    // Message dispatch methods
    // -----------------------------------------------------------------------

    pub async fn send_message(&mut self, req: SendMessageRequest) -> Result<MessageId> {
        match self {
            Self::Direct(s) => {
                let valid = ValidSendMessageRequest::try_from(req)?;
                s.with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::send_message(conn, &valid).await })
                })
                .await
            }
            Self::Socket(c) => {
                let params = serde_json::to_value(&req)
                    .map_err(|e| FilamentError::Protocol(e.to_string()))?;
                c.send_message(params).await
            }
        }
    }

    pub async fn get_inbox(&mut self, agent: &str) -> Result<Vec<Message>> {
        match self {
            Self::Direct(s) => store::get_inbox(s.pool(), agent).await,
            Self::Socket(c) => c.get_inbox(agent).await,
        }
    }

    pub async fn mark_message_read(&mut self, id: &str) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let id = id.to_string();
                s.with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::mark_message_read(conn, &id).await })
                })
                .await
            }
            Self::Socket(c) => c.mark_message_read(id).await,
        }
    }

    // -----------------------------------------------------------------------
    // Reservation dispatch methods
    // -----------------------------------------------------------------------

    pub async fn acquire_reservation(
        &mut self,
        agent: &str,
        glob: &str,
        exclusive: bool,
        ttl: TtlSeconds,
    ) -> Result<ReservationId> {
        match self {
            Self::Direct(s) => {
                let agent = agent.to_string();
                let glob = glob.to_string();
                s.with_transaction(|conn| {
                    let agent = agent.clone();
                    let glob = glob.clone();
                    Box::pin(async move {
                        store::acquire_reservation(conn, &agent, &glob, exclusive, ttl).await
                    })
                })
                .await
            }
            Self::Socket(c) => {
                c.acquire_reservation(agent, glob, exclusive, ttl.value())
                    .await
            }
        }
    }

    pub async fn find_reservation(
        &mut self,
        glob: &str,
        agent: &str,
    ) -> Result<Option<Reservation>> {
        match self {
            Self::Direct(s) => store::find_reservation(s.pool(), glob, agent).await,
            Self::Socket(c) => c.find_reservation(glob, agent).await,
        }
    }

    pub async fn list_reservations(&mut self, agent: Option<&str>) -> Result<Vec<Reservation>> {
        match self {
            Self::Direct(s) => store::list_reservations(s.pool(), agent).await,
            Self::Socket(c) => c.list_reservations(agent).await,
        }
    }

    pub async fn release_reservation(&mut self, id: &str) -> Result<()> {
        match self {
            Self::Direct(s) => {
                let id = id.to_string();
                s.with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::release_reservation(conn, &id).await })
                })
                .await
            }
            Self::Socket(c) => c.release_reservation(id).await,
        }
    }

    pub async fn expire_stale_reservations(&mut self) -> Result<u64> {
        match self {
            Self::Direct(s) => {
                s.with_transaction(|conn| {
                    Box::pin(async move { store::expire_stale_reservations(conn).await })
                })
                .await
            }
            Self::Socket(c) => c.expire_stale_reservations().await,
        }
    }

    // -----------------------------------------------------------------------
    // Dispatch methods (require daemon)
    // -----------------------------------------------------------------------

    pub async fn dispatch_agent(
        &mut self,
        task_slug: &str,
        role: &str,
    ) -> Result<crate::models::AgentRunId> {
        match self {
            Self::Direct(_) => Err(FilamentError::AgentDispatchFailed {
                reason: "dispatch requires daemon mode (run `filament serve` first)".to_string(),
            }),
            Self::Socket(c) => c.dispatch_agent(task_slug, role).await,
        }
    }

    pub async fn dispatch_batch(
        &mut self,
        role: &str,
        max_parallel: Option<usize>,
    ) -> Result<serde_json::Value> {
        match self {
            Self::Direct(_) => Err(FilamentError::AgentDispatchFailed {
                reason: "dispatch requires daemon mode (run `filament serve` first)".to_string(),
            }),
            Self::Socket(c) => c.dispatch_batch(role, max_parallel).await,
        }
    }

    pub async fn get_agent_run(&mut self, run_id: &str) -> Result<crate::models::AgentRun> {
        match self {
            Self::Direct(s) => store::get_agent_run(s.pool(), run_id).await,
            Self::Socket(c) => c.get_agent_run(run_id).await,
        }
    }

    pub async fn list_agent_runs_by_task(
        &mut self,
        task_id: &str,
    ) -> Result<Vec<crate::models::AgentRun>> {
        match self {
            Self::Direct(s) => store::list_agent_runs_by_task(s.pool(), task_id).await,
            Self::Socket(c) => c.list_agent_runs_by_task(task_id).await,
        }
    }

    pub async fn list_running_agents(&mut self) -> Result<Vec<crate::models::AgentRun>> {
        match self {
            Self::Direct(s) => store::list_running_agents(s.pool()).await,
            Self::Socket(c) => c.list_running_agents().await,
        }
    }

    // -----------------------------------------------------------------------
    // Graph dispatch methods
    // -----------------------------------------------------------------------

    pub async fn ready_tasks(&mut self) -> Result<Vec<Entity>> {
        match self {
            Self::Direct(s) => {
                let mut conn = s.pool().acquire().await.map_err(FilamentError::Database)?;
                store::ready_tasks(&mut conn).await
            }
            Self::Socket(c) => c.ready_tasks().await,
        }
    }

    pub async fn critical_path(&mut self, entity_id: &str) -> Result<Vec<EntityId>> {
        match self {
            Self::Direct(s) => {
                let mut graph = KnowledgeGraph::new();
                graph.hydrate(s.pool()).await?;
                Ok(graph.critical_path(entity_id))
            }
            Self::Socket(c) => c.critical_path(entity_id).await,
        }
    }

    pub async fn impact_score(&mut self, entity_id: &str) -> Result<usize> {
        match self {
            Self::Direct(s) => {
                let mut graph = KnowledgeGraph::new();
                graph.hydrate(s.pool()).await?;
                Ok(graph.impact_score(entity_id))
            }
            Self::Socket(c) => c.impact_score(entity_id).await,
        }
    }

    pub async fn context_summaries(
        &mut self,
        entity_id: &str,
        depth: usize,
    ) -> Result<Vec<String>> {
        match self {
            Self::Direct(s) => {
                let mut graph = KnowledgeGraph::new();
                graph.hydrate(s.pool()).await?;
                Ok(graph.context_summaries(entity_id, depth))
            }
            Self::Socket(c) => c.context_query(entity_id, Some(depth)).await,
        }
    }

    pub async fn check_cycle(&mut self) -> Result<bool> {
        match self {
            Self::Direct(s) => {
                let mut graph = KnowledgeGraph::new();
                graph.hydrate(s.pool()).await?;
                Ok(graph.has_cycle())
            }
            Self::Socket(c) => c.check_cycle().await,
        }
    }

    pub async fn get_entity_events(&mut self, entity_id: &str) -> Result<Vec<Event>> {
        match self {
            Self::Direct(s) => store::get_entity_events(s.pool(), entity_id).await,
            Self::Socket(c) => c.get_entity_events(entity_id).await,
        }
    }
}
