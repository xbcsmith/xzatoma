//! GitHub Copilot provider implementation for XZatoma
//!
//! This module implements the Provider trait for GitHub Copilot, including
//! OAuth device flow authentication and token caching in the system keyring.

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    CompletionResponse, FunctionCall, Message, ModelCapability, ModelInfo, Provider,
    ProviderCapabilities, TokenUsage, ToolCall,
};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// GitHub OAuth device code endpoint
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
/// GitHub OAuth token endpoint
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// Copilot token exchange endpoint
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
/// Copilot chat completions endpoint
const COPILOT_COMPLETIONS_URL: &str = "https://api.githubcopilot.com/chat/completions";
/// GitHub Copilot OAuth client ID
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// GitHub Copilot provider
///
/// This provider connects to GitHub Copilot's API to generate completions.
/// It implements OAuth device flow for authentication and caches tokens
/// in the system keyring.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::CopilotConfig;
/// use xzatoma::providers::{CopilotProvider, Provider, Message};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = CopilotConfig {
///     model: "gpt-5-mini".to_string(),
/// };
/// let provider = CopilotProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let completion = provider.complete(&messages, &[]).await?;
/// let message = completion.message;
/// # Ok(())
/// # }
/// ```
pub struct CopilotProvider {
    client: Client,
    config: Arc<RwLock<CopilotConfig>>,
    keyring_service: String,
    keyring_user: String,
}

/// Request for GitHub device code
#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

/// Response containing device code and user verification URL
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Request to exchange device code for access token
#[derive(Debug, Serialize)]
struct TokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

/// Response containing GitHub access token
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[allow(dead_code)]
    scope: String,
}

/// Cached token information stored in keyring
#[derive(Debug, Serialize, Deserialize)]
struct CachedToken {
    github_token: String,
    copilot_token: String,
    expires_at: u64,
}

/// Request structure for Copilot API
#[derive(Debug, Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<CopilotTool>,
    stream: bool,
}

/// Message structure for Copilot API
#[derive(Debug, Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<CopilotToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Tool definition for Copilot API
#[derive(Debug, Serialize)]
struct CopilotTool {
    r#type: String,
    function: CopilotFunction,
}

/// Function definition for Copilot tools
#[derive(Debug, Serialize)]
struct CopilotFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Tool call in Copilot format
#[derive(Debug, Serialize, Deserialize)]
struct CopilotToolCall {
    id: String,
    r#type: String,
    function: CopilotFunctionCall,
}

/// Function call details in Copilot format
#[derive(Debug, Serialize, Deserialize)]
struct CopilotFunctionCall {
    name: String,
    arguments: String,
}

/// Response structure from Copilot API
#[derive(Debug, Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
    #[allow(dead_code)]
    usage: Option<CopilotUsage>,
}

/// Choice in Copilot response
#[derive(Debug, Deserialize)]
struct CopilotChoice {
    message: CopilotMessage,
    #[allow(dead_code)]
    finish_reason: String,
}

/// Token usage information from Copilot
#[derive(Debug, Deserialize)]
struct CopilotUsage {
    #[allow(dead_code)]
    prompt_tokens: usize,
    #[allow(dead_code)]
    completion_tokens: usize,
    #[allow(dead_code)]
    total_tokens: usize,
}

