//! Ollama provider implementation for XZatoma
//!
//! This module implements the Provider trait for Ollama, connecting to a local
//! or remote Ollama server to generate completions with tool calling support.

use crate::config::OllamaConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{CompletionResponse, FunctionCall, Message, Provider, TokenUsage, ToolCall};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Ollama API provider
///
/// This provider connects to an Ollama server (local or remote) to generate
/// completions. It supports tool calling and converts between XZatoma's
/// message format and Ollama's API format.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::OllamaConfig;
/// use xzatoma::providers::{OllamaProvider, Provider, Message};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = OllamaConfig {
///     host: "http://localhost:11434".to_string(),
///     model: "qwen2.5-coder".to_string(),
/// };
/// let provider = OllamaProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let completion = provider.complete(&messages, &[]).await?;
/// let message = completion.message;
/// # Ok(())
/// # }
/// ```
pub struct OllamaProvider {
    client: Client,
    config: OllamaConfig,
}

/// Request structure for Ollama API
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    stream: bool,
}

/// Message structure for Ollama API
#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

/// Tool definition for Ollama API
#[derive(Debug, Serialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaFunction,
}

/// Function definition for Ollama tools
#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Tool call in Ollama format
#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolCall {
    id: String,
    r#type: String,
    function: OllamaFunctionCall,
}

/// Function call details in Ollama format
#[derive(Debug, Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

/// Response structure from Ollama API
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: usize,
    #[serde(default)]
    eval_count: usize,
    #[serde(default)]
    total_duration: u64,
}

impl OllamaProvider {
    /// Create a new Ollama provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Ollama configuration containing host and model
    ///
    /// # Returns
    ///
    /// Returns a new OllamaProvider instance
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client initialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OllamaConfig;
    /// use xzatoma::providers::OllamaProvider;
    ///
    /// let config = OllamaConfig {
    ///     host: "http://localhost:11434".to_string(),
    ///     model: "qwen2.5-coder".to_string(),
    /// };
    /// let provider = OllamaProvider::new(config);
    /// assert!(provider.is_ok());
    /// ```
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!(
            "Initialized Ollama provider: host={}, model={}",
            config.host,
            config.model
        );

