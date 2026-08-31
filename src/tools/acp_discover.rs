//! ACP agent discovery tool for multi-agent coordination
//!
//! This module implements `DiscoverAcpAgentsTool`, which queries the
//! `GET /agents` endpoint on a remote ACP server and returns the list of
//! available agents as structured JSON.
//!
//! All calls are gated by the same SSRF allow-list used by `call_acp_agent`.
//! Requests to URLs not in `acp.client.allowed_base_urls` are rejected before
//! any network call is made.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::AcpClientConfig;
use crate::error::{Result, XzatomaError};
use crate::tools::{ToolExecutor, ToolResult};

/// Tool name constant for `discover_acp_agents`.
pub const TOOL_DISCOVER_ACP_AGENTS: &str = "discover_acp_agents";

/// Parameters accepted by `DiscoverAcpAgentsTool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoverAcpAgentsParams {
    /// Base URL of the remote ACP server (e.g. `"http://agent2:8765"`).
    url: String,
}

/// ACP agent discovery tool.
///
/// Queries `GET {url}/agents` on a remote ACP server and returns the agent
/// list as structured JSON.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::acp_discover::DiscoverAcpAgentsTool;
/// use xzatoma::config::AcpClientConfig;
/// use std::sync::Arc;
///
/// let config = AcpClientConfig {
///     default_timeout_seconds: 30,
///     allowed_base_urls: vec!["http://localhost:8765".to_string()],
/// };
/// let tool = DiscoverAcpAgentsTool::new(Arc::new(config));
/// ```
pub struct DiscoverAcpAgentsTool {
    config: Arc<AcpClientConfig>,
}

impl DiscoverAcpAgentsTool {
    /// Creates a new `DiscoverAcpAgentsTool`.
    ///
    /// # Arguments
    ///
    /// * `config` - Outbound ACP client configuration
    ///
    /// # Returns
    ///
    /// Returns a new `DiscoverAcpAgentsTool`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::acp_discover::DiscoverAcpAgentsTool;
    /// use xzatoma::config::AcpClientConfig;
    /// use std::sync::Arc;
    ///
    /// let config = Arc::new(AcpClientConfig::default());
    /// let tool = DiscoverAcpAgentsTool::new(config);
    /// ```
    pub fn new(config: Arc<AcpClientConfig>) -> Self {
        Self { config }
    }

    /// Validates `url` against the SSRF allow-list.
    fn validate_url(&self, url: &str) -> Result<()> {
        if self.config.allowed_base_urls.is_empty() {
            return Err(XzatomaError::Tool(format!(
                "discover_acp_agents: URL '{}' is blocked -- acp.client.allowed_base_urls is empty",
                url
            )));
        }

        let url_trimmed = url.trim_end_matches('/');
        let allowed = self
            .config
            .allowed_base_urls
            .iter()
            .any(|allowed_url| allowed_url.trim_end_matches('/') == url_trimmed);

        if !allowed {
            return Err(XzatomaError::Tool(format!(
                "discover_acp_agents: URL '{}' is not in acp.client.allowed_base_urls",
                url
            )));
        }

        Ok(())
    }

    /// Builds a `reqwest::Client` with the configured timeout.
    fn build_client(&self) -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.default_timeout_seconds))
            .build()
            .map_err(|e| {
                XzatomaError::Tool(format!(
                    "discover_acp_agents: failed to build HTTP client: {}",
                    e
                ))
            })
    }
}

impl std::fmt::Debug for DiscoverAcpAgentsTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoverAcpAgentsTool")
            .field("timeout_seconds", &self.config.default_timeout_seconds)
            .field(
                "allowed_base_urls_count",
                &self.config.allowed_base_urls.len(),
            )
            .finish()
    }
}

