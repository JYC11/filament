use clap::Args;
use filament_core::error::Result;

use super::helpers::{connect, output_json};
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
    let entity = conn.resolve_entity(&args.around).await?;

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

#[derive(Args, Debug)]
pub struct PageRankArgs {
    /// Damping factor (default 0.85).
    #[arg(long, default_value = "0.85")]
    damping: f64,
    /// Number of iterations (default 50).
    #[arg(long, default_value = "50")]
    iterations: usize,
    /// Maximum results to show.
    #[arg(long, default_value = "20")]
    limit: usize,
}

pub async fn pagerank(cli: &Cli, args: &PageRankArgs) -> Result<()> {
    let mut conn = connect().await?;
    let scores = conn
        .pagerank(Some(args.damping), Some(args.iterations))
        .await?;

    // Sort by score descending
    let mut sorted: Vec<_> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let limited: Vec<_> = sorted.into_iter().take(args.limit).collect();

    if cli.json {
        let map: std::collections::HashMap<String, f64> = limited
            .iter()
            .map(|(id, score)| (id.to_string(), *score))
            .collect();
        output_json(&map);
    } else if limited.is_empty() {
        println!("No entities in graph.");
    } else {
        // Resolve names for display
        let ids: Vec<String> = limited.iter().map(|(id, _)| id.to_string()).collect();
        let entities = conn.batch_get_entities(&ids).await.unwrap_or_default();

        println!("PageRank (damping={}, iterations={}):", args.damping, args.iterations);
        for (id, score) in &limited {
            let name = entities
                .get(&id.to_string())
                .map_or_else(|| id.to_string(), |e| e.name().to_string());
            println!("  {name}: {score:.6}");
        }
    }
    Ok(())
}

#[derive(Args, Debug)]
pub struct DegreeCentralityArgs {
    /// Maximum results to show.
    #[arg(long, default_value = "20")]
    limit: usize,
}

pub async fn degree_centrality(cli: &Cli, args: &DegreeCentralityArgs) -> Result<()> {
    let mut conn = connect().await?;
    let degrees = conn.degree_centrality().await?;

    // Sort by total degree descending
    let mut sorted: Vec<_> = degrees.into_iter().collect();
    sorted.sort_by(|a, b| (b.1).2.cmp(&(a.1).2));
    let limited: Vec<_> = sorted.into_iter().take(args.limit).collect();

    if cli.json {
        let items: Vec<serde_json::Value> = limited
            .iter()
            .map(|(id, (in_d, out_d, total))| {
                serde_json::json!({
                    "entity_id": id.to_string(),
                    "in_degree": in_d,
                    "out_degree": out_d,
                    "total_degree": total,
                })
            })
            .collect();
        output_json(&items);
    } else if limited.is_empty() {
        println!("No entities in graph.");
    } else {
        let ids: Vec<String> = limited.iter().map(|(id, _)| id.to_string()).collect();
        let entities = conn.batch_get_entities(&ids).await.unwrap_or_default();

        println!("Degree Centrality:");
        println!("  {:30} {:>4} {:>4} {:>5}", "Name", "In", "Out", "Total");
        for (id, (in_d, out_d, total)) in &limited {
            let name = entities
                .get(&id.to_string())
                .map_or_else(|| id.to_string(), |e| e.name().to_string());
            println!("  {name:30} {in_d:>4} {out_d:>4} {total:>5}");
        }
    }
    Ok(())
}
