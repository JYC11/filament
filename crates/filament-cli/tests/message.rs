mod common;

use common::{filament, init_project};
use predicates::prelude::*;

#[test]
fn message_send_inbox_read() {
    let dir = init_project();

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            "agent-a",
            "--to",
            "agent-b",
            "--body",
            "Hello from A",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent message:"));

    filament(&dir)
        .args(["message", "inbox", "agent-b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello from A"));

    // Get the message ID from JSON output
    let output = filament(&dir)
        .args(["--json", "message", "inbox", "agent-b"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let msgs: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let msg_id = msgs[0]["id"].as_str().unwrap().to_string();

    filament(&dir)
        .args(["message", "read", &msg_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Marked as read:"));

    // Inbox should now be empty
    filament(&dir)
        .args(["message", "inbox", "agent-b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No unread messages"));
}

#[test]
fn message_read_invalid_id() {
    let dir = init_project();

    filament(&dir)
        .args(["message", "read", "nonexistent-msg-id"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Message not found"));
}

#[test]
fn message_send_with_type() {
    let dir = init_project();

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            "agent-x",
            "--to",
            "agent-y",
            "--body",
            "I'm blocked",
            "--type",
            "blocker",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["message", "inbox", "agent-y"])
        .assert()
        .success()
        .stdout(predicate::str::contains("type:blocker"));
}
