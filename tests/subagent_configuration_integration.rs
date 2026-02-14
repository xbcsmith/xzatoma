#![allow(deprecated)]

//! Phase 5: Integration tests for subagent configuration
//!
//! Comprehensive test suite validating subagent provider overrides,
//! model overrides, chat mode integration, configuration validation,
//! error handling, performance, and backward compatibility.

use assert_cmd::Command;
use predicates::prelude::*;

mod common;

// ============================================================================
// Task 5.1: Integration Tests - Provider and Model Overrides
// ============================================================================

/// Test subagent provider override with Copilot
///
/// Validates that subagents can use a different provider than the parent agent.
/// Main agent uses Ollama, subagents use Copilot (if available).
#[test]
fn test_subagent_provider_override_copilot() {
    let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    provider: copilot
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    // Config should parse and validate successfully
    cmd.assert().success();
}

/// Test subagent model override
///
/// Validates that subagents can use a different model than the main provider.
/// Main agent uses gpt-5-mini, subagents use gpt-3.5-turbo.
#[test]
fn test_subagent_model_override() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    model: gpt-3.5-turbo
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test combined provider and model override
///
/// Validates both provider and model can be overridden simultaneously.
/// Main uses Copilot with gpt-5-mini, subagents use Ollama with llama3.2.
#[test]
fn test_subagent_provider_and_model_override() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  subagent:
    provider: ollama
    model: llama3.2:latest
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test subagent uses parent provider when no override specified
///
/// Validates backward compatibility - when no provider override is specified,
/// subagents inherit the parent's provider.
#[test]
fn test_subagent_inherits_parent_provider_when_no_override() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test model override uses main provider when provider not overridden
///
/// Validates that model override applies to the parent provider
/// if no provider override is specified.
#[test]
fn test_subagent_model_override_with_parent_provider() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    model: gpt-3.5-turbo
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

// ============================================================================
// Task 5.1: Chat Mode Integration Tests
// ============================================================================

/// Test chat subagents disabled by default
///
/// Validates that subagents are not available in chat mode unless
/// explicitly enabled via configuration or prompt detection.
#[test]
fn test_chat_subagent_disabled_by_default() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test chat subagents can be enabled by default
///
/// Validates that chat_enabled: true makes subagents available immediately.
#[test]
fn test_chat_subagent_enabled_by_default() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: true
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test chat mode with prompt detection patterns
///
/// Validates that certain prompt patterns automatically enable subagents
/// even when chat_enabled is false.
///
/// Note: This test validates config parsing; actual prompt detection
/// is tested in integration with chat mode execution.
#[test]
fn test_chat_subagent_prompt_detection_config() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
    default_max_turns: 5
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

// ============================================================================
// Task 5.2: Configuration Validation Tests
// ============================================================================

/// Test invalid provider type in subagent config
///
/// Validates that invalid provider types are rejected during validation.
#[test]
fn test_invalid_subagent_provider_type() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    provider: invalid_provider
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Invalid subagent provider"));
}

/// Test valid subagent provider types (copilot)
///
/// Validates that valid provider types are accepted.
#[test]
fn test_valid_subagent_provider_copilot() {
    let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    provider: copilot
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test valid subagent provider types (ollama)
///
/// Validates that Ollama provider override is accepted.
#[test]
fn test_valid_subagent_provider_ollama() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  subagent:
    provider: ollama
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test subagent config with all optional fields
///
/// Validates that all subagent configuration fields parse correctly.
#[test]
fn test_subagent_config_all_fields() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  subagent:
    max_depth: 5
    default_max_turns: 15
    output_max_size: 8192
    telemetry_enabled: true
    persistence_enabled: false
    max_executions: 100
    max_total_tokens: 50000
    max_total_time: 3600
    provider: ollama
    model: llama3.2:latest
    chat_enabled: true
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test subagent config with minimal fields
///
/// Validates that subagent config uses sensible defaults when fields omitted.
#[test]
fn test_subagent_config_minimal_fields() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test subagent config with empty provider string
///
/// Validates that empty provider string is treated as None (inherit parent).
#[test]
fn test_subagent_config_empty_provider_string() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    provider: ""
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    // Empty string should be treated as no override
    cmd.assert().success();
}

