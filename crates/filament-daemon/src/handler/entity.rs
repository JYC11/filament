use std::sync::Arc;

use filament_core::dto::{CreateEntityRequest, EntityChangeset, ValidCreateEntityRequest};
use filament_core::error::Result;
use filament_core::models::{EntityStatus, EntityType};
use filament_core::protocol::Notification;
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, IdParam};
use crate::state::SharedState;

pub async fn create(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let req: CreateEntityRequest = parse_params(params)?;
    let valid = ValidCreateEntityRequest::try_from(req)?;
    let (entity_id, slug) = state
        .store
        .with_transaction(|conn| {
            let valid = valid.clone();
            Box::pin(async move { store::create_entity(conn, &valid).await })
        })
        .await?;

    // Update in-memory graph
    let entity = store::get_entity(state.store.pool(), entity_id.as_str()).await?;
    state.graph_write().await.add_node_from_entity(&entity);

    state.notify(Notification {
        event_type: "entity_created".to_string(),
        entity_id: Some(entity_id.to_string()),
        detail: Some(serde_json::json!({ "slug": slug.as_str() })),
    });

    Ok(serde_json::json!({ "id": entity_id, "slug": slug.as_str() }))
}

pub async fn get(params: serde_json::Value, state: &Arc<SharedState>) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    let entity = store::get_entity(state.store.pool(), &p.id).await?;
    Ok(serde_json::to_value(&entity).expect("infallible"))
}

pub async fn get_by_slug(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: SlugParam = parse_params(params)?;
    let entity = store::get_entity_by_slug(state.store.pool(), &p.slug).await?;
    Ok(serde_json::to_value(&entity).expect("infallible"))
}

pub async fn list(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ListEntitiesParam = parse_params(params)?;
    let entities = store::list_entities(
        state.store.pool(),
        p.entity_type.as_ref().map(EntityType::as_str),
        p.status.as_ref().map(EntityStatus::as_str),
    )
    .await?;
    Ok(serde_json::to_value(&entities).expect("infallible"))
}

pub async fn update(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: UpdateEntityParam = parse_params(params)?;
    let entity = state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            let changeset = p.changeset.clone();
            Box::pin(async move { store::update_entity(conn, &id, &changeset).await })
        })
        .await?;

    // Update in-memory graph
    state.graph_write().await.add_node_from_entity(&entity);

    state.notify(Notification {
        event_type: "entity_updated".to_string(),
        entity_id: Some(p.id.clone()),
        detail: Some(serde_json::json!({ "version": entity.common().version })),
    });

    Ok(serde_json::to_value(&entity).expect("infallible"))
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

    // Update in-memory graph
    let entity = store::get_entity(state.store.pool(), &p.id).await?;
    state.graph_write().await.add_node_from_entity(&entity);

    state.notify(Notification {
        event_type: "entity_updated".to_string(),
        entity_id: Some(p.id.clone()),
        detail: None,
    });

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn update_status(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: UpdateStatusParam = parse_params(params)?;

    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            let status = p.status;
            Box::pin(async move { store::update_entity_status(conn, &id, status).await })
        })
        .await?;

    // Update in-memory graph
    let entity = store::get_entity(state.store.pool(), &p.id).await?;
    state.graph_write().await.add_node_from_entity(&entity);

    state.notify(Notification {
        event_type: "status_change".to_string(),
        entity_id: Some(p.id.clone()),
        detail: Some(serde_json::json!({ "status": p.status.as_str() })),
    });

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn batch_get(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: BatchGetParam = parse_params(params)?;
    let refs: Vec<&str> = p.ids.iter().map(String::as_str).collect();
    let entities = store::batch_get_entities(state.store.pool(), &refs).await?;
    Ok(serde_json::to_value(&entities).expect("infallible"))
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
            Box::pin(async move { store::delete_entity(conn, &id, None).await })
        })
        .await?;

    // Update in-memory graph
    state.graph_write().await.remove_node(&p.id);

    state.notify(Notification {
        event_type: "entity_deleted".to_string(),
        entity_id: Some(p.id.clone()),
        detail: None,
    });

    Ok(serde_json::json!({ "ok": true }))
}

pub async fn search(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: SearchParam = parse_params(params)?;
    let results = store::search_entities(
        state.store.pool(),
        &p.query,
        p.entity_type.as_ref().map(EntityType::as_str),
        p.limit.unwrap_or(20),
    )
    .await?;
    let items: Vec<filament_core::dto::SearchResult> = results
        .into_iter()
        .map(|(entity, rank)| filament_core::dto::SearchResult { entity, rank })
        .collect();
    Ok(serde_json::to_value(&items).expect("infallible"))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SlugParam {
    slug: String,
}

#[derive(Deserialize)]
struct ListEntitiesParam {
    entity_type: Option<EntityType>,
    status: Option<EntityStatus>,
}

#[derive(Deserialize)]
struct BatchGetParam {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateEntityParam {
    id: String,
    changeset: EntityChangeset,
}

#[derive(Deserialize)]
struct UpdateStatusParam {
    id: String,
    status: EntityStatus,
}

#[derive(Deserialize)]
struct UpdateSummaryParam {
    id: String,
    summary: String,
}

#[derive(Deserialize)]
struct SearchParam {
    query: String,
    entity_type: Option<EntityType>,
    limit: Option<u32>,
}
