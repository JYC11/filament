mod common;

use common::{filament, init_project};
use predicates::prelude::*;

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

#[test]
fn context_finds_incoming_edge_neighbors() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "upstream",
            "--type",
            "module",
            "--summary",
            "Upstream module",
        ])
        .assert()
        .success();
    filament(&dir)
        .args([
            "add",
            "downstream",
            "--type",
            "module",
            "--summary",
            "Downstream module",
        ])
        .assert()
        .success();

    // upstream depends_on downstream (edge from upstream to downstream)
    filament(&dir)
        .args(["relate", "upstream", "depends_on", "downstream"])
        .assert()
        .success();

    // Query context around downstream — should find upstream via incoming edge
    filament(&dir)
        .args(["context", "--around", "downstream", "--depth", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upstream"));
}

#[test]
fn context_no_relations() {
    let dir = init_project();

    filament(&dir)
        .args([
            "add",
            "lonely",
            "--type",
            "module",
            "--summary",
            "No friends",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["context", "--around", "lonely", "--depth", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No context found"));
}
