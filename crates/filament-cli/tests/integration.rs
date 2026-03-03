use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Create a command that runs `filament` in a temp directory.
fn filament(dir: &TempDir) -> Command {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("filament").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

/// Initialize a filament project and return the temp dir.
fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized filament project"));
    dir
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[test]
fn init_creates_filament_dir() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(dir.path().join(".filament").is_dir());
    assert!(dir.path().join(".filament/content").is_dir());
    assert!(dir.path().join(".filament/filament.db").is_file());
}

#[test]
fn init_idempotent() {
    let dir = init_project();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Already initialized"));
}

#[test]
fn init_json_output() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .args(["--json", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""status": "initialized""#));
}

// ---------------------------------------------------------------------------
// Entity CRUD
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Relation commands
// ---------------------------------------------------------------------------

#[test]
fn relate_and_unrelate() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "source", "--type", "module"])
        .assert()
        .success();
    filament(&dir)
        .args(["add", "target", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "source", "depends_on", "target"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created relation:"));

    filament(&dir)
        .args(["unrelate", "source", "depends_on", "target"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed relation:"));
}

// ---------------------------------------------------------------------------
// Task commands
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Context query
// ---------------------------------------------------------------------------

#[test]
fn context_around_entity() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "center",
            "--type",
            "module",
            "--summary",
            "Center node",
        ])
        .assert()
        .success();
    filament(&dir)
        .args([
            "add",
            "neighbor",
            "--type",
            "module",
            "--summary",
            "Nearby node",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "center", "relates_to", "neighbor"])
        .assert()
        .success();

    filament(&dir)
        .args(["context", "--around", "center", "--depth", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("neighbor"));
}

// ---------------------------------------------------------------------------
// Message commands
// ---------------------------------------------------------------------------

#[test]
fn message_send_inbox_read() {
    let dir = init_project();

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
            "--body",
            "Hello from A",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent message:"));

    filament(&dir)
        .args(["message", "inbox", "agent-b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello from A"));

    // Get the message ID from JSON output
    let output = filament(&dir)
        .args(["--json", "message", "inbox", "agent-b"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let msgs: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let msg_id = msgs[0]["id"].as_str().unwrap().to_string();

    filament(&dir)
        .args(["message", "read", &msg_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Marked as read:"));

    // Inbox should now be empty
    filament(&dir)
        .args(["message", "inbox", "agent-b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No unread messages"));
}

// ---------------------------------------------------------------------------
// Reservation commands
// ---------------------------------------------------------------------------

#[test]
fn reserve_and_release() {
    let dir = init_project();

    filament(&dir)
        .args([
            "reserve",
            "src/**/*.rs",
            "--agent",
            "agent-1",
            "--exclusive",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reserved:"));

    filament(&dir)
        .args(["reservations"])
        .assert()
        .success()
        .stdout(predicate::str::contains("src/**/*.rs"));

    filament(&dir)
        .args(["release", "src/**/*.rs", "--agent", "agent-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Released:"));

    filament(&dir)
        .args(["reservations"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No active reservations"));
}

#[test]
fn reserve_conflict() {
    let dir = init_project();

    filament(&dir)
        .args(["reserve", "src/*.rs", "--agent", "agent-1", "--exclusive"])
        .assert()
        .success();

    // Second agent can't take exclusive reservation on same glob
    filament(&dir)
        .args(["reserve", "src/*.rs", "--agent", "agent-2", "--exclusive"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("File reserved by"));
}

// ---------------------------------------------------------------------------
// JSON output mode
// ---------------------------------------------------------------------------

#[test]
fn list_json_output() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "json-entity",
            "--type",
            "module",
            "--summary",
            "For JSON test",
        ])
        .assert()
        .success();

    let output = filament(&dir).args(["--json", "list"]).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let entities: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["name"], "json-entity");
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// End-to-end workflow
// ---------------------------------------------------------------------------

#[test]
fn full_workflow() {
    let dir = init_project();

    // Add entities
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
        .success();

    filament(&dir)
        .args([
            "task",
            "add",
            "implement-login",
            "--summary",
            "Build login endpoint",
            "--priority",
            "1",
        ])
        .assert()
        .success();

    // Create relation
    filament(&dir)
        .args(["relate", "implement-login", "depends_on", "auth-module"])
        .assert()
        .success();

    // Context query (from implement-login, which has outgoing depends_on edge to auth-module)
    filament(&dir)
        .args(["context", "--around", "implement-login", "--depth", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth-module"));

    // Task ready (implement-login depends on auth-module, but depends_on doesn't block)
    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("implement-login"));

    // Close task
    filament(&dir)
        .args(["task", "close", "implement-login"])
        .assert()
        .success();

    // Verify closed
    filament(&dir)
        .args(["inspect", "implement-login"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status:   closed"));
}
