mod common;

use common::{add_entity, add_task, filament, init_project};
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn no_project_error() {
    let dir = TempDir::new().unwrap();

    filament(&dir)
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a filament project"));
}

#[test]
fn list_json_output() {
    let dir = init_project();

    add_entity(
        &dir,
        "json-entity",
        "module",
        &["--summary", "For JSON test"],
    );

    let output = filament(&dir).args(["--json", "list"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let entities: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["name"], "json-entity");
}

#[test]
fn full_workflow() {
    let dir = init_project();

    // Add entities
    let mod_slug = add_entity(
        &dir,
        "auth-module",
        "module",
        &["--summary", "Authentication system"],
    );

    let task_slug = add_task(
        &dir,
        "implement-login",
        &["--summary", "Build login endpoint", "--priority", "1"],
    );

    // Create relation
    filament(&dir)
        .args(["relate", &task_slug, "depends_on", &mod_slug])
        .assert()
        .success();

    // Context query (from implement-login, which has outgoing depends_on edge to auth-module)
    filament(&dir)
        .args(["context", "--around", &task_slug, "--depth", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-module"));

    // Task ready: implement-login depends_on auth-module (open), so it's blocked
    let output = filament(&dir).args(["task", "ready"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("implement-login"),
        "implement-login should be blocked by depends_on auth-module"
    );

    // Close the dependency, then implement-login should become ready
    filament(&dir)
        .args(["update", &mod_slug, "--status", "closed"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("implement-login"));

    // Close task
    filament(&dir)
        .args(["task", "close", &task_slug])
        .assert()
        .success();

    // Verify closed
    filament(&dir)
        .args(["inspect", &task_slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status:   closed"));
}
