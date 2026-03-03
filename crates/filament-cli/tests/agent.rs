mod common;

use common::{add_task, filament, init_project};
use predicates::prelude::*;

#[test]
fn agent_dispatch_requires_daemon() {
    let dir = init_project();
    let slug = add_task(&dir, "test-dispatch", &["--summary", "Test dispatch"]);

    // Without a daemon running, dispatch should fail with a clear error
    filament(&dir)
        .args(["agent", "dispatch", &slug, "--role", "coder"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("daemon").or(predicate::str::contains("dispatch")));
}

#[test]
fn agent_history_direct_mode() {
    let dir = init_project();
    let slug = add_task(&dir, "history-task", &["--summary", "Test history"]);

    // History should work in direct mode (just shows empty)
    filament(&dir)
        .args(["agent", "history", &slug])
        .assert()
        .success()
        .stdout(predicate::str::contains("No agent runs"));
}

#[test]
fn agent_list_direct_mode() {
    let dir = init_project();

    // List running agents in direct mode
    filament(&dir)
        .args(["agent", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No running agents"));
}

#[test]
fn agent_dispatch_all_requires_daemon() {
    let dir = init_project();

    filament(&dir)
        .args(["agent", "dispatch-all", "--role", "coder"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("daemon").or(predicate::str::contains("dispatch")));
}
