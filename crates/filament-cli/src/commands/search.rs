use clap::Args;
use filament_core::error::Result;
use filament_core::models::EntityType;

use super::helpers::{connect, output_json, truncate_with_ellipsis};
use crate::Cli;

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query (FTS5 syntax: words, phrases "like this", OR, NOT).
    query: String,
    /// Filter by entity type.
    #[arg(long, rename_all = "snake_case")]
    r#type: Option<EntityType>,
    /// Maximum results to return.
    #[arg(long, default_value = "20")]
    limit: u32,
}

pub async fn search(cli: &Cli, args: &SearchArgs) -> Result<()> {
    let mut conn = connect().await?;
    let results = conn
        .search_entities(&args.query, args.r#type, args.limit)
        .await?;

    if cli.json {
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|(e, rank)| {
                serde_json::json!({
                    "slug": e.slug().as_str(),
                    "name": e.name().as_str(),
                    "entity_type": e.entity_type().as_str(),
                    "status": e.status().as_str(),
                    "priority": e.priority().value(),
                    "rank": rank,
                    "summary": e.summary(),
                })
            })
            .collect();
        output_json(&items);
    } else if results.is_empty() {
        println!("No results found.");
    } else {
        for (e, rank) in &results {
            let summary_preview = truncate_with_ellipsis(e.summary(), 60);
            println!(
                "[{}] {} ({}, {}) [P{}] {:.2} {}",
                e.slug(),
                e.name(),
                e.entity_type(),
                e.status(),
                e.priority(),
                rank,
                summary_preview
            );
        }
    }
    Ok(())
}
