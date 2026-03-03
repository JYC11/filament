mod entity;
mod init;
mod message;
mod query;
mod relation;
mod reserve;
mod task;

use std::path::{Path, PathBuf};

use clap::Subcommand;
use filament_core::connection::FilamentConnection;
use filament_core::error::{FilamentError, Result};
use filament_core::models::{Entity, EntityId};
use filament_core::store::{self, FilamentStore};

use crate::Cli;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new filament project in the current directory.
    Init,

    // -- Entity commands (top-level) --
    /// Add a new entity.
    Add(entity::AddArgs),
    /// Remove an entity.
    Remove(entity::RemoveArgs),
    /// Update an entity.
    Update(entity::UpdateArgs),
    /// Inspect an entity (show details and key facts).
    Inspect(entity::InspectArgs),
    /// Read an entity's full content file.
    Read(entity::ReadArgs),
    /// List entities.
    List(entity::ListArgs),

    // -- Relation commands (top-level) --
    /// Create a relation between two entities.
    Relate(relation::RelateArgs),
    /// Remove a relation between two entities.
    Unrelate(relation::UnrelateArgs),

    // -- Task subgroup --
    /// Task management commands.
    Task(task::TaskCommand),

    // -- Query commands --
    /// Show context around an entity (graph neighborhood).
    Context(query::ContextArgs),

    // -- Message commands --
    /// Messaging commands.
    Message(message::MessageCommand),

    // -- Reservation commands --
    /// Acquire a file reservation.
    Reserve(reserve::ReserveArgs),
    /// Release a file reservation.
    Release(reserve::ReleaseArgs),
    /// List file reservations.
    Reservations(reserve::ReservationsArgs),
}

impl Commands {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match self {
            Self::Init => init::run(cli).await,
            Self::Add(args) => entity::add(cli, args).await,
            Self::Remove(args) => entity::remove(cli, args).await,
            Self::Update(args) => entity::update(cli, args).await,
            Self::Inspect(args) => entity::inspect(cli, args).await,
            Self::Read(args) => entity::read(cli, args).await,
            Self::List(args) => entity::list(cli, args).await,
            Self::Relate(args) => relation::relate(cli, args).await,
            Self::Unrelate(args) => relation::unrelate(cli, args).await,
            Self::Task(cmd) => cmd.run(cli).await,
            Self::Context(args) => query::context(cli, args).await,
            Self::Message(cmd) => cmd.run(cli).await,
            Self::Reserve(args) => reserve::reserve(cli, args).await,
            Self::Release(args) => reserve::release(cli, args).await,
            Self::Reservations(args) => reserve::reservations(cli, args).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Find the project root by walking up from CWD looking for `.filament/`.
fn find_project_root() -> Result<PathBuf> {
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
async fn connect() -> Result<FilamentStore> {
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
async fn resolve_entity(store: &FilamentStore, name_or_id: &str) -> Result<Entity> {
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
async fn resolve_entity_id(store: &FilamentStore, name_or_id: &str) -> Result<EntityId> {
    Ok(resolve_entity(store, name_or_id).await?.id)
}

/// Print a value as JSON.
fn output_json<T: serde::Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("JSON serialization")
    );
}

/// Read content from a file path.
fn read_content_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(FilamentError::Io)
}
