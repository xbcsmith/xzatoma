#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn minimal_config_yaml() -> String {
    r#"
provider:
  type: ollama
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
agent:
  max_turns: 50
  timeout_seconds: 300
  terminal:
    default_mode: restricted_autonomous
    timeout_seconds: 30
    max_stdout_bytes: 1048576
    max_stderr_bytes: 262144
"#
    .trim_start()
    .to_string()
}

fn temp_config_file(contents: &str) -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(&config_path, contents).expect("config file should be written");
    (temp_dir, config_path)
}

#[test]
#[serial]
fn test_agent_command_startup_does_not_write_to_stdout() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.arg("--config").arg(&config_path).arg("agent");

    cmd.assert().success().stdout(predicate::str::is_empty());
}

#[test]
#[serial]
fn test_agent_command_accepts_provider_model_and_allow_dangerous_without_stdout() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("agent")
        .arg("--provider")
        .arg("ollama")
        .arg("--model")
        .arg("llama3.2:latest")
        .arg("--allow-dangerous");

    cmd.assert().success().stdout(predicate::str::is_empty());
}

#[test]
#[serial]
fn test_agent_command_accepts_working_dir_without_stdout() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());
    let workspace = TempDir::new().expect("workspace temp dir should be created");

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("agent")
        .arg("--working-dir")
        .arg(workspace.path());

    cmd.assert().success().stdout(predicate::str::is_empty());
}

#[test]
#[serial]
fn test_agent_command_invalid_provider_reports_error_on_stderr_only() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut cmd = Command::cargo_bin("xzatoma").expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("agent")
        .arg("--provider")
        .arg("invalid");

    cmd.assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Invalid provider type"));
}
