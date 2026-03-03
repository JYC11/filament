mod common;

use common::{filament, init_project};
use predicates::prelude::*;

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

#[test]
fn relate_invalid_type_fails() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "src", "--type", "module"])
        .assert()
        .success();
    filament(&dir)
        .args(["add", "tgt", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "src", "invalid_relation", "tgt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid relation type"));
}

#[test]
fn relate_self_referential_fails() {
    let dir = init_project();

    filament(&dir)
        .args(["add", "self-ref", "--type", "module"])
        .assert()
        .success();

    filament(&dir)
        .args(["relate", "self-ref", "blocks", "self-ref"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must differ"));
}
