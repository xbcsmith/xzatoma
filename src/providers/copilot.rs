//! GitHub Copilot provider implementation for XZatoma
//!
//! This module implements the Provider trait for GitHub Copilot, including
//! OAuth device flow authentication and token caching in the system keyring.

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{FunctionCall, Message, Provider, ToolCall};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
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
///     model: "gpt-4o".to_string(),
/// };
/// let provider = CopilotProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let response = provider.complete(&messages, &[]).await?;
/// # Ok(())
/// # }
/// ```
pub struct CopilotProvider {
    client: Client,
    config: CopilotConfig,
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
    ///     model: "gpt-4o".to_string(),
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
            config,
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
    /// use xzatoma::providers::CopilotProvider;
    ///
    /// let config = CopilotConfig {
    ///     model: "gpt-4o".to_string(),
    /// };
    /// let provider = CopilotProvider::new(config).unwrap();
    /// assert_eq!(provider.model(), "gpt-4o");
    /// ```
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Authenticate and get Copilot token
    ///
    /// Checks keyring for cached token first. If not found or expired,
    /// performs OAuth device flow to get new token.
    async fn authenticate(&self) -> Result<String> {
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
        let device_response: DeviceCodeResponse = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .json(&DeviceCodeRequest {
                client_id: GITHUB_CLIENT_ID.to_string(),
                scope: "read:user".to_string(),
            })
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Device code request failed: {}", e)))?
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
                .json(&TokenRequest {
                    client_id: GITHUB_CLIENT_ID.to_string(),
                    device_code: device_response.device_code.clone(),
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                })
                .send()
                .await
                .map_err(|e| XzatomaError::Provider(format!("Token poll failed: {}", e)))?;

            if response.status().is_success() {
                let token_response: TokenResponse = response
                    .json()
                    .await
                    .map_err(|e| XzatomaError::Provider(format!("Failed to parse token: {}", e)))?;

                println!("Authorization successful!");
                tracing::info!("GitHub OAuth device flow completed successfully");
                return Ok(token_response.access_token);
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
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(&self, messages: &[Message], tools: &[serde_json::Value]) -> Result<Message> {
        let token = self.authenticate().await?;

        let copilot_request = CopilotRequest {
            model: self.config.model.clone(),
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

        Ok(self.convert_response_message(choice.message))
    }
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

    #[test]
    fn test_copilot_provider_model() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.model(), "gpt-4o");
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
}
