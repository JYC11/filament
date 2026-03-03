use std::path::PathBuf;

/// Configuration for the daemon server.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// Path to the Unix socket.
    pub socket_path: PathBuf,
    /// Path to the `SQLite` database.
    pub db_path: PathBuf,
    /// Path to store the PID file.
    pub pid_path: PathBuf,
    /// Interval in seconds between stale reservation cleanup sweeps.
    pub cleanup_interval_secs: u64,
}

impl ServeConfig {
    /// Create a config from a project root directory.
    #[must_use]
    pub fn from_project_root(root: &std::path::Path) -> Self {
        let runtime_dir = root.join(".filament");
        Self {
            socket_path: runtime_dir.join("filament.sock"),
            db_path: runtime_dir.join("filament.db"),
            pid_path: runtime_dir.join("filament.pid"),
            cleanup_interval_secs: 60,
        }
    }
}
