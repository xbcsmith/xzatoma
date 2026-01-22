//! Ollama provider implementation for XZatoma
//!
//! This module implements the Provider trait for Ollama, connecting to a local
//! or remote Ollama server to generate completions with tool calling support.
//! Includes model listing, model switching, and token usage tracking.

use crate::config::OllamaConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    CompletionResponse, FunctionCall, Message, ModelCapability, ModelInfo, Provider,
    ProviderCapabilities, TokenUsage, ToolCall,
};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Ollama API provider
///
/// This provider connects to an Ollama server (local or remote) to generate
/// completions. It supports tool calling, model listing, model switching,
/// and token usage tracking. Models are cached for 5 minutes to reduce API calls.
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
///     model: "llama3.2:latest".to_string(),
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
    config: Arc<RwLock<OllamaConfig>>,
    #[allow(clippy::type_complexity)]
    model_cache: Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>,
}

/// Response from Ollama's /api/tags endpoint
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelTag>,
}

/// Model metadata from /api/tags
#[derive(Debug, Deserialize)]
struct OllamaModelTag {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    digest: String,
    #[serde(default)]
    modified_at: String,
}

/// Response from Ollama's /api/show endpoint
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    name: String,
    #[serde(default)]
    model_info: serde_json::Value,
    #[serde(default)]
    parameters: String,
    #[serde(default)]
    template: String,
    #[serde(default)]
    details: OllamaModelDetails,
}

/// Model details from /api/show
#[derive(Debug, Deserialize, Default)]
struct OllamaModelDetails {
    #[serde(default)]
    parameter_size: String,
    #[serde(default)]
    quantization_level: String,
    #[serde(default)]
    family: String,
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
    #[serde(default)]
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
    #[serde(default)]
    id: String,
    #[serde(default = "default_tool_type")]
    r#type: String,
    function: OllamaFunctionCall,
}

/// Function call details in Ollama format
#[derive(Debug, Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

/// Default type for tool calls (used when field is missing)
fn default_tool_type() -> String {
    "function".to_string()
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
    ///     model: "llama3.2:latest".to_string(),
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

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            model_cache: Arc::new(RwLock::new(None)),
        })
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
    ///     model: "llama3.2:latest".to_string(),
    /// };
    /// let provider = OllamaProvider::new(config).unwrap();
    /// assert_eq!(provider.host(), "http://localhost:11434");
    /// ```
    pub fn host(&self) -> String {
        self.config
            .read()
            .map(|config| config.host.clone())
            .unwrap_or_default()
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
    ///     model: "llama3.2:latest".to_string(),
    /// };
    /// let provider = OllamaProvider::new(config).unwrap();
    /// assert_eq!(provider.model(), "llama3.2:latest");
    /// ```
    pub fn model(&self) -> String {
        self.config
            .read()
            .map(|config| config.model.clone())
            .unwrap_or_default()
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
                .enumerate()
                .map(|(idx, tc)| ToolCall {
                    id: if tc.id.is_empty() {
                        format!(
                            "call_{}_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis(),
                            idx
                        )
                    } else {
                        tc.id
                    },
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: serde_json::to_string(&tc.function.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
                .collect();

            Message::assistant_with_tools(converted_calls)
        } else {
            // Handle empty content by using empty string
            Message::assistant(if ollama_msg.content.is_empty() {
                "".to_string()
            } else {
                ollama_msg.content
            })
        }
    }

    /// Fetch models from Ollama's /api/tags endpoint
    async fn fetch_models_from_api(&self) -> Result<Vec<ModelInfo>> {
        let host = self
            .config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?
            .host
            .clone();

        let url = format!("{}/api/tags", host);
        tracing::debug!("Fetching models from Ollama: {}", url);

        let response = self.client.get(&url).send().await.map_err(|e| {
            tracing::warn!("Failed to fetch Ollama models: {}", e);
            XzatomaError::Provider(format!("Failed to connect to Ollama server: {}", e))
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

        let ollama_response: OllamaTagsResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Ollama tags response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Ollama response: {}", e))
        })?;

        let models: Vec<ModelInfo> = ollama_response
            .models
            .into_iter()
            .map(|tag| {
                // Extract model family from name (e.g., "llama2:7b" -> "llama2")
                let family = tag.name.split(':').next().unwrap_or(&tag.name);

                let mut model_info = ModelInfo::new(
                    &tag.name,
                    format!("{} ({})", tag.name, format_size(tag.size)),
                    get_context_window_for_model(&tag.name),
                );

                // Add capabilities based on model family
                add_model_capabilities(&mut model_info, family);

                model_info
            })
            .collect();

        tracing::debug!("Fetched {} models from Ollama", models.len());
        Ok(models)
    }

    /// Get model details from Ollama's /api/show endpoint
    async fn fetch_model_details(&self, model_name: &str) -> Result<ModelInfo> {
        let host = self
            .config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?
            .host
            .clone();

        let url = format!("{}/api/show", host);
        tracing::debug!("Fetching model details for: {}", model_name);

        #[derive(Serialize)]
        struct ShowRequest {
            name: String,
        }

        let response = self
            .client
            .post(&url)
            .json(&ShowRequest {
                name: model_name.to_string(),
            })
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("Failed to fetch Ollama model details: {}", e);
                XzatomaError::Provider(format!("Failed to fetch model details: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Ollama returned error {}: {}", status, error_text);
            return Err(XzatomaError::Provider(format!("Model not found: {}", model_name)).into());
        }

        let show_response: OllamaShowResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Ollama show response: {}", e);
            XzatomaError::Provider(format!("Failed to parse model details: {}", e))
        })?;

        let mut model_info = ModelInfo::new(
            &show_response.name,
            &show_response.name,
            get_context_window_for_model(&show_response.name),
        );

        let family = show_response
            .name
            .split(':')
            .next()
            .unwrap_or(&show_response.name);
        add_model_capabilities(&mut model_info, family);

        Ok(model_info)
    }

    /// Invalidate the model cache
    fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.model_cache.write() {
            *cache = None;
            tracing::debug!("Model cache invalidated");
        }
    }

    /// Check if cache is still valid (less than 5 minutes old)
    fn is_cache_valid(cached_at: Instant) -> bool {
        cached_at.elapsed() < Duration::from_secs(300)
    }
}

