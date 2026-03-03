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
        .stderr(predicate::str::contains("invalid relation type"));
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
