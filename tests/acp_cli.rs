use predicates::prelude::*;
use serial_test::serial;
use std::fs;
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
acp:
  enabled: false
  host: "127.0.0.1"
  port: 8765
  compatibility_mode: versioned
  base_path: "/api/v1/acp"
  default_run_mode: async
  persistence:
    enabled: true
    max_events_per_run: 1000
    max_completed_runs: 1000
"#
    .trim_start()
    .to_string()
}

fn root_compatible_config_yaml() -> String {
    r#"
provider:
  type: ollama
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
agent:
  max_turns: 50
  timeout_seconds: 300
acp:
  enabled: false
  host: "0.0.0.0"
  port: 9000
  compatibility_mode: root_compatible
  base_path: "/api/v1/acp"
  default_run_mode: streaming
  persistence:
    enabled: true
    max_events_per_run: 2000
    max_completed_runs: 3000
"#
    .trim_start()
    .to_string()
}

#[test]
#[serial]
fn test_acp_config_command_prints_effective_configuration() {
    let (_temp_dir, config_path) = common::temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("config");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"host\": \"127.0.0.1\""))
        .stdout(predicate::str::contains("\"port\": 8765"))
        .stdout(predicate::str::contains(
            "\"compatibility_mode\": \"versioned\"",
        ))
        .stdout(predicate::str::contains("\"base_path\": \"/api/v1/acp\""))
        .stdout(predicate::str::contains("\"default_run_mode\": \"async\""))
        .stdout(predicate::str::contains("\"enabled\": false"));
}

#[test]
#[serial]
fn test_acp_config_command_reflects_env_overrides() {
    let (_temp_dir, config_path) = common::temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("config")
        .env("XZATOMA_ACP_HOST", "0.0.0.0")
        .env("XZATOMA_ACP_PORT", "9999")
        .env("XZATOMA_ACP_COMPATIBILITY_MODE", "root_compatible")
        .env("XZATOMA_ACP_BASE_PATH", "/acp")
        .env("XZATOMA_ACP_DEFAULT_RUN_MODE", "streaming")
        .env("XZATOMA_ACP_ENABLED", "true");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"host\": \"0.0.0.0\""))
        .stdout(predicate::str::contains("\"port\": 9999"))
        .stdout(predicate::str::contains(
            "\"compatibility_mode\": \"root_compatible\"",
        ))
        .stdout(predicate::str::contains("\"base_path\": \"/acp\""))
        .stdout(predicate::str::contains(
            "\"default_run_mode\": \"streaming\"",
        ))
        .stdout(predicate::str::contains("\"enabled\": true"));
}

#[test]
#[serial]
fn test_acp_validate_command_succeeds_for_valid_configuration() {
    let (_temp_dir, config_path) = common::temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("validate");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "ACP configuration validation succeeded",
        ))
        .stdout(predicate::str::contains(
            "ACP compatibility mode: versioned",
        ));
}

#[test]
#[serial]
fn test_acp_validate_command_succeeds_for_valid_manifest_json() {
    let config_dir = TempDir::new().expect("temp dir should exist");
    let config_path = config_dir.path().join("config.yaml");
    fs::write(&config_path, minimal_config_yaml()).expect("config should write");

    let manifest_path = config_dir.path().join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{
  "name": "xzatoma",
  "version": "0.2.0",
  "displayName": "XZatoma ACP Agent",
  "description": "ACP integration test manifest",
  "capabilities": [
    "manifest_read",
    "runs_create",
    "runs_get",
    "runs_events"
  ],
  "metadata": {
    "implementation": "xzatoma_primary_agent",
    "language": "rust"
  },
  "links": [
    {
      "rel": "self",
      "href": "http://127.0.0.1:8765/api/v1/acp/agents/xzatoma"
    }
  ]
}"#,
    )
    .expect("manifest should write");

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("validate")
        .arg("--manifest")
        .arg(&manifest_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "ACP manifest validation succeeded",
        ))
        .stdout(predicate::str::contains(
            "ACP compatibility mode: versioned",
        ));
}

#[test]
#[serial]
fn test_acp_validate_command_rejects_invalid_manifest_extension() {
    let config_dir = TempDir::new().expect("temp dir should exist");
    let config_path = config_dir.path().join("config.yaml");
    fs::write(&config_path, minimal_config_yaml()).expect("config should write");

    let manifest_path = config_dir.path().join("manifest.txt");
    fs::write(&manifest_path, "not a supported manifest").expect("manifest should write");

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("validate")
        .arg("--manifest")
        .arg(&manifest_path);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Unsupported ACP manifest extension 'txt'; expected .json or .yaml",
    ));
}

#[test]
#[serial]
fn test_acp_validate_command_rejects_invalid_configuration() {
    let invalid_config = r#"
provider:
  type: ollama
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
agent:
  max_turns: 50
  timeout_seconds: 300
acp:
  enabled: false
  host: "127.0.0.1"
  port: 8765
  compatibility_mode: versioned
  base_path: "/"
  default_run_mode: async
  persistence:
    enabled: true
    max_events_per_run: 1000
    max_completed_runs: 1000
"#;

    let (_temp_dir, config_path) = common::temp_config_file(invalid_config);

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("validate");

    cmd.assert().failure().stderr(predicate::str::contains(
        "acp.base_path cannot be '/' in versioned compatibility mode",
    ));
}

#[test]
#[serial]
fn test_acp_runs_command_prints_header_when_no_runs_exist() {
    let storage_dir = TempDir::new().expect("storage temp dir should exist");
    let db_path = storage_dir.path().join("history.db");
    let (_config_dir, config_path) = common::temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("runs")
        .env("XZATOMA_HISTORY_DB", &db_path);

    cmd.assert().success().stdout(predicate::str::contains(
        "run_id\tsession_id\tstate\tcreated_at\tupdated_at",
    ));
}

#[test]
#[serial]
fn test_acp_runs_command_rejects_zero_limit() {
    let storage_dir = TempDir::new().expect("storage temp dir should exist");
    let db_path = storage_dir.path().join("history.db");
    let (_config_dir, config_path) = common::temp_config_file(&minimal_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("runs")
        .arg("--limit")
        .arg("0")
        .env("XZATOMA_HISTORY_DB", &db_path);

    cmd.assert().failure().stderr(predicate::str::contains(
        "ACP run listing limit must be greater than 0",
    ));
}

#[test]
#[serial]
fn test_acp_config_command_supports_root_compatible_configuration() {
    let (_temp_dir, config_path) = common::temp_config_file(&root_compatible_config_yaml());

    let mut cmd = common::xzatoma_command().expect("binary should build");
    cmd.arg("--config")
        .arg(&config_path)
        .arg("acp")
        .arg("config");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"host\": \"0.0.0.0\""))
        .stdout(predicate::str::contains("\"port\": 9000"))
        .stdout(predicate::str::contains(
            "\"compatibility_mode\": \"root_compatible\"",
        ))
        .stdout(predicate::str::contains(
            "\"default_run_mode\": \"streaming\"",
        ))
        .stdout(predicate::str::contains("\"max_events_per_run\": 2000"))
        .stdout(predicate::str::contains("\"max_completed_runs\": 3000"));
}
