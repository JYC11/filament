use std::sync::Arc;

use filament_core::error::FilamentError;
use filament_core::protocol::{Method, Notification, Request, SubscribeParams};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{debug, error};

pub use crate::state::SharedState;

use crate::handler;

/// Handle a single client connection: read NDJSON lines, dispatch, write responses.
///
/// # Panics
///
/// Panics if response serialization fails (infallible for well-formed `Response`).
pub async fn handle_connection(stream: UnixStream, state: Arc<SharedState>) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break, // client disconnected cleanly
            Err(e) => {
                debug!("connection read error: {e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let err_response = filament_core::protocol::Response::error(
                    String::new(),
                    filament_core::error::StructuredError::from(&FilamentError::Protocol(
                        e.to_string(),
                    )),
                );
                let json = serde_json::to_string(&err_response).expect("infallible");
                if writer.write_all(json.as_bytes()).await.is_err()
                    || writer.write_all(b"\n").await.is_err()
                    || writer.flush().await.is_err()
                {
                    error!("failed to write error response");
                    return;
                }
                continue;
            }
        };

        // Check if this is a subscribe request — switch to push mode
        if request.method == Method::Subscribe {
            let filter: SubscribeParams =
                serde_json::from_value(request.params.clone()).unwrap_or_default();

            // Send initial success response
            let response = filament_core::protocol::Response::success(
                request.id,
                serde_json::json!({ "subscribed": true }),
            );
            let json = serde_json::to_string(&response).expect("infallible");
            if writer.write_all(json.as_bytes()).await.is_err()
                || writer.write_all(b"\n").await.is_err()
                || writer.flush().await.is_err()
            {
                return;
            }

            // Enter push mode
            handle_subscription(state, &filter, &mut writer).await;
            return;
        }

        debug!(method = ?request.method, id = %request.id, "handling request");
        let response = handler::dispatch(request, &state).await;
        let json = serde_json::to_string(&response).expect("infallible");

        if writer.write_all(json.as_bytes()).await.is_err()
            || writer.write_all(b"\n").await.is_err()
            || writer.flush().await.is_err()
        {
            error!("failed to write response");
            return;
        }
    }
}

/// Push notifications to a subscribed client until disconnection.
async fn handle_subscription(
    state: Arc<SharedState>,
    filter: &SubscribeParams,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
) {
    let mut rx = state.subscribe();
    let event_filter: Vec<String> = filter.event_types.clone();

    loop {
        match rx.recv().await {
            Ok(notification) => {
                if !event_filter.is_empty() && !event_filter.contains(&notification.event_type) {
                    continue;
                }
                if let Err(e) = write_notification(writer, &notification).await {
                    debug!("subscriber disconnected: {e}");
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                debug!("subscriber lagged, skipped {n} notifications");
                // Send a lag warning as a notification
                let lag_notice = Notification {
                    event_type: "system_lag".to_string(),
                    entity_id: None,
                    detail: Some(serde_json::json!({ "skipped": n })),
                };
                if write_notification(writer, &lag_notice).await.is_err() {
                    return;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                debug!("notification channel closed");
                return;
            }
        }
    }
}

async fn write_notification(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    notification: &Notification,
) -> Result<(), std::io::Error> {
    let json = serde_json::to_string(notification).expect("infallible");
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
