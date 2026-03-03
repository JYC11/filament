use std::fs;

use filament_core::connection::FilamentConnection;
use filament_core::error::Result;

use crate::Cli;

pub async fn run(cli: &Cli) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let filament_dir = cwd.join(".filament");

    if filament_dir.exists() {
        if cli.json {
            println!(
                r#"{{"status": "already_initialized", "path": "{}"}}"#,
                filament_dir.display()
            );
        } else {
            println!("Already initialized: {}", filament_dir.display());
        }
        return Ok(());
    }

    fs::create_dir_all(filament_dir.join("content"))?;

    let db_path = filament_dir.join("filament.db");
    let db_str = db_path.to_str().ok_or_else(|| {
        filament_core::error::FilamentError::Validation(format!(
            "database path is not valid UTF-8: {}",
            db_path.display()
        ))
    })?;

    // This creates the DB and runs migrations
    FilamentConnection::direct(db_str).await?;

    if cli.json {
        println!(
            r#"{{"status": "initialized", "path": "{}"}}"#,
            filament_dir.display()
        );
    } else {
        println!("Initialized filament project at {}", filament_dir.display());
    }

    Ok(())
}
