use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;

use crate::error::{FilamentError, Result};
use crate::models::{
    AgentRun, AgentRunId, Entity, EntityId, EntityStatus, EntityType, Event, Message, MessageId,
    Relation, RelationId, Reservation, ReservationId, Slug,
};
use crate::protocol::{Method, Request, Response};

/// Client for communicating with the filament daemon over a Unix socket.
pub struct DaemonClient {
    reader: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: BufWriter<tokio::net::unix::OwnedWriteHalf>,
    next_id: u64,
}

#[allow(clippy::missing_errors_doc)]
impl DaemonClient {
    /// Connect to a daemon at the given socket path.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Io` if the connection fails.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self::from_stream(stream))
    }

    /// Wrap an existing `UnixStream`.
    pub fn from_stream(stream: UnixStream) -> Self {
        let (read_half, write_half) = stream.into_split();
        Self {
            reader: BufReader::new(read_half).lines(),
            writer: BufWriter::new(write_half),
            next_id: 1,
        }
    }

    async fn call(
        &mut self,
        method: Method,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.to_string();
        self.next_id += 1;

        let request = Request {
            id: id.clone(),
            method,
            params,
        };

        let line = serde_json::to_string(&request).expect("infallible");
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(FilamentError::Io)?;
        self.writer
            .write_all(b"\n")
            .await
            .map_err(FilamentError::Io)?;
        self.writer.flush().await.map_err(FilamentError::Io)?;

        let response_line = self
            .reader
            .next_line()
            .await
            .map_err(FilamentError::Io)?
            .ok_or_else(|| FilamentError::Protocol("connection closed".to_string()))?;

        let response: Response = serde_json::from_str(&response_line)
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;

        if response.id != id {
            return Err(FilamentError::Protocol(format!(
                "response id mismatch: expected {id}, got {}",
                response.id
            )));
        }

        if let Some(err) = response.error {
            return Err(FilamentError::Protocol(format!(
                "{}: {}",
                err.code, err.message
            )));
        }

        response
            .result
            .ok_or_else(|| FilamentError::Protocol("empty response".to_string()))
    }

    // -- Entity operations --

    pub async fn create_entity(&mut self, params: serde_json::Value) -> Result<(EntityId, Slug)> {
        let result = self.call(Method::CreateEntity, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        let slug: String = serde_json::from_value(result["slug"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        let slug = Slug::try_from(slug).map_err(FilamentError::Protocol)?;
        Ok((EntityId::from(id), slug))
    }

    pub async fn get_entity(&mut self, id: &str) -> Result<Entity> {
        let result = self
            .call(Method::GetEntity, serde_json::json!({ "id": id }))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn get_entity_by_slug(&mut self, slug: &str) -> Result<Entity> {
        let result = self
            .call(Method::GetEntityBySlug, serde_json::json!({ "slug": slug }))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn list_entities(
        &mut self,
        entity_type: Option<EntityType>,
        status: Option<EntityStatus>,
    ) -> Result<Vec<Entity>> {
        let result = self
            .call(
                Method::ListEntities,
                serde_json::json!({
                    "entity_type": entity_type.as_ref().map(EntityType::as_str),
                    "status": status.as_ref().map(EntityStatus::as_str),
                }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn update_entity_summary(&mut self, id: &str, summary: &str) -> Result<()> {
        self.call(
            Method::UpdateEntitySummary,
            serde_json::json!({ "id": id, "summary": summary }),
        )
        .await?;
        Ok(())
    }

    pub async fn update_entity_status(&mut self, id: &str, status: EntityStatus) -> Result<()> {
        self.call(
            Method::UpdateEntityStatus,
            serde_json::json!({ "id": id, "status": status.as_str() }),
        )
        .await?;
        Ok(())
    }

    pub async fn delete_entity(&mut self, id: &str) -> Result<()> {
        self.call(Method::DeleteEntity, serde_json::json!({ "id": id }))
            .await?;
        Ok(())
    }

    // -- Relation operations --

    pub async fn create_relation(&mut self, params: serde_json::Value) -> Result<RelationId> {
        let result = self.call(Method::CreateRelation, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(RelationId::from(id))
    }

    pub async fn list_relations(&mut self, entity_id: &str) -> Result<Vec<Relation>> {
        let result = self
            .call(
                Method::ListRelations,
                serde_json::json!({ "entity_id": entity_id }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn delete_relation(
        &mut self,
        source_id: &str,
        target_id: &str,
        relation_type: &str,
    ) -> Result<()> {
        self.call(
            Method::DeleteRelation,
            serde_json::json!({
                "source_id": source_id,
                "target_id": target_id,
                "relation_type": relation_type,
            }),
        )
        .await?;
        Ok(())
    }

    // -- Message operations --

    pub async fn send_message(&mut self, params: serde_json::Value) -> Result<MessageId> {
        let result = self.call(Method::SendMessage, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(MessageId::from(id))
    }

    pub async fn get_inbox(&mut self, agent: &str) -> Result<Vec<Message>> {
        let result = self
            .call(Method::GetInbox, serde_json::json!({ "agent": agent }))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn mark_message_read(&mut self, id: &str) -> Result<()> {
        self.call(Method::MarkMessageRead, serde_json::json!({ "id": id }))
            .await?;
        Ok(())
    }

    // -- Reservation operations --

    pub async fn acquire_reservation(
        &mut self,
        agent_name: &str,
        file_glob: &str,
        exclusive: bool,
        ttl_secs: u32,
    ) -> Result<ReservationId> {
        let result = self
            .call(
                Method::AcquireReservation,
                serde_json::json!({
                    "agent_name": agent_name,
                    "file_glob": file_glob,
                    "exclusive": exclusive,
                    "ttl_secs": ttl_secs,
                }),
            )
            .await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(ReservationId::from(id))
    }

    pub async fn find_reservation(
        &mut self,
        file_glob: &str,
        agent_name: &str,
    ) -> Result<Option<Reservation>> {
        let result = self
            .call(
                Method::FindReservation,
                serde_json::json!({ "file_glob": file_glob, "agent_name": agent_name }),
            )
            .await?;
        let inner = result
            .get("reservation")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        serde_json::from_value(inner).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn list_reservations(&mut self, agent: Option<&str>) -> Result<Vec<Reservation>> {
        let result = self
            .call(
                Method::ListReservations,
                serde_json::json!({ "agent": agent }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn release_reservation(&mut self, id: &str) -> Result<()> {
        self.call(Method::ReleaseReservation, serde_json::json!({ "id": id }))
            .await?;
        Ok(())
    }

    pub async fn expire_stale_reservations(&mut self) -> Result<u64> {
        let result = self
            .call(Method::ExpireStaleReservations, serde_json::json!({}))
            .await?;
        let count: u64 = serde_json::from_value(result["expired"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(count)
    }

    // -- Agent run operations --

    pub async fn create_agent_run(
        &mut self,
        task_id: &str,
        agent_role: &str,
        pid: Option<i32>,
    ) -> Result<AgentRunId> {
        let result = self
            .call(
                Method::CreateAgentRun,
                serde_json::json!({
                    "task_id": task_id,
                    "agent_role": agent_role,
                    "pid": pid,
                }),
            )
            .await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(AgentRunId::from(id))
    }

    pub async fn finish_agent_run(
        &mut self,
        id: &str,
        status: &str,
        result_json: Option<&str>,
    ) -> Result<()> {
        self.call(
            Method::FinishAgentRun,
            serde_json::json!({
                "id": id,
                "status": status,
                "result_json": result_json,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn list_running_agents(&mut self) -> Result<Vec<AgentRun>> {
        let result = self
            .call(Method::ListRunningAgents, serde_json::json!({}))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    // -- Dispatch operations --

    pub async fn dispatch_agent(&mut self, task_slug: &str, role: &str) -> Result<AgentRunId> {
        let result = self
            .call(
                Method::DispatchAgent,
                serde_json::json!({ "task_slug": task_slug, "role": role }),
            )
            .await?;
        let id: String = serde_json::from_value(result["run_id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(AgentRunId::from(id))
    }

    pub async fn get_agent_run(&mut self, run_id: &str) -> Result<AgentRun> {
        let result = self
            .call(Method::GetAgentRun, serde_json::json!({ "run_id": run_id }))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn list_agent_runs_by_task(&mut self, task_id: &str) -> Result<Vec<AgentRun>> {
        let result = self
            .call(
                Method::ListAgentRunsByTask,
                serde_json::json!({ "task_id": task_id }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    // -- Graph operations --

    pub async fn ready_tasks(&mut self) -> Result<Vec<Entity>> {
        let result = self.call(Method::ReadyTasks, serde_json::json!({})).await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn critical_path(&mut self, entity_id: &str) -> Result<Vec<EntityId>> {
        let result = self
            .call(
                Method::CriticalPath,
                serde_json::json!({ "entity_id": entity_id }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn impact_score(&mut self, entity_id: &str) -> Result<usize> {
        let result = self
            .call(
                Method::ImpactScore,
                serde_json::json!({ "entity_id": entity_id }),
            )
            .await?;
        let score: usize = serde_json::from_value(result["score"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(score)
    }

    pub async fn batch_impact_scores(
        &mut self,
        entity_ids: &[String],
    ) -> Result<std::collections::HashMap<String, usize>> {
        let result = self
            .call(
                Method::BatchImpactScores,
                serde_json::json!({ "entity_ids": entity_ids }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn blocked_by_counts(
        &mut self,
    ) -> Result<std::collections::HashMap<String, usize>> {
        let result = self
            .call(Method::BlockedByCounts, serde_json::json!({}))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn context_query(
        &mut self,
        entity_id: &str,
        depth: Option<usize>,
    ) -> Result<Vec<String>> {
        let result = self
            .call(
                Method::ContextQuery,
                serde_json::json!({ "entity_id": entity_id, "depth": depth }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn check_cycle(&mut self) -> Result<bool> {
        let result = self.call(Method::CheckCycle, serde_json::json!({})).await?;
        let has_cycle: bool = serde_json::from_value(result["has_cycle"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(has_cycle)
    }

    pub async fn get_entity_events(&mut self, entity_id: &str) -> Result<Vec<Event>> {
        let result = self
            .call(
                Method::GetEntityEvents,
                serde_json::json!({ "entity_id": entity_id }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }
}