#[async_trait]
impl ToolExecutor for DiscoverAcpAgentsTool {
    fn tool_definition(&self) -> Value {
        json!({
            "name": TOOL_DISCOVER_ACP_AGENTS,
            "description": "Discover agents available on a remote ACP server by calling its GET /agents endpoint.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Base URL of the remote ACP server (e.g. 'http://agent2:8765')"
                    }
                },
                "required": ["url"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: DiscoverAcpAgentsParams = crate::error::parse_tool_args(args).map_err(|e| {
            XzatomaError::Tool(format!("discover_acp_agents: invalid parameters: {}", e))
        })?;

        // Validate URL against the SSRF allow-list before any network call.
        self.validate_url(&params.url)?;

        let client = self.build_client()?;

        let agents_url = format!("{}/agents", params.url.trim_end_matches('/'));

        let resp = client.get(&agents_url).send().await.map_err(|e| {
            XzatomaError::Tool(format!("discover_acp_agents: GET /agents failed: {}", e))
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(XzatomaError::Tool(format!(
                "discover_acp_agents: GET /agents returned HTTP {}: {}",
                status, text
            )));
        }

        let agents_json: Value = resp.json().await.map_err(|e| {
            XzatomaError::Tool(format!(
                "discover_acp_agents: failed to parse GET /agents response: {}",
                e
            ))
        })?;

        let output = serde_json::to_string_pretty(&agents_json).map_err(|e| {
            XzatomaError::Tool(format!(
                "discover_acp_agents: failed to serialize agent list: {}",
                e
            ))
        })?;

        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_with_no_allowed_urls() -> DiscoverAcpAgentsTool {
        DiscoverAcpAgentsTool::new(Arc::new(AcpClientConfig {
            default_timeout_seconds: 30,
            allowed_base_urls: vec![],
        }))
    }

    fn tool_with_allowed_url(url: &str) -> DiscoverAcpAgentsTool {
        DiscoverAcpAgentsTool::new(Arc::new(AcpClientConfig {
            default_timeout_seconds: 30,
            allowed_base_urls: vec![url.to_string()],
        }))
    }

    #[test]
    fn test_validate_url_rejects_when_allow_list_is_empty() {
        let tool = tool_with_no_allowed_urls();
        let err = tool.validate_url("http://localhost:8765").unwrap_err();
        assert!(err.to_string().contains("allowed_base_urls is empty"));
    }

    #[test]
    fn test_validate_url_rejects_url_not_in_allow_list() {
        let tool = tool_with_allowed_url("http://agent1:8765");
        let err = tool.validate_url("http://agent2:9000").unwrap_err();
        assert!(
            err.to_string()
                .contains("not in acp.client.allowed_base_urls")
        );
    }

    #[test]
    fn test_validate_url_accepts_url_in_allow_list() {
        let tool = tool_with_allowed_url("http://agent1:8765");
        assert!(tool.validate_url("http://agent1:8765").is_ok());
    }

    #[test]
    fn test_validate_url_accepts_url_with_trailing_slash() {
        let tool = tool_with_allowed_url("http://agent1:8765");
        assert!(tool.validate_url("http://agent1:8765/").is_ok());
    }

    #[tokio::test]
    async fn test_execute_rejects_url_not_in_allow_list_without_network_call() {
        let tool = tool_with_no_allowed_urls();
        let result = tool
            .execute(json!({
                "url": "http://not-allowed:8765"
            }))
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("allowed_base_urls is empty")
                || msg.contains("not in acp.client.allowed_base_urls")
        );
    }

    #[test]
    fn test_tool_definition_has_required_fields() {
        let tool = tool_with_no_allowed_urls();
        let def = tool.tool_definition();
        assert_eq!(def["name"], TOOL_DISCOVER_ACP_AGENTS);
        let required = &def["parameters"]["required"];
        assert!(required.as_array().unwrap().contains(&json!("url")));
    }

    #[test]
    fn test_debug_impl() {
        let tool = tool_with_no_allowed_urls();
        let debug_str = format!("{:?}", tool);
        assert!(debug_str.contains("DiscoverAcpAgentsTool"));
    }
}
