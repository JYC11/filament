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

    // dispatch-all requires daemon mode (dispatch_agent only works via socket)
    if !conn.is_daemon_mode() {
        return Err(filament_core::error::FilamentError::AgentDispatchFailed {
            reason: "dispatch requires daemon mode (run `filament serve` first)".to_string(),
        });
    }

    // Get ready tasks, then dispatch individually to avoid concurrent write races
    let ready = conn.ready_tasks().await?;
    let to_dispatch: Vec<_> = ready.into_iter().take(args.max_parallel).collect();

    let mut dispatched = Vec::new();
    let mut errors = Vec::new();

    for task in &to_dispatch {
        let slug = task.slug().as_str();
        match conn.dispatch_agent(slug, &args.role).await {
            Ok(run_id) => dispatched.push((slug.to_string(), run_id)),
            Err(e) => errors.push((slug.to_string(), e.to_string())),
        }
    }

    if cli.json {
        let result = serde_json::json!({
            "dispatched": dispatched.iter().map(|(slug, run_id)| {
                serde_json::json!({"task_slug": slug, "run_id": run_id.as_str()})
            }).collect::<Vec<_>>(),
            "errors": errors.iter().map(|(slug, err)| {
                serde_json::json!({"task_slug": slug, "error": err})
            }).collect::<Vec<_>>(),
        });
        output_json(&result);
    } else {
        println!(
            "Dispatched {} agents ({} errors)",
            dispatched.len(),
            errors.len()
        );
        for (slug, run_id) in &dispatched {
            println!("  {slug} -> {run_id}");
        }
        for (slug, err) in &errors {
            eprintln!("  {slug} error: {err}");
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
