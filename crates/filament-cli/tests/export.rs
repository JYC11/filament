mod common;

use common::{add_task, filament, init_project};
use predicates::prelude::*;

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[test]
fn export_empty_db_produces_valid_json() {
    let dir = init_project();

    let output = filament(&dir).arg("export").output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(data["version"], 1);
    assert!(data["entities"].as_array().unwrap().is_empty());
}

#[test]
fn export_includes_created_entities() {
    let dir = init_project();
    add_task(&dir, "Export Test Task", &[]);

    let output = filament(&dir).arg("export").output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(data["entities"].as_array().unwrap().len(), 1);
}

#[test]
fn export_no_events_flag_omits_events() {
    let dir = init_project();
    add_task(&dir, "Event Test", &[]);

    let output = filament(&dir)
        .args(["export", "--no-events"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let data: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(data["events"].as_array().unwrap().is_empty());
    // Entities should still be present
    assert!(!data["entities"].as_array().unwrap().is_empty());
}

#[test]
fn export_to_file_writes_json() {
    let dir = init_project();
    add_task(&dir, "File Export Task", &[]);

    let output_path = dir.path().join("backup.json");
    filament(&dir)
        .args(["export", "--output", output_path.to_str().unwrap()])
        .assert()
        .success();

    let contents = std::fs::read_to_string(&output_path).unwrap();
    let data: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(data["version"], 1);
    assert!(!data["entities"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[test]
fn import_from_file_creates_entities() {
    // Export from one project
    let src = init_project();
    add_task(&src, "Migrate Me", &["--summary", "portable task"]);
    let export_output = filament(&src).arg("export").output().unwrap();
    let export_json = String::from_utf8(export_output.stdout).unwrap();

    // Import into a fresh project
    let dst = init_project();
    let import_file = dst.path().join("import.json");
    std::fs::write(&import_file, &export_json).unwrap();

    filament(&dst)
        .args(["import", "--input", import_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("entities:  1"));
}

#[test]
fn import_invalid_json_gives_clear_error() {
    let dir = init_project();
    let bad_file = dir.path().join("bad.json");
    std::fs::write(&bad_file, "not valid json").unwrap();

    filament(&dir)
        .args(["import", "--input", bad_file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid export JSON"));
}

// ---------------------------------------------------------------------------
// Round-trip: export → import
// ---------------------------------------------------------------------------

#[test]
fn export_import_round_trip() {
    let src = init_project();
    let slug1 = add_task(&src, "Task A", &["--priority", "0"]);
    let slug2 = add_task(&src, "Task B", &["--priority", "1"]);

    // Relate them
    filament(&src)
        .args(["relate", &slug1, "blocks", &slug2])
        .assert()
        .success();

    // Export
    let export_output = filament(&src).arg("export").output().unwrap();
    let json = String::from_utf8(export_output.stdout).unwrap();

    // Import into fresh project
    let dst = init_project();
    let import_file = dst.path().join("data.json");
    std::fs::write(&import_file, &json).unwrap();

    filament(&dst)
        .args(["import", "--input", import_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("entities:  2"))
        .stdout(predicate::str::contains("relations: 1"));

    // Verify entities exist in destination
    filament(&dst)
        .args(["list", "--type", "task"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task A"))
        .stdout(predicate::str::contains("Task B"));
}

// ---------------------------------------------------------------------------
// Escalations
// ---------------------------------------------------------------------------

#[test]
fn escalations_empty_shows_no_pending() {
    let dir = init_project();

    filament(&dir)
        .arg("escalations")
        .assert()
        .success()
        .stdout(predicate::str::contains("No pending escalations"));
}
