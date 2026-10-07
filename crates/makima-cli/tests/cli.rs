//! End-to-end tests of the `makima` binary against temporary config files.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn makima(config: &Path) -> Command {
    let mut command = Command::cargo_bin("makima").unwrap_or_else(|e| panic!("binary builds: {e}"));
    command.arg("--config").arg(config);
    command
}

#[test]
fn init_writes_a_valid_config_once() {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("makima/config.toml");

    makima(&path).args(["config", "init"]).assert().success();
    makima(&path)
        .args(["config", "check"])
        .assert()
        .success()
        .stdout(contains("is valid"));
    makima(&path)
        .args(["config", "init"])
        .assert()
        .failure()
        .stderr(contains("already exists"));
    makima(&path)
        .args(["config", "init", "--force"])
        .assert()
        .success();
}

#[test]
fn check_points_at_the_mistake() {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("config.toml");
    fs::write(&path, "[terminal]\nfont-sise = 13\n").unwrap_or_else(|e| panic!("{e}"));

    makima(&path)
        .args(["config", "check"])
        .assert()
        .failure()
        .stderr(contains("parsing"))
        .stderr(contains("unknown field `font-sise`"));
}

#[test]
fn missing_configs_use_defaults() {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("absent.toml");

    makima(&path)
        .args(["config", "check"])
        .assert()
        .success()
        .stdout(contains("using defaults"));
    makima(&path)
        .arg("presets")
        .assert()
        .success()
        .stdout(contains("claude"));
}

#[test]
fn presets_include_config_agents() {
    let dir = TempDir::new().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[[agents]]\nid = \"aider\"\nlabel = \"Aider\"\ncommand = \"aider --yes\"\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));

    makima(&path)
        .arg("presets")
        .assert()
        .success()
        .stdout(contains("aider --yes"));
}