/// Get context window size for a model based on its name
fn get_context_window_for_model(model_name: &str) -> usize {
    // Common context windows for popular models
    if model_name.contains("70b")
        || model_name.contains("mistral")
        || model_name.contains("neural-chat")
    {
        8192
    } else {
        4096 // Default for 7b, 13b, orca, dolphin, and unknown
    }
}

/// Add model capabilities based on model family
fn add_model_capabilities(model: &mut ModelInfo, family: &str) {
    // Only specific Ollama models support function calling (tool use)
    // Based on Ollama documentation and testing
    match family.to_lowercase().as_str() {
        // Models that support tool calling
        "llama3.2" | "llama3.3" | "mistral" | "mistral-nemo" | "firefunction" | "command-r"
        | "command-r-plus" | "granite3" | "granite4" => {
            model.add_capability(ModelCapability::FunctionCalling);
        }
        _ => {
            // Most other models do NOT support tool calling
            // Including: llama3, llama2, gemma, qwen, codellama, etc.
        }
    }

    // Add other capabilities based on model family
    match family.to_lowercase().as_str() {
        "mistral" | "mistral-nemo" | "neural-chat" => {
            model.add_capability(ModelCapability::LongContext);
        }
        "llava" => {
            model.add_capability(ModelCapability::Vision);
        }
        _ => {}
    }
}

