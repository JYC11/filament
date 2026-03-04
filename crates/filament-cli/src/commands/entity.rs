use std::path::Path;

use clap::Args;
use filament_core::dto::CreateEntityRequest;
use filament_core::error::Result;
use filament_core::models::{EntityType, Priority};

use super::helpers::{connect, output_json, print_entity_list, print_relations, read_content_file};
use crate::Cli;

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Entity name.
    name: String,
    /// Entity type (task, module, service, agent, plan, doc).
    #[arg(long, rename_all = "snake_case")]
    r#type: EntityType,
    /// Short summary.
    #[arg(long, default_value = "")]
    summary: String,
    /// Key facts as JSON object.
    #[arg(long)]
    facts: Option<String>,
    /// Path to content file.
    #[arg(long)]
    content: Option<String>,
    /// Priority (0=highest, 4=lowest).
    #[arg(long)]
    priority: Option<u8>,
}

#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// Entity slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Entity slug or ID.
    slug: String,
    /// New summary.
    #[arg(long)]
    summary: Option<String>,
    /// New status: open, closed, `in_progress`, blocked.
    #[arg(long)]
    status: Option<filament_core::models::EntityStatus>,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Entity slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
pub struct ReadArgs {
    /// Entity slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter by entity type.
    #[arg(long, rename_all = "snake_case")]
    r#type: Option<EntityType>,
    /// Filter by status (open, `in_progress`, closed, blocked, all).
    #[arg(long)]
    status: Option<String>,
}

pub async fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
    if let Some(ref path) = args.content {
        if !std::path::Path::new(path).exists() {
            return Err(filament_core::error::FilamentError::Validation(format!(
                "content file not found: {path}"
            )));
        }
    }

    let mut conn = connect().await?;

    let key_facts: Option<serde_json::Value> = args
        .facts
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            filament_core::error::FilamentError::Validation(format!(
                "invalid JSON for --facts: {e}"
            ))
        })?;

    let req = CreateEntityRequest {
        name: args.name.clone(),
        entity_type: args.r#type.clone(),
        summary: Some(args.summary.clone()),
        key_facts,
        content_path: args.content.clone(),
        priority: args.priority.map(Priority::new).transpose()?,
    };

    let (id, slug) = conn.create_entity(req).await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str(), "slug": slug.as_str()}));
    } else {
        println!("Created entity: {slug} ({id})");
    }
    Ok(())
}

pub async fn remove(cli: &Cli, args: &RemoveArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;

    conn.delete_entity(entity.id().as_str()).await?;

    if cli.json {
        output_json(&serde_json::json!({"deleted": entity.id().as_str()}));
    } else {
        println!("Removed entity: {} ({})", entity.name(), entity.slug());
    }
    Ok(())
}

pub async fn update(cli: &Cli, args: &UpdateArgs) -> Result<()> {
    if args.summary.is_none() && args.status.is_none() {
        return Err(filament_core::error::FilamentError::Validation(
            "specify at least one of --summary or --status to update".to_string(),
        ));
    }

    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let id = entity.id().clone();

    if let Some(summary) = &args.summary {
        conn.update_entity_summary(id.as_str(), summary).await?;
    }
    if let Some(status) = &args.status {
        conn.update_entity_status(id.as_str(), status.clone())
            .await?;
    }

    if cli.json {
        output_json(&serde_json::json!({"updated": id.as_str()}));
    } else {
        println!("Updated entity: {} ({})", entity.name(), entity.slug());
    }
    Ok(())
}

pub async fn inspect(cli: &Cli, args: &InspectArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let c = entity.common();

    let relations = conn.list_relations(c.id.as_str()).await?;

    if cli.json {
        let out = serde_json::json!({
            "entity": entity,
            "relations": relations,
        });
        output_json(&out);
    } else {
        println!("Name:     {}", c.name);
        println!("Slug:     {}", c.slug);
        println!("ID:       {}", c.id);
        println!("Type:     {}", entity.entity_type());
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
        if let Some(ref content) = c.content {
            println!("Content:  {}", content.path);
        }
        print_relations(&mut conn, &c.id, c.name.as_str(), &relations).await?;
        println!("Created:  {}", c.created_at);
        println!("Updated:  {}", c.updated_at);
    }
    Ok(())
}

pub async fn read(cli: &Cli, args: &ReadArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let c = entity.common();

    let Some(ref content_ref) = c.content else {
        if cli.json {
            output_json(&serde_json::json!({"name": c.name.as_str(), "content": null}));
        } else {
            println!("No content file for entity: {}", c.name);
        }
        return Ok(());
    };

    let content = read_content_file(Path::new(&content_ref.path))?;

    if cli.json {
        let out = serde_json::json!({
            "name": c.name.as_str(),
            "content_path": content_ref.path,
            "content": content,
        });
        output_json(&out);
    } else {
        println!("{content}");
    }
    Ok(())
}

pub async fn list(cli: &Cli, args: &ListArgs) -> Result<()> {
    let mut conn = connect().await?;

    let status_filter: Option<filament_core::models::EntityStatus> = match args.status.as_deref() {
        None | Some("all") => None,
        Some(other) => Some(other.parse()?),
    };

    let entities = conn
        .list_entities(args.r#type.clone(), status_filter)
        .await?;

    print_entity_list(cli, &entities, "No entities found.");
    Ok(())
}
