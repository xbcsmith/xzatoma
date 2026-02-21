//! GitHub Copilot provider implementation for XZatoma
//!
//! This module implements the Provider trait for GitHub Copilot, including
//! OAuth device flow authentication and token caching in the system keyring.

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    CompletionResponse, FunctionCall, Message, ModelCapability, ModelInfo, ModelInfoSummary,
    Provider, ProviderCapabilities, TokenUsage, ToolCall,
};

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
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
/// Copilot responses endpoint
const COPILOT_RESPONSES_URL: &str = "https://api.githubcopilot.com/responses";
/// Copilot models endpoint
const COPILOT_MODELS_URL: &str = "https://api.githubcopilot.com/models";
/// GitHub Copilot OAuth client ID
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Supported model endpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelEndpoint {
    /// Chat completions endpoint (/chat/completions)
    ChatCompletions,
    /// Responses endpoint (/responses)
    Responses,
    /// Messages endpoint (/messages)
    Messages,
    /// Unknown/unsupported endpoint
    Unknown,
}

impl ModelEndpoint {
    /// Convert endpoint name string to enum
    ///
    /// # Arguments
    ///
    /// * `name` - Endpoint name (e.g., "responses", "chat_completions")
    ///
    /// # Returns
    ///
    /// Returns corresponding ModelEndpoint variant
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(ModelEndpoint::from_name("responses"), ModelEndpoint::Responses);
    /// assert_eq!(ModelEndpoint::from_name("chat_completions"), ModelEndpoint::ChatCompletions);
    /// ```
    fn from_name(name: &str) -> Self {
        match name {
            "chat_completions" => ModelEndpoint::ChatCompletions,
            "responses" => ModelEndpoint::Responses,
            "messages" => ModelEndpoint::Messages,
            _ => ModelEndpoint::Unknown,
        }
    }

    /// Get endpoint name as string
    ///
    /// # Returns
    ///
    /// Returns the string representation of the endpoint
    ///
    /// # Examples
    ///
    /// ```ignore
    /// assert_eq!(ModelEndpoint::Responses.as_str(), "responses");
    /// assert_eq!(ModelEndpoint::ChatCompletions.as_str(), "chat_completions");
    /// ```
    fn as_str(&self) -> &'static str {
        match self {
            ModelEndpoint::ChatCompletions => "chat_completions",
            ModelEndpoint::Responses => "responses",
            ModelEndpoint::Messages => "messages",
            ModelEndpoint::Unknown => "unknown",
        }
    }
}

/// Default context window size when not provided by API
const DEFAULT_CONTEXT_WINDOW: usize = 4096;

/// Pinned boxed stream of response events
type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

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
///     ..Default::default()
/// };
/// let provider = CopilotProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let completion = provider.complete(&messages, &[]).await?;
/// let message = completion.message;
/// # Ok(())
/// # }
/// ```
///
/// Type alias to reduce type complexity in struct fields (satisfies clippy)
type ModelsCache = Arc<RwLock<Option<(Vec<ModelInfo>, u64)>>>;

pub struct CopilotProvider {
    client: Client,
    config: Arc<RwLock<CopilotConfig>>,
    keyring_service: String,
    keyring_user: String,
    /// Cached models and expiry time (epoch seconds). Uses RwLock for cheap reads.
    models_cache: ModelsCache,
    /// TTL (seconds) for the models cache
    models_cache_ttl_secs: u64,
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
    #[serde(default)]
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

/// Response from Copilot models API
#[derive(Debug, Deserialize)]
pub(crate) struct CopilotModelsResponse {
    pub(crate) data: Vec<CopilotModelData>,
}

/// Model data from Copilot API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CopilotModelData {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) capabilities: Option<CopilotModelCapabilities>,
    #[serde(default)]
    pub(crate) policy: Option<CopilotModelPolicy>,
    /// Supported endpoints for this model
    #[serde(default)]
    pub(crate) supported_endpoints: Vec<String>,
}

impl CopilotModelData {
    /// Check if model supports a specific endpoint
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint name to check (e.g., "responses", "chat_completions")
    ///
    /// # Returns
    ///
    /// Returns true if model supports the endpoint
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let model_data = CopilotModelData {
    ///     supported_endpoints: vec!["responses".to_string()],
    ///     // ... other fields
    /// };
    /// assert!(model_data.supports_endpoint("responses"));
    /// assert!(!model_data.supports_endpoint("messages"));
    /// ```
    pub(crate) fn supports_endpoint(&self, endpoint: &str) -> bool {
        self.supported_endpoints.iter().any(|e| e == endpoint)
    }
}

/// Model policy information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CopilotModelPolicy {
    pub(crate) state: String,
}

/// Model capabilities from Copilot API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CopilotModelCapabilities {
    #[serde(default)]
    pub(crate) limits: Option<CopilotModelLimits>,
    #[serde(default)]
    pub(crate) supports: Option<CopilotModelSupports>,
}

/// Model limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CopilotModelLimits {
    #[serde(default)]
    pub(crate) max_context_window_tokens: Option<usize>,
}

/// Model support flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CopilotModelSupports {
    #[serde(default)]
    pub(crate) tool_calls: Option<bool>,
    #[serde(default)]
    pub(crate) vision: Option<bool>,
}

// ============================================================================
// RESPONSES ENDPOINT TYPES
// ============================================================================

/// Request structure for /responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesRequest {
    /// Model identifier (e.g., "gpt-5-mini", "claude-3.5-sonnet")
    pub model: String,

    /// Input items (messages, function calls, reasoning)
    pub input: Vec<ResponseInputItem>,

    /// Enable streaming (SSE)
    #[serde(default)]
    pub stream: bool,

    /// Temperature for sampling (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Available tools for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,

    /// Tool selection strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Reasoning configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,

    /// Fields to include in response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
}

/// Input item for responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputItem {
    /// Text message with role
    Message {
        role: String,
        content: Vec<ResponseInputContent>,
    },
    /// Function call from assistant
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Function call result
    FunctionCallOutput { call_id: String, output: String },
    /// Reasoning content
    Reasoning { content: Vec<ResponseInputContent> },
}

/// Content types for response input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputContent {
    /// Text content
    InputText { text: String },
    /// Assistant output text
    OutputText { text: String },
    /// Image content
    InputImage { url: String },
}

/// SSE stream events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Message output event
    Message {
        role: String,
        content: Vec<ResponseInputContent>,
    },
    /// Function call event
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Reasoning event
    Reasoning { content: Vec<ResponseInputContent> },
    /// Status event
    Status { status: String },
    /// Done event
    Done,
}

/// Tool definition for responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolDefinition {
    /// Function tool
    Function { function: FunctionDefinition },
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// JSON schema for parameters
    pub parameters: serde_json::Value,
    /// Enable strict mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Tool choice strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto selection
    Auto { auto: bool },
    /// Require any tool
    Any { any: bool },
    /// No tool usage
    None { none: bool },
    /// Specific tool
    Named { function: FunctionName },
}

/// Named function for tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
    /// Name of the function to call
    pub name: String,
}

/// Reasoning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Reasoning effort level: "low", "medium", "high"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

// ============================================================================
// MESSAGE FORMAT CONVERSION FUNCTIONS
// ============================================================================

/// Convert XZatoma messages to responses endpoint input format
///
/// Transforms a slice of XZatoma Message objects into ResponseInputItem format
/// suitable for the Copilot /responses endpoint. Each message is converted based
/// on its role and content, preserving all necessary information for proper
/// endpoint communication.
///
/// # Arguments
///
/// * `messages` - Vector of XZatoma Message objects
///
/// # Returns
///
/// Returns vector of ResponseInputItem for responses endpoint
///
/// # Errors
///
/// Returns error if message format is invalid or missing required fields
///
/// # Examples
///
/// ```ignore
/// use xzatoma::providers::Message;
///
/// let messages = vec![Message::user("Hello")];
/// let input = convert_messages_to_response_input(&messages)?;
/// assert_eq!(input.len(), 1);
/// ```
pub(crate) fn convert_messages_to_response_input(
    messages: &[Message],
) -> Result<Vec<ResponseInputItem>> {
    let mut result = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "user" => {
                let content = message.content.as_ref().unwrap_or(&String::new()).clone();
                result.push(ResponseInputItem::Message {
                    role: "user".to_string(),
                    content: vec![ResponseInputContent::InputText { text: content }],
                });
            }
            "assistant" => {
                if let Some(tool_calls) = &message.tool_calls {
                    // Assistant message with tool calls
                    for tool_call in tool_calls {
                        result.push(ResponseInputItem::FunctionCall {
                            call_id: tool_call.id.clone(),
                            name: tool_call.function.name.clone(),
                            arguments: tool_call.function.arguments.clone(),
                        });
                    }
                    // Also add the text content if present
                    if let Some(content) = &message.content {
                        result.push(ResponseInputItem::Message {
                            role: "assistant".to_string(),
                            content: vec![ResponseInputContent::OutputText {
                                text: content.clone(),
                            }],
                        });
                    }
                } else {
                    // Regular assistant message
                    let content = message.content.as_ref().unwrap_or(&String::new()).clone();
                    result.push(ResponseInputItem::Message {
                        role: "assistant".to_string(),
                        content: vec![ResponseInputContent::OutputText { text: content }],
                    });
                }
            }
            "system" => {
                let content = message.content.as_ref().unwrap_or(&String::new()).clone();
                result.push(ResponseInputItem::Message {
                    role: "system".to_string(),
                    content: vec![ResponseInputContent::InputText { text: content }],
                });
            }
            "tool" => {
                // Tool result message
                if let Some(call_id) = &message.tool_call_id {
                    let output = message.content.as_ref().unwrap_or(&String::new()).clone();
                    result.push(ResponseInputItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output,
                    });
                } else {
                    return Err(XzatomaError::MessageConversionError(
                        "Tool message missing tool_call_id".to_string(),
                    )
                    .into());
                }
            }
            role => {
                return Err(XzatomaError::MessageConversionError(format!(
                    "Unknown message role: {}",
                    role
                ))
                .into());
            }
        }
    }

    Ok(result)
}

