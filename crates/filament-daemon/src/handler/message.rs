use std::sync::Arc;

use filament_core::error::Result;
use filament_core::models::{SendMessageRequest, ValidSendMessageRequest};
use filament_core::store;
use serde::Deserialize;

use super::{parse_params, IdParam};
use crate::state::SharedState;

pub async fn send(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let req: SendMessageRequest = parse_params(params)?;
    let valid = ValidSendMessageRequest::try_from(req)?;
    let msg_id = state
        .store
        .with_transaction(|conn| {
            let valid = valid.clone();
            Box::pin(async move { store::send_message(conn, &valid).await })
        })
        .await?;
    Ok(serde_json::json!({ "id": msg_id }))
}

pub async fn inbox(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: AgentParam = parse_params(params)?;
    let messages = store::get_inbox(state.store.pool(), &p.agent).await?;
    Ok(serde_json::to_value(&messages).expect("infallible"))
}

pub async fn mark_read(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    state
        .store
        .with_transaction(|conn| {
            let id = p.id.clone();
            Box::pin(async move { store::mark_message_read(conn, &id).await })
        })
        .await?;
    Ok(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Param structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AgentParam {
    agent: String,
}
