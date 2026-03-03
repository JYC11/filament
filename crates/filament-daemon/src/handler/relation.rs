use std::sync::Arc;

use filament_core::error::Result;
use filament_core::models::{CreateRelationRequest, RelationType, ValidCreateRelationRequest};
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, EntityIdParam};
use crate::state::SharedState;

pub async fn create(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let req: CreateRelationRequest = parse_params(params)?;
    let valid = ValidCreateRelationRequest::try_from(req)?;
    let relation_id = state
        .store
        .with_transaction(|conn| {
            let valid = valid.clone();
            Box::pin(async move { store::create_relation(conn, &valid).await })
        })
        .await?;

    // Update in-memory graph: fetch the exact relation we just created
    let rel = store::get_relation(state.store.pool(), relation_id.as_str()).await?;
    let edge_result = state.graph_write().await.add_edge_from_relation(&rel);
    if let Err(e) = edge_result {
        tracing::warn!("graph edge add failed, re-hydrating: {e}");
        state
            .graph_write()
            .await
            .hydrate(state.store.pool())
            .await?;
    }

    Ok(serde_json::json!({ "id": relation_id }))
}

pub async fn list(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: EntityIdParam = parse_params(params)?;
    let relations = store::list_relations(state.store.pool(), &p.entity_id).await?;
    Ok(serde_json::to_value(&relations).expect("infallible"))
}

pub async fn delete(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: DeleteRelationParam = parse_params(params)?;

    state
        .store
        .with_transaction(|conn| {
            let source_id = p.source_id.clone();
            let target_id = p.target_id.clone();
            let relation_type = p.relation_type.as_str().to_string();
            Box::pin(async move {
                store::delete_relation_by_endpoints(conn, &source_id, &target_id, &relation_type)
                    .await
            })
        })
        .await?;

    // Update in-memory graph
    state
        .graph_write()
        .await
        .remove_edge(&p.source_id, &p.target_id, &p.relation_type);

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn blocked_by_counts(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let counts = store::blocked_by_counts(state.store.pool()).await?;
    Ok(serde_json::to_value(&counts).expect("infallible"))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DeleteRelationParam {
    source_id: String,
    target_id: String,
    relation_type: RelationType,
}
