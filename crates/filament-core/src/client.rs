use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;

use crate::error::{FilamentError, Result};
use crate::models::{AgentRun, Entity, EntityId, Event, Message, Relation, Reservation};
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

    pub async fn create_entity(&mut self, params: serde_json::Value) -> Result<EntityId> {
        let result = self.call(Method::CreateEntity, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(EntityId::from(id))
    }

    pub async fn get_entity(&mut self, id: &str) -> Result<Entity> {
        let result = self
            .call(Method::GetEntity, serde_json::json!({ "id": id }))
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn list_entities(
        &mut self,
        entity_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Entity>> {
        let result = self
            .call(
                Method::ListEntities,
                serde_json::json!({ "entity_type": entity_type, "status": status }),
            )
            .await?;
        serde_json::from_value(result).map_err(|e| FilamentError::Protocol(e.to_string()))
    }

    pub async fn update_entity_status(&mut self, id: &str, status: &str) -> Result<()> {
        self.call(
            Method::UpdateEntityStatus,
            serde_json::json!({ "id": id, "status": status }),
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

    pub async fn create_relation(&mut self, params: serde_json::Value) -> Result<String> {
        let result = self.call(Method::CreateRelation, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(id)
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

    pub async fn send_message(&mut self, params: serde_json::Value) -> Result<String> {
        let result = self.call(Method::SendMessage, params).await?;
        let id: String = serde_json::from_value(result["id"].clone())
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        Ok(id)
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
    ) -> Result<String> {
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
        Ok(id)
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
    ) -> Result<String> {
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
        Ok(id)
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

    // -- Reservation listing (uses list_entities pattern) --

    pub fn list_reservations(&mut self, _agent: Option<&str>) -> Result<Vec<Reservation>> {
        Err(FilamentError::Protocol(
            "list_reservations not available over socket (no protocol method)".to_string(),
        ))
    }
}
