//! Ollama provider implementation for XZatoma
//!
//! This module implements the Provider trait for Ollama, connecting to a local
//! or remote Ollama server to generate completions with tool calling support.
//! Includes model listing, model switching, and token usage tracking.

use crate::config::OllamaConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::cache::{ModelCache, is_cache_valid, new_model_cache};
use crate::providers::{
    CompletionResponse, FunctionCall, Message, ModelCapability, ModelInfo, Provider,
    ProviderCapabilities, ProviderFunctionCall, ProviderMessage, ProviderTool, ProviderToolCall,
    TokenUsage, ToolCall, convert_tools_from_json, messages_contain_image_content,
    read_config_lock,
};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use url::Url;

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
///     request_timeout_seconds: 600,
///     ..Default::default()
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
    model_cache: ModelCache,
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
    // Required for JSON deserialization; digest is present in the API response
    // but not currently read by the model listing path.
    #[serde(default, rename = "digest")]
    _digest: String,
    #[serde(default)]
    modified_at: String,
}

/// Response from Ollama's /api/show endpoint
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    model_info: serde_json::Value,
    // Required for JSON deserialization; parameters and template are returned
    // by /api/show but not currently consumed by the provider.
    #[serde(default, rename = "parameters")]
    _parameters: String,
    #[serde(default, rename = "template")]
    _template: String,
    #[serde(default)]
    details: OllamaModelDetails,
    #[serde(default)]
    capabilities: Vec<String>,
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

/// Shared type aliases for Ollama's wire format.
///
/// Ollama's JSON schema for requests and responses is structurally identical
/// to the canonical shared types defined in `providers`.  These aliases
/// keep internal code readable without duplicating struct definitions.
type OllamaMessage = ProviderMessage;
type OllamaToolCall = ProviderToolCall;
type OllamaFunctionCall = ProviderFunctionCall;

