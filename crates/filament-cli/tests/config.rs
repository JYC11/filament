mod common;

use predicates::prelude::*;

#[test]
fn config_show_defaults() {
    let dir = common::init_project();
    common::filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default_priority     = 2"))
        .stdout(predicate::str::contains("output_format        = text"))
        .stdout(predicate::str::contains("agent_command        = claude"));
}

#[test]
fn config_show_json() {
    let dir = common::init_project();
    common::filament(&dir)
        .args(["--json", "config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"default_priority\": 2"))
        .stdout(predicate::str::contains("\"agent_command\": \"claude\""));
}

#[test]
fn config_init_prints_template() {
    let dir = common::init_project();
    common::filament(&dir)
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Filament project configuration"))
        .stdout(predicate::str::contains("default_priority"))
        .stdout(predicate::str::contains("output_format"));
}

#[test]
fn config_path_shows_config_location() {
    let dir = common::init_project();
    common::filament(&dir)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
}

#[test]
fn config_file_overrides_defaults() {
    let dir = common::init_project();

    // Write a config file
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "default_priority = 4\nagent_command = \"my-agent\"\n",
    )
    .unwrap();

    common::filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default_priority     = 4"))
        .stdout(predicate::str::contains("agent_command        = my-agent"));
}

#[test]
fn config_file_json_output_format_applied() {
    let dir = common::init_project();

    // Write config that sets json as default output
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "output_format = \"json\"\n",
    )
    .unwrap();

    // Even without --json flag, config show should use json format
    // The config show command's own output is controlled by cli.json,
    // but we can verify config shows json format
    common::filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"default_priority\""));
}

#[test]
fn config_default_priority_used_by_task_add() {
    let dir = common::init_project();

    // Write config with priority 4
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "default_priority = 4\n",
    )
    .unwrap();

    // Add a task without --priority flag
    common::filament(&dir)
        .args(["task", "add", "test-task", "--summary", "testing priority"])
        .assert()
        .success();

    // List tasks and check priority
    common::filament(&dir)
        .args(["task", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[P4]"));
}
