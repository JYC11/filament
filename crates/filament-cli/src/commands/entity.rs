use std::path::Path;

use clap::Args;
use filament_core::config::FilamentConfig;
use filament_core::dto::{ChangesetCommon, CreateEntityRequest, EntityChangeset};
use filament_core::error::Result;
use filament_core::models::{EntityType, LessonFields, Priority};

use super::helpers::{
    connect, find_project_root, output_json, print_entity_list, print_relations, read_content_file,
};
use crate::Cli;

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Entity name.
    name: String,
    /// Entity type (task, module, service, agent, plan, doc, lesson).
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
    /// New priority (0=highest, 4=lowest).
    #[arg(long)]
    priority: Option<u8>,
    /// New key facts as JSON object.
    #[arg(long)]
    facts: Option<String>,
    /// New content file path.
    #[arg(long)]
    content: Option<String>,
    /// Expected version for conflict detection.
    ///
    /// If omitted, the current version is read automatically.
    #[arg(long)]
    version: Option<i64>,
}

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Entity slug or ID.
    slug: String,
}

#[derive(Args, Debug)]
pub struct ResolveArgs {
    /// Entity slug or ID.
    slug: String,
    /// Accept all current (theirs) values, resolving the conflict.
    #[arg(long)]
    theirs: bool,
    /// Override summary during resolve.
    #[arg(long)]
    summary: Option<String>,
    /// Override status during resolve.
    #[arg(long)]
    status: Option<filament_core::models::EntityStatus>,
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

    let priority = {
        let p = args.priority.unwrap_or_else(|| {
            find_project_root()
                .map(|r| FilamentConfig::load(&r).resolve_default_priority())
                .unwrap_or(2)
        });
        Some(Priority::new(p)?)
    };

    let req = CreateEntityRequest::from_parts(
        args.r#type,
        args.name.clone(),
        Some(args.summary.clone()),
        priority,
        key_facts,
        args.content.clone(),
    )?;

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

    conn.delete_entity(entity.id().as_str(), Some(entity.common().version))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"deleted": entity.id().as_str()}));
    } else {
        println!("Removed entity: {} ({})", entity.name(), entity.slug());
    }
    Ok(())
}

pub async fn update(cli: &Cli, args: &UpdateArgs) -> Result<()> {
    if args.summary.is_none()
        && args.status.is_none()
        && args.priority.is_none()
        && args.facts.is_none()
        && args.content.is_none()
    {
        return Err(filament_core::error::FilamentError::Validation(
            "specify at least one of --summary, --status, --priority, --facts, or --content to update".to_string(),
        ));
    }

    if let Some(ref path) = args.content {
        if !Path::new(path).exists() {
            return Err(filament_core::error::FilamentError::Validation(format!(
                "content file not found: {path}"
            )));
        }
    }

    let priority = args.priority.map(Priority::new).transpose()?;

    let key_facts: Option<String> = args
        .facts
        .as_deref()
        .map(|s| {
            serde_json::from_str::<serde_json::Value>(s)
                .map_err(|e| {
                    filament_core::error::FilamentError::Validation(format!(
                        "invalid JSON for --facts: {e}"
                    ))
                })
                .map(|v| v.to_string())
        })
        .transpose()?;

    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let id = entity.id().clone();
    let expected_version = args.version.unwrap_or_else(|| entity.common().version);

    let changeset = EntityChangeset::for_type(
        entity.entity_type(),
        ChangesetCommon {
            name: None,
            summary: args.summary.clone(),
            status: args.status,
            priority,
            key_facts,
            expected_version,
        },
        args.content.clone(),
    );

    match conn.update_entity(id.as_str(), &changeset).await {
        Ok(updated) => {
            if cli.json {
                output_json(
                    &serde_json::json!({"updated": id.as_str(), "version": updated.common().version}),
                );
            } else {
                println!("Updated entity: {} ({})", entity.name(), entity.slug());
            }
            Ok(())
        }
        Err(filament_core::error::FilamentError::VersionConflict {
            ref entity_id,
            current_version,
            ref conflicts,
        }) if !conflicts.is_empty() => {
            // Print detailed conflict info (suppresses the generic error in main.rs)
            if cli.json {
                output_json(&serde_json::json!({
                    "error": "VERSION_CONFLICT",
                    "entity_id": entity_id,
                    "current_version": current_version,
                    "conflicts": conflicts,
                }));
            } else {
                eprintln!(
                    "Conflict on entity {} (version {current_version}):",
                    entity.slug()
                );
                eprintln!("  {:<15} {:<25} {:<25}", "Field", "Yours", "Theirs");
                for c in conflicts {
                    eprintln!(
                        "  {:<15} {:<25} {:<25}",
                        c.field, c.your_value, c.their_value
                    );
                }
                eprintln!();
                eprintln!(
                    "Re-read the entity and retry, or use: fl resolve {}",
                    entity.slug()
                );
            }
            // Return a non-zero exit code without re-printing the error
            std::process::exit(6);
        }
        Err(e) => Err(e),
    }
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
        println!("Version:  {}", c.version);
        if let Some(fields) = LessonFields::from_entity(&entity) {
            println!("Problem:  {}", fields.problem);
            println!("Solution: {}", fields.solution);
            if let Some(ref pat) = fields.pattern {
                println!("Pattern:  {pat}");
            }
            println!("Learned:  {}", fields.learned);
        } else {
            if !c.summary.is_empty() {
                println!("Summary:  {}", c.summary);
            }
            if c.key_facts != serde_json::json!({}) {
                println!(
                    "Facts:    {}",
                    serde_json::to_string_pretty(&c.key_facts).expect("JSON")
                );
            }
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

    let entities = conn.list_entities(args.r#type, status_filter).await?;

    print_entity_list(cli, &entities, "No entities found.");
    Ok(())
}

pub async fn resolve(cli: &Cli, args: &ResolveArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = conn.resolve_entity(&args.slug).await?;
    let c = entity.common();

    if args.theirs {
        // Accept current DB values — no actual update needed, just acknowledge
        if cli.json {
            output_json(&serde_json::json!({
                "resolved": c.id.as_str(),
                "strategy": "theirs",
                "version": c.version,
            }));
        } else {
            println!(
                "Resolved conflict on {} ({}) — accepted current values (version {})",
                c.name, c.slug, c.version
            );
        }
        return Ok(());
    }

    // Apply user-specified overrides at the current version
    if args.summary.is_none() && args.status.is_none() {
        if !cli.json {
            println!("Current state of {} ({}):", c.name, c.slug);
            println!("  Status:   {}", c.status);
            println!("  Priority: {}", c.priority);
            println!("  Version:  {}", c.version);
            if !c.summary.is_empty() {
                println!("  Summary:  {}", c.summary);
            }
            println!();
            println!("To resolve, specify new values:");
            println!(
                "  fl resolve {} --summary \"...\" --status <status>",
                c.slug
            );
            println!("  fl resolve {} --theirs", c.slug);
        }
        return Ok(());
    }

    let changeset = EntityChangeset::for_type(
        entity.entity_type(),
        ChangesetCommon {
            name: None,
            summary: args.summary.clone(),
            status: args.status,
            priority: None,
            key_facts: None,
            expected_version: c.version,
        },
        None,
    );

    let updated = conn.update_entity(c.id.as_str(), &changeset).await?;
    let uc = updated.common();

    if cli.json {
        output_json(&serde_json::json!({
            "resolved": uc.id.as_str(),
            "version": uc.version,
        }));
    } else {
        println!(
            "Resolved conflict on {} ({}) — now at version {}",
            uc.name, uc.slug, uc.version
        );
    }
    Ok(())
}
