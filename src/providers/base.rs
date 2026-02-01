//! Base provider trait and common types for XZatoma
//!
//! This module defines the Provider trait that all AI providers must implement,
//! along with common message types, response structures, and metadata types for
//! model discovery and capability querying.

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;

/// Message structure for conversation
///
/// Represents a message in the conversation with the AI provider.
/// Messages can be from the user, assistant, system, or tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender (user, assistant, system, tool)
    pub role: String,
    /// Content of the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional tool calls in the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Optional tool call ID (for tool result messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Creates a new user message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::user("Hello, assistant!");
    /// assert_eq!(msg.role, "user");
    /// ```
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new assistant message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::assistant("Hello, user!");
    /// assert_eq!(msg.role, "assistant");
    /// ```
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new system message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::system("You are a helpful assistant");
    /// assert_eq!(msg.role, "system");
    /// ```
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new tool result message
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - The ID of the tool call this result corresponds to
    /// * `content` - The tool execution result content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::tool_result("call_123", "File contents...");
    /// assert_eq!(msg.role, "tool");
    /// assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    /// ```
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Creates an assistant message with tool calls
    ///
    /// # Arguments
    ///
    /// * `tool_calls` - The tool calls to include
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, ToolCall, FunctionCall};
    ///
    /// let tool_call = ToolCall {
    ///     id: "call_123".to_string(),
    ///     function: FunctionCall {
    ///         name: "read_file".to_string(),
    ///         arguments: r#"{"path":"test.txt"}"#.to_string(),
    ///     },
    /// };
    /// let msg = Message::assistant_with_tools(vec![tool_call]);
    /// assert_eq!(msg.role, "assistant");
    /// assert!(msg.tool_calls.is_some());
    /// ```
    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }
}

/// Function call information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function/tool to call
    pub name: String,
    /// Arguments for the function (as JSON string)
    pub arguments: String,
}

/// Tool call structure
///
/// Represents a request from the AI to execute a tool with specific arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Function call details
    pub function: FunctionCall,
}

/// Model capability feature flags
///
/// Enum representing capabilities that models may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelCapability {
    /// Model supports longer context windows (typically 100k+ tokens)
    LongContext,
    /// Model supports function calling/tool use
    FunctionCalling,
    /// Model supports vision/image understanding
    Vision,
    /// Model supports streaming responses
    Streaming,
    /// Model supports JSON output mode
    JsonMode,
}

impl std::fmt::Display for ModelCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LongContext => write!(f, "LongContext"),
            Self::FunctionCalling => write!(f, "FunctionCalling"),
            Self::Vision => write!(f, "Vision"),
            Self::Streaming => write!(f, "Streaming"),
            Self::JsonMode => write!(f, "JsonMode"),
        }
    }
}

/// Default context window size when not provided by API
const DEFAULT_CONTEXT_WINDOW: usize = 4096;

/// Token usage information from a completion
///
/// Tracks the number of tokens used in prompts and completions,
/// as reported by the AI provider.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: usize,
    /// Number of tokens in the completion
    pub completion_tokens: usize,
    /// Total tokens used (prompt + completion)
    pub total_tokens: usize,
}

impl TokenUsage {
    /// Create a new TokenUsage instance
    ///
    /// # Arguments
    ///
    /// * `prompt_tokens` - Number of prompt tokens
    /// * `completion_tokens` - Number of completion tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::TokenUsage;
    ///
    /// let usage = TokenUsage::new(100, 50);
    /// assert_eq!(usage.prompt_tokens, 100);
    /// assert_eq!(usage.completion_tokens, 50);
    /// assert_eq!(usage.total_tokens, 150);
    /// ```
    pub fn new(prompt_tokens: usize, completion_tokens: usize) -> Self {
        let total_tokens = prompt_tokens + completion_tokens;
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens,
        }
    }
}

/// Model information and capabilities
///
/// Contains metadata about an available AI model, including its name,
/// context window, and supported capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique identifier for the model (e.g., "gpt-4", "llama3.2:latest")
    pub name: String,
    /// Display name for user-friendly presentation (e.g., "GPT-4 Turbo")
    pub display_name: String,
    /// Maximum context window size in tokens
    pub context_window: usize,
    /// Supported capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Provider-specific metadata (key-value pairs)
    pub provider_specific: HashMap<String, String>,
}

