use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use filament_core::config::FilamentConfig;
use filament_core::error::Result;
use filament_core::graph::KnowledgeGraph;
use filament_core::protocol::Notification;
use filament_core::store::{self, FilamentStore};
use tokio::sync::{broadcast, RwLock};

/// Configuration for agent dispatch.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    /// Command to run (default: "claude"). Config file, then `FILAMENT_AGENT_COMMAND` env var.
    pub agent_command: String,
    /// Project root directory (for MCP config and working directory).
    pub project_root: PathBuf,
    /// Graph context depth for agent prompts (default: 2). `FILAMENT_CONTEXT_DEPTH`.
    pub context_depth: usize,
    /// Auto-dispatch unblocked tasks on completion (default: false). `FILAMENT_AUTO_DISPATCH=1`.
    pub auto_dispatch: bool,
    /// Max auto-dispatched tasks per completion event (default: 3). `FILAMENT_MAX_AUTO_DISPATCH`.
    pub max_auto_dispatch: usize,
    /// Max seconds an agent subprocess may run (default: 3600). 0 = no limit.
    pub agent_timeout_secs: u64,
}

impl DispatchConfig {
    /// Create from project root, reading config file then env vars for overrides.
    #[must_use]
    pub fn from_project_root(root: &Path) -> Self {
        let cfg = FilamentConfig::load(root);
        Self {
            agent_command: cfg.resolve_agent_command(),
            project_root: root.to_path_buf(),
            context_depth: cfg.resolve_context_depth(),
            auto_dispatch: cfg.resolve_auto_dispatch(),
            max_auto_dispatch: cfg.resolve_max_auto_dispatch(),
            agent_timeout_secs: cfg.resolve_agent_timeout_secs(),
        }
    }
}

/// Channel capacity for change notifications — old events are dropped if subscribers lag.
const NOTIFY_CAPACITY: usize = 256;

/// Shared state accessible by all connection handlers.
pub struct SharedState {
    pub store: FilamentStore,
    graph: RwLock<KnowledgeGraph>,
    dispatch_config: Option<DispatchConfig>,
    notify_tx: broadcast::Sender<Notification>,
    last_activity: AtomicU64,
}

impl SharedState {
    pub fn new(store: FilamentStore, graph: KnowledgeGraph) -> Self {
        let (notify_tx, _) = broadcast::channel(NOTIFY_CAPACITY);
        Self {
            store,
            graph: RwLock::new(graph),
            dispatch_config: None,
            notify_tx,
            last_activity: AtomicU64::new(epoch_secs()),
        }
    }

    /// Create with dispatch configuration enabled.
    pub fn with_dispatch(
        store: FilamentStore,
        graph: KnowledgeGraph,
        config: DispatchConfig,
    ) -> Self {
        let (notify_tx, _) = broadcast::channel(NOTIFY_CAPACITY);
        Self {
            store,
            graph: RwLock::new(graph),
            dispatch_config: Some(config),
            notify_tx,
            last_activity: AtomicU64::new(epoch_secs()),
        }
    }

    /// Get the dispatch config, if configured.
    #[must_use]
    pub fn dispatch_config(&self) -> Option<DispatchConfig> {
        self.dispatch_config.clone()
    }

    /// Emit a change notification to all subscribers.
    pub fn notify(&self, notification: Notification) {
        // Ignore send errors — no subscribers is fine
        let _ = self.notify_tx.send(notification);
    }

    /// Subscribe to change notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notify_tx.subscribe()
    }

    /// Record activity — resets the idle timer.
    pub fn touch(&self) {
        self.last_activity.store(epoch_secs(), Ordering::Relaxed);
    }

    /// Seconds since last activity.
    pub fn idle_secs(&self) -> u64 {
        epoch_secs().saturating_sub(self.last_activity.load(Ordering::Relaxed))
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

    /// Reconcile dead agent processes. Checks each running agent's PID via `kill -0`;
    /// if the process is dead, marks the run as failed and reverts the task to open.
    ///
    /// Returns the number of reconciled runs.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Database` on SQL failure.
    pub async fn reconcile_dead_agents(&self) -> Result<u64> {
        let running = store::list_running_agents(self.store.pool()).await?;
        if running.is_empty() {
            return Ok(0);
        }

        let dead_run_ids: Vec<(String, String)> = running
            .iter()
            .filter(|run| {
                let Some(pid) = run.pid else {
                    // No PID recorded — treat as dead (can't check)
                    return true;
                };
                !is_pid_alive(pid)
            })
            .map(|run| (run.id.to_string(), run.task_id.to_string()))
            .collect();

        if dead_run_ids.is_empty() {
            return Ok(0);
        }

        let count = dead_run_ids.len() as u64;
        self.store
            .with_transaction(|conn| {
                Box::pin(async move {
                    for (run_id, task_id) in &dead_run_ids {
                        store::finish_agent_run(
                            conn,
                            run_id,
                            filament_core::models::AgentStatus::Failed,
                            Some("{\"error\":\"agent process died — reconciled by daemon\"}"),
                        )
                        .await?;
                        store::update_entity_status(
                            conn,
                            task_id,
                            filament_core::models::EntityStatus::Open,
                        )
                        .await?;
                    }
                    Ok(count)
                })
            })
            .await
    }
}

/// Check if a process is alive using `kill -0`.
fn is_pid_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
