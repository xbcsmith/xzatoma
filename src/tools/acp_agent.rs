//! ACP agent call tool for inter-agent communication
//!
//! This module implements `AcpAgentTool`, which allows an agent running inside
//! XZatoma to call out to a remote ACP-compatible server. Two modes are
//! supported:
//!
//! - `sync` -- posts a run, polls until a terminal state, returns the output.
//! - `async` -- posts a run and returns the `run_id` immediately.
//!
//! All calls are gated by an SSRF allow-list (`acp.client.allowed_base_urls`).
//! Requests to URLs not present in the allow-list are rejected immediately
//! without making any network call.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::AcpClientConfig;
use crate::error::{Result, XzatomaError};
use crate::tools::{ToolExecutor, ToolResult};

/// Tool name constant for `call_acp_agent`.
pub const TOOL_CALL_ACP_AGENT: &str = "call_acp_agent";

/// Parameters accepted by `AcpAgentTool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AcpAgentParams {
    /// Base URL of the remote ACP server (e.g. `"http://agent2:8765"`).
    url: String,
    /// Message text to send as the run input.
    input: String,
    /// Execution mode: `"sync"` or `"async"`.
    mode: String,
}

/// Minimal wire-format representation of the remote run create/get response.
#[derive(Debug, Deserialize)]
struct RemoteRunResponse {
    run: RemoteRun,
}

/// Minimal remote run record used for polling.
///
/// `id` is a plain `String` because `AcpRunId` on the server side is a newtype
/// tuple struct that serializes as a bare JSON string.
#[derive(Debug, Deserialize)]
struct RemoteRun {
    id: String,
    status: RemoteRunStatus,
    #[serde(default)]
    output: RemoteRunOutput,
}

/// Status sub-object carried in every run response.
#[derive(Debug, Deserialize)]
struct RemoteRunStatus {
    state: String,
}

/// Output section of a run response.
///
/// Messages are kept as raw `Value` objects so that the tool stays decoupled
/// from server-side schema evolution; only the fields needed for text
/// extraction are accessed.
#[derive(Debug, Default, Deserialize)]
struct RemoteRunOutput {
    #[serde(default)]
    messages: Vec<Value>,
}

/// ACP agent call tool.
///
/// Calls a remote ACP server and either waits for the run to complete (`sync`
/// mode) or returns the `run_id` immediately (`async` mode).
///
/// # Examples
///
/// ```
/// use xzatoma::tools::acp_agent::AcpAgentTool;
/// use xzatoma::config::AcpClientConfig;
/// use std::sync::Arc;
///
/// let config = AcpClientConfig {
///     default_timeout_seconds: 30,
///     allowed_base_urls: vec!["http://localhost:8765".to_string()],
/// };
/// let tool = AcpAgentTool::new(Arc::new(config));
/// ```
pub struct AcpAgentTool {
    config: Arc<AcpClientConfig>,
}

impl AcpAgentTool {
    /// Creates a new `AcpAgentTool`.
    ///
    /// # Arguments
    ///
    /// * `config` - Outbound ACP client configuration
    ///
    /// # Returns
    ///
    /// Returns a new `AcpAgentTool`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::acp_agent::AcpAgentTool;
    /// use xzatoma::config::AcpClientConfig;
    /// use std::sync::Arc;
    ///
    /// let config = Arc::new(AcpClientConfig::default());
    /// let tool = AcpAgentTool::new(config);
    /// ```
    pub fn new(config: Arc<AcpClientConfig>) -> Self {
        Self { config }
    }

    /// Validates `url` against the SSRF allow-list in `allowed_base_urls`.
    ///
    /// Returns an `XzatomaError::Tool` error immediately if the URL is not
    /// present in the allow-list. No network call is made on rejection.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Tool` when the allow-list is empty or the URL
    /// does not appear in it.
    fn validate_url(&self, url: &str) -> Result<()> {
        if self.config.allowed_base_urls.is_empty() {
            return Err(XzatomaError::Tool(format!(
                "call_acp_agent: URL '{}' is blocked -- acp.client.allowed_base_urls is empty",
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
                "call_acp_agent: URL '{}' is not in acp.client.allowed_base_urls",
                url
            )));
        }

        Ok(())
    }

