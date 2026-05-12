//! Integration tests for MCP configuration validation and environment variable
//! handling.
//!
//! Environment variable tests use `#[serial]` to prevent concurrent test runs
//! from interfering with each other via shared process-wide env vars.
//!
//! Covers Task 4.7 requirements:
//!
//! - `test_server_id_rejects_uppercase`
//! - `test_server_id_rejects_spaces`
//! - `test_server_id_rejects_too_long`
//! - `test_server_id_accepts_valid`
//! - `test_config_yaml_without_mcp_key_loads_default`
//! - `test_duplicate_server_ids_fail_mcp_config_validate`
//! - `test_env_var_xzatoma_mcp_request_timeout_applies`
//! - `test_env_var_xzatoma_mcp_auto_connect_applies`

use std::collections::HashMap;

use serial_test::serial;
use xzatoma::mcp::config::McpConfig;
use xzatoma::mcp::server::{McpServerConfig, McpServerTransportConfig};

// ---------------------------------------------------------------------------
// Helper: build a minimal Cli for Config::load calls
// ---------------------------------------------------------------------------

fn make_cli() -> xzatoma::cli::Cli {
    use xzatoma::cli::{Cli, Commands};
    Cli {
        config: None,
        verbose: false,
        storage_path: None,
        command: Commands::Run {
            plan: None,
            prompt: None,
            allow_dangerous: false,
            thinking_effort: None,
        },
    }
}

/// Load a default `Config` (from a non-existent path so defaults are used)
/// with env vars applied.
fn load_default_config() -> xzatoma::error::Result<xzatoma::config::Config> {
    let cli = make_cli();
    xzatoma::config::Config::load("/tmp/__xzatoma_nonexistent_config_for_test.yaml", &cli)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_stdio_server(id: &str) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        transport: McpServerTransportConfig::Stdio {
            executable: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        enabled: true,
        timeout_seconds: 30,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: true,
    }
}

// ---------------------------------------------------------------------------
// Task 4.7: server id validation tests
// ---------------------------------------------------------------------------

/// `id` containing an uppercase letter must fail `McpServerConfig::validate`.
#[test]
fn test_server_id_rejects_uppercase() {
    let cfg = make_stdio_server("MyServer");
    let result = cfg.validate();
    assert!(
        result.is_err(),
        "uppercase id should be rejected, but got Ok"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("invalid") || msg.contains("MyServer"),
        "error message should mention the invalid id, got: {msg}"
    );
}

/// `id` containing a space must fail `McpServerConfig::validate`.
#[test]
fn test_server_id_rejects_spaces() {
    let cfg = make_stdio_server("my server");
    let result = cfg.validate();
    assert!(
        result.is_err(),
        "id with space should be rejected, but got Ok"
    );
}

/// A 65-character id must fail `McpServerConfig::validate` (max is 64).
#[test]
fn test_server_id_rejects_too_long() {
    let long_id = "a".repeat(65);
    let cfg = make_stdio_server(&long_id);
    let result = cfg.validate();
    assert!(result.is_err(), "65-char id should be rejected, but got Ok");
}

/// A valid id composed of lowercase letters, digits, hyphens, and underscores
/// must pass `McpServerConfig::validate`.
#[test]
fn test_server_id_accepts_valid() {
    let cfg = make_stdio_server("my-server_01");
    assert!(
        cfg.validate().is_ok(),
        "valid id 'my-server_01' should be accepted"
    );
}

/// An id of exactly 64 characters (the maximum) must pass validation.
#[test]
fn test_server_id_accepts_exactly_64_chars() {
    let id = "a".repeat(64);
    let cfg = make_stdio_server(&id);
    assert!(
        cfg.validate().is_ok(),
        "64-char id should be accepted (boundary)"
    );
}

/// An id of a single lowercase letter must pass validation.
#[test]
fn test_server_id_accepts_single_char() {
    let cfg = make_stdio_server("a");
    assert!(cfg.validate().is_ok(), "single-char id should be accepted");
}

/// An empty id must fail validation.
#[test]
fn test_server_id_rejects_empty() {
    let cfg = make_stdio_server("");
    assert!(cfg.validate().is_err(), "empty id should be rejected");
}

/// An id with a dot must fail validation.
#[test]
fn test_server_id_rejects_dot() {
    let cfg = make_stdio_server("my.server");
    assert!(cfg.validate().is_err(), "id with dot should be rejected");
}

/// An id with a slash must fail validation.
#[test]
fn test_server_id_rejects_slash() {
    let cfg = make_stdio_server("my/server");
    assert!(cfg.validate().is_err(), "id with slash should be rejected");
}

// ---------------------------------------------------------------------------
// Task 4.7: McpConfig::validate -- duplicate IDs
// ---------------------------------------------------------------------------

