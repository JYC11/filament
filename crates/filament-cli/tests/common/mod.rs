use assert_cmd::cargo_bin_cmd;
use assert_cmd::Command;
use tempfile::TempDir;

/// Create a command that runs `fl` in a temp directory.
/// Auto-start is disabled by default in tests to avoid stray daemon processes.
pub fn filament(dir: &TempDir) -> Command {
    let mut cmd = cargo_bin_cmd!("fl");
    cmd.current_dir(dir.path());
    cmd.env("FILAMENT_NO_AUTO_START", "1");
    cmd
}

/// Initialize a fl project and return the temp dir.
#[allow(dead_code)]
pub fn init_project() -> TempDir {
    use predicates::prelude::*;
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized fl project"));
    dir
}

/// Run `fl add` and return the generated slug.
/// Parses the slug from output format: "Created entity: {slug} ({id})"
#[allow(dead_code)]
pub fn add_entity(dir: &TempDir, name: &str, entity_type: &str, extra_args: &[&str]) -> String {
    let mut cmd = filament(dir);
    cmd.args(["add", name, "--type", entity_type]);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    extract_slug_from_output(&stdout)
}

/// Run `fl task add` and return the generated slug.
/// Parses the slug from output format: "Created task: {slug} ({id})"
#[allow(dead_code)]
pub fn add_task(dir: &TempDir, title: &str, extra_args: &[&str]) -> String {
    let mut cmd = filament(dir);
    cmd.args(["task", "add", title]);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "task add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    extract_slug_from_output(&stdout)
}

/// Extract slug from CLI output.
/// Matches "Created entity: {slug} ({id})" or "Created task: {slug} ({id})"
#[allow(dead_code)]
fn extract_slug_from_output(output: &str) -> String {
    // Find the line with "Created" and extract the slug (first word after ": ")
    for line in output.lines() {
        if let Some(rest) = line
            .strip_prefix("Created entity: ")
            .or_else(|| line.strip_prefix("Created task: "))
        {
            if let Some(slug) = rest.split_whitespace().next() {
                return slug.to_string();
            }
        }
    }
    panic!("Could not extract slug from output: {output}");
}
