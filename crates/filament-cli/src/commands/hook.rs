use clap::{Args, Subcommand};
use filament_core::error::{FilamentError, Result};

use super::helpers;
use crate::Cli;

#[derive(Args, Debug)]
pub struct HookCommand {
    #[command(subcommand)]
    action: HookAction,
}

#[derive(Subcommand, Debug)]
pub enum HookAction {
    /// Install the pre-commit reservation check hook.
    Install,
    /// Uninstall the pre-commit reservation check hook.
    Uninstall,
    /// Run the pre-commit reservation check (called by git).
    Check(CheckArgs),
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Agent name to exclude from conflict checks (your own reservations).
    #[arg(long)]
    pub agent: Option<String>,
}

impl HookCommand {
    pub async fn run(&self, cli: &Cli) -> Result<()> {
        match &self.action {
            HookAction::Install => install(cli),
            HookAction::Uninstall => uninstall(cli),
            HookAction::Check(args) => check(cli, args).await,
        }
    }
}

const HOOK_MARKER: &str = "# filament-reservation-check";

fn install(cli: &Cli) -> Result<()> {
    let root = helpers::find_project_root()?;
    let hooks_dir = root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Err(FilamentError::Validation(
            "not a git repository (no .git/hooks/)".to_string(),
        ));
    }

    let hook_path = hooks_dir.join("pre-commit");
    let filament_bin = std::env::current_exe()
        .map_err(FilamentError::Io)?
        .to_string_lossy()
        .to_string();

    let hook_snippet = format!(
        r#"
{HOOK_MARKER}
{filament_bin} hook check
FILAMENT_HOOK_EXIT=$?
if [ $FILAMENT_HOOK_EXIT -ne 0 ]; then
    echo "filament: commit blocked by file reservation conflicts"
    exit $FILAMENT_HOOK_EXIT
fi
{HOOK_MARKER}-end
"#
    );

    if hook_path.exists() {
        let existing = std::fs::read_to_string(&hook_path).map_err(FilamentError::Io)?;
        if existing.contains(HOOK_MARKER) {
            if !cli.quiet {
                println!("pre-commit hook already installed");
            }
            return Ok(());
        }
        // Append to existing hook
        let updated = format!("{existing}\n{hook_snippet}");
        std::fs::write(&hook_path, updated).map_err(FilamentError::Io)?;
    } else {
        let content = format!("#!/bin/sh\n{hook_snippet}");
        std::fs::write(&hook_path, content).map_err(FilamentError::Io)?;
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).map_err(FilamentError::Io)?;
    }

    if cli.json {
        helpers::output_json(&serde_json::json!({ "status": "installed" }));
    } else {
        println!("pre-commit reservation check hook installed");
    }
    Ok(())
}

fn uninstall(cli: &Cli) -> Result<()> {
    let root = helpers::find_project_root()?;
    let hook_path = root.join(".git").join("hooks").join("pre-commit");

    if !hook_path.exists() {
        if !cli.quiet {
            println!("no pre-commit hook found");
        }
        return Ok(());
    }

    let content = std::fs::read_to_string(&hook_path).map_err(FilamentError::Io)?;
    if !content.contains(HOOK_MARKER) {
        if !cli.quiet {
            println!("pre-commit hook does not contain filament check");
        }
        return Ok(());
    }

    // Remove our section
    let mut lines: Vec<&str> = content.lines().collect();
    let start = lines.iter().position(|l| l.contains(HOOK_MARKER));
    let end = lines
        .iter()
        .position(|l| l.contains(&format!("{HOOK_MARKER}-end")));

    if let (Some(s), Some(e)) = (start, end) {
        lines.drain(s..=e);
    }

    let remaining = lines.join("\n");
    if remaining.trim() == "#!/bin/sh" || remaining.trim().is_empty() {
        std::fs::remove_file(&hook_path).map_err(FilamentError::Io)?;
    } else {
        std::fs::write(&hook_path, remaining).map_err(FilamentError::Io)?;
    }

    if cli.json {
        helpers::output_json(&serde_json::json!({ "status": "uninstalled" }));
    } else {
        println!("pre-commit reservation check hook removed");
    }
    Ok(())
}