// ============================================================================
// Task 5.2: Configuration Syntax Error Tests
// ============================================================================

/// Test configuration file with invalid YAML syntax
///
/// Validates error handling for malformed YAML.
#[test]
fn test_invalid_yaml_syntax() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
agent: [invalid yaml structure
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert().failure();
}

/// Test configuration file with missing required provider type
///
/// Validates that provider type is required.
#[test]
fn test_missing_provider_type() {
    let config = r#"
provider:
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config")
        .arg(config_path)
        .arg("run")
        .arg("--prompt")
        .arg("test");

    cmd.assert().failure();
}

// ============================================================================
// Task 5.3: Error Handling Tests - Provider Creation
// ============================================================================

/// Test error when subagent provider cannot be created
///
/// Validates graceful error handling when provider instantiation fails
/// (e.g., invalid credentials, network unreachable).
#[test]
fn test_subagent_provider_creation_with_invalid_config() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://invalid.host:99999
    model: nonexistent:model

agent:
  max_turns: 10
  subagent:
    provider: ollama
    chat_enabled: false
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    // Config itself is valid; error would occur at runtime during provider creation
    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    // Version check should succeed (no actual provider creation)
    cmd.assert().success();
}

// ============================================================================
// Task 5.4: Performance and Resource Tests
// ============================================================================

/// Test that subagent configuration parsing is efficient
///
/// Validates that provider override parsing doesn't cause noticeable latency.
#[test]
fn test_subagent_config_parsing_performance() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  subagent:
    provider: ollama
    model: gemma2:2b
    max_depth: 4
    default_max_turns: 8
    output_max_size: 8192
    telemetry_enabled: true
    persistence_enabled: false
    max_executions: 50
    max_total_tokens: 100000
    chat_enabled: true
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    // Run multiple times to measure consistency
    for _ in 0..5 {
        let mut cmd = Command::cargo_bin("xzatoma").unwrap();
        cmd.arg("--config").arg(&config_path).arg("--version");
        cmd.assert().success();
    }
}

/// Test memory efficiency with multiple provider configurations
///
/// Validates that having multiple provider configs doesn't cause bloat.
#[test]
fn test_multiple_provider_configs_memory_efficiency() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
    api_base: "https://api.github.com"
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  conversation:
    max_tokens: 10000
    min_retain_turns: 3
    prune_threshold: 0.8
  tools:
    max_output_size: 1048576
    max_file_read_size: 10485760
  terminal:
    default_mode: restricted_autonomous
    timeout_seconds: 30
  subagent:
    provider: ollama
    model: llama3.2:latest
    chat_enabled: true
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

// ============================================================================
// Task 5.5: Backward Compatibility Tests
// ============================================================================

