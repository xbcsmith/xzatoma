//! MCP server configuration types
//!
//! This module defines per-server connection descriptors and transport
//! configuration for MCP client connections. Each [`McpServerConfig`]
//! describes one MCP server that the client should connect to.
//!
//! # Transport Variants
//!
//! - [`McpServerTransportConfig::Stdio`] -- launch a local subprocess and
//!   communicate over its stdin/stdout pipes.
//! - [`McpServerTransportConfig::Http`] -- connect to a remote server via
//!   Streamable HTTP/SSE, optionally protected by OAuth 2.1.
//!
//! # Validation
//!
//! Call [`McpServerConfig::validate`] before using a config value. The
//! top-level [`crate::mcp::config::McpConfig::validate`] calls this
//! automatically for every entry in the `servers` list.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, XzatomaError};

// ---------------------------------------------------------------------------
// Private defaults
// ---------------------------------------------------------------------------

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30
}

// ---------------------------------------------------------------------------
// OAuthServerConfig
// ---------------------------------------------------------------------------

/// OAuth 2.1 configuration overrides for a single HTTP MCP server.
///
/// All fields are optional. When absent the authorization module falls back to
/// dynamic client registration (RFC 7591) or browser-based authorization code
/// flow.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::server::OAuthServerConfig;
///
/// let cfg = OAuthServerConfig {
///     client_id: Some("my-client-id".to_string()),
///     client_secret: None,
///     redirect_port: Some(8080),
///     metadata_url: None,
/// };
/// assert_eq!(cfg.client_id.as_deref(), Some("my-client-id"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthServerConfig {
    /// Static OAuth client ID.
    ///
    /// When provided, dynamic client registration is skipped and this value
    /// is sent directly to the token endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Static OAuth client secret (confidential clients only).
    ///
    /// Public clients (PKCE-only) should leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// Local TCP port for the OAuth redirect callback listener.
    ///
    /// Defaults to `0` (OS-assigned) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_port: Option<u16>,

    /// Override URL for the authorization server's `.well-known` discovery
    /// document.
    ///
    /// When `None` the standard discovery paths derived from the resource
    /// endpoint are tried automatically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_url: Option<String>,
}

// ---------------------------------------------------------------------------
// MCP approval policy
// ---------------------------------------------------------------------------

/// Approval action for an MCP operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpApprovalAction {
    /// Allow the operation without prompting when the server is trusted.
    Allow,
    /// Ask the user when interaction is available.
    Prompt,
    /// Reject the operation before contacting the MCP server.
    #[default]
    Deny,
}

/// Per-server MCP approval policy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct McpServerApprovalPolicy {
    /// Whether this server is trusted for explicit allow rules.
    pub trusted: bool,
    /// Default action for tool calls without a specific rule.
    pub default_tool_action: McpApprovalAction,
    /// Per-tool approval actions keyed by original MCP tool name.
    pub tools: HashMap<String, McpApprovalAction>,
    /// Approval action for resource reads.
    pub resource_read_action: McpApprovalAction,
    /// Approval action for prompt retrieval.
    pub prompt_get_action: McpApprovalAction,
}

impl Default for McpServerApprovalPolicy {
    fn default() -> Self {
        Self {
            trusted: false,
            default_tool_action: McpApprovalAction::Prompt,
            tools: HashMap::new(),
            resource_read_action: McpApprovalAction::Prompt,
            prompt_get_action: McpApprovalAction::Prompt,
        }
    }
}

// ---------------------------------------------------------------------------
// McpServerTransportConfig
// ---------------------------------------------------------------------------

