use std::path::{Path, PathBuf};

use filament_core::connection::FilamentConnection;
use filament_core::error::{FilamentError, Result};
use filament_core::models::{Entity, EntityId};
use filament_core::store::{self, FilamentStore};

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

/// Connect to the project database, returning the store handle.
pub async fn connect() -> Result<FilamentStore> {
    let root = find_project_root()?;
    let conn = FilamentConnection::auto_detect(&root).await?;
    match conn {
        FilamentConnection::Direct(store) => Ok(store),
        FilamentConnection::Socket(_) => Err(FilamentError::Validation(
            "daemon mode not yet supported".to_string(),
        )),
    }
}

/// Resolve an entity name or ID to an `Entity`.
pub async fn resolve_entity(store: &FilamentStore, name_or_id: &str) -> Result<Entity> {
    // Try by name first (most common CLI usage)
    match store::get_entity_by_name(store.pool(), name_or_id).await {
        Ok(entity) => return Ok(entity),
        Err(FilamentError::EntityNotFound { .. }) => {}
        Err(e) => return Err(e),
    }
    // Fall back to ID lookup
    store::get_entity(store.pool(), name_or_id).await
}

/// Resolve an entity name to just the ID.
pub async fn resolve_entity_id(store: &FilamentStore, name_or_id: &str) -> Result<EntityId> {
    Ok(resolve_entity(store, name_or_id).await?.id)
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
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    } else {
        s.to_string()
    }
}

/// Print a list of entities in human-readable or JSON format.
pub fn print_entity_list(cli: &Cli, entities: &[Entity], empty_msg: &str) {
    if cli.json {
        output_json(&entities);
    } else if entities.is_empty() {
        println!("{empty_msg}");
    } else {
        for e in entities {
            let summary_preview = truncate_with_ellipsis(&e.summary, 60);
            println!(
                "[P{}] {} ({}) [{}] {}",
                e.priority, e.name, e.entity_type, e.status, summary_preview
            );
        }
    }
}
