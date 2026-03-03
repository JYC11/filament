use clap::{Args, Subcommand};
use filament_core::error::Result;
use filament_core::models::{CreateEntityRequest, CreateRelationRequest, EntityId};

use super::helpers::{
    connect, output_json, print_entity_list, resolve_entity, resolve_entity_id,
    truncate_with_ellipsis,
};
use crate::Cli;

#[derive(Args, Debug)]
pub struct TaskCommand {
    #[command(subcommand)]
    command: TaskSubcommand,
}

impl TaskCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.command {
            TaskSubcommand::Add(args) => add(cli, args).await,
            TaskSubcommand::List(args) => list(cli, args).await,
            TaskSubcommand::Ready(args) => ready(cli, args).await,
            TaskSubcommand::Show(args) => show(cli, args).await,
            TaskSubcommand::Close(args) => close(cli, args).await,
            TaskSubcommand::Assign(args) => assign(cli, args).await,
            TaskSubcommand::CriticalPath(args) => critical_path(cli, args).await,
        }
    }
}

#[derive(Subcommand, Debug)]
enum TaskSubcommand {
    /// Add a new task.
    Add(TaskAddArgs),
    /// List tasks.
    List(TaskListArgs),
    /// Show ready (unblocked) tasks.
    Ready(TaskReadyArgs),
    /// Show task details.
    Show(TaskShowArgs),
    /// Close a task.
    Close(TaskCloseArgs),
    /// Assign a task to an agent.
    Assign(TaskAssignArgs),
    /// Show critical path from a task.
    CriticalPath(TaskCriticalPathArgs),
}

#[derive(Args, Debug)]
struct TaskAddArgs {
    /// Task title (used as entity name).
    title: String,
    /// Summary description.
    #[arg(long, default_value = "")]
    summary: String,
    /// Priority (0=highest, 4=lowest).
    #[arg(long)]
    priority: Option<u8>,
    /// Name of entity this task blocks.
    #[arg(long)]
    blocks: Option<String>,
    /// Name of entity this task depends on.
    #[arg(long)]
    depends_on: Option<String>,
}

#[derive(Args, Debug)]
struct TaskListArgs {
    #[allow(clippy::doc_markdown)]
    /// Filter by status (open, closed, in_progress, all).
    #[arg(long, default_value = "open", conflicts_with = "unblocked")]
    status: String,
    /// Show only unblocked tasks (cannot be combined with --status).
    #[arg(long)]
    unblocked: bool,
}

#[derive(Args, Debug)]
struct TaskReadyArgs {
    /// Maximum number of tasks to show.
    #[arg(long, default_value = "20")]
    limit: usize,
}

#[derive(Args, Debug)]
struct TaskShowArgs {
    /// Task name or ID.
    name: String,
}

#[derive(Args, Debug)]
struct TaskCloseArgs {
    /// Task name or ID.
    name: String,
}

#[derive(Args, Debug)]
struct TaskAssignArgs {
    /// Task name or ID.
    name: String,
    /// Agent name to assign to.
    #[arg(long)]
    to: String,
}

#[derive(Args, Debug)]
struct TaskCriticalPathArgs {
    /// Task name or ID.
    name: String,
}

async fn add(cli: &Cli, args: &TaskAddArgs) -> Result<()> {
    let mut conn = connect().await?;

    let req = CreateEntityRequest {
        name: args.title.clone(),
        entity_type: "task".to_string(),
        summary: Some(args.summary.clone()),
        key_facts: None,
        content_path: None,
        priority: args.priority,
    };

    // Resolve relation targets before creating the entity
    let blocks_id = if let Some(blocks_name) = &args.blocks {
        Some(resolve_entity_id(&mut conn, blocks_name).await?)
    } else {
        None
    };
    let depends_on_id = if let Some(dep_name) = &args.depends_on {
        Some(resolve_entity_id(&mut conn, dep_name).await?)
    } else {
        None
    };

    let id = conn.create_entity(req).await?;

    if let Some(target_id) = blocks_id {
        let rel_req = CreateRelationRequest {
            source_id: id.to_string(),
            target_id: target_id.to_string(),
            relation_type: "blocks".to_string(),
            weight: None,
            summary: None,
            metadata: None,
        };
        conn.create_relation(rel_req).await?;
    }

    if let Some(dep_id) = depends_on_id {
        let rel_req = CreateRelationRequest {
            source_id: id.to_string(),
            target_id: dep_id.to_string(),
            relation_type: "depends_on".to_string(),
            weight: None,
            summary: None,
            metadata: None,
        };
        conn.create_relation(rel_req).await?;
    }

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!("Created task: {id}");
    }
    Ok(())
}

