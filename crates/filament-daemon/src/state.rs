use std::path::{Path, PathBuf};

use filament_core::error::Result;
use filament_core::graph::KnowledgeGraph;
use filament_core::store::{self, FilamentStore};
use tokio::sync::RwLock;

/// Configuration for agent dispatch.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    /// Command to run (default: "claude"). Overridden by `FILAMENT_AGENT_COMMAND` env var.
    pub agent_command: String,
    /// Project root directory (for MCP config and working directory).
    pub project_root: PathBuf,
}

impl DispatchConfig {
    /// Create from project root, reading `FILAMENT_AGENT_COMMAND` env var.
    #[must_use]
    pub fn from_project_root(root: &Path) -> Self {
        Self {
            agent_command: std::env::var("FILAMENT_AGENT_COMMAND")
                .unwrap_or_else(|_| "claude".to_string()),
            project_root: root.to_path_buf(),
        }
    }
}

/// Shared state accessible by all connection handlers.
pub struct SharedState {
    pub store: FilamentStore,
    graph: RwLock<KnowledgeGraph>,
    dispatch_config: Option<DispatchConfig>,
}

impl SharedState {
    pub fn new(store: FilamentStore, graph: KnowledgeGraph) -> Self {
        Self {
            store,
            graph: RwLock::new(graph),
            dispatch_config: None,
        }
    }

    /// Create with dispatch configuration enabled.
    pub fn with_dispatch(
        store: FilamentStore,
        graph: KnowledgeGraph,
        config: DispatchConfig,
    ) -> Self {
        Self {
            store,
            graph: RwLock::new(graph),
            dispatch_config: Some(config),
        }
    }

    /// Get the dispatch config, if configured.
    #[must_use]
    pub fn dispatch_config(&self) -> Option<DispatchConfig> {
        self.dispatch_config.clone()
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
