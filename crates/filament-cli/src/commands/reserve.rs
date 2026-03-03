use clap::Args;
use filament_core::error::{FilamentError, Result};
use filament_core::models::TtlSeconds;
use filament_core::store;

use super::helpers::{connect, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct ReserveArgs {
    /// File glob pattern to reserve.
    glob: String,
    /// Agent name.
    #[arg(long)]
    agent: String,
    /// Exclusive reservation.
    #[arg(long)]
    exclusive: bool,
    /// TTL in seconds.
    #[arg(long, default_value = "3600")]
    ttl: u32,
}

#[derive(Args, Debug)]
pub struct ReleaseArgs {
    /// File glob pattern to release.
    glob: String,
    /// Agent name.
    #[arg(long)]
    agent: String,
}

#[derive(Args, Debug)]
pub struct ReservationsArgs {
    /// Filter by agent name.
    #[arg(long)]
    agent: Option<String>,
    /// Clean up expired reservations.
    #[arg(long)]
    clean: bool,
}

pub async fn reserve(cli: &Cli, args: &ReserveArgs) -> Result<()> {
    let s = connect().await?;
    let ttl = TtlSeconds::new(args.ttl)?;

    let agent = args.agent.clone();
    let glob = args.glob.clone();
    let exclusive = args.exclusive;
    let id = s
        .with_transaction(|tx| {
            Box::pin(
                async move { store::acquire_reservation(tx, &agent, &glob, exclusive, ttl).await },
            )
        })
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!("Reserved: {} for {} ({})", args.glob, args.agent, id);
    }
    Ok(())
}

pub async fn release(cli: &Cli, args: &ReleaseArgs) -> Result<()> {
    let s = connect().await?;

    let reservation = store::find_reservation(s.pool(), &args.glob, &args.agent).await?;
    let Some(reservation) = reservation else {
        return Err(FilamentError::Validation(format!(
            "no active reservation found for glob '{}' by agent '{}'",
            args.glob, args.agent
        )));
    };

    let id = reservation.id.to_string();
    s.with_transaction(|tx| Box::pin(async move { store::release_reservation(tx, &id).await }))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"released": true}));
    } else {
        println!("Released: {} for {}", args.glob, args.agent);
    }
    Ok(())
}

pub async fn reservations(cli: &Cli, args: &ReservationsArgs) -> Result<()> {
    let s = connect().await?;

    if args.clean {
        let cleaned = s
            .with_transaction(|tx| {
                Box::pin(async move { store::expire_stale_reservations(tx).await })
            })
            .await?;
        if !cli.json && cleaned > 0 {
            println!("Cleaned up {cleaned} expired reservation(s).");
        }
    }

    let reservations = store::list_reservations(s.pool(), args.agent.as_deref()).await?;

    if cli.json {
        output_json(&reservations);
    } else if reservations.is_empty() {
        println!("No active reservations.");
    } else {
        for r in &reservations {
            let excl = if r.exclusive { " [exclusive]" } else { "" };
            println!(
                "{} — {} by {}{} (expires {})",
                r.id, r.file_glob, r.agent_name, excl, r.expires_at
            );
        }
    }
    Ok(())
}
