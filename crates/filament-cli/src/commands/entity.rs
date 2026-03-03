use std::path::Path;

use clap::Args;
use filament_core::error::Result;
use filament_core::models::CreateEntityRequest;

use super::helpers::{connect, output_json, print_entity_list, read_content_file, resolve_entity};
use crate::Cli;

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Entity name.
    name: String,
    /// Entity type (task, module, service, agent, plan, doc).
    #[arg(long, rename_all = "snake_case")]
    r#type: String,
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
    /// Entity name or ID.
    name: String,
}

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Entity name or ID.
    name: String,
    /// New summary.
    #[arg(long)]
    summary: Option<String>,
    #[allow(clippy::doc_markdown)]
    /// New status: open, closed, in_progress, blocked.
    #[arg(long)]
    status: Option<String>,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Entity name or ID.
    name: String,
}

#[derive(Args, Debug)]
pub struct ReadArgs {
    /// Entity name or ID.
    name: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter by entity type.
    #[arg(long, rename_all = "snake_case")]
    r#type: Option<String>,
    /// Filter by status.
    #[arg(long)]
    status: Option<String>,
}

pub async fn add(cli: &Cli, args: &AddArgs) -> Result<()> {
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
        priority: args.priority,
    };

    let id = conn.create_entity(req).await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!("Created entity: {id}");
    }
    Ok(())
}

pub async fn remove(cli: &Cli, args: &RemoveArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    conn.delete_entity(entity.id.as_str()).await?;

    if cli.json {
        output_json(&serde_json::json!({"deleted": entity.id.as_str()}));
    } else {
        println!("Removed entity: {} ({})", entity.name, entity.id);
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
    let entity = resolve_entity(&mut conn, &args.name).await?;
    let id = entity.id.clone();

    if let Some(summary) = &args.summary {
        conn.update_entity_summary(id.as_str(), summary).await?;
    }
    if let Some(status) = &args.status {
        conn.update_entity_status(id.as_str(), status).await?;
    }

    if cli.json {
        output_json(&serde_json::json!({"updated": id.as_str()}));
    } else {
        println!("Updated entity: {} ({})", entity.name, id);
    }
    Ok(())
}

pub async fn inspect(cli: &Cli, args: &InspectArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    let relations = conn.list_relations(entity.id.as_str()).await?;

    if cli.json {
        let out = serde_json::json!({
            "entity": entity,
            "relations": relations,
        });
        output_json(&out);
    } else {
        println!("Name:     {}", entity.name);
        println!("ID:       {}", entity.id);
        println!("Type:     {}", entity.entity_type);
        println!("Status:   {}", entity.status);
        println!("Priority: {}", entity.priority);
        if !entity.summary.is_empty() {
            println!("Summary:  {}", entity.summary);
        }
        if entity.key_facts != serde_json::json!({}) {
            println!(
                "Facts:    {}",
                serde_json::to_string_pretty(&entity.key_facts).expect("JSON")
            );
        }
        if let Some(ref path) = entity.content_path {
            println!("Content:  {path}");
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
        println!("Created:  {}", entity.created_at);
        println!("Updated:  {}", entity.updated_at);
    }
    Ok(())
}

pub async fn read(cli: &Cli, args: &ReadArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.name).await?;

    let Some(ref content_path) = entity.content_path else {
        if cli.json {
            output_json(&serde_json::json!({"name": entity.name.as_str(), "content": null}));
        } else {
            println!("No content file for entity: {}", entity.name);
        }
        return Ok(());
    };

    let content = read_content_file(Path::new(content_path))?;

    if cli.json {
        let out = serde_json::json!({
            "name": entity.name.as_str(),
            "content_path": content_path,
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

    let entities = conn
        .list_entities(args.r#type.as_deref(), args.status.as_deref())
        .await?;

    print_entity_list(cli, &entities, "No entities found.");
    Ok(())
}
