//! OpenAI provider implementation for XZatoma
//!
//! This module implements the [`Provider`] trait for the OpenAI Chat Completions API.
//! It supports both SSE streaming and non-streaming completion paths, tool calling,
//! model listing with a 5-minute cache, model switching, and Bearer token
//! authorization. Any server that implements the OpenAI Chat Completions API
//! (llama.cpp, vLLM, Mistral.rs, Candle-vLLM) can be targeted by overriding
//! `base_url` in [`OpenAIConfig`].
//!
//! # Streaming vs non-streaming path
//!
//! When `enable_streaming` is `true` and no tool schemas are passed to
//! [`Provider::complete`], the SSE streaming path is used and the response is
//! accumulated into a single [`CompletionResponse`] by [`StreamAccumulator`].
//! When tools are present, the non-streaming path is always used to avoid
//! partial tool-call accumulation. Both paths populate `finish_reason` and
//! token usage when available.
//!
//! # Model listing
//!
//! The [`list_models`] implementation filters out non-chat models (embedding,
//! TTS, Whisper, DALL-E, and moderation) and infers per-model capabilities
//! from the model identifier using [`build_capabilities_from_id`].

use crate::config::OpenAIConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    convert_tools_from_json, validate_message_sequence, CompletionResponse, FinishReason,
    FunctionCall, Message, ModelCapability, ModelInfo, Provider, ProviderCapabilities,
    ProviderTool, TokenUsage, ToolCall,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Type alias for the in-memory model cache shared across async operations.
///
/// Caches the list of available models together with the timestamp of the last
/// fetch so that repeated calls to `list_models` can avoid hitting the API on
/// every invocation.
type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>;

// ---------------------------------------------------------------------------
// Non-streaming wire types
// ---------------------------------------------------------------------------

/// OpenAI Chat Completions request body (`POST /v1/chat/completions`).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ProviderTool>,
    stream: bool,
}

/// Single message in an OpenAI request or response body.
///
/// `content` is `Option<String>` because the OpenAI API permits `null` for
/// assistant messages that contain only tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// A single tool call inside an OpenAI assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

/// Function name and serialized JSON arguments within an [`OpenAIToolCall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

/// Top-level OpenAI Chat Completions response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
    model: Option<String>,
}

/// One completion choice inside an [`OpenAIResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

/// Token usage counters returned by the non-streaming completion path.
///
/// Also used for optional usage reporting in the SSE streaming path when
/// the server includes a `usage` field in the final chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Response from the OpenAI `GET /v1/models` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelEntry>,
}

/// Single model entry in the models list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIModelEntry {
    id: String,
    owned_by: Option<String>,
}

// ---------------------------------------------------------------------------
// SSE streaming wire types
// ---------------------------------------------------------------------------

/// A single chunk delivered over the SSE stream.
///
/// The optional `usage` field is populated in the final chunk by some servers
/// and model versions; it is treated as informational when present.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

/// One choice delta inside a streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
    index: u32,
}

/// Content, reasoning, or tool-call delta for a single streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIStreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    /// Reasoning content emitted by extended-thinking models (e.g. the o1 family).
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIStreamToolCallDelta>>,
}

/// Incremental tool-call data delivered in streaming chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIStreamToolCallDelta {
    index: u32,
    id: Option<String>,
    r#type: Option<String>,
    function: Option<OpenAIStreamFunctionDelta>,
}

/// Incremental function name and argument data within a streaming tool-call delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIStreamFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

// ---------------------------------------------------------------------------
// Private utility functions
// ---------------------------------------------------------------------------

