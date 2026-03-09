mod common;

use common::{add_entity, filament, init_project};
use predicates::prelude::*;

#[test]
fn message_send_inbox_read() {
    let dir = init_project();

    let agent_a = add_entity(&dir, "agent-a", "agent", &["--summary", "Agent A"]);
    let agent_b = add_entity(&dir, "agent-b", "agent", &["--summary", "Agent B"]);

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            &agent_a,
            "--to",
            &agent_b,
            "--body",
            "Hello from A",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent message:"));

    filament(&dir)
        .args(["message", "inbox", &agent_b])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello from A"));

    // Get the message ID from JSON output
    let output = filament(&dir)
        .args(["--json", "message", "inbox", &agent_b])
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
        .args(["message", "inbox", &agent_b])
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

    let agent_x = add_entity(&dir, "agent-x", "agent", &["--summary", "Agent X"]);
    let agent_y = add_entity(&dir, "agent-y", "agent", &["--summary", "Agent Y"]);

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            &agent_x,
            "--to",
            &agent_y,
            "--body",
            "I'm blocked",
            "--type",
            "blocker",
        ])
        .assert()
        .success();

    filament(&dir)
        .args(["message", "inbox", &agent_y])
        .assert()
        .success()
        .stdout(predicate::str::contains("type:blocker"));
}

#[test]
fn message_send_to_user_escalation() {
    let dir = init_project();

    let agent_z = add_entity(&dir, "agent-z", "agent", &["--summary", "Agent Z"]);

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            &agent_z,
            "--to",
            "user",
            "--body",
            "Need help",
            "--type",
            "blocker",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sent message:"));
}

#[test]
fn message_send_to_nonexistent_entity_rejected() {
    let dir = init_project();

    let agent_ok = add_entity(&dir, "agent-ok", "agent", &["--summary", "Valid agent"]);

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            &agent_ok,
            "--to",
            "zzzzzzzz",
            "--body",
            "This should fail",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("to_agent entity not found"));
}

#[test]
fn message_send_from_nonexistent_entity_rejected() {
    let dir = init_project();

    let agent_ok = add_entity(&dir, "agent-ok", "agent", &["--summary", "Valid agent"]);

    filament(&dir)
        .args([
            "message",
            "send",
            "--from",
            "zzzzzzzz",
            "--to",
            &agent_ok,
            "--body",
            "This should fail",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("from_agent entity not found"));
}
