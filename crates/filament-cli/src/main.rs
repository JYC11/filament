mod commands;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use commands::Commands;

/// Filament — local multi-agent orchestration, knowledge graph, and task management.
#[derive(Parser, Debug)]
#[command(name = "filament", version, about)]
pub struct Cli {
    /// Output JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    /// Increase verbosity (-v for debug, -vv for trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress non-error output.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
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
    let cli = Cli::parse();
    let mcp_mode = matches!(cli.command, Commands::Mcp);
    init_tracing(cli.verbose, cli.quiet, mcp_mode);

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
