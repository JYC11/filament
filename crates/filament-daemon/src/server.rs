use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::graph::KnowledgeGraph;
use filament_core::protocol::Request;
use filament_core::store::{self, FilamentStore};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;
use tracing::{debug, error};

use crate::handler;

/// Shared state accessible by all connection handlers.
pub struct SharedState {
    pub store: FilamentStore,
    graph: RwLock<KnowledgeGraph>,
}

impl SharedState {
    pub fn new(store: FilamentStore, graph: KnowledgeGraph) -> Self {
        Self {
            store,
            graph: RwLock::new(graph),
        }
    }

    pub async fn graph_read(&self) -> tokio::sync::RwLockReadGuard<'_, KnowledgeGraph> {
        self.graph.read().await
    }

    pub async fn graph_write(&self) -> tokio::sync::RwLockWriteGuard<'_, KnowledgeGraph> {
        self.graph.write().await
    }

    /// Run stale reservation cleanup. Called periodically by the daemon.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Database` on SQL failure.
    pub async fn expire_stale_reservations(&self) -> Result<u64> {
        self.store
            .with_transaction(|conn| {
                Box::pin(async move { store::expire_stale_reservations(conn).await })
            })
            .await
    }
}

/// Handle a single client connection: read NDJSON lines, dispatch, write responses.
#[allow(clippy::missing_panics_doc)]
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