/// Map an OpenAI finish-reason string to a typed [`FinishReason`].
///
/// Handles the OpenAI API strings `"stop"`, `"length"`, `"tool_calls"`,
/// `"function_call"` (the legacy tool-call alias used by older model versions),
/// and `"content_filter"`. Any unrecognized or empty string maps to
/// [`FinishReason::Stop`].
///
/// # Arguments
///
/// * `s` - Raw finish-reason string from the API response
///
/// # Returns
///
/// The corresponding [`FinishReason`] variant, defaulting to `Stop`.
fn map_finish_reason(s: &str) -> FinishReason {
    match s {
        "stop" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "tool_calls" | "function_call" => FinishReason::ToolCalls,
        "content_filter" => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

/// Return `true` when the model ID belongs to a non-chat model category.
///
/// Filters out embedding, text-to-speech, speech-to-text, image generation,
/// and moderation models that must not appear in the chat model listing.
/// The comparison is case-insensitive.
///
/// # Arguments
///
/// * `id` - The model identifier to classify
///
/// # Returns
///
/// `true` when `id` (lowercased) contains any of `"embed"`, `"tts"`,
/// `"whisper"`, `"dall-e"`, or `"moderation"`; `false` otherwise.
fn is_non_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    lower.contains("embed")
        || lower.contains("tts")
        || lower.contains("whisper")
        || lower.contains("dall-e")
        || lower.contains("moderation")
}

/// Infer [`ModelCapability`] values from an OpenAI model identifier.
///
/// All OpenAI chat models support streaming. Function calling is assumed unless
/// the model ID contains patterns associated with older completion-only model
/// families: `"babbage"`, `"davinci"`, `"curie"`, `"ada"`, or `"text-"`.
///
/// This function is intended to replace the previous unconditional assignment
/// of the deprecated `Completion` variant. It never constructs that variant.
///
/// # Arguments
///
/// * `id` - The model identifier string
///
/// # Returns
///
/// A `Vec<ModelCapability>` that always includes [`ModelCapability::Streaming`]
/// and conditionally includes [`ModelCapability::FunctionCalling`].
fn build_capabilities_from_id(id: &str) -> Vec<ModelCapability> {
    let id_lower = id.to_lowercase();
    let mut caps = vec![ModelCapability::Streaming];

    // Older completion-only families do not expose the function-calling API.
    let no_fc_patterns = ["babbage", "davinci", "curie", "ada", "text-"];
    if !no_fc_patterns.iter().any(|p| id_lower.contains(p)) {
        caps.push(ModelCapability::FunctionCalling);
    }

    caps
}

/// Percent-encode characters that are unsafe in a URL path segment.
///
/// Encodes `%`, `/`, `?`, and `#` in that order so that a model identifier
/// containing any of those characters can be embedded safely in a URL path
/// without being misinterpreted as path or query delimiters.
///
/// The `%` character is encoded first to prevent double-encoding any existing
/// percent sequences in the input string.
///
/// # Arguments
///
/// * `s` - The raw path segment string
///
/// # Returns
///
/// A new `String` with the four special characters replaced by their
/// percent-encoded equivalents.
fn encode_path_segment(s: &str) -> String {
    s.replace('%', "%25")
        .replace('/', "%2F")
        .replace('?', "%3F")
        .replace('#', "%23")
}

// ---------------------------------------------------------------------------
// Streaming accumulator helper types
// ---------------------------------------------------------------------------

/// Accumulated tool-call state built up across SSE streaming chunks.
///
/// Each entry in the tool-call map keyed by `index` collects the `id`,
/// function `name`, and incrementally appended `arguments_buf` from the delta
/// stream, producing a fully formed [`ToolCall`] after the stream ends.
struct AccumulatedToolCall {
    id: String,
    name: String,
    arguments_buf: String,
}

/// Streaming completion accumulator.
///
/// Collects incremental `content`, optional `reasoning`, `tool_calls`,
/// `usage`, and `finish_reason` from successive [`OpenAIStreamChunk`]s parsed
/// out of the SSE event stream, then produces a single [`CompletionResponse`]
/// via [`StreamAccumulator::finalize`].
///
/// Replace the inline per-field buffers in the streaming loop with a single
/// `StreamAccumulator::new()`, call [`apply_chunk`] for each parsed chunk,
/// and call [`finalize`] to produce the response.
struct StreamAccumulator {
    /// Accumulated text content from `delta.content` fields.
    content: String,
    /// Accumulated reasoning content from `delta.reasoning` fields, if any.
    reasoning: Option<String>,
    /// Partial tool-call state keyed by the delta `index` field.
    tool_calls: HashMap<u32, AccumulatedToolCall>,
    /// Token usage, populated from the `usage` field of the final chunk when present.
    usage: Option<TokenUsage>,
    /// Last-seen finish reason; defaults to [`FinishReason::Stop`].
    finish_reason: FinishReason,
}

impl StreamAccumulator {
    /// Create a new, empty [`StreamAccumulator`].
    fn new() -> Self {
        Self {
            content: String::new(),
            reasoning: None,
            tool_calls: HashMap::new(),
            usage: None,
            finish_reason: FinishReason::Stop,
        }
    }

    /// Process a single parsed SSE chunk and update the accumulator state.
    ///
    /// Appends `delta.content` to the content buffer, appends
    /// `delta.reasoning` to the reasoning buffer (creating it on first use),
    /// delegates tool-call deltas to [`apply_tool_call_chunk`], captures
    /// `finish_reason` when present, and records `usage` when the chunk
    /// carries a usage object.
    ///
    /// # Arguments
    ///
    /// * `chunk` - A reference to one parsed [`OpenAIStreamChunk`]
    fn apply_chunk(&mut self, chunk: &OpenAIStreamChunk) {
        if let Some(choice) = chunk.choices.first() {
            let delta = &choice.delta;

            if let Some(ref c) = delta.content {
                self.content.push_str(c);
            }

            if let Some(ref r) = delta.reasoning {
                self.reasoning.get_or_insert_with(String::new).push_str(r);
            }

            if let Some(ref tc_deltas) = delta.tool_calls {
                self.apply_tool_call_chunk(tc_deltas);
            }

            if let Some(ref fr_str) = choice.finish_reason {
                self.finish_reason = map_finish_reason(fr_str);
            }
        }

        // The usage object lives at the chunk level, outside the choices array.
        if let Some(ref u) = chunk.usage {
            self.usage = Some(TokenUsage::new(
                u.prompt_tokens as usize,
                u.completion_tokens as usize,
            ));
        }
    }

    /// Apply incremental tool-call delta entries to the accumulator map.
    ///
    /// Each delta is keyed by its `index` field. The first delta for each
    /// index supplies the `id` and function `name`; subsequent deltas append
    /// additional `arguments` fragments to the buffer.
    ///
    /// # Arguments
    ///
    /// * `tc_deltas` - Slice of tool-call deltas from `delta.tool_calls`
    fn apply_tool_call_chunk(&mut self, tc_deltas: &[OpenAIStreamToolCallDelta]) {
        for tc_delta in tc_deltas {
            let entry =
                self.tool_calls
                    .entry(tc_delta.index)
                    .or_insert_with(|| AccumulatedToolCall {
                        id: String::new(),
                        name: String::new(),
                        arguments_buf: String::new(),
                    });

            if let Some(ref id) = tc_delta.id {
                if entry.id.is_empty() {
                    entry.id = id.clone();
                }
            }

            if let Some(ref func) = tc_delta.function {
                if let Some(ref name) = func.name {
                    if entry.name.is_empty() {
                        entry.name = name.clone();
                    }
                }
                if let Some(ref args) = func.arguments {
                    entry.arguments_buf.push_str(args);
                }
            }
        }
    }

    /// Consume the accumulator and produce a [`CompletionResponse`].
    ///
    /// When the accumulator contains any tool calls, the response message is
    /// built via [`Message::assistant_with_tools`] with tool calls ordered
    /// by their delta `index`. Otherwise the accumulated `content` string is
    /// used. Token usage and `finish_reason` are always included. When
    /// reasoning content was captured it is set on the response.
    fn finalize(self) -> CompletionResponse {
        let message = if !self.tool_calls.is_empty() {
            let mut tc_list: Vec<(u32, AccumulatedToolCall)> =
                self.tool_calls.into_iter().collect();
            tc_list.sort_by_key(|(idx, _)| *idx);
            let tool_calls: Vec<ToolCall> = tc_list
                .into_iter()
                .map(|(_, acc)| ToolCall {
                    id: acc.id,
                    function: FunctionCall {
                        name: acc.name,
                        arguments: acc.arguments_buf,
                    },
                })
                .collect();
            Message::assistant_with_tools(tool_calls)
        } else {
            Message::assistant(self.content)
        };

        let base = if let Some(usage) = self.usage {
            CompletionResponse::with_usage(message, usage)
        } else {
            CompletionResponse::new(message)
        };

        let base = base.with_finish_reason(self.finish_reason);

        if let Some(reasoning) = self.reasoning {
            base.set_reasoning(reasoning)
        } else {
            base
        }
    }
}

// ---------------------------------------------------------------------------
// Provider struct
// ---------------------------------------------------------------------------

/// OpenAI API provider for XZatoma.
///
/// Connects to the OpenAI Chat Completions API (or any compatible server) to
/// generate completions. Supports tool calling, model listing with a 5-minute
/// cache, model switching, and both SSE streaming and non-streaming paths.
///
/// Both the streaming and non-streaming paths now populate `finish_reason` and
/// token usage on the returned [`CompletionResponse`] when the server provides
/// them.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::OpenAIConfig;
/// use xzatoma::providers::{OpenAIProvider, Provider, Message};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = OpenAIConfig {
///     api_key: "sk-test".to_string(),
///     base_url: "https://api.openai.com/v1".to_string(),
///     model: "gpt-4o-mini".to_string(),
///     organization_id: None,
///     enable_streaming: false,
/// };
/// let provider = OpenAIProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let completion = provider.complete(&messages, &[]).await?;
/// let _message = completion.message;
/// # Ok(())
/// # }
/// ```
pub struct OpenAIProvider {
    client: Client,
    config: Arc<RwLock<OpenAIConfig>>,
    model_cache: ModelCache,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider instance.
    ///
    /// Builds an HTTP client with a 120-second timeout and the `xzatoma/0.1.0`
    /// user-agent string, then wraps the provided configuration in an
    /// `Arc<RwLock<_>>` for safe shared access.
    ///
    /// # Arguments
    ///
    /// * `config` - OpenAI configuration containing the API key, base URL, model,
    ///   and streaming preference
    ///
    /// # Returns
    ///
    /// Returns a new `OpenAIProvider` ready for use.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the underlying HTTP client cannot be
    /// initialized (for example, if TLS initialization fails).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OpenAIConfig;
    /// use xzatoma::providers::OpenAIProvider;
    ///
    /// let config = OpenAIConfig::default();
    /// let provider = OpenAIProvider::new(config);
    /// assert!(provider.is_ok());
    /// ```
    pub fn new(config: OpenAIConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!(
            "Initialized OpenAI provider: base_url={}, model={}",
            config.base_url,
            config.model
        );

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            model_cache: Arc::new(RwLock::new(None)),
        })
    }

    /// Return the configured base URL for this provider.
    ///
    /// Reads the value under the read lock and returns a clone. Returns an
    /// empty string if the lock is poisoned.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OpenAIConfig;
    /// use xzatoma::providers::OpenAIProvider;
    ///
    /// let config = OpenAIConfig::default();
    /// let provider = OpenAIProvider::new(config).unwrap();
    /// assert_eq!(provider.base_url(), "https://api.openai.com/v1");
    /// ```
    pub fn base_url(&self) -> String {
        self.config
            .read()
            .map(|c| c.base_url.clone())
            .unwrap_or_default()
    }

    /// Return the name of the currently configured model.
    ///
    /// Reads the value under the read lock and returns a clone. Returns an
    /// empty string if the lock is poisoned.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::OpenAIConfig;
    /// use xzatoma::providers::OpenAIProvider;
    ///
    /// let config = OpenAIConfig::default();
    /// let provider = OpenAIProvider::new(config).unwrap();
    /// assert_eq!(provider.model(), "gpt-4o-mini");
    /// ```
    pub fn model(&self) -> String {
        self.config
            .read()
            .map(|c| c.model.clone())
            .unwrap_or_default()
    }

    /// Convert XZatoma messages to OpenAI wire format.
    ///
    /// Calls [`validate_message_sequence`] to drop orphan tool messages, then
    /// maps each validated [`Message`] to [`OpenAIMessage`]. Messages where
    /// both `content` and `tool_calls` are `None` are skipped entirely.
    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAIMessage> {
        let validated = validate_message_sequence(messages);
        validated
            .into_iter()
            .filter_map(|m| {
                if m.content.is_none() && m.tool_calls.is_none() {
                    return None;
                }

                let tool_calls = m.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|tc| OpenAIToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: OpenAIFunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect()
                });

                Some(OpenAIMessage {
                    role: m.role,
                    content: m.content,
                    tool_calls,
                    tool_call_id: m.tool_call_id,
                })
            })
            .collect()
    }

    /// Convert an OpenAI response message back to the XZatoma [`Message`] type.
    ///
    /// If the message contains non-empty tool calls, returns
    /// [`Message::assistant_with_tools`]. Otherwise returns
    /// [`Message::assistant`] with the content, defaulting to an empty string
    /// when `content` is `None`.
    fn convert_response_message(&self, msg: OpenAIMessage) -> Message {
        if let Some(tool_calls) = msg.tool_calls {
            if !tool_calls.is_empty() {
                let converted: Vec<ToolCall> = tool_calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id,
                        function: FunctionCall {
                            name: tc.function.name,
                            arguments: tc.function.arguments,
                        },
                    })
                    .collect();
                return Message::assistant_with_tools(converted);
            }
        }
        Message::assistant(msg.content.unwrap_or_default())
    }

    /// Build the HTTP headers required for every request to the OpenAI API.
    ///
    /// Always inserts `Content-Type: application/json`. Adds
    /// `Authorization: Bearer <api_key>` when `api_key` is non-empty. Adds
    /// `OpenAI-Organization: <org_id>` when an organization ID is configured
    /// and non-empty.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if a header value cannot be constructed
    /// from the configured strings (for example, if the API key contains
    /// non-ASCII characters that are invalid in HTTP header values).
    fn build_request_headers(&self) -> Result<reqwest::header::HeaderMap> {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let (api_key, organization_id) = {
            let config = self.config.read().map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?;
            (config.api_key.clone(), config.organization_id.clone())
        };

        if !api_key.is_empty() {
            let auth_value = format!("Bearer {}", api_key);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth_value).map_err(|e| {
                    XzatomaError::Provider(format!("Invalid API key header value: {}", e))
                })?,
            );
        }

        if let Some(org_id) = organization_id {
            if !org_id.is_empty() {
                headers.insert(
                    HeaderName::from_static("openai-organization"),
                    HeaderValue::from_str(&org_id).map_err(|e| {
                        XzatomaError::Provider(format!(
                            "Invalid organization ID header value: {}",
                            e
                        ))
                    })?,
                );
            }
        }

        Ok(headers)
    }

    /// Send a non-streaming POST to `/chat/completions` and return the parsed
    /// completion response.
    ///
    /// The returned [`CompletionResponse`] includes `finish_reason` derived
    /// from the first choice's finish reason field and token usage when present.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails, the server
    /// returns a non-success status, the body cannot be deserialized, or the
    /// response contains no choices.
    async fn post_completions(&self, request: &OpenAIRequest) -> Result<CompletionResponse> {
        let headers = self.build_request_headers()?;
        let url = format!("{}/chat/completions", self.base_url());

        tracing::debug!(
            "Sending OpenAI request (non-streaming): {} messages, {} tools",
            request.messages.len(),
            request.tools.len()
        );

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("OpenAI request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::Provider(format!("HTTP {}: {}", status, body)));
        }

        let openai_response: OpenAIResponse = response.json().await.map_err(|e| {
            XzatomaError::Provider(format!("Failed to parse OpenAI response: {}", e))
        })?;

        let choice = openai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| XzatomaError::Provider("No choices in response".to_string()))?;

        // Capture finish_reason before consuming choice.message.
        let finish_reason = map_finish_reason(choice.finish_reason.as_deref().unwrap_or("stop"));

        let message = self.convert_response_message(choice.message);

        let completion = if let Some(usage) = openai_response.usage {
            let token_usage = TokenUsage::new(
                usage.prompt_tokens as usize,
                usage.completion_tokens as usize,
            );
            CompletionResponse::with_usage(message, token_usage)
        } else {
            CompletionResponse::new(message)
        };

        let completion = completion.with_finish_reason(finish_reason);

        let completion = if let Some(model) = openai_response.model {
            completion.set_model(model)
        } else {
            completion
        };

        Ok(completion)
    }

    /// Send a streaming POST to `/chat/completions` using SSE and accumulate
    /// the full response into a single [`CompletionResponse`].
    ///
    /// Tool-use requests are always routed through the non-streaming path; this
    /// method is only called when the request contains no tool schemas.
    ///
    /// The response includes `finish_reason` and token usage when provided by
    /// the server. Reasoning content (from extended-thinking models) is
    /// captured and set on the response when present.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails, the server
    /// returns a non-success status, or a byte-level read error occurs on the
    /// stream.
    async fn post_completions_streaming(
        &self,
        request: &OpenAIRequest,
    ) -> Result<CompletionResponse> {
        use futures::StreamExt;

        let mut headers = self.build_request_headers()?;
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("text/event-stream"),
        );
        let url = format!("{}/chat/completions", self.base_url());

        tracing::debug!(
            "Sending OpenAI request (streaming): {} messages",
            request.messages.len()
        );

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .await
            .map_err(|e| {
                XzatomaError::Provider(format!("OpenAI streaming request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::Provider(format!("HTTP {}: {}", status, body)));
        }

        let mut stream = response.bytes_stream();
        let mut acc = StreamAccumulator::new();
        let mut line_buf: Vec<u8> = Vec::new();

        'stream: while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| XzatomaError::Provider(format!("Error reading SSE stream: {}", e)))?;

            for byte in chunk {
                if byte == b'\n' {
                    let raw_line = String::from_utf8_lossy(&line_buf).to_string();
                    line_buf.clear();
                    let line = raw_line.trim_start().to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if line.starts_with(':') {
                        continue;
                    }

                    if let Some(after_prefix) = line.strip_prefix("data:") {
                        let payload = after_prefix.trim();

                        if payload == "[DONE]" {
                            break 'stream;
                        }

                        match serde_json::from_str::<OpenAIStreamChunk>(payload) {
                            Ok(sse_chunk) => {
                                acc.apply_chunk(&sse_chunk);
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "Failed to parse SSE chunk: {} (payload: {:?})",
                                    e,
                                    payload
                                );
                            }
                        }
                    }
                } else {
                    line_buf.push(byte);
                }
            }
        }

        Ok(acc.finalize())
    }

    /// Search for a model by name in the cached or freshly fetched model list.
    ///
    /// This is the fallback path used by [`get_model_info`] when the direct
    /// `GET /models/{id}` endpoint returns 404 or cannot be deserialized.
    ///
    /// # Arguments
    ///
    /// * `model_name` - The model name to search for
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the model is not found in the list
    /// or if the list request fails.
    async fn find_in_model_list(&self, model_name: &str) -> Result<ModelInfo> {
        let models = self.list_models().await?;
        models
            .into_iter()
            .find(|info| info.name == model_name)
            .ok_or_else(|| XzatomaError::Provider(format!("Model not found: {}", model_name)))
    }
}

