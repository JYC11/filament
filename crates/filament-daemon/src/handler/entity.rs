use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::models::{CreateEntityRequest, EntityStatus, ValidCreateEntityRequest};
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, IdParam};
use crate::server::SharedState;

pub async fn create(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
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

pub async fn get(params: serde_json::Value, state: &Arc<SharedState>) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    let entity = store::get_entity(state.store.pool(), &p.id).await?;
    Ok(serde_json::to_value(&entity).expect("infallible"))
}

pub async fn get_by_name(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: NameParam = parse_params(params)?;
    let entity = store::get_entity_by_name(state.store.pool(), &p.name).await?;
    Ok(serde_json::to_value(&entity).expect("infallible"))
}

pub async fn list(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ListEntitiesParam = parse_params(params)?;
    let entities = store::list_entities(
        state.store.pool(),
        p.entity_type.as_deref(),
        p.status.as_deref(),
    )
    .await?;
    Ok(serde_json::to_value(&entities).expect("infallible"))
}

pub async fn update_summary(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: UpdateSummaryParam = parse_params(params)?;
    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            let summary = p.summary.clone();
            Box::pin(async move { store::update_entity_summary(conn, &id, &summary).await })
        })
        .await?;

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn update_status(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: UpdateStatusParam = parse_params(params)?;
    let status: EntityStatus = serde_json::from_value(serde_json::Value::String(p.status.clone()))
        .map_err(|_| FilamentError::Validation(format!("invalid status: '{}'", p.status)))?;

    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            let status = status.clone();
            Box::pin(async move { store::update_entity_status(conn, &id, status).await })
        })
        .await?;

    // Update in-memory graph
    let entity = store::get_entity(state.store.pool(), &p.id).await?;
    state.graph_write().await.add_node_from_entity(&entity);

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn delete(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            Box::pin(async move { store::delete_entity(conn, &id).await })
        })
        .await?;

    // Update in-memory graph
    state.graph_write().await.remove_node(&p.id);

    Ok(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NameParam {
    name: String,
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
struct UpdateSummaryParam {
    id: String,
    summary: String,
}
