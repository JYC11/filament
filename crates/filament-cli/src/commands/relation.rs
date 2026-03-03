use clap::Args;
use filament_core::error::Result;
use filament_core::models::CreateRelationRequest;
use filament_core::store;

use super::helpers::{connect, output_json, resolve_entity_id};
use crate::Cli;

#[derive(Args, Debug)]
pub struct RelateArgs {
    /// Source entity name.
    source: String,
    #[allow(clippy::doc_markdown)]
    /// Relation type (blocks, depends_on, produces, owns, relates_to, assigned_to).
    relation_type: String,
    /// Target entity name.
    target: String,
    /// Optional summary.
    #[arg(long, default_value = "")]
    summary: String,
    /// Relation weight.
    #[arg(long)]
    weight: Option<f64>,
}

#[derive(Args, Debug)]
pub struct UnrelateArgs {
    /// Source entity name.
    source: String,
    /// Relation type.
    relation_type: String,
    /// Target entity name.
    target: String,
}

pub async fn relate(cli: &Cli, args: &RelateArgs) -> Result<()> {
    let s = connect().await?;

    let source_id = resolve_entity_id(&s, &args.source).await?;
    let target_id = resolve_entity_id(&s, &args.target).await?;

    let req = CreateRelationRequest {
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        relation_type: args.relation_type.clone(),
        weight: args.weight,
        summary: Some(args.summary.clone()),
        metadata: None,
    };
    let valid = req.try_into()?;

    let id = s
        .with_transaction(|tx| Box::pin(async move { store::create_relation(tx, &valid).await }))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!(
            "Created relation: {} {} {} ({})",
            args.source, args.relation_type, args.target, id
        );
    }
    Ok(())
}

pub async fn unrelate(cli: &Cli, args: &UnrelateArgs) -> Result<()> {
    let s = connect().await?;

    let source_id = resolve_entity_id(&s, &args.source).await?;
    let target_id = resolve_entity_id(&s, &args.target).await?;

    let rel_type = args.relation_type.clone();
    let src = source_id.to_string();
    let tgt = target_id.to_string();
    s.with_transaction(|tx| {
        Box::pin(
            async move { store::delete_relation_by_endpoints(tx, &src, &tgt, &rel_type).await },
        )
    })
    .await?;

    if cli.json {
        output_json(&serde_json::json!({"deleted": true}));
    } else {
        println!(
            "Removed relation: {} {} {}",
            args.source, args.relation_type, args.target
        );
    }
    Ok(())
}
