use std::path::Path;

use sqlx::{Pool, Sqlite};
use tokio::net::UnixStream;

use crate::client::DaemonClient;
use crate::error::Result;
use crate::schema::init_pool;
use crate::store::FilamentStore;

/// Connection mode: direct `SQLite` or via daemon socket.
pub enum FilamentConnection {
    /// Direct `SQLite` access (single-user mode).
    Direct(FilamentStore),
    /// Connected to daemon via Unix socket (multi-agent mode).
    Socket(DaemonClient),
}

/// Runtime directory name created by `filament init`.
const RUNTIME_DIR: &str = ".filament";
const SOCKET_NAME: &str = "filament.sock";
const DB_NAME: &str = "filament.db";

impl FilamentConnection {
    /// Auto-detect connection mode.
    /// If `.filament/filament.sock` exists and is connectable, use Socket.
    /// Otherwise, open a Direct connection to `.filament/filament.db`.
    ///
    /// # Errors
    ///
    /// Returns an error if neither the socket nor database can be opened.
    pub async fn auto_detect(project_root: &Path) -> Result<Self> {
        let runtime_dir = project_root.join(RUNTIME_DIR);
        let sock_path = runtime_dir.join(SOCKET_NAME);

        // Try socket first (daemon mode)
        if sock_path.exists() {
            if let Ok(stream) = UnixStream::connect(&sock_path).await {
                return Ok(Self::Socket(DaemonClient::from_stream(stream)));
            }
        }

        // Fall back to direct mode
        let db_path = runtime_dir.join(DB_NAME);
        let db_str = db_path.to_str().ok_or_else(|| {
            crate::error::FilamentError::Validation(format!(
                "database path is not valid UTF-8: {}",
                db_path.display()
            ))
        })?;
        let pool = init_pool(db_str).await?;
        Ok(Self::Direct(FilamentStore::new(pool)))
    }

    /// Open a direct connection to a specific database path.
    ///
    /// # Errors
    ///
    /// Returns `FilamentError::Database` if the pool fails to connect.
    pub async fn direct(db_path: &str) -> Result<Self> {
        let pool = init_pool(db_path).await?;
        Ok(Self::Direct(FilamentStore::new(pool)))
    }

    /// Get the store (only available in Direct mode).
    #[must_use]
    pub const fn store(&self) -> Option<&FilamentStore> {
        match self {
            Self::Direct(store) => Some(store),
            Self::Socket(_) => None,
        }
    }

    /// Get the underlying pool (only available in Direct mode).
    #[must_use]
    pub const fn pool(&self) -> Option<&Pool<Sqlite>> {
        match self {
            Self::Direct(store) => Some(store.pool()),
            Self::Socket(_) => None,
        }
    }

    /// Get the daemon client (only available in Socket mode).
    #[must_use]
    pub const fn client(&mut self) -> Option<&mut DaemonClient> {
        match self {
            Self::Direct(_) => None,
            Self::Socket(client) => Some(client),
        }
    }
}
