mod agent_run;
mod dispatch;
mod entity;
mod event;
mod graph;
mod message;
mod relation;
mod reservation;

use std::sync::Arc;

use filament_core::error::{FilamentError, Result, StructuredError};
use filament_core::protocol::{Method, Request, Response};
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

async fn handle(request: Request, state: &Arc<SharedState>) -> Result<serde_json::Value> {
    let params = request.params;

    match request.method {
        // Entity
        Method::CreateEntity => entity::create(params, state).await,
        Method::GetEntity => entity::get(params, state).await,
        Method::GetEntityBySlug => entity::get_by_slug(params, state).await,
        Method::ListEntities => entity::list(params, state).await,
        Method::UpdateEntitySummary => entity::update_summary(params, state).await,
        Method::UpdateEntityStatus => entity::update_status(params, state).await,
        Method::DeleteEntity => entity::delete(params, state).await,
        Method::BatchGetEntities => entity::batch_get(params, state).await,
        // Relation
        Method::CreateRelation => relation::create(params, state).await,
        Method::ListRelations => relation::list(params, state).await,
        Method::DeleteRelation => relation::delete(params, state).await,
        // Message
        Method::SendMessage => message::send(params, state).await,
        Method::GetInbox => message::inbox(params, state).await,
        Method::MarkMessageRead => message::mark_read(params, state).await,
        // Reservation
        Method::AcquireReservation => reservation::acquire(params, state).await,
        Method::FindReservation => reservation::find(params, state).await,
        Method::ListReservations => reservation::list(params, state).await,
        Method::ReleaseReservation => reservation::release(params, state).await,
        Method::ExpireStaleReservations => reservation::expire_stale(params, state).await,
        // Agent run
        Method::CreateAgentRun => agent_run::create(params, state).await,
        Method::FinishAgentRun => agent_run::finish(params, state).await,
        Method::ListRunningAgents => agent_run::list_running(params, state).await,
        Method::GetAgentRun => dispatch::get_run(params, state).await,
        Method::ListAgentRunsByTask => dispatch::list_runs_by_task(params, state).await,
        // Dispatch
        Method::DispatchAgent => dispatch::dispatch_agent(params, state).await,
        // Graph
        Method::ReadyTasks => graph::ready_tasks(params, state).await,
        Method::CriticalPath => graph::critical_path(params, state).await,
        Method::ImpactScore => graph::impact_score(params, state).await,
        Method::BatchImpactScores => graph::batch_impact_scores(params, state).await,
        Method::ContextQuery => graph::context_query(params, state).await,
        Method::CheckCycle => graph::check_cycle(params, state).await,
        // Batch relation
        Method::BlockedByCounts => relation::blocked_by_counts(params, state).await,
        // Event
        Method::GetEntityEvents => event::get_events(params, state).await,
    }
}

// ---------------------------------------------------------------------------
// Shared param structs and utilities
// ---------------------------------------------------------------------------

pub(crate) fn parse_params<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| FilamentError::Protocol(e.to_string()))
}

#[derive(Deserialize)]
pub(crate) struct IdParam {
    pub id: String,
}

#[derive(Deserialize)]
pub(crate) struct EntityIdParam {
    pub entity_id: String,
}
