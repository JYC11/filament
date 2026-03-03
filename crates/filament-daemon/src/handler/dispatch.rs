use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::store;
use serde::Deserialize;

use super::parse_params;
use crate::dispatch;
use crate::roles::AgentRole;
use crate::server::SharedState;

pub async fn dispatch_agent(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: DispatchAgentParam = parse_params(params)?;
    let role: AgentRole = p
        .role
        .parse()
        .map_err(|e: String| FilamentError::Validation(e))?;

    let config = state
        .dispatch_config()
        .ok_or_else(|| FilamentError::AgentDispatchFailed {
            reason: "dispatch not configured (daemon not started with dispatch support)"
                .to_string(),
        })?;

    let run_id = dispatch::dispatch_agent(state, &config, &p.task_slug, role).await?;
    Ok(serde_json::json!({ "run_id": run_id }))
}

pub async fn dispatch_batch(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: DispatchBatchParam = parse_params(params)?;
    let role: AgentRole = p
        .role
        .parse()
        .map_err(|e: String| FilamentError::Validation(e))?;
    let max_parallel = p.max_parallel.unwrap_or(3).min(10);

    let config = state
        .dispatch_config()
        .ok_or_else(|| FilamentError::AgentDispatchFailed {
            reason: "dispatch not configured".to_string(),
        })?;

    // Get ready tasks
    let mut conn = state
        .store
        .pool()
        .acquire()
        .await
        .map_err(FilamentError::Database)?;
    let ready = store::ready_tasks(&mut conn).await?;
    drop(conn);

    let to_dispatch: Vec<_> = ready.into_iter().take(max_parallel).collect();
    let mut run_ids = Vec::new();
    let mut errors = Vec::new();

    for task in &to_dispatch {
        match dispatch::dispatch_agent(state, &config, task.slug().as_str(), role).await {
            Ok(run_id) => run_ids.push(serde_json::json!({
                "task_slug": task.slug().as_str(),
                "run_id": run_id.as_str(),
            })),
            Err(e) => errors.push(serde_json::json!({
                "task_slug": task.slug().as_str(),
                "error": e.to_string(),
            })),
        }
    }

    Ok(serde_json::json!({
        "dispatched": run_ids,
        "errors": errors,
    }))
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
struct DispatchBatchParam {
    role: String,
    max_parallel: Option<usize>,
}

#[derive(Deserialize)]
struct GetRunParam {
    run_id: String,
}

#[derive(Deserialize)]
struct ListRunsByTaskParam {
    task_id: String,
}
