use clap::{Args, Subcommand};
use filament_core::error::Result;

use super::helpers::{connect, output_json, resolve_task};
use crate::Cli;

#[derive(Args, Debug)]
pub struct AgentCommand {
    #[command(subcommand)]
    command: AgentSubcommand,
}

impl AgentCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.command {
            AgentSubcommand::Dispatch(args) => dispatch(cli, args).await,
            AgentSubcommand::DispatchAll(args) => dispatch_all(cli, args).await,
            AgentSubcommand::Status(args) => status(cli, args).await,
            AgentSubcommand::List => list(cli).await,
            AgentSubcommand::History(args) => history(cli, args).await,
        }
    }
}

#[derive(Subcommand, Debug)]
enum AgentSubcommand {
    /// Dispatch an agent to work on a task.
    Dispatch(DispatchArgs),
    /// Dispatch agents to all ready tasks.
    DispatchAll(DispatchAllArgs),
    /// Show agent run status.
    Status(StatusArgs),
    /// List running agents.
    List,
    /// Show run history for a task.
    History(HistoryArgs),
}

#[derive(Args, Debug)]
struct DispatchArgs {
    /// Task slug or ID to dispatch to.
    task: String,
    /// Agent role (coder, reviewer, planner, dockeeper).
    #[arg(long, default_value = "coder")]
    role: String,
}

#[derive(Args, Debug)]
struct DispatchAllArgs {
    /// Maximum number of agents to run in parallel.
    #[arg(long, default_value = "3")]
    max_parallel: usize,
    /// Agent role (coder, reviewer, planner, dockeeper).
    #[arg(long, default_value = "coder")]
    role: String,
}

#[derive(Args, Debug)]
struct StatusArgs {
    /// Agent run ID.
    run_id: String,
}

#[derive(Args, Debug)]
struct HistoryArgs {
    /// Task slug or ID.
    task: String,
}

async fn dispatch(cli: &Cli, args: &DispatchArgs) -> Result<()> {
    let mut conn = connect().await?;
    let run_id = conn.dispatch_agent(&args.task, &args.role).await?;

    if cli.json {
        output_json(&serde_json::json!({ "run_id": run_id.as_str() }));
    } else {
        println!("Dispatched {} agent: {}", args.role, run_id);
        println!("Monitor with: filament agent status {run_id}");
    }
    Ok(())
}

async fn dispatch_all(cli: &Cli, args: &DispatchAllArgs) -> Result<()> {
    let mut conn = connect().await?;
    let result = conn
        .dispatch_batch(&args.role, Some(args.max_parallel))
        .await?;

    if cli.json {
        output_json(&result);
    } else {
        let dispatched = result["dispatched"].as_array().map_or(0, Vec::len);
        let errors = result["errors"].as_array().map_or(0, Vec::len);
        println!("Dispatched {dispatched} agents ({errors} errors)");
        if let Some(runs) = result["dispatched"].as_array() {
            for run in runs {
                println!(
                    "  {} -> {}",
                    run["task_slug"].as_str().unwrap_or("?"),
                    run["run_id"].as_str().unwrap_or("?")
                );
            }
        }
        if let Some(errs) = result["errors"].as_array() {
            for err in errs {
                eprintln!(
                    "  {} error: {}",
                    err["task_slug"].as_str().unwrap_or("?"),
                    err["error"].as_str().unwrap_or("?")
                );
            }
        }
    }
    Ok(())
}

async fn status(cli: &Cli, args: &StatusArgs) -> Result<()> {
    let mut conn = connect().await?;
    let run = conn.get_agent_run(&args.run_id).await?;

    if cli.json {
        output_json(&run);
    } else {
        println!("Run:      {}", run.id);
        println!("Task:     {}", run.task_id);
        println!("Role:     {}", run.agent_role);
        println!("Status:   {}", run.status);
        if let Some(pid) = run.pid {
            println!("PID:      {pid}");
        }
        println!("Started:  {}", run.started_at);
        if let Some(finished) = run.finished_at {
            println!("Finished: {finished}");
        }
        if let Some(ref result_json) = run.result_json {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(result_json) {
                if let Some(summary) = parsed.get("summary").and_then(|s| s.as_str()) {
                    println!("Summary:  {summary}");
                }
            }
        }
    }
    Ok(())
}

async fn list(cli: &Cli) -> Result<()> {
    let mut conn = connect().await?;
    let agents = conn.list_running_agents().await?;

    if cli.json {
        output_json(&agents);
    } else if agents.is_empty() {
        println!("No running agents.");
    } else {
        for run in &agents {
            println!(
                "[{}] {} on task {} (pid: {})",
                run.status,
                run.agent_role,
                run.task_id,
                run.pid.map_or_else(|| "-".to_string(), |p| p.to_string())
            );
        }
    }
    Ok(())
}

async fn history(cli: &Cli, args: &HistoryArgs) -> Result<()> {
    let mut conn = connect().await?;
    let task = resolve_task(&mut conn, &args.task).await?;
    let runs = conn.list_agent_runs_by_task(task.id.as_str()).await?;

    if cli.json {
        output_json(&runs);
    } else if runs.is_empty() {
        println!("No agent runs for task: {} ({})", task.name, task.slug);
    } else {
        println!("Agent runs for: {} ({})", task.name, task.slug);
        for run in &runs {
            let duration = run.finished_at.map_or_else(
                || "running".to_string(),
                |f| {
                    let dur = f - run.started_at;
                    format!("{}s", dur.num_seconds())
                },
            );
            println!(
                "  {} [{}] {} ({}) {}",
                run.started_at.format("%Y-%m-%d %H:%M"),
                run.status,
                run.agent_role,
                run.id,
                duration
            );
        }
    }
    Ok(())
}