impl ModelInfo {
    /// Create a new ModelInfo instance
    ///
    /// # Arguments
    ///
    /// * `name` - Model identifier
    /// * `display_name` - User-friendly display name
    /// * `context_window` - Context window size in tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ModelInfo;
    ///
    /// let model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// assert_eq!(model.name, "gpt-4");
    /// assert_eq!(model.context_window, 8192);
    /// ```
    pub fn new(
        name: impl Into<String>,
        display_name: impl Into<String>,
        context_window: usize,
    ) -> Self {
        Self {
            name: name.into(),
            display_name: display_name.into(),
            context_window,
            capabilities: Vec::new(),
            provider_specific: HashMap::new(),
        }
    }

    /// Add a capability to this model
    ///
    /// # Arguments
    ///
    /// * `capability` - Capability to add
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.add_capability(ModelCapability::FunctionCalling);
    /// assert!(model.capabilities.contains(&ModelCapability::FunctionCalling));
    /// ```
    pub fn add_capability(&mut self, capability: ModelCapability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
    }

    /// Check if this model supports a capability
    ///
    /// # Arguments
    ///
    /// * `capability` - Capability to check for
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.add_capability(ModelCapability::FunctionCalling);
    /// assert!(model.supports_capability(ModelCapability::FunctionCalling));
    /// assert!(!model.supports_capability(ModelCapability::Vision));
    /// ```
    pub fn supports_capability(&self, capability: ModelCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Set provider-specific metadata
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::ModelInfo;
    ///
    /// let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// model.set_provider_metadata("version", "2024-01");
    /// assert_eq!(model.provider_specific.get("version"), Some(&"2024-01".to_string()));
    /// ```
    pub fn set_provider_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.provider_specific.insert(key.into(), value.into());
    }

    /// Add capabilities and return self for builder pattern
    ///
    /// # Arguments
    ///
    /// * `capabilities` - Vector of capabilities to add
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelCapability};
    ///
    /// let model = ModelInfo::new("gpt-4", "GPT-4", 8192)
    ///     .with_capabilities(vec![
    ///         ModelCapability::FunctionCalling,
    ///         ModelCapability::Vision,
    ///     ]);
    /// assert_eq!(model.capabilities.len(), 2);
    /// ```
    pub fn with_capabilities(mut self, capabilities: Vec<ModelCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// Extended model information with full provider API data
///
/// This structure extends ModelInfo with additional fields from provider APIs
/// that are useful for summary output and advanced tooling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoSummary {
    /// Core model information
    pub info: ModelInfo,

    /// Provider API state (e.g., "enabled", "disabled")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,

    /// Maximum prompt tokens allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_prompt_tokens: Option<usize>,

    /// Maximum completion tokens allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<usize>,

    /// Whether the model supports tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_tool_calls: Option<bool>,

    /// Whether the model supports vision/image input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,

    /// Raw provider-specific data (fallback for unknown fields)
    pub raw_data: serde_json::Value,
}

impl ModelInfoSummary {
    /// Create summary from core ModelInfo
    ///
    /// # Arguments
    ///
    /// * `info` - Core model information
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelInfoSummary};
    ///
    /// let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// let summary = ModelInfoSummary::from_model_info(info);
    /// assert_eq!(summary.info.name, "gpt-4");
    /// assert!(summary.state.is_none());
    /// ```
    pub fn from_model_info(info: ModelInfo) -> Self {
        Self {
            info,
            state: None,
            max_prompt_tokens: None,
            max_completion_tokens: None,
            supports_tool_calls: None,
            supports_vision: None,
            raw_data: serde_json::Value::Null,
        }
    }

    /// Create summary with full data
    ///
    /// # Arguments
    ///
    /// * `info` - Core model information
    /// * `state` - Provider API state
    /// * `max_prompt_tokens` - Maximum prompt tokens
    /// * `max_completion_tokens` - Maximum completion tokens
    /// * `supports_tool_calls` - Tool calling support flag
    /// * `supports_vision` - Vision support flag
    /// * `raw_data` - Raw provider-specific data
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{ModelInfo, ModelInfoSummary};
    /// use serde_json;
    ///
    /// let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
    /// let summary = ModelInfoSummary::new(
    ///     info,
    ///     Some("enabled".to_string()),
    ///     Some(6144),
    ///     Some(2048),
    ///     Some(true),
    ///     Some(true),
    ///     serde_json::json!({"version": "2024-01"}),
    /// );
    /// assert_eq!(summary.state, Some("enabled".to_string()));
    /// assert_eq!(summary.supports_tool_calls, Some(true));
    /// ```
    pub fn new(
        info: ModelInfo,
        state: Option<String>,
        max_prompt_tokens: Option<usize>,
        max_completion_tokens: Option<usize>,
        supports_tool_calls: Option<bool>,
        supports_vision: Option<bool>,
        raw_data: serde_json::Value,
    ) -> Self {
        Self {
            info,
            state,
            max_prompt_tokens,
            max_completion_tokens,
            supports_tool_calls,
            supports_vision,
            raw_data,
        }
    }
}