impl CopilotProvider {
    /// Create a new Copilot provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Copilot configuration containing model name
    ///
    /// # Returns
    ///
    /// Returns a new CopilotProvider instance
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client initialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::CopilotConfig;
    /// use xzatoma::providers::CopilotProvider;
    ///
    /// let config = CopilotConfig {
    ///     model: "gpt-5-mini".to_string(),
    /// };
    /// let provider = CopilotProvider::new(config);
    /// assert!(provider.is_ok());
    /// ```
    pub fn new(config: CopilotConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!("Initialized Copilot provider: model={}", config.model);

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            keyring_service: "xzatoma".to_string(),
            keyring_user: "github_copilot".to_string(),
        })
    }

    /// Get the configured model name
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::CopilotConfig;
    /// use xzatoma::providers::{CopilotProvider, Provider};
    ///
    /// let config = CopilotConfig {
    ///     model: "gpt-5-mini".to_string(),
    /// };
    /// let provider = CopilotProvider::new(config).unwrap();
    /// assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    /// ```
    ///
    /// Authenticate and get Copilot token
    ///
    /// Checks keyring for cached token first. If not found or expired,
    /// performs OAuth device flow to get new token.
    pub async fn authenticate(&self) -> Result<String> {
        if let Ok(cached) = self.get_cached_token() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now + 300 {
                tracing::debug!("Using cached Copilot token");
                return Ok(cached.copilot_token);
            }

            tracing::debug!("Cached token expired, refreshing");
        }

        tracing::info!("Starting GitHub OAuth device flow");
        let github_token = self.device_flow().await?;

        tracing::debug!("Exchanging GitHub token for Copilot token");
        let copilot_token = self.get_copilot_token(&github_token).await?;

        let cached = CachedToken {
            github_token,
            copilot_token: copilot_token.clone(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
        };

        if let Err(e) = self.cache_token(&cached) {
            tracing::warn!("Failed to cache token: {}", e);
        }

        Ok(copilot_token)
    }

    /// Perform OAuth device flow to get GitHub token
    async fn device_flow(&self) -> Result<String> {
        // Request a device code (GitHub expects form-encoded body for these endpoints)
        // Request a device code.  If the server returns a non-2xx response
        // read the body and present a detailed error (status + body) so users
        // can see why the request failed (network / API / enterprise GitHub).
        let resp = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&DeviceCodeRequest {
                client_id: GITHUB_CLIENT_ID.to_string(),
                scope: "read:user".to_string(),
            })
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Device code request failed: {}", e)))?;

        if !resp.status().is_success() {
            // Try to include the response body (best-effort) for a clearer error.
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(XzatomaError::Provider(format!(
                "Device code request returned {}: {}",
                status, body
            ))
            .into());
        }

        let device_response: DeviceCodeResponse = resp
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse device code: {}", e)))?;

        println!("\nGitHub Authentication Required:");
        println!("  1. Visit: {}", device_response.verification_uri);
        println!("  2. Enter code: {}", device_response.user_code);
        println!("\nWaiting for authorization...");

        let interval = Duration::from_secs(device_response.interval);
        let max_attempts = device_response.expires_in / device_response.interval;

        for attempt in 0..max_attempts {
            tokio::time::sleep(interval).await;

            let response = self
                .client
                .post(GITHUB_TOKEN_URL)
                .header("Accept", "application/json")
                .form(&TokenRequest {
                    client_id: GITHUB_CLIENT_ID.to_string(),
                    device_code: device_response.device_code.clone(),
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                })
                .send()
                .await
                .map_err(|e| XzatomaError::Provider(format!("Token poll failed: {}", e)))?;

            // The token endpoint may return 200 with an error body (e.g. authorization_pending).
            // Parse the JSON and handle known device-flow error responses explicitly.
            let body: serde_json::Value = response.json().await.map_err(|e| {
                XzatomaError::Provider(format!("Failed to parse token poll response: {}", e))
            })?;

            // If an access token is present, we're done.
            if let Some(access) = body.get("access_token").and_then(|v| v.as_str()) {
                println!("Authorization successful!");
                tracing::info!("GitHub OAuth device flow completed successfully");
                return Ok(access.to_string());
            }

            // Handle standard OAuth device-flow transient/fatal errors.
            if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
                match err {
                    "authorization_pending" => {
                        tracing::debug!("authorization_pending; continuing to poll");
                        // keep polling
                    }
                    "slow_down" => {
                        tracing::debug!("slow_down received; backing off an extra interval");
                        // apply a small backoff before the next attempt
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                    "expired_token" => {
                        return Err(XzatomaError::Provider(
                            "Device flow expired before authorization".to_string(),
                        )
                        .into());
                    }
                    other => {
                        return Err(XzatomaError::Provider(format!(
                            "Device flow error from provider: {}",
                            other
                        ))
                        .into());
                    }
                }
            } else {
                // No access_token and no explicit error — continue polling conservatively.
                tracing::debug!("Token poll returned no token and no error; continuing");
            }

            tracing::debug!("Polling attempt {}/{}", attempt + 1, max_attempts);
        }

        Err(
            XzatomaError::Provider("Device flow timed out waiting for authorization".to_string())
                .into(),
        )
    }

    /// Exchange GitHub token for Copilot token
    async fn get_copilot_token(&self, github_token: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct CopilotTokenResponse {
            token: String,
        }

        let response: CopilotTokenResponse = self
            .client
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", github_token))
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Copilot token request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse Copilot token: {}", e)))?;

        Ok(response.token)
    }

    // parse_github_token_poll moved to module scope (see function below)
    // (kept a short placeholder here so impl remains readable)

    /// Get cached token from system keyring
    fn get_cached_token(&self) -> Result<CachedToken> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)?;

        let json = entry.get_password()?;

        Ok(serde_json::from_str(&json)?)
    }

    /// Cache token in system keyring
    fn cache_token(&self, token: &CachedToken) -> Result<()> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)?;

        let json = serde_json::to_string(token)?;

        entry.set_password(&json)?;

        Ok(())
    }

    /// Convert XZatoma messages to Copilot format
    fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
        messages
            .iter()
            .filter_map(|m| {
                if m.content.is_none() && m.tool_calls.is_none() {
                    return None;
                }

                let tool_calls = m.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|tc| CopilotToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: CopilotFunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect()
                });

                Some(CopilotMessage {
                    role: m.role.clone(),
                    content: m.content.clone().unwrap_or_default(),
                    tool_calls,
                    tool_call_id: m.tool_call_id.clone(),
                })
            })
            .collect()
    }

    /// Convert tool schemas to Copilot format
    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<CopilotTool> {
        tools
            .iter()
            .filter_map(|t| {
                let obj = t.as_object()?;
                let name = obj.get("name")?.as_str()?.to_string();
                let description = obj.get("description")?.as_str()?.to_string();
                let parameters = obj.get("parameters")?.clone();

                Some(CopilotTool {
                    r#type: "function".to_string(),
                    function: CopilotFunction {
                        name,
                        description,
                        parameters,
                    },
                })
            })
            .collect()
    }

    /// Convert Copilot response message back to XZatoma format
    fn convert_response_message(&self, copilot_msg: CopilotMessage) -> Message {
        if let Some(tool_calls) = copilot_msg.tool_calls {
            let converted_calls: Vec<ToolCall> = tool_calls
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    },
                })
                .collect();

            Message::assistant_with_tools(converted_calls)
        } else {
            Message::assistant(copilot_msg.content)
        }
    }

    /// Get the list of supported Copilot models with their metadata
    fn get_copilot_models() -> Vec<ModelInfo> {
        let mut models = vec![
            ModelInfo::new("gpt-4", "GPT-4", 8192),
            ModelInfo::new("gpt-4-turbo", "GPT-4 Turbo", 128000),
            ModelInfo::new("gpt-3.5-turbo", "GPT-3.5 Turbo", 4096),
            ModelInfo::new("claude-3.5-sonnet", "Claude 3.5 Sonnet", 200000),
            ModelInfo::new("claude-sonnet-4.5", "Claude Sonnet 4.5", 200000),
            ModelInfo::new("o1-preview", "OpenAI o1 Preview", 128000),
            ModelInfo::new("o1-mini", "OpenAI o1 Mini", 128000),
        ];

        // Configure capabilities for each model
        models[0].add_capability(ModelCapability::FunctionCalling);

        models[1].add_capability(ModelCapability::FunctionCalling);
        models[1].add_capability(ModelCapability::LongContext);

        models[2].add_capability(ModelCapability::FunctionCalling);

        models[3].add_capability(ModelCapability::FunctionCalling);
        models[3].add_capability(ModelCapability::LongContext);
        models[3].add_capability(ModelCapability::Vision);

        models[4].add_capability(ModelCapability::FunctionCalling);
        models[4].add_capability(ModelCapability::LongContext);
        models[4].add_capability(ModelCapability::Vision);

        models[5].add_capability(ModelCapability::FunctionCalling);
        models[5].add_capability(ModelCapability::LongContext);

        models[6].add_capability(ModelCapability::FunctionCalling);
        models[6].add_capability(ModelCapability::LongContext);

        models
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let token = self.authenticate().await?;

        let model = self
            .config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?
            .model
            .clone();
        let copilot_request = CopilotRequest {
            model,
            messages: self.convert_messages(messages),
            tools: self.convert_tools(tools),
            stream: false,
        };

        tracing::debug!(
            "Sending Copilot request: {} messages, {} tools",
            copilot_request.messages.len(),
            copilot_request.tools.len()
        );

        let response = self
            .client
            .post(COPILOT_COMPLETIONS_URL)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&copilot_request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Copilot request failed: {}", e);
                XzatomaError::Provider(format!("Copilot request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Copilot returned error {}: {}", status, error_text);
            return Err(XzatomaError::Provider(format!(
                "Copilot returned error {}: {}",
                status, error_text
            ))
            .into());
        }

        let copilot_response: CopilotResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Copilot response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Copilot response: {}", e))
        })?;

        let choice =
            copilot_response.choices.into_iter().next().ok_or_else(|| {
                XzatomaError::Provider("No choices in Copilot response".to_string())
            })?;

        tracing::debug!("Copilot response received successfully");

        let message = self.convert_response_message(choice.message);

        // Extract token usage if available
        let usage = copilot_response
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        let response = match usage {
            Some(u) => CompletionResponse::with_usage(message, u),
            None => CompletionResponse::new(message),
        };
        Ok(response)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        tracing::debug!("Listing Copilot models");
        Ok(Self::get_copilot_models())
    }

    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
        tracing::debug!("Getting info for model: {}", model_name);
        Self::get_copilot_models()
            .into_iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                XzatomaError::Provider(format!("Model not found: {}", model_name)).into()
            })
    }

    fn get_current_model(&self) -> Result<String> {
        self.config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string()).into()
            })
            .map(|config| config.model.clone())
    }

    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: true,
            supports_model_switching: true,
            supports_token_counts: true,
            supports_streaming: false,
        }
    }

    async fn set_model(&mut self, model_name: String) -> Result<()> {
        // Validate that the model exists
        let models = Self::get_copilot_models();
        if !models.iter().any(|m| m.name == model_name) {
            return Err(XzatomaError::Provider(format!("Model not found: {}", model_name)).into());
        }

        // Update the model in the config
        let mut config = self.config.write().map_err(|_| {
            XzatomaError::Provider("Failed to acquire write lock on config".to_string())
        })?;
        config.model = model_name.clone();
        tracing::info!("Switched Copilot model to: {}", model_name);
        Ok(())
    }
}

