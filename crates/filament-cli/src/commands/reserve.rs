use clap::Args;
use filament_core::error::{FilamentError, Result};
use filament_core::models::TtlSeconds;

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
    let mut conn = connect().await?;
    let ttl = TtlSeconds::new(args.ttl)?;

    let id = conn
        .acquire_reservation(&args.agent, &args.glob, args.exclusive, ttl)
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!("Reserved: {} for {} ({})", args.glob, args.agent, id);
    }
    Ok(())
}

pub async fn release(cli: &Cli, args: &ReleaseArgs) -> Result<()> {
    let mut conn = connect().await?;

    let reservation = conn.find_reservation(&args.glob, &args.agent).await?;
    let Some(reservation) = reservation else {
        return Err(FilamentError::Validation(format!(
            "no active reservation found for glob '{}' by agent '{}'",
            args.glob, args.agent
        )));
    };

    conn.release_reservation(reservation.id.as_str()).await?;

    if cli.json {
        output_json(&serde_json::json!({"released": true}));
    } else {
        println!("Released: {} for {}", args.glob, args.agent);
    }
    Ok(())
}

pub async fn reservations(cli: &Cli, args: &ReservationsArgs) -> Result<()> {
    let mut conn = connect().await?;

    let mut cleaned_count = None;
    if args.clean {
        let cleaned = conn.expire_stale_reservations().await?;
        cleaned_count = Some(cleaned);
        if !cli.json && cleaned > 0 {
            println!("Cleaned up {cleaned} expired reservation(s).");
        }
    }

    let reservations = conn.list_reservations(args.agent.as_deref()).await?;

    if cli.json {
        if let Some(cleaned) = cleaned_count {
            output_json(&serde_json::json!({
                "cleaned": cleaned,
                "reservations": reservations,
            }));
        } else {
            output_json(&reservations);
        }
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
