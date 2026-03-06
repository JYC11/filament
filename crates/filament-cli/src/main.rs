mod commands;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use commands::Commands;

/// Filament — local multi-agent orchestration, knowledge graph, and task management.
#[derive(Parser, Debug)]
#[command(name = "fl", version, about)]
pub struct Cli {
    /// Output JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase verbosity (-v for debug, -vv for trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress non-error output.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Apply config file defaults that can be overridden by CLI flags.
    fn apply_config_defaults(&mut self) {
        // Only apply config for commands that operate in a project context.
        // `init`, `completions`, etc. work outside a project.
        if let Ok(root) = commands::helpers::find_project_root() {
            let cfg = filament_core::config::FilamentConfig::load(&root);
            if !self.json && cfg.json_output() {
                self.json = true;
            }
        }
    }
}

fn init_tracing(verbose: u8, quiet: bool, stderr_only: bool) {
    let filter = if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info,filament=debug",
            _ => "trace",
        }
    };

    if stderr_only {
        // MCP mode: stdout is the JSON-RPC transport — all logs must go to stderr.
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .with_target(false)
            .without_time()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new(filter))
            .with_target(false)
            .without_time()
            .init();
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let mut cli = Cli::parse();
    cli.apply_config_defaults();
    let tui_mode = matches!(cli.command, Commands::Tui);
    let stderr_only = matches!(cli.command, Commands::Mcp) || tui_mode;
    if !tui_mode {
        init_tracing(cli.verbose, cli.quiet, stderr_only);
    }

    match cli.command.run(&cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if cli.json {
                let structured = filament_core::error::StructuredError::from(&e);
                let json = serde_json::to_string_pretty(&structured).expect("JSON serialization");
                eprintln!("{json}");
            } else {
                eprintln!("error: {e}");
                if let Some(hint) = e.hint() {
                    eprintln!("hint: {hint}");
                }
            }
            let code = u8::try_from(e.exit_code()).unwrap_or(1);
            ExitCode::from(code)
        }
    }
}
