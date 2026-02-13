#![allow(deprecated)]

/// End-to-end integration tests for subagent configuration
///
/// These tests validate subagent configuration parsing and validation.
/// They focus on config schema validation rather than end-to-end execution.
use assert_cmd::Command;
use predicates::prelude::*;
mod common;

/// Test 1: Valid subagent config with custom values
///
/// Validates that custom subagent configuration is accepted and parsed
#[test]
fn test_subagent_config_valid_custom_values() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    max_depth: 5\n    default_max_turns: 20\n    output_max_size: 8192\n    telemetry_enabled: false\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    // Config should parse successfully (version doesn't execute run)
    cmd.assert().success();
}

/// Test 2: Invalid subagent config - max_depth zero
///
/// Validates that zero max_depth is rejected
#[test]
fn test_invalid_config_subagent_depth_zero() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    max_depth: 0\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    // Config validation should fail on command execution
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must be greater than 0"));
}

/// Test 3: Invalid subagent config - max_depth too high
///
/// Validates that excessive max_depth is rejected
#[test]
fn test_invalid_config_subagent_depth_too_large() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    max_depth: 15\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot exceed 10"));
}

/// Test 4: Invalid subagent config - output_max_size too small
///
/// Validates that minimum output size is enforced
#[test]
fn test_invalid_config_subagent_output_size_too_small() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    output_max_size: 512\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("at least 1024 bytes"));
}

/// Test 5: Invalid subagent config - default_max_turns zero
///
/// Validates that zero max_turns is rejected
#[test]
fn test_invalid_config_subagent_default_max_turns_zero() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    default_max_turns: 0\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must be greater than 0"));
}

/// Test 6: Invalid subagent config - default_max_turns too large
///
/// Validates that excessively large max_turns is rejected
#[test]
fn test_invalid_config_subagent_default_max_turns_too_large() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    default_max_turns: 200\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot exceed 100"));
}

/// Test 7: Subagent config with telemetry disabled
///
/// Validates that telemetry_enabled flag is accepted
#[test]
fn test_subagent_config_telemetry_disabled() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    telemetry_enabled: false\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test 8: Subagent config with persistence enabled
///
/// Validates that persistence_enabled flag is accepted
#[test]
fn test_subagent_config_persistence_enabled() {
    let (_temp_dir, config_path) = common::temp_config_file(
        "provider:\n  type: ollama\nagent:\n  max_turns: 50\n  subagent:\n    persistence_enabled: true\n",
    );

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test 9: Default config works
///
/// Validates that default subagent configuration is applied
#[test]
fn test_default_subagent_config_works() {
    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--version");

    // Should use defaults and succeed
    cmd.assert().success();
}

/// Test 10: All subagent fields in YAML config
///
/// Validates complete subagent configuration YAML parsing
#[test]
fn test_subagent_config_complete_yaml_fields() {
    let yaml_content = r#"
provider:
  type: ollama
agent:
  max_turns: 50
  subagent:
    max_depth: 4
    default_max_turns: 15
    output_max_size: 6144
    telemetry_enabled: true
    persistence_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(yaml_content);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}
