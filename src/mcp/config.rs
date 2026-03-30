//! MCP client configuration types
//!
//! This module defines [`McpConfig`], the top-level configuration structure
//! for all MCP server connections. It is embedded in [`crate::config::Config`]
//! under the `mcp:` YAML key.
//!
//! # Defaults
//!
//! All fields have sensible defaults so that existing configuration files that
//! omit the `mcp:` key continue to deserialize without error. The struct
//! derives [`Default`] and is annotated with `#[serde(default)]` at the
//! struct level.
//!
//! # Validation
//!
//! Call [`McpConfig::validate`] (or rely on [`crate::config::Config::validate`]
//! which calls it automatically) to catch duplicate server IDs and
//! per-server configuration errors before the application starts.

use serde::{Deserialize, Serialize};

use crate::error::{Result, XzatomaError};
use crate::mcp::server::McpServerConfig;

// ---------------------------------------------------------------------------
// Private defaults
// ---------------------------------------------------------------------------

fn default_request_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// McpConfig
// ---------------------------------------------------------------------------

/// Top-level MCP client configuration.
///
/// Holds the list of MCP servers to connect to and global policy flags
/// that apply to all servers.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::config::McpConfig;
///
/// // Default configuration is valid and has no servers.
/// let cfg = McpConfig::default();
/// assert!(cfg.servers.is_empty());
/// assert!(cfg.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// List of MCP servers to connect to.
    ///
    /// Each entry describes a single server with its transport, capability
    /// flags, and optional OAuth configuration. IDs must be unique within
    /// this list.
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,

    /// Default timeout in seconds for individual MCP requests.
    ///
    /// Can be overridden per-server via [`McpServerConfig::timeout_seconds`].
    /// Overridable at runtime via the `XZATOMA_MCP_REQUEST_TIMEOUT` env var.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,

    /// Automatically connect to all enabled servers on startup.
    ///
    /// When `false` servers must be connected explicitly via the `mcp connect`
    /// command. Overridable at runtime via the `XZATOMA_MCP_AUTO_CONNECT` env
    /// var.
    #[serde(default = "default_true")]
    pub auto_connect: bool,

    /// Expose a synthetic `mcp_resources` tool that lists and reads resources
    /// from all connected servers.
    #[serde(default = "default_true")]
    pub expose_resources_tool: bool,

    /// Expose a synthetic `mcp_prompts` tool that lists and retrieves prompts
    /// from all connected servers.
    #[serde(default = "default_true")]
    pub expose_prompts_tool: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            request_timeout_seconds: 30,
            auto_connect: true,
            expose_resources_tool: true,
            expose_prompts_tool: true,
        }
    }
}

impl McpConfig {
    /// Validate the MCP client configuration.
    ///
    /// Checks:
    ///
    /// 1. No two entries in [`servers`][Self::servers] share the same `id`.
    /// 2. Each server entry passes its own [`McpServerConfig::validate`].
    ///
    /// # Returns
    ///
    /// `Ok(())` when all checks pass.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Config`] for duplicate server IDs or any
    /// per-server validation failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::config::McpConfig;
    /// use xzatoma::mcp::server::{McpServerConfig, McpServerTransportConfig};
    ///
    /// let mut cfg = McpConfig::default();
    /// cfg.servers.push(McpServerConfig {
    ///     id: "fs".to_string(),
    ///     transport: McpServerTransportConfig::Stdio {
    ///         executable: "npx".to_string(),
    ///         args: vec![],
    ///         env: std::collections::HashMap::new(),
    ///         working_dir: None,
    ///     },
    ///     enabled: true,
    ///     timeout_seconds: 30,
    ///     tools_enabled: true,
    ///     resources_enabled: false,
    ///     prompts_enabled: false,
    ///     sampling_enabled: false,
    ///     elicitation_enabled: true,
    /// });
    ///
    /// assert!(cfg.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        // Rule 1: check for duplicate IDs.
        let mut seen = std::collections::HashSet::new();
        for server in &self.servers {
            if !seen.insert(server.id.as_str()) {
                return Err(XzatomaError::Config(format!(
                    "duplicate MCP server id: {}",
                    server.id
                )));
            }
        }