/// Provider capabilities
/// Provider-level capabilities and features
///
/// Describes which features and operations a provider supports.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProviderCapabilities {
    /// Provider supports listing available models
    pub supports_model_listing: bool,
    /// Provider supports querying detailed model information
    pub supports_model_details: bool,
    /// Provider supports changing the active model
    pub supports_model_switching: bool,
    /// Provider returns token usage information in responses
    pub supports_token_counts: bool,
    /// Provider supports streaming responses
    pub supports_streaming: bool,
}

/// Completion response with message and optional token usage
///
/// Contains both the response message and metadata about token usage.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// The response message from the AI
    pub message: Message,
    /// Optional token usage information
    pub usage: Option<TokenUsage>,
}

impl CompletionResponse {
    /// Create a new CompletionResponse
    ///
    /// # Arguments
    ///
    /// * `message` - The response message
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message};
    ///
    /// let response = CompletionResponse::new(Message::assistant("Hello!"));
    /// assert_eq!(response.message.role, "assistant");
    /// assert!(response.usage.is_none());
    /// ```
    pub fn new(message: Message) -> Self {
        Self {
            message,
            usage: None,
        }
    }

    /// Create a new CompletionResponse with token usage
    ///
    /// # Arguments
    ///
    /// * `message` - The response message
    /// * `usage` - Token usage information
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{CompletionResponse, Message, TokenUsage};
    ///
    /// let usage = TokenUsage::new(100, 50);
    /// let response = CompletionResponse::with_usage(Message::assistant("Hello!"), usage);
    /// assert_eq!(response.message.role, "assistant");
    /// assert!(response.usage.is_some());
    /// ```
    pub fn with_usage(message: Message, usage: TokenUsage) -> Self {
        Self {
            message,
            usage: Some(usage),
        }
    }
}

/// Provider trait for AI providers
///
/// All AI providers (Copilot, Ollama, etc.) must implement this trait.
/// The trait provides a common interface for completing conversations
/// with tool support and model management capabilities.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::{Provider, Message, CompletionResponse};
/// use xzatoma::error::Result;
/// use async_trait::async_trait;
///
/// struct MyProvider;
///
/// #[async_trait]
/// impl Provider for MyProvider {
///     async fn complete(
///         &self,
///         messages: &[Message],
///         tools: &[serde_json::Value],
///     ) -> Result<CompletionResponse> {
///         // Implementation here
///         Ok(CompletionResponse::new(Message::assistant("Response")))
///     }
/// }
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
    /// Completes a conversation with the given messages and available tools
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools for the assistant to use (as JSON schemas)
    ///
    /// # Returns
    ///
    /// Returns the assistant's response message along with token usage information
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails or response is invalid
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse>;

    /// List available models for this provider
    ///
    /// # Returns
    ///
    /// Returns a vector of available models
    ///
    /// # Errors
    ///
    /// Returns error if the provider doesn't support model listing
    /// or if the API call fails
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// model listing is not supported by this provider.
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Err(crate::error::XzatomaError::Provider(
            "Model listing is not supported by this provider".to_string(),
        )
        .into())
    }

    /// Get detailed information about a specific model
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed information about the model
    ///
    /// # Errors
    ///
    /// Returns error if the provider doesn't support detailed model info
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// detailed model information is not supported.
    async fn get_model_info(&self, _model_name: &str) -> Result<ModelInfo> {
        Err(crate::error::XzatomaError::Provider(
            "Detailed model information is not supported by this provider".to_string(),
        )
        .into())
    }

    /// Get the name of the currently active model
    ///
    /// # Returns
    ///
    /// Returns the name of the currently active model
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a generic unavailable message.
    fn get_current_model(&self) -> Result<String> {
        Err(crate::error::XzatomaError::Provider(
            "Current model information is not available from this provider".to_string(),
        )
        .into())
    }

    /// Get the capabilities of this provider
    ///
    /// # Returns
    ///
    /// Returns the provider's capabilities
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a capabilities struct with
    /// all features disabled.
    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    /// Change the active model (if supported)
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model to switch to
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if the model was switched successfully
    ///
    /// # Errors
    ///
    /// Returns error if the provider doesn't support model switching
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// model switching is not supported.
    async fn set_model(&mut self, _model_name: String) -> Result<()> {
        Err(crate::error::XzatomaError::Provider(
            "Model switching is not supported by this provider".to_string(),
        )
        .into())
    }

    /// List models with full summary data
    ///
    /// # Returns
    ///
    /// Returns a vector of models with extended summary information
    ///
    /// # Errors
    ///
    /// Returns error if the provider doesn't support model listing
    /// or if the API call fails
    ///
    /// # Default Implementation
    ///
    /// The default implementation converts basic ModelInfo to ModelInfoSummary.
    /// Providers can override this to provide additional summary fields.
    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models = self.list_models().await?;
        Ok(models
            .into_iter()
            .map(ModelInfoSummary::from_model_info)
            .collect())
    }

    /// Get model info with full summary data
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed model information with extended summary data
    ///
    /// # Errors
    ///
    /// Returns error if the provider doesn't support detailed model info
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation converts basic ModelInfo to ModelInfoSummary.
    /// Providers can override this to provide additional summary fields.
    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let info = self.get_model_info(model_name).await?;
        Ok(ModelInfoSummary::from_model_info(info))
    }
}

