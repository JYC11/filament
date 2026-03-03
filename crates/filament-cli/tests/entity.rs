mod common;

use common::{filament, init_project};
use predicates::prelude::*;
use std::io::Write;

#[test]
fn entity_add_and_list() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "auth-module",
            "--type",
            "module",
            "--summary",
            "Authentication system",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created entity:"));

    filament(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-module"));
}

#[test]
fn entity_add_json() {
    let dir = init_project();

    filament(&dir)
        .args([
            "--json",
            "add",
            "test-entity",
            "--type",
            "module",
            "--summary",
            "test",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""id":"#));
}

#[test]
fn entity_inspect() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "my-service",
            "--type",
            "service",
            "--summary",
            "Main API service",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "my-service"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Name:     my-service")
                .and(predicate::str::contains("Type:     service"))
                .and(predicate::str::contains("Summary:  Main API service")),
        );
}

#[test]
fn entity_update_summary_and_status() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "updatable",
            "--type",
            "module",
            "--summary",
            "original",
        ])
        .assert()
        .success();

    filament(&dir)
        .args([
            "update",
            "updatable",
            "--summary",
            "updated summary",
            "--status",
            "in_progress",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "updatable"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Summary:  updated summary")
                .and(predicate::str::contains("Status:   in_progress")),
        );
}

#[test]
fn entity_update_summary_only() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "sum-only", "--type", "module", "--summary", "old"])
        .assert()
        .success();

    filament(&dir)
        .args(["update", "sum-only", "--summary", "new summary"])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "sum-only"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Summary:  new summary")
                .and(predicate::str::contains("Status:   open")),
        );
}

#[test]
fn entity_update_status_only() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "stat-only",
            "--type",
            "module",
            "--summary",
            "keep me",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["update", "stat-only", "--status", "blocked"])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "stat-only"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Summary:  keep me")
                .and(predicate::str::contains("Status:   blocked")),
        );
}

#[test]
fn entity_update_invalid_status() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "bad-stat", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["update", "bad-stat", "--status", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid status"));
}

#[test]
fn entity_remove() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "to-remove", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["remove", "to-remove"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed entity: to-remove"));

    filament(&dir)
        .args(["inspect", "to-remove"])
        .assert()
        .failure();
}

#[test]
fn entity_not_found_error() {
    let dir = init_project();

    filament(&dir)
        .args(["inspect", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Entity not found"));
}

#[test]
fn entity_not_found_json_error() {
    let dir = init_project();

    filament(&dir)
        .args(["--json", "inspect", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""code": "ENTITY_NOT_FOUND""#));
}

#[test]
fn entity_list_with_filters() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "mod-a", "--type", "module", "--summary", "Module A"])
        .assert()
        .success();

    filament(&dir)
        .args(["add", "task-a", "--type", "task", "--summary", "Task A"])
        .assert()
        .success();

    // Filter by type
    filament(&dir)
        .args(["list", "--type", "task"])
        .assert()
        .success()
        .stdout(predicate::str::contains("task-a").and(predicate::str::contains("mod-a").not()));

    // Filter by status
    filament(&dir)
        .args(["list", "--status", "closed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No entities found."));
}

#[test]
fn entity_with_priority() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "urgent",
            "--type",
            "task",
            "--summary",
            "Urgent task",
            "--priority",
            "0",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "urgent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Priority: 0"));
}

#[test]
fn entity_add_with_facts() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "factual",
            "--type",
            "module",
            "--summary",
            "Has facts",
            "--facts",
            r#"{"lang": "rust", "version": "1.75"}"#,
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", "factual"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Facts:")
                .and(predicate::str::contains("rust"))
                .and(predicate::str::contains("1.75")),
        );
}

#[test]
fn entity_read_with_content_file() {
    let dir = init_project();

    // Create a content file
    let content_path = dir.path().join("readme.txt");
    let mut f = std::fs::File::create(&content_path).unwrap();
    writeln!(f, "This is the full content.").unwrap();

    filament(&dir)
        .args([
            "add",
            "readable",
            "--type",
            "doc",
            "--summary",
            "Has content",
            "--content",
            content_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["read", "readable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("This is the full content."));
}

#[test]
fn entity_read_no_content() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "no-content", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["read", "no-content"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No content file"));
}

#[test]
fn entity_duplicate_name_allowed() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "dup-name", "--type", "module", "--summary", "first"])
        .assert()
        .success();

    // Names are not UNIQUE at DB level — second add succeeds
    filament(&dir)
        .args(["add", "dup-name", "--type", "module", "--summary", "second"])
        .assert()
        .success();

    // resolve_entity returns the first match by name
    filament(&dir)
        .args(["inspect", "dup-name"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dup-name"));
}

#[test]
fn entity_update_no_args_error() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "no-change", "--type", "module", "--summary", "test"])
        .assert()
        .success();

    filament(&dir)
        .args(["update", "no-change"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--summary or --status"));
}

#[test]
fn invalid_entity_type_error() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "bad",
            "--type",
            "nonexistent",
            "--summary",
            "bad type",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid entity type"));
}
