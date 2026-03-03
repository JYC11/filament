mod common;

use common::{filament, init_project};
use predicates::prelude::*;

#[test]
fn reserve_and_release() {
    let dir = init_project();

    filament(&dir)
        .args([
            "reserve",
            "src/**/*.rs",
            "--agent",
            "agent-1",
            "--exclusive",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reserved:"));

    filament(&dir)
        .args(["reservations"])
        .assert()
        .success()
        .stdout(predicate::str::contains("src/**/*.rs"));

    filament(&dir)
        .args(["release", "src/**/*.rs", "--agent", "agent-1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Released:"));

    filament(&dir)
        .args(["reservations"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No active reservations"));
}

#[test]
fn reserve_conflict() {
    let dir = init_project();

    filament(&dir)
        .args(["reserve", "src/*.rs", "--agent", "agent-1", "--exclusive"])
        .assert()
        .success();

    // Second agent can't take exclusive reservation on same glob
    filament(&dir)
        .args(["reserve", "src/*.rs", "--agent", "agent-2", "--exclusive"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("File reserved by"));
}

#[test]
fn reservations_filter_by_agent() {
    let dir = init_project();

    filament(&dir)
        .args(["reserve", "src/*.rs", "--agent", "agent-1", "--exclusive"])
        .assert()
        .success();

    filament(&dir)
        .args(["reserve", "tests/*.rs", "--agent", "agent-2"])
        .assert()
        .success();

    // Filter shows only agent-1's reservations
    filament(&dir)
        .args(["reservations", "--agent", "agent-1"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("src/*.rs").and(predicate::str::contains("tests/*.rs").not()),
        );
}

#[test]
fn reservations_clean() {
    let dir = init_project();

    // Create a reservation with minimum TTL (1 second)
    filament(&dir)
        .args(["reserve", "tmp/*.rs", "--agent", "agent-1", "--ttl", "1"])
        .assert()
        .success();

    // Wait for expiry
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Clean should remove expired
    filament(&dir)
        .args(["reservations", "--clean"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleaned up"));
}