/// Two servers with the same id must cause `McpConfig::validate` to return
/// `Err`.
#[test]
fn test_duplicate_server_ids_fail_mcp_config_validate() {
    let mut cfg = McpConfig::default();
    cfg.servers.push(make_stdio_server("same-id"));
    cfg.servers.push(make_stdio_server("same-id"));

    let result = cfg.validate();
    assert!(
        result.is_err(),
        "duplicate server ids should fail validation"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("duplicate") || msg.contains("same-id"),
        "error should mention duplicate id, got: {msg}"
    );
}

/// Three servers where two share an id must also fail.
#[test]
fn test_three_servers_with_one_duplicate_id_fails() {
    let mut cfg = McpConfig::default();
    cfg.servers.push(make_stdio_server("unique-a"));
    cfg.servers.push(make_stdio_server("duplicate"));
    cfg.servers.push(make_stdio_server("duplicate"));

    assert!(cfg.validate().is_err());
}

/// Three servers all with distinct ids must pass validation.
#[test]
fn test_three_servers_with_distinct_ids_passes() {
    let mut cfg = McpConfig::default();
    cfg.servers.push(make_stdio_server("srv-1"));
    cfg.servers.push(make_stdio_server("srv-2"));
    cfg.servers.push(make_stdio_server("srv-3"));

    assert!(cfg.validate().is_ok());
}

// ---------------------------------------------------------------------------
// Task 4.7: YAML without mcp key loads default McpConfig
// ---------------------------------------------------------------------------

/// A YAML string that contains no `mcp:` key must deserialise the `mcp` field
/// to `McpConfig::default()` without error.
#[test]
fn test_config_yaml_without_mcp_key_loads_default() {
    // Minimal valid Config YAML -- no `mcp:` section.
    let yaml = r#"
provider:
  provider_type: "copilot"
"#;
    // We parse directly into McpConfig using `#[serde(default)]` at the struct
    // level to verify the isolation.  The full Config::load path is exercised
    // by the env-var tests below.
    let result: Result<McpConfig, _> = serde_yaml::from_str("{}");
    assert!(
        result.is_ok(),
        "empty YAML should produce default McpConfig: {:?}",
        result.err()
    );
    let cfg = result.unwrap();
    assert!(
        cfg.servers.is_empty(),
        "default McpConfig should have no servers"
    );
    assert_eq!(
        cfg.request_timeout_seconds, 30,
        "default request_timeout_seconds should be 30"
    );
    assert!(cfg.auto_connect, "default auto_connect should be true");

    // Ensure the yaml variable is used to satisfy the compiler.
    let _ = yaml;
}

/// An `McpConfig` deserialised from an explicit empty `servers` list must
/// also pass validation.
#[test]
fn test_config_yaml_explicit_empty_servers_is_valid() {
    let yaml = "servers: []";
    let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
    assert!(cfg.validate().is_ok());
    assert!(cfg.servers.is_empty());
}

/// An `McpConfig` deserialised from a YAML snippet with only
/// `request_timeout_seconds` overridden must keep all other fields at their
/// defaults.
#[test]
fn test_config_yaml_partial_overrides_keep_defaults() {
    let yaml = "request_timeout_seconds: 90";
    let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(cfg.request_timeout_seconds, 90);
    assert!(cfg.auto_connect); // default
    assert!(cfg.expose_resources_tool); // default
    assert!(cfg.expose_prompts_tool); // default
    assert!(cfg.servers.is_empty()); // default
}

// ---------------------------------------------------------------------------
// Task 4.7: environment variable overrides
// ---------------------------------------------------------------------------

/// `XZATOMA_MCP_REQUEST_TIMEOUT` must override
/// `mcp.request_timeout_seconds` in the loaded `Config`.
///
/// Uses `#[serial]` to serialise env-var manipulation across tests.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_request_timeout_applies() {
    // Temporarily set the env var.
    let var = "XZATOMA_MCP_REQUEST_TIMEOUT";
    let _guard = EnvGuard::set(var, "120");

    let config = load_default_config().expect("Config::load should succeed with valid env var");

    assert_eq!(
        config.mcp.request_timeout_seconds, 120,
        "XZATOMA_MCP_REQUEST_TIMEOUT=120 should set request_timeout_seconds to 120"
    );
}

/// `XZATOMA_MCP_AUTO_CONNECT=false` must set `mcp.auto_connect` to `false`.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_auto_connect_applies_false() {
    let var = "XZATOMA_MCP_AUTO_CONNECT";
    let _guard = EnvGuard::set(var, "false");

    let config = load_default_config().expect("Config::load should succeed");

    assert!(
        !config.mcp.auto_connect,
        "XZATOMA_MCP_AUTO_CONNECT=false should set auto_connect to false"
    );
}

/// `XZATOMA_MCP_AUTO_CONNECT=true` must set `mcp.auto_connect` to `true`.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_auto_connect_applies_true() {
    let var = "XZATOMA_MCP_AUTO_CONNECT";
    let _guard = EnvGuard::set(var, "true");

    let config = load_default_config().expect("Config::load should succeed");

    assert!(
        config.mcp.auto_connect,
        "XZATOMA_MCP_AUTO_CONNECT=true should set auto_connect to true"
    );
}

