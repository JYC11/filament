use std::sync::Arc;

use filament_core::error::Result;
use filament_core::store;

use super::{parse_params, EntityIdParam};
use crate::state::SharedState;

pub async fn get_events(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: EntityIdParam = parse_params(params)?;
    let events = store::get_entity_events(state.store.pool(), &p.entity_id).await?;
    Ok(serde_json::to_value(&events).expect("infallible"))
}
