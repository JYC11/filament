use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::models::AgentStatus;
use filament_core::store;
use serde::Deserialize;

use super::parse_params;
use crate::server::SharedState;

pub async fn create(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: CreateAgentRunParam = parse_params(params)?;
    let pid = p.pid;
    let run_id = state
        .store
        .with_transaction(|conn| {
            let task_id = p.task_id.clone();
            let agent_role = p.agent_role.clone();
            Box::pin(async move { store::create_agent_run(conn, &task_id, &agent_role, pid).await })
        })
        .await?;
    Ok(serde_json::json!({ "id": run_id }))
}

pub async fn finish(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: FinishAgentRunParam = parse_params(params)?;
    let status: AgentStatus = serde_json::from_value(serde_json::Value::String(p.status.clone()))
        .map_err(|_| {
        FilamentError::Validation(format!("invalid agent status: '{}'", p.status))
    })?;
    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            let status = status.clone();
            let result_json = p.result_json.clone();
            Box::pin(async move {
                store::finish_agent_run(conn, &id, status, result_json.as_deref()).await
            })
        })
        .await?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn list_running(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let agents = store::list_running_agents(state.store.pool()).await?;
    Ok(serde_json::to_value(&agents).expect("infallible"))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

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
