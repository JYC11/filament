mod common;

use predicates::prelude::*;
use tempfile::TempDir;

fn init_git_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    // git init
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    // Configure git user for commits
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    // fl init
    common::filament(&dir).arg("init").assert().success();
    // Gitignore .fl/ so DB changes don't dirty the tree
    std::fs::write(dir.path().join(".gitignore"), ".fl/\n").unwrap();
    // Initial commit so HEAD exists
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    dir
}

#[test]
fn audit_creates_snapshot_on_branch() {
    let dir = init_git_project();

    // Add an entity so the graph is non-empty
    common::filament(&dir)
        .args([
            "add",
            "test-module",
            "--type",
            "module",
            "--summary",
            "a test module",
        ])
        .assert()
        .success();

    // Run audit
    common::filament(&dir)
        .arg("audit")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Audit snapshot committed to branch 'filament-audit'",
        ));

    // Verify the branch exists
    let branch_output = std::process::Command::new("git")
        .args(["branch", "--list", "filament-audit"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let branch_list = String::from_utf8_lossy(&branch_output.stdout);
    assert!(
        branch_list.contains("filament-audit"),
        "filament-audit branch should exist, got: {branch_list}"
    );

    // Verify we're back on the original branch (main/master)
    let head_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let current = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_string();
    assert!(
        current == "main" || current == "master",
        "should be back on original branch, got: {current}"
    );
}

#[test]
fn audit_json_output() {
    let dir = init_git_project();

    // Run audit with --json
    common::filament(&dir)
        .args(["--json", "audit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"committed\""))
        .stdout(predicate::str::contains("\"branch\""))
        .stdout(predicate::str::contains("\"entities\""));
}

#[test]
fn audit_rejects_dirty_tree() {
    let dir = init_git_project();

    // Create an untracked file and stage it to dirty the tree
    std::fs::write(dir.path().join("dirty.txt"), "dirty").unwrap();
    std::process::Command::new("git")
        .args(["add", "dirty.txt"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Audit should fail because of uncommitted changes
    common::filament(&dir)
        .arg("audit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));
}

#[test]
fn audit_custom_branch() {
    let dir = init_git_project();

    // Run audit with custom branch name
    common::filament(&dir)
        .args(["audit", "--branch", "my-audit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-audit"));
}
