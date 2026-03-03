use std::path::{Path, PathBuf};

use filament_core::connection::FilamentConnection;
use filament_core::error::{FilamentError, Result};
use filament_core::models::{Entity, EntityCommon, EntityId, Relation};

use crate::Cli;

/// Find the project root by walking up from CWD looking for `.filament/`.
pub fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;

    loop {
        if dir.join(".filament").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(FilamentError::Validation(
                "not a filament project (no .filament/ found). Run `filament init` first."
                    .to_string(),
            ));
        }
    }
}

/// Connect to the project, returning a `FilamentConnection` (Direct or Socket).
pub async fn connect() -> Result<FilamentConnection> {
    let root = find_project_root()?;
    FilamentConnection::auto_detect(&root).await
}

/// Resolve an entity by slug (first) or UUID fallback.
pub async fn resolve_entity(conn: &mut FilamentConnection, slug_or_id: &str) -> Result<Entity> {
    conn.resolve_entity(slug_or_id).await
}

/// Resolve an entity by slug/ID and verify it is a Task.
pub async fn resolve_task(conn: &mut FilamentConnection, slug_or_id: &str) -> Result<EntityCommon> {
    conn.resolve_task(slug_or_id).await
}

/// Resolve an entity by slug/ID and verify it is an Agent.
pub async fn resolve_agent(
    conn: &mut FilamentConnection,
    slug_or_id: &str,
) -> Result<EntityCommon> {
    conn.resolve_agent(slug_or_id).await
}

/// Resolve an entity slug/ID to just the ID.
pub async fn resolve_entity_id(
    conn: &mut FilamentConnection,
    slug_or_id: &str,
) -> Result<EntityId> {
    Ok(resolve_entity(conn, slug_or_id).await?.id().clone())
}

/// Print a value as JSON.
pub fn output_json<T: serde::Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("JSON serialization")
    );
}

/// Read content from a file path.
pub fn read_content_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(FilamentError::Io)
}

/// Truncate a string to `max_chars` characters, appending "..." if truncated.
/// Safe for multi-byte UTF-8 strings (operates on char boundaries).
pub fn truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    if max_chars < 4 {
        return s.chars().take(max_chars).collect();
    }
    let truncated: String = s.chars().take(max_chars - 3).collect();
    format!("{truncated}...")
}

/// Print relations for an entity, batch-fetching related entity names.
pub async fn print_relations(
    conn: &mut FilamentConnection,
    entity_id: &EntityId,
    entity_name: &str,
    relations: &[Relation],
) -> Result<()> {
    if relations.is_empty() {
        return Ok(());
    }
    let other_ids: Vec<String> = relations
        .iter()
        .map(|r| {
            if r.source_id == *entity_id {
                r.target_id.to_string()
            } else {
                r.source_id.to_string()
            }
        })
        .collect();
    let name_map = conn.batch_get_entities(&other_ids).await?;

    println!("Relations:");
    for r in relations {
        let other_id = if r.source_id == *entity_id {
            &r.target_id
        } else {
            &r.source_id
        };
        let other_name = name_map
            .get(other_id.as_str())
            .map_or_else(|| other_id.to_string(), |e| e.name().to_string());
        if r.source_id == *entity_id {
            println!("  {entity_name} -> {other_name} ({})", r.relation_type);
        } else {
            println!("  {other_name} -> {entity_name} ({})", r.relation_type);
        }
    }
    Ok(())
}

/// Print a list of entities in human-readable or JSON format.
pub fn print_entity_list(cli: &Cli, entities: &[Entity], empty_msg: &str) {
    if cli.json {
        output_json(&entities);
    } else if entities.is_empty() {
        println!("{empty_msg}");
    } else {
        for e in entities {
            let summary_preview = truncate_with_ellipsis(e.summary(), 60);
            println!(
                "[{}] {} ({}, {}) [P{}] {}",
                e.slug(),
                e.name(),
                e.entity_type(),
                e.status(),
                e.priority(),
                summary_preview
            );
        }
    }
}
