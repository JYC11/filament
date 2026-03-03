use std::sync::Arc;

use filament_core::error::Result;
use filament_core::models::{ReservationMode, TtlSeconds};
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, IdParam};
use crate::state::SharedState;

pub async fn acquire(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: AcquireReservationParam = parse_params(params)?;
    let mode = ReservationMode::from(p.exclusive.unwrap_or(false));
    let ttl = TtlSeconds::new(p.ttl_secs.unwrap_or(300))?;
    let res_id = state
        .store
        .with_transaction(|conn| {
            let agent = p.agent_name.clone();
            let glob = p.file_glob.clone();
            Box::pin(
                async move { store::acquire_reservation(conn, &agent, &glob, mode, ttl).await },
            )
        })
        .await?;
    Ok(serde_json::json!({ "id": res_id }))
}

pub async fn find(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: FindReservationParam = parse_params(params)?;
    let reservation =
        store::find_reservation(state.store.pool(), &p.file_glob, &p.agent_name).await?;
    Ok(serde_json::json!({ "reservation": reservation }))
}

pub async fn list(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ListReservationsParam = parse_params(params)?;
    let reservations = store::list_reservations(state.store.pool(), p.agent.as_deref()).await?;
    Ok(serde_json::to_value(&reservations).expect("infallible"))
}

pub async fn release(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            Box::pin(async move { store::release_reservation(conn, &id).await })
        })
        .await?;
    Ok(serde_json::json!({ "ok": true }))
}

pub async fn expire_stale(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let count = state.expire_stale_reservations().await?;
    Ok(serde_json::json!({ "expired": count }))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AcquireReservationParam {
    agent_name: String,
    file_glob: String,
    exclusive: Option<bool>,
    ttl_secs: Option<u32>,
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