// ---------------------------------------------------------------------------
// Provider trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Provider for OpenAIProvider {
    /// Returns `true` if this provider has a non-empty API key stored in its
    /// configuration. Local servers that require no auth will return `false`
    /// here; that is expected and correct.
    fn is_authenticated(&self) -> bool {
        self.config
            .read()
            .map(|c| !c.api_key.is_empty())
            .unwrap_or(false)
    }

    /// Returns `None` because the model name is stored behind a `RwLock`;
    /// a borrowed `&str` cannot outlive the lock guard. Use
    /// `get_current_model` for an owned copy.
    fn current_model(&self) -> Option<&str> {
        None
    }

    /// Set the active model in memory. No API validation is performed.
    /// Callers that require model-existence validation should call
    /// `list_models` before calling this method.
    fn set_model(&mut self, model: &str) {
        if let Ok(mut config) = self.config.write() {
            config.model = model.to_string();
            tracing::info!("Switched OpenAI model to: {}", model);
        }
    }

    /// Fetch the list of available models. Delegates to the overridden
    /// `list_models`, which uses the 300-second in-process cache.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails or the
    /// response cannot be deserialized.
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        self.list_models().await
    }

    /// Complete a conversation using the OpenAI Chat Completions API.
    ///
    /// When `enable_streaming` is `true` and no tool schemas are provided, the
    /// SSE streaming path is used. When tools are present, the non-streaming
    /// path is always used to avoid partial tool-call accumulation.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails or the
    /// response cannot be parsed.
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
        };

        let openai_tools = convert_tools_from_json(tools);
        let use_streaming = enable_streaming && openai_tools.is_empty();

        let request = OpenAIRequest {
            model,
            messages: self.convert_messages(messages),
            tools: openai_tools,
            stream: use_streaming,
        };

        if use_streaming {
            self.post_completions_streaming(&request).await
        } else {
            self.post_completions(&request).await
        }
    }

    /// List available models from the OpenAI `/v1/models` endpoint.
    ///
    /// Results are cached for 300 seconds (5 minutes). The list is sorted by
    /// model name before being returned. Non-chat models (embedding, TTS,
    /// Whisper, DALL-E, moderation) are excluded. Each remaining entry is
    /// annotated with capabilities inferred by [`build_capabilities_from_id`].
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails or the
    /// response body cannot be deserialized.
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        if let Ok(cache) = self.model_cache.read() {
            if let Some((models, cached_at)) = cache.as_ref() {
                if cached_at.elapsed() < Duration::from_secs(300) {
                    tracing::debug!("Using cached OpenAI model list");
                    return Ok(models.clone());
                }
            }
        }

        let headers = self.build_request_headers()?;
        let url = format!("{}/models", self.base_url());
        tracing::debug!("Fetching OpenAI models from: {}", url);

        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to fetch models: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::Provider(format!("HTTP {}: {}", status, body)));
        }

        let models_response: OpenAIModelsResponse = response.json().await.map_err(|e| {
            XzatomaError::Provider(format!("Failed to parse models response: {}", e))
        })?;

        let mut models: Vec<ModelInfo> = models_response
            .data
            .into_iter()
            .filter(|entry| !is_non_chat_model(&entry.id))
            .map(|entry| {
                let mut info = ModelInfo::new(entry.id.clone(), entry.id.clone(), 0);
                for cap in build_capabilities_from_id(&entry.id) {
                    info.add_capability(cap);
                }
                info
            })
            .collect();

        models.sort_by(|a, b| a.name.cmp(&b.name));

        if let Ok(mut cache) = self.model_cache.write() {
            *cache = Some((models.clone(), Instant::now()));
        }

        Ok(models)
    }

    /// Get information about a specific model by name.
    ///
    /// First attempts a direct `GET /models/{encoded_id}` request, where
    /// `encoded_id` is the model name with `%`, `/`, `?`, and `#`
    /// percent-encoded via [`encode_path_segment`]. Falls back to a full
    /// model-list scan when the direct request returns HTTP 404 or when the
    /// response body cannot be deserialized as a model entry.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if neither the direct request nor the
    /// list scan succeeds, or if the model is not found in the list.
    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
        let encoded = encode_path_segment(model_name);
        let url = format!("{}/models/{}", self.base_url(), encoded);

        tracing::debug!("Fetching model info from: {}", url);

        let headers = self.build_request_headers()?;
        let response = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to fetch model info: {}", e)))?;

        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            tracing::debug!(
                "Model {} not found at direct endpoint, falling back to list scan",
                model_name
            );
            return self.find_in_model_list(model_name).await;
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::Provider(format!("HTTP {}: {}", status, body)));
        }

        match response.json::<OpenAIModelEntry>().await {
            Ok(entry) => {
                let mut info = ModelInfo::new(entry.id.clone(), entry.id.clone(), 0);
                for cap in build_capabilities_from_id(&entry.id) {
                    info.add_capability(cap);
                }
                Ok(info)
            }
            Err(e) => {
                tracing::debug!(
                    "Failed to deserialize model entry for {}: {}, falling back to list scan",
                    model_name,
                    e
                );
                self.find_in_model_list(model_name).await
            }
        }
    }

    /// Return the name of the currently configured model.
    ///
    /// Returns `"none"` if the read lock cannot be acquired.
    ///
    /// Overrides the trait default to read from the internal
    /// `RwLock<OpenAIConfig>` directly, since `current_model` cannot return a
    /// borrowed reference to lock-guarded data.
    fn get_current_model(&self) -> String {
        self.config
            .read()
            .map(|c| c.model.clone())
            .unwrap_or_else(|_| "none".to_string())
    }

    /// Return the static capability flags for this provider.
    ///
    /// OpenAI supports model listing, model switching, token counts on the
    /// non-streaming path, and SSE streaming. Detailed per-model metadata
    /// beyond the `id` field is not available through the standard
    /// `/v1/models` endpoint.
    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: false,
            supports_model_switching: true,
            supports_token_counts: true,
            supports_streaming: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // -----------------------------------------------------------------------
    // Helper: build a minimal OpenAIConfig pointing at a mock server
    // -----------------------------------------------------------------------

    fn make_config(server_uri: &str) -> OpenAIConfig {
        OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server_uri.to_string(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: false,
        }
    }

    fn non_streaming_completion_body() -> serde_json::Value {
        json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Hello from OpenAI"
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            },
            "model": "gpt-4o-mini"
        })
    }

    fn models_list_body() -> serde_json::Value {
        json!({
            "data": [
                { "id": "gpt-4o", "owned_by": "openai" },
                { "id": "gpt-4o-mini", "owned_by": "openai" }
            ]
        })
    }

    // -----------------------------------------------------------------------
    // Construction and accessor tests (no HTTP)
    // -----------------------------------------------------------------------

    #[test]
    fn test_openai_provider_creation() {
        let config = OpenAIConfig::default();
        let result = OpenAIProvider::new(config);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
    }

    #[test]
    fn test_openai_provider_base_url() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();
        assert_eq!(provider.base_url(), "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_provider_model() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();
        assert_eq!(provider.model(), "gpt-4o-mini");
    }

    // -----------------------------------------------------------------------
    // map_finish_reason unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_finish_reason_stop_returns_stop() {
        assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
    }

    #[test]
    fn test_map_finish_reason_length_returns_length() {
        assert_eq!(map_finish_reason("length"), FinishReason::Length);
    }

    #[test]
    fn test_map_finish_reason_tool_calls_returns_tool_calls() {
        assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_function_call_maps_to_tool_calls() {
        assert_eq!(map_finish_reason("function_call"), FinishReason::ToolCalls);
    }

    #[test]
    fn test_map_finish_reason_content_filter_returns_content_filter() {
        assert_eq!(
            map_finish_reason("content_filter"),
            FinishReason::ContentFilter
        );
    }

    #[test]
    fn test_map_finish_reason_unknown_string_defaults_to_stop() {
        assert_eq!(map_finish_reason("unknown_value"), FinishReason::Stop);
        assert_eq!(map_finish_reason(""), FinishReason::Stop);
        assert_eq!(map_finish_reason("cancelled"), FinishReason::Stop);
    }

    // -----------------------------------------------------------------------
    // is_non_chat_model unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_non_chat_model_true_for_embed() {
        assert!(is_non_chat_model("text-embedding-ada-002"));
        assert!(is_non_chat_model("text-embedding-3-small"));
        assert!(is_non_chat_model("text-embedding-3-large"));
    }

    #[test]
    fn test_is_non_chat_model_true_for_tts() {
        assert!(is_non_chat_model("tts-1"));
        assert!(is_non_chat_model("tts-1-hd"));
    }

    #[test]
    fn test_is_non_chat_model_true_for_whisper() {
        assert!(is_non_chat_model("whisper-1"));
    }

    #[test]
    fn test_is_non_chat_model_true_for_dall_e() {
        assert!(is_non_chat_model("dall-e-2"));
        assert!(is_non_chat_model("dall-e-3"));
    }

    #[test]
    fn test_is_non_chat_model_true_for_moderation() {
        assert!(is_non_chat_model("text-moderation-latest"));
        assert!(is_non_chat_model("text-moderation-stable"));
        assert!(is_non_chat_model("omni-moderation-latest"));
    }

    #[test]
    fn test_is_non_chat_model_false_for_chat_models() {
        assert!(!is_non_chat_model("gpt-4o"));
        assert!(!is_non_chat_model("gpt-4o-mini"));
        assert!(!is_non_chat_model("gpt-3.5-turbo"));
        assert!(!is_non_chat_model("o1-mini"));
        assert!(!is_non_chat_model("o3-mini"));
        assert!(!is_non_chat_model("o4-mini"));
    }

    // -----------------------------------------------------------------------
    // encode_path_segment unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_encode_path_segment_encodes_slash() {
        assert_eq!(encode_path_segment("gpt-4/extended"), "gpt-4%2Fextended");
    }

    #[test]
    fn test_encode_path_segment_encodes_question_mark() {
        assert_eq!(encode_path_segment("model?version=1"), "model%3Fversion=1");
    }

    #[test]
    fn test_encode_path_segment_encodes_hash() {
        assert_eq!(encode_path_segment("model#v1"), "model%23v1");
    }

    #[test]
    fn test_encode_path_segment_encodes_percent_first() {
        // The percent sign must be encoded before others to avoid double-encoding.
        assert_eq!(encode_path_segment("100%"), "100%25");
        // A pre-existing encoded sequence must not be double-encoded.
        assert_eq!(encode_path_segment("a%2Fb"), "a%252Fb");
    }

    #[test]
    fn test_encode_path_segment_leaves_alphanumerics_unchanged() {
        assert_eq!(encode_path_segment("gpt-4o-mini"), "gpt-4o-mini");
        assert_eq!(encode_path_segment("o3"), "o3");
        assert_eq!(encode_path_segment("abc123"), "abc123");
    }

    #[test]
    fn test_encode_path_segment_encodes_multiple_specials() {
        assert_eq!(
            encode_path_segment("org/model#v1?x=1"),
            "org%2Fmodel%23v1%3Fx=1"
        );
    }

    // -----------------------------------------------------------------------
    // build_capabilities_from_id unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_capabilities_from_id_modern_model_gets_streaming_and_fc() {
        let caps = build_capabilities_from_id("gpt-4o");
        assert!(caps.contains(&ModelCapability::Streaming));
        assert!(caps.contains(&ModelCapability::FunctionCalling));
    }

    #[test]
    fn test_build_capabilities_from_id_gpt4o_mini_gets_streaming_and_fc() {
        let caps = build_capabilities_from_id("gpt-4o-mini");
        assert!(caps.contains(&ModelCapability::Streaming));
        assert!(caps.contains(&ModelCapability::FunctionCalling));
    }

    #[test]
    fn test_build_capabilities_from_id_old_model_gets_streaming_only() {
        for old_name in &[
            "babbage-002",
            "davinci-002",
            "ada",
            "curie",
            "text-davinci-003",
        ] {
            let caps = build_capabilities_from_id(old_name);
            assert!(
                caps.contains(&ModelCapability::Streaming),
                "Expected Streaming for {}",
                old_name
            );
            assert!(
                !caps.contains(&ModelCapability::FunctionCalling),
                "Did not expect FunctionCalling for {}",
                old_name
            );
        }
    }

    #[test]
    fn test_build_capabilities_from_id_never_produces_deprecated_completion() {
        for id in &["gpt-4o", "gpt-4o-mini", "o1-mini", "babbage-002"] {
            let caps = build_capabilities_from_id(id);
            #[allow(deprecated)]
            let has_deprecated = caps.contains(&ModelCapability::Completion);
            assert!(
                !has_deprecated,
                "build_capabilities_from_id must not produce deprecated Completion for {}",
                id
            );
        }
    }

    // -----------------------------------------------------------------------
    // StreamAccumulator unit tests
    // -----------------------------------------------------------------------

    fn make_text_chunk(content: &str, finish_reason: Option<&str>) -> OpenAIStreamChunk {
        OpenAIStreamChunk {
            choices: vec![OpenAIStreamChoice {
                delta: OpenAIStreamDelta {
                    content: Some(content.to_string()),
                    reasoning: None,
                    tool_calls: None,
                },
                finish_reason: finish_reason.map(|s| s.to_string()),
                index: 0,
            }],
            usage: None,
        }
    }

    fn make_reasoning_chunk(reasoning: &str) -> OpenAIStreamChunk {
        OpenAIStreamChunk {
            choices: vec![OpenAIStreamChoice {
                delta: OpenAIStreamDelta {
                    content: None,
                    reasoning: Some(reasoning.to_string()),
                    tool_calls: None,
                },
                finish_reason: None,
                index: 0,
            }],
            usage: None,
        }
    }

    #[test]
    fn test_stream_accumulator_processes_single_text_delta() {
        let mut acc = StreamAccumulator::new();
        acc.apply_chunk(&make_text_chunk("Hello", None));
        let response = acc.finalize();

        assert_eq!(response.message.content.as_deref(), Some("Hello"));
        assert!(response.message.tool_calls.is_none());
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert!(response.usage.is_none());
        assert!(response.reasoning.is_none());
    }

    #[test]
    fn test_stream_accumulator_concatenates_multiple_text_deltas() {
        let mut acc = StreamAccumulator::new();
        acc.apply_chunk(&make_text_chunk("Hello", None));
        acc.apply_chunk(&make_text_chunk(" ", None));
        acc.apply_chunk(&make_text_chunk("world", Some("stop")));
        let response = acc.finalize();

        assert_eq!(response.message.content.as_deref(), Some("Hello world"));
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_stream_accumulator_accumulates_tool_call_deltas() {
        let mut acc = StreamAccumulator::new();

        // First chunk: id and function name.
        let chunk1 = OpenAIStreamChunk {
            choices: vec![OpenAIStreamChoice {
                delta: OpenAIStreamDelta {
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![OpenAIStreamToolCallDelta {
                        index: 0,
                        id: Some("call_abc".to_string()),
                        r#type: Some("function".to_string()),
                        function: Some(OpenAIStreamFunctionDelta {
                            name: Some("read_file".to_string()),
                            arguments: Some("{\"path\"".to_string()),
                        }),
                    }]),
                },
                finish_reason: None,
                index: 0,
            }],
            usage: None,
        };

        // Second chunk: additional arguments and finish reason.
        let chunk2 = OpenAIStreamChunk {
            choices: vec![OpenAIStreamChoice {
                delta: OpenAIStreamDelta {
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![OpenAIStreamToolCallDelta {
                        index: 0,
                        id: None,
                        r#type: None,
                        function: Some(OpenAIStreamFunctionDelta {
                            name: None,
                            arguments: Some(":\"test.txt\"}".to_string()),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
                index: 0,
            }],
            usage: None,
        };

        acc.apply_chunk(&chunk1);
        acc.apply_chunk(&chunk2);
        let response = acc.finalize();

        let calls = response
            .message
            .tool_calls
            .expect("Expected tool calls in response");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, "{\"path\":\"test.txt\"}");
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    }

    #[test]
    fn test_stream_accumulator_orders_tool_calls_by_index() {
        let mut acc = StreamAccumulator::new();

        // Deliver the second tool call before the first to test ordering.
        let chunk = OpenAIStreamChunk {
            choices: vec![OpenAIStreamChoice {
                delta: OpenAIStreamDelta {
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![
                        OpenAIStreamToolCallDelta {
                            index: 1,
                            id: Some("call_b".to_string()),
                            r#type: Some("function".to_string()),
                            function: Some(OpenAIStreamFunctionDelta {
                                name: Some("write_file".to_string()),
                                arguments: Some("{}".to_string()),
                            }),
                        },
                        OpenAIStreamToolCallDelta {
                            index: 0,
                            id: Some("call_a".to_string()),
                            r#type: Some("function".to_string()),
                            function: Some(OpenAIStreamFunctionDelta {
                                name: Some("read_file".to_string()),
                                arguments: Some("{}".to_string()),
                            }),
                        },
                    ]),
                },
                finish_reason: Some("tool_calls".to_string()),
                index: 0,
            }],
            usage: None,
        };

        acc.apply_chunk(&chunk);
        let response = acc.finalize();

        let calls = response
            .message
            .tool_calls
            .expect("Expected tool calls in response");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a", "Index 0 must come first");
        assert_eq!(calls[1].id, "call_b", "Index 1 must come second");
    }

    #[test]
    fn test_stream_accumulator_captures_reasoning_content() {
        let mut acc = StreamAccumulator::new();
        acc.apply_chunk(&make_reasoning_chunk("Let me think..."));
        acc.apply_chunk(&make_reasoning_chunk(" Step 2."));
        acc.apply_chunk(&make_text_chunk("The answer is 42", Some("stop")));
        let response = acc.finalize();

        assert_eq!(
            response.message.content.as_deref(),
            Some("The answer is 42")
        );
        assert_eq!(
            response.reasoning.as_deref(),
            Some("Let me think... Step 2.")
        );
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_stream_accumulator_no_reasoning_when_absent() {
        let mut acc = StreamAccumulator::new();
        acc.apply_chunk(&make_text_chunk("Result", Some("stop")));
        let response = acc.finalize();

        assert!(response.reasoning.is_none());
    }

    #[test]
    fn test_stream_accumulator_captures_usage_from_chunk() {
        let mut acc = StreamAccumulator::new();
        acc.apply_chunk(&make_text_chunk("Done", Some("stop")));

        // Simulate a final usage chunk (no content delta).
        let usage_chunk = OpenAIStreamChunk {
            choices: vec![],
            usage: Some(OpenAIUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
                total_tokens: 30,
            }),
        };
        acc.apply_chunk(&usage_chunk);
        let response = acc.finalize();

        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 20);
        assert_eq!(usage.completion_tokens, 10);
    }

    #[test]
    fn test_stream_accumulator_empty_produces_empty_assistant_message() {
        let acc = StreamAccumulator::new();
        let response = acc.finalize();

        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content.as_deref(), Some(""));
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert!(response.usage.is_none());
        assert!(response.reasoning.is_none());
    }

    // -----------------------------------------------------------------------
    // Message conversion tests (no HTTP)
    // -----------------------------------------------------------------------

    #[test]
    fn test_convert_messages_basic() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let result = provider.convert_messages(&messages);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[0].content.as_deref(), Some("Hello"));
        assert!(result[0].tool_calls.is_none());
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();

        let tool_call = ToolCall {
            id: "call_abc".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"test.txt"}"#.to_string(),
            },
        };
        let messages = vec![Message::assistant_with_tools(vec![tool_call])];

        let result = provider.convert_messages(&messages);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        let calls = result[0]
            .tool_calls
            .as_ref()
            .expect("tool_calls should be Some");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, r#"{"path":"test.txt"}"#);
    }

    #[test]
    fn test_convert_messages_drops_orphan_tool() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();

        let messages = vec![
            Message::user("run something"),
            Message::tool_result("orphan_id", "result"),
        ];

        let result = provider.convert_messages(&messages);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
    }

    #[test]
    fn test_convert_messages_preserves_valid_tool_pair() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "list_files".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let messages = vec![
            Message::user("list files"),
            Message::assistant_with_tools(vec![tool_call]),
            Message::tool_result("call_123", "file_a.txt\nfile_b.txt"),
        ];

        let result = provider.convert_messages(&messages);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, "user");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[2].role, "tool");
        assert_eq!(result[2].tool_call_id.as_deref(), Some("call_123"));
    }

    // -----------------------------------------------------------------------
    // Response message conversion tests (no HTTP)
    // -----------------------------------------------------------------------

    #[test]
    fn test_convert_response_message_text() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();

        let msg = OpenAIMessage {
            role: "assistant".to_string(),
            content: Some("The answer is 42".to_string()),
            tool_calls: None,
            tool_call_id: None,
        };

        let result = provider.convert_response_message(msg);

        assert_eq!(result.role, "assistant");
        assert_eq!(result.content.as_deref(), Some("The answer is 42"));
        assert!(result.tool_calls.is_none());
    }

    #[test]
    fn test_convert_response_message_with_tools() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();

        let msg = OpenAIMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![OpenAIToolCall {
                id: "call_xyz".to_string(),
                r#type: "function".to_string(),
                function: OpenAIFunctionCall {
                    name: "write_file".to_string(),
                    arguments: r#"{"path":"out.txt","content":"data"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
        };

        let result = provider.convert_response_message(msg);

        assert_eq!(result.role, "assistant");
        let calls = result.tool_calls.expect("tool_calls should be Some");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_xyz");
        assert_eq!(calls[0].function.name, "write_file");
        assert_eq!(
            calls[0].function.arguments,
            r#"{"path":"out.txt","content":"data"}"#
        );
    }

    // -----------------------------------------------------------------------
    // Tool conversion test (no HTTP)
    // -----------------------------------------------------------------------

    #[test]
    fn test_convert_tools() {
        let tools = vec![
            json!({
                "name": "read_file",
                "description": "Read a file",
                "parameters": { "type": "object", "properties": {} }
            }),
            json!({
                "name": "write_file",
                "description": "Write a file",
                "parameters": { "type": "object", "properties": {} }
            }),
        ];

        let result = convert_tools_from_json(&tools);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].function.name, "read_file");
        assert_eq!(result[1].function.name, "write_file");
    }

    // -----------------------------------------------------------------------
    // Capability and model accessor tests (no HTTP)
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_current_model() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model(), "gpt-4o-mini");
    }

    #[test]
    fn test_provider_capabilities() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config).unwrap();
        let caps = provider.get_provider_capabilities();

        assert!(caps.supports_model_listing);
        assert!(!caps.supports_model_details);
        assert!(caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(caps.supports_streaming);
    }

    // -----------------------------------------------------------------------
    // HTTP interaction tests (wiremock)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_complete_non_streaming() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_completion_body()))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let result = provider.complete(&messages, &[]).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let response = result.unwrap();
        assert_eq!(
            response.message.content.as_deref(),
            Some("Hello from OpenAI")
        );
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(
            response.finish_reason,
            FinishReason::Stop,
            "Non-streaming path must populate finish_reason"
        );
    }

    #[tokio::test]
    async fn test_complete_non_streaming_length_finish_reason() {
        let server = MockServer::start().await;

        let body = json!({
            "choices": [{
                "message": { "role": "assistant", "content": "truncated" },
                "finish_reason": "length"
            }],
            "usage": { "prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150 },
            "model": "gpt-4o-mini"
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.complete(&[Message::user("Hello")], &[]).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().finish_reason, FinishReason::Length);
    }

    #[tokio::test]
    async fn test_complete_streaming() {
        let server = MockServer::start().await;

        let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"},\"finish_reason\":null,\"index\":0}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"world\"},\"finish_reason\":\"stop\",\"index\":0}]}\n\ndata: [DONE]\n\n";

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: true,
        };
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let result = provider.complete(&messages, &[]).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let response = result.unwrap();
        assert_eq!(
            response.message.content.as_deref(),
            Some("Hello world"),
            "Accumulated content mismatch"
        );
        assert_eq!(
            response.finish_reason,
            FinishReason::Stop,
            "Streaming path must populate finish_reason"
        );
    }

    #[tokio::test]
    async fn test_stream_accumulator_done_sentinel_terminates_stream() {
        let server = MockServer::start().await;

        // Content after [DONE] must not be included in the response.
        let sse_body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Before\"},\"finish_reason\":null,\"index\":0}]}\n\n",
            "data: [DONE]\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" IGNORE\"},\"finish_reason\":null,\"index\":0}]}\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: true,
        };
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.complete(&[Message::user("Hello")], &[]).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let response = result.unwrap();
        assert_eq!(
            response.message.content.as_deref(),
            Some("Before"),
            "Content after [DONE] must be ignored"
        );
    }

    #[tokio::test]
    async fn test_complete_streaming_captures_finish_reason() {
        let server = MockServer::start().await;

        let sse_body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null,\"index\":0}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\",\"index\":0}]}\n\n",
            "data: [DONE]\n\n",
        );

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_body),
            )
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: true,
        };
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.complete(&[Message::user("Hello")], &[]).await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        assert_eq!(result.unwrap().finish_reason, FinishReason::Length);
    }

    #[tokio::test]
    async fn test_list_models() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(models_list_body()))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.list_models().await;

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let models = result.unwrap();
        assert_eq!(models.len(), 2);
        // Results must be sorted by name.
        assert_eq!(models[0].name, "gpt-4o");
        assert_eq!(models[1].name, "gpt-4o-mini");
        // Each model must have Streaming and FunctionCalling capabilities.
        assert!(
            models[0].supports_capability(ModelCapability::Streaming),
            "gpt-4o must have Streaming"
        );
        assert!(
            models[0].supports_capability(ModelCapability::FunctionCalling),
            "gpt-4o must have FunctionCalling"
        );
        // Verify the deprecated Completion variant is NOT assigned.
        #[allow(deprecated)]
        let has_deprecated = models[0].supports_capability(ModelCapability::Completion);
        assert!(
            !has_deprecated,
            "list_models must not assign the deprecated Completion capability"
        );
    }

    #[tokio::test]
    async fn test_fetch_models_filters_non_chat_models() {
        let server = MockServer::start().await;

        let body = json!({
            "data": [
                { "id": "gpt-4o", "owned_by": "openai" },
                { "id": "text-embedding-ada-002", "owned_by": "openai" },
                { "id": "whisper-1", "owned_by": "openai" },
                { "id": "dall-e-3", "owned_by": "openai" },
                { "id": "tts-1", "owned_by": "openai" },
                { "id": "text-moderation-latest", "owned_by": "openai" }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.list_models().await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let models = result.unwrap();

        // Only gpt-4o must survive the filter.
        assert_eq!(
            models.len(),
            1,
            "Expected 1 chat model, got: {:?}",
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        assert_eq!(models[0].name, "gpt-4o");

        // Confirm no surviving model matches the non-chat filter.
        for model in &models {
            assert!(
                !is_non_chat_model(&model.name),
                "Non-chat model slipped through: {}",
                model.name
            );
        }
    }

    #[tokio::test]
    async fn test_get_model_info_direct_hit_returns_model_info() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models/gpt-4-turbo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "gpt-4-turbo",
                "owned_by": "openai"
            })))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.get_model_info("gpt-4-turbo").await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());

        let info = result.unwrap();
        assert_eq!(info.name, "gpt-4-turbo");
        assert!(
            info.supports_capability(ModelCapability::Streaming),
            "Expected Streaming capability"
        );
        assert!(
            info.supports_capability(ModelCapability::FunctionCalling),
            "Expected FunctionCalling capability"
        );
    }

    #[tokio::test]
    async fn test_get_model_info_falls_back_to_list_on_404() {
        let server = MockServer::start().await;

        // Direct GET returns 404 for this model.
        Mock::given(method("GET"))
            .and(path("/models/test-model"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        // List endpoint provides the model for the fallback scan.
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    { "id": "test-model", "owned_by": "openai" }
                ]
            })))
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.get_model_info("test-model").await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());

        let info = result.unwrap();
        assert_eq!(info.name, "test-model");
    }

    #[test]
    fn test_set_model_valid() {
        let config = OpenAIConfig::default();
        let mut provider = OpenAIProvider::new(config).unwrap();
        provider.set_model("gpt-4o");
        assert_eq!(provider.get_current_model(), "gpt-4o");
    }

    #[test]
    fn test_set_model_in_memory_setter() {
        // set_model no longer validates against the model list; it is an
        // infallible in-memory setter. Validation is the caller's responsibility
        // via list_models.
        let config = OpenAIConfig::default();
        let mut provider = OpenAIProvider::new(config).unwrap();
        provider.set_model("nonexistent-model");
        assert_eq!(provider.get_current_model(), "nonexistent-model");
    }

    #[tokio::test]
    async fn test_complete_with_tools_uses_non_streaming_path() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("\"stream\":false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_completion_body()))
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: true, // streaming enabled but tools force non-streaming
        };
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];
        let tools = vec![json!({
            "name": "read_file",
            "description": "Read a file",
            "parameters": { "type": "object", "properties": {} }
        })];

        let result = provider.complete(&messages, &tools).await;
        assert!(
            result.is_ok(),
            "Expected non-streaming path to succeed, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_bearer_token_sent_in_header() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_completion_body()))
            .expect(1)
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let result = provider.complete(&messages, &[]).await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_no_auth_header_when_api_key_empty() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_completion_body()))
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: String::new(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: None,
            enable_streaming: false,
        };
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let _ = provider.complete(&messages, &[]).await.unwrap();

        let requests = server.received_requests().await.unwrap_or_default();
        assert!(!requests.is_empty(), "Expected at least one request");
        let req = &requests[0];
        let has_auth = req
            .headers
            .iter()
            .any(|(k, _)| k.as_str() == "authorization");
        assert!(
            !has_auth,
            "Authorization header must be absent when api_key is empty"
        );
    }

    #[tokio::test]
    async fn test_org_header_sent_when_set() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("openai-organization", "myorg"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_completion_body()))
            .expect(1)
            .mount(&server)
            .await;

        let config = OpenAIConfig {
            api_key: "test-key".to_string(),
            base_url: server.uri(),
            model: "gpt-4o-mini".to_string(),
            organization_id: Some("myorg".to_string()),
            enable_streaming: false,
        };
        let provider = OpenAIProvider::new(config).unwrap();
        let messages = vec![Message::user("Hello")];

        let result = provider.complete(&messages, &[]).await;
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_list_models_cache_hit() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(models_list_body()))
            .expect(1)
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let provider = OpenAIProvider::new(config).unwrap();

        let first = provider.list_models().await.unwrap();
        let second = provider.list_models().await.unwrap();

        assert_eq!(first.len(), second.len());
        assert_eq!(first[0].name, second[0].name);
    }
}