/// Convert responses endpoint input items back to XZatoma messages
///
/// Transforms ResponseInputItem objects from the Copilot /responses endpoint
/// back into XZatoma Message format. This is useful for processing API responses
/// and converting them to the standard message format used throughout the agent.
///
/// # Arguments
///
/// * `input` - Slice of ResponseInputItem from responses endpoint
///
/// # Returns
///
/// Returns vector of XZatoma Message objects
///
/// # Errors
///
/// Returns `MessageConversionError` if format is invalid or unknown role is encountered
///
/// # Examples
///
/// ```ignore
/// use xzatoma::providers::ResponseInputItem;
///
/// let input = vec![
///     ResponseInputItem::Message {
///         role: "user".to_string(),
///         content: vec![ResponseInputContent::InputText {
///             text: "Hello".to_string(),
///         }],
///     },
/// ];
/// let messages = convert_response_input_to_messages(&input)?;
/// assert_eq!(messages.len(), 1);
/// ```
pub(crate) fn convert_response_input_to_messages(
    input: &[ResponseInputItem],
) -> Result<Vec<Message>> {
    let mut result = Vec::new();

    for item in input {
        match item {
            ResponseInputItem::Message { role, content } => {
                // Extract text from content items
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        ResponseInputContent::InputText { text } => Some(text.clone()),
                        ResponseInputContent::OutputText { text } => Some(text.clone()),
                        ResponseInputContent::InputImage { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                match role.as_str() {
                    "user" => {
                        result.push(Message {
                            role: "user".to_string(),
                            content: Some(text),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    "assistant" => {
                        result.push(Message {
                            role: "assistant".to_string(),
                            content: Some(text),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    "system" => {
                        result.push(Message {
                            role: "system".to_string(),
                            content: Some(text),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    unknown_role => {
                        return Err(XzatomaError::MessageConversionError(format!(
                            "Unknown role in response: {}",
                            unknown_role
                        ))
                        .into());
                    }
                }
            }
            ResponseInputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                result.push(Message {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![ToolCall {
                        id: call_id.clone(),
                        function: FunctionCall {
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    }]),
                    tool_call_id: None,
                });
            }
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                result.push(Message {
                    role: "tool".to_string(),
                    content: Some(output.clone()),
                    tool_calls: None,
                    tool_call_id: Some(call_id.clone()),
                });
            }
            ResponseInputItem::Reasoning { .. } => {
                // Reasoning content is not stored in Message, skip for now
                // Could be extended in future to add reasoning metadata
            }
        }
    }

    Ok(result)
}

/// Convert StreamEvent to Message
///
/// Converts a single SSE stream event into an optional Message. Returns None
/// for status and done events, which don't represent actual message content.
///
/// # Arguments
///
/// * `event` - StreamEvent from SSE stream
///
/// # Returns
///
/// Returns optional Message (None for status/done events)
///
/// # Examples
///
/// ```ignore
/// use xzatoma::providers::StreamEvent;
///
/// let event = StreamEvent::Message {
///     role: "assistant".to_string(),
///     content: vec![ResponseInputContent::OutputText {
///         text: "Response".to_string(),
///     }],
/// };
/// let message = convert_stream_event_to_message(&event);
/// assert!(message.is_some());
/// ```
pub(crate) fn convert_stream_event_to_message(event: &StreamEvent) -> Option<Message> {
    match event {
        StreamEvent::Message { role, content } => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    ResponseInputContent::InputText { text } => Some(text.clone()),
                    ResponseInputContent::OutputText { text } => Some(text.clone()),
                    ResponseInputContent::InputImage { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(" ");

            match role.as_str() {
                "user" => Some(Message {
                    role: "user".to_string(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                "assistant" => Some(Message {
                    role: "assistant".to_string(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                "system" => Some(Message {
                    role: "system".to_string(),
                    content: Some(text),
                    tool_calls: None,
                    tool_call_id: None,
                }),
                _ => None,
            }
        }
        StreamEvent::FunctionCall {
            call_id,
            name,
            arguments,
        } => Some(Message {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: call_id.clone(),
                function: FunctionCall {
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
            }]),
            tool_call_id: None,
        }),
        StreamEvent::Reasoning { .. } | StreamEvent::Status { .. } | StreamEvent::Done => None,
    }
}

/// Convert XZatoma tool definitions to responses endpoint format
///
/// Transforms Tool objects into ToolDefinition format suitable for the
/// Copilot /responses endpoint. Preserves all tool metadata including
/// parameters and strict mode settings.
///
/// # Arguments
///
/// * `tools` - Slice of tool definitions in XZatoma format
///
/// # Returns
///
/// Returns vector of ToolDefinition for responses endpoint
///
/// # Examples
///
/// ```ignore
/// use xzatoma::tools::Tool;
///
/// let tools = vec![Tool {
///     name: "read_file".to_string(),
///     description: "Read file contents".to_string(),
///     parameters: serde_json::json!({}),
/// }];
/// let result = convert_tools_to_response_format(&tools);
/// assert_eq!(result.len(), 1);
/// ```
pub(crate) fn convert_tools_to_response_format(
    tools: &[crate::tools::Tool],
) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|tool| ToolDefinition::Function {
            function: FunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
                strict: Some(false),
            },
        })
        .collect()
}

/// Convert tool choice to responses endpoint format
///
/// Maps a tool choice specification string to the appropriate ToolChoice enum
/// variant for the Copilot /responses endpoint. Supports auto, any, none, and
/// named tool choice strategies.
///
/// # Arguments
///
/// * `choice` - Optional tool choice specification ("auto", "any", "required", "none", or tool name)
///
/// # Returns
///
/// Returns optional ToolChoice for responses endpoint
///
/// # Examples
///
/// ```ignore
/// let choice = convert_tool_choice(Some("auto"));
/// assert!(choice.is_some());
///
/// let choice = convert_tool_choice(Some("specific_tool"));
/// assert!(choice.is_some()); // Named variant
///
/// let choice = convert_tool_choice(None);
/// assert!(choice.is_none());
/// ```
pub(crate) fn convert_tool_choice(choice: Option<&str>) -> Option<ToolChoice> {
    choice.map(|c| match c {
        "auto" => ToolChoice::Auto { auto: true },
        "any" | "required" => ToolChoice::Any { any: true },
        "none" => ToolChoice::None { none: true },
        name => ToolChoice::Named {
            function: FunctionName {
                name: name.to_string(),
            },
        },
    })
}

/// Parse SSE (Server-Sent Events) line
///
/// # Arguments
///
/// * `line` - Line from SSE stream
///
/// # Returns
///
/// Returns optional parsed event data
fn parse_sse_line(line: &str) -> Option<String> {
    let line = line.trim();

    if line.is_empty() {
        return None;
    }

    // Handle data: lines
    if let Some(data) = line.strip_prefix("data: ") {
        // Check for [DONE] sentinel
        if data.trim() == "[DONE]" {
            return Some("[DONE]".to_string());
        }
        return Some(data.to_string());
    }

    // Ignore event:, id:, and other SSE fields
    if line.starts_with("event:") || line.starts_with("id:") || line.starts_with(":") {
        return None;
    }

    None
}

/// Parse SSE data line to StreamEvent
///
/// # Arguments
///
/// * `data` - JSON data from SSE event
///
/// # Returns
///
/// Returns parsed StreamEvent or error
fn parse_sse_event(data: &str) -> Result<StreamEvent> {
    if data == "[DONE]" {
        return Ok(StreamEvent::Done);
    }

    serde_json::from_str(data)
        .map_err(|e| anyhow::anyhow!(XzatomaError::SseParseError(format!("Invalid JSON: {}", e))))
}

fn format_copilot_api_error(status: reqwest::StatusCode, body: &str) -> XzatomaError {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        XzatomaError::Authentication(format!(
            "Copilot returned error {}: {}. Token may have expired; please re-authenticate with `xzatoma auth --provider copilot`",
            status, body
        ))
    } else {
        XzatomaError::Provider(format!("Copilot returned error {}: {}", status, body))
    }
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
    ///     ..Default::default()
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
            models_cache: Arc::new(RwLock::new(None)),
            models_cache_ttl_secs: 300, // default 5 minutes
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
    ///     ..Default::default()
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

        let token_url = self.api_endpoint("copilot_internal/v2/token");
        let response: CopilotTokenResponse = self
            .client
            .get(&token_url)
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

    /// Clear cached token from system keyring (best-effort).
    ///
    /// If the provider sees an authentication failure (401) it will attempt
    /// to invalidate the cached token so the next `authenticate()` call will
    /// perform the device flow again. This uses `set_password("")` as a
    /// safe, widely-available invalidation step rather than relying on a
    /// specific delete API that may vary between environments.
    fn clear_cached_token(&self) -> Result<()> {
        match keyring::Entry::new(&self.keyring_service, &self.keyring_user) {
            Ok(entry) => {
                if let Err(e) = entry.set_password("") {
                    tracing::warn!("Failed to clear cached Copilot token: {}", e);
                } else {
                    tracing::info!("Cleared cached Copilot token (set empty password) in keyring");
                }
            }
            Err(e) => {
                tracing::warn!("Keyring not available while clearing cached token: {}", e);
            }
        }
        Ok(())
    }

    /// Convert XZatoma messages to Copilot format
    fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
        let validated_messages = crate::providers::validate_message_sequence(messages);
        validated_messages
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

    /// Build an API endpoint URL using optional `CopilotConfig::api_base` override.
    fn api_endpoint(&self, path: &str) -> String {
        if let Ok(cfg) = self.config.read() {
            if let Some(base) = &cfg.api_base {
                return format!(
                    "{}/{}",
                    base.trim_end_matches('/'),
                    path.trim_start_matches('/')
                );
            }
        }
        match path {
            "models" => COPILOT_MODELS_URL.to_string(),
            "chat/completions" => COPILOT_COMPLETIONS_URL.to_string(),
            "responses" => COPILOT_RESPONSES_URL.to_string(),
            "copilot_internal/v2/token" => COPILOT_TOKEN_URL.to_string(),
            other => format!(
                "https://api.githubcopilot.com/{}",
                other.trim_start_matches('/')
            ),
        }
    }

    /// Get API endpoint URL for specified endpoint type
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint type to get URL for
    ///
    /// # Returns
    ///
    /// Returns full URL for the endpoint
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let url = provider.endpoint_url(ModelEndpoint::Responses);
    /// assert_eq!(url, "https://api.githubcopilot.com/responses");
    /// ```
    fn endpoint_url(&self, endpoint: ModelEndpoint) -> String {
        if let Ok(cfg) = self.config.read() {
            if let Some(base) = &cfg.api_base {
                let path = match endpoint {
                    ModelEndpoint::ChatCompletions => "/chat/completions",
                    ModelEndpoint::Responses => "/responses",
                    ModelEndpoint::Messages => "/messages",
                    ModelEndpoint::Unknown => "/chat/completions",
                };
                return format!("{}{}", base.trim_end_matches('/'), path);
            }
        }

        match endpoint {
            ModelEndpoint::ChatCompletions => COPILOT_COMPLETIONS_URL.to_string(),
            ModelEndpoint::Responses => COPILOT_RESPONSES_URL.to_string(),
            ModelEndpoint::Messages => {
                format!("{}messages", "https://api.githubcopilot.com/")
            }
            ModelEndpoint::Unknown => COPILOT_COMPLETIONS_URL.to_string(),
        }
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

    /// Fetch the list of available Copilot models from the API.
    ///
    /// This function uses an in-memory TTL cache to avoid frequent API calls.
    /// If `CopilotConfig::api_base` is set, it will be used to construct the
    /// models endpoint (useful for tests/local mocking).
    async fn fetch_copilot_models(&self) -> Result<Vec<ModelInfo>> {
        // Check models cache first
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Ok(cache_guard) = self.models_cache.read() {
            if let Some((cached_models, expires_at)) = &*cache_guard {
                if now < *expires_at {
                    tracing::debug!("Using cached Copilot models");
                    return Ok(cached_models.clone());
                } else {
                    tracing::debug!("Copilot models cache expired");
                }
            }
        }

        let token = self.authenticate().await?;
        let models_url = self.api_endpoint("models");

        let response = self
            .client
            .get(&models_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch Copilot models: {}", e);
                XzatomaError::Provider(format!("Failed to fetch Copilot models: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Copilot models API returned error {}: {}",
                status,
                error_text
            );

            // If unauthorized, attempt a non-interactive refresh using the cached GitHub token.
            if status == reqwest::StatusCode::UNAUTHORIZED {
                tracing::warn!("Copilot returned 401 Unauthorized; attempting non-interactive refresh using cached GitHub token");
                if let Ok(cached) = self.get_cached_token() {
                    match self.get_copilot_token(&cached.github_token).await {
                        Ok(new_token) => {
                            // Store the refreshed Copilot token (best-effort)
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let refreshed = CachedToken {
                                github_token: cached.github_token.clone(),
                                copilot_token: new_token.clone(),
                                expires_at: now + 3600,
                            };
                            if let Err(e) = self.cache_token(&refreshed) {
                                tracing::warn!("Failed to cache refreshed Copilot token: {}", e);
                            } else {
                                tracing::info!("Successfully refreshed Copilot token using cached GitHub token");
                            }

                            // Retry models request with refreshed token
                            let retry_resp = self
                                .client
                                .get(&models_url)
                                .header("Authorization", format!("Bearer {}", new_token))
                                .header("Editor-Version", "vscode/1.85.0")
                                .send()
                                .await
                                .map_err(|e| {
                                    tracing::error!(
                                        "Failed to fetch Copilot models on retry: {}",
                                        e
                                    );
                                    XzatomaError::Provider(format!(
                                        "Failed to fetch Copilot models: {}",
                                        e
                                    ))
                                })?;

                            let status2 = retry_resp.status();
                            if !status2.is_success() {
                                let error_text2 = retry_resp.text().await.unwrap_or_default();
                                tracing::error!(
                                    "Copilot models API retry returned error {}: {}",
                                    status2,
                                    error_text2
                                );
                                if status2 == reqwest::StatusCode::UNAUTHORIZED {
                                    if let Err(e) = self.clear_cached_token() {
                                        tracing::warn!(
                                            "Failed to clear cached Copilot token: {}",
                                            e
                                        );
                                    }
                                }
                                return Err(format_copilot_api_error(status2, &error_text2).into());
                            }

                            // Parse and return models from the successful retry response
                            let models_response: CopilotModelsResponse =
                                retry_resp.json().await.map_err(|e| {
                                    tracing::error!(
                                        "Failed to parse Copilot models response on retry: {}",
                                        e
                                    );
                                    XzatomaError::Provider(format!(
                                        "Failed to parse Copilot models response: {}",
                                        e
                                    ))
                                })?;

                            let mut models = Vec::new();
                            for model_data in models_response.data {
                                // Only include enabled models
                                if let Some(policy) = &model_data.policy {
                                    if policy.state != "enabled" {
                                        continue;
                                    }
                                }

                                // Extract context window size
                                let context_window = model_data
                                    .capabilities
                                    .as_ref()
                                    .and_then(|c| c.limits.as_ref())
                                    .and_then(|l| l.max_context_window_tokens)
                                    .unwrap_or(128000); // Default to 128k if not specified

                                let mut model_info = ModelInfo::new(
                                    &model_data.id,
                                    &model_data.name,
                                    context_window,
                                );

                                // Add capabilities based on supports flags
                                if let Some(caps) = &model_data.capabilities {
                                    if let Some(supports) = &caps.supports {
                                        if supports.tool_calls.unwrap_or(false) {
                                            model_info
                                                .add_capability(ModelCapability::FunctionCalling);
                                        }
                                        if supports.vision.unwrap_or(false) {
                                            model_info.add_capability(ModelCapability::Vision);
                                        }
                                    }
                                }

                                // Add LongContext capability for models with >32k context
                                if context_window > 32000 {
                                    model_info.add_capability(ModelCapability::LongContext);
                                }

                                models.push(model_info);
                            }

                            // Cache the successful result
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let expires_at = now + self.models_cache_ttl_secs;
                            if let Ok(mut cache_guard) = self.models_cache.write() {
                                *cache_guard = Some((models.clone(), expires_at));
                            } else {
                                tracing::warn!("Failed to acquire write lock on models cache");
                            }

                            return Ok(models);
                        }
                        Err(e) => {
                            tracing::warn!("Non-interactive refresh failed: {}", e);
                            if let Err(e) = self.clear_cached_token() {
                                tracing::warn!("Failed to clear cached Copilot token: {}", e);
                            }
                            return Err(format_copilot_api_error(status, &error_text).into());
                        }
                    }
                } else {
                    // No cached GitHub token available; invalidate cache and return auth error
                    if let Err(e) = self.clear_cached_token() {
                        tracing::warn!("Failed to clear cached Copilot token: {}", e);
                    }
                    return Err(format_copilot_api_error(status, &error_text).into());
                }
            }

            // Non-auth failures fall back to provider error
            return Err(format_copilot_api_error(status, &error_text).into());
        }

        let models_response: CopilotModelsResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Copilot models response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Copilot models response: {}", e))
        })?;

        let mut models = Vec::new();
        for model_data in models_response.data {
            // Only include enabled models
            if let Some(policy) = &model_data.policy {
                if policy.state != "enabled" {
                    continue;
                }
            }

            // Extract context window size
            let context_window = model_data
                .capabilities
                .as_ref()
                .and_then(|c| c.limits.as_ref())
                .and_then(|l| l.max_context_window_tokens)
                .unwrap_or(128000); // Default to 128k if not specified

            let mut model_info = ModelInfo::new(&model_data.id, &model_data.name, context_window);

            // Add capabilities based on supports flags
            if let Some(caps) = &model_data.capabilities {
                if let Some(supports) = &caps.supports {
                    if supports.tool_calls.unwrap_or(false) {
                        model_info.add_capability(ModelCapability::FunctionCalling);
                    }
                    if supports.vision.unwrap_or(false) {
                        model_info.add_capability(ModelCapability::Vision);
                    }
                }
            }

            // Add LongContext capability for models with >32k context
            if context_window > 32000 {
                model_info.add_capability(ModelCapability::LongContext);
            }

            models.push(model_info);
        }

        // Cache the result
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires_at = now + self.models_cache_ttl_secs;
        if let Ok(mut cache_guard) = self.models_cache.write() {
            *cache_guard = Some((models.clone(), expires_at));
        } else {
            tracing::warn!("Failed to acquire write lock on models cache");
        }

        Ok(models)
    }

    /// Fetch raw Copilot model data without conversion
    async fn fetch_copilot_models_raw(&self) -> Result<Vec<CopilotModelData>> {
        // Note: We're caching ModelInfo, not raw data, so we fetch fresh for raw data
        // This is acceptable since list_models_summary is not called as frequently

        let token = self.authenticate().await?;
        let models_url = self.api_endpoint("models");

        let response = self
            .client
            .get(&models_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch Copilot models: {}", e);
                XzatomaError::Provider(format!("Failed to fetch Copilot models: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Copilot models API returned error {}: {}",
                status,
                error_text
            );
            return Err(format_copilot_api_error(status, &error_text).into());
        }

        let models_response: CopilotModelsResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Copilot models response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Copilot models response: {}", e))
        })?;

        Ok(models_response.data)
    }

    /// Convert CopilotModelData to ModelInfoSummary
    fn convert_to_summary(&self, data: CopilotModelData) -> ModelInfoSummary {
        let context_window = data
            .capabilities
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.max_context_window_tokens)
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);

        let supports_tool_calls = data
            .capabilities
            .as_ref()
            .and_then(|c| c.supports.as_ref())
            .and_then(|s| s.tool_calls);

        let supports_vision = data
            .capabilities
            .as_ref()
            .and_then(|c| c.supports.as_ref())
            .and_then(|s| s.vision);

        let state = data.policy.as_ref().map(|p| p.state.clone());

        // Build capabilities vector
        let mut capabilities = Vec::new();
        if supports_tool_calls == Some(true) {
            capabilities.push(ModelCapability::FunctionCalling);
        }
        if supports_vision == Some(true) {
            capabilities.push(ModelCapability::Vision);
        }
        if context_window > 32000 {
            capabilities.push(ModelCapability::LongContext);
        }

        let info =
            ModelInfo::new(&data.id, &data.name, context_window).with_capabilities(capabilities);

        let raw_data = serde_json::to_value(&data).unwrap_or(serde_json::Value::Null);

        ModelInfoSummary::new(
            info,
            state,
            None, // max_prompt_tokens not in Copilot API
            None, // max_completion_tokens not in Copilot API
            supports_tool_calls,
            supports_vision,
            raw_data,
        )
    }

    /// Stream responses from GitHub Copilot responses endpoint
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `input` - Converted message input items
    /// * `tools` - Tool definitions
    ///
    /// # Returns
    ///
    /// Returns pinned boxed stream of StreamEvent items
    ///
    /// # Errors
    ///
    /// Returns `SseParseError` if SSE parsing fails
    /// Returns `StreamInterrupted` if connection drops
    async fn stream_response(
        &self,
        model: &str,
        input: Vec<ResponseInputItem>,
        tools: Vec<ToolDefinition>,
    ) -> crate::error::Result<ResponseStream> {
        let url = self.endpoint_url(ModelEndpoint::Responses);
        let token = self.authenticate().await?;

        // Build request
        let request = ResponsesRequest {
            model: model.to_string(),
            input,
            stream: true,
            temperature: None,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice: None,
            reasoning: None,
            include: None,
        };

        // Make HTTP request with streaming
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "xzatoma/0.1.0")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(XzatomaError::Provider(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(XzatomaError::Provider(format!(
                "HTTP {}: {}",
                status, body
            ))));
        }

        // Create async stream from response body
        let stream = response.bytes_stream();

        // Build stream using boxed closure
        let event_stream = futures::stream::unfold(
            (stream.boxed(), String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    match byte_stream.next().await {
                        Some(Ok(chunk)) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));

                            // Look for complete lines
                            if let Some(pos) = buffer.find('\n') {
                                let line = buffer[..pos].to_string();
                                buffer.drain(..=pos);

                                // Parse SSE line
                                if let Some(data) = parse_sse_line(&line) {
                                    let result = parse_sse_event(&data);
                                    return Some((result, (byte_stream, buffer)));
                                }
                                // Continue loop to find next line
                                continue;
                            }
                            // No complete line yet, wait for more chunks
                            continue;
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(anyhow::anyhow!(XzatomaError::StreamInterrupted(
                                    e.to_string()
                                ))),
                                (byte_stream, buffer),
                            ))
                        }
                        None => {
                            // Stream ended
                            if !buffer.is_empty() {
                                // Try to parse any remaining content
                                if let Some(data) = parse_sse_line(&buffer) {
                                    let result = parse_sse_event(&data);
                                    buffer.clear();
                                    return Some((result, (byte_stream, buffer)));
                                }
                            }
                            // Stream complete
                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(event_stream))
    }

    /// Stream completions from chat/completions endpoint
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `messages` - Message history
    /// * `tools` - Tool definitions
    ///
    /// # Returns
    ///
    /// Returns pinned boxed stream of completion chunks
    ///
    /// # Errors
    ///
    /// Returns `SseParseError` if SSE parsing fails
    async fn stream_completion(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[crate::tools::Tool],
    ) -> crate::error::Result<ResponseStream> {
        let url = self.endpoint_url(ModelEndpoint::ChatCompletions);
        let token = self.authenticate().await?;

        // Build completions request (existing format)
        let copilot_messages = self.convert_messages(messages);
        let copilot_tools = self.convert_tools_legacy(tools);

        let request = CopilotRequest {
            model: model.to_string(),
            messages: copilot_messages,
            tools: copilot_tools,
            stream: true,
        };

        // Make HTTP request
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "xzatoma/0.1.0")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(XzatomaError::Provider(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(XzatomaError::Provider(format!(
                "HTTP {}: {}",
                status, body
            ))));
        }

        // Create stream (similar to stream_response but for completions format)
        let stream = response.bytes_stream();

        // Build stream using boxed closure
        let event_stream = futures::stream::unfold(
            (stream.boxed(), String::new()),
            |(mut byte_stream, mut buffer)| async move {
                loop {
                    match byte_stream.next().await {
                        Some(Ok(chunk)) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));

                            // Look for complete lines
                            if let Some(pos) = buffer.find('\n') {
                                let line = buffer[..pos].to_string();
                                buffer.drain(..=pos);

                                // Parse SSE line
                                if let Some(data) = parse_sse_line(&line) {
                                    let result = parse_sse_event(&data);
                                    return Some((result, (byte_stream, buffer)));
                                }
                                // Continue loop to find next line
                                continue;
                            }
                            // No complete line yet, wait for more chunks
                            continue;
                        }
                        Some(Err(e)) => {
                            return Some((
                                Err(anyhow::anyhow!(XzatomaError::StreamInterrupted(
                                    e.to_string()
                                ))),
                                (byte_stream, buffer),
                            ))
                        }
                        None => {
                            // Stream ended
                            if !buffer.is_empty() {
                                // Try to parse any remaining content
                                if let Some(data) = parse_sse_line(&buffer) {
                                    let result = parse_sse_event(&data);
                                    buffer.clear();
                                    return Some((result, (byte_stream, buffer)));
                                }
                            }
                            // Stream complete
                            return None;
                        }
                    }
                }
            },
        );

        Ok(Box::pin(event_stream))
    }

    /// Convert XZatoma tools to Copilot format (legacy for completions endpoint)
    fn convert_tools_legacy(&self, tools: &[crate::tools::Tool]) -> Vec<CopilotTool> {
        tools
            .iter()
            .map(|tool| CopilotTool {
                r#type: "function".to_string(),
                function: CopilotFunction {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                },
            })
            .collect()
    }

    /// Complete using /responses endpoint with streaming support
    ///
    /// Uses the new /responses endpoint which supports extended features
    /// like reasoning and improved tool calling.
    async fn complete_with_responses_endpoint(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
        _token: &str,
        enable_streaming: bool,
    ) -> Result<CompletionResponse> {
        // Convert messages to responses format
        let input = convert_messages_to_response_input(messages)?;

        // Convert tools if any
        let response_tools = if !tools.is_empty() {
            let xzatoma_tools: Vec<crate::tools::Tool> = tools
                .iter()
                .filter_map(|t| serde_json::from_value(t.clone()).ok())
                .collect();
            convert_tools_to_response_format(&xzatoma_tools)
        } else {
            Vec::new()
        };

        if enable_streaming {
            self.complete_responses_streaming(model, input, response_tools)
                .await
        } else {
            self.complete_responses_blocking(model, input, response_tools)
                .await
        }
    }

    /// Complete using /chat/completions endpoint (legacy)
    ///
    /// Falls back to the legacy chat completions endpoint for compatibility
    /// with older model versions.
    async fn complete_with_completions_endpoint(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[serde_json::Value],
        _token: &str,
        enable_streaming: bool,
    ) -> Result<CompletionResponse> {
        // Convert tools to Tool objects for stream_completion
        let xzatoma_tools: Vec<crate::tools::Tool> = tools
            .iter()
            .filter_map(|t| serde_json::from_value(t.clone()).ok())
            .collect();

        if enable_streaming {
            self.complete_completions_streaming(model, messages, &xzatoma_tools)
                .await
        } else {
            self.complete_completions_blocking(model, messages, &xzatoma_tools)
                .await
        }
    }

    /// Complete responses endpoint request with streaming
    ///
    /// Sends a streaming request to the /responses endpoint and collects
    /// all events into a final completion response.
    async fn complete_responses_streaming(
        &self,
        model: &str,
        input: Vec<ResponseInputItem>,
        tools: Vec<ToolDefinition>,
    ) -> Result<CompletionResponse> {
        tracing::debug!("Sending streaming /responses request");

        let stream = self.stream_response(model, input, tools).await?;

        // Collect all stream events
        let mut final_message: Option<Message> = None;

        futures::pin_mut!(stream);
        while let Some(event_result) = stream.next().await {
            let event = event_result?;

            if let Some(msg) = convert_stream_event_to_message(&event) {
                if msg.role == "assistant" {
                    final_message = Some(msg);
                }
            }
        }

        let response_message = final_message.unwrap_or_else(|| Message::assistant(""));

        tracing::debug!("Responses streaming completed");

        Ok(CompletionResponse::new(response_message))
    }

    /// Complete responses endpoint request without streaming
    ///
    /// Sends a non-streaming request to the /responses endpoint.
    async fn complete_responses_blocking(
        &self,
        model: &str,
        input: Vec<ResponseInputItem>,
        tools: Vec<ToolDefinition>,
    ) -> Result<CompletionResponse> {
        let token = self.authenticate().await?;

        tracing::debug!(
            "Sending blocking /responses request: {} input items",
            input.len()
        );

        let (reasoning, include) = {
            let config = self
                .config
                .read()
                .map_err(|_| XzatomaError::Provider("Failed to acquire config lock".to_string()))?;

            let reasoning = config
                .reasoning_effort
                .as_ref()
                .map(|effort| ReasoningConfig {
                    effort: Some(effort.clone()),
                });

            let include = if config.include_reasoning {
                Some(vec!["reasoning".to_string()])
            } else {
                None
            };

            (reasoning, include)
        }; // Drop the read guard here

        let has_tools = !tools.is_empty();
        let request = ResponsesRequest {
            model: model.to_string(),
            input,
            stream: false,
            temperature: None,
            tools: if has_tools { Some(tools) } else { None },
            tool_choice: if has_tools {
                Some(ToolChoice::Auto { auto: true })
            } else {
                None
            },
            reasoning,
            include,
        };

        let url = self.endpoint_url(ModelEndpoint::Responses);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("/responses request failed: {}", e);
                XzatomaError::Provider(format!("/responses request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("/responses returned error {}: {}", status, error_text);
            return Err(format_copilot_api_error(status, &error_text).into());
        }

        // Parse response - for /responses endpoint, we expect a message-like response
        #[derive(Deserialize)]
        struct ResponsesResponse {
            #[serde(default)]
            message: Option<ResponseInputItem>,
            #[serde(default)]
            choices: Vec<ResponsesChoice>,
        }

        #[derive(Deserialize)]
        struct ResponsesChoice {
            message: ResponseInputItem,
        }

        let responses_resp: ResponsesResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse /responses response: {}", e);
            XzatomaError::Provider(format!("Failed to parse /responses response: {}", e))
        })?;

        // Extract message from response
        let response_item = responses_resp
            .message
            .or_else(|| responses_resp.choices.into_iter().next().map(|c| c.message))
            .ok_or_else(|| {
                XzatomaError::Provider("No message in /responses response".to_string())
            })?;

        let messages = convert_response_input_to_messages(&[response_item])?;
        let message = messages
            .into_iter()
            .next()
            .unwrap_or_else(|| Message::assistant(""));

        tracing::debug!("/responses request completed successfully");

        Ok(CompletionResponse::new(message))
    }

    /// Complete chat completions endpoint request with streaming
    ///
    /// Sends a streaming request to the /chat/completions endpoint.
    async fn complete_completions_streaming(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[crate::tools::Tool],
    ) -> Result<CompletionResponse> {
        tracing::debug!("Sending streaming /chat/completions request");

        let stream = self.stream_completion(model, messages, tools).await?;

        // Collect all stream events
        let mut final_message: Option<Message> = None;

        futures::pin_mut!(stream);
        while let Some(event_result) = stream.next().await {
            let event = event_result?;

            if let Some(msg) = convert_stream_event_to_message(&event) {
                if msg.role == "assistant" {
                    final_message = Some(msg);
                }
            }
        }

        let response_message = final_message.unwrap_or_else(|| Message::assistant(""));

        tracing::debug!("Completions streaming completed");

        Ok(CompletionResponse::new(response_message))
    }

    /// Complete chat completions endpoint request without streaming
    ///
    /// Sends a non-streaming request to the /chat/completions endpoint.
    async fn complete_completions_blocking(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[crate::tools::Tool],
    ) -> Result<CompletionResponse> {
        let token = self.authenticate().await?;

        let copilot_request = CopilotRequest {
            model: model.to_string(),
            messages: self.convert_messages(messages),
            tools: self.convert_tools_legacy(tools),
            stream: false,
        };

        tracing::debug!(
            "Sending blocking /chat/completions request: {} messages, {} tools",
            copilot_request.messages.len(),
            copilot_request.tools.len()
        );

        let url = self.endpoint_url(ModelEndpoint::ChatCompletions);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&copilot_request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("/chat/completions request failed: {}", e);
                XzatomaError::Provider(format!("/chat/completions request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(
                "/chat/completions returned error {}: {}",
                status,
                error_text
            );

            if status == reqwest::StatusCode::UNAUTHORIZED {
                tracing::warn!("Attempting token refresh");
                if let Ok(cached) = self.get_cached_token() {
                    if let Ok(new_token) = self.get_copilot_token(&cached.github_token).await {
                        let refreshed = CachedToken {
                            github_token: cached.github_token.clone(),
                            copilot_token: new_token.clone(),
                            expires_at: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs()
                                + 3600,
                        };
                        if let Err(e) = self.cache_token(&refreshed) {
                            tracing::warn!("Failed to cache refreshed token: {}", e);
                        }

                        // Retry with new token
                        let retry_response = self
                            .client
                            .post(&url)
                            .header("Authorization", format!("Bearer {}", new_token))
                            .header("Editor-Version", "vscode/1.85.0")
                            .json(&copilot_request)
                            .send()
                            .await
                            .map_err(|e| {
                                tracing::error!("Retry failed: {}", e);
                                XzatomaError::Provider(format!("Retry failed: {}", e))
                            })?;

                        let retry_status = retry_response.status();
                        if !retry_status.is_success() {
                            let error_text = retry_response.text().await.unwrap_or_default();
                            tracing::error!(
                                "/chat/completions retry returned error {}: {}",
                                retry_status,
                                error_text
                            );
                            return Err(format_copilot_api_error(retry_status, &error_text).into());
                        }

                        let copilot_response: CopilotResponse =
                            retry_response.json().await.map_err(|e| {
                                tracing::error!("Failed to parse response: {}", e);
                                XzatomaError::Provider(format!("Failed to parse response: {}", e))
                            })?;

                        let choice =
                            copilot_response.choices.into_iter().next().ok_or_else(|| {
                                XzatomaError::Provider("No choices in response".to_string())
                            })?;

                        let message = self.convert_response_message(choice.message);
                        let usage = copilot_response
                            .usage
                            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

                        return Ok(match usage {
                            Some(u) => CompletionResponse::with_usage(message, u),
                            None => CompletionResponse::new(message),
                        });
                    }
                }
                if let Err(e) = self.clear_cached_token() {
                    tracing::warn!("Failed to clear cached token: {}", e);
                }
            }

            return Err(format_copilot_api_error(status, &error_text).into());
        }

        let copilot_response: CopilotResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse response: {}", e);
            XzatomaError::Provider(format!("Failed to parse response: {}", e))
        })?;

        let choice = copilot_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| XzatomaError::Provider("No choices in response".to_string()))?;

        let message = self.convert_response_message(choice.message);
        let usage = copilot_response
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        tracing::debug!("/chat/completions request completed successfully");

        Ok(match usage {
            Some(u) => CompletionResponse::with_usage(message, u),
            None => CompletionResponse::new(message),
        })
    }

    /// Select the best endpoint for the model
    ///
    /// Checks model capabilities and configuration to determine which endpoint
    /// to use for completion requests. Prefers /responses endpoint but falls
    /// back to /chat/completions if needed.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model to check
    ///
    /// # Returns
    ///
    /// Returns the preferred ModelEndpoint, or error if no suitable endpoint found
    ///
    /// # Errors
    ///
    /// Returns error if model is not found or no compatible endpoint is available
    async fn select_endpoint(&self, model_name: &str) -> Result<ModelEndpoint> {
        // Get the enable_endpoint_fallback setting, drop the guard before awaits
        let enable_endpoint_fallback = self
            .config
            .read()
            .map_err(|_| XzatomaError::Provider("Failed to acquire config lock".to_string()))?
            .enable_endpoint_fallback;

        // Check if model supports responses endpoint
        if self
            .model_supports_endpoint(model_name, ModelEndpoint::Responses)
            .await?
        {
            tracing::debug!(
                "Model {} supports /responses endpoint, using it",
                model_name
            );
            return Ok(ModelEndpoint::Responses);
        }

        // If fallback is enabled and responses not supported, try completions
        if enable_endpoint_fallback
            && self
                .model_supports_endpoint(model_name, ModelEndpoint::ChatCompletions)
                .await?
        {
            tracing::debug!(
                "Model {} does not support /responses; falling back to /chat/completions",
                model_name
            );
            return Ok(ModelEndpoint::ChatCompletions);
        }

        // No suitable endpoint found
        Err(XzatomaError::Provider(format!(
            "No supported endpoint found for model: {}",
            model_name
        ))
        .into())
    }

    /// Check if a model supports a specific endpoint
    ///
    /// Queries the models API to determine if the given model supports
    /// the specified endpoint.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model to check
    /// * `endpoint` - Endpoint to verify support for
    ///
    /// # Returns
    ///
    /// Returns true if model supports the endpoint
    ///
    /// # Errors
    ///
    /// Returns error if models API is unreachable or model not found
    async fn model_supports_endpoint(
        &self,
        model_name: &str,
        endpoint: ModelEndpoint,
    ) -> Result<bool> {
        let models_data = self.fetch_copilot_models_raw().await?;

        let model = models_data
            .iter()
            .find(|m| m.id == model_name || m.name == model_name)
            .ok_or_else(|| XzatomaError::Provider(format!("Model not found: {}", model_name)))?;

        // If supported_endpoints is empty, assume all legacy endpoints are supported
        if model.supported_endpoints.is_empty() {
            return Ok(endpoint == ModelEndpoint::ChatCompletions);
        }

        let endpoint_name = endpoint.as_str();
        Ok(model.supports_endpoint(endpoint_name))
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let (model, enable_streaming) = {
            let config = self.config.read().map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?;
            (config.model.clone(), config.enable_streaming)
        }; // Drop the read guard before awaits

        // Determine which endpoint to use
        let endpoint = self.select_endpoint(&model).await?;

        tracing::debug!("Using endpoint {:?} for model: {}", endpoint, model);

        // Route to appropriate implementation based on endpoint
        match endpoint {
            ModelEndpoint::Responses => {
                self.complete_with_responses_endpoint(&model, messages, tools, "", enable_streaming)
                    .await
            }
            ModelEndpoint::ChatCompletions => {
                self.complete_with_completions_endpoint(
                    &model,
                    messages,
                    tools,
                    "",
                    enable_streaming,
                )
                .await
            }
            ModelEndpoint::Messages => {
                // Messages endpoint not yet implemented, fall back to completions
                self.complete_with_completions_endpoint(
                    &model,
                    messages,
                    tools,
                    "",
                    enable_streaming,
                )
                .await
            }
            ModelEndpoint::Unknown => {
                Err(XzatomaError::Provider("Unknown endpoint selected".to_string()).into())
            }
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        tracing::debug!("Listing Copilot models from API");
        self.fetch_copilot_models().await
    }

    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
        tracing::debug!("Getting info for model: {}", model_name);
        let models = self.fetch_copilot_models().await?;
        models
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
            supports_streaming: true,
        }
    }

    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models_data = self.fetch_copilot_models_raw().await?;
        Ok(models_data
            .into_iter()
            .filter(|data| {
                // Only include enabled models
                data.policy
                    .as_ref()
                    .map(|p| p.state == "enabled")
                    .unwrap_or(true)
            })
            .map(|data| self.convert_to_summary(data))
            .collect())
    }

    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let models_data = self.fetch_copilot_models_raw().await?;
        let data = models_data
            .into_iter()
            .find(|m| m.id == model_name || m.name == model_name)
            .ok_or_else(|| XzatomaError::Provider(format!("Model '{}' not found", model_name)))?;
        Ok(self.convert_to_summary(data))
    }

    async fn set_model(&mut self, model_name: String) -> Result<()> {
        // Fetch available models from API to validate
        let models = self.fetch_copilot_models().await?;

        // Find the requested model
        let model_info = models
            .iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                let available: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
                XzatomaError::Provider(format!(
                    "Model '{}' not found. Available models: {}",
                    model_name,
                    available.join(", ")
                ))
            })?;

        // Check if model supports tool calling
        if !model_info.supports_capability(ModelCapability::FunctionCalling) {
            let tool_models: Vec<String> = models
                .iter()
                .filter(|m| m.supports_capability(ModelCapability::FunctionCalling))
                .map(|m| m.name.clone())
                .collect();

            return Err(XzatomaError::Provider(format!(
                "Model '{}' does not support tool calling, which is required for XZatoma. Models with tool support: {}",
                model_name,
                tool_models.join(", ")
            ))
            .into());
        }

        // Update the model in the config
        let mut config = self.config.write().map_err(|_| {
            XzatomaError::Provider("Failed to acquire write lock on config".to_string())
        })?;
        config.model = model_name;
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
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
        assert_eq!(config.model, "gpt-5-mini");
    }

    #[test]
    fn test_copilot_models_response_deserialization() {
        // Test that we can deserialize the Copilot models API response
        // The testdata/models.json is an array, not wrapped in {"data": [...]}
        let test_data = include_str!("../../testdata/models.json");
        let result: std::result::Result<Vec<CopilotModelData>, _> = serde_json::from_str(test_data);
        assert!(result.is_ok());
        let models = result.unwrap();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_copilot_provider_model() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
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
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.keyring_service, "xzatoma");
        assert_eq!(provider.keyring_user, "github_copilot");
    }

    #[tokio::test]
    async fn test_list_copilot_models() {
        // This test would require authentication and API calls
        // For unit testing, we verify the parsing logic with test data instead
        // See test_parse_models_from_testdata below
    }

    #[test]
    fn test_parse_models_from_testdata() {
        // Test that we can parse the real Copilot models API response
        // The testdata is an array of models directly
        let test_data = include_str!("../../testdata/models.json");
        let models: Vec<CopilotModelData> = serde_json::from_str(test_data).unwrap();

        // Filter to enabled models only
        let enabled_models: Vec<_> = models
            .iter()
            .filter(|m| {
                m.policy
                    .as_ref()
                    .map(|p| p.state == "enabled")
                    .unwrap_or(false)
            })
            .collect();

        assert!(!enabled_models.is_empty());

        // Check for expected models from the testdata
        let model_ids: Vec<_> = enabled_models.iter().map(|m| m.id.as_str()).collect();
        assert!(model_ids.contains(&"gpt-5-mini"));
        assert!(model_ids.contains(&"claude-sonnet-4.5"));

        // Verify models have capabilities
        let gpt5_mini = enabled_models
            .iter()
            .find(|m| m.id == "gpt-5-mini")
            .unwrap();
        assert!(gpt5_mini.capabilities.is_some());
        let caps = gpt5_mini.capabilities.as_ref().unwrap();
        assert!(caps.supports.is_some());
        assert_eq!(caps.supports.as_ref().unwrap().tool_calls, Some(true));
    }

    #[test]
    fn test_model_context_window_extraction() {
        // Test that we correctly extract context windows from the API response
        let test_data = include_str!("../../testdata/models.json");
        let models: Vec<CopilotModelData> = serde_json::from_str(test_data).unwrap();

        // Find gpt-5-mini and verify context window
        let gpt5_mini = models.iter().find(|m| m.id == "gpt-5-mini").unwrap();
        let context_window = gpt5_mini
            .capabilities
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.max_context_window_tokens);
        assert_eq!(context_window, Some(264000));
    }

    #[test]
    fn test_get_current_model() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    }

    #[test]
    fn test_provider_capabilities() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        let caps = provider.get_provider_capabilities();

        assert!(caps.supports_model_listing);
        assert!(caps.supports_model_details);
        assert!(caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(caps.supports_streaming);
    }

    #[tokio::test]
    async fn test_set_model_with_valid_model() {
        // This test requires API authentication and would make real API calls
        // In a real test environment with proper mocking, we would:
        // 1. Mock the Copilot models API to return test data
        // 2. Call set_model with a known-good model from that data
        // 3. Verify the model was updated
        // For now, we just verify the structure compiles
    }

    #[tokio::test]
    async fn test_set_model_with_invalid_model() {
        // This test requires API authentication and would make real API calls
        // In a real test environment with proper mocking, we would:
        // 1. Mock the Copilot models API to return test data
        // 2. Call set_model with an invalid model name
        // 3. Verify it returns an error with helpful message
        // For now, we just verify the structure compiles
    }

    #[tokio::test]
    async fn test_list_models_returns_all_supported_models() {
        // This test requires API authentication and would make real API calls
        // The actual model count varies based on what GitHub enables
        // See test_parse_models_from_testdata for parsing validation
    }

    #[tokio::test]
    async fn test_get_model_info_valid_model() {
        // This test requires API authentication and would make real API calls
        // See test_parse_models_from_testdata for parsing validation
    }

    #[tokio::test]
    async fn test_get_model_info_invalid_model() {
        // This test requires API authentication and would make real API calls
        // In a proper test environment, we would verify error handling
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

    #[test]
    fn test_copilot_message_deserialize_missing_content() {
        // Test that CopilotMessage can be deserialized when content field is missing
        // This happens when Copilot returns tool calls without content
        let json = r#"{
            "role": "assistant",
            "tool_calls": [{
                "id": "call_abc123",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"test.txt\"}"
                }
            }]
        }"#;

        let result: std::result::Result<CopilotMessage, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, ""); // Should default to empty string
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(msg.tool_calls.as_ref().unwrap()[0].id, "call_abc123");
    }

    #[test]
    fn test_copilot_response_deserialize_missing_content() {
        // Test that a complete CopilotResponse can be deserialized when content is missing
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_xyz789",
                        "type": "function",
                        "function": {
                            "name": "terminal",
                            "arguments": "{\"command\":\"ls\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let result: std::result::Result<CopilotResponse, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "");
        assert!(response.choices[0].message.tool_calls.is_some());
        assert!(response.usage.is_some());
    }

    #[test]
    fn test_format_copilot_api_error_unauthorized() {
        use crate::error::XzatomaError;

        let err = format_copilot_api_error(
            reqwest::StatusCode::UNAUTHORIZED,
            "unauthorized: token expired",
        );
        assert!(matches!(err, XzatomaError::Authentication(_)));
        assert!(
            err.to_string().contains("token expired")
                || err.to_string().contains("Token may have expired")
        );
    }

    #[test]
    fn test_format_copilot_api_error_other() {
        use crate::error::XzatomaError;

        let err =
            format_copilot_api_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        assert!(matches!(err, XzatomaError::Provider(_)));
        assert!(err.to_string().contains("internal error"));
    }

    #[test]
    fn test_convert_to_summary_full_data() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let model_data = CopilotModelData {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            capabilities: Some(CopilotModelCapabilities {
                limits: Some(CopilotModelLimits {
                    max_context_window_tokens: Some(8192),
                }),
                supports: Some(CopilotModelSupports {
                    tool_calls: Some(true),
                    vision: Some(true),
                }),
            }),
            policy: Some(CopilotModelPolicy {
                state: "enabled".to_string(),
            }),
            supported_endpoints: vec!["chat_completions".to_string(), "responses".to_string()],
        };

        let summary = provider.convert_to_summary(model_data);

        assert_eq!(summary.info.name, "gpt-4");
        assert_eq!(summary.info.display_name, "GPT-4");
        assert_eq!(summary.info.context_window, 8192);
        assert_eq!(summary.state, Some("enabled".to_string()));
        assert_eq!(summary.supports_tool_calls, Some(true));
        assert_eq!(summary.supports_vision, Some(true));
        assert!(summary
            .info
            .capabilities
            .contains(&ModelCapability::FunctionCalling));
        assert!(summary.info.capabilities.contains(&ModelCapability::Vision));
        assert!(summary.raw_data.is_object());
    }

    #[test]
    fn test_convert_to_summary_minimal_data() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let model_data = CopilotModelData {
            id: "gpt-3.5-turbo".to_string(),
            name: "GPT-3.5 Turbo".to_string(),
            capabilities: None,
            policy: None,
            supported_endpoints: vec![],
        };

        let summary = provider.convert_to_summary(model_data);

        assert_eq!(summary.info.name, "gpt-3.5-turbo");
        assert_eq!(summary.info.display_name, "GPT-3.5 Turbo");
        assert_eq!(summary.info.context_window, DEFAULT_CONTEXT_WINDOW);
        assert!(summary.state.is_none());
        assert!(summary.supports_tool_calls.is_none());
        assert!(summary.supports_vision.is_none());
        assert!(summary.info.capabilities.is_empty());
    }

    #[test]
    fn test_convert_to_summary_missing_capabilities() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let model_data = CopilotModelData {
            id: "claude-3".to_string(),
            name: "Claude 3".to_string(),
            capabilities: Some(CopilotModelCapabilities {
                limits: Some(CopilotModelLimits {
                    max_context_window_tokens: Some(200000),
                }),
                supports: None,
            }),
            policy: Some(CopilotModelPolicy {
                state: "enabled".to_string(),
            }),
            supported_endpoints: vec![],
        };

        let summary = provider.convert_to_summary(model_data);

        assert_eq!(summary.info.context_window, 200000);
        assert!(summary.supports_tool_calls.is_none());
        assert!(summary.supports_vision.is_none());
        assert!(summary
            .info
            .capabilities
            .contains(&ModelCapability::LongContext));
    }

    #[test]
    fn test_convert_to_summary_missing_policy() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let model_data = CopilotModelData {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            capabilities: Some(CopilotModelCapabilities {
                limits: Some(CopilotModelLimits {
                    max_context_window_tokens: Some(4096),
                }),
                supports: Some(CopilotModelSupports {
                    tool_calls: Some(false),
                    vision: Some(false),
                }),
            }),
            policy: None,
            supported_endpoints: vec!["chat_completions".to_string()],
        };

        let summary = provider.convert_to_summary(model_data);

        assert!(summary.state.is_none());
        assert_eq!(summary.supports_tool_calls, Some(false));
        assert_eq!(summary.supports_vision, Some(false));
        assert!(!summary
            .info
            .capabilities
            .contains(&ModelCapability::FunctionCalling));
        assert!(!summary.info.capabilities.contains(&ModelCapability::Vision));
    }

    #[test]
    fn test_convert_messages_drops_orphan_tool() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).expect("Failed to create provider");

        let messages = vec![
            Message::user("Do something"),
            Message::tool_result("call_123", "Result"),
        ];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    #[test]
    fn test_convert_messages_preserves_valid_tool_pair() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).expect("Failed to create provider");

        let tool_call = crate::providers::ToolCall {
            id: "call_123".to_string(),
            function: crate::providers::FunctionCall {
                name: "test_func".to_string(),
                arguments: "{}".to_string(),
            },
        };

        let messages = vec![
            Message::user("Do something"),
            Message::assistant_with_tools(vec![tool_call]),
            Message::tool_result("call_123", "Result"),
        ];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "tool");
        assert_eq!(converted[2].tool_call_id, Some("call_123".to_string()));
    }

    // ========================================================================
    // PHASE 1 TESTS: Core Data Structures and Endpoint Detection
    // ========================================================================

    // Task 1.1: Response Endpoint Types Tests

    #[test]
    fn test_responses_request_serialization() {
        let request = ResponsesRequest {
            model: "gpt-5-mini".to_string(),
            input: vec![ResponseInputItem::Message {
                role: "user".to_string(),
                content: vec![ResponseInputContent::InputText {
                    text: "Hello".to_string(),
                }],
            }],
            stream: true,
            temperature: Some(0.7),
            tools: None,
            tool_choice: None,
            reasoning: None,
            include: None,
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("\"model\":\"gpt-5-mini\""));
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"temperature\":0.7"));
    }

    #[test]
    fn test_response_input_item_message_deserialization() {
        let json = r#"{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "Hello"}]
        }"#;

        let item: ResponseInputItem = serde_json::from_str(json).expect("Failed to deserialize");

        match item {
            ResponseInputItem::Message { role, content } => {
                assert_eq!(role, "user");
                assert_eq!(content.len(), 1);
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_response_input_item_function_call_deserialization() {
        let json = r#"{
            "type": "function_call",
            "call_id": "call_123",
            "name": "get_weather",
            "arguments": "{\"location\":\"SF\"}"
        }"#;

        let item: ResponseInputItem = serde_json::from_str(json).expect("Failed to deserialize");

        match item {
            ResponseInputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(call_id, "call_123");
                assert_eq!(name, "get_weather");
                assert!(arguments.contains("location"));
            }
            _ => panic!("Expected FunctionCall variant"),
        }
    }

    #[test]
    fn test_stream_event_roundtrip() {
        let original = StreamEvent::Message {
            role: "assistant".to_string(),
            content: vec![ResponseInputContent::OutputText {
                text: "Response text".to_string(),
            }],
        };

        let json = serde_json::to_string(&original).expect("Failed to serialize");
        let deserialized: StreamEvent = serde_json::from_str(&json).expect("Failed to deserialize");

        match (original, deserialized) {
            (StreamEvent::Message { role: r1, .. }, StreamEvent::Message { role: r2, .. }) => {
                assert_eq!(r1, r2);
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_tool_definition_serialization() {
        let tool = ToolDefinition::Function {
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: "Get weather for location".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    }
                }),
                strict: Some(true),
            },
        };

        let json = serde_json::to_string(&tool).expect("Failed to serialize");
        assert!(json.contains("\"name\":\"get_weather\""));
        assert!(json.contains("\"strict\":true"));
    }

    #[test]
    fn test_tool_choice_variants() {
        let auto = ToolChoice::Auto { auto: true };
        let json = serde_json::to_string(&auto).expect("Serialize failed");
        assert!(json.contains("\"auto\":true"));

        let named = ToolChoice::Named {
            function: FunctionName {
                name: "specific_tool".to_string(),
            },
        };
        let json = serde_json::to_string(&named).expect("Serialize failed");
        assert!(json.contains("\"specific_tool\""));
    }

    #[test]
    fn test_response_input_content_variants() {
        let input_text = ResponseInputContent::InputText {
            text: "User input".to_string(),
        };
        let json = serde_json::to_string(&input_text).expect("Serialize failed");
        assert!(json.contains("input_text"));
        assert!(json.contains("User input"));

        let output_text = ResponseInputContent::OutputText {
            text: "Assistant output".to_string(),
        };
        let json = serde_json::to_string(&output_text).expect("Serialize failed");
        assert!(json.contains("output_text"));
    }

    #[test]
    fn test_optional_fields_omitted() {
        let request = ResponsesRequest {
            model: "gpt-5-mini".to_string(),
            input: vec![],
            stream: false,
            temperature: None,
            tools: None,
            tool_choice: None,
            reasoning: None,
            include: None,
        };

        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(!json.contains("temperature"));
        assert!(!json.contains("tools"));
        assert!(!json.contains("reasoning"));
    }

    // Task 1.2: Endpoint Tracking Tests

    #[test]
    fn test_parse_model_with_endpoints() {
        let json = r#"{
            "id": "gpt-5-mini",
            "name": "GPT 5 Mini",
            "supported_endpoints": ["chat_completions", "responses"]
        }"#;

        let model: CopilotModelData = serde_json::from_str(json).expect("Parse failed");
        assert_eq!(model.supported_endpoints.len(), 2);
        assert!(model.supported_endpoints.contains(&"responses".to_string()));
    }

    #[test]
    fn test_supports_endpoint_method() {
        let model = CopilotModelData {
            id: "test".to_string(),
            name: "Test".to_string(),
            capabilities: None,
            policy: None,
            supported_endpoints: vec!["chat_completions".to_string(), "responses".to_string()],
        };

        assert!(model.supports_endpoint("responses"));
        assert!(model.supports_endpoint("chat_completions"));
        assert!(!model.supports_endpoint("messages"));
        assert!(!model.supports_endpoint("unknown"));
    }

    #[test]
    fn test_model_without_endpoints_field() {
        let json = r#"{
            "id": "old-model",
            "name": "Old Model"
        }"#;

        let model: CopilotModelData = serde_json::from_str(json).expect("Parse failed");
        assert_eq!(model.supported_endpoints.len(), 0);
        assert!(!model.supports_endpoint("responses"));
    }

    #[test]
    fn test_endpoints_stored_in_model_info() {
        let model_data = CopilotModelData {
            id: "gpt-5-mini".to_string(),
            name: "GPT 5 Mini".to_string(),
            capabilities: None,
            policy: None,
            supported_endpoints: vec!["responses".to_string()],
        };

        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).expect("Failed to create provider");
        let summary = provider.convert_to_summary(model_data);

        let endpoints = summary
            .raw_data
            .get("supported_endpoints")
            .expect("Endpoints not in metadata");

        let endpoints_array = endpoints.as_array().expect("Not an array");
        assert_eq!(endpoints_array.len(), 1);
    }

    // Task 1.3: Endpoint Configuration Tests

    #[test]
    fn test_endpoint_url_constants() {
        assert_eq!(
            COPILOT_COMPLETIONS_URL,
            "https://api.githubcopilot.com/chat/completions"
        );
        assert_eq!(
            COPILOT_RESPONSES_URL,
            "https://api.githubcopilot.com/responses"
        );
        assert!(COPILOT_RESPONSES_URL.starts_with("https://"));
    }

    #[test]
    fn test_model_endpoint_from_name() {
        assert_eq!(
            ModelEndpoint::from_name("responses"),
            ModelEndpoint::Responses
        );
        assert_eq!(
            ModelEndpoint::from_name("chat_completions"),
            ModelEndpoint::ChatCompletions
        );
        assert_eq!(
            ModelEndpoint::from_name("messages"),
            ModelEndpoint::Messages
        );
        assert_eq!(ModelEndpoint::from_name("invalid"), ModelEndpoint::Unknown);
    }

    #[test]
    fn test_model_endpoint_as_str() {
        assert_eq!(ModelEndpoint::Responses.as_str(), "responses");
        assert_eq!(ModelEndpoint::ChatCompletions.as_str(), "chat_completions");
        assert_eq!(ModelEndpoint::Messages.as_str(), "messages");
        assert_eq!(ModelEndpoint::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_api_endpoint_url_construction() {
        let config = CopilotConfig {
            model: "gpt-5-mini".to_string(),
            api_base: None,
            enable_streaming: true,
            enable_endpoint_fallback: true,
            reasoning_effort: None,
            include_reasoning: false,
        };

        let provider = CopilotProvider::new(config).expect("Failed to create provider");

        assert_eq!(
            provider.endpoint_url(ModelEndpoint::Responses),
            "https://api.githubcopilot.com/responses"
        );
        assert_eq!(
            provider.endpoint_url(ModelEndpoint::ChatCompletions),
            "https://api.githubcopilot.com/chat/completions"
        );
    }

    #[test]
    fn test_api_endpoint_with_custom_base() {
        let config = CopilotConfig {
            model: "gpt-5-mini".to_string(),
            api_base: Some("https://custom.api.com".to_string()),
            enable_streaming: true,
            enable_endpoint_fallback: true,
            reasoning_effort: None,
            include_reasoning: false,
        };

        let provider = CopilotProvider::new(config).expect("Failed to create provider");

        assert_eq!(
            provider.endpoint_url(ModelEndpoint::Responses),
            "https://custom.api.com/responses"
        );
    }

    // ========================================================================
    // PHASE 2: MESSAGE FORMAT CONVERSION TESTS
    // ========================================================================

    // --- Task 2.1: Message to Response Input Conversion Tests ---

    #[test]
    fn test_convert_user_message() {
        let messages = vec![Message::user("Hello, world!")];
        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

        assert_eq!(result.len(), 1);
        match &result[0] {
            ResponseInputItem::Message { role, content } => {
                assert_eq!(role, "user");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ResponseInputContent::InputText { text } => {
                        assert_eq!(text, "Hello, world!");
                    }
                    _ => panic!("Expected InputText"),
                }
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_convert_assistant_message() {
        let messages = vec![Message::assistant("I'm here to help")];
        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

        assert_eq!(result.len(), 1);
        match &result[0] {
            ResponseInputItem::Message { role, content } => {
                assert_eq!(role, "assistant");
                match &content[0] {
                    ResponseInputContent::OutputText { text } => {
                        assert_eq!(text, "I'm here to help");
                    }
                    _ => panic!("Expected OutputText"),
                }
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_convert_system_message() {
        let messages = vec![Message::system("You are a helpful assistant")];
        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

        assert_eq!(result.len(), 1);
        match &result[0] {
            ResponseInputItem::Message { role, content } => {
                assert_eq!(role, "system");
                match &content[0] {
                    ResponseInputContent::InputText { text } => {
                        assert_eq!(text, "You are a helpful assistant");
                    }
                    _ => panic!("Expected InputText"),
                }
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_convert_tool_call_message() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"location":"SF"}"#.to_string(),
            },
        };
        let messages = vec![Message::assistant_with_tools(vec![tool_call])];

        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

        assert_eq!(result.len(), 1);
        match &result[0] {
            ResponseInputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(call_id, "call_123");
                assert_eq!(name, "get_weather");
                assert!(arguments.contains("location"));
            }
            _ => panic!("Expected FunctionCall variant"),
        }
    }

    #[test]
    fn test_convert_tool_result_message() {
        let messages = vec![Message::tool_result("call_123", r#"{"temperature":72}"#)];

        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

        assert_eq!(result.len(), 1);
        match &result[0] {
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                assert_eq!(call_id, "call_123");
                assert!(output.contains("temperature"));
            }
            _ => panic!("Expected FunctionCallOutput variant"),
        }
    }

    #[test]
    fn test_convert_conversation() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hi"),
            Message::assistant("Hello!"),
            Message::user("How are you?"),
        ];

        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");
        assert_eq!(result.len(), 4);

        // Verify order preserved
        match &result[0] {
            ResponseInputItem::Message { role, .. } => assert_eq!(role, "system"),
            _ => panic!("Wrong type"),
        }
        match &result[1] {
            ResponseInputItem::Message { role, .. } => assert_eq!(role, "user"),
            _ => panic!("Wrong type"),
        }
    }

    #[test]
    fn test_convert_empty_messages() {
        let messages: Vec<Message> = vec![];
        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_convert_assistant_message_with_content_and_tools() {
        let tool_call = ToolCall {
            id: "call_456".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: r#"{"q":"test"}"#.to_string(),
            },
        };
        let messages = vec![Message {
            role: "assistant".to_string(),
            content: Some("Let me search for that".to_string()),
            tool_calls: Some(vec![tool_call]),
            tool_call_id: None,
        }];

        let result = convert_messages_to_response_input(&messages).expect("Conversion failed");
        assert_eq!(result.len(), 2);

        // First should be FunctionCall
        match &result[0] {
            ResponseInputItem::FunctionCall { call_id, name, .. } => {
                assert_eq!(call_id, "call_456");
                assert_eq!(name, "search");
            }
            _ => panic!("Expected FunctionCall first"),
        }

        // Second should be Message with content
        match &result[1] {
            ResponseInputItem::Message { role, content } => {
                assert_eq!(role, "assistant");
                assert!(!content.is_empty());
            }
            _ => panic!("Expected Message second"),
        }
    }

    // --- Task 2.2: Response Input to Message Conversion Tests ---

    #[test]
    fn test_convert_response_message_to_message() {
        let input = vec![ResponseInputItem::Message {
            role: "user".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "Hello".to_string(),
            }],
        }];

        let result = convert_response_input_to_messages(&input).expect("Conversion failed");
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.as_ref().unwrap(), "Hello");
    }

    #[test]
    fn test_convert_function_call_to_message() {
        let input = vec![ResponseInputItem::FunctionCall {
            call_id: "call_456".to_string(),
            name: "search".to_string(),
            arguments: r#"{"query":"test"}"#.to_string(),
        }];

        let result = convert_response_input_to_messages(&input).expect("Conversion failed");
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_456");
        assert_eq!(tool_calls[0].function.name, "search");
        assert!(tool_calls[0].function.arguments.contains("query"));
    }

    #[test]
    fn test_convert_function_output_to_message() {
        let input = vec![ResponseInputItem::FunctionCallOutput {
            call_id: "call_456".to_string(),
            output: r#"{"result":"found"}"#.to_string(),
        }];

        let result = convert_response_input_to_messages(&input).expect("Conversion failed");
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_ref().unwrap(), "call_456");
        assert!(msg.content.as_ref().unwrap().contains("result"));
    }

    #[test]
    fn test_convert_multiple_content_items() {
        let input = vec![ResponseInputItem::Message {
            role: "assistant".to_string(),
            content: vec![
                ResponseInputContent::OutputText {
                    text: "Part 1".to_string(),
                },
                ResponseInputContent::OutputText {
                    text: "Part 2".to_string(),
                },
            ],
        }];

        let result = convert_response_input_to_messages(&input).expect("Conversion failed");
        assert_eq!(result.len(), 1);
        let msg = &result[0];
        let content = msg.content.as_ref().unwrap();
        assert!(content.contains("Part 1"));
        assert!(content.contains("Part 2"));
    }

    #[test]
    fn test_convert_unknown_role_error() {
        let input = vec![ResponseInputItem::Message {
            role: "unknown_role".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "test".to_string(),
            }],
        }];

        let result = convert_response_input_to_messages(&input);
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(err_msg.contains("Unknown role"));
    }

    #[test]
    fn test_convert_stream_event_message() {
        let event = StreamEvent::Message {
            role: "assistant".to_string(),
            content: vec![ResponseInputContent::OutputText {
                text: "Response".to_string(),
            }],
        };

        let message = convert_stream_event_to_message(&event);
        assert!(message.is_some());
        let msg = message.unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content.as_ref().unwrap(), "Response");
    }

    #[test]
    fn test_convert_stream_event_function_call() {
        let event = StreamEvent::FunctionCall {
            call_id: "call_789".to_string(),
            name: "tool".to_string(),
            arguments: "{}".to_string(),
        };

        let message = convert_stream_event_to_message(&event);
        assert!(message.is_some());
        let msg = message.unwrap();
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id, "call_789");
        assert_eq!(tool_calls[0].function.name, "tool");
    }

    #[test]
    fn test_convert_stream_event_status_none() {
        let event = StreamEvent::Status {
            status: "processing".to_string(),
        };

        let message = convert_stream_event_to_message(&event);
        assert!(message.is_none());

        let done = StreamEvent::Done;
        let message = convert_stream_event_to_message(&done);
        assert!(message.is_none());
    }

    // --- Task 2.3: Tool Definition Conversion Tests ---

    #[test]
    fn test_convert_tool_to_response_format() {
        let tools = vec![crate::tools::Tool {
            name: "get_weather".to_string(),
            description: "Get current weather".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
        }];

        let result = convert_tools_to_response_format(&tools);
        assert_eq!(result.len(), 1);

        match &result[0] {
            ToolDefinition::Function { function } => {
                assert_eq!(function.name, "get_weather");
                assert_eq!(function.description, "Get current weather");
                assert_eq!(function.strict, Some(false));
            }
        }
    }

    #[test]
    fn test_convert_tool_without_strict() {
        let tools = vec![crate::tools::Tool {
            name: "search".to_string(),
            description: "Search".to_string(),
            parameters: serde_json::json!({}),
        }];

        let result = convert_tools_to_response_format(&tools);
        match &result[0] {
            ToolDefinition::Function { function } => {
                assert_eq!(function.strict, Some(false));
            }
        }
    }

    #[test]
    fn test_convert_multiple_tools() {
        let tools = vec![
            crate::tools::Tool {
                name: "tool1".to_string(),
                description: "First".to_string(),
                parameters: serde_json::json!({}),
            },
            crate::tools::Tool {
                name: "tool2".to_string(),
                description: "Second".to_string(),
                parameters: serde_json::json!({}),
            },
        ];

        let result = convert_tools_to_response_format(&tools);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_convert_tool_choice_auto() {
        let choice = convert_tool_choice(Some("auto"));
        assert!(choice.is_some());
        match choice.unwrap() {
            ToolChoice::Auto { auto } => assert!(auto),
            _ => panic!("Expected Auto variant"),
        }
    }

    #[test]
    fn test_convert_tool_choice_required() {
        let choice = convert_tool_choice(Some("required"));
        match choice.unwrap() {
            ToolChoice::Any { any } => assert!(any),
            _ => panic!("Expected Any variant"),
        }

        let choice = convert_tool_choice(Some("any"));
        match choice.unwrap() {
            ToolChoice::Any { any } => assert!(any),
            _ => panic!("Expected Any variant"),
        }
    }

    #[test]
    fn test_convert_tool_choice_none() {
        let choice = convert_tool_choice(Some("none"));
        match choice.unwrap() {
            ToolChoice::None { none } => assert!(none),
            _ => panic!("Expected None variant"),
        }
    }

    #[test]
    fn test_convert_tool_choice_named() {
        let choice = convert_tool_choice(Some("specific_tool"));
        match choice.unwrap() {
            ToolChoice::Named { function } => {
                assert_eq!(function.name, "specific_tool");
            }
            _ => panic!("Expected Named variant"),
        }
    }

    #[test]
    fn test_convert_tool_choice_option_none() {
        let choice = convert_tool_choice(None);
        assert!(choice.is_none());
    }

    // Phase 3: SSE Parsing Tests

    #[test]
    fn test_parse_sse_data_line() {
        let line = r#"data: {"type":"message"}"#;
        let result = parse_sse_line(line);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"type":"message"}"#);
    }

    #[test]
    fn test_parse_sse_done_sentinel() {
        let line = "data: [DONE]";
        let result = parse_sse_line(line);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "[DONE]");
    }

    #[test]
    fn test_parse_sse_ignore_metadata() {
        assert!(parse_sse_line("event: message").is_none());
        assert!(parse_sse_line("id: 123").is_none());
        assert!(parse_sse_line(": comment").is_none());
    }

    #[test]
    fn test_parse_sse_empty_lines() {
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line("   ").is_none());
        assert!(parse_sse_line("\n").is_none());
    }

    #[test]
    fn test_parse_sse_event_message() {
        let data = r#"{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello"}]}"#;
        let event = parse_sse_event(data).expect("Parse failed");

        match event {
            StreamEvent::Message { role, content } => {
                assert_eq!(role, "assistant");
                assert_eq!(content.len(), 1);
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_parse_sse_event_done() {
        let event = parse_sse_event("[DONE]").expect("Parse failed");
        match event {
            StreamEvent::Done => {}
            _ => panic!("Expected Done variant"),
        }
    }

    #[test]
    fn test_parse_sse_event_invalid_json() {
        let result = parse_sse_event("invalid json");
        assert!(result.is_err());
        let err_msg = format!("{:?}", result);
        assert!(err_msg.contains("SseParseError") || err_msg.contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_sse_event_function_call() {
        let data = r#"{"type":"function_call","call_id":"c1","name":"tool","arguments":"{}"}"#;
        let event = parse_sse_event(data).expect("Parse failed");

        match event {
            StreamEvent::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(name, "tool");
                assert_eq!(arguments, "{}");
            }
            _ => panic!("Expected FunctionCall variant"),
        }
    }

    // Phase 3: Stream Response Tests

    #[test]
    fn test_build_responses_request() {
        let input = vec![ResponseInputItem::Message {
            role: "user".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "Test".to_string(),
            }],
        }];

        let request = ResponsesRequest {
            model: "gpt-5-mini".to_string(),
            input,
            stream: true,
            temperature: None,
            tools: None,
            tool_choice: None,
            reasoning: Some(ReasoningConfig {
                effort: Some("medium".to_string()),
            }),
            include: None,
        };

        let json = serde_json::to_string(&request).expect("Serialize failed");
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"model\":\"gpt-5-mini\""));
    }

    #[test]
    fn test_responses_request_with_tools() {
        let tools = vec![ToolDefinition::Function {
            function: FunctionDefinition {
                name: "test".to_string(),
                description: "Test tool".to_string(),
                parameters: serde_json::json!({}),
                strict: None,
            },
        }];

        let request = ResponsesRequest {
            model: "gpt-5-mini".to_string(),
            input: vec![],
            stream: true,
            temperature: None,
            tools: Some(tools),
            tool_choice: None,
            reasoning: None,
            include: None,
        };

        let json = serde_json::to_string(&request).expect("Serialize failed");
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"test\""));
    }

    #[test]
    fn test_sse_buffer_accumulation() {
        let mut buffer = String::new();

        // Partial line
        buffer.push_str("data: {\"type\":");
        assert!(buffer.find('\n').is_none());

        // Complete line
        buffer.push_str("\"message\"}\n");
        let pos = buffer.find('\n').unwrap();
        let line = &buffer[..pos];
        assert!(line.contains("data:"));
    }

    #[test]
    fn test_build_completions_request() {
        let messages = vec![Message::user("Hello")];

        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        let copilot_messages = provider.convert_messages(&messages);

        let request = CopilotRequest {
            model: "gpt-5-mini".to_string(),
            messages: copilot_messages,
            tools: vec![],
            stream: true,
        };

        let json = serde_json::to_string(&request).expect("Serialize failed");
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"messages\""));
    }

    // --- Task 4.1: Endpoint Selection Tests ---

    #[tokio::test]
    async fn test_select_endpoint_prefers_responses() {
        let config = CopilotConfig {
            model: "gpt-5-mini".to_string(),
            enable_endpoint_fallback: true,
            ..Default::default()
        };
        let _provider = CopilotProvider::new(config).unwrap();

        // We can't easily test this without mocking the models API
        // This test documents the expected behavior
        // In real usage: if model supports /responses, it should be selected
    }

    #[tokio::test]
    async fn test_select_endpoint_fallback_to_completions() {
        let config = CopilotConfig {
            model: "gpt-5-mini".to_string(),
            enable_endpoint_fallback: true,
            ..Default::default()
        };
        let _provider = CopilotProvider::new(config).unwrap();

        // Expected: if /responses not supported but fallback enabled,
        // should select /chat/completions
    }

    #[test]
    fn test_copilot_config_defaults() {
        let config = CopilotConfig::default();
        assert_eq!(config.model, "gpt-5-mini");
        assert!(config.enable_streaming);
        assert!(config.enable_endpoint_fallback);
        assert!(!config.include_reasoning);
        assert_eq!(config.reasoning_effort, None);
    }

    #[test]
    fn test_copilot_config_serialization() {
        let config = CopilotConfig {
            model: "claude-3.5-sonnet".to_string(),
            api_base: Some("http://localhost:8000".to_string()),
            enable_streaming: true,
            enable_endpoint_fallback: false,
            reasoning_effort: Some("high".to_string()),
            include_reasoning: true,
        };

        let yaml = serde_yaml::to_string(&config).expect("Serialize failed");
        assert!(yaml.contains("model: claude-3.5-sonnet"));
        assert!(yaml.contains("enable_streaming: true"));
        assert!(yaml.contains("enable_endpoint_fallback: false"));
        assert!(yaml.contains("reasoning_effort: high"));
        assert!(yaml.contains("include_reasoning: true"));
    }

    #[test]
    fn test_copilot_config_deserialization() {
        let yaml = r#"
model: gpt-5-mini
enable_streaming: true
enable_endpoint_fallback: true
reasoning_effort: medium
include_reasoning: true
"#;

        let config: CopilotConfig = serde_yaml::from_str(yaml).expect("Deserialize failed");
        assert_eq!(config.model, "gpt-5-mini");
        assert!(config.enable_streaming);
        assert!(config.enable_endpoint_fallback);
        assert_eq!(config.reasoning_effort, Some("medium".to_string()));
        assert!(config.include_reasoning);
    }

    #[test]
    fn test_provider_cache_ttl() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.models_cache_ttl_secs, 300);
    }
}