    /// Builds a `reqwest::Client` with the configured per-request timeout.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Tool` if the client cannot be constructed.
    fn build_client(&self) -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.default_timeout_seconds))
            .build()
            .map_err(|e| {
                XzatomaError::Tool(format!(
                    "call_acp_agent: failed to build HTTP client: {}",
                    e
                ))
            })
    }

    /// Posts a new run to `POST {base_url}/runs` and returns the parsed response.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Tool` on network failure, non-2xx status, or
    /// JSON parse failure.
    async fn post_run(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        input: &str,
        mode: &str,
    ) -> Result<RemoteRunResponse> {
        let runs_url = format!("{}/runs", base_url.trim_end_matches('/'));
        let body = json!({
            "input": [
                {
                    "role": "user",
                    "parts": [{"type": "text", "text": input}]
                }
            ],
            "mode": mode
        });

        let resp = client
            .post(&runs_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| XzatomaError::Tool(format!("call_acp_agent: POST /runs failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(XzatomaError::Tool(format!(
                "call_acp_agent: POST /runs returned HTTP {}: {}",
                status, text
            )));
        }

        resp.json::<RemoteRunResponse>().await.map_err(|e| {
            XzatomaError::Tool(format!(
                "call_acp_agent: failed to parse POST /runs response: {}",
                e
            ))
        })
    }

    /// Polls `GET {base_url}/runs/{run_id}` until the run reaches a terminal
    /// state (`completed`, `failed`, or `cancelled`).
    ///
    /// A 500 ms delay is inserted between each poll to avoid hammering the
    /// server.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Tool` on network failure, non-2xx status, or
    /// JSON parse failure.
    async fn poll_until_terminal(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        run_id: &str,
    ) -> Result<RemoteRunResponse> {
        let get_url = format!("{}/runs/{}", base_url.trim_end_matches('/'), run_id);

        loop {
            let resp = client.get(&get_url).send().await.map_err(|e| {
                XzatomaError::Tool(format!(
                    "call_acp_agent: GET /runs/{} failed: {}",
                    run_id, e
                ))
            })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(XzatomaError::Tool(format!(
                    "call_acp_agent: GET /runs/{} returned HTTP {}: {}",
                    run_id, status, text
                )));
            }

            let run_resp: RemoteRunResponse = resp.json().await.map_err(|e| {
                XzatomaError::Tool(format!(
                    "call_acp_agent: failed to parse GET /runs/{} response: {}",
                    run_id, e
                ))
            })?;

            match run_resp.run.status.state.as_str() {
                "completed" | "failed" | "cancelled" => return Ok(run_resp),
                _ => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
    }

    /// Extracts concatenated text from all `text`-typed parts across every
    /// message in the run output.
    ///
    /// Non-text parts (e.g. artifacts) are silently ignored.
    fn extract_output(run: &RemoteRun) -> String {
        run.output
            .messages
            .iter()
            .flat_map(|msg| {
                msg.get("parts")
                    .and_then(|p| p.as_array())
                    .into_iter()
                    .flatten()
            })
            .filter_map(|part| {
                if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                    part.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl std::fmt::Debug for AcpAgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpAgentTool")
            .field("timeout_seconds", &self.config.default_timeout_seconds)
            .field(
                "allowed_base_urls_count",
                &self.config.allowed_base_urls.len(),
            )
            .finish()
    }
}

#[async_trait]
impl ToolExecutor for AcpAgentTool {
    fn tool_definition(&self) -> Value {
        json!({
            "name": TOOL_CALL_ACP_AGENT,
            "description": "Call a remote ACP agent and return its response. Use mode 'sync' to wait for completion or 'async' to get a run_id immediately.",
            "parameters": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Base URL of the remote ACP server (e.g. 'http://agent2:8765')"
                    },
                    "input": {
                        "type": "string",
                        "description": "Message text to send to the remote agent"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["sync", "async"],
                        "description": "Execution mode: 'sync' waits for completion and returns output; 'async' returns run_id immediately"
                    }
                },
                "required": ["url", "input", "mode"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: AcpAgentParams = crate::error::parse_tool_args(args).map_err(|e| {
            XzatomaError::Tool(format!("call_acp_agent: invalid parameters: {}", e))
        })?;

        // Validate URL against the SSRF allow-list before any network call.
        self.validate_url(&params.url)?;

        let client = self.build_client()?;

        match params.mode.as_str() {
            "sync" => {
                let create_resp = self
                    .post_run(&client, &params.url, &params.input, "sync")
                    .await?;
                let run_id = create_resp.run.id.clone();

                let final_resp = self
                    .poll_until_terminal(&client, &params.url, &run_id)
                    .await?;
                let state = &final_resp.run.status.state;

                if state == "failed" || state == "cancelled" {
                    return Ok(ToolResult::error(format!(
                        "Remote ACP run '{}' ended with state '{}'",
                        run_id, state
                    )));
                }

                let output = Self::extract_output(&final_resp.run);
                Ok(ToolResult::success(if output.is_empty() {
                    format!("Run '{}' completed with no output.", run_id)
                } else {
                    output
                }))
            }
            "async" => {
                let create_resp = self
                    .post_run(&client, &params.url, &params.input, "async")
                    .await?;
                let run_id = create_resp.run.id.clone();
                Ok(ToolResult::success(format!(
                    "Run accepted. run_id: {}",
                    run_id
                )))
            }
            other => Err(XzatomaError::Tool(format!(
                "call_acp_agent: unsupported mode '{}'; use 'sync' or 'async'",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_with_no_allowed_urls() -> AcpAgentTool {
        AcpAgentTool::new(Arc::new(AcpClientConfig {
            default_timeout_seconds: 30,
            allowed_base_urls: vec![],
        }))
    }

    fn tool_with_allowed_url(url: &str) -> AcpAgentTool {
        AcpAgentTool::new(Arc::new(AcpClientConfig {
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
                "url": "http://not-allowed:8765",
                "input": "hello",
                "mode": "sync"
            }))
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        // The error must originate from the allow-list check, not from a
        // network attempt -- confirming no HTTP request was made.
        assert!(
            msg.contains("allowed_base_urls is empty")
                || msg.contains("not in acp.client.allowed_base_urls")
        );
    }

    #[tokio::test]
    async fn test_execute_rejects_invalid_mode() {
        let tool = tool_with_allowed_url("http://localhost:8765");
        let result = tool
            .execute(json!({
                "url": "http://localhost:8765",
                "input": "hello",
                "mode": "invalid"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported mode"));
    }

    #[test]
    fn test_tool_definition_has_required_fields() {
        let tool = tool_with_no_allowed_urls();
        let def = tool.tool_definition();
        assert_eq!(def["name"], TOOL_CALL_ACP_AGENT);
        let required = &def["parameters"]["required"];
        assert!(required.as_array().unwrap().contains(&json!("url")));
        assert!(required.as_array().unwrap().contains(&json!("input")));
        assert!(required.as_array().unwrap().contains(&json!("mode")));
    }

    #[test]
    fn test_debug_impl() {
        let tool = tool_with_no_allowed_urls();
        let debug_str = format!("{:?}", tool);
        assert!(debug_str.contains("AcpAgentTool"));
    }

    #[test]
    fn test_extract_output_returns_text_from_parts() {
        let run = RemoteRun {
            id: "run-1".to_string(),
            status: RemoteRunStatus {
                state: "completed".to_string(),
            },
            output: RemoteRunOutput {
                messages: vec![json!({
                    "role": "assistant",
                    "parts": [
                        {"type": "text", "text": "Hello"},
                        {"type": "artifact", "name": "file.txt"}
                    ]
                })],
            },
        };
        assert_eq!(AcpAgentTool::extract_output(&run), "Hello");
    }

    #[test]
    fn test_extract_output_joins_multiple_text_parts() {
        let run = RemoteRun {
            id: "run-2".to_string(),
            status: RemoteRunStatus {
                state: "completed".to_string(),
            },
            output: RemoteRunOutput {
                messages: vec![
                    json!({"parts": [{"type": "text", "text": "First"}]}),
                    json!({"parts": [{"type": "text", "text": "Second"}]}),
                ],
            },
        };
        assert_eq!(AcpAgentTool::extract_output(&run), "First\nSecond");
    }

    #[test]
    fn test_extract_output_empty_when_no_messages() {
        let run = RemoteRun {
            id: "run-3".to_string(),
            status: RemoteRunStatus {
                state: "completed".to_string(),
            },
            output: RemoteRunOutput { messages: vec![] },
        };
        assert_eq!(AcpAgentTool::extract_output(&run), "");
    }

    #[test]
    fn test_validate_url_rejects_different_port() {
        let tool = tool_with_allowed_url("http://agent1:8765");
        let err = tool.validate_url("http://agent1:9999").unwrap_err();
        assert!(
            err.to_string()
                .contains("not in acp.client.allowed_base_urls")
        );
    }

    #[test]
    fn test_validate_url_multiple_allowed_urls_accepts_any() {
        let tool = AcpAgentTool::new(Arc::new(AcpClientConfig {
            default_timeout_seconds: 30,
            allowed_base_urls: vec![
                "http://agent1:8765".to_string(),
                "http://agent2:9000".to_string(),
            ],
        }));
        assert!(tool.validate_url("http://agent1:8765").is_ok());
        assert!(tool.validate_url("http://agent2:9000").is_ok());
        assert!(tool.validate_url("http://agent3:1234").is_err());
    }
}
