use std::path::PathBuf;

use clap::Args;
use filament_core::dto::ExportData;
use filament_core::error::{FilamentError, Result};

use super::helpers::connect;
use crate::Cli;

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Write output to file instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Exclude events from the export.
    #[arg(long)]
    no_events: bool,
}

pub async fn export(_cli: &Cli, args: &ExportArgs) -> Result<()> {
    let mut conn = connect().await?;
    let data = conn.export_all(!args.no_events).await?;
    let json =
        serde_json::to_string_pretty(&data).map_err(|e| FilamentError::Protocol(e.to_string()))?;

    if let Some(path) = &args.output {
        std::fs::write(path, &json)?;
        eprintln!("Exported to {}", path.display());
    } else {
        println!("{json}");
    }

    Ok(())
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// Read input from file instead of stdin.
    #[arg(long)]
    input: Option<PathBuf>,
    /// Skip importing events.
    #[arg(long)]
    no_events: bool,
}

pub async fn import(cli: &Cli, args: &ImportArgs) -> Result<()> {
    let json_str = if let Some(path) = &args.input {
        std::fs::read_to_string(path)?
    } else {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(FilamentError::Io)?;
        buf
    };

    let data: ExportData = serde_json::from_str(&json_str)
        .map_err(|e| FilamentError::Validation(format!("invalid export JSON: {e}")))?;

    let mut conn = connect().await?;
    let result = conn.import_data(&data, !args.no_events).await?;

    if cli.json {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        println!("{json}");
    } else {
        println!("Imported:");
        println!("  entities:  {}", result.entities_imported);
        println!("  relations: {}", result.relations_imported);
        println!("  messages:  {}", result.messages_imported);
        println!("  events:    {}", result.events_imported);
    }

    Ok(())
}