/// Optional Ollama model parameters sent in the `options` field.
///
/// When all fields are `None`, the entire `options` object is omitted from the
/// request body; Ollama then uses its model defaults.
#[derive(Debug, Serialize)]
struct OllamaOptions {
    /// Context window size in tokens.
    ///
    /// When `Some(n)`, sets `num_ctx` in the Ollama request options.
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

/// Ollama chat completion request body including optional model parameters.
///
/// Used instead of [`OllamaRequest`] (which is [`ProviderRequest`]) whenever
/// the config contains model options such as `num_ctx`. The `options` key is
/// omitted from the serialized body when no options are set, so Ollama falls
/// back to its built-in model defaults.
#[derive(Debug, Serialize)]
struct OllamaRequestFull {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ProviderTool>,
    stream: bool,
    /// Optional Ollama model options (e.g., `num_ctx`). Omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
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
    // Required for JSON deserialization; total_duration is returned by the API
    // but only prompt_eval_count and eval_count are used for token tracking.
    #[serde(default, rename = "total_duration")]
    _total_duration: u64,
}

/// A streaming chunk that carries only an error message.
///
/// Ollama emits this structure instead of the normal
/// `{"message":{...},"done":...}` shape when it encounters a fatal error during
/// generation -- for example when the prompt exceeds the model's context
/// window, or when the model runs out of memory allocating the KV cache.
#[derive(Debug, Deserialize)]
struct OllamaStreamError {
    error: String,
}

/// Message within a single streaming chunk from the Ollama `/api/chat` endpoint.
///
/// This is a dedicated type rather than a re-use of `ProviderMessage`/`OllamaMessage`
/// because some models (e.g. Gemma 4 native-thinking variants) populate a
/// separate `thinking` field for chain-of-thought tokens instead of embedding
/// `<think>` markers inside `content`. Adding `thinking` to the shared
/// `ProviderMessage` would pollute the OpenAI and Copilot provider paths.
#[derive(Debug, Deserialize, Default)]
struct OllamaStreamMessage {
    #[serde(default, rename = "role")]
    _role: String,
    #[serde(default)]
    content: String,
    /// Chain-of-thought tokens from models that use a dedicated `thinking`
    /// wire field (e.g. Gemma 4). Empty for DeepSeek-R1 / Qwen3, which
    /// embed reasoning inside `content` using `<think>` tags instead.
    #[serde(default)]
    thinking: String,
    /// Tool calls requested by the model in this streaming chunk.
    ///
    /// Ollama emits tool calls on a non-`done` chunk with `content: ""` when
    /// the model decides to invoke a tool. They must be read here (not from
    /// the `done: true` chunk) so that the streaming path returns a proper
    /// tool-call response rather than an empty-content assistant message.
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

/// A single chunk from the Ollama `/api/chat` streaming response.
///
/// Replaces the non-streaming `OllamaResponse` in the streaming parse loop so
/// that the `thinking` field carried by native-thinking models is visible.
/// `OllamaResponse` is kept unchanged for the synchronous `complete` path.
#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: OllamaStreamMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: usize,
    #[serde(default)]
    eval_count: usize,
    #[serde(default, rename = "total_duration")]
    _total_duration: u64,
}

impl OllamaProvider {
    /// Create a new Ollama provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Ollama configuration containing host, model, and request timeout.
    ///   The HTTP client timeout is set to `config.request_timeout_seconds`.
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
    ///     request_timeout_seconds: 600,
    ///     ..Default::default()
    /// };
    /// let provider = OllamaProvider::new(config);
    /// assert!(provider.is_ok());
    /// ```
    pub fn new(mut config: OllamaConfig) -> Result<Self> {
        config.host =
            crate::security::validate_provider_base_url(&config.host, "provider.ollama.host")
                .map_err(|error| XzatomaError::Provider(error.to_string()))?;

        // Normalize "localhost" to the IPv4 literal "127.0.0.1".
        //
        // On macOS and Linux, `localhost` resolves via DNS to both `::1`
        // (IPv6) and `127.0.0.1` (IPv4).  Ollama binds to `127.0.0.1` only
        // by default, so any IPv6 connection attempt is immediately refused.
        // For a fresh connection Happy Eyeballs recovers (IPv6 ECONNREFUSED
        // triggers an immediate IPv4 fallback), but a stale connection-pool
        // entry for a POST request is never retried by hyper because POST is
        // non-idempotent.  Rewriting the hostname to the IPv4 literal bypasses
        // the DNS dual-stack path entirely so the HTTP client always targets
        // 127.0.0.1 directly.
        config.host = normalize_localhost_to_ipv4(&config.host);

        // Use a short connect timeout so that any remaining IPv6 attempt does
        // not block the fallback to IPv4 for longer than necessary.
        //
        // pool_idle_timeout ensures that connections idle for more than 90 s
        // are dropped before they can go stale; this prevents the scenario
        // where the Zed session is created (model-listing requests pool a
        // connection) and the user types the first prompt minutes later,
        // by which time Ollama may have closed the server-side socket.
        //
        // tcp_keepalive causes the OS to send periodic keep-alive probes so
        // that a half-open connection is detected and evicted from the pool
        // before it is mistakenly reused.
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_keepalive(Duration::from_secs(30))
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
            model_cache: new_model_cache(),
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
    ///     request_timeout_seconds: 600,
    ///     ..Default::default()
    /// };
    /// let provider = OllamaProvider::new(config).unwrap();
    /// // "localhost" is normalised to the IPv4 literal to avoid Happy-Eyeballs
    /// // issues when Ollama only binds to 127.0.0.1.
    /// assert_eq!(provider.host(), "http://127.0.0.1:11434");
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
    ///     request_timeout_seconds: 600,
    ///     ..Default::default()
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

    /// Convert XZatoma messages to Ollama wire format.
    ///
    /// Uses the shared [`ProviderMessage`] type (aliased as `OllamaMessage`)
    /// which maps directly to Ollama's JSON schema.  `tool_call_id` is set to
    /// `None` because Ollama does not use that field.
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        let validated_messages = crate::providers::validate_message_sequence(messages);
        validated_messages
            .iter()
            .filter_map(|m| {
                // Skip messages without content, images, or tool calls.
                if m.content.is_none() && !m.has_multimodal_content() && m.tool_calls.is_none() {
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

                let mut message = ProviderMessage::from_message_for_ollama(m);
                message.tool_calls = tool_calls;
                // Ollama does not use tool_call_id in messages.
                message.tool_call_id = None;

                Some(message)
            })
            .collect()
    }

    /// Convert tool schemas to Ollama wire format.
    ///
    /// Delegates to the shared [`convert_tools_from_json`] helper which
    /// replaces the formerly duplicated implementation in this module.
    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<crate::providers::ProviderTool> {
        convert_tools_from_json(tools)
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
        let host = read_config_lock(&self.config)?.host.clone();

        let url = format!("{}/api/tags", host);
        tracing::debug!("Fetching models from Ollama: {}", url);

        let response = self.client.get(&url).send().await.map_err(|source| {
            tracing::warn!("Failed to fetch Ollama models: {}", source);
            XzatomaError::ProviderHttpRequest {
                provider: "ollama".to_string(),
                endpoint: "api/tags".to_string(),
                source: source.into(),
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = crate::providers::http::redacted_body(response).await;
            tracing::error!("Ollama returned error {}: {}", status, error_text);
            return Err(crate::providers::http::provider_http_status(
                "ollama", "api/tags", status, error_text,
            ));
        }

        let ollama_response: OllamaTagsResponse = response.json().await.map_err(|source| {
            tracing::error!("Failed to parse Ollama tags response: {}", source);
            XzatomaError::ProviderResponseParse {
                provider: "ollama".to_string(),
                endpoint: "api/tags".to_string(),
                source: source.into(),
            }
        })?;

        // Try to fetch richer model details for each tag via /api/show where possible.
        // If fetching details fails for a model, fall back to tag-based heuristics.
        let mut models: Vec<ModelInfo> = Vec::new();
        for tag in ollama_response.models.into_iter() {
            match self.fetch_model_details(&tag.name).await {
                Ok(mut detailed_model) => {
                    // Ensure display name includes size reported by tags
                    detailed_model.display_name =
                        format!("{} ({})", detailed_model.name, format_size(tag.size));
                    detailed_model.set_provider_metadata("size", format_size(tag.size));
                    detailed_model.set_provider_metadata("modified_at", tag.modified_at.clone());
                    models.push(detailed_model);
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to fetch Ollama model details for {}: {}; falling back to tag data",
                        tag.name,
                        err
                    );
                    let family = tag.name.split(':').next().unwrap_or(&tag.name);
                    let mut model_info = ModelInfo::new(
                        &tag.name,
                        format!("{} ({})", tag.name, format_size(tag.size)),
                        get_context_window_for_model(&tag.name),
                    );
                    add_model_capabilities(&mut model_info, family);
                    models.push(model_info);
                }
            }
        }

        tracing::debug!("Fetched {} models from Ollama", models.len());
        Ok(models)
    }

    /// Complete a conversation using Ollama streaming with per-chunk callbacks.
    ///
    /// Sends the request with `stream: true`, parses newline-delimited JSON
    /// chunks, and calls the appropriate callback for each partial content
    /// token. Think-tag models (DeepSeek-R1, Qwen3) that embed `<think>` and
    /// `</think>` markers in their output are handled by a simple state machine:
    /// content between the tags is routed to `on_reasoning_chunk` and content
    /// outside the tags is routed to `on_content_chunk`.
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools (as JSON schemas)
    /// * `on_reasoning_chunk` - Optional callback for incremental reasoning tokens
    /// * `on_content_chunk` - Optional callback for incremental content tokens
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError` if the HTTP request fails or a stream parse error
    /// occurs.
    async fn complete_streaming_with_callbacks(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
        on_reasoning_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
        on_content_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
    ) -> Result<CompletionResponse> {
        use futures::StreamExt;

        let (url, model, stream_idle_timeout_secs, num_ctx) = {
            let config = read_config_lock(&self.config)?;
            (
                format!("{}/api/chat", config.host),
                config.model.clone(),
                config.stream_idle_timeout_seconds,
                config.num_ctx,
            )
        };

        if messages_contain_image_content(messages) && !self.model_has_vision_capability(&model) {
            return Err(XzatomaError::Provider(format!(
                "Ollama model '{}' does not support image input",
                model
            )));
        }

        let options = num_ctx.map(|n| OllamaOptions { num_ctx: Some(n) });
        let ollama_request = OllamaRequestFull {
            model,
            messages: self.convert_messages(messages),
            tools: self.convert_tools(tools),
            stream: true,
            options,
        };

        let response = send_post_with_retry(&self.client, &url, &ollama_request)
            .await
            .map_err(|source| XzatomaError::ProviderHttpRequest {
                provider: "ollama".to_string(),
                endpoint: "api/chat:stream".to_string(),
                source: source.into(),
            })?;

        let response =
            crate::providers::http::check_response(response, "ollama", "api/chat:stream").await?;

        let mut stream = response.bytes_stream();
        let mut buffer = crate::providers::streaming::LineBuffer::new();
        let mut content_acc = String::new();
        let mut reasoning_acc = String::new();
        let mut tool_calls_acc: Vec<OllamaToolCall> = Vec::new();
        let mut in_think_block = false;
        let mut prompt_eval_count = 0usize;
        let mut eval_count = 0usize;

        let idle_duration = Duration::from_secs(stream_idle_timeout_secs);

        // The 'stream label lets inner arms break out of the outer loop so
        // that partial content accumulated before any failure is preserved.
        'stream: loop {
            // Wrap each chunk read in an idle timeout so that a stalled or
            // OOM-killed Ollama is detected quickly with an actionable message
            // instead of hanging until the overall request_timeout fires.
            let chunk_result = match tokio::time::timeout(idle_duration, stream.next()).await {
                Ok(Some(r)) => r,
                Ok(None) => break 'stream, // stream ended normally
                Err(_elapsed) => {
                    // No bytes arrived within idle_duration.
                    let accumulated = content_acc.len() + reasoning_acc.len();
                    if accumulated > 0 {
                        tracing::warn!(
                            "Ollama stream idle after {} char(s) of content; \
                             returning partial response",
                            accumulated
                        );
                        break 'stream;
                    }
                    return Err(XzatomaError::Provider(format!(
                        "Ollama stream produced no output within {}s. \
                         The prompt may exceed the model's context window, \
                         or Ollama may be under memory pressure. \
                         Try a shorter prompt or a model with a larger context window \
                         (check: ollama show <model>).",
                        idle_duration.as_secs()
                    )));
                }
            };

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    // The response body was interrupted (Ollama OOM-crashed, was
                    // restarted, or the OS reset the TCP connection). Return
                    // partial content when available; otherwise surface a clear
                    // error with context-window / OOM guidance.
                    let accumulated = content_acc.len() + reasoning_acc.len();
                    if accumulated > 0 {
                        tracing::warn!(
                            "Ollama stream body error after {} char(s) of content; \
                             returning partial response: {}",
                            accumulated,
                            e
                        );
                        break 'stream;
                    }
                    tracing::debug!("Ollama stream body error before any content: {:?}", e);
                    return Err(XzatomaError::Provider(format!(
                        "Ollama stream failed before generating any content: {}. \
                         The prompt may exceed the model's context window, \
                         or Ollama may have run out of memory. \
                         Try a shorter prompt or a model with a larger context window \
                         (check: ollama show <model>).",
                        e
                    )));
                }
            };

            buffer.push_bytes(&chunk);

            while let Some(raw_line) = buffer.next_line() {
                let line = raw_line.trim().to_string();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<OllamaStreamChunk>(&line) {
                    Ok(chunk_resp) => {
                        if chunk_resp.done {
                            prompt_eval_count = chunk_resp.prompt_eval_count;
                            eval_count = chunk_resp.eval_count;
                            // Some Ollama versions place tool calls on the done chunk
                            // rather than on a preceding non-done chunk. Capture them
                            // here so the response assembly path sees all tool calls
                            // regardless of which chunk carried them.
                            if !chunk_resp.message.tool_calls.is_empty() {
                                tool_calls_acc.extend(chunk_resp.message.tool_calls);
                            }
                            break;
                        }

                        // Collect tool calls emitted on non-done chunks. Ollama
                        // sends tool calls as a single chunk with content: "",
                        // tool_calls: [...], done: false. If this chunk carries
                        // tool calls there will be no content to process, so
                        // skip content routing.
                        if !chunk_resp.message.tool_calls.is_empty() {
                            tool_calls_acc.extend(chunk_resp.message.tool_calls);
                            continue;
                        }

                        // Route native thinking-field tokens (Gemma 4 and similar
                        // models that separate chain-of-thought from response
                        // content at the wire level) to reasoning_acc.
                        let thinking_delta = chunk_resp.message.thinking;
                        if !thinking_delta.is_empty() {
                            if let Some(cb) = on_reasoning_chunk {
                                cb(thinking_delta.clone());
                            }
                            reasoning_acc.push_str(&thinking_delta);
                        }

                        // Route content-field tokens through the think-tag state
                        // machine for DeepSeek-R1 / Qwen3 models that embed
                        // reasoning via <think> markers inside content.
                        let delta = chunk_resp.message.content;
                        if !delta.is_empty() {
                            process_ollama_think_chunk(
                                &delta,
                                &mut in_think_block,
                                &mut content_acc,
                                &mut reasoning_acc,
                                on_reasoning_chunk,
                                on_content_chunk,
                            );
                        }
                    }
                    Err(_) => {
                        // Ollama sends {"error":"..."} as a streaming chunk when
                        // it encounters a fatal error (context window exceeded,
                        // out of memory, model not loaded, etc.). Surface this as
                        // a Provider error so the user sees the actual reason
                        // rather than a silent empty response.
                        if let Ok(err) = serde_json::from_str::<OllamaStreamError>(&line) {
                            return Err(XzatomaError::Provider(format!(
                                "Ollama error: {}",
                                err.error
                            )));
                        }
                        tracing::debug!("Failed to parse Ollama stream chunk (line: {:?})", line);
                    }
                }
            }
        }

        // Build the final response from accumulated content, reasoning, or tool calls.
        //
        // Tool calls take priority: if the model emitted any tool calls during the
        // stream they are returned as a tool-call message. Content and reasoning
        // accumulated alongside tool calls (Ollama sends content: "" for tool-call
        // chunks) are discarded. This restores the tool-calling path that was
        // silently dropped when supports_streaming() was set to true: the old code
        // only reached complete() (which calls convert_response_message) when no
        // streaming callbacks were present; now the streaming path handles it too.
        if !tool_calls_acc.is_empty() {
            tracing::debug!(
                "Ollama streaming response contains {} tool call(s); returning tool-call message",
                tool_calls_acc.len()
            );
            let converted_calls: Vec<ToolCall> = tool_calls_acc
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
            let message = Message::assistant_with_tools(converted_calls);
            let response = if prompt_eval_count > 0 || eval_count > 0 {
                CompletionResponse::with_usage(
                    message,
                    TokenUsage::new(prompt_eval_count, eval_count),
                )
            } else {
                CompletionResponse::new(message)
            };
            return Ok(response);
        }

        // Native-thinking models (e.g. Gemma 4) may spend their entire token
        // budget on chain-of-thought tokens emitted in the `thinking` field
        // while leaving `content` empty for the whole response. When that
        // happens, promote the reasoning to response content so the agent
        // receives usable output instead of the "empty response" error.
        // Clear final_reasoning in that case to avoid the Zed UI rendering
        // the same text twice (once as reasoning, once as content).
        let (final_content, final_reasoning) = if !content_acc.is_empty() {
            (content_acc, reasoning_acc)
        } else if !reasoning_acc.is_empty() {
            tracing::debug!(
                "Ollama response has {} reasoning char(s) but no explicit content; \
                 promoting thinking to response content",
                reasoning_acc.len()
            );
            (reasoning_acc, String::new())
        } else {
            (String::new(), String::new())
        };

        let message = Message::assistant(&final_content);

        let response = if prompt_eval_count > 0 || eval_count > 0 {
            CompletionResponse::with_usage(message, TokenUsage::new(prompt_eval_count, eval_count))
        } else {
            CompletionResponse::new(message)
        };

        let response = if !final_reasoning.is_empty() {
            response.set_reasoning(final_reasoning)
        } else {
            response
        };

        Ok(response)
    }

    /// Get model details from Ollama's /api/show endpoint
    async fn fetch_model_details(&self, model_name: &str) -> Result<ModelInfo> {
        let host = read_config_lock(&self.config)?.host.clone();

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
            .map_err(|source| {
                tracing::warn!("Failed to fetch Ollama model details: {}", source);
                XzatomaError::ProviderHttpRequest {
                    provider: "ollama".to_string(),
                    endpoint: "api/show".to_string(),
                    source: source.into(),
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = crate::providers::http::redacted_body(response).await;
            tracing::error!("Ollama returned error {}: {}", status, error_text);
            return Err(crate::providers::http::provider_http_status(
                "ollama",
                "api/show",
                status,
                format!("model={}: {}", model_name, error_text),
            ));
        }

        // Read the response body as text first so we can handle varying response shapes
        let body = response.text().await.map_err(|source| {
            tracing::error!("Failed to read Ollama show response body: {}", source);
            XzatomaError::ProviderHttpRequest {
                provider: "ollama".to_string(),
                endpoint: "api/show:body".to_string(),
                source: source.into(),
            }
        })?;

        let raw_json: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);

        let show_response: OllamaShowResponse = serde_json::from_str(&body).map_err(|source| {
            tracing::error!("Failed to parse Ollama show response: {}", source);
            XzatomaError::ProviderResponseParse {
                provider: "ollama".to_string(),
                endpoint: "api/show".to_string(),
                source: source.into(),
            }
        })?;

        // Use the name from the response when present; otherwise fall back to the requested model name
        let name = show_response
            .name
            .clone()
            .unwrap_or_else(|| model_name.to_string());

        if show_response.name.is_none() {
            tracing::debug!(
                "Ollama show response missing 'name' field, falling back to requested model name: {}",
                name
            );
        }

        let mut model_info =
            build_model_info_from_show_response(&show_response, &name, Some(raw_json));

        // Include reported parameter size and quantization in metadata if available
        if !show_response.details.parameter_size.is_empty() {
            model_info.set_provider_metadata(
                "parameter_size",
                show_response.details.parameter_size.clone(),
            );
        }
        if !show_response.details.quantization_level.is_empty() {
            model_info.set_provider_metadata(
                "quantization_level",
                show_response.details.quantization_level.clone(),
            );
        }

        Ok(model_info)
    }

    /// Returns whether the given Ollama model is known to support vision input.
    ///
    /// Consults the live model cache populated by [`fetch_models_from_api`] first.
    /// When the current model is present in the cache and its [`ModelInfo`] carries
    /// [`ModelCapability::Vision`] (set by `build_model_info_from_show_response` from
    /// the `/api/show` `capabilities` array), this returns `true` unconditionally.
    /// When the model is absent from the cache the function falls back to the static
    /// name-based allowlist in [`crate::providers::ollama_model_supports_vision`].
    ///
    /// This method is intentionally cheap and non-blocking: it only reads the shared
    /// `Arc<RwLock<...>>` cache and never makes a network call.
    ///
    /// # Arguments
    ///
    /// * `model` - The model name to check.
    ///
    /// # Returns
    ///
    /// Returns `true` if the model is confirmed to support vision input.
    fn model_has_vision_capability(&self, model: &str) -> bool {
        if let Ok(cache) = self.model_cache.read()
            && let Some((models, _)) = cache.as_ref()
            && let Some(info) = models.iter().find(|m| m.name == model)
        {
            return info.supports_capability(ModelCapability::Vision);
        }
        // Cache miss or lock failure: fall back to the static name-based allowlist.
        crate::providers::ollama_model_supports_vision(model)
    }
}

/// Apply a single streaming delta from Ollama to the state machine for
/// think-tag detection.
///
/// Splits the `delta` around `<think>` and `</think>` markers (or their
/// `<|thinking|>` / `<|/thinking|>` variants), routes each segment to the
/// appropriate callback, and accumulates into `content_acc` and
/// `reasoning_acc` for the final [`CompletionResponse`].
///
/// # Arguments
///
/// * `delta` - The incremental content from this streaming chunk
/// * `in_think_block` - Mutable flag tracking whether we are inside a think block
/// * `content_acc` - Accumulator for clean response content
/// * `reasoning_acc` - Accumulator for reasoning content
/// * `on_reasoning_chunk` - Optional callback for reasoning tokens
/// * `on_content_chunk` - Optional callback for content tokens
fn process_ollama_think_chunk(
    delta: &str,
    in_think_block: &mut bool,
    content_acc: &mut String,
    reasoning_acc: &mut String,
    on_reasoning_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
    on_content_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
) {
    // Opening tags
    const THINK_OPEN: &[&str] = &["<think>", "<|thinking|>"];
    // Closing tags
    const THINK_CLOSE: &[&str] = &["</think>", "<|/thinking|>"];

    let mut remaining = delta;

    while !remaining.is_empty() {
        if *in_think_block {
            // Look for a closing tag
            let close_pos = THINK_CLOSE
                .iter()
                .filter_map(|tag| remaining.find(tag).map(|p| (p, *tag)))
                .min_by_key(|(pos, _)| *pos);

            if let Some((pos, tag)) = close_pos {
                let before = &remaining[..pos];
                if !before.is_empty() {
                    reasoning_acc.push_str(before);
                    if let Some(cb) = on_reasoning_chunk {
                        cb(before.to_string());
                    }
                }
                *in_think_block = false;
                remaining = &remaining[pos + tag.len()..];
            } else {
                // Entire remaining delta is reasoning
                reasoning_acc.push_str(remaining);
                if let Some(cb) = on_reasoning_chunk {
                    cb(remaining.to_string());
                }
                remaining = "";
            }
        } else {
            // Look for an opening tag
            let open_pos = THINK_OPEN
                .iter()
                .filter_map(|tag| remaining.find(tag).map(|p| (p, *tag)))
                .min_by_key(|(pos, _)| *pos);

            if let Some((pos, tag)) = open_pos {
                let before = &remaining[..pos];
                if !before.is_empty() {
                    content_acc.push_str(before);
                    if let Some(cb) = on_content_chunk {
                        cb(before.to_string());
                    }
                }
                *in_think_block = true;
                remaining = &remaining[pos + tag.len()..];
            } else {
                // Entire remaining delta is content
                content_acc.push_str(remaining);
                if let Some(cb) = on_content_chunk {
                    cb(remaining.to_string());
                }
                remaining = "";
            }
        }
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

/// Add model capabilities based on model family and model name.
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
        _ if crate::providers::ollama_model_supports_vision(&model.name) => {
            model.add_capability(ModelCapability::Vision);
        }
        "codellama" | "codegemma" | "deepseek-coder" | "starcoder" | "starcoder2" | "codestral"
        | "qwen2.5-coder" => {
            model.add_capability(ModelCapability::CodeGeneration);
        }
        _ => {}
    }
}

/// Build a `ModelInfo` from an Ollama show response, falling back to the requested
/// model name when the response does not include a `name` field.
fn build_model_info_from_show_response(
    show: &OllamaShowResponse,
    requested_name: &str,
    raw_json: Option<serde_json::Value>,
) -> ModelInfo {
    let name = show
        .name
        .clone()
        .unwrap_or_else(|| requested_name.to_string());
    let display_name = name.clone();

    // Start with the heuristic but prefer explicit values from the show response
    let mut context_window = get_context_window_for_model(&name);

    if let Some(obj) = show.model_info.as_object() {
        // Prefer architecture-specific context length (e.g., "granite.context_length")
        if let Some(arch) = obj.get("general.architecture").and_then(|v| v.as_str()) {
            let key = format!("{}.context_length", arch);
            if let Some(val) = obj.get(&key).and_then(|v| v.as_u64()) {
                context_window = val as usize;
            } else if let Some(val) = obj.get("context_length").and_then(|v| v.as_u64()) {
                context_window = val as usize;
            } else {
                // Fallback: find any field that ends with 'context_length'
                for (k, v) in obj.iter() {
                    if k.ends_with("context_length")
                        && let Some(val) = v.as_u64()
                    {
                        context_window = val as usize;
                        break;
                    }
                }
            }
        }
    }

    let mut model_info = ModelInfo::new(&name, &display_name, context_window);

    // Map explicit capabilities from the show response into our ModelCapability flags
    if !show.capabilities.is_empty() {
        // keep the raw list for inspection
        let caps_joined = show.capabilities.join(", ");
        model_info.set_provider_metadata("capabilities", caps_joined.clone());

        for cap in &show.capabilities {
            match cap.to_lowercase().as_str() {
                "tools" => model_info.add_capability(ModelCapability::FunctionCalling),
                "vision" => model_info.add_capability(ModelCapability::Vision),
                "streaming" => model_info.add_capability(ModelCapability::Streaming),
                "long_context" | "longcontext" | "long-context" => {
                    model_info.add_capability(ModelCapability::LongContext)
                }
                "json" | "json_mode" | "json-mode" | "completion" => {
                    // Retained in provider metadata; no active ModelCapability variant
                    // is assigned for these legacy provider strings.
                }
                _ => {
                    // Unknown capability: preserve via provider metadata (already added)
                }
            }
        }
    }

    // Prefer the family reported in details if present, otherwise derive from the name
    let family = if !show.details.family.is_empty() {
        show.details.family.clone()
    } else {
        name.split(':').next().unwrap_or(&name).to_string()
    };

    // Add family-based heuristics as a fallback
    add_model_capabilities(&mut model_info, &family);

    // Record some helpful provider-specific metadata
    if let Some(arch) = show
        .model_info
        .get("general.architecture")
        .and_then(|v| v.as_str())
    {
        model_info.set_provider_metadata("architecture", arch);
    }
    if !show.details.parameter_size.is_empty() {
        model_info.set_provider_metadata("parameter_size", show.details.parameter_size.clone());
    }
    if !show.details.quantization_level.is_empty() {
        model_info.set_provider_metadata(
            "quantization_level",
            show.details.quantization_level.clone(),
        );
    }

    model_info.raw_data = raw_json;

    model_info
}

/// Replace the `localhost` hostname in a URL with the IPv4 loopback
/// address `127.0.0.1`.
///
/// On macOS and Linux `localhost` typically resolves via DNS to both `::1`
/// (IPv6) and `127.0.0.1` (IPv4). reqwest's Happy Eyeballs algorithm tries
/// the IPv6 address first. Ollama binds to `127.0.0.1` only by default, so
/// the IPv6 attempt is always refused. For new connections Happy Eyeballs
/// falls back to IPv4, but for stale pool entries on non-idempotent POST
/// requests hyper surfaces the failure as "error sending request" without
/// retrying.
///
/// By rewriting `localhost` to the IPv4 literal here the HTTP client never
/// performs a dual-stack DNS lookup and always connects directly to
/// `127.0.0.1`.
///
/// Only the bare hostname `localhost` (case-insensitive) is rewritten.
/// Explicit IP literals such as `127.0.0.1` or `[::1]`, and remote
/// hostnames, are passed through unchanged.
fn normalize_localhost_to_ipv4(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };
    if parsed
        .host_str()
        .is_some_and(|h| h.eq_ignore_ascii_case("localhost"))
    {
        // set_host is infallible for well-formed IPv4 literals; fall back
        // to the original string on the unexpected error path.
        if parsed.set_host(Some("127.0.0.1")).is_ok() {
            return parsed.to_string().trim_end_matches('/').to_string();
        }
    }
    url.to_string()
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

/// Send a JSON POST request with a single retry on connection-level failures.
///
/// Non-idempotent POST requests are never automatically retried by hyper, so
/// stale connection-pool entries surface as "error sending request" failures.
/// This helper retries exactly once when [`reqwest::Error::is_connect`] or
/// [`reqwest::Error::is_request`] is true, allowing the pool to discard the
/// broken socket and open a fresh connection on the second attempt.  Timeout
/// errors are not retried because a second attempt would also time out.
///
/// # Arguments
///
/// * `client` - The HTTP client to use for both attempts.
/// * `url`    - The URL to POST to.
/// * `body`   - A serializable value that is encoded as JSON for both attempts.
///
/// # Returns
///
/// The [`reqwest::Response`] from the first successful send.
///
/// # Errors
///
/// Returns the [`reqwest::Error`] from the second attempt if both fail.
async fn send_post_with_retry<T: Serialize>(
    client: &Client,
    url: &str,
    body: &T,
) -> reqwest::Result<reqwest::Response> {
    match client.post(url).json(body).send().await {
        Ok(r) => Ok(r),
        Err(e) if (e.is_connect() || e.is_request()) && !e.is_timeout() => {
            tracing::warn!(
                "Ollama POST to {} failed with a connection error, retrying once \
                 to discard a stale connection-pool entry: {}",
                url,
                e
            );
            client.post(url).json(body).send().await
        }
        Err(e) => Err(e),
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let (url, model, num_ctx) = {
            let config = read_config_lock(&self.config)?;
            (
                format!("{}/api/chat", config.host),
                config.model.clone(),
                config.num_ctx,
            )
        };

        if messages_contain_image_content(messages) && !self.model_has_vision_capability(&model) {
            return Err(XzatomaError::Provider(format!(
                "Ollama model '{}' does not support image input",
                model
            )));
        }

        let options = num_ctx.map(|n| OllamaOptions { num_ctx: Some(n) });
        let ollama_request = OllamaRequestFull {
            model,
            messages: self.convert_messages(messages),
            tools: self.convert_tools(tools),
            stream: false,
            options,
        };

        tracing::debug!(
            "Sending Ollama request: {} messages, {} tools",
            ollama_request.messages.len(),
            ollama_request.tools.len()
        );

        let response = send_post_with_retry(&self.client, &url, &ollama_request)
            .await
            .map_err(|source| XzatomaError::ProviderHttpRequest {
                provider: "ollama".to_string(),
                endpoint: "api/chat".to_string(),
                source: source.into(),
            })?;

        let response =
            crate::providers::http::check_response(response, "ollama", "api/chat").await?;

        let ollama_response: OllamaResponse = response.json().await.map_err(|source| {
            tracing::error!("Failed to parse Ollama response: {}", source);
            XzatomaError::ProviderResponseParse {
                provider: "ollama".to_string(),
                endpoint: "api/chat".to_string(),
                source: source.into(),
            }
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

    /// Returns `true` if this provider has valid stored credentials.
    ///
    /// Ollama does not require authentication; this always returns `true`.
    fn is_authenticated(&self) -> bool {
        true
    }

    /// Returns a borrowed reference to the currently active model name, or
    /// `None` if no model is configured.
    ///
    /// The model name is stored behind a `RwLock`; a borrowed reference cannot
    /// be returned directly. Use `get_current_model` for an owned copy.
    fn current_model(&self) -> Option<&str> {
        None
    }

    /// Fetch the list of available models from the remote API. This is the
    /// canonical implementation method; `list_models` provides a default that
    /// delegates here.
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails.
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        self.fetch_models_from_api().await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        tracing::debug!("Listing Ollama models");

        // Check cache first
        if let Ok(cache) = self.model_cache.read()
            && let Some((models, cached_at)) = cache.as_ref()
            && is_cache_valid(*cached_at)
        {
            tracing::debug!("Using cached model list");
            return Ok(models.clone());
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
        if let Ok(cache) = self.model_cache.read()
            && let Some((models, cached_at)) = cache.as_ref()
            && is_cache_valid(*cached_at)
            && let Some(model) = models.iter().find(|m| m.name == model_name)
        {
            return Ok(model.clone());
        }

        // Not in cache, fetch from API
        self.fetch_model_details(model_name).await
    }

    /// Get the name of the currently active model.
    ///
    /// # Returns
    ///
    /// Returns the model name as an owned `String`, or `"none"` if the read
    /// lock cannot be acquired.
    fn get_current_model(&self) -> String {
        self.config
            .read()
            .map(|c| c.model.clone())
            .unwrap_or_else(|_| "none".to_string())
    }

    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: true,
            supports_model_switching: true,
            supports_token_counts: true,
            supports_streaming: false,
            supports_vision: true,
        }
    }

    /// Returns whether the given model name supports vision (image) input.
    ///
    /// Consults the live model cache (populated by `list_models` at session startup)
    /// before falling back to the static name-based allowlist. This ensures that any
    /// Ollama model reporting `"vision"` in its `/api/show` `capabilities` array is
    /// accepted even when its name does not appear on the static allowlist.
    fn model_supports_vision(&self, _provider_name: &str, model_name: &str) -> bool {
        self.model_has_vision_capability(model_name)
    }

    /// Returns `true` because Ollama supports streaming completions.
    ///
    /// When `complete_with_callbacks` is called with at least one active
    /// callback, the provider uses the Ollama streaming API (`stream: true`)
    /// to deliver per-chunk events.
    fn supports_streaming(&self) -> bool {
        true
    }

    /// Set the active model in memory without any API validation. Callers
    /// that need model-existence validation should call `list_models` before
    /// calling this method.
    fn set_model(&mut self, model: &str) {
        if let Ok(mut config) = self.config.write() {
            config.model = model.to_string();
        }
    }

    fn set_model_inplace(&self, model: &str) {
        if let Ok(mut config) = self.config.write() {
            config.model = model.to_string();
        }
    }

    /// Complete a conversation with per-chunk streaming callbacks.
    ///
    /// Enables Ollama streaming and calls `on_content_chunk` for each content
    /// token. For models that emit `<think>` or `<|thinking|>` tags, those
    /// tokens are routed to `on_reasoning_chunk` instead.
    ///
    /// Falls back to `complete` when neither callback is provided.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if the HTTP request fails or a stream
    /// parse error occurs.
    async fn complete_with_callbacks(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
        on_reasoning_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
        on_content_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
    ) -> Result<CompletionResponse> {
        if on_reasoning_chunk.is_none() && on_content_chunk.is_none() {
            return self.complete(messages, tools).await;
        }
        self.complete_streaming_with_callbacks(
            messages,
            tools,
            on_reasoning_chunk,
            on_content_chunk,
        )
        .await
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
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_ollama_provider_host() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        // "localhost" is normalised to 127.0.0.1 to avoid dual-stack DNS.
        assert_eq!(provider.host(), "http://127.0.0.1:11434");
    }

    #[test]
    fn test_ollama_provider_normalizes_trailing_slash_host() {
        let config = OllamaConfig {
            host: "http://localhost:11434/".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        // Both the trailing slash and "localhost" are normalised.
        assert_eq!(provider.host(), "http://127.0.0.1:11434");
    }

    #[test]
    fn test_ollama_provider_normalizes_localhost_to_ipv4() {
        // Any casing of "localhost" is rewritten to 127.0.0.1.
        for host in &[
            "http://localhost:11434",
            "http://LOCALHOST:11434",
            "http://Localhost:11434",
        ] {
            let config = OllamaConfig {
                host: host.to_string(),
                model: "llama3.2:latest".to_string(),
                request_timeout_seconds: 600,
                stream_idle_timeout_seconds: 120,
                num_ctx: None,
            };
            let provider = OllamaProvider::new(config).unwrap();
            assert_eq!(
                provider.host(),
                "http://127.0.0.1:11434",
                "expected localhost normalisation for input {}",
                host
            );
        }
    }

    #[test]
    fn test_ollama_provider_does_not_normalise_explicit_ipv4() {
        // An explicit 127.0.0.1 address must not be altered.
        let config = OllamaConfig {
            host: "http://127.0.0.1:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.host(), "http://127.0.0.1:11434");
    }

    #[test]
    fn test_ollama_provider_rejects_host_with_query() {
        let config = OllamaConfig {
            host: "http://localhost:11434?token=secret".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let result = OllamaProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_ollama_provider_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.model(), "llama3.2:latest");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
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
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
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
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
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
    fn test_convert_messages_with_multimodal_image_sets_native_images() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llava:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        let message = Message::try_user_from_multimodal_input(
            crate::providers::MultimodalPromptInput::new(vec![
                crate::providers::PromptInputPart::text("describe"),
                crate::providers::PromptInputPart::image(
                    crate::providers::ImagePromptPart::inline_base64("image/png", "AAAA"),
                ),
                crate::providers::PromptInputPart::text("briefly"),
            ]),
        )
        .unwrap();

        let converted = provider.convert_messages(&[message]);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content, "describe\n\nbriefly");
        assert_eq!(converted[0].images, vec!["AAAA".to_string()]);
        assert!(converted[0].content_parts.is_some());
        assert!(converted[0].tool_calls.is_none());
    }

    #[test]
    fn test_convert_messages_with_text_only_multimodal_omits_native_images() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        let message = Message::try_user_from_multimodal_input(
            crate::providers::MultimodalPromptInput::new(vec![
                crate::providers::PromptInputPart::text("first"),
                crate::providers::PromptInputPart::text("second"),
            ]),
        )
        .unwrap();

        let converted = provider.convert_messages(&[message]);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content, "first\n\nsecond");
        assert!(converted[0].images.is_empty());
        assert!(converted[0].content_parts.is_some());
    }

    #[test]
    fn test_ollama_model_supports_vision_allowlist() {
        assert!(crate::providers::ollama_model_supports_vision(
            "llava:latest"
        ));
        assert!(crate::providers::ollama_model_supports_vision("gemma3:12b"));
        assert!(!crate::providers::ollama_model_supports_vision(
            "llama3.2:latest"
        ));
    }

    #[test]
    fn test_model_has_vision_capability_uses_cache_when_populated() {
        use std::time::Instant;
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "mymodel:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

        // Seed the model cache with a ModelInfo that has Vision capability.
        let mut info = ModelInfo::new("mymodel:latest", "MyModel", 4096);
        info.add_capability(ModelCapability::Vision);
        *provider.model_cache.write().unwrap() = Some((vec![info], Instant::now()));

        // Should return true from cache even though the name is not on the static allowlist.
        assert!(provider.model_has_vision_capability("mymodel:latest"));
    }

    #[test]
    fn test_model_has_vision_capability_falls_back_to_static_on_cache_miss() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llava:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        // Cache is empty; should fall back to static allowlist.
        assert!(provider.model_has_vision_capability("llava:latest"));
        assert!(!provider.model_has_vision_capability("llama3.2:latest"));
    }

    #[test]
    fn test_convert_response_message_text() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:latest".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

        let ollama_msg = OllamaMessage {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            content_parts: None,
            images: vec![],
            tool_calls: None,
            tool_call_id: None,
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
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

        let ollama_msg = OllamaMessage {
            role: "assistant".to_string(),
            content: String::new(),
            content_parts: None,
            images: vec![],
            tool_calls: Some(vec![OllamaToolCall {
                id: "call_123".to_string(),
                r#type: "function".to_string(),
                function: OllamaFunctionCall {
                    name: "test_tool".to_string(),
                    arguments: serde_json::json!({"key": "value"}),
                },
            }]),
            tool_call_id: None,
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
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

        let messages = vec![
            Message {
                role: "user".to_string(),
                content: None,
                content_parts: None,
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
    fn test_add_model_capabilities_code_generation() {
        let families = [
            "codellama",
            "codegemma",
            "deepseek-coder",
            "starcoder",
            "starcoder2",
            "codestral",
            "qwen2.5-coder",
        ];
        for family in &families {
            let mut model = ModelInfo::new("test", "test", 4096);
            add_model_capabilities(&mut model, family);
            assert!(
                model.supports_capability(ModelCapability::CodeGeneration),
                "Expected CodeGeneration for family: {}",
                family
            );
        }
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
        assert!(is_cache_valid(instant));
    }

    #[test]
    fn test_is_cache_valid_expired() {
        let instant = Instant::now() - Duration::from_secs(400);
        assert!(!is_cache_valid(instant));
    }

    #[test]
    fn test_provider_capabilities() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        let capabilities = provider.get_provider_capabilities();

        assert!(capabilities.supports_model_listing);
        assert!(capabilities.supports_model_details);
        assert!(capabilities.supports_model_switching);
        assert!(capabilities.supports_token_counts);
        assert!(!capabilities.supports_streaming);
        assert!(capabilities.supports_vision);
    }

    #[test]
    fn test_get_current_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test-model".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model(), "test-model");
    }

    #[test]
    fn test_parse_show_response_missing_name() {
        let json = r#"{
            "model_info": { "description": "Test model" },
            "parameters": "",
            "template": "",
            "details": { "parameter_size": "", "quantization_level": "", "family": "granite4" }
        }"#;

        let show: OllamaShowResponse = serde_json::from_str(json).unwrap();
        assert!(show.name.is_none());
        assert_eq!(show.details.family, "granite4");
    }

    #[test]
    fn test_build_model_info_from_show_response_missing_name() {
        let show = OllamaShowResponse {
            name: None,
            model_info: serde_json::json!({"description": "Test model"}),
            _parameters: String::new(),
            _template: String::new(),
            details: OllamaModelDetails {
                parameter_size: String::new(),
                quantization_level: String::new(),
                family: "granite4".to_string(),
            },
            capabilities: Vec::new(),
        };

        let model_info = build_model_info_from_show_response(&show, "granite4:latest", None);
        assert_eq!(model_info.name, "granite4:latest");
        assert_eq!(model_info.display_name, "granite4:latest");
        assert!(model_info.supports_capability(ModelCapability::FunctionCalling));
    }

    #[test]
    fn test_build_model_info_from_show_response_parses_context_and_capabilities() {
        let json = r#"{
            "name": "granite4:latest",
            "model_info": {
                "general.architecture": "granite",
                "granite.context_length": 131072
            },
            "capabilities": ["completion", "tools"],
            "parameters": "",
            "template": "",
            "details": { "parameter_size": "3.4B", "quantization_level": "Q4_K", "family": "granite" }
        }"#;

        let show: OllamaShowResponse = serde_json::from_str(json).unwrap();
        let model_info = build_model_info_from_show_response(
            &show,
            "granite4:latest",
            Some(serde_json::from_str(json).unwrap()),
        );
        assert_eq!(model_info.context_window, 131072);
        assert!(model_info.supports_capability(ModelCapability::FunctionCalling));
        assert_eq!(
            model_info.provider_specific.get("capabilities").unwrap(),
            "completion, tools"
        );
        assert_eq!(
            model_info.provider_specific.get("architecture").unwrap(),
            "granite"
        );
        assert_eq!(
            model_info.provider_specific.get("parameter_size").unwrap(),
            "3.4B"
        );
    }

    #[test]
    fn test_convert_messages_drops_orphan_tool() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

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
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "test".to_string(),
            request_timeout_seconds: 600,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        };
        let provider = OllamaProvider::new(config).unwrap();

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
        assert_eq!(converted[2].content, "Result");
    }

    #[test]
    fn test_process_ollama_think_chunk_routes_content_outside_think_tags() {
        let mut in_think_block = false;
        let mut content_acc = String::new();
        let mut reasoning_acc = String::new();
        let content_calls = std::sync::Mutex::new(Vec::<String>::new());
        let on_content = |text: String| content_calls.lock().unwrap().push(text);

        process_ollama_think_chunk(
            "Hello world",
            &mut in_think_block,
            &mut content_acc,
            &mut reasoning_acc,
            None,
            Some(&on_content),
        );

        assert_eq!(content_acc, "Hello world");
        assert_eq!(reasoning_acc, "");
        assert_eq!(*content_calls.lock().unwrap(), vec!["Hello world"]);
    }

    #[test]
    fn test_process_ollama_think_chunk_routes_reasoning_inside_think_tags() {
        let mut in_think_block = false;
        let mut content_acc = String::new();
        let mut reasoning_acc = String::new();
        let reasoning_calls = std::sync::Mutex::new(Vec::<String>::new());
        let on_reasoning = |text: String| reasoning_calls.lock().unwrap().push(text);

        // Single chunk containing the full think block and trailing content
        process_ollama_think_chunk(
            "<think>Let me think</think>Answer",
            &mut in_think_block,
            &mut content_acc,
            &mut reasoning_acc,
            Some(&on_reasoning),
            None,
        );

        assert_eq!(reasoning_acc, "Let me think");
        assert_eq!(content_acc, "Answer");
        assert!(
            reasoning_calls
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.contains("Let me think"))
        );
        assert!(
            !in_think_block,
            "think block should be closed after </think>"
        );
    }

    #[test]
    fn test_process_ollama_think_chunk_spans_multiple_calls() {
        let mut in_think_block = false;
        let mut content_acc = String::new();
        let mut reasoning_acc = String::new();

        // Opening tag chunk
        process_ollama_think_chunk(
            "<think>",
            &mut in_think_block,
            &mut content_acc,
            &mut reasoning_acc,
            None,
            None,
        );
        assert!(in_think_block, "should be in think block after opening tag");

        // Reasoning content chunk (inside think block)
        process_ollama_think_chunk(
            "reasoning content",
            &mut in_think_block,
            &mut content_acc,
            &mut reasoning_acc,
            None,
            None,
        );
        assert_eq!(reasoning_acc, "reasoning content");

        // Closing tag chunk
        process_ollama_think_chunk(
            "</think>",
            &mut in_think_block,
            &mut content_acc,
            &mut reasoning_acc,
            None,
            None,
        );
        assert!(!in_think_block, "should exit think block after closing tag");
    }

    // --- OllamaStreamChunk / native thinking field tests ---

    #[test]
    fn test_ollama_stream_chunk_parses_content_only() {
        let json = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse content-only chunk");
        assert_eq!(chunk.message.content, "Hello");
        assert!(chunk.message.thinking.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn test_ollama_stream_chunk_parses_thinking_field() {
        let json = r#"{"message":{"role":"assistant","content":"","thinking":"let me think"},"done":false}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse thinking-field chunk");
        assert!(chunk.message.content.is_empty());
        assert_eq!(chunk.message.thinking, "let me think");
    }

    #[test]
    fn test_ollama_stream_chunk_parses_done_with_counts() {
        let json = r#"{"message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":42,"eval_count":100}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(json).expect("should parse done chunk");
        assert!(chunk.done);
        assert_eq!(chunk.prompt_eval_count, 42);
        assert_eq!(chunk.eval_count, 100);
    }

    #[test]
    fn test_ollama_stream_chunk_missing_thinking_defaults_to_empty() {
        // Models that do not emit the thinking field should deserialize cleanly.
        let json = r#"{"message":{"role":"assistant","content":"Hi"},"done":false}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse without thinking field");
        assert_eq!(chunk.message.thinking, "");
    }

    #[test]
    fn test_reasoning_promoted_to_content_when_content_empty() {
        // Simulate the response-assembly path: when content_acc is empty but
        // reasoning_acc is not (Gemma 4 native-thinking behaviour), the
        // reasoning is promoted to final_content and final_reasoning is cleared.
        let content_acc = String::new();
        let reasoning_acc = String::from("I thought about it carefully.");

        let (final_content, final_reasoning) = if !content_acc.is_empty() {
            (content_acc, reasoning_acc)
        } else if !reasoning_acc.is_empty() {
            (reasoning_acc, String::new())
        } else {
            (String::new(), String::new())
        };

        assert_eq!(final_content, "I thought about it carefully.");
        assert!(
            final_reasoning.is_empty(),
            "reasoning should be cleared after promotion"
        );
    }

    #[test]
    fn test_content_takes_priority_over_reasoning_when_both_present() {
        // When both content and reasoning are non-empty, content is returned
        // as the response and reasoning is kept separate for the Zed panel.
        let content_acc = String::from("Final answer.");
        let reasoning_acc = String::from("I thought about it.");

        let (final_content, final_reasoning) = if !content_acc.is_empty() {
            (content_acc, reasoning_acc)
        } else if !reasoning_acc.is_empty() {
            (reasoning_acc, String::new())
        } else {
            (String::new(), String::new())
        };

        assert_eq!(final_content, "Final answer.");
        assert_eq!(final_reasoning, "I thought about it.");
    }

    #[test]
    fn test_empty_response_gives_empty_content_and_reasoning() {
        let content_acc = String::new();
        let reasoning_acc = String::new();

        let (final_content, final_reasoning) = if !content_acc.is_empty() {
            (content_acc, reasoning_acc)
        } else if !reasoning_acc.is_empty() {
            (reasoning_acc, String::new())
        } else {
            (String::new(), String::new())
        };

        assert!(final_content.is_empty());
        assert!(final_reasoning.is_empty());
    }

    // --- Tool call streaming tests ---

    #[test]
    fn test_ollama_stream_chunk_parses_tool_calls_on_non_done_chunk() {
        let json = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"read_file","arguments":{"path":"foo.txt"}}}]},"done":false}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse tool-call chunk");
        assert_eq!(chunk.message.tool_calls.len(), 1);
        assert_eq!(chunk.message.tool_calls[0].function.name, "read_file");
        assert!(chunk.message.content.is_empty());
        assert!(!chunk.done);
    }

    #[test]
    fn test_ollama_stream_chunk_parses_tool_calls_on_done_chunk() {
        // Some Ollama versions attach tool calls to the done chunk instead.
        let json = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"write_file","arguments":{"path":"out.txt","content":"hello"}}}]},"done":true,"prompt_eval_count":5,"eval_count":10}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse done tool-call chunk");
        assert!(chunk.done);
        assert_eq!(chunk.message.tool_calls.len(), 1);
        assert_eq!(chunk.message.tool_calls[0].function.name, "write_file");
    }

    #[test]
    fn test_ollama_stream_chunk_no_tool_calls_defaults_to_empty_vec() {
        // Content-only chunks must not have tool_calls populated.
        let json = r#"{"message":{"role":"assistant","content":"hello"},"done":false}"#;
        let chunk: OllamaStreamChunk =
            serde_json::from_str(json).expect("should parse content chunk");
        assert!(chunk.message.tool_calls.is_empty());
    }

    #[test]
    fn test_ollama_request_body_omits_options_when_num_ctx_is_none() {
        let request = OllamaRequestFull {
            model: "llama3.2".to_string(),
            messages: vec![],
            tools: vec![],
            stream: false,
            options: None,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("options"),
            "options key should be absent when num_ctx is None"
        );
    }

    #[test]
    fn test_ollama_request_body_includes_num_ctx_when_set() {
        let request = OllamaRequestFull {
            model: "llama3.2".to_string(),
            messages: vec![],
            tools: vec![],
            stream: false,
            options: Some(OllamaOptions {
                num_ctx: Some(16384),
            }),
        };
        let json = serde_json::to_value(&request).unwrap();
        let options = json.get("options").expect("options key should be present");
        assert_eq!(options["num_ctx"], 16384, "num_ctx should be 16384");
    }
}
