use clap::{Args, Subcommand};
use filament_core::config::FilamentConfig;
use filament_core::dto::CreateEntityRequest;
use filament_core::error::Result;
use filament_core::models::{EntityType, LessonFields, Priority};

use super::helpers::{connect, find_project_root, output_json, print_entity_list};
use crate::Cli;

#[derive(Args, Debug)]
pub struct LessonCommand {
    #[command(subcommand)]
    command: LessonSubcommand,
}

impl LessonCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.command {
            LessonSubcommand::Add(args) => add(cli, args).await,
            LessonSubcommand::List(args) => list(cli, args).await,
            LessonSubcommand::Show(args) => show(cli, args).await,
        }
    }
}

#[derive(Subcommand, Debug)]
enum LessonSubcommand {
    /// Record a new lesson (gotcha, pattern, solution).
    Add(LessonAddArgs),
    /// List lessons.
    List(LessonListArgs),
    /// Show lesson details.
    Show(LessonShowArgs),
}

#[derive(Args, Debug)]
struct LessonAddArgs {
    /// Lesson title (used as entity name).
    title: String,
    /// What was failing (symptoms, error messages).
    #[arg(long)]
    problem: String,
    /// Specific steps taken to fix.
    #[arg(long)]
    solution: String,
    /// Key insight for next time.
    #[arg(long)]
    learned: String,
    /// Optional reusable pattern name (e.g., "n-plus-one-fix", "circuit-breaker").
    #[arg(long)]
    pattern: Option<String>,
    /// Priority (0=highest, 4=lowest).
    #[arg(long)]
    priority: Option<u8>,
}

#[derive(Args, Debug)]
struct LessonListArgs {
    /// Filter by pattern name.
    #[arg(long)]
    pattern: Option<String>,
    /// Filter by status (open, closed, all).
    #[arg(long, default_value = "all")]
    status: String,
}

#[derive(Args, Debug)]
struct LessonShowArgs {
    /// Lesson slug or ID.
    slug: String,
}

async fn add(cli: &Cli, args: &LessonAddArgs) -> Result<()> {
    let mut conn = connect().await?;

    let fields = LessonFields {
        problem: args.problem.clone(),
        solution: args.solution.clone(),
        pattern: args.pattern.clone(),
        learned: args.learned.clone(),
    };

    let req = CreateEntityRequest {
        name: args.title.clone(),
        entity_type: EntityType::Lesson,
        summary: Some(args.learned.clone()),
        key_facts: Some(fields.to_key_facts()),
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

    let (id, slug) = conn.create_entity(req).await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str(), "slug": slug.as_str()}));
    } else {
        println!("Created lesson: {slug} ({id})");
    }
    Ok(())
}

async fn list(cli: &Cli, args: &LessonListArgs) -> Result<()> {
    let mut conn = connect().await?;

    let status_filter = match args.status.as_str() {
        "all" => None,
        other => Some(other.parse()?),
    };

    let mut entities = conn
        .list_entities(Some(EntityType::Lesson), status_filter)
        .await?;

    // Filter by pattern if specified
    if let Some(ref pat) = args.pattern {
        let pat_lower = pat.to_lowercase();
        entities.retain(|e| {
            LessonFields::from_entity(e)
                .and_then(|f| f.pattern)
                .is_some_and(|p| p.to_lowercase().contains(&pat_lower))
        });
    }

    print_entity_list(cli, &entities, "No lessons found.");
    Ok(())
}

async fn show(cli: &Cli, args: &LessonShowArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let c = entity.common();

    if cli.json {
        let out = serde_json::json!({
            "entity": entity,
            "lesson_fields": LessonFields::from_entity(&entity),
        });
        output_json(&out);
    } else {
        println!("Lesson:   {}", c.name);
        println!("Slug:     {}", c.slug);
        println!("ID:       {}", c.id);
        println!("Status:   {}", c.status);
        println!("Priority: {}", c.priority);

        if let Some(fields) = LessonFields::from_entity(&entity) {
            println!("Problem:  {}", fields.problem);
            println!("Solution: {}", fields.solution);
            if let Some(ref pat) = fields.pattern {
                println!("Pattern:  {pat}");
            }
            println!("Learned:  {}", fields.learned);
        } else if !c.summary.is_empty() {
            println!("Summary:  {}", c.summary);
        }

        let relations = conn.list_relations(c.id.as_str()).await?;
        super::helpers::print_relations(&mut conn, &c.id, c.name.as_str(), &relations).await?;
        println!("Created:  {}", c.created_at);
        println!("Updated:  {}", c.updated_at);
    }
    Ok(())
}