/// Format byte size for display
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.1}{}", size, UNITS[unit_idx])
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let (url, model) = {
            let config = self.config.read().map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?;
            (format!("{}/api/chat", config.host), config.model.clone())
        };

        let ollama_request = OllamaRequest {
            model,
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

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        tracing::debug!("Listing Ollama models");

        // Check cache first
        if let Ok(cache) = self.model_cache.read() {
            if let Some((models, cached_at)) = cache.as_ref() {
                if Self::is_cache_valid(*cached_at) {
                    tracing::debug!("Using cached model list");
                    return Ok(models.clone());
                }
            }
        }

        // Cache miss or expired, fetch from API
        let models = self.fetch_models_from_api().await?;

        // Update cache
        if let Ok(mut cache) = self.model_cache.write() {
            *cache = Some((models.clone(), Instant::now()));
        }

        Ok(models)
    }

    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
        tracing::debug!("Getting info for model: {}", model_name);

        // Try to get from cache first
        if let Ok(cache) = self.model_cache.read() {
            if let Some((models, cached_at)) = cache.as_ref() {
                if Self::is_cache_valid(*cached_at) {
                    if let Some(model) = models.iter().find(|m| m.name == model_name) {
                        return Ok(model.clone());
                    }
                }
            }
        }

        // Not in cache, fetch from API
        self.fetch_model_details(model_name).await
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
        // Validate that the model exists by fetching the list
        let models = self.list_models().await?;

        let model_info = models.iter().find(|m| m.name == model_name);

        if model_info.is_none() {
            return Err(XzatomaError::Provider(format!("Model not found: {}", model_name)).into());
        }

        // Check if the model supports tool calling (required for XZatoma)
        let model = model_info.unwrap();
        if !model.supports_capability(ModelCapability::FunctionCalling) {
            return Err(XzatomaError::Provider(format!(
                "Model '{}' does not support tool calling. XZatoma requires models with tool/function calling support. Try llama3.2:latest, llama3.3:latest, or mistral:latest instead.",
                model_name
            )).into());
        }

        // Update the model in the config
        let mut config = self.config.write().map_err(|_| {
            XzatomaError::Provider("Failed to acquire write lock on config".to_string())
        })?;
        config.model = model_name.clone();
        drop(config);

        // Invalidate cache to ensure fresh model list next time
        self.invalidate_cache();

        tracing::info!("Switched Ollama model to: {}", model_name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
        };
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_ollama_provider_host() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.host(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_provider_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.model(), "llama3.2:latest");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
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
            model: "llama3.2:latest".to_string(),
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
            model: "llama3.2:latest".to_string(),
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
            model: "llama3.2:latest".to_string(),
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
            model: "llama3.2:latest".to_string(),
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
            model: "llama3.2:latest".to_string(),
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

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1048576), "1.0MB");
        assert_eq!(format_size(1073741824), "1.0GB");
    }

    #[test]
    fn test_get_context_window_for_model() {
        assert_eq!(get_context_window_for_model("llama2:7b"), 4096);
        assert_eq!(get_context_window_for_model("llama2:13b"), 4096);
        assert_eq!(get_context_window_for_model("mistral:latest"), 8192);
        assert_eq!(get_context_window_for_model("neural-chat:latest"), 8192);
        assert_eq!(get_context_window_for_model("unknown"), 4096);
    }

    #[test]
    fn test_add_model_capabilities_function_calling() {
        // Test model that supports function calling
        let mut model = ModelInfo::new("llama3.2", "Llama 3.2", 4096);
        add_model_capabilities(&mut model, "llama3.2");
        assert!(model.supports_capability(ModelCapability::FunctionCalling));

        // Test model that does NOT support function calling
        let mut model_no_tools = ModelInfo::new("llama3", "Llama 3", 4096);
        add_model_capabilities(&mut model_no_tools, "llama3");
        assert!(!model_no_tools.supports_capability(ModelCapability::FunctionCalling));
    }

    #[test]
    fn test_add_model_capabilities_long_context() {
        let mut model = ModelInfo::new("mistral", "Mistral", 8192);
        add_model_capabilities(&mut model, "mistral");
        assert!(model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::LongContext));

        // Mistral-nemo also supports both
        let mut model_nemo = ModelInfo::new("mistral-nemo", "Mistral Nemo", 8192);
        add_model_capabilities(&mut model_nemo, "mistral-nemo");
        assert!(model_nemo.supports_capability(ModelCapability::FunctionCalling));
        assert!(model_nemo.supports_capability(ModelCapability::LongContext));
    }

    #[test]
    fn test_add_model_capabilities_vision() {
        let mut model = ModelInfo::new("llava", "LLaVA", 4096);
        add_model_capabilities(&mut model, "llava");
        // LLaVA does NOT support function calling, only vision
        assert!(!model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::Vision));
    }

    #[test]
    fn test_is_cache_valid_fresh() {
        let instant = Instant::now();
        assert!(OllamaProvider::is_cache_valid(instant));
    }

    #[test]
    fn test_is_cache_valid_expired() {
        let instant = Instant::now() - Duration::from_secs(400);
        assert!(!OllamaProvider::is_cache_valid(instant));
    }

    #[test]
    fn test_provider_capabilities() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        let capabilities = provider.get_provider_capabilities();

        assert!(capabilities.supports_model_listing);
        assert!(capabilities.supports_model_details);
        assert!(capabilities.supports_model_switching);
        assert!(capabilities.supports_token_counts);
        assert!(!capabilities.supports_streaming);
    }

    #[test]
    fn test_get_current_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test-model".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "test-model");
    }
}
