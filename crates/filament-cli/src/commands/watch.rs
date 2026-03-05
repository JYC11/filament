use clap::Args;
use filament_core::client::DaemonClient;
use filament_core::error::{FilamentError, Result};
use filament_core::protocol::SubscribeParams;

use super::helpers;
use crate::Cli;

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Filter by event types (comma-separated). Empty = all events.
    #[arg(long, value_delimiter = ',')]
    pub events: Vec<String>,
}

pub async fn watch(cli: &Cli, args: &WatchArgs) -> Result<()> {
    let root = helpers::find_project_root()?;
    let socket_path = root.join(".filament").join("filament.sock");
    if !socket_path.exists() {
        return Err(FilamentError::Validation(
            "daemon not running. Start it with `filament serve`.".to_string(),
        ));
    }

    let mut client = DaemonClient::connect(&socket_path).await?;
    let params = SubscribeParams {
        event_types: args.events.clone(),
    };
    let mut stream = client.subscribe(params).await?;

    if !cli.quiet {
        eprintln!("watching for changes (Ctrl+C to stop)...");
    }

    while let Some(notification) = stream.next().await? {
        if cli.json {
            helpers::output_json(&notification);
        } else {
            let entity = notification.entity_id.as_deref().unwrap_or("-");
            let detail = notification
                .detail
                .map_or_else(String::new, |v| format!(" {v}"));
            println!("[{}] entity={entity}{detail}", notification.event_type);
        }
    }

    Ok(())
}
