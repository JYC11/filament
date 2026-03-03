use clap::{Args, Subcommand};
use filament_core::error::Result;
use filament_core::models::SendMessageRequest;
use filament_core::store;

use super::helpers::{connect, output_json, truncate_with_ellipsis};
use crate::Cli;

#[derive(Args, Debug)]
pub struct MessageCommand {
    #[command(subcommand)]
    command: MessageSubcommand,
}

impl MessageCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.command {
            MessageSubcommand::Send(args) => send(cli, args).await,
            MessageSubcommand::Inbox(args) => inbox(cli, args).await,
            MessageSubcommand::Read(args) => read(cli, args).await,
        }
    }
}

#[derive(Subcommand, Debug)]
enum MessageSubcommand {
    /// Send a message to an agent.
    Send(MessageSendArgs),
    /// Show inbox for an agent.
    Inbox(MessageInboxArgs),
    /// Mark a message as read.
    Read(MessageReadArgs),
}

#[derive(Args, Debug)]
struct MessageSendArgs {
    /// Sender agent name.
    #[arg(long)]
    from: String,
    /// Recipient agent name.
    #[arg(long)]
    to: String,
    /// Message body.
    #[arg(long)]
    body: String,
    /// Message type (text, question, blocker, artifact).
    #[arg(long, rename_all = "snake_case", default_value = "text")]
    r#type: String,
}

#[derive(Args, Debug)]
struct MessageInboxArgs {
    /// Agent name to check inbox for.
    agent: String,
}

#[derive(Args, Debug)]
struct MessageReadArgs {
    /// Message ID to mark as read.
    id: String,
}

async fn send(cli: &Cli, args: &MessageSendArgs) -> Result<()> {
    let s = connect().await?;

    let req = SendMessageRequest {
        from_agent: args.from.clone(),
        to_agent: args.to.clone(),
        body: args.body.clone(),
        msg_type: Some(args.r#type.clone()),
        in_reply_to: None,
        task_id: None,
    };
    let valid = req.try_into()?;

    let id = s
        .with_transaction(|tx| Box::pin(async move { store::send_message(tx, &valid).await }))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"id": id.as_str()}));
    } else {
        println!("Sent message: {id}");
    }
    Ok(())
}

async fn inbox(cli: &Cli, args: &MessageInboxArgs) -> Result<()> {
    let s = connect().await?;

    let messages = store::get_inbox(s.pool(), &args.agent).await?;

    if cli.json {
        output_json(&messages);
    } else if messages.is_empty() {
        println!("No unread messages for: {}", args.agent);
    } else {
        for m in &messages {
            let preview = truncate_with_ellipsis(m.body.as_str(), 80);
            println!(
                "[{}] from:{} type:{} — {}",
                m.id, m.from_agent, m.msg_type, preview
            );
        }
    }
    Ok(())
}

async fn read(cli: &Cli, args: &MessageReadArgs) -> Result<()> {
    let s = connect().await?;

    let id = args.id.clone();
    s.with_transaction(|tx| Box::pin(async move { store::mark_message_read(tx, &id).await }))
        .await?;

    if cli.json {
        output_json(&serde_json::json!({"read": args.id}));
    } else {
        println!("Marked as read: {}", args.id);
    }
    Ok(())
}