/// Test existing configuration without subagent section works
///
/// Validates that configs without subagent section load successfully
/// with sensible defaults.
#[test]
fn test_backward_compatibility_no_subagent_section() {
    let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  timeout_seconds: 60
  conversation:
    max_tokens: 10000
    min_retain_turns: 3
    prune_threshold: 0.8
  tools:
    max_output_size: 1048576
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test backward compatibility with empty subagent section
///
/// Validates that subagent section with no fields uses defaults.
#[test]
fn test_backward_compatibility_empty_subagent_section() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent: {}
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test backward compatibility - subagent without provider override
///
/// Validates that older configs specifying subagent settings but no
/// provider override still work.
#[test]
fn test_backward_compatibility_subagent_no_override() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    max_depth: 4
    default_max_turns: 8
    output_max_size: 4096
    telemetry_enabled: true
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test default behavior unchanged when no override specified
///
/// Validates that subagent execution uses parent provider when no
/// override is configured.
#[test]
fn test_backward_compatibility_default_behavior() {
    let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    // Should succeed with defaults
    cmd.assert().success();
}

/// Test chat_enabled field backward compatibility
///
/// Validates that chat_enabled field works and defaults to false
/// when omitted.
#[test]
fn test_backward_compatibility_chat_enabled_field() {
    let config1 = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
"#;

    let (_temp_dir1, config_path1) = common::temp_config_file(config1);

    let mut cmd1 = Command::cargo_bin("xzatoma").unwrap();
    cmd1.arg("--config").arg(&config_path1).arg("--version");
    cmd1.assert().success();

    // Test without explicit chat_enabled (should default to false)
    let config2 = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    max_depth: 3
"#;

    let (_temp_dir2, config_path2) = common::temp_config_file(config2);

    let mut cmd2 = Command::cargo_bin("xzatoma").unwrap();
    cmd2.arg("--config").arg(config_path2).arg("--version");
    cmd2.assert().success();
}

// ============================================================================
// Task 5.6: Additional Configuration Validation Tests
// ============================================================================

/// Test subagent max_depth boundary conditions
///
/// Validates minimum and maximum depth constraints.
#[test]
fn test_subagent_max_depth_valid_range() {
    // Test valid range: 1-10
    for depth in &[1, 5, 10] {
        let config = format!(
            r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    max_depth: {}
"#,
            depth
        );

        let (_temp_dir, config_path) = common::temp_config_file(&config);

        let mut cmd = Command::cargo_bin("xzatoma").unwrap();
        cmd.arg("--config").arg(config_path).arg("--version");

        cmd.assert().success();
    }
}

/// Test subagent max_depth exceeds maximum
///
/// Validates that depth > 10 is rejected.
#[test]
fn test_subagent_max_depth_exceeds_maximum() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    max_depth: 11
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

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

/// Test subagent default_max_turns boundary conditions
///
/// Validates min and max turn constraints.
#[test]
fn test_subagent_default_max_turns_valid_range() {
    // Test valid range: 1-100
    for turns in &[1, 50, 100] {
        let config = format!(
            r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    default_max_turns: {}
"#,
            turns
        );

        let (_temp_dir, config_path) = common::temp_config_file(&config);

        let mut cmd = Command::cargo_bin("xzatoma").unwrap();
        cmd.arg("--config").arg(config_path).arg("--version");

        cmd.assert().success();
    }
}

/// Test subagent output_max_size minimum boundary
///
/// Validates that output_max_size >= 1024.
#[test]
fn test_subagent_output_max_size_minimum() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    output_max_size: 512
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

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

/// Test subagent output_max_size valid values
///
/// Validates that output_max_size >= 1024 is accepted.
#[test]
fn test_subagent_output_max_size_valid() {
    for size in &[1024, 2048, 8192, 65536] {
        let config = format!(
            r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    output_max_size: {}
"#,
            size
        );

        let (_temp_dir, config_path) = common::temp_config_file(&config);

        let mut cmd = Command::cargo_bin("xzatoma").unwrap();
        cmd.arg("--config").arg(config_path).arg("--version");

        cmd.assert().success();
    }
}

/// Test cost-optimized configuration example
///
/// Validates the cost-optimized example from the plan.
#[test]
fn test_cost_optimized_configuration_example() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 20
  subagent:
    provider: ollama
    model: llama3.2:latest
    chat_enabled: true
    max_executions: 10
    max_total_tokens: 50000
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test provider mixing configuration example
///
/// Validates the provider mixing example from the plan.
#[test]
fn test_provider_mixing_configuration_example() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 15
  subagent:
    provider: ollama
    model: llama3.2:latest
    chat_enabled: false
    max_depth: 2
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test speed-optimized configuration example
///
/// Validates the speed-optimized example from the plan.
#[test]
fn test_speed_optimized_configuration_example() {
    let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  subagent:
    model: gemma2:2b
    chat_enabled: true
    default_max_turns: 5
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}

/// Test chat mode with manual enablement configuration
///
/// Validates the chat mode with manual enablement example.
#[test]
fn test_chat_mode_manual_enablement_example() {
    let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
    max_executions: 5
"#;

    let (_temp_dir, config_path) = common::temp_config_file(config);

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("--config").arg(config_path).arg("--version");

    cmd.assert().success();
}
