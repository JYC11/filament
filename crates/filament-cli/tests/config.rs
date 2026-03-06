mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn init_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    Command::cargo_bin("fl")
        .unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    dir
}

fn filament(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("fl").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

#[test]
fn config_show_defaults() {
    let dir = init_project();
    filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default_priority     = 2"))
        .stdout(predicate::str::contains("output_format        = text"))
        .stdout(predicate::str::contains("agent_command        = claude"));
}

#[test]
fn config_show_json() {
    let dir = init_project();
    filament(&dir)
        .args(["--json", "config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"default_priority\": 2"))
        .stdout(predicate::str::contains("\"agent_command\": \"claude\""));
}

#[test]
fn config_init_prints_template() {
    let dir = init_project();
    filament(&dir)
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Filament project configuration"))
        .stdout(predicate::str::contains("default_priority"))
        .stdout(predicate::str::contains("output_format"));
}

#[test]
fn config_path_shows_config_location() {
    let dir = init_project();
    filament(&dir)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
}

#[test]
fn config_file_overrides_defaults() {
    let dir = init_project();

    // Write a config file
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "default_priority = 4\nagent_command = \"my-agent\"\n",
    )
    .unwrap();

    filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default_priority     = 4"))
        .stdout(predicate::str::contains("agent_command        = my-agent"));
}

#[test]
fn config_file_json_output_format_applied() {
    let dir = init_project();

    // Write config that sets json as default output
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "output_format = \"json\"\n",
    )
    .unwrap();

    // Even without --json flag, config show should use json format
    // The config show command's own output is controlled by cli.json,
    // but we can verify config shows json format
    filament(&dir)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"default_priority\""));
}

#[test]
fn config_default_priority_used_by_task_add() {
    let dir = init_project();

    // Write config with priority 4
    std::fs::write(
        dir.path().join(".fl").join("config.toml"),
        "default_priority = 4\n",
    )
    .unwrap();

    // Add a task without --priority flag
    filament(&dir)
        .args(["task", "add", "test-task", "--summary", "testing priority"])
        .assert()
        .success();

    // List tasks and check priority
    filament(&dir)
        .args(["task", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[P4]"));
}
