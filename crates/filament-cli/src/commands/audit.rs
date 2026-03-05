use clap::Args;
use filament_core::error::{FilamentError, Result};

use super::helpers::{connect, find_project_root, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct AuditArgs {
    /// Git branch to commit audit snapshots to.
    #[arg(long, default_value = "filament-audit")]
    branch: String,
    /// Custom commit message.
    #[arg(long)]
    message: Option<String>,
}

pub async fn audit(cli: &Cli, args: &AuditArgs) -> Result<()> {
    let root = find_project_root()?;
    let mut conn = connect().await?;

    // Export current state
    let data = conn.export_all(true).await?;
    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| FilamentError::Protocol(e.to_string()))?;

    // Write export to a file in the project root
    let audit_file = root.join(".filament").join("audit-snapshot.json");
    std::fs::write(&audit_file, &json).map_err(FilamentError::Io)?;

    // Check if we're in a git repo
    let git_status = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(&root)
        .output()
        .map_err(FilamentError::Io)?;

    if !git_status.status.success() {
        return Err(FilamentError::Validation(
            "not a git repository".to_string(),
        ));
    }

    // Get current branch to restore later
    let current_branch = git_current_branch(&root)?;

    // Check if audit branch exists
    let branch_exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &args.branch])
        .current_dir(&root)
        .output()
        .map_err(FilamentError::Io)?
        .status
        .success();

    if branch_exists {
        run_git(&root, &["checkout", &args.branch])?;
    } else {
        // Create orphan branch
        run_git(&root, &["checkout", "--orphan", &args.branch])?;
        run_git(&root, &["rm", "-rf", "--cached", "."])?;
    }

    // Stage the audit file
    run_git(
        &root,
        &[
            "add",
            "-f",
            ".filament/audit-snapshot.json",
        ],
    )?;

    // Commit
    let entity_count = data.entities.len();
    let relation_count = data.relations.len();
    let msg = args.message.clone().unwrap_or_else(|| {
        format!(
            "audit: {entity_count} entities, {relation_count} relations"
        )
    });
    run_git(&root, &["commit", "-m", &msg, "--allow-empty"])?;

    // Switch back to original branch
    run_git(&root, &["checkout", &current_branch])?;

    if cli.json {
        output_json(&serde_json::json!({
            "status": "committed",
            "branch": args.branch,
            "entities": entity_count,
            "relations": relation_count,
        }));
    } else {
        println!(
            "Audit snapshot committed to branch '{}' ({entity_count} entities, {relation_count} relations)",
            args.branch
        );
    }

    Ok(())
}

fn git_current_branch(root: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(FilamentError::Io)?;

    if !output.status.success() {
        return Err(FilamentError::Validation(
            "failed to determine current git branch".to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(root: &std::path::Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(FilamentError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(FilamentError::Validation(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            stderr.trim()
        )));
    }

    Ok(())
}
