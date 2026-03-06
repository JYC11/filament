use clap::{Args, Subcommand};
use filament_core::config::FilamentConfig;
use filament_core::error::Result;

use super::helpers;
use crate::Cli;

#[derive(Args, Debug)]
pub struct ConfigCommand {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show resolved configuration (config file + env var overrides).
    Show,
    /// Print a template config.toml to stdout.
    Init,
    /// Show the config file path.
    Path,
}

impl ConfigCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.action {
            ConfigAction::Show => show(cli).await,
            ConfigAction::Init => {
                init_config();
                Ok(())
            }
            ConfigAction::Path => path(),
        }
    }
}

#[allow(clippy::unused_async)]
async fn show(cli: &Cli) -> Result<()> {
    let root = helpers::find_project_root()?;
    let cfg = FilamentConfig::load(&root);

    if cli.json {
        let resolved = serde_json::json!({
            "default_priority": cfg.resolve_default_priority(),
            "output_format": if cfg.json_output() { "json" } else { "text" },
            "agent_command": cfg.resolve_agent_command(),
            "auto_dispatch": cfg.resolve_auto_dispatch(),
            "context_depth": cfg.resolve_context_depth(),
            "max_auto_dispatch": cfg.resolve_max_auto_dispatch(),
            "cleanup_interval_secs": cfg.resolve_cleanup_interval_secs(),
        });
        helpers::output_json(&resolved);
    } else {
        println!("default_priority     = {}", cfg.resolve_default_priority());
        println!(
            "output_format        = {}",
            if cfg.json_output() { "json" } else { "text" }
        );
        println!("agent_command        = {}", cfg.resolve_agent_command());
        println!("auto_dispatch        = {}", cfg.resolve_auto_dispatch());
        println!("context_depth        = {}", cfg.resolve_context_depth());
        println!("max_auto_dispatch    = {}", cfg.resolve_max_auto_dispatch());
        println!(
            "cleanup_interval_secs = {}",
            cfg.resolve_cleanup_interval_secs()
        );
    }
    Ok(())
}

fn init_config() {
    print!(
        "\
# Filament project configuration
# Place this file at .fl/config.toml
# All fields are optional — missing values use defaults.
# Environment variables (FILAMENT_*) override these values.

# Default priority for new entities (1-5, default 2)
# default_priority = 2

# Default output format: \"text\" or \"json\" (default \"text\")
# output_format = \"text\"

# Command to run agents (default \"claude\")
# Overridden by FILAMENT_AGENT_COMMAND env var
# agent_command = \"claude\"

# Auto-dispatch unblocked tasks on agent completion (default false)
# Overridden by FILAMENT_AUTO_DISPATCH env var
# auto_dispatch = false

# Graph context depth for agent prompts (default 2)
# Overridden by FILAMENT_CONTEXT_DEPTH env var
# context_depth = 2

# Max tasks to auto-dispatch per completion event (default 3)
# Overridden by FILAMENT_MAX_AUTO_DISPATCH env var
# max_auto_dispatch = 3

# Seconds between stale reservation cleanup sweeps (default 60)
# cleanup_interval_secs = 60
"
    );
}

fn path() -> Result<()> {
    let root = helpers::find_project_root()?;
    println!("{}", root.join(".fl").join("config.toml").display());
    Ok(())
}
