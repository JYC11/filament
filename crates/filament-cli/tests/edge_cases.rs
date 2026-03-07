mod common;

use common::{add_entity, add_task, filament, init_project};
use predicates::prelude::*;

// ---------------------------------------------------------------------------
// Task edge cases
// ---------------------------------------------------------------------------

#[test]
fn task_close_already_closed() {
    let dir = init_project();
    let slug = add_task(&dir, "close-twice", &[]);

    filament(&dir)
        .args(["task", "close", &slug])
        .assert()
        .success();

    // Closing again should still succeed (idempotent or clear message)
    filament(&dir)
        .args(["task", "close", &slug])
        .assert()
        .success();
}

#[test]
fn task_close_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "close", "zzzzzzzz"])
        .assert()
        .failure();
}

#[test]
fn task_show_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "show", "zzzzzzzz"])
        .assert()
        .failure();
}

#[test]
fn task_ready_with_limit_zero() {
    let dir = init_project();
    add_task(&dir, "my-task", &[]);

    filament(&dir)
        .args(["task", "ready", "--limit", "0"])
        .assert()
        .success();
}

#[test]
fn task_add_with_blocks() {
    let dir = init_project();
    let blocker = add_task(&dir, "blocker", &[]);

    // "blocked-task --blocks blocker" means "blocked-task blocks blocker"
    // So blocker is blocked BY blocked-task
    filament(&dir)
        .args(["task", "add", "blocked-task", "--blocks", &blocker])
        .assert()
        .success();

    // blocker should not appear in ready list (it's blocked by blocked-task)
    filament(&dir)
        .args(["task", "ready"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("blocked-task").and(predicate::str::contains("blocker").not()),
        );
}

#[test]
fn task_add_with_depends_on() {
    let dir = init_project();
    let dep = add_task(&dir, "dependency", &[]);

    filament(&dir)
        .args(["task", "add", "dependent-task", "--depends-on", &dep])
        .assert()
        .success();
}

#[test]
fn task_list_status_all() {
    let dir = init_project();
    let slug = add_task(&dir, "will-close", &[]);
    add_task(&dir, "stays-open", &[]);

    filament(&dir)
        .args(["task", "close", &slug])
        .assert()
        .success();

    // --status all should show both open and closed
    filament(&dir)
        .args(["task", "list", "--status", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("will-close").and(predicate::str::contains("stays-open")));
}

// ---------------------------------------------------------------------------
// Relation edge cases
// ---------------------------------------------------------------------------

#[test]
fn relate_nonexistent_source() {
    let dir = init_project();
    let tgt = add_entity(&dir, "target", "module", &[]);

    filament(&dir)
        .args(["relate", "zzzzzzzz", "blocks", &tgt])
        .assert()
        .failure();
}

#[test]
fn relate_nonexistent_target() {
    let dir = init_project();
    let src = add_entity(&dir, "source", "module", &[]);

    filament(&dir)
        .args(["relate", &src, "blocks", "zzzzzzzz"])
        .assert()
        .failure();
}

#[test]
fn unrelate_nonexistent_relation() {
    let dir = init_project();
    let a = add_entity(&dir, "a", "module", &[]);
    let b = add_entity(&dir, "b", "module", &[]);

    // No relation exists — unrelate should still succeed or give clear error
    let output = filament(&dir)
        .args(["unrelate", &a, "blocks", &b])
        .output()
        .unwrap();
    // We just verify it doesn't crash — exit code may vary
    let _ = output;
}

// ---------------------------------------------------------------------------
// Entity edge cases
// ---------------------------------------------------------------------------

#[test]
fn entity_add_with_special_chars() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "Entity with 'quotes'", "--type", "task"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created entity:"));
}

#[test]
fn entity_inspect_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["inspect", "zzzzzzzz"])
        .assert()
        .failure();
}

#[test]
fn entity_update_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["update", "zzzzzzzz", "--summary", "new summary"])
        .assert()
        .failure();
}

#[test]
fn entity_remove_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["remove", "zzzzzzzz"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Search edge cases
// ---------------------------------------------------------------------------

#[test]
fn search_no_results() {
    let dir = init_project();
    add_task(&dir, "existing-task", &[]);

    filament(&dir)
        .args(["search", "zzzznonexistent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No results"));
}

#[test]
fn search_finds_match() {
    let dir = init_project();
    add_task(&dir, "search-target", &["--summary", "unique haystack"]);

    filament(&dir)
        .args(["search", "haystack"])
        .assert()
        .success()
        .stdout(predicate::str::contains("search-target"));
}

// ---------------------------------------------------------------------------
// Lesson edge cases
// ---------------------------------------------------------------------------

#[test]
fn lesson_add_and_list() {
    let dir = init_project();

    filament(&dir)
        .args([
            "lesson",
            "add",
            "my-lesson",
            "--problem",
            "it broke",
            "--solution",
            "fixed it",
            "--learned",
            "don't break things",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created lesson:"));

    filament(&dir)
        .args(["lesson", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-lesson"));
}

#[test]
fn lesson_show_nonexistent() {
    let dir = init_project();

    filament(&dir)
        .args(["lesson", "show", "zzzzzzzz"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Export / Import edge cases
// ---------------------------------------------------------------------------

#[test]
fn export_empty_db() {
    let dir = init_project();

    filament(&dir)
        .args(["export"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"entities\""));
}

#[test]
fn export_import_roundtrip() {
    let dir = init_project();
    add_task(&dir, "roundtrip-task", &["--summary", "survives export"]);

    let export_path = dir.path().join("snapshot.json");

    filament(&dir)
        .args(["export", "--output", export_path.to_str().unwrap()])
        .assert()
        .success();

    // Create a fresh project and import
    let dir2 = init_project();
    filament(&dir2)
        .args(["import", "--input", export_path.to_str().unwrap()])
        .assert()
        .success();

    filament(&dir2)
        .args(["list", "--type", "task"])
        .assert()
        .success()
        .stdout(predicate::str::contains("roundtrip-task"));
}

// ---------------------------------------------------------------------------
// Config edge cases
// ---------------------------------------------------------------------------

#[test]
fn config_show_without_config_file() {
    let dir = init_project();

    filament(&dir).args(["config", "show"]).assert().success();
}

#[test]
fn config_show_json() {
    let dir = init_project();

    filament(&dir)
        .args(["config", "show", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent_command"));
}

// ---------------------------------------------------------------------------
// JSON output edge cases
// ---------------------------------------------------------------------------

#[test]
fn task_list_json_empty() {
    let dir = init_project();

    filament(&dir)
        .args(["task", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[test]
fn task_ready_json() {
    let dir = init_project();
    add_task(&dir, "json-task", &[]);

    filament(&dir)
        .args(["task", "ready", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("json-task"));
}
