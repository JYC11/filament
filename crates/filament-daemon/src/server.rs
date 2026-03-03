use std::sync::Arc;

use filament_core::error::FilamentError;
use filament_core::protocol::Request;
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