        Ok(Self { client, config })
    }

    /// Get the configured Ollama host
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OllamaConfig;
    /// use xzatoma::providers::OllamaProvider;
    ///
    /// let config = OllamaConfig {
    ///     host: "http://localhost:11434".to_string(),
    ///     model: "qwen2.5-coder".to_string(),
    /// };
    /// let provider = OllamaProvider::new(config).unwrap();
    /// assert_eq!(provider.host(), "http://localhost:11434");
    /// ```
    pub fn host(&self) -> &str {
        &self.config.host
    }

    /// Get the configured model name
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OllamaConfig;
    /// use xzatoma::providers::OllamaProvider;
    ///
    /// let config = OllamaConfig {
    ///     host: "http://localhost:11434".to_string(),
    ///     model: "qwen2.5-coder".to_string(),
    /// };
    /// let provider = OllamaProvider::new(config).unwrap();
    /// assert_eq!(provider.model(), "qwen2.5-coder");
    /// ```
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Convert XZatoma messages to Ollama format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .filter_map(|m| {
                // Skip messages without content (unless they have tool calls)
                if m.content.is_none() && m.tool_calls.is_none() {
                    return None;
                }

                let tool_calls = m.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|tc| OllamaToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: OllamaFunctionCall {
                                name: tc.function.name.clone(),
                                arguments: serde_json::from_str(&tc.function.arguments)
                                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                            },
                        })
                        .collect()
                });

                Some(OllamaMessage {
                    role: m.role.clone(),
                    content: m.content.clone().unwrap_or_default(),
                    tool_calls,
                })
            })
            .collect()
    }

    /// Convert tool schemas to Ollama format
    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<OllamaTool> {
        tools
            .iter()
            .filter_map(|t| {
                let obj = t.as_object()?;
                let name = obj.get("name")?.as_str()?.to_string();
                let description = obj.get("description")?.as_str()?.to_string();
                let parameters = obj.get("parameters")?.clone();

                Some(OllamaTool {
                    r#type: "function".to_string(),
                    function: OllamaFunction {
                        name,
                        description,
                        parameters,
                    },
                })
            })
            .collect()
    }

    /// Convert Ollama response message back to XZatoma format
    fn convert_response_message(&self, ollama_msg: OllamaMessage) -> Message {
        if let Some(tool_calls) = ollama_msg.tool_calls {
            let converted_calls: Vec<ToolCall> = tool_calls
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: serde_json::to_string(&tc.function.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect();

            Message::assistant_with_tools(converted_calls)
        } else {
            Message::assistant(ollama_msg.content)
        }
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let url = format!("{}/api/chat", self.config.host);

        let ollama_request = OllamaRequest {
            model: self.config.model.clone(),
            messages: self.convert_messages(messages),
            tools: self.convert_tools(tools),
            stream: false,
        };

        tracing::debug!(
            "Sending Ollama request: {} messages, {} tools",
            ollama_request.messages.len(),
            ollama_request.tools.len()
        );

        let response = self
            .client
            .post(&url)
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Ollama request failed: {}", e);
                XzatomaError::Provider(format!("Ollama request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Ollama returned error {}: {}", status, error_text);
            return Err(XzatomaError::Provider(format!(
                "Ollama returned error {}: {}",
                status, error_text
            ))
            .into());
        }

        let ollama_response: OllamaResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Ollama response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Ollama response: {}", e))
        })?;

        tracing::debug!(
            "Ollama response: done={}, prompt_tokens={}, completion_tokens={}",
            ollama_response.done,
            ollama_response.prompt_eval_count,
            ollama_response.eval_count
        );

        let message = self.convert_response_message(ollama_response.message);

        // Extract token usage from response
        let response = if ollama_response.prompt_eval_count > 0 || ollama_response.eval_count > 0 {
            let usage = TokenUsage::new(
                ollama_response.prompt_eval_count,
                ollama_response.eval_count,
            );
            CompletionResponse::with_usage(message, usage)
        } else {
            CompletionResponse::new(message)
        };

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_ollama_provider_host() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.host(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_provider_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.model(), "qwen2.5-coder");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let ollama_messages = provider.convert_messages(&messages);
        assert_eq!(ollama_messages.len(), 3);
        assert_eq!(ollama_messages[0].role, "system");
        assert_eq!(ollama_messages[1].role, "user");
        assert_eq!(ollama_messages[2].role, "assistant");
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"test.txt"}"#.to_string(),
            },
        };

        let messages = vec![Message::assistant_with_tools(vec![tool_call])];

        let ollama_messages = provider.convert_messages(&messages);
        assert_eq!(ollama_messages.len(), 1);
        assert!(ollama_messages[0].tool_calls.is_some());
    }

    #[test]
    fn test_convert_tools() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

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

        let ollama_tools = provider.convert_tools(&tools);
        assert_eq!(ollama_tools.len(), 1);
        assert_eq!(ollama_tools[0].function.name, "read_file");
        assert_eq!(ollama_tools[0].function.description, "Read a file");
    }

    #[test]
    fn test_convert_response_message_text() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

        let ollama_msg = OllamaMessage {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            tool_calls: None,
        };

        let msg = provider.convert_response_message(ollama_msg);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hello!".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_convert_response_message_with_tools() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

        let ollama_msg = OllamaMessage {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![OllamaToolCall {
                id: "call_123".to_string(),
                r#type: "function".to_string(),
                function: OllamaFunctionCall {
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({"path": "test.txt"}),
                },
            }]),
        };

        let msg = provider.convert_response_message(ollama_msg);
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(msg.tool_calls.as_ref().unwrap()[0].id, "call_123");
    }

    #[test]
    fn test_convert_messages_filters_empty() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();

        let messages = vec![
            Message {
                role: "user".to_string(),
                content: None,
                tool_calls: None,
                tool_call_id: None,
            },
            Message::user("Valid message"),
        ];

        let ollama_messages = provider.convert_messages(&messages);
        assert_eq!(ollama_messages.len(), 1);
        assert_eq!(ollama_messages[0].content, "Valid message");
    }
}
