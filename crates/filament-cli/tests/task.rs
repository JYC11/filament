mod common;

use common::{add_entity, add_task, filament, init_project};
use predicates::prelude::*;

#[test]
fn task_add_and_list() {
    let dir = init_project();

    filament(&dir)
        .args([
            "task",
            "add",
            "build-login",
            "--summary",
            "Build login endpoint",
            "--priority",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task:"));

    filament(&dir)
        .args(["task", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("build-login"));
}

#[test]
fn task_show() {
    let dir = init_project();

    let slug = add_task(&dir, "show-me", &["--summary", "A showable task"]);

    filament(&dir)
        .args(["task", "show", &slug])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Task:     show-me")
                .and(predicate::str::contains("Summary:  A showable task")),
        );
}

#[test]
fn task_close() {
    let dir = init_project();

    let slug = add_task(&dir, "closeable", &["--summary", "Will be closed"]);

    filament(&dir)
        .args(["task", "close", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Closed task:"));

    // Should no longer appear in default (open) task list
    filament(&dir)
        .args(["task", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("closeable").not());

    // But appears in --status all
    filament(&dir)
        .args(["task", "list", "--status", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("closeable"));
}

#[test]
fn task_ready() {
    let dir = init_project();

    add_task(&dir, "unblocked-task", &["--summary", "Should be ready"]);

    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unblocked-task"));
}

#[test]
fn task_with_blocks() {
    let dir = init_project();

    // Create a target task first, then create a blocker
    let blocked_slug = add_task(&dir, "blocked-task", &["--summary", "This will be blocked"]);

    add_task(
        &dir,
        "blocker-task",
        &[
            "--summary",
            "This blocks another",
            "--blocks",
            &blocked_slug,
        ],
    );

    // blocked-task should not be ready (blocker-task blocks it)
    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("blocker-task")
                .and(predicate::str::contains("blocked-task").not()),
        );

    // Close the blocker
    let blocker_slug = {
        // Find the blocker slug from the task list
        let output = filament(&dir)
            .args(["--json", "task", "list"])
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
        let blocker = tasks.iter().find(|t| t["name"] == "blocker-task").unwrap();
        blocker["slug"].as_str().unwrap().to_string()
    };

    filament(&dir)
        .args(["task", "close", &blocker_slug])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("blocked-task"));
}

#[test]
fn task_critical_path() {
    let dir = init_project();

    let slug1 = add_task(&dir, "step-1", &["--summary", "First step"]);
    let slug2 = add_task(&dir, "step-2", &["--summary", "Second step"]);

    filament(&dir)
        .args(["relate", &slug1, "blocks", &slug2])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "critical-path", &slug1])
        .assert()
        .success()
        .stdout(predicate::str::contains("step-1").and(predicate::str::contains("step-2")));
}

#[test]
fn task_critical_path_no_deps() {
    let dir = init_project();

    let slug = add_task(&dir, "standalone", &["--summary", "No deps"]);

    // With no outgoing dependency edges, the path is just the node itself (1 step)
    filament(&dir)
        .args(["task", "critical-path", &slug])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Critical path (1 step)")
                .and(predicate::str::contains("standalone")),
        );
}

#[test]
fn task_assign() {
    let dir = init_project();

    let task_slug = add_task(&dir, "assignable", &["--summary", "Will be assigned"]);
    let agent_slug = add_entity(&dir, "worker-agent", "agent", &["--summary", "An agent"]);

    filament(&dir)
        .args(["task", "assign", &task_slug, "--to", &agent_slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("Assigned assignable to"));
}

#[test]
fn task_list_unblocked() {
    let dir = init_project();

    add_task(&dir, "free-task", &["--summary", "Not blocked"]);
    let victim_slug = add_task(&dir, "victim-task", &["--summary", "Will be blocked"]);

    add_task(
        &dir,
        "blocker",
        &["--summary", "Blocks another task", "--blocks", &victim_slug],
    );

    // --unblocked should show free-task and blocker, but NOT victim-task
    let output = filament(&dir)
        .args(["task", "list", "--unblocked"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("free-task"), "should contain free-task");
    assert!(stdout.contains("blocker"), "should contain blocker");
    // victim-task should not appear as a list item
    assert!(
        !stdout.contains("victim-task"),
        "victim-task should not appear in unblocked list, but found in: {stdout}"
    );
}

#[test]
fn task_show_displays_relation_names() {
    let dir = init_project();

    let parent_slug = add_task(&dir, "parent-task", &["--summary", "Parent"]);
    let child_slug = add_task(&dir, "child-task", &["--summary", "Child"]);

    filament(&dir)
        .args(["relate", &parent_slug, "blocks", &child_slug])
        .assert()
        .success();

    // Should show entity names (not UUIDs) in relations
    filament(&dir)
        .args(["task", "show", &parent_slug])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Relations:")
                .and(predicate::str::contains("child-task"))
                .and(predicate::str::contains("blocks")),
        );
}

#[test]
fn task_ready_json() {
    let dir = init_project();

    add_task(
        &dir,
        "json-ready",
        &["--summary", "Ready task", "--priority", "0"],
    );

    let output = filament(&dir)
        .args(["--json", "task", "ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["name"], "json-ready");
    assert_eq!(tasks[0]["priority"], 0);
    assert_eq!(tasks[0]["status"], "open");
}

#[test]
fn task_critical_path_plural_steps() {
    let dir = init_project();

    let slug_a = add_task(&dir, "cp-a", &["--summary", "Step A"]);
    let slug_b = add_task(&dir, "cp-b", &["--summary", "Step B"]);

    filament(&dir)
        .args(["relate", &slug_a, "blocks", &slug_b])
        .assert()
        .success();

    // 2 steps should use plural "steps"
    filament(&dir)
        .args(["task", "critical-path", &slug_a])
        .assert()
        .success()
        .stdout(predicate::str::contains("Critical path (2 steps)"));
}

#[test]
fn task_close_rejects_non_task() {
    let dir = init_project();

    let slug = add_entity(&dir, "my-module", "module", &["--summary", "A module"]);

    filament(&dir)
        .args(["task", "close", &slug])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a task"));
}

#[test]
fn task_assign_rejects_non_task() {
    let dir = init_project();

    let mod_slug = add_entity(&dir, "my-module", "module", &["--summary", "A module"]);
    let agent_slug = add_entity(&dir, "worker", "agent", &["--summary", "An agent"]);

    filament(&dir)
        .args(["task", "assign", &mod_slug, "--to", &agent_slug])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a task"));
}

#[test]
fn task_list_status_conflicts_with_unblocked() {
    let dir = init_project();

    // --status and --unblocked should conflict
    filament(&dir)
        .args(["task", "list", "--status", "closed", "--unblocked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn task_add_with_depends_on() {
    let dir = init_project();

    let target_slug = add_task(&dir, "dep-target", &["--summary", "Dependency target"]);

    add_task(
        &dir,
        "dep-source",
        &[
            "--summary",
            "Depends on target",
            "--depends-on",
            &target_slug,
        ],
    );

    // Find the dep-source slug from the task list
    let output = filament(&dir)
        .args(["--json", "task", "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let source = tasks.iter().find(|t| t["name"] == "dep-source").unwrap();
    let source_slug = source["slug"].as_str().unwrap();

    // The dependency should show in task show
    filament(&dir)
        .args(["task", "show", source_slug])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Relations:")
                .and(predicate::str::contains("dep-target"))
                .and(predicate::str::contains("depends_on")),
        );
}
