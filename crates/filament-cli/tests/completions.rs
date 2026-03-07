mod common;

use common::filament;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn completions_bash_outputs_valid_script() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fl"));
}

#[test]
fn completions_zsh_outputs_valid_script() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fl"));
}

#[test]
fn completions_fish_outputs_valid_script() {
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fl"));
}
