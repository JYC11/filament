mod agent;
mod entity;
pub mod helpers;
mod init;
mod mcp;
mod message;
mod query;
mod relation;
mod reserve;
mod serve;
mod task;
mod tui;

use clap::Subcommand;
use filament_core::error::Result;

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

    // -- Agent subgroup --
    /// Agent dispatching and monitoring commands.
    Agent(agent::AgentCommand),

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

    // -- Daemon commands --
    /// Start the filament daemon.
    Serve(serve::ServeArgs),
    /// Stop the filament daemon.
    Stop,

    // -- MCP server --
    /// Start the MCP stdio server (for AI agent integration).
    Mcp,

    // -- TUI --
    /// Launch the interactive TUI.
    Tui,
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
            Self::Agent(cmd) => cmd.run(cli).await,
            Self::Context(args) => query::context(cli, args).await,
            Self::Message(cmd) => cmd.run(cli).await,
            Self::Reserve(args) => reserve::reserve(cli, args).await,
            Self::Release(args) => reserve::release(cli, args).await,
            Self::Reservations(args) => reserve::reservations(cli, args).await,
            Self::Serve(args) => serve::serve(cli, args).await,
            Self::Stop => serve::stop(cli).await,
            Self::Mcp => mcp::run().await,
            Self::Tui => tui::run().await,
        }
    }
}
