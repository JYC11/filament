mod common;

use common::{filament, init_project};
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

    filament(&dir)
        .args(["task", "add", "show-me", "--summary", "A showable task"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "show", "show-me"])
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

    filament(&dir)
        .args(["task", "add", "closeable", "--summary", "Will be closed"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "close", "closeable"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Closed task: closeable"));

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

    filament(&dir)
        .args([
            "task",
            "add",
            "unblocked-task",
            "--summary",
            "Should be ready",
        ])
        .assert()
        .success();

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
    filament(&dir)
        .args([
            "task",
            "add",
            "blocked-task",
            "--summary",
            "This will be blocked",
        ])
        .assert()
        .success();

    filament(&dir)
        .args([
            "task",
            "add",
            "blocker-task",
            "--summary",
            "This blocks another",
            "--blocks",
            "blocked-task",
        ])
        .assert()
        .success();

    // blocked-task should not be ready (blocker-task blocks it)
    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("blocker-task")
                .and(predicate::str::contains("blocked-task").not()),
        );

    // Close the blocker, now blocked-task becomes ready
    filament(&dir)
        .args(["task", "close", "blocker-task"])
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

    filament(&dir)
        .args(["task", "add", "step-1", "--summary", "First step"])
        .assert()
        .success();
    filament(&dir)
        .args(["task", "add", "step-2", "--summary", "Second step"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "step-1", "blocks", "step-2"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "critical-path", "step-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("step-1").and(predicate::str::contains("step-2")));
}

#[test]
fn task_critical_path_no_deps() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "add", "standalone", "--summary", "No deps"])
        .assert()
        .success();

    // With no outgoing dependency edges, the path is just the node itself (1 step)
    filament(&dir)
        .args(["task", "critical-path", "standalone"])
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

    filament(&dir)
        .args(["task", "add", "assignable", "--summary", "Will be assigned"])
        .assert()
        .success();

    filament(&dir)
        .args([
            "add",
            "worker-agent",
            "--type",
            "agent",
            "--summary",
            "An agent",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "assign", "assignable", "--to", "worker-agent"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Assigned assignable to worker-agent",
        ));
}

#[test]
fn task_list_unblocked() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "add", "free-task", "--summary", "Not blocked"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "add", "victim-task", "--summary", "Will be blocked"])
        .assert()
        .success();

    filament(&dir)
        .args([
            "task",
            "add",
            "blocker",
            "--summary",
            "Blocks another task",
            "--blocks",
            "victim-task",
        ])
        .assert()
        .success();

    // --unblocked should show free-task and blocker, but NOT victim-task
    let output = filament(&dir)
        .args(["task", "list", "--unblocked"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("free-task"), "should contain free-task");
    assert!(stdout.contains("blocker"), "should contain blocker");
    // victim-task appears only on its own line (blocked), not in a summary
    let victim_lines: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("[P") && line.contains("victim-task"))
        .collect();
    assert!(
        victim_lines.is_empty(),
        "victim-task should not appear as a list item, but found: {victim_lines:?}"
    );
}

#[test]
fn task_show_displays_relation_names() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "add", "parent-task", "--summary", "Parent"])
        .assert()
        .success();
    filament(&dir)
        .args(["task", "add", "child-task", "--summary", "Child"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "parent-task", "blocks", "child-task"])
        .assert()
        .success();

    // Should show entity names (not UUIDs) in relations
    filament(&dir)
        .args(["task", "show", "parent-task"])
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

    filament(&dir)
        .args([
            "task",
            "add",
            "json-ready",
            "--summary",
            "Ready task",
            "--priority",
            "0",
        ])
        .assert()
        .success();

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

    filament(&dir)
        .args(["task", "add", "cp-a", "--summary", "Step A"])
        .assert()
        .success();
    filament(&dir)
        .args(["task", "add", "cp-b", "--summary", "Step B"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "cp-a", "blocks", "cp-b"])
        .assert()
        .success();

    // 2 steps should use plural "steps"
    filament(&dir)
        .args(["task", "critical-path", "cp-a"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Critical path (2 steps)"));
}

#[test]
fn task_close_rejects_non_task() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "my-module", "--type", "module", "--summary", "A module"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "close", "my-module"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a task"));
}

#[test]
fn task_assign_rejects_non_task() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "my-module", "--type", "module", "--summary", "A module"])
        .assert()
        .success();

    filament(&dir)
        .args(["add", "worker", "--type", "agent", "--summary", "An agent"])
        .assert()
        .success();

    filament(&dir)
        .args(["task", "assign", "my-module", "--to", "worker"])
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

    filament(&dir)
        .args(["task", "add", "dep-target", "--summary", "Dependency target"])
        .assert()
        .success();

    filament(&dir)
        .args([
            "task",
            "add",
            "dep-source",
            "--summary",
            "Depends on target",
            "--depends-on",
            "dep-target",
        ])
        .assert()
        .success();

    // The dependency should show in task show
    filament(&dir)
        .args(["task", "show", "dep-source"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Relations:")
                .and(predicate::str::contains("dep-target"))
                .and(predicate::str::contains("depends_on")),
        );
}
