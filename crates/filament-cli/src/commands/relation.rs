use clap::Args;
use filament_core::dto::CreateRelationRequest;
use filament_core::error::Result;
use filament_core::models::RelationType;

use super::helpers::{connect, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct RelateArgs {
    /// Source entity slug or ID.
    source: String,
    /// Relation type (blocks, `depends_on`, produces, owns, `relates_to`, `assigned_to`).
    relation_type: RelationType,
    /// Target entity slug or ID.
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
    /// Source entity slug or ID.
    source: String,
    /// Relation type.
    relation_type: RelationType,
    /// Target entity slug or ID.
    target: String,
}

pub async fn relate(cli: &Cli, args: &RelateArgs) -> Result<()> {
    let mut conn = connect().await?;

    let source_id = conn.resolve_entity(&args.source).await?.id().clone();
    let target_id = conn.resolve_entity(&args.target).await?.id().clone();

    let req = CreateRelationRequest {
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        relation_type: args.relation_type.clone(),
        weight: args.weight,
        summary: Some(args.summary.clone()),
        metadata: None,
    };

    let id = conn.create_relation(req).await?;

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
    let mut conn = connect().await?;

    let source_id = conn.resolve_entity(&args.source).await?.id().clone();
    let target_id = conn.resolve_entity(&args.target).await?.id().clone();

    conn.delete_relation(
        source_id.as_str(),
        target_id.as_str(),
        args.relation_type.as_str(),
    )
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
