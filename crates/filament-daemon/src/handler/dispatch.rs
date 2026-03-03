use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::store;
use serde::Deserialize;

use super::parse_params;
use crate::dispatch;
use crate::roles::AgentRole;
use crate::state::SharedState;

pub async fn dispatch_agent(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: DispatchAgentParam = parse_params(params)?;
    let role: AgentRole = p.role.parse()?;

    let config = state
        .dispatch_config()
        .ok_or_else(|| FilamentError::AgentDispatchFailed {
            reason: "dispatch not configured (daemon not started with dispatch support)"
                .to_string(),
        })?;

    let run_id = dispatch::dispatch_agent(state, &config, &p.task_slug, role).await?;
    Ok(serde_json::json!({ "run_id": run_id }))
}

pub async fn get_run(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: GetRunParam = parse_params(params)?;
    let run = store::get_agent_run(state.store.pool(), &p.run_id).await?;
    Ok(serde_json::to_value(&run).expect("infallible"))
}

pub async fn list_runs_by_task(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ListRunsByTaskParam = parse_params(params)?;
    let runs = store::list_agent_runs_by_task(state.store.pool(), &p.task_id).await?;
    Ok(serde_json::to_value(&runs).expect("infallible"))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DispatchAgentParam {
    task_slug: String,
    role: String,
}

#[derive(Deserialize)]
struct GetRunParam {
    run_id: String,
}

#[derive(Deserialize)]
struct ListRunsByTaskParam {
    task_id: String,
}
