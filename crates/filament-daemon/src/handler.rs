use std::sync::Arc;

use filament_core::error::{FilamentError, Result, StructuredError};
use filament_core::models::{
    AgentStatus, CreateEntityRequest, CreateRelationRequest, EntityStatus, SendMessageRequest,
    TtlSeconds, ValidCreateEntityRequest, ValidCreateRelationRequest, ValidSendMessageRequest,
};
use filament_core::protocol::{Method, Request, Response};
use filament_core::store;
use serde::Deserialize;

use crate::server::SharedState;

/// Dispatch a request to the appropriate handler.
pub async fn dispatch(request: Request, state: &Arc<SharedState>) -> Response {
    let id = request.id.clone();
    match handle(request, state).await {
        Ok(value) => Response::success(id, value),
        Err(e) => Response::error(id, StructuredError::from(&e)),
    }
}

#[allow(clippy::too_many_lines)]
async fn handle(request: Request, state: &Arc<SharedState>) -> Result<serde_json::Value> {
    let params = request.params;

    match request.method {
        // -- Entity operations --
        Method::CreateEntity => {
            let req: CreateEntityRequest = parse_params(params)?;
            let valid = ValidCreateEntityRequest::try_from(req)?;
            let entity_id = state
                .store
                .with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::create_entity(conn, &valid).await })
                })
                .await?;

            // Update in-memory graph
            let entity = store::get_entity(state.store.pool(), entity_id.as_str()).await?;
            state.graph_write().await.add_node_from_entity(&entity);

            Ok(serde_json::json!({ "id": entity_id }))
        }

        Method::GetEntity => {
            let p: IdParam = parse_params(params)?;
            let entity = store::get_entity(state.store.pool(), &p.id).await?;
            Ok(serde_json::to_value(&entity).expect("infallible"))
        }

        Method::GetEntityByName => {
            let p: NameParam = parse_params(params)?;
            let entity = store::get_entity_by_name(state.store.pool(), &p.name).await?;
            Ok(serde_json::to_value(&entity).expect("infallible"))
        }

        Method::ListEntities => {
            let p: ListEntitiesParam = parse_params(params)?;
            let entities = store::list_entities(
                state.store.pool(),
                p.entity_type.as_deref(),
                p.status.as_deref(),
            )
            .await?;
            Ok(serde_json::to_value(&entities).expect("infallible"))
        }

        Method::UpdateEntitySummary => {
            let p: UpdateSummaryParam = parse_params(params)?;
            let id = p.id.clone();
            let summary = p.summary.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    let summary = summary.clone();
                    Box::pin(async move { store::update_entity_summary(conn, &id, &summary).await })
                })
                .await?;

            Ok(serde_json::json!({ "ok": true }))
        }

        Method::UpdateEntityStatus => {
            let p: UpdateStatusParam = parse_params(params)?;
            let status: EntityStatus = serde_json::from_value(serde_json::Value::String(
                p.status.clone(),
            ))
            .map_err(|_| FilamentError::Validation(format!("invalid status: '{}'", p.status)))?;

            let id = p.id.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    let status = status.clone();
                    Box::pin(async move { store::update_entity_status(conn, &id, status).await })
                })
                .await?;

            // Update in-memory graph
            let entity = store::get_entity(state.store.pool(), &p.id).await?;
            state.graph_write().await.add_node_from_entity(&entity);

            Ok(serde_json::json!({ "ok": true }))
        }

        Method::DeleteEntity => {
            let p: IdParam = parse_params(params)?;
            let id = p.id.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::delete_entity(conn, &id).await })
                })
                .await?;

            // Update in-memory graph
            state.graph_write().await.remove_node(&p.id);

            Ok(serde_json::json!({ "ok": true }))
        }

        // -- Relation operations --
        Method::CreateRelation => {
            let req: CreateRelationRequest = parse_params(params)?;
            let valid = ValidCreateRelationRequest::try_from(req)?;
            let relation_id = state
                .store
                .with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::create_relation(conn, &valid).await })
                })
                .await?;

            // Update in-memory graph: fetch the relation we just created
            let relations =
                store::list_relations(state.store.pool(), valid.source_id.as_str()).await?;
            if let Some(rel) = relations
                .iter()
                .find(|r| r.id.as_str() == relation_id.as_str())
            {
                let _ = state.graph_write().await.add_edge_from_relation(rel);
            }

            Ok(serde_json::json!({ "id": relation_id }))
        }

        Method::ListRelations => {
            let p: EntityIdParam = parse_params(params)?;
            let relations = store::list_relations(state.store.pool(), &p.entity_id).await?;
            Ok(serde_json::to_value(&relations).expect("infallible"))
        }

        Method::DeleteRelation => {
            let p: DeleteRelationParam = parse_params(params)?;

            // Validate relation_type BEFORE the DB operation
            let rt: filament_core::models::RelationType =
                serde_json::from_value(serde_json::Value::String(p.relation_type.clone()))
                    .map_err(|_| {
                        FilamentError::Validation(format!(
                            "invalid relation type: '{}'",
                            p.relation_type
                        ))
                    })?;

            let source_id = p.source_id.clone();
            let target_id = p.target_id.clone();
            let relation_type = p.relation_type.clone();
            state
                .store
                .with_transaction(|conn| {
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
                .await?;

            // Update in-memory graph
            state
                .graph_write()
                .await
                .remove_edge(&p.source_id, &p.target_id, &rt);

            Ok(serde_json::json!({ "ok": true }))
        }

        // -- Message operations --
        Method::SendMessage => {
            let req: SendMessageRequest = parse_params(params)?;
            let valid = ValidSendMessageRequest::try_from(req)?;
            let msg_id = state
                .store
                .with_transaction(|conn| {
                    let valid = valid.clone();
                    Box::pin(async move { store::send_message(conn, &valid).await })
                })
                .await?;
            Ok(serde_json::json!({ "id": msg_id }))
        }

        Method::GetInbox => {
            let p: AgentParam = parse_params(params)?;
            let messages = store::get_inbox(state.store.pool(), &p.agent).await?;
            Ok(serde_json::to_value(&messages).expect("infallible"))
        }

        Method::MarkMessageRead => {
            let p: IdParam = parse_params(params)?;
            let id = p.id.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::mark_message_read(conn, &id).await })
                })
                .await?;
            Ok(serde_json::json!({ "ok": true }))
        }

        // -- Reservation operations --
        Method::AcquireReservation => {
            let p: AcquireReservationParam = parse_params(params)?;
            let agent = p.agent_name.clone();
            let glob = p.file_glob.clone();
            let exclusive = p.exclusive.unwrap_or(false);
            let ttl = TtlSeconds::new(p.ttl_secs.unwrap_or(300))?;
            let res_id = state
                .store
                .with_transaction(|conn| {
                    let agent = agent.clone();
                    let glob = glob.clone();
                    Box::pin(async move {
                        store::acquire_reservation(conn, &agent, &glob, exclusive, ttl).await
                    })
                })
                .await?;
            Ok(serde_json::json!({ "id": res_id }))
        }

        Method::FindReservation => {
            let p: FindReservationParam = parse_params(params)?;
            let reservation =
                store::find_reservation(state.store.pool(), &p.file_glob, &p.agent_name).await?;
            Ok(serde_json::json!({ "reservation": reservation }))
        }

        Method::ListReservations => {
            let p: ListReservationsParam = parse_params(params)?;
            let reservations =
                store::list_reservations(state.store.pool(), p.agent.as_deref()).await?;
            Ok(serde_json::to_value(&reservations).expect("infallible"))
        }

        Method::ReleaseReservation => {
            let p: IdParam = parse_params(params)?;
            let id = p.id.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    Box::pin(async move { store::release_reservation(conn, &id).await })
                })
                .await?;
            Ok(serde_json::json!({ "ok": true }))
        }

        Method::ExpireStaleReservations => {
            let count = state.expire_stale_reservations().await?;
            Ok(serde_json::json!({ "expired": count }))
        }

        // -- Agent run operations --
        Method::CreateAgentRun => {
            let p: CreateAgentRunParam = parse_params(params)?;
            let task_id = p.task_id.clone();
            let agent_role = p.agent_role.clone();
            let pid = p.pid;
            let run_id = state
                .store
                .with_transaction(|conn| {
                    let task_id = task_id.clone();
                    let agent_role = agent_role.clone();
                    Box::pin(async move {
                        store::create_agent_run(conn, &task_id, &agent_role, pid).await
                    })
                })
                .await?;
            Ok(serde_json::json!({ "id": run_id }))
        }

        Method::FinishAgentRun => {
            let p: FinishAgentRunParam = parse_params(params)?;
            let id = p.id.clone();
            let status: AgentStatus =
                serde_json::from_value(serde_json::Value::String(p.status.clone())).map_err(
                    |_| FilamentError::Validation(format!("invalid agent status: '{}'", p.status)),
                )?;
            let result_json = p.result_json.clone();
            state
                .store
                .with_transaction(|conn| {
                    let id = id.clone();
                    let status = status.clone();
                    let result_json = result_json.clone();
                    Box::pin(async move {
                        store::finish_agent_run(conn, &id, status, result_json.as_deref()).await
                    })
                })
                .await?;
            Ok(serde_json::json!({ "ok": true }))
        }

        Method::ListRunningAgents => {
            let agents = store::list_running_agents(state.store.pool()).await?;
            Ok(serde_json::to_value(&agents).expect("infallible"))
        }

        // -- Graph operations (read-only, use graph read lock) --
        Method::ReadyTasks => {
            let mut conn = state
                .store
                .pool()
                .acquire()
                .await
                .map_err(FilamentError::Database)?;
            let tasks = store::ready_tasks(&mut conn).await?;
            Ok(serde_json::to_value(&tasks).expect("infallible"))
        }

        Method::CriticalPath => {
            let p: EntityIdParam = parse_params(params)?;
            let path = state.graph_read().await.critical_path(&p.entity_id);
            Ok(serde_json::to_value(&path).expect("infallible"))
        }

        Method::ImpactScore => {
            let p: EntityIdParam = parse_params(params)?;
            let score = state.graph_read().await.impact_score(&p.entity_id);
            Ok(serde_json::json!({ "score": score }))
        }

        Method::ContextQuery => {
            let p: ContextQueryParam = parse_params(params)?;
            let summaries = state
                .graph_read()
                .await
                .context_summaries(&p.entity_id, p.depth.unwrap_or(2));
            Ok(serde_json::to_value(&summaries).expect("infallible"))
        }

        Method::CheckCycle => {
            let has_cycle = state.graph_read().await.has_cycle();
            Ok(serde_json::json!({ "has_cycle": has_cycle }))
        }

        // -- Event operations --
        Method::GetEntityEvents => {
            let p: EntityIdParam = parse_params(params)?;
            let events = store::get_entity_events(state.store.pool(), &p.entity_id).await?;
            Ok(serde_json::to_value(&events).expect("infallible"))
        }
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| FilamentError::Protocol(e.to_string()))
}

