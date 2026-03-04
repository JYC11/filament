pub mod config;
pub mod dispatch;
pub(crate) mod dto;
pub mod handler;
pub mod mcp;
pub mod roles;
pub mod server;
pub mod state;

use std::sync::Arc;

use filament_core::error::{FilamentError, Result};
use filament_core::graph::KnowledgeGraph;
use filament_core::schema::init_pool;
use filament_core::store::FilamentStore;
use tokio::net::UnixListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use config::ServeConfig;
use state::{DispatchConfig, SharedState};

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

    // Reconcile stale agent runs left by a previous unclean shutdown
    let store_tmp = FilamentStore::new(pool.clone());
    let reconciled = store_tmp
        .with_transaction(|conn| {
            Box::pin(
                async move { filament_core::store::reconcile_stale_agent_runs(conn).await },
            )
        })
        .await?;
    if reconciled > 0 {
        info!(count = reconciled, "reconciled stale agent runs from previous session");
    }

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

    // Reap orphan agent processes and reconcile their DB state
    reap_orphan_agents(&state).await;

    // Cleanup
    let _ = std::fs::remove_file(&config.socket_path);
    let _ = std::fs::remove_file(&config.pid_path);
    info!("daemon stopped");

    Ok(())
}

/// Kill running agent processes and mark their DB records as failed.
///
/// Sends SIGTERM to each tracked PID, waits briefly, then SIGKILLs survivors.
/// Finally reconciles the DB state via [`filament_core::store::reconcile_stale_agent_runs`].
async fn reap_orphan_agents(state: &SharedState) {
    use filament_core::store;

    // 1. Collect PIDs of running agents
    let running = match store::list_running_agents(state.store.pool()).await {
        Ok(runs) => runs,
        Err(e) => {
            error!("failed to list running agents during shutdown: {e}");
            return;
        }
    };

    if running.is_empty() {
        return;
    }

    let pids: Vec<i32> = running.iter().filter_map(|r| r.pid).collect();
    info!(count = running.len(), "reaping orphan agent processes");

    // 2. Send SIGTERM to each PID
    for &pid in &pids {
        let _ = std::process::Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
    }

    // 3. Wait up to 3 seconds for graceful exit
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // 4. SIGKILL any survivors (kill -0 checks if still alive)
    for &pid in &pids {
        let alive = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .is_ok_and(|s| s.success());
        if alive {
            warn!(pid = pid, "agent process did not exit after SIGTERM, sending SIGKILL");
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status();
        }
    }

    // 5. Reconcile DB state (mark running → failed, revert tasks)
    if let Err(e) = state
        .store
        .with_transaction(|conn| {
            Box::pin(async move { store::reconcile_stale_agent_runs(conn).await })
        })
        .await
    {
        error!("failed to reconcile agent runs during shutdown: {e}");
    }
}
