use clap::{Args, Subcommand};
use filament_core::config::FilamentConfig;
use filament_core::dto::{CreateEntityRequest, CreateRelationRequest};
use filament_core::error::Result;
use filament_core::models::{EntityType, Priority, RelationType};

use super::helpers::{
    connect, find_project_root, output_json, print_entity_list, print_relations,
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
            TaskSubcommand::BlockerDepth(args) => blocker_depth(cli, args).await,
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
    /// Show blocker depth of a task.
    BlockerDepth(TaskBlockerDepthArgs),
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
struct TaskBlockerDepthArgs {
    /// Task slug or ID.
    slug: String,
}

async fn add(cli: &Cli, args: &TaskAddArgs) -> Result<()> {
    let mut conn = connect().await?;

    let req = CreateEntityRequest {
        name: args.title.clone(),
        entity_type: EntityType::Task,
        summary: Some(args.summary.clone()),
        key_facts: None,
        content_path: None,
        priority: {
            let p = args.priority.unwrap_or_else(|| {
                find_project_root()
                    .map(|r| FilamentConfig::load(&r).resolve_default_priority())
                    .unwrap_or(2)
            });
            Some(Priority::new(p)?)
        },
    };

    // Resolve relation targets before creating the entity
    let blocks_id = if let Some(blocks_slug) = &args.blocks {
        Some(conn.resolve_entity(blocks_slug).await?.id().clone())
    } else {
        None
    };
    let depends_on_id = if let Some(dep_slug) = &args.depends_on {
        Some(conn.resolve_entity(dep_slug).await?.id().clone())
    } else {
        None
    };

    let (id, slug) = conn.create_entity(req).await?;

    if let Some(target_id) = blocks_id {
        let rel_req = CreateRelationRequest {
            source_id: id.to_string(),
            target_id: target_id.to_string(),
            relation_type: RelationType::Blocks,
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
            relation_type: RelationType::DependsOn,
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
    let c = conn.resolve_task(&args.slug).await?;
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
        print_relations(&mut conn, &c.id, c.name.as_str(), &relations).await?;
        println!("Created:  {}", c.created_at);
        println!("Updated:  {}", c.updated_at);
    }
    Ok(())
}

async fn close(cli: &Cli, args: &TaskCloseArgs) -> Result<()> {
    let mut conn = connect().await?;
    let c = conn.resolve_task(&args.slug).await?;

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
    let task = conn.resolve_task(&args.slug).await?;
    let agent = conn.resolve_agent(&args.to).await?;

    let rel_req = CreateRelationRequest {
        source_id: agent.id.to_string(),
        target_id: task.id.to_string(),
        relation_type: RelationType::AssignedTo,
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

async fn blocker_depth(cli: &Cli, args: &TaskBlockerDepthArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;

    let depth = conn.blocker_depth(entity.id().as_str()).await?;

    if cli.json {
        output_json(&serde_json::json!({ "depth": depth }));
    } else if depth == 0 {
        println!("{}: no upstream blockers", entity.name());
    } else {
        let label = if depth == 1 { "layer" } else { "layers" };
        println!(
            "{}: blocker depth {} ({label} of unclosed prerequisites)",
            entity.name(),
            depth
        );
    }
    Ok(())
}
