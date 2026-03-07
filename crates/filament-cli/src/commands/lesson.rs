use clap::{Args, Subcommand};
use filament_core::config::FilamentConfig;
use filament_core::dto::{CreateCommon, CreateContentOptional, CreateEntityRequest};
use filament_core::error::Result;
use filament_core::models::{LessonFields, Priority};

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
            LessonSubcommand::Delete(args) => delete(cli, args).await,
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
    /// Delete a lesson.
    Delete(LessonDeleteArgs),
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

#[derive(Args, Debug)]
struct LessonDeleteArgs {
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

    let req = CreateEntityRequest::Lesson(CreateContentOptional {
        common: CreateCommon {
            name: args.title.clone(),
            summary: Some(args.learned.clone()),
            priority: {
                let p = args.priority.unwrap_or_else(|| {
                    find_project_root()
                        .map(|r| FilamentConfig::load(&r).resolve_default_priority())
                        .unwrap_or(2)
                });
                Some(Priority::new(p)?)
            },
            key_facts: Some(fields.to_key_facts()),
        },
        content_path: None,
    });

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

    let entities = conn
        .list_lessons(status_filter, args.pattern.as_deref())
        .await?;

    print_entity_list(cli, &entities, "No lessons found.");
    Ok(())
}

async fn delete(cli: &Cli, args: &LessonDeleteArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_lesson(&args.slug).await?;

    conn.delete_entity(entity.id.as_str(), Some(entity.version))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"deleted": entity.id.as_str()}));
    } else {
        println!("Deleted lesson: {} ({})", entity.name, entity.slug);
    }
    Ok(())
}

async fn show(cli: &Cli, args: &LessonShowArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    if !matches!(entity, filament_core::models::Entity::Lesson(_)) {
        return Err(filament_core::error::FilamentError::TypeMismatch {
            expected: filament_core::enums::EntityType::Lesson,
            actual: entity.entity_type(),
            slug: entity.common().slug.clone(),
        });
    }
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
