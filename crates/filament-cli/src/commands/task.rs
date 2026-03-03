use clap::{Args, Subcommand};
use filament_core::error::Result;
use filament_core::models::{CreateEntityRequest, CreateRelationRequest, EntityId};

use super::helpers::{
    connect, output_json, print_entity_list, resolve_agent, resolve_entity, resolve_entity_id,
    resolve_task, truncate_with_ellipsis,
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
    /// Slug or ID of entity this task blocks.
    #[arg(long)]
    blocks: Option<String>,
    /// Slug or ID of entity this task depends on.
    #[arg(long)]
    depends_on: Option<String>,
}

#[derive(Args, Debug)]
struct TaskListArgs {
    /// Filter by status (open, closed, `in_progress`, all).
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
    /// Task slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
struct TaskCloseArgs {
    /// Task slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
struct TaskAssignArgs {
    /// Task slug or ID.
    slug: String,
    /// Agent slug or ID to assign to.
    #[arg(long)]
    to: String,
}

#[derive(Args, Debug)]
struct TaskCriticalPathArgs {
    /// Task slug or ID.
    slug: String,
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
    let blocks_id = if let Some(blocks_slug) = &args.blocks {
        Some(resolve_entity_id(&mut conn, blocks_slug).await?)
    } else {
        None
    };
    let depends_on_id = if let Some(dep_slug) = &args.depends_on {
        Some(resolve_entity_id(&mut conn, dep_slug).await?)
    } else {
        None
    };

    let (id, slug) = conn.create_entity(req).await?;

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
        output_json(&serde_json::json!({"id": id.as_str(), "slug": slug.as_str()}));
    } else {
        println!("Created task: {slug} ({id})");
    }
    Ok(())
}

async fn list(cli: &Cli, args: &TaskListArgs) -> Result<()> {
    let mut conn = connect().await?;

    if args.unblocked {
        let tasks = conn.ready_tasks().await?;
        print_entity_list(cli, &tasks, "No tasks found.");
        return Ok(());
    }

    let status_filter: Option<filament_core::models::EntityStatus> = match args.status.as_str() {
        "all" => None,
        other => Some(other.parse()?),
    };

    let entities = conn
        .list_entities(Some(filament_core::models::EntityType::Task), status_filter)
        .await?;
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
                    "slug": t.slug().as_str(),
                    "name": t.name().as_str(),
                    "entity_id": t.id().as_str(),
                    "priority": t.priority().value(),
                    "status": t.status().as_str(),
                    "summary": t.summary(),
                })
            })
            .collect();
        output_json(&items);
    } else if limited.is_empty() {
        println!("No ready tasks.");
    } else {
        for t in &limited {
            let summary_preview = truncate_with_ellipsis(t.summary(), 60);
            println!(
                "[{}] [P{}] {} [{}] {}",
                t.slug(),
                t.priority(),
                t.name(),
                t.status(),
                summary_preview
            );
        }
    }
    Ok(())
}

async fn show(cli: &Cli, args: &TaskShowArgs) -> Result<()> {
    let mut conn = connect().await?;
    let c = resolve_task(&mut conn, &args.slug).await?;
    let relations = conn.list_relations(c.id.as_str()).await?;

    if cli.json {
        let entity = filament_core::models::Entity::Task(c.clone());
        let out = serde_json::json!({
            "entity": entity,
            "relations": relations,
        });
        output_json(&out);
    } else {
        println!("Task:     {}", c.name);
        println!("Slug:     {}", c.slug);
        println!("ID:       {}", c.id);
        println!("Status:   {}", c.status);
        println!("Priority: {}", c.priority);
        if !c.summary.is_empty() {
            println!("Summary:  {}", c.summary);
        }
        if c.key_facts != serde_json::json!({}) {
            println!(
                "Facts:    {}",
                serde_json::to_string_pretty(&c.key_facts).expect("JSON")
            );
        }
        if !relations.is_empty() {
            // Batch-fetch all related entity names in one query (avoids N+1)
            let other_ids: Vec<String> = relations
                .iter()
                .map(|r| {
                    if r.source_id == c.id {
                        r.target_id.to_string()
                    } else {
                        r.source_id.to_string()
                    }
                })
                .collect();
            let name_map = conn.batch_get_entities(&other_ids).await?;

            println!("Relations:");
            for r in &relations {
                let other_id = if r.source_id == c.id {
                    &r.target_id
                } else {
                    &r.source_id
                };
                let other_name = name_map
                    .get(other_id.as_str())
                    .map_or_else(|| other_id.to_string(), |e| e.name().to_string());
                if r.source_id == c.id {
                    println!("  {} -> {} ({})", c.name, other_name, r.relation_type);
                } else {
                    println!("  {} -> {} ({})", other_name, c.name, r.relation_type);
                }
            }
        }
        println!("Created:  {}", c.created_at);
        println!("Updated:  {}", c.updated_at);
    }
    Ok(())
}

async fn close(cli: &Cli, args: &TaskCloseArgs) -> Result<()> {
    let mut conn = connect().await?;
    let c = resolve_task(&mut conn, &args.slug).await?;

    conn.update_entity_status(c.id.as_str(), filament_core::models::EntityStatus::Closed)
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"closed": c.id.as_str()}));
    } else {
        println!("Closed task: {} ({})", c.name, c.slug);
    }
    Ok(())
}

async fn assign(cli: &Cli, args: &TaskAssignArgs) -> Result<()> {
    let mut conn = connect().await?;
    let task = resolve_task(&mut conn, &args.slug).await?;
    let agent = resolve_agent(&mut conn, &args.to).await?;

    let rel_req = CreateRelationRequest {
        source_id: agent.id.to_string(),
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
    let entity = resolve_entity(&mut conn, &args.slug).await?;

    let path = conn.critical_path(entity.id().as_str()).await?;

    if cli.json {
        let items: Vec<_> = path.iter().map(EntityId::as_str).collect();
        output_json(&items);
    } else if path.is_empty() {
        println!("No dependency chain found for: {}", entity.name());
    } else {
        let label = if path.len() == 1 { "step" } else { "steps" };
        println!("Critical path ({} {label}):", path.len());
        // Batch-fetch all path entity names in one query (avoids N+1)
        let path_ids: Vec<String> = path.iter().map(ToString::to_string).collect();
        let name_map = conn.batch_get_entities(&path_ids).await?;
        for (i, id) in path.iter().enumerate() {
            let name = name_map
                .get(id.as_str())
                .map_or_else(|| id.to_string(), |e| e.name().to_string());
            println!("  {}. {}", i + 1, name);
        }
    }
    Ok(())
}