async fn list(cli: &Cli, args: &TaskListArgs) -> Result<()> {
    let mut conn = connect().await?;

    let status_filter = match args.status.as_str() {
        "all" => None,
        other => Some(other),
    };

    if args.unblocked {
        let tasks = conn.ready_tasks().await?;
        print_entity_list(cli, &tasks, "No tasks found.");
        return Ok(());
    }

    let entities = conn.list_entities(Some("task"), status_filter).await?;
    print_entity_list(cli, &entities, "No tasks found.");
    Ok(())
}

async fn ready(cli: &Cli, args: &TaskReadyArgs) -> Result<()> {
    let mut conn = connect().await?;

    let tasks = conn.ready_tasks().await?;
    let limited: Vec<_> = tasks.into_iter().take(args.limit).collect();

    if cli.json {
        let items: Vec<_> = limited
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name.as_str(),
                    "entity_id": t.id.as_str(),
                    "priority": t.priority.value(),
                    "status": t.status.as_str(),
                    "summary": t.summary.as_str(),
                })
            })
            .collect();
        output_json(&items);
    } else if limited.is_empty() {
        println!("No ready tasks.");
    } else {
        for t in &limited {
            let summary_preview = truncate_with_ellipsis(&t.summary, 60);
            println!(
                "[P{}] {} [{}] {}",
                t.priority, t.name, t.status, summary_preview
            );
        }
    }
    Ok(())
}

async fn show(cli: &Cli, args: &TaskShowArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    if entity.entity_type.as_str() != "task" {
        return Err(filament_core::error::FilamentError::Validation(format!(
            "'{}' is a {}, not a task",
            entity.name, entity.entity_type
        )));
    }

    let relations = conn.list_relations(entity.id.as_str()).await?;

    if cli.json {
        let out = serde_json::json!({
            "entity": entity,
            "relations": relations,
        });
        output_json(&out);
    } else {
        println!("Task:     {}", entity.name);
        println!("ID:       {}", entity.id);
        println!("Status:   {}", entity.status);
        println!("Priority: {}", entity.priority);
        if !entity.summary.is_empty() {
            println!("Summary:  {}", entity.summary);
        }
        if !relations.is_empty() {
            println!("Relations:");
            for r in &relations {
                let other_id = if r.source_id == entity.id {
                    &r.target_id
                } else {
                    &r.source_id
                };
                let other_name = conn
                    .get_entity(other_id.as_str())
                    .await
                    .map_or_else(|_| other_id.to_string(), |e| e.name.to_string());
                if r.source_id == entity.id {
                    println!("  {} -> {} ({})", entity.name, other_name, r.relation_type);
                } else {
                    println!("  {} -> {} ({})", other_name, entity.name, r.relation_type);
                }
            }
        }
    }
    Ok(())
}

async fn close(cli: &Cli, args: &TaskCloseArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    if entity.entity_type.as_str() != "task" {
        return Err(filament_core::error::FilamentError::Validation(format!(
            "'{}' is a {}, not a task",
            entity.name, entity.entity_type
        )));
    }

    conn.update_entity_status(entity.id.as_str(), "closed")
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"closed": entity.id.as_str()}));
    } else {
        println!("Closed task: {} ({})", entity.name, entity.id);
    }
    Ok(())
}

async fn assign(cli: &Cli, args: &TaskAssignArgs) -> Result<()> {
    let mut conn = connect().await?;
    let task = resolve_entity(&mut conn, &args.name).await?;

    if task.entity_type.as_str() != "task" {
        return Err(filament_core::error::FilamentError::Validation(format!(
            "'{}' is a {}, not a task",
            task.name, task.entity_type
        )));
    }

    let agent = resolve_entity_id(&mut conn, &args.to).await?;

    let rel_req = CreateRelationRequest {
        source_id: agent.to_string(),
        target_id: task.id.to_string(),
        relation_type: "assigned_to".to_string(),
        weight: None,
        summary: None,
        metadata: None,
    };

    conn.create_relation(rel_req).await?;

    if cli.json {
        output_json(&serde_json::json!({"assigned": task.name.as_str(), "to": args.to}));
    } else {
        println!("Assigned {} to {}", task.name, args.to);
    }
    Ok(())
}

async fn critical_path(cli: &Cli, args: &TaskCriticalPathArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    let path = conn.critical_path(entity.id.as_str()).await?;

    if cli.json {
        let items: Vec<_> = path.iter().map(EntityId::as_str).collect();
        output_json(&items);
    } else if path.is_empty() {
        println!("No dependency chain found for: {}", entity.name);
    } else {
        let label = if path.len() == 1 { "step" } else { "steps" };
        println!("Critical path ({} {label}):", path.len());
        for (i, id) in path.iter().enumerate() {
            let name = conn
                .get_entity(id.as_str())
                .await
                .map_or_else(|_| id.to_string(), |e| e.name.to_string());
            println!("  {}. {}", i + 1, name);
        }
    }
    Ok(())
}