async fn check(cli: &Cli, args: &CheckArgs) -> Result<()> {
    let mut conn = helpers::connect().await?;

    // Get list of staged files from git
    let output = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .output()
        .map_err(FilamentError::Io)?;

    if !output.status.success() {
        return Err(FilamentError::Validation(
            "failed to get staged files from git".to_string(),
        ));
    }

    let staged_files: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|l| !l.is_empty())
        .collect();

    if staged_files.is_empty() {
        return Ok(());
    }

    // Get all active reservations
    let reservations = conn.list_reservations(None).await?;
    let my_agent = args.agent.as_deref().unwrap_or("");

    let mut conflicts: Vec<(&str, String, String)> = Vec::new();

    for reservation in &reservations {
        // Skip own reservations
        if !my_agent.is_empty() && reservation.agent_name.as_str() == my_agent {
            continue;
        }
        // Only warn about exclusive reservations from other agents
        if !reservation.mode.is_exclusive() {
            continue;
        }

        // Check if any staged file matches this reservation's glob
        for file in &staged_files {
            if glob_matches(reservation.file_glob.as_str(), file) {
                conflicts.push((
                    *file,
                    reservation.agent_name.as_str().to_string(),
                    reservation.file_glob.as_str().to_string(),
                ));
            }
        }
    }

    if conflicts.is_empty() {
        return Ok(());
    }

    if cli.json {
        let items: Vec<serde_json::Value> = conflicts
            .iter()
            .map(|(file, agent, glob)| {
                serde_json::json!({
                    "file": file,
                    "agent": agent,
                    "glob": glob,
                })
            })
            .collect();
        helpers::output_json(&serde_json::json!({ "conflicts": items }));
    } else {
        eprintln!("filament: file reservation conflicts detected:");
        for (file, agent, glob) in &conflicts {
            eprintln!("  {file} — reserved by {agent} (glob: {glob})");
        }
    }

    Err(FilamentError::Validation(format!(
        "{} file(s) conflict with exclusive reservations",
        conflicts.len()
    )))
}

/// Simple glob matching: supports `*` (any chars) and `?` (single char).
fn glob_matches(pattern: &str, path: &str) -> bool {
    // Handle common patterns: exact match, `*.ext`, `dir/*`
    if pattern == path {
        return true;
    }

    let pattern_parts: Vec<&str> = pattern.split('*').collect();
    if pattern_parts.len() == 1 {
        // No wildcards — exact match only
        return pattern == path;
    }

    // Simple `*` glob matching
    let mut remaining = path;
    for (i, part) in pattern_parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // Must start with this prefix
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == pattern_parts.len() - 1 {
            // Must end with this suffix
            if !remaining.ends_with(part) {
                return false;
            }
            remaining = "";
        } else {
            // Must contain this part somewhere
            match remaining.find(part) {
                Some(pos) => remaining = &remaining[pos + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_exact_match() {
        assert!(glob_matches("foo.rs", "foo.rs"));
        assert!(!glob_matches("foo.rs", "bar.rs"));
    }

    #[test]
    fn glob_star_extension() {
        assert!(glob_matches("*.rs", "foo.rs"));
        assert!(glob_matches("*.rs", "src/bar.rs"));
        assert!(!glob_matches("*.rs", "foo.py"));
    }

    #[test]
    fn glob_prefix_star() {
        assert!(glob_matches("src/*", "src/foo.rs"));
        assert!(glob_matches("src/*", "src/bar/baz.rs"));
        assert!(!glob_matches("src/*", "lib/foo.rs"));
    }

    #[test]
    fn glob_middle_star() {
        assert!(glob_matches("src/*.rs", "src/foo.rs"));
        assert!(!glob_matches("src/*.rs", "src/foo.py"));
    }
}
