mod common;

use common::{add_entity, filament, init_project};
use predicates::prelude::*;

#[test]
fn context_around_entity() {
    let dir = init_project();

    let slug_center = add_entity(&dir, "center", "module", &["--summary", "Center node"]);
    let slug_neighbor = add_entity(&dir, "neighbor", "module", &["--summary", "Nearby node"]);

    filament(&dir)
        .args(["relate", &slug_center, "relates_to", &slug_neighbor])
        .assert()
        .success();

    filament(&dir)
        .args(["context", "--around", &slug_center, "--depth", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("neighbor"));
}

#[test]
fn context_finds_incoming_edge_neighbors() {
    let dir = init_project();

    let slug_up = add_entity(
        &dir,
        "upstream",
        "module",
        &["--summary", "Upstream module"],
    );
    let slug_down = add_entity(
        &dir,
        "downstream",
        "module",
        &["--summary", "Downstream module"],
    );

    // upstream depends_on downstream (edge from upstream to downstream)
    filament(&dir)
        .args(["relate", &slug_up, "depends_on", &slug_down])
        .assert()
        .success();

    // Query context around downstream — should find upstream via incoming edge
    filament(&dir)
        .args(["context", "--around", &slug_down, "--depth", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upstream"));
}

#[test]
fn context_no_relations() {
    let dir = init_project();

    let slug = add_entity(&dir, "lonely", "module", &["--summary", "No friends"]);

    filament(&dir)
        .args(["context", "--around", &slug, "--depth", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No context found"));
}
