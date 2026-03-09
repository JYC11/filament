use std::sync::Arc;

use filament_core::dto::{
    ListMessagesRequest, MessageParticipant, SendMessageRequest, ValidSendMessageRequest,
};
use filament_core::error::{FilamentError, Result};
use filament_core::protocol::Notification;
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
    validate_participant(state, &valid.from_agent, "from_agent").await?;
    validate_participant(state, &valid.to_agent, "to_agent").await?;
    let msg_id = state
        .store
        .with_transaction(|conn| {
            let valid = valid.clone();
            Box::pin(async move { store::send_message(conn, &valid).await })
        })
        .await?;
    state.notify(Notification {
        event_type: "message_sent".to_string(),
        entity_id: None,
        detail: Some(serde_json::json!({ "id": msg_id })),
    });

    Ok(serde_json::json!({ "id": msg_id }))
}

pub async fn get(params: serde_json::Value, state: &Arc<SharedState>) -> Result<serde_json::Value> {
    let p: IdParam = parse_params(params)?;
    let msg = store::get_message(state.store.pool(), &p.id).await?;
    Ok(serde_json::to_value(&msg).expect("infallible"))
}

pub async fn inbox(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let p: AgentParam = parse_params(params)?;
    let messages = store::get_inbox(state.store.pool(), &p.agent).await?;
    Ok(serde_json::to_value(&messages).expect("infallible"))
}

pub async fn list_paged(
    params: serde_json::Value,
    state: &Arc<SharedState>,
) -> Result<serde_json::Value> {
    let req: ListMessagesRequest = parse_params(params)?;
    let result = store::list_messages_paged(state.store.pool(), &req).await?;
    Ok(serde_json::to_value(&result).expect("infallible"))
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

async fn validate_participant(
    state: &SharedState,
    participant: &MessageParticipant,
    field_name: &str,
) -> Result<()> {
    if let MessageParticipant::Entity(ref slug_or_id) = participant {
        store::resolve_entity(state.store.pool(), slug_or_id.as_str())
            .await
            .map_err(|e| match e {
                FilamentError::EntityNotFound { .. } => FilamentError::Validation(format!(
                    "{field_name} entity not found: {slug_or_id}"
                )),
                other => other,
            })?;
    }
    Ok(())
}
