use assert_cmd::Command;
use tempfile::TempDir;

/// Create a command that runs `filament` in a temp directory.
pub fn filament(dir: &TempDir) -> Command {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("filament").unwrap();
    cmd.current_dir(dir.path());
    cmd
}

/// Initialize a filament project and return the temp dir.
pub fn init_project() -> TempDir {
    use predicates::prelude::*;
    let dir = TempDir::new().unwrap();
    filament(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized filament project"));
    dir
}
