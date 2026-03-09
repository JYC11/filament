mod common;

use common::{add_entity, filament, init_project};
use predicates::prelude::*;

#[test]
fn relate_and_unrelate() {
    let dir = init_project();

    let slug_src = add_entity(&dir, "source", "module", &[]);
    let slug_tgt = add_entity(&dir, "target", "module", &[]);

    filament(&dir)
        .args(["relate", &slug_src, "depends_on", &slug_tgt])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created relation:"));

    filament(&dir)
        .args(["unrelate", &slug_src, "depends_on", &slug_tgt])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed relation:"));
}

#[test]
fn relate_invalid_type_fails() {
    let dir = init_project();

    let slug_src = add_entity(&dir, "src", "module", &[]);
    let slug_tgt = add_entity(&dir, "tgt", "module", &[]);

    filament(&dir)
        .args(["relate", &slug_src, "invalid_relation", &slug_tgt])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid RelationType"));
}

#[test]
fn relate_self_referential_fails() {
    let dir = init_project();

    let slug = add_entity(&dir, "self-ref", "module", &[]);

    filament(&dir)
        .args(["relate", &slug, "blocks", &slug])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must differ"));
}

#[test]
fn relate_circular_dependency_prevented() {
    let dir = init_project();

    let a = add_entity(&dir, "cyc-a", "task", &["--summary", "node a"]);
    let b = add_entity(&dir, "cyc-b", "task", &["--summary", "node b"]);
    let c = add_entity(&dir, "cyc-c", "task", &["--summary", "node c"]);

    // A blocks B — OK
    filament(&dir)
        .args(["relate", &a, "blocks", &b])
        .assert()
        .success();

    // B blocks C — OK
    filament(&dir)
        .args(["relate", &b, "blocks", &c])
        .assert()
        .success();

    // C blocks A — should fail (creates cycle A→B→C→A)
    filament(&dir)
        .args(["relate", &c, "blocks", &a])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));

    // Also test depends_on cycle: C depends_on A already has A→B→C chain
    filament(&dir)
        .args(["relate", &a, "depends_on", &c])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));

    // Direct reverse (2-node cycle): B blocks A
    filament(&dir)
        .args(["relate", &b, "blocks", &a])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}