        // Rule 2: validate each individual server entry.
        for server in &self.servers {
            server.validate()?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::server::{McpServerTransportConfig, OAuthServerConfig};
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Default
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_has_no_servers() {
        let cfg = McpConfig::default();
        assert!(cfg.servers.is_empty());
    }

    #[test]
    fn test_default_request_timeout_is_30() {
        let cfg = McpConfig::default();
        assert_eq!(cfg.request_timeout_seconds, 30);
    }

    #[test]
    fn test_default_auto_connect_is_true() {
        let cfg = McpConfig::default();
        assert!(cfg.auto_connect);
    }

    #[test]
    fn test_default_expose_resources_tool_is_true() {
        let cfg = McpConfig::default();
        assert!(cfg.expose_resources_tool);
    }

    #[test]
    fn test_default_expose_prompts_tool_is_true() {
        let cfg = McpConfig::default();
        assert!(cfg.expose_prompts_tool);
    }

    // -----------------------------------------------------------------------
    // validate -- empty list
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_empty_servers_list_is_ok() {
        let cfg = McpConfig::default();
        assert!(cfg.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // validate -- duplicate IDs
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_duplicate_server_ids_returns_error() {
        let mut cfg = McpConfig::default();
        cfg.servers.push(make_stdio_server("dup-server"));
        cfg.servers.push(make_stdio_server("dup-server"));
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate MCP server id"));
        assert!(err.to_string().contains("dup-server"));
    }

    #[test]
    fn test_validate_distinct_server_ids_pass() {
        let mut cfg = McpConfig::default();
        cfg.servers.push(make_stdio_server("server-a"));
        cfg.servers.push(make_stdio_server("server-b"));
        assert!(cfg.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // validate -- per-server propagation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_propagates_server_id_error() {
        let mut cfg = McpConfig::default();
        // Invalid id -- uppercase letter
        cfg.servers.push(make_stdio_server("BadId"));
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_propagates_stdio_empty_executable_error() {
        let mut cfg = McpConfig::default();
        cfg.servers.push(McpServerConfig {
            id: "good-id".to_string(),
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

    // -----------------------------------------------------------------------
    // Serde -- missing mcp key deserialises to default
    // -----------------------------------------------------------------------

    #[test]
    fn test_yaml_without_mcp_key_gives_default() {
        // Simulate a config snippet that has no `mcp:` key.
        let yaml = "servers: []";
        let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
        assert!(cfg.servers.is_empty());
        assert_eq!(cfg.request_timeout_seconds, 30);
        assert!(cfg.auto_connect);
    }

    #[test]
    fn test_yaml_empty_string_gives_default() {
        // An empty YAML document should produce the default McpConfig.
        let cfg: McpConfig = serde_yaml::from_str("{}").expect("should deserialize");
        assert!(cfg.servers.is_empty());
    }

    #[test]
    fn test_yaml_partial_overrides_only_specified_fields() {
        let yaml = "request_timeout_seconds: 60\nauto_connect: false";
        let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
        assert_eq!(cfg.request_timeout_seconds, 60);
        assert!(!cfg.auto_connect);
        // Unspecified fields keep their defaults.
        assert!(cfg.expose_resources_tool);
        assert!(cfg.expose_prompts_tool);
        assert!(cfg.servers.is_empty());
    }

    #[test]
    fn test_yaml_with_one_stdio_server_parses_correctly() {
        let yaml = r#"
servers:
  - id: "fs"
    transport:
      type: stdio
      executable: "npx"
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
"#;
        let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].id, "fs");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_yaml_with_http_server_with_oauth_parses_correctly() {
        let yaml = r#"
servers:
  - id: "remote"
    transport:
      type: http
      endpoint: "https://api.example.com/mcp"
      oauth:
        client_id: "my-client"
        redirect_port: 8080
"#;
        let cfg: McpConfig = serde_yaml::from_str(yaml).expect("should deserialize");
        assert_eq!(cfg.servers.len(), 1);
        assert_eq!(cfg.servers[0].id, "remote");
        match &cfg.servers[0].transport {
            McpServerTransportConfig::Http { oauth, .. } => {
                let oauth = oauth.as_ref().expect("oauth should be Some");
                assert_eq!(oauth.client_id.as_deref(), Some("my-client"));
                assert_eq!(oauth.redirect_port, Some(8080));
            }
            _ => panic!("expected Http transport"),
        }
        assert!(cfg.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // Serde round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_roundtrip_yaml_preserves_all_fields() {
        let mut original = McpConfig {
            servers: vec![make_stdio_server("srv-1")],
            request_timeout_seconds: 45,
            auto_connect: false,
            expose_resources_tool: false,
            expose_prompts_tool: false,
        };
        original.servers.push(make_http_server("srv-2"));

        let yaml = serde_yaml::to_string(&original).expect("serialize");
        let restored: McpConfig = serde_yaml::from_str(&yaml).expect("deserialize");

        assert_eq!(restored.servers.len(), 2);
        assert_eq!(restored.request_timeout_seconds, 45);
        assert!(!restored.auto_connect);
        assert!(!restored.expose_resources_tool);
        assert!(!restored.expose_prompts_tool);
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

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

    fn make_http_server(id: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            transport: McpServerTransportConfig::Http {
                endpoint: url::Url::parse("https://api.example.com/mcp").unwrap(),
                headers: HashMap::new(),
                timeout_seconds: None,
                oauth: Some(OAuthServerConfig {
                    client_id: Some("cid".to_string()),
                    client_secret: None,
                    redirect_port: None,
                    metadata_url: None,
                }),
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
}
