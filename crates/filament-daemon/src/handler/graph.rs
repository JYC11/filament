use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, EntityIdParam};
use crate::server::SharedState;

pub async fn ready_tasks(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let mut conn = state
        .store
        .pool()
        .acquire()
        .await
        .map_err(FilamentError::Database)?;
    let tasks = store::ready_tasks(&mut conn).await?;
    Ok(serde_json::to_value(&tasks).expect("infallible"))
}

pub async fn critical_path(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: EntityIdParam = parse_params(params)?;
    let path = state.graph_read().await.critical_path(&p.entity_id);
    Ok(serde_json::to_value(&path).expect("infallible"))
}

pub async fn impact_score(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: EntityIdParam = parse_params(params)?;
    let score = state.graph_read().await.impact_score(&p.entity_id);
    Ok(serde_json::json!({ "score": score }))
}

pub async fn batch_impact_scores(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: BatchImpactScoresParam = parse_params(params)?;
    let scores = state.graph_read().await.batch_impact_scores(&p.entity_ids);
    Ok(serde_json::to_value(&scores).expect("infallible"))
}

pub async fn context_query(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ContextQueryParam = parse_params(params)?;
    let summaries = state
        .graph_read()
        .await
        .context_summaries(&p.entity_id, p.depth.unwrap_or(2));
    Ok(serde_json::to_value(&summaries).expect("infallible"))
}

pub async fn check_cycle(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let has_cycle = state.graph_read().await.has_cycle();
    Ok(serde_json::json!({ "has_cycle": has_cycle }))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct BatchImpactScoresParam {
    entity_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ContextQueryParam {
    entity_id: String,
    depth: Option<usize>,
}
