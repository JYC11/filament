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

const HOOK_MARKER: &str = "# fl-reservation-check";

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
    echo "fl: commit blocked by file reservation conflicts"
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
            println!("pre-commit hook does not contain fl check");
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
        eprintln!("fl: file reservation conflicts detected:");
        for (file, agent, glob) in &conflicts {
            eprintln!("  {file} — reserved by {agent} (glob: {glob})");
        }
    }

    Err(FilamentError::Validation(format!(
        "{} file(s) conflict with exclusive reservations",
        conflicts.len()
    )))
}

/// Simple glob matching: supports `*` (any non-`/` chars) and `**` (any chars including `/`).
fn glob_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }

    // Normalize `**` to a sentinel, then split on `*`
    // Strategy: convert pattern to a regex-like check
    // `**` matches anything (including `/`), `*` matches anything except `/`
    glob_match_recursive(pattern, path)
}

fn glob_match_recursive(pattern: &str, path: &str) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern == "**" || pattern == "**/" {
        return true;
    }

    if let Some(rest) = pattern.strip_prefix("**/") {
        // `**/X` — match X at any directory depth
        if glob_match_recursive(rest, path) {
            return true;
        }
        // Try skipping one path segment at a time
        for (i, c) in path.char_indices() {
            if c == '/' && glob_match_recursive(rest, &path[i + 1..]) {
                return true;
            }
        }
        return false;
    }

    if let Some(rest) = pattern.strip_prefix('*') {
        // `*` — match any non-`/` chars
        for i in 0..=path.len() {
            if i > 0 && path.as_bytes()[i - 1] == b'/' {
                break;
            }
            if glob_match_recursive(rest, &path[i..]) {
                return true;
            }
        }
        return false;
    }

    // Literal character match
    let mut pattern_chars = pattern.chars();
    let mut path_chars = path.chars();
    if let (Some(pc), Some(tc)) = (pattern_chars.next(), path_chars.next()) {
        if pc == tc {
            return glob_match_recursive(pattern_chars.as_str(), path_chars.as_str());
        }
    }
    false
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
        assert!(!glob_matches("*.rs", "foo.py"));
        // Single `*` does NOT match across `/`
        assert!(!glob_matches("*.rs", "src/bar.rs"));
    }

    #[test]
    fn glob_prefix_star() {
        assert!(glob_matches("src/*", "src/foo.rs"));
        // Single `*` does NOT match across `/`
        assert!(!glob_matches("src/*", "src/bar/baz.rs"));
        assert!(!glob_matches("src/*", "lib/foo.rs"));
    }

    #[test]
    fn glob_middle_star() {
        assert!(glob_matches("src/*.rs", "src/foo.rs"));
        assert!(!glob_matches("src/*.rs", "src/foo.py"));
        assert!(!glob_matches("src/*.rs", "src/sub/foo.rs"));
    }

    #[test]
    fn glob_double_star() {
        // `**` matches across directory boundaries
        assert!(glob_matches("**/*.rs", "foo.rs"));
        assert!(glob_matches("**/*.rs", "src/foo.rs"));
        assert!(glob_matches("**/*.rs", "src/sub/foo.rs"));
        assert!(!glob_matches("**/*.rs", "src/foo.py"));

        assert!(glob_matches("src/**/*.rs", "src/foo.rs"));
        assert!(glob_matches("src/**/*.rs", "src/sub/foo.rs"));
        assert!(glob_matches("src/**/*.rs", "src/a/b/c.rs"));
        assert!(!glob_matches("src/**/*.rs", "lib/foo.rs"));
    }

    #[test]
    fn glob_empty_inputs() {
        assert!(glob_matches("", ""));
        assert!(!glob_matches("", "foo"));
        assert!(!glob_matches("foo", ""));
    }

    #[test]
    fn glob_double_star_at_end() {
        // `src/**` matches everything under src/
        assert!(glob_matches("src/**", "src/foo.rs"));
        assert!(glob_matches("src/**", "src/a/b/c.rs"));
        assert!(!glob_matches("src/**", "lib/foo.rs"));
    }

    #[test]
    fn glob_bare_double_star() {
        // `**` alone matches anything
        assert!(glob_matches("**", "foo.rs"));
        assert!(glob_matches("**", "src/bar/baz.rs"));
        assert!(glob_matches("**", ""));
    }

    #[test]
    fn glob_multiple_double_stars() {
        assert!(glob_matches("**/src/**/*.rs", "src/foo.rs"));
        assert!(glob_matches("**/src/**/*.rs", "a/src/bar.rs"));
        assert!(glob_matches("**/src/**/*.rs", "a/b/src/c/d.rs"));
        assert!(!glob_matches("**/src/**/*.rs", "a/lib/foo.rs"));
    }

    #[test]
    fn glob_dot_files() {
        assert!(glob_matches("**/.*", ".gitignore"));
        assert!(glob_matches("**/.*", "src/.hidden"));
        assert!(glob_matches("**/.hidden/*.rs", ".hidden/foo.rs"));
    }

    #[test]
    fn glob_no_wildcards_mismatch_length() {
        assert!(!glob_matches("src/foo.rs", "src/foo.rsx"));
        assert!(!glob_matches("src/foo.rsx", "src/foo.rs"));
    }

    #[test]
    fn glob_star_matches_empty() {
        // `*` can match zero characters
        assert!(glob_matches("src/*.rs", "src/.rs"));
        assert!(glob_matches("*", ""));
    }
}
