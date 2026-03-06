mod common;

use common::{filament, init_project};
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn init_creates_filament_dir() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(dir.path().join(".fl").is_dir());
    assert!(dir.path().join(".fl/content").is_dir());
    assert!(dir.path().join(".fl/fl.db").is_file());
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
