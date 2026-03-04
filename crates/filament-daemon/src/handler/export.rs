use std::sync::Arc;

use filament_core::error::Result;
use filament_core::store;
use serde::Deserialize;

use super::parse_params;
use crate::state::SharedState;

#[derive(Deserialize)]
struct ExportParams {
    #[serde(default = "default_true")]
    include_events: bool,
}

const fn default_true() -> bool {
    true
}

pub async fn export_all(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ExportParams = parse_params(params)?;
    let data = store::export_all(state.store.pool(), p.include_events).await?;
    Ok(serde_json::to_value(data).expect("infallible"))
}

#[derive(Deserialize)]
struct ImportParams {
    data: filament_core::dto::ExportData,
    #[serde(default = "default_true")]
    include_events: bool,
}

pub async fn import_data(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: ImportParams = parse_params(params)?;
    let result = state
        .store
        .with_transaction(|conn| {
            let data = p.data.clone();
            let include_events = p.include_events;
            Box::pin(async move { store::import_data(conn, &data, include_events).await })
        })
        .await?;

    // Refresh graph after import
    state
        .graph_write()
        .await
        .hydrate(state.store.pool())
        .await?;

    Ok(serde_json::to_value(result).expect("infallible"))
}

pub async fn list_pending_escalations(
    _params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let escalations = store::list_pending_escalations(state.store.pool()).await?;
    Ok(serde_json::to_value(escalations).expect("infallible"))
}
