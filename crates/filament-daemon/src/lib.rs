pub mod config;
pub mod dispatch;
pub mod handler;
pub mod mcp;
pub mod roles;
pub mod server;

use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::graph::KnowledgeGraph;
use filament_core::schema::init_pool;
use filament_core::store::FilamentStore;
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use config::ServeConfig;
use dispatch::DispatchConfig;
use server::SharedState;

/// Start the daemon server.
///
/// Binds a Unix socket, accepts connections, and handles NDJSON requests.
/// The `cancel` token allows clean shutdown from tests or signal handlers.
///
/// # Errors
///
/// Returns an error if the socket cannot be bound or the database cannot be opened.
pub async fn serve(config: ServeConfig, cancel: CancellationToken) -> Result<()> {
    serve_with_dispatch(config, cancel, None).await
}

/// Start the daemon server with an optional explicit dispatch configuration.
///
/// When `dispatch_override` is `None`, dispatch config is derived from the project root
/// and the `FILAMENT_AGENT_COMMAND` environment variable.
/// When `Some`, the provided config is used directly (useful for testing).
///
/// # Errors
///
/// Returns an error if the socket cannot be bound or the database cannot be opened.
pub async fn serve_with_dispatch(
    config: ServeConfig,
    cancel: CancellationToken,
    dispatch_override: Option<DispatchConfig>,
) -> Result<()> {
    // Remove stale socket file
    if config.socket_path.exists() {
        std::fs::remove_file(&config.socket_path)?;
    }

    // Init DB pool and hydrate graph
    let db_str = config.db_path.to_str().ok_or_else(|| {
        FilamentError::Validation(format!(
            "database path is not valid UTF-8: {}",
            config.db_path.display()
        ))
    })?;
    let pool = init_pool(db_str).await?;
    let store = FilamentStore::new(pool.clone());

    let mut graph = KnowledgeGraph::new();
    graph.hydrate(&pool).await?;

    // Use provided dispatch config or derive from project root
    let dispatch_config = dispatch_override.unwrap_or_else(|| {
        let project_root = config
            .db_path
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        DispatchConfig::from_project_root(project_root)
    });
    let state = Arc::new(SharedState::with_dispatch(store, graph, dispatch_config));

    // Bind socket first — write PID file only after successful bind
    let listener = UnixListener::bind(&config.socket_path)?;
    let pid = std::process::id();
    std::fs::write(&config.pid_path, pid.to_string())?;
    info!(
        socket = %config.socket_path.display(),
        pid = pid,
        "daemon listening"
    );

    // Spawn periodic cleanup
    let cleanup_state = Arc::clone(&state);
    let cleanup_cancel = cancel.clone();
    let cleanup_interval = config.cleanup_interval_secs;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(cleanup_interval));
        loop {
            tokio::select! {
                () = cleanup_cancel.cancelled() => break,
                _ = interval.tick() => {
                    if let Err(e) = cleanup_state.expire_stale_reservations().await {
                        error!("cleanup error: {e}");
                    }
                }
            }
        }
    });

    // Accept loop
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("shutdown signal received");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let conn_state = Arc::clone(&state);
                        tokio::spawn(server::handle_connection(stream, conn_state));
                    }
                    Err(e) => {
                        error!("accept error: {e}");
                    }
                }
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&config.socket_path);
    let _ = std::fs::remove_file(&config.pid_path);
    info!("daemon stopped");

    Ok(())
}