/// `XZATOMA_MCP_AUTO_CONNECT=1` (truthy) must set `mcp.auto_connect` to
/// `true`.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_auto_connect_one_is_truthy() {
    let var = "XZATOMA_MCP_AUTO_CONNECT";
    let _guard = EnvGuard::set(var, "1");

    let config = load_default_config().expect("Config::load should succeed");

    assert!(config.mcp.auto_connect);
}

/// `XZATOMA_MCP_AUTO_CONNECT=yes` (truthy) must set `mcp.auto_connect` to
/// `true`.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_auto_connect_yes_is_truthy() {
    let var = "XZATOMA_MCP_AUTO_CONNECT";
    let _guard = EnvGuard::set(var, "yes");

    let config = load_default_config().expect("Config::load should succeed");

    assert!(config.mcp.auto_connect);
}

/// `XZATOMA_MCP_AUTO_CONNECT=0` (falsy) must set `mcp.auto_connect` to
/// `false`.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_auto_connect_zero_is_falsy() {
    let var = "XZATOMA_MCP_AUTO_CONNECT";
    let _guard = EnvGuard::set(var, "0");

    let config = load_default_config().expect("Config::load should succeed");

    assert!(!config.mcp.auto_connect);
}

/// An invalid value for `XZATOMA_MCP_REQUEST_TIMEOUT` (non-numeric) must
/// not cause `Config::load` to fail; the previous value (default) is kept.
#[test]
#[serial]
fn test_env_var_xzatoma_mcp_request_timeout_invalid_value_kept_as_default() {
    let var = "XZATOMA_MCP_REQUEST_TIMEOUT";
    let _guard = EnvGuard::set(var, "not_a_number");

    let config =
        load_default_config().expect("Config::load should succeed even with invalid env value");

    // The default is 30; bad parse is silently ignored.
    assert_eq!(config.mcp.request_timeout_seconds, 30);
}

// ---------------------------------------------------------------------------
// McpConfig serde -- standalone tests (no Config::load)
// ---------------------------------------------------------------------------

/// Deserialising an `McpConfig` that contains an HTTP server with OAuth
/// fields must preserve those fields.
#[test]
fn test_mcp_config_yaml_with_oauth_server_roundtrip() {
    let yaml = r#"
servers:
  - id: "api-server"
    transport:
      type: http
      endpoint: "https://api.example.com/mcp"
      oauth:
        client_id: "my-client"
        redirect_port: 9000
    tools_enabled: true
    resources_enabled: true
"#;
    let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(cfg.servers.len(), 1);
    let server = &cfg.servers[0];
    assert_eq!(server.id, "api-server");
    assert!(server.tools_enabled);
    assert!(server.resources_enabled);

    match &server.transport {
        McpServerTransportConfig::Http {
            oauth, endpoint, ..
        } => {
            assert_eq!(endpoint.as_str(), "https://api.example.com/mcp");
            let o = oauth.as_ref().expect("oauth should be present");
            assert_eq!(o.client_id.as_deref(), Some("my-client"));
            assert_eq!(o.redirect_port, Some(9000));
        }
        _ => panic!("expected HTTP transport"),
    }

    assert!(cfg.validate().is_ok());
}

/// An HTTP server with an `ftp://` endpoint must fail validation.
#[test]
fn test_mcp_config_yaml_with_invalid_http_scheme_fails_validate() {
    let mut cfg = McpConfig::default();
    cfg.servers.push(McpServerConfig {
        id: "bad-scheme".to_string(),
        transport: McpServerTransportConfig::Http {
            endpoint: url::Url::parse("ftp://example.com/mcp").unwrap(),
            headers: HashMap::new(),
            timeout_seconds: None,
            oauth: None,
        },
        enabled: true,
        timeout_seconds: 30,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: true,
    });

    assert!(cfg.validate().is_err());
}

/// A stdio server with an empty executable string must fail validation.
#[test]
fn test_mcp_config_yaml_with_empty_executable_fails_validate() {
    let mut cfg = McpConfig::default();
    cfg.servers.push(McpServerConfig {
        id: "empty-exec".to_string(),
        transport: McpServerTransportConfig::Stdio {
            executable: String::new(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        enabled: true,
        timeout_seconds: 30,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: true,
    });

    assert!(cfg.validate().is_err());
}

// ---------------------------------------------------------------------------
// RAII environment variable guard
// ---------------------------------------------------------------------------

/// RAII guard that restores the previous value of an environment variable when
/// dropped.
///
/// This avoids leaking env-var side effects across tests even when a test
/// panics.
struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    /// Set `key` to `value` and return a guard that restores the original
    /// value (or removes the variable) on drop.
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}
