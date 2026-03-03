mod common;

use common::{add_entity, filament, init_project};
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
        .stdout(predicate::str::contains(r#""id":"#).and(predicate::str::contains(r#""slug":"#)));
}

#[test]
fn entity_inspect() {
    let dir = init_project();

    let slug = add_entity(
        &dir,
        "my-service",
        "service",
        &["--summary", "Main API service"],
    );

    filament(&dir)
        .args(["inspect", &slug])
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

    let slug = add_entity(&dir, "updatable", "module", &["--summary", "original"]);

    filament(&dir)
        .args([
            "update",
            &slug,
            "--summary",
            "updated summary",
            "--status",
            "in_progress",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", &slug])
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

    let slug = add_entity(&dir, "sum-only", "module", &["--summary", "old"]);

    filament(&dir)
        .args(["update", &slug, "--summary", "new summary"])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", &slug])
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

    let slug = add_entity(&dir, "stat-only", "module", &["--summary", "keep me"]);

    filament(&dir)
        .args(["update", &slug, "--status", "blocked"])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", &slug])
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

    let slug = add_entity(&dir, "bad-stat", "module", &[]);

    filament(&dir)
        .args(["update", &slug, "--status", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid status"));
}

#[test]
fn entity_remove() {
    let dir = init_project();

    let slug = add_entity(&dir, "to-remove", "module", &[]);

    filament(&dir)
        .args(["remove", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed entity:"));

    filament(&dir).args(["inspect", &slug]).assert().failure();
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

    add_entity(&dir, "mod-a", "module", &["--summary", "Module A"]);
    add_entity(&dir, "task-a", "task", &["--summary", "Task A"]);

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

    let slug = add_entity(
        &dir,
        "urgent",
        "task",
        &["--summary", "Urgent task", "--priority", "0"],
    );

    filament(&dir)
        .args(["inspect", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Priority: 0"));
}

#[test]
fn entity_add_with_facts() {
    let dir = init_project();

    let slug = add_entity(
        &dir,
        "factual",
        "module",
        &[
            "--summary",
            "Has facts",
            "--facts",
            r#"{"lang": "rust", "version": "1.75"}"#,
        ],
    );

    filament(&dir)
        .args(["inspect", &slug])
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

    let slug = add_entity(
        &dir,
        "readable",
        "doc",
        &[
            "--summary",
            "Has content",
            "--content",
            content_path.to_str().unwrap(),
        ],
    );

    filament(&dir)
        .args(["read", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("This is the full content."));
}

#[test]
fn entity_read_no_content() {
    let dir = init_project();

    let slug = add_entity(&dir, "no-content", "module", &[]);

    filament(&dir)
        .args(["read", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("No content file"));
}

#[test]
fn entity_duplicate_name_allowed() {
    let dir = init_project();

    let slug1 = add_entity(&dir, "dup-name", "module", &["--summary", "first"]);

    // Names are not UNIQUE at DB level — second add succeeds with different slug
    let slug2 = add_entity(&dir, "dup-name", "module", &["--summary", "second"]);

    assert_ne!(slug1, slug2, "slugs should be unique even with same name");

    // Both are inspectable by their own slug
    filament(&dir)
        .args(["inspect", &slug1])
        .assert()
        .success()
        .stdout(predicate::str::contains("dup-name"));

    filament(&dir)
        .args(["inspect", &slug2])
        .assert()
        .success()
        .stdout(predicate::str::contains("dup-name"));
}

#[test]
fn entity_update_no_args_error() {
    let dir = init_project();

    let slug = add_entity(&dir, "no-change", "module", &["--summary", "test"]);

    filament(&dir)
        .args(["update", &slug])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--summary or --status"));
}

#[test]
fn entity_inspect_shows_relations() {
    let dir = init_project();

    let slug_a = add_entity(&dir, "svc-a", "service", &["--summary", "Service A"]);
    let slug_b = add_entity(&dir, "mod-b", "module", &["--summary", "Module B"]);

    filament(&dir)
        .args(["relate", &slug_a, "depends_on", &slug_b])
        .assert()
        .success();

    filament(&dir)
        .args(["inspect", &slug_a])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Relations:")
                .and(predicate::str::contains("mod-b"))
                .and(predicate::str::contains("depends_on")),
        );
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