/// Transport mechanism used to reach a single MCP server.
///
/// This enum is tagged with `type` in YAML/JSON so that the variant can be
/// selected declaratively:
///
/// ```yaml
/// transport:
///   type: stdio
///   executable: npx
///   args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
/// ```
///
/// ```yaml
/// transport:
///   type: http
///   endpoint: https://api.example.com/mcp
/// ```
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::server::McpServerTransportConfig;
///
/// let stdio = McpServerTransportConfig::Stdio {
///     executable: "npx".to_string(),
///     args: vec!["-y".to_string(), "@my/server".to_string()],
///     env: std::collections::HashMap::new(),
///     working_dir: None,
/// };
///
/// match &stdio {
///     McpServerTransportConfig::Stdio { executable, .. } => {
///         assert_eq!(executable, "npx");
///     }
///     _ => unreachable!(),
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpServerTransportConfig {
    /// Launch a subprocess and communicate over its stdin/stdout pipes.
    Stdio {
        /// Path or name of the MCP server executable.
        executable: String,

        /// Command-line arguments passed to the executable.
        #[serde(default)]
        args: Vec<String>,

        /// Environment variables injected into the child process.
        ///
        /// The child's inherited environment is cleared before these are
        /// applied (matching [`crate::mcp::transport::stdio::StdioTransport`]
        /// behavior).
        #[serde(default)]
        env: HashMap<String, String>,

        /// Optional working directory for the child process.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },

    /// Connect to a remote server via Streamable HTTP/SSE.
    Http {
        /// Full URL of the MCP endpoint (e.g. `https://api.example.com/mcp`).
        endpoint: url::Url,

        /// Extra HTTP headers added to every request.
        ///
        /// After OAuth token acquisition the `Authorization` header is
        /// injected automatically; entries here supplement (not replace) that.
        #[serde(default)]
        headers: HashMap<String, String>,

        /// Per-request timeout in seconds.
        ///
        /// Overrides [`McpServerConfig::timeout_seconds`] for HTTP-level
        /// operations when set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u64>,

        /// OAuth 2.1 configuration for this endpoint.
        ///
        /// When `None` the endpoint is assumed to be publicly accessible or
        /// protected by a static API key supplied via `headers`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oauth: Option<OAuthServerConfig>,
    },
}

// ---------------------------------------------------------------------------
// McpServerConfig
// ---------------------------------------------------------------------------

/// Full configuration for a single MCP server connection.
///
/// Identified by a unique [`id`][Self::id] that must match the pattern
/// `^[a-z0-9_-]{1,64}$`. Call [`validate`][Self::validate] before use.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::server::{McpServerConfig, McpServerTransportConfig};
///
/// let cfg = McpServerConfig {
///     id: "filesystem".to_string(),
///     transport: McpServerTransportConfig::Stdio {
///         executable: "npx".to_string(),
///         args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string(), "/tmp".to_string()],
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
///     approval: Default::default(),
/// };
///
/// assert!(cfg.validate().is_ok());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique identifier for this server entry.
    ///
    /// Must match `^[a-z0-9_-]{1,64}$`. Used as a key in the server registry
    /// and as the keyring service name prefix for OAuth tokens.
    pub id: String,

    /// Transport mechanism used to reach this server.
    pub transport: McpServerTransportConfig,

    /// Whether this server is active.
    ///
    /// When `false` the server is skipped during `auto_connect`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum seconds to wait for a single MCP request to complete.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Expose this server's tools through the agent's tool registry.
    #[serde(default = "default_true")]
    pub tools_enabled: bool,

    /// Expose this server's resources through the agent.
    #[serde(default)]
    pub resources_enabled: bool,

    /// Expose this server's prompts through the agent.
    #[serde(default)]
    pub prompts_enabled: bool,

    /// Allow the server to request LLM sampling from the client.
    #[serde(default)]
    pub sampling_enabled: bool,

    /// Allow the server to request structured user input via elicitation.
    #[serde(default = "default_true")]
    pub elicitation_enabled: bool,

    /// Explicit approval policy for MCP tools, resources, and prompts.
    #[serde(default)]
    pub approval: McpServerApprovalPolicy,
}

impl McpServerConfig {
    /// Validate this server configuration.
    ///
    /// Checks:
    ///
    /// 1. `id` matches `^[a-z0-9_-]{1,64}$`.
    /// 2. For `Stdio` transport: `executable` must be non-empty.
    /// 3. For `Http` transport: `endpoint` scheme must be `"http"` or
    ///    `"https"`.
    ///
    /// # Returns
    ///
    /// `Ok(())` when all checks pass.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Config`] for any validation failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::server::{McpServerConfig, McpServerTransportConfig};
    ///
    /// let cfg = McpServerConfig {
    ///     id: "my-server_01".to_string(),
    ///     transport: McpServerTransportConfig::Stdio {
    ///         executable: "my-server".to_string(),
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
    ///     approval: Default::default(),
    /// };
    ///
    /// assert!(cfg.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        // Rule 1: id must match ^[a-z0-9_-]{1,64}$
        let id_re = regex::Regex::new(r"^[a-z0-9_-]{1,64}$")
            // SAFETY: The pattern is a compile-time constant and is always valid.
            .expect("static regex is always valid");

        if !id_re.is_match(&self.id) {
            return Err(XzatomaError::Config(format!(
                "MCP server id '{}' is invalid: must match ^[a-z0-9_-]{{1,64}}$",
                self.id
            )));
        }

        for tool_name in self.approval.tools.keys() {
            if tool_name.trim().is_empty() {
                return Err(XzatomaError::Config(format!(
                    "MCP server '{}': approval tool names cannot be empty",
                    self.id
                )));
            }
        }