// ---------------------------------------------------------------------------
// Param structs (private to this module)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct EntityIdParam {
    entity_id: String,
}

#[derive(Deserialize)]
struct ListEntitiesParam {
    entity_type: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct UpdateStatusParam {
    id: String,
    status: String,
}

#[derive(Deserialize)]
struct DeleteRelationParam {
    source_id: String,
    target_id: String,
    relation_type: String,
}

#[derive(Deserialize)]
struct AgentParam {
    agent: String,
}

#[derive(Deserialize)]
struct AcquireReservationParam {
    agent_name: String,
    file_glob: String,
    exclusive: Option<bool>,
    ttl_secs: Option<u32>,
}

#[derive(Deserialize)]
struct CreateAgentRunParam {
    task_id: String,
    agent_role: String,
    pid: Option<i32>,
}

#[derive(Deserialize)]
struct FinishAgentRunParam {
    id: String,
    status: String,
    result_json: Option<String>,
}

#[derive(Deserialize)]
struct NameParam {
    name: String,
}

#[derive(Deserialize)]
struct UpdateSummaryParam {
    id: String,
    summary: String,
}

#[derive(Deserialize)]
struct FindReservationParam {
    file_glob: String,
    agent_name: String,
}

#[derive(Deserialize)]
struct ListReservationsParam {
    agent: Option<String>,
}

#[derive(Deserialize)]
struct ContextQueryParam {
    entity_id: String,
    depth: Option<usize>,
}
