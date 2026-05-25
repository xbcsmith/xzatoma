use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command as StdCommand, Stdio};
use tempfile::TempDir;

mod common;

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

fn spawn_agent(config_path: &std::path::Path) -> Child {
    let mut cmd = StdCommand::new(common::xzatoma_binary_path());
    cmd.arg("--config")
        .arg(config_path)
        .arg("agent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.spawn().expect("agent command should spawn")
}

fn read_initialize_response(child: &mut Child) -> serde_json::Value {
    let stdin = child.stdin.as_mut().expect("agent stdin should be piped");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":1,"clientCapabilities":{{}}}}}}"#
    )
    .expect("initialize request should be written");

    let stdout = child.stdout.take().expect("agent stdout should be piped");
    let mut reader = BufReader::new(stdout);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .expect("initialize response should be readable");

    serde_json::from_str(&response).expect("initialize response should be valid JSON")
}

fn terminate_child(mut child: Child) {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => panic!("agent process should be killable: {}", error),
    }

    match child.wait() {
        Ok(_status) => {}
        Err(error) => panic!("agent process should be waitable: {}", error),
    }
}

#[test]
#[serial]
fn test_agent_command_initialize_returns_xzatoma_metadata() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut child = spawn_agent(&config_path);
    let response = read_initialize_response(&mut child);
    terminate_child(child);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["agentInfo"]["name"], "xzatoma");
    assert_eq!(
        response["result"]["agentInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
#[serial]
fn test_agent_command_accepts_provider_model_and_allow_dangerous_for_initialize() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut cmd = StdCommand::new(common::xzatoma_binary_path());
    cmd.arg("--config")
        .arg(&config_path)
        .arg("agent")
        .arg("--provider")
        .arg("ollama")
        .arg("--model")
        .arg("llama3.2:latest")
        .arg("--allow-dangerous")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("agent command should spawn");
    let response = read_initialize_response(&mut child);
    terminate_child(child);

    assert_eq!(response["result"]["agentInfo"]["name"], "xzatoma");
    assert_eq!(
        response["result"]["agentCapabilities"]["promptCapabilities"]["image"],
        true
    );
}

#[test]
#[serial]
fn test_agent_command_accepts_working_dir_for_initialize() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());
    let workspace = TempDir::new().expect("workspace temp dir should be created");

    let mut cmd = StdCommand::new(common::xzatoma_binary_path());
    cmd.arg("--config")
        .arg(&config_path)
        .arg("agent")
        .arg("--working-dir")
        .arg(workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("agent command should spawn");
    let response = read_initialize_response(&mut child);
    terminate_child(child);

    assert_eq!(response["result"]["agentInfo"]["name"], "xzatoma");
    assert_eq!(response["result"]["protocolVersion"], 1);
}

#[test]
#[serial]
fn test_agent_command_invalid_provider_reports_error_on_stderr_only() {
    let (_temp_dir, config_path) = temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
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