        // Rule 2 / 3: transport-specific checks.
        match &self.transport {
            McpServerTransportConfig::Stdio { executable, .. } => {
                if executable.is_empty() {
                    return Err(XzatomaError::Config(format!(
                        "MCP server '{}': stdio transport requires a non-empty executable",
                        self.id
                    )));
                }
            }
            McpServerTransportConfig::Http {
                endpoint, oauth, ..
            } => {
                let scheme = endpoint.scheme();
                if scheme != "http" && scheme != "https" {
                    return Err(XzatomaError::Config(format!(
                        "MCP server '{}': http transport endpoint scheme must be 'http' or \
                         'https', got '{}'",
                        self.id, scheme
                    )));
                }

                if oauth.is_some() && scheme != "https" {
                    return Err(XzatomaError::Config(format!(
                        "MCP server '{}': OAuth-enabled HTTP transport must use https",
                        self.id
                    )));
                }

                if let Some(oauth_cfg) = oauth {
                    if let Some(metadata_url) = &oauth_cfg.metadata_url {
                        let parsed = url::Url::parse(metadata_url).map_err(|error| {
                            XzatomaError::Config(format!(
                                "MCP server '{}': oauth.metadata_url must be a valid URL: {}",
                                self.id, error
                            ))
                        })?;
                        crate::security::validate_public_https_url_sync(
                            &parsed,
                            "oauth.metadata_url",
                        )
                        .map_err(|error| XzatomaError::Config(error.to_string()))?;
                    }
                }
            }
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

    // -----------------------------------------------------------------------
    // id validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_accepts_lowercase_alphanumeric_id() {
        let cfg = make_stdio_config("abc123");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_accepts_id_with_hyphens_and_underscores() {
        let cfg = make_stdio_config("my-server_01");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_id_with_uppercase() {
        let cfg = make_stdio_config("MyServer");
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn test_validate_rejects_id_with_spaces() {
        let cfg = make_stdio_config("my server");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_rejects_empty_id() {
        let cfg = make_stdio_config("");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_rejects_id_longer_than_64_chars() {
        let long_id = "a".repeat(65);
        let cfg = make_stdio_config(&long_id);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_validate_accepts_id_of_exactly_64_chars() {
        let id = "a".repeat(64);
        let cfg = make_stdio_config(&id);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_id_with_dot() {
        let cfg = make_stdio_config("my.server");
        assert!(cfg.validate().is_err());
    }

    // -----------------------------------------------------------------------
    // Stdio transport validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_rejects_stdio_with_empty_executable() {
        let cfg = McpServerConfig {
            id: "good-id".to_string(),
            transport: McpServerTransportConfig::Stdio {
                executable: "".to_string(),
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
            approval: Default::default(),
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("non-empty executable"));
    }

    #[test]
    fn test_validate_accepts_stdio_with_nonempty_executable() {
        let cfg = make_stdio_config("my-server");
        assert!(cfg.validate().is_ok());
    }

    // -----------------------------------------------------------------------
    // HTTP transport validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_accepts_http_endpoint() {
        let cfg = make_http_config("my-server", "http://localhost:8080/mcp");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_accepts_https_endpoint() {
        let cfg = make_http_config("my-server", "https://api.example.com/mcp");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_ftp_scheme() {
        let cfg = make_http_config("my-server", "ftp://example.com/mcp");
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("scheme"));
    }

    #[test]
    fn test_validate_rejects_file_scheme() {
        let cfg = make_http_config("my-server", "file:///tmp/mcp");
        assert!(cfg.validate().is_err());
    }

    // -----------------------------------------------------------------------
    // Serde round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_stdio_config_roundtrip_yaml() {
        let cfg = make_stdio_config("fs-server");
        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        let restored: McpServerConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(restored.id, "fs-server");
        match restored.transport {
            McpServerTransportConfig::Stdio { executable, .. } => {
                assert_eq!(executable, "npx");
            }
            _ => panic!("expected Stdio transport"),
        }
    }

    #[test]
    fn test_http_config_roundtrip_yaml() {
        let cfg = make_http_config("remote", "https://api.example.com/mcp");
        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        let restored: McpServerConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(restored.id, "remote");
        match restored.transport {
            McpServerTransportConfig::Http { endpoint, .. } => {
                assert_eq!(endpoint.as_str(), "https://api.example.com/mcp");
            }
            _ => panic!("expected Http transport"),
        }
    }

    #[test]
    fn test_oauth_config_defaults_all_none() {
        let oauth = OAuthServerConfig::default();
        assert!(oauth.client_id.is_none());
        assert!(oauth.client_secret.is_none());
        assert!(oauth.redirect_port.is_none());
        assert!(oauth.metadata_url.is_none());
    }

    #[test]
    fn test_server_config_default_field_values() {
        let cfg = make_stdio_config("defaults-test");
        assert!(cfg.enabled);
        assert_eq!(cfg.timeout_seconds, 30);
        assert!(cfg.tools_enabled);
        assert!(!cfg.resources_enabled);
        assert!(!cfg.prompts_enabled);
        assert!(!cfg.sampling_enabled);
        assert!(cfg.elicitation_enabled);
        assert!(!cfg.approval.trusted);
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_stdio_config(id: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            transport: McpServerTransportConfig::Stdio {
                executable: "npx".to_string(),
                args: vec!["-y".to_string(), "@my/server".to_string()],
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
            approval: Default::default(),
        }
    }

    fn make_http_config(id: &str, endpoint: &str) -> McpServerConfig {
        McpServerConfig {
            id: id.to_string(),
            transport: McpServerTransportConfig::Http {
                endpoint: url::Url::parse(endpoint).expect("valid test URL"),
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
            approval: Default::default(),
        }
    }
}