/// Parse a token-poll response from GitHub's device token endpoint.
///
/// Returns:
/// - Ok(Some(token)) when an access token is present
/// - Ok(None) when polling should continue (authorization_pending / slow_down)
/// - Err(...) for fatal errors (expired_token or unknown error)
fn parse_github_token_poll(value: &serde_json::Value) -> Result<Option<String>> {
    if let Some(tok) = value.get("access_token").and_then(|v| v.as_str()) {
        return Ok(Some(tok.to_string()));
    }

    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        match err {
            "authorization_pending" => return Ok(None),
            "slow_down" => return Ok(None),
            "expired_token" => {
                return Err(XzatomaError::Provider("Device flow expired".to_string()).into());
            }
            other => {
                return Err(XzatomaError::Provider(format!("Device flow error: {}", other)).into());
            }
        }
    }

    // No token and no recognizable error -> keep polling
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copilot_provider_creation() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config);
        assert!(provider.is_ok());
    }

    // --- Device-flow response parsing tests (no network) ---
    #[test]
    fn test_parse_github_token_poll_pending() {
        let v = serde_json::json!({ "error": "authorization_pending" });
        assert!(parse_github_token_poll(&v).unwrap().is_none());
    }

    #[test]
    fn test_parse_github_token_poll_slow_down() {
        let v = serde_json::json!({ "error": "slow_down" });
        assert!(parse_github_token_poll(&v).unwrap().is_none());
    }

    #[test]
    fn test_parse_github_token_poll_success() {
        let v = serde_json::json!({ "access_token": "gho_ABC123" });
        assert_eq!(
            parse_github_token_poll(&v).unwrap(),
            Some("gho_ABC123".to_string())
        );
    }

    #[test]
    fn test_parse_github_token_poll_expired() {
        let v = serde_json::json!({ "error": "expired_token" });
        assert!(parse_github_token_poll(&v).is_err());
    }

    #[test]
    fn test_copilot_config_default_model() {
        let cfg = CopilotConfig::default();
        assert_eq!(cfg.model, "gpt-5-mini");
    }

    #[test]
    fn test_copilot_provider_model() {
        let config = CopilotConfig {
            model: "gpt-5-mini".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi"),
        ];

        let copilot_messages = provider.convert_messages(&messages);
        assert_eq!(copilot_messages.len(), 3);
        assert_eq!(copilot_messages[0].role, "system");
        assert_eq!(copilot_messages[1].role, "user");
        assert_eq!(copilot_messages[2].role, "assistant");
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"test.txt"}"#.to_string(),
            },
        };

        let messages = vec![Message::assistant_with_tools(vec![tool_call])];

        let copilot_messages = provider.convert_messages(&messages);
        assert_eq!(copilot_messages.len(), 1);
        assert!(copilot_messages[0].tool_calls.is_some());
    }

    #[test]
    fn test_convert_tools() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let tools = vec![serde_json::json!({
            "name": "read_file",
            "description": "Read a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }
        })];

        let copilot_tools = provider.convert_tools(&tools);
        assert_eq!(copilot_tools.len(), 1);
        assert_eq!(copilot_tools[0].function.name, "read_file");
        assert_eq!(copilot_tools[0].function.description, "Read a file");
    }

    #[test]
    fn test_convert_response_message_text() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let copilot_msg = CopilotMessage {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let msg = provider.convert_response_message(copilot_msg);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hello!".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_convert_response_message_with_tools() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let copilot_msg = CopilotMessage {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![CopilotToolCall {
                id: "call_123".to_string(),
                r#type: "function".to_string(),
                function: CopilotFunctionCall {
                    name: "read_file".to_string(),
                    arguments: r#"{"path":"test.txt"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
        };

        let msg = provider.convert_response_message(copilot_msg);
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_keyring_service_names() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.keyring_service, "xzatoma");
        assert_eq!(provider.keyring_user, "github_copilot");
    }

    #[test]
    fn test_list_copilot_models() {
        let models = CopilotProvider::get_copilot_models();
        assert!(!models.is_empty());

        // Check that we have the expected models
        let model_names: Vec<_> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(model_names.contains(&"gpt-4"));
        assert!(model_names.contains(&"gpt-4-turbo"));
        assert!(model_names.contains(&"gpt-3.5-turbo"));
        assert!(model_names.contains(&"claude-3.5-sonnet"));
        assert!(model_names.contains(&"claude-sonnet-4.5"));
        assert!(model_names.contains(&"o1-preview"));
        assert!(model_names.contains(&"o1-mini"));
    }

    #[test]
    fn test_copilot_model_capabilities() {
        let models = CopilotProvider::get_copilot_models();

        // gpt-4 should support function calling
        let gpt4 = models.iter().find(|m| m.name == "gpt-4").unwrap();
        assert!(gpt4.supports_capability(ModelCapability::FunctionCalling));
        assert!(!gpt4.supports_capability(ModelCapability::Vision));

        // gpt-4-turbo should support long context and function calling
        let gpt4_turbo = models.iter().find(|m| m.name == "gpt-4-turbo").unwrap();
        assert!(gpt4_turbo.supports_capability(ModelCapability::FunctionCalling));
        assert!(gpt4_turbo.supports_capability(ModelCapability::LongContext));
        assert_eq!(gpt4_turbo.context_window, 128000);

        // Claude should support vision
        let claude = models
            .iter()
            .find(|m| m.name == "claude-3.5-sonnet")
            .unwrap();
        assert!(claude.supports_capability(ModelCapability::Vision));
        assert!(claude.supports_capability(ModelCapability::LongContext));
        assert_eq!(claude.context_window, 200000);
    }

    #[test]
    fn test_copilot_model_context_windows() {
        let models = CopilotProvider::get_copilot_models();

        let gpt4 = models.iter().find(|m| m.name == "gpt-4").unwrap();
        assert_eq!(gpt4.context_window, 8192);

        let gpt35 = models.iter().find(|m| m.name == "gpt-3.5-turbo").unwrap();
        assert_eq!(gpt35.context_window, 4096);

        let o1_preview = models.iter().find(|m| m.name == "o1-preview").unwrap();
        assert_eq!(o1_preview.context_window, 128000);
    }

    #[test]
    fn test_get_current_model() {
        let config = CopilotConfig {
            model: "gpt-4-turbo".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-4-turbo");
    }

    #[test]
    fn test_provider_capabilities() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        let caps = provider.get_provider_capabilities();

        assert!(caps.supports_model_listing);
        assert!(caps.supports_model_details);
        assert!(caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_set_model_with_valid_model() {
        let config = CopilotConfig {
            model: "gpt-4".to_string(),
        };
        let mut provider = CopilotProvider::new(config).unwrap();

        // Should succeed with valid model
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { provider.set_model("gpt-4-turbo".to_string()).await });

        assert!(result.is_ok());
        assert_eq!(provider.get_current_model().unwrap(), "gpt-4-turbo");
    }

    #[test]
    fn test_set_model_with_invalid_model() {
        let config = CopilotConfig {
            model: "gpt-4".to_string(),
        };
        let mut provider = CopilotProvider::new(config).unwrap();

        // Should fail with invalid model
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { provider.set_model("invalid-model-name".to_string()).await });

        assert!(result.is_err());
        // Original model should be unchanged
        assert_eq!(provider.get_current_model().unwrap(), "gpt-4");
    }

    #[test]
    fn test_list_models_returns_all_supported_models() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { provider.list_models().await });

        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.len(), 7); // Seven known models
    }

    #[test]
    fn test_get_model_info_valid_model() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { provider.get_model_info("claude-3.5-sonnet").await });

        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.name, "claude-3.5-sonnet");
        assert_eq!(info.display_name, "Claude 3.5 Sonnet");
        assert_eq!(info.context_window, 200000);
    }

    #[test]
    fn test_get_model_info_invalid_model() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { provider.get_model_info("nonexistent-model").await });

        assert!(result.is_err());
    }

    #[test]
    fn test_token_usage_extraction() {
        // Test that TokenUsage is correctly created
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_completion_response_with_usage() {
        let message = Message::assistant("Test response");
        let usage = TokenUsage::new(100, 50);
        let response = CompletionResponse::with_usage(message, usage);

        assert_eq!(response.message.content, Some("Test response".to_string()));
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().total_tokens, 150);
    }

    #[test]
    fn test_completion_response_without_usage() {
        let message = Message::assistant("Test response");
        let response = CompletionResponse::new(message);

        assert_eq!(response.message.content, Some("Test response".to_string()));
        assert!(response.usage.is_none());
    }
}