/// Validates message sequence and removes orphan tool messages
///
/// Orphan tool messages are those that don't have a corresponding preceding
/// assistant message with matching tool_calls. This validates message integrity
/// and prevents provider API errors (e.g., 400 Bad Request from Copilot API).
///
/// An orphan tool message is:
/// - A message with role="tool" but no matching assistant message with tool_calls
/// - A message with role="tool" but no tool_call_id field
///
/// # Arguments
///
/// * `messages` - The messages to validate
///
/// # Returns
///
/// Returns a vector of validated messages with orphans removed and warnings logged
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{Message, validate_message_sequence};
///
/// let messages = vec![
///     Message::user("Do something"),
///     Message::tool_result("call_123", "Result"),
/// ];
/// let validated = validate_message_sequence(&messages);
/// assert_eq!(validated.len(), 1); // Orphan tool removed, only user remains
/// ```
pub fn validate_message_sequence(messages: &[Message]) -> Vec<Message> {
    use std::collections::HashSet;

    // First pass: collect all tool_call IDs from assistant messages with tool_calls
    let mut valid_tool_ids: HashSet<String> = HashSet::new();
    for message in messages {
        if message.role == "assistant" {
            if let Some(tool_calls) = &message.tool_calls {
                for tool_call in tool_calls {
                    valid_tool_ids.insert(tool_call.id.clone());
                }
            }
        }
    }

    // Second pass: filter out orphan tool messages
    messages
        .iter()
        .filter_map(|message| {
            // Tool messages must have a tool_call_id and a matching assistant message
            if message.role == "tool" {
                if let Some(tool_call_id) = &message.tool_call_id {
                    if !valid_tool_ids.contains(tool_call_id) {
                        tracing::warn!(
                            "Dropping orphan tool message with tool_call_id: {}",
                            tool_call_id
                        );
                        return None;
                    }
                } else {
                    tracing::warn!("Dropping tool message without tool_call_id");
                    return None;
                }
            }

            Some(message.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_user_with_string() {
        let msg = Message::user(String::from("Hello"));
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hi there".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("System prompt");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, Some("System prompt".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("call_123", "result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, Some("result".to_string()));
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_message_assistant_with_tools() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let msg = Message::assistant_with_tools(vec![tool_call]);
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Test\""));
    }

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: r#"{"arg":"value"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("\"id\":\"call_123\""));
        assert!(json.contains("\"name\":\"test_tool\""));
        assert!(json.contains("\"arguments\""));
    }

    #[test]
    fn test_function_call() {
        let func_call = FunctionCall {
            name: "read_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        };
        assert_eq!(func_call.name, "read_file");
        assert!(func_call.arguments.contains("path"));
    }

    #[test]
    fn test_token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_zero() {
        let usage = TokenUsage::new(0, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage::new(100, 50);
        let json = serde_json::to_string(&usage).unwrap();
        let deserialized: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.prompt_tokens, 100);
        assert_eq!(deserialized.completion_tokens, 50);
    }

    #[test]
    fn test_model_capability_display() {
        assert_eq!(ModelCapability::LongContext.to_string(), "LongContext");
        assert_eq!(
            ModelCapability::FunctionCalling.to_string(),
            "FunctionCalling"
        );
        assert_eq!(ModelCapability::Vision.to_string(), "Vision");
        assert_eq!(ModelCapability::Streaming.to_string(), "Streaming");
        assert_eq!(ModelCapability::JsonMode.to_string(), "JsonMode");
    }

    #[test]
    fn test_model_capability_serialization() {
        let cap = ModelCapability::LongContext;
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: ModelCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ModelCapability::LongContext);
    }

    #[test]
    fn test_model_info_creation() {
        let model = ModelInfo::new("gpt-4", "GPT-4 Turbo", 8192);
        assert_eq!(model.name, "gpt-4");
        assert_eq!(model.display_name, "GPT-4 Turbo");
        assert_eq!(model.context_window, 8192);
        assert!(model.capabilities.is_empty());
        assert!(model.provider_specific.is_empty());
    }

    #[test]
    fn test_model_info_add_capability() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        assert!(model.capabilities.is_empty());

        model.add_capability(ModelCapability::FunctionCalling);
        assert_eq!(model.capabilities.len(), 1);
        assert!(model
            .capabilities
            .contains(&ModelCapability::FunctionCalling));

        model.add_capability(ModelCapability::FunctionCalling);
        assert_eq!(model.capabilities.len(), 1);
    }

    #[test]
    fn test_model_info_supports_capability() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.add_capability(ModelCapability::FunctionCalling);
        model.add_capability(ModelCapability::Vision);

        assert!(model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::Vision));
        assert!(!model.supports_capability(ModelCapability::Streaming));
    }

    #[test]
    fn test_model_info_provider_metadata() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.set_provider_metadata("version", "2024-01");
        model.set_provider_metadata("region", "us-east-1");

        assert_eq!(
            model.provider_specific.get("version"),
            Some(&"2024-01".to_string())
        );
        assert_eq!(
            model.provider_specific.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_model_info_serialization() {
        let mut model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        model.add_capability(ModelCapability::FunctionCalling);
        model.set_provider_metadata("version", "2024-01");

        let json = serde_json::to_string(&model).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "gpt-4");
        assert_eq!(deserialized.context_window, 8192);
        assert_eq!(deserialized.capabilities.len(), 1);
        assert_eq!(
            deserialized.provider_specific.get("version"),
            Some(&"2024-01".to_string())
        );
    }

    #[test]
    fn test_provider_capabilities_default() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.supports_model_listing);
        assert!(!caps.supports_model_details);
        assert!(!caps.supports_model_switching);
        assert!(!caps.supports_token_counts);
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn test_completion_response_new() {
        let msg = Message::assistant("Hello!");
        let response = CompletionResponse::new(msg);

        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content, Some("Hello!".to_string()));
        assert!(response.usage.is_none());
    }

    #[test]
    fn test_completion_response_with_usage() {
        let msg = Message::assistant("Hello!");
        let usage = TokenUsage::new(100, 50);
        let response = CompletionResponse::with_usage(msg, usage);

        assert_eq!(response.message.role, "assistant");
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().prompt_tokens, 100);
        assert_eq!(response.usage.unwrap().completion_tokens, 50);
    }

    #[test]
    fn test_default_list_models_error() {
        use async_trait::async_trait;

        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let provider = MockProvider;
            let result = provider.list_models().await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_default_get_model_info_error() {
        use async_trait::async_trait;

        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let provider = MockProvider;
            let result = provider.get_model_info("gpt-4").await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_default_get_current_model_error() {
        use async_trait::async_trait;

        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let provider = MockProvider;
        let result = provider.get_current_model();
        assert!(result.is_err());
    }

    #[test]
    fn test_default_set_model_error() {
        use async_trait::async_trait;

        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut provider = MockProvider;
            let result = provider.set_model("gpt-4".to_string()).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_provider_capabilities_creation() {
        let mut caps = ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: true,
            supports_model_switching: false,
            supports_token_counts: true,
            supports_streaming: true,
        };

        assert!(caps.supports_model_listing);
        assert!(caps.supports_model_details);
        assert!(!caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(caps.supports_streaming);

        caps.supports_model_switching = true;
        assert!(caps.supports_model_switching);
    }

    #[test]
    fn test_model_info_with_capabilities() {
        let model = ModelInfo::new("gpt-4", "GPT-4", 8192).with_capabilities(vec![
            ModelCapability::FunctionCalling,
            ModelCapability::Vision,
        ]);

        assert_eq!(model.capabilities.len(), 2);
        assert!(model.supports_capability(ModelCapability::FunctionCalling));
        assert!(model.supports_capability(ModelCapability::Vision));
    }

    #[test]
    fn test_model_info_summary_from_model_info() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::from_model_info(info);

        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.info.display_name, "GPT-4");
        assert_eq!(summary.info.context_window, 8192);
        assert!(summary.state.is_none());
        assert!(summary.max_prompt_tokens.is_none());
        assert!(summary.max_completion_tokens.is_none());
        assert!(summary.supports_tool_calls.is_none());
        assert!(summary.supports_vision.is_none());
        assert_eq!(summary.raw_data, serde_json::Value::Null);
    }

    #[test]
    fn test_model_info_summary_new() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let raw = serde_json::json!({"version": "2024-01"});
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(true),
            raw.clone(),
        );

        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.state, Some("enabled".to_string()));
        assert_eq!(summary.max_prompt_tokens, Some(6144));
        assert_eq!(summary.max_completion_tokens, Some(2048));
        assert_eq!(summary.supports_tool_calls, Some(true));
        assert_eq!(summary.supports_vision, Some(true));
        assert_eq!(summary.raw_data, raw);
    }

    #[test]
    fn test_model_info_summary_serialization() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(false),
            serde_json::json!({"test": "data"}),
        );

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"name\":\"gpt-4\""));
        assert!(json.contains("\"state\":\"enabled\""));
        assert!(json.contains("\"max_prompt_tokens\":6144"));
        assert!(json.contains("\"supports_tool_calls\":true"));
        assert!(json.contains("\"supports_vision\":false"));
    }

    #[test]
    fn test_validate_message_sequence_drops_orphan_tool() {
        let messages = vec![
            Message::user("Do something"),
            Message::tool_result("call_123", "Result"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, "user");
    }

    #[test]
    fn test_validate_message_sequence_preserves_valid_pair() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_func".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let messages = vec![
            Message::user("Do something"),
            Message::assistant_with_tools(vec![tool_call]),
            Message::tool_result("call_123", "Result"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 3);
        assert_eq!(validated[0].role, "user");
        assert_eq!(validated[1].role, "assistant");
        assert_eq!(validated[2].role, "tool");
        assert_eq!(validated[2].tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_validate_message_sequence_allows_user_and_system() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Question"),
            Message::assistant("Answer"),
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 3);
        assert_eq!(validated[0].role, "system");
        assert_eq!(validated[1].role, "user");
        assert_eq!(validated[2].role, "assistant");
    }

    #[test]
    fn test_validate_message_sequence_drops_tool_without_id() {
        let messages = vec![
            Message::user("Do something"),
            Message {
                role: "tool".to_string(),
                content: Some("Result".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let validated = validate_message_sequence(&messages);

        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, "user");
    }

    #[test]
    fn test_model_info_summary_deserialization() {
        let json = r#"{
            "info": {
                "name": "gpt-4",
                "display_name": "GPT-4",
                "context_window": 8192,
                "capabilities": [],
                "provider_specific": {}
            },
            "state": "enabled",
            "max_prompt_tokens": 6144,
            "max_completion_tokens": 2048,
            "supports_tool_calls": true,
            "supports_vision": false,
            "raw_data": {"test": "data"}
        }"#;

        let summary: ModelInfoSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.state, Some("enabled".to_string()));
        assert_eq!(summary.max_prompt_tokens, Some(6144));
        assert_eq!(summary.max_completion_tokens, Some(2048));
        assert_eq!(summary.supports_tool_calls, Some(true));
        assert_eq!(summary.supports_vision, Some(false));
    }
}
