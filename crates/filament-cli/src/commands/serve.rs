use std::path::PathBuf;

use clap::Args;
use filament_core::error::{FilamentError, Result};
use tokio_util::sync::CancellationToken;

use super::helpers;
use crate::Cli;

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Run in the foreground (don't daemonize).
    #[arg(long)]
    pub foreground: bool,

    /// Override socket path.
    #[arg(long)]
    pub socket_path: Option<PathBuf>,
}

/// Start the daemon.
pub async fn serve(cli: &Cli, args: &ServeArgs) -> Result<()> {
    let root = helpers::find_project_root()?;
    let mut config = filament_daemon::config::ServeConfig::from_project_root(&root);

    if let Some(ref path) = args.socket_path {
        config.socket_path = path.clone();
    }

    // Check for already-running daemon via PID file
    if config.pid_path.exists() {
        let pid_str = std::fs::read_to_string(&config.pid_path).unwrap_or_default();
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            // Probe if the process is still alive
            if is_process_alive(pid) {
                return Err(FilamentError::Validation(format!(
                    "daemon already running (PID {pid}). Stop it with `filament stop`."
                )));
            }
            // Stale PID file — clean up
            let _ = std::fs::remove_file(&config.pid_path);
            let _ = std::fs::remove_file(&config.socket_path);
        }
    }

    if args.foreground {
        // Run directly in this process
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        // Handle Ctrl+C and SIGTERM
        tokio::spawn(async move {
            let ctrl_c = tokio::signal::ctrl_c();
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");

            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }
            cancel_clone.cancel();
        });

        filament_daemon::serve(config, cancel).await
    } else {
        // Re-exec as a background process with --foreground
        let exe = std::env::current_exe().map_err(FilamentError::Io)?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("serve").arg("--foreground");
        if let Some(ref path) = args.socket_path {
            cmd.arg("--socket-path").arg(path);
        }
        // Detach: redirect stdout/stderr to /dev/null, don't inherit stdin
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let child = cmd.spawn().map_err(FilamentError::Io)?;
        let pid = child.id();
        if cli.json {
            helpers::output_json(&serde_json::json!({ "status": "started", "pid": pid }));
        } else {
            println!("daemon started (PID {pid})");
        }
        Ok(())
    }
}

/// Stop the daemon by reading the PID file and sending SIGTERM.
#[allow(clippy::unused_async)]
pub async fn stop(cli: &Cli) -> Result<()> {
    let root = helpers::find_project_root()?;
    let config = filament_daemon::config::ServeConfig::from_project_root(&root);

    if !config.pid_path.exists() {
        return Err(FilamentError::Validation(
            "no daemon running (PID file not found)".to_string(),
        ));
    }

    let pid_str = std::fs::read_to_string(&config.pid_path).map_err(FilamentError::Io)?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .map_err(|_| FilamentError::Validation(format!("invalid PID in file: '{pid_str}'")))?;

    // Send SIGTERM via kill command (safe Rust, no unsafe needed)
    let status = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(FilamentError::Io)?;

    if status.success() {
        if cli.json {
            helpers::output_json(&serde_json::json!({ "status": "stopped", "pid": pid }));
        } else {
            println!("sent SIGTERM to daemon (PID {pid})");
        }
        // Clean up PID file (daemon should do this, but be safe)
        let _ = std::fs::remove_file(&config.pid_path);
    } else {
        return Err(FilamentError::Validation(format!(
            "failed to kill daemon (PID {pid}) — process may have already exited"
        )));
    }

    Ok(())
}

/// Check if a process is alive by sending signal 0.
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
