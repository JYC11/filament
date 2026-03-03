use clap::Args;
use filament_core::error::Result;

use super::helpers::{connect, output_json, resolve_entity};
use crate::Cli;

#[derive(Args, Debug)]
pub struct ContextArgs {
    /// Entity slug or ID to explore around.
    #[arg(long)]
    around: String,
    /// Maximum hops from the entity.
    #[arg(long, default_value = "2")]
    depth: usize,
    /// Maximum results to show.
    #[arg(long, default_value = "20")]
    limit: usize,
}

pub async fn context(cli: &Cli, args: &ContextArgs) -> Result<()> {
    let mut conn = connect().await?;
    let entity = resolve_entity(&mut conn, &args.around).await?;

    let summaries = conn
        .context_summaries(entity.id().as_str(), args.depth)
        .await?;
    let limited: Vec<_> = summaries.into_iter().take(args.limit).collect();

    if cli.json {
        output_json(&limited);
    } else if limited.is_empty() {
        println!("No context found around: {}", entity.name());
    } else {
        println!("Context around {} (depth {}):", entity.name(), args.depth);
        for s in &limited {
            println!("  {s}");
        }
    }
    Ok(())
}
