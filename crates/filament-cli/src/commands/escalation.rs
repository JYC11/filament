use filament_core::error::{FilamentError, Result};

use super::helpers::connect;
use crate::Cli;

pub async fn escalations(cli: &Cli) -> Result<()> {
    let mut conn = connect().await?;
    let items = conn.list_pending_escalations().await?;

    if cli.json {
        let json = serde_json::to_string_pretty(&items)
            .map_err(|e| FilamentError::Protocol(e.to_string()))?;
        println!("{json}");
        return Ok(());
    }

    if items.is_empty() {
        println!("No pending escalations.");
        return Ok(());
    }

    let header = format!("{:<12} {:<20} {:<50} {}", "KIND", "AGENT", "BODY", "TASK");
    println!("{header}");
    println!("{}", "-".repeat(90));

    for esc in &items {
        let task = esc
            .task_id
            .as_deref()
            .unwrap_or("-");
        let body = if esc.body.chars().count() > 50 {
            let truncated: String = esc.body.chars().take(47).collect();
            format!("{truncated}...")
        } else {
            esc.body.clone()
        };
        println!(
            "{:<12} {:<20} {:<50} {}",
            esc.kind, esc.agent_name, body, task
        );
    }

    Ok(())
}
