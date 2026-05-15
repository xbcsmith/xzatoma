//! Agent core implementation with autonomous execution loop
//!
//! This module implements the main agent execution loop that:
//! - Manages conversation with AI providers
//! - Executes tool calls requested by the provider
//! - Enforces iteration limits and timeouts
//! - Handles errors and stops conditions gracefully

use crate::agent::events::{AgentExecutionEvent, AgentObserver, NoOpObserver};
use crate::chat_mode::{ChatMode, SafetyMode};
use crate::config::AgentConfig;
use crate::error::{Result, XzatomaError};
use crate::prompts;
use crate::providers::{Message, Provider, TokenUsage, ToolCall};
use crate::tools::{ToolRegistry, ToolResult};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::thinking::extract_thinking;
use super::{ContextInfo, Conversation};

/// The main agent that executes autonomous tasks
///
/// The agent maintains a conversation with an AI provider and executes
/// tool calls requested by the provider. It enforces safety limits:
/// - Maximum iterations to prevent infinite loops
/// - Timeout to prevent runaway execution
/// - Tool execution validation
/// - Token usage tracking across multiple completions
///
/// # Examples
///
/// ```
/// use xzatoma::agent::Agent;
/// use xzatoma::config::AgentConfig;
/// use xzatoma::tools::ToolRegistry;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// // Requires a provider implementation
/// # use xzatoma::config::CopilotConfig;
/// # use xzatoma::providers::CopilotProvider;
/// # let provider = CopilotProvider::new(CopilotConfig::default())?;
/// let tools = ToolRegistry::new();
/// let config = AgentConfig::default();
///
/// let mut agent = Agent::new(provider, tools, config)?;
/// let result = agent.execute("Write a hello world program").await?;
/// # Ok(())
/// # }
/// ```
pub struct Agent {
    provider: Arc<dyn Provider>,
    conversation: Conversation,
    tools: ToolRegistry,
    config: AgentConfig,
    accumulated_usage: Arc<Mutex<Option<TokenUsage>>>,
    transient_system_messages: Vec<String>,
}

/// Combines reasoning text from two independent sources.
///
/// `raw` is the structured `CompletionResponse.reasoning` field populated by the
/// provider accumulator. `tags` is the reasoning extracted from inline thinking
/// blocks by `extract_thinking`. When both are present they are joined with a
/// single newline so the observer receives all available chain-of-thought content.
///
/// Returns `None` when both inputs are `None` so callers can skip emitting a
/// `ReasoningEmitted` event entirely.
fn combine_reasoning(raw: Option<String>, tags: Option<String>) -> Option<String> {
    match (raw, tags) {
        (Some(r), Some(t)) => Some(format!("{}\n{}", r, t)),
        (Some(r), None) => Some(r),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

impl Agent {
    /// Creates a new agent instance
    ///
    /// # Arguments
    ///
    /// * `provider` - The AI provider to use for completions
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    pub fn new(
        provider: impl Provider + 'static,
        tools: ToolRegistry,
        config: AgentConfig,
    ) -> Result<Self> {
        // Validate configuration
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        let conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
            config.conversation.prune_threshold.into(),
        );

        Ok(Self {
            provider: Arc::new(provider),
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Creates a new agent instance with a boxed provider
    ///
    /// Useful when the provider type is not known at compile time,
    /// or when working with dynamically created providers.
    ///
    /// # Arguments
    ///
    /// * `provider` - A boxed provider instance
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    pub fn new_boxed(
        provider: Box<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
    ) -> Result<Self> {
        // Validate configuration
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        let conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
            config.conversation.prune_threshold.into(),
        );

        Ok(Self {
            provider: Arc::from(provider),
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Creates a new agent instance sharing an existing provider
    ///
    /// This constructor allows multiple agents to share the same
    /// provider instance (via Arc), useful for subagents that need
    /// the same LLM client without duplication.
    ///
    /// # Arguments
    ///
    /// * `provider` - Shared reference to an existing provider
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Agent;
    /// use xzatoma::tools::ToolRegistry;
    /// use xzatoma::config::AgentConfig;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let some_provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let parent_provider: Arc<dyn xzatoma::providers::Provider> = Arc::new(some_provider);
    /// let parent_agent = Agent::new_from_shared_provider(
    ///     Arc::clone(&parent_provider),
    ///     ToolRegistry::new(),
    ///     AgentConfig::default(),
    /// )?;
    ///
    /// // Subagent shares the same provider
    /// let subagent = Agent::new_from_shared_provider(
    ///     parent_provider,
    ///     ToolRegistry::new(),
    ///     AgentConfig::default(),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_from_shared_provider(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
    ) -> Result<Self> {
        // Validate configuration (same as new() method)
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        let conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
            config.conversation.prune_threshold.into(),
        );

        Ok(Self {
            provider, // Use provided Arc directly (no wrapping)
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Creates a new agent instance with an existing conversation
    ///
    /// Useful for preserving conversation history when switching modes or
    /// rebuilding the tool registry while maintaining the discussion context.
    ///
    /// # Arguments
    ///
    /// * `provider` - The AI provider to use for completions
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    /// * `conversation` - An existing conversation to use
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::{Agent, Conversation};
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # use xzatoma::tools::ToolRegistry;
    /// # use xzatoma::agent::Agent;
    /// # let provider_impl = CopilotProvider::new(CopilotConfig::default())?;
    /// # let old_agent = Agent::new(provider_impl, ToolRegistry::new(), AgentConfig::default())?;
    /// // Get existing conversation from old agent
    /// # let _ref = &old_agent;
    /// let conversation = old_agent.conversation().clone();
    ///
    /// // Create new agent with same conversation but different tools
    /// let new_tools = ToolRegistry::new();
    /// let config = AgentConfig::default();
    /// let new_agent = Agent::with_conversation(
    ///     Box::new(CopilotProvider::new(CopilotConfig::default())?),
    ///     new_tools,
    ///     config,
    ///     conversation,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_conversation(
        provider: Box<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
        conversation: Conversation,
    ) -> Result<Self> {
        // Validate configuration
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        Ok(Self {
            provider: Arc::from(provider),
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Creates a new agent instance with an existing conversation and shared provider
    ///
    /// Useful for resuming conversations while sharing a provider instance with other agents.
    /// This combines the benefits of conversation persistence with provider efficiency (via Arc).
    ///
    /// # Arguments
    ///
    /// * `provider` - Shared reference to an existing provider
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    /// * `conversation` - An existing conversation to use
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::{Agent, Conversation};
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # let provider_impl = xzatoma::providers::CopilotProvider::new(xzatoma::config::CopilotConfig::default())?;
    /// # let provider: Arc<dyn xzatoma::providers::Provider> = Arc::new(provider_impl);
    /// let conversation = Conversation::new(4096, 5, 0.8);
    ///
    /// let agent = Agent::with_conversation_and_shared_provider(
    ///     Arc::clone(&provider),
    ///     ToolRegistry::new(),
    ///     AgentConfig::default(),
    ///     conversation,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_conversation_and_shared_provider(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
        conversation: Conversation,
    ) -> Result<Self> {
        // Validate configuration
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        Ok(Self {
            provider,
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Creates a new agent instance with mode-specific system prompt
    ///
    /// This constructor creates an agent with a system prompt tailored to the
    /// specified chat mode (Planning or Write) and safety mode (AlwaysConfirm or NeverConfirm).
    /// The system prompt is automatically added to the conversation.
    ///
    /// # Arguments
    ///
    /// * `provider` - A boxed provider instance
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    /// * `mode` - The ChatMode (Planning or Write)
    /// * `safety` - The SafetyMode (AlwaysConfirm or NeverConfirm)
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance with the system prompt added or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Agent;
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode};
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # let provider: Box<dyn xzatoma::providers::Provider> = Box::new(xzatoma::providers::CopilotProvider::new(xzatoma::config::CopilotConfig::default())?);
    /// let tools = ToolRegistry::new();
    /// let config = AgentConfig::default();
    /// let mut agent = Agent::new_with_mode(
    ///     provider,
    ///     tools,
    ///     config,
    ///     ChatMode::Planning,
    ///     SafetyMode::AlwaysConfirm,
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_mode(
        provider: Box<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
        mode: ChatMode,
        safety: SafetyMode,
    ) -> Result<Self> {
        // Validate configuration
        if config.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        let mut conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
            config.conversation.prune_threshold.into(),
        );

        // Build and add mode-specific system prompt
        let system_prompt = prompts::build_system_prompt(mode, safety);
        conversation.add_system_message(system_prompt);

        debug!("Created agent with mode={:?} safety={:?}", mode, safety);

        Ok(Self {
            provider: Arc::from(provider),
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
            transient_system_messages: Vec::new(),
        })
    }

    /// Executes the agent with the given user prompt
    ///
    /// This is the main execution loop that:
    /// 1. Adds user prompt to conversation
    /// 2. Calls provider for completion
    /// 3. Executes any requested tool calls
    /// 4. Repeats until provider stops or limits are reached
    ///
    /// # Arguments
    ///
    /// * `user_prompt` - The initial user request
    ///
    /// # Returns
    ///
    /// Returns the final assistant response or an error
    ///
    /// # Errors
    ///
    /// - `XzatomaError::MaxIterationsExceeded` if iteration limit is reached
    /// - `XzatomaError::RuntimeTimeout` if timeout is exceeded
    /// - `XzatomaError::Provider` if provider calls fail
    /// - `XzatomaError::Tool` if tool execution fails
    ///
    /// # Examples
    ///
    /// ```
    /// # use xzatoma::agent::Agent;
    /// # use xzatoma::config::AgentConfig;
    /// # use xzatoma::tools::ToolRegistry;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let tools = ToolRegistry::new();
    /// # let config = AgentConfig::default();
    /// # let mut agent = Agent::new(provider, tools, config)?;
    /// let result = agent.execute("List files in current directory").await?;
    /// println!("Agent result: {}", result);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&mut self, user_prompt: impl Into<String>) -> Result<String> {
        let token = CancellationToken::new();
        let mut observer = NoOpObserver;
        self.execute_with_observer(user_prompt, &token, &mut observer)
            .await
    }

    /// Executes the agent with an observer and a cancellation token.
    ///
    /// This is the evented core execution path. The legacy [`Agent::execute`]
    /// delegates here using a [`NoOpObserver`] and a non-cancelled token.
    ///
    /// # Arguments
    ///
    /// * `user_prompt` - The user's input prompt.
    /// * `cancellation_token` - Token checked at safe boundaries; cancel it to
    ///   abort execution at the next opportunity.
    /// * `observer` - Receives [`AgentExecutionEvent`] values as execution proceeds.
    ///
    /// # Returns
    ///
    /// Returns the final assistant response or an error.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Cancelled`] if the cancellation token fires.
    /// Returns the same errors as [`Agent::execute`] for all other failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzatoma::agent::Agent;
    /// # use xzatoma::agent::events::NoOpObserver;
    /// # use xzatoma::config::AgentConfig;
    /// # use xzatoma::tools::ToolRegistry;
    /// # use tokio_util::sync::CancellationToken;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let tools = ToolRegistry::new();
    /// # let config = AgentConfig::default();
    /// # let mut agent = Agent::new(provider, tools, config)?;
    /// let token = CancellationToken::new();
    /// let mut observer = NoOpObserver;
    /// let result = agent
    ///     .execute_with_observer("Do the task", &token, &mut observer)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_with_observer(
        &mut self,
        user_prompt: impl Into<String>,
        cancellation_token: &CancellationToken,
        observer: &mut dyn AgentObserver,
    ) -> Result<String> {
        if cancellation_token.is_cancelled() {
            observer.on_event(AgentExecutionEvent::CancellationRequested);
            return Err(XzatomaError::Cancelled);
        }

        observer.on_event(AgentExecutionEvent::PromptStarted);

        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        self.conversation.add_user_message(user_prompt.into());

        let mut iteration = 0;

        loop {
            if cancellation_token.is_cancelled() {
                observer.on_event(AgentExecutionEvent::CancellationRequested);
                return Err(XzatomaError::Cancelled);
            }

            iteration += 1;

            if iteration > self.config.max_turns {
                warn!("Maximum iterations ({}) exceeded", self.config.max_turns);
                let error = XzatomaError::MaxIterationsExceeded {
                    limit: self.config.max_turns,
                    message: format!(
                        "Agent exceeded maximum iteration limit of {}",
                        self.config.max_turns
                    ),
                };
                observer.on_event(AgentExecutionEvent::ExecutionFailed {
                    error: error.to_string(),
                });
                return Err(error);
            }

            if start_time.elapsed() > timeout {
                let elapsed = start_time.elapsed();
                warn!("Agent execution timeout after {:?}", elapsed);
                let error = XzatomaError::RuntimeTimeout {
                    operation: "agent execution".to_string(),
                    timeout_seconds: self.config.timeout_seconds,
                    elapsed_seconds: elapsed.as_secs(),
                };
                observer.on_event(AgentExecutionEvent::ExecutionFailed {
                    error: error.to_string(),
                });
                return Err(error);
            }

            debug!(
                "Iteration {}/{}, tokens: {}/{}",
                iteration,
                self.config.max_turns,
                self.conversation.token_count(),
                self.conversation.max_tokens()
            );

            let tool_definitions = self.tools.all_definitions();
            let prompt_messages = self.messages_with_transient_system_messages();

            observer.on_event(AgentExecutionEvent::ProviderRequestStarted);

            let completion_response = tokio::select! {
                result = self.provider.complete(&prompt_messages, &tool_definitions) => result?,
                _ = cancellation_token.cancelled() => {
                    observer.on_event(AgentExecutionEvent::CancellationRequested);
                    return Err(XzatomaError::Cancelled);
                }
            };

            let raw_reasoning = completion_response.reasoning;
            let mut message = completion_response.message;
            debug!("Provider response: {:?}", message);

            // Strip inline thinking tags from the assistant text. Any text enclosed in
            // thinking tag blocks is removed from the message content before it enters
            // conversation history, and the extracted content is collected as tag_reasoning.
            let tag_reasoning = if let Some(text) = message.content.take() {
                let (clean, tag_r) = extract_thinking(&text);
                message.content = Some(clean);
                tag_r
            } else {
                None
            };

            // Emit a ReasoningEmitted event when reasoning is available from either the
            // structured CompletionResponse.reasoning field or extracted thinking tags.
            if let Some(combined) = combine_reasoning(raw_reasoning, tag_reasoning) {
                observer.on_event(AgentExecutionEvent::ReasoningEmitted { text: combined });
            }

            if let Some(usage) = completion_response.usage {
                self.conversation.update_from_provider_usage(&usage);
                let mut accumulated = self.accumulated_usage.lock().unwrap();
                if let Some(existing) = *accumulated {
                    *accumulated = Some(TokenUsage::new(
                        existing.prompt_tokens + usage.prompt_tokens,
                        existing.completion_tokens + usage.completion_tokens,
                    ));
                } else {
                    *accumulated = Some(usage);
                }
                drop(accumulated);
            }

            // Emit context window state regardless of whether provider usage was returned.
            // get_context_info() prefers provider usage over the heuristic when available.
            let ctx = self.get_context_info(self.conversation.max_tokens());
            observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
                used_tokens: ctx.used_tokens as u64,
                max_tokens: ctx.max_tokens as u64,
            });

            let has_tool_calls = message.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty());

            observer.on_event(AgentExecutionEvent::ProviderResponseReceived {
                text: message.content.clone(),
                has_tool_calls,
            });

            if let Some(text) = &message.content {
                if !text.is_empty() {
                    observer
                        .on_event(AgentExecutionEvent::AssistantTextEmitted { text: text.clone() });
                }
            }

            self.conversation.add_message(message.clone());

            if let Some(tool_calls) = &message.tool_calls {
                if tool_calls.is_empty() {
                    debug!("Provider returned empty tool calls, stopping");
                    break;
                }

                debug!("Executing {} tool calls", tool_calls.len());

                for tool_call in tool_calls {
                    if cancellation_token.is_cancelled() {
                        observer.on_event(AgentExecutionEvent::CancellationRequested);
                        return Err(XzatomaError::Cancelled);
                    }

                    observer.on_event(AgentExecutionEvent::ToolCallStarted {
                        id: tool_call.id.clone(),
                        name: tool_call.function.name.clone(),
                        arguments: tool_call.function.arguments.clone(),
                    });

                    let result = tokio::select! {
                        result = self.execute_tool_call(tool_call) => result,
                        _ = cancellation_token.cancelled() => {
                            observer.on_event(AgentExecutionEvent::CancellationRequested);
                            return Err(XzatomaError::Cancelled);
                        }
                    };

                    match result {
                        Ok(tool_result) => {
                            observer.on_event(AgentExecutionEvent::ToolCallCompleted {
                                id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                output: tool_result.output.clone(),
                            });
                            self.conversation
                                .add_tool_result(&tool_call.id, tool_result.to_message());
                        }
                        Err(error) => {
                            observer.on_event(AgentExecutionEvent::ToolCallFailed {
                                id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                error: error.to_string(),
                            });
                            return Err(error);
                        }
                    }
                }

                let auto_threshold = self.config.conversation.auto_summary_threshold as f64;
                if self.conversation.should_auto_summarize(auto_threshold) {
                    warn!(
                        "Context window critical (>{}%), triggering automatic summarization",
                        (auto_threshold * 100.0) as u8
                    );
                    match self.perform_auto_summarization().await {
                        Ok(_) => {
                            info!("Automatic summarization complete, conversation pruned");
                        }
                        Err(error) => {
                            warn!(
                                "Automatic summarization failed: {}. Continuing with pruning.",
                                error
                            );
                            self.conversation.prune_if_needed();
                        }
                    }
                }

                continue;
            }

            if message.content.is_some() {
                debug!("Provider returned final response, stopping");
                break;
            }

            warn!("Provider returned neither content nor tool calls");
            let error = XzatomaError::Provider(
                "Provider returned invalid response (no content or tool calls)".to_string(),
            );
            observer.on_event(AgentExecutionEvent::ExecutionFailed {
                error: error.to_string(),
            });
            return Err(error);
        }

        let final_message = self
            .conversation
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .and_then(|m| m.content.as_ref())
            .cloned()
            .unwrap_or_else(|| "No response from assistant".to_string());

        observer.on_event(AgentExecutionEvent::ExecutionCompleted {
            response: final_message.clone(),
        });

        info!(
            "Agent execution completed in {} iterations, {} seconds",
            iteration,
            start_time.elapsed().as_secs()
        );

        Ok(final_message)
    }

    /// Executes the agent with already-constructed provider messages.
    ///
    /// This entry point is used by transports that need to preserve richer
    /// provider-layer message data, such as ACP stdio multimodal prompts with
    /// ordered text and image content parts. The messages are appended to the
    /// current conversation before the normal autonomous execution loop runs.
    ///
    /// # Arguments
    ///
    /// * `messages` - Provider-layer messages to append before execution
    ///
    /// # Returns
    ///
    /// Returns the final assistant response text or an error.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Provider` if `messages` is empty. Returns the same
    /// errors as [`Self::execute`] for provider calls, tool execution, iteration
    /// limits, and timeouts.
    ///
    /// # Examples
    ///
    /// ```
    /// # use xzatoma::agent::Agent;
    /// # use xzatoma::config::AgentConfig;
    /// # use xzatoma::providers::Message;
    /// # use xzatoma::tools::ToolRegistry;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let tools = ToolRegistry::new();
    /// # let config = AgentConfig::default();
    /// # let mut agent = Agent::new(provider, tools, config)?;
    /// let messages = vec![Message::user("Describe this task")];
    /// let result = agent.execute_provider_messages(messages).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_provider_messages(&mut self, messages: Vec<Message>) -> Result<String> {
        let token = CancellationToken::new();
        let mut observer = NoOpObserver;
        self.execute_provider_messages_with_observer(messages, &token, &mut observer)
            .await
    }

    /// Executes the agent from already-constructed provider messages with an
    /// observer and cancellation token.
    ///
    /// This variant is used by ACP stdio transports that need to preserve rich
    /// provider-layer message data such as multimodal image parts. The messages
    /// are appended to the current conversation before the execution loop runs.
    ///
    /// # Arguments
    ///
    /// * `messages` - Provider-layer messages to append before execution.
    /// * `cancellation_token` - Token checked at safe boundaries.
    /// * `observer` - Receives [`AgentExecutionEvent`] values as execution proceeds.
    ///
    /// # Returns
    ///
    /// Returns the final assistant response or an error.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Provider`] if `messages` is empty.
    /// Returns [`XzatomaError::Cancelled`] if the cancellation token fires.
    /// Returns the same errors as [`Agent::execute_provider_messages`] otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use xzatoma::agent::Agent;
    /// # use xzatoma::agent::events::NoOpObserver;
    /// # use xzatoma::config::AgentConfig;
    /// # use xzatoma::providers::Message;
    /// # use xzatoma::tools::ToolRegistry;
    /// # use tokio_util::sync::CancellationToken;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let tools = ToolRegistry::new();
    /// # let config = AgentConfig::default();
    /// # let mut agent = Agent::new(provider, tools, config)?;
    /// let messages = vec![Message::user("Describe this task")];
    /// let token = CancellationToken::new();
    /// let mut observer = NoOpObserver;
    /// let result = agent
    ///     .execute_provider_messages_with_observer(messages, &token, &mut observer)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_provider_messages_with_observer(
        &mut self,
        messages: Vec<Message>,
        cancellation_token: &CancellationToken,
        observer: &mut dyn AgentObserver,
    ) -> Result<String> {
        if messages.is_empty() {
            return Err(XzatomaError::Provider(
                "provider message execution requires at least one message".to_string(),
            ));
        }

        if cancellation_token.is_cancelled() {
            observer.on_event(AgentExecutionEvent::CancellationRequested);
            return Err(XzatomaError::Cancelled);
        }

        observer.on_event(AgentExecutionEvent::PromptStarted);

        let vision_count = messages.iter().filter(|m| m.has_image_content()).count();
        if vision_count > 0 {
            observer.on_event(AgentExecutionEvent::VisionInputAttached {
                count: vision_count,
            });
        }

        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        info!("Starting agent execution from provider messages");

        for message in messages {
            self.conversation.add_message(message);
        }

        let mut iteration = 0;

        loop {
            if cancellation_token.is_cancelled() {
                observer.on_event(AgentExecutionEvent::CancellationRequested);
                return Err(XzatomaError::Cancelled);
            }

            iteration += 1;

            if iteration > self.config.max_turns {
                warn!("Maximum iterations ({}) exceeded", self.config.max_turns);
                let error = XzatomaError::MaxIterationsExceeded {
                    limit: self.config.max_turns,
                    message: format!(
                        "Agent exceeded maximum iteration limit of {}",
                        self.config.max_turns
                    ),
                };
                observer.on_event(AgentExecutionEvent::ExecutionFailed {
                    error: error.to_string(),
                });
                return Err(error);
            }

            if start_time.elapsed() > timeout {
                let elapsed = start_time.elapsed();
                warn!("Agent execution timeout after {:?}", elapsed);
                let error = XzatomaError::RuntimeTimeout {
                    operation: "agent provider-message execution".to_string(),
                    timeout_seconds: self.config.timeout_seconds,
                    elapsed_seconds: elapsed.as_secs(),
                };
                observer.on_event(AgentExecutionEvent::ExecutionFailed {
                    error: error.to_string(),
                });
                return Err(error);
            }

            debug!(
                "Iteration {}/{}, tokens: {}/{}",
                iteration,
                self.config.max_turns,
                self.conversation.token_count(),
                self.conversation.max_tokens()
            );

            let tool_definitions = self.tools.all_definitions();
            let prompt_messages = self.messages_with_transient_system_messages();

            observer.on_event(AgentExecutionEvent::ProviderRequestStarted);

            let completion_response = tokio::select! {
                result = self.provider.complete(&prompt_messages, &tool_definitions) => result?,
                _ = cancellation_token.cancelled() => {
                    observer.on_event(AgentExecutionEvent::CancellationRequested);
                    return Err(XzatomaError::Cancelled);
                }
            };

            let raw_reasoning = completion_response.reasoning;
            let mut message = completion_response.message;
            debug!("Provider response: {:?}", message);

            // Strip inline thinking tags from the assistant text.
            let tag_reasoning = if let Some(text) = message.content.take() {
                let (clean, tag_r) = extract_thinking(&text);
                message.content = Some(clean);
                tag_r
            } else {
                None
            };

            // Emit a ReasoningEmitted event when reasoning is available.
            if let Some(combined) = combine_reasoning(raw_reasoning, tag_reasoning) {
                observer.on_event(AgentExecutionEvent::ReasoningEmitted { text: combined });
            }

            if let Some(usage) = completion_response.usage {
                self.conversation.update_from_provider_usage(&usage);

                let mut accumulated_usage = self.accumulated_usage.lock().unwrap();
                if let Some(existing) = *accumulated_usage {
                    *accumulated_usage = Some(TokenUsage::new(
                        existing.prompt_tokens + usage.prompt_tokens,
                        existing.completion_tokens + usage.completion_tokens,
                    ));
                } else {
                    *accumulated_usage = Some(usage);
                }
                drop(accumulated_usage);
            }

            // Emit context window state regardless of whether provider usage was returned.
            // get_context_info() prefers provider usage over the heuristic when available.
            let ctx = self.get_context_info(self.conversation.max_tokens());
            observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
                used_tokens: ctx.used_tokens as u64,
                max_tokens: ctx.max_tokens as u64,
            });

            let has_tool_calls = message.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty());

            observer.on_event(AgentExecutionEvent::ProviderResponseReceived {
                text: message.content.clone(),
                has_tool_calls,
            });

            if let Some(text) = &message.content {
                if !text.is_empty() {
                    observer
                        .on_event(AgentExecutionEvent::AssistantTextEmitted { text: text.clone() });
                }
            }

            self.conversation.add_message(message.clone());

            if let Some(tool_calls) = &message.tool_calls {
                if tool_calls.is_empty() {
                    debug!("Provider returned empty tool calls, stopping");
                    break;
                }

                debug!("Executing {} tool calls", tool_calls.len());

                for tool_call in tool_calls {
                    if cancellation_token.is_cancelled() {
                        observer.on_event(AgentExecutionEvent::CancellationRequested);
                        return Err(XzatomaError::Cancelled);
                    }

                    observer.on_event(AgentExecutionEvent::ToolCallStarted {
                        id: tool_call.id.clone(),
                        name: tool_call.function.name.clone(),
                        arguments: tool_call.function.arguments.clone(),
                    });

                    let result = tokio::select! {
                        result = self.execute_tool_call(tool_call) => result,
                        _ = cancellation_token.cancelled() => {
                            observer.on_event(AgentExecutionEvent::CancellationRequested);
                            return Err(XzatomaError::Cancelled);
                        }
                    };

                    match result {
                        Ok(tool_result) => {
                            observer.on_event(AgentExecutionEvent::ToolCallCompleted {
                                id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                output: tool_result.output.clone(),
                            });
                            self.conversation
                                .add_tool_result(&tool_call.id, tool_result.to_message());
                        }
                        Err(error) => {
                            observer.on_event(AgentExecutionEvent::ToolCallFailed {
                                id: tool_call.id.clone(),
                                name: tool_call.function.name.clone(),
                                error: error.to_string(),
                            });
                            return Err(error);
                        }
                    }
                }

                let auto_threshold = self.config.conversation.auto_summary_threshold as f64;
                if self.conversation.should_auto_summarize(auto_threshold) {
                    warn!(
                        "Context window critical (>{}%), triggering automatic summarization",
                        (auto_threshold * 100.0) as u8
                    );
                    match self.perform_auto_summarization().await {
                        Ok(_) => {
                            info!("Automatic summarization complete, conversation pruned");
                        }
                        Err(error) => {
                            warn!(
                                "Automatic summarization failed: {}. Continuing with pruning.",
                                error
                            );
                            self.conversation.prune_if_needed();
                        }
                    }
                }

                continue;
            }

            if message.content.is_some() {
                debug!("Provider returned final response, stopping");
                break;
            }

            warn!("Provider returned neither content nor tool calls");
            let error = XzatomaError::Provider(
                "Provider returned invalid response (no content or tool calls)".to_string(),
            );
            observer.on_event(AgentExecutionEvent::ExecutionFailed {
                error: error.to_string(),
            });
            return Err(error);
        }

        let final_message = self
            .conversation
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == "assistant")
            .and_then(|message| message.content.as_ref())
            .cloned()
            .unwrap_or_else(|| "No response from assistant".to_string());

        observer.on_event(AgentExecutionEvent::ExecutionCompleted {
            response: final_message.clone(),
        });

        info!(
            "Agent execution completed in {} iterations, {} seconds",
            iteration,
            start_time.elapsed().as_secs()
        );

        Ok(final_message)
    }

    /// Executes a single tool call
    ///
    /// # Arguments
    ///
    /// * `tool_call` - The tool call to execute
    ///
    /// # Returns
    ///
    /// Returns the tool execution result or an error
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Tool` if:
    /// - Tool is not found in registry
    /// - Tool execution fails
    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        let tool_name = &tool_call.function.name;
        debug!("Executing tool: {}", tool_name);

        // Get tool from registry
        let tool_executor = self
            .tools
            .get(tool_name)
            .ok_or_else(|| XzatomaError::Tool(format!("Tool not found: {}", tool_name)))?;

        // Parse arguments
        let args: serde_json::Value =
            serde_json::from_str(&tool_call.function.arguments).map_err(|e| {
                XzatomaError::Tool(format!(
                    "Failed to parse tool arguments for '{}': {}",
                    tool_name, e
                ))
            })?;

        // Execute tool
        let result = tool_executor.execute(args).await.map_err(|e| {
            XzatomaError::Tool(format!("Tool '{}' execution failed: {}", tool_name, e))
        })?;

        // Truncate output if needed
        let max_output_size = self.config.tools.max_output_size;
        let original_len = result.output.len();
        let truncated_result = result.truncate_if_needed(max_output_size);

        if truncated_result.truncated {
            debug!(
                "Tool output truncated from {} to {} bytes",
                original_len, max_output_size
            );
        }

        Ok(truncated_result)
    }

    /// Performs automatic summarization of the conversation
    ///
    /// This method creates a summary of older messages in the conversation
    /// to reduce token usage when approaching context limits.
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if summarization was successful, or an error if it fails
    ///
    /// # Errors
    ///
    /// Returns error if summary generation or insertion fails
    async fn perform_auto_summarization(&mut self) -> Result<()> {
        debug!("Starting automatic summarization");

        // Get the summary model from config, or use current provider's model
        let summary_model = self
            .config
            .conversation
            .summary_model
            .clone()
            .unwrap_or_else(|| self.provider.get_current_model());

        debug!("Using summary model: {}", summary_model);

        // Count messages before summarization
        let message_count = self.conversation.messages().len();

        // Summarize and reset the conversation
        self.conversation.summarize_and_reset()?;

        info!(
            "Conversation summarized: {} messages reduced, {} tokens now used",
            message_count,
            self.conversation.token_count()
        );

        Ok(())
    }

    /// Returns a reference to the conversation
    ///
    /// Useful for testing and debugging
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns a mutable reference to the conversation
    pub fn conversation_mut(&mut self) -> &mut Conversation {
        &mut self.conversation
    }

    /// Sets transient system messages used only during prompt assembly.
    ///
    /// These messages are appended to the provider input immediately before each
    /// completion request and are not stored in `Conversation.messages`.
    pub fn set_transient_system_messages(&mut self, messages: Vec<String>) {
        self.transient_system_messages = messages;
    }

    /// Clears all transient system messages.
    ///
    /// This removes any runtime-only prompt injections without modifying
    /// `Conversation.messages`.
    pub fn clear_transient_system_messages(&mut self) {
        self.transient_system_messages.clear();
    }

    /// Returns the current transient system messages.
    pub fn transient_system_messages(&self) -> &[String] {
        &self.transient_system_messages
    }

    fn messages_with_transient_system_messages(&self) -> Vec<Message> {
        if self.transient_system_messages.is_empty() {
            return self.conversation.messages().to_vec();
        }

        let mut messages = Vec::with_capacity(
            self.conversation.messages().len() + self.transient_system_messages.len(),
        );

        let mut inserted = false;
        for message in self.conversation.messages() {
            if !inserted && message.role != "system" {
                for transient in &self.transient_system_messages {
                    messages.push(Message::system(transient.clone()));
                }
                inserted = true;
            }

            messages.push(message.clone());
        }

        if !inserted {
            for transient in &self.transient_system_messages {
                messages.push(Message::system(transient.clone()));
            }
        }

        messages
    }

    /// Returns a reference to the provider
    ///
    /// Useful for accessing provider-specific methods like model listing and switching
    pub fn provider(&self) -> &dyn Provider {
        &*self.provider
    }

    /// Returns a reference to the tool registry
    ///
    /// Useful for accessing and managing available tools
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Returns a mutable reference to the agent's tool registry.
    ///
    /// This accessor is used by the ACP stdio layer to replace individual tools
    /// (such as the terminal tool) when the session mode changes at runtime.
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to the agent's [`ToolRegistry`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Agent;
    /// use xzatoma::tools::ToolRegistry;
    ///
    /// // The registry can be updated to reflect runtime mode changes.
    /// // let mut agent = ...;
    /// // let registry = agent.tools_mut();
    /// ```
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Returns the number of registered tools
    ///
    /// Useful for testing and debugging
    pub fn num_tools(&self) -> usize {
        self.tools.len()
    }

    /// Returns the accumulated token usage from all completions
    ///
    /// This tracks the total prompt tokens, completion tokens, and cumulative usage
    /// from all provider completions across the entire agent session.
    ///
    /// # Returns
    ///
    /// Returns a TokenUsage struct if the provider has reported token counts,
    /// otherwise returns None
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Agent;
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let mut agent = Agent::new(provider, ToolRegistry::new(), AgentConfig::default())?;
    /// # agent.execute("test").await?;
    /// let usage = agent.get_token_usage();
    /// if let Some(u) = usage {
    ///     println!("Used {} prompt tokens and {} completion tokens",
    ///              u.prompt_tokens, u.completion_tokens);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_token_usage(&self) -> Option<TokenUsage> {
        *self.accumulated_usage.lock().unwrap()
    }

    /// Returns context window information for the current conversation
    ///
    /// Provides metrics about how the conversation fits within the model's context window,
    /// including maximum tokens, tokens used, remaining tokens, and percentage used.
    ///
    /// # Arguments
    ///
    /// * `model_context_window` - The context window size of the current model in tokens
    ///
    /// # Returns
    ///
    /// Returns ContextInfo with current usage and remaining tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Agent;
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default())?;
    /// # let mut agent = Agent::new(provider, ToolRegistry::new(), AgentConfig::default())?;
    /// # agent.execute("test").await?;
    /// let context = agent.get_context_info(8192);
    /// println!("Context: {}/{} tokens ({:.1}% used)",
    ///          context.used_tokens, context.max_tokens, context.percentage_used);
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_context_info(&self, model_context_window: usize) -> ContextInfo {
        // Use accumulated agent-level usage if available, otherwise delegate to conversation
        if let Some(usage) = self.get_token_usage() {
            ContextInfo::new(model_context_window, usage.total_tokens)
        } else {
            self.conversation.get_context_info(model_context_window)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{CompletionResponse, FunctionCall, Message};
    use async_trait::async_trait;

    /// Mock provider for testing
    #[derive(Clone)]
    struct MockProvider {
        responses: Vec<Message>,
        call_count: Arc<std::sync::Mutex<usize>>,
        token_usage: Option<TokenUsage>,
    }

    impl MockProvider {
        fn new(responses: Vec<Message>) -> Self {
            Self {
                responses,
                call_count: Arc::new(std::sync::Mutex::new(0)),
                token_usage: None,
            }
        }

        fn with_token_usage(responses: Vec<Message>, usage: TokenUsage) -> Self {
            Self {
                responses,
                call_count: Arc::new(std::sync::Mutex::new(0)),
                token_usage: Some(usage),
            }
        }

        // Required for assertions in tests that verify the provider was called
        #[allow(dead_code)]
        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn is_authenticated(&self) -> bool {
            false
        }

        fn current_model(&self) -> Option<&str> {
            None
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> Result<Vec<crate::providers::ModelInfo>> {
            Err(crate::error::XzatomaError::Provider(
                "not supported".to_string(),
            ))
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            let mut count = self.call_count.lock().unwrap();
            let index = *count;
            *count += 1;

            if index < self.responses.len() {
                let message = self.responses[index].clone();
                let response = if let Some(usage) = self.token_usage {
                    CompletionResponse::with_usage(message, usage)
                } else {
                    CompletionResponse::new(message)
                };
                Ok(response)
            } else {
                // Return final message if we run out of responses
                let message = Message::assistant("Done");
                let response = if let Some(usage) = self.token_usage {
                    CompletionResponse::with_usage(message, usage)
                } else {
                    CompletionResponse::new(message)
                };
                Ok(response)
            }
        }
    }

    #[tokio::test]
    async fn test_agent_creation() {
        let provider = MockProvider::new(vec![]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let agent = Agent::new(provider, tools, config);
        assert!(agent.is_ok());
    }

    #[tokio::test]
    async fn test_agent_creation_with_zero_max_turns_fails() {
        let provider = MockProvider::new(vec![]);
        let tools = ToolRegistry::new();
        let config = AgentConfig {
            max_turns: 0,
            ..Default::default()
        };

        let agent = Agent::new(provider, tools, config);
        assert!(agent.is_err());
    }

    #[tokio::test]
    async fn test_agent_execute_simple_response() {
        let provider = MockProvider::new(vec![Message::assistant("Hello, world!")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Say hello").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn test_agent_execute_multiple_turns() {
        // Agent stops on first message with content (correct behavior)
        let provider = MockProvider::new(vec![Message::assistant("First response")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Complex task").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "First response");
    }

    #[tokio::test]
    async fn test_agent_respects_max_iterations() {
        // Provider that returns tool calls to force multiple iterations
        let mut responses: Vec<Message> = Vec::new();
        for i in 0..10 {
            responses.push(Message {
                role: "assistant".to_string(),
                content: None,
                content_parts: None,
                tool_calls: Some(vec![ToolCall {
                    id: format!("call_{}", i),
                    function: FunctionCall {
                        name: "nonexistent_tool".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            });
        }

        let provider = MockProvider::new(responses);
        let tools = ToolRegistry::new();
        let config = AgentConfig {
            max_turns: 5,
            ..Default::default()
        };

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Loop test").await;

        // Should fail due to tool not found, but we're testing iteration limit
        // In reality it fails on tool execution, but that's okay
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_handles_empty_response() {
        let provider = MockProvider::new(vec![Message {
            role: "assistant".to_string(),
            content: None,
            content_parts: None,
            tool_calls: None,
            tool_call_id: None,
        }]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Test").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_with_tool_calls() {
        let provider = MockProvider::new(vec![
            Message {
                role: "assistant".to_string(),
                content: None,
                content_parts: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    function: FunctionCall {
                        name: "test_tool".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            Message::assistant("Tool result processed"),
        ]);

        let tools = ToolRegistry::new();
        // Note: We would need to register a mock tool executor here
        // For now, this test will fail with "Tool not found" which is expected

        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Use tool").await;

        // Should fail because tool is not registered
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_conversation_tracking() {
        let provider = MockProvider::new(vec![Message::assistant("Response")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let agent = Agent::new(provider, tools, config).unwrap();

        // Conversation should be empty initially
        assert_eq!(agent.conversation().len(), 0);
    }

    #[tokio::test]
    async fn test_agent_num_tools() {
        let provider = MockProvider::new(vec![]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let agent = Agent::new(provider, tools, config).unwrap();
        assert_eq!(agent.num_tools(), 0);
    }

    #[tokio::test]
    async fn test_agent_timeout_enforcement() {
        let responses: Vec<Message> = Vec::new();
        let provider = MockProvider::new(responses);
        let tools = ToolRegistry::new();
        let config = AgentConfig {
            timeout_seconds: 0,
            max_turns: 10000,
            ..Default::default()
        };

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Timeout test").await;

        assert!(matches!(result, Err(XzatomaError::RuntimeTimeout { .. })));
    }

    #[tokio::test]
    async fn test_agent_tool_error_still_precedes_nonzero_timeout() {
        let mut responses: Vec<Message> = Vec::new();
        for i in 0..1000 {
            responses.push(Message {
                role: "assistant".to_string(),
                content: None,
                content_parts: None,
                tool_calls: Some(vec![ToolCall {
                    id: format!("call_{}", i),
                    function: FunctionCall {
                        name: "slow_tool".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            });
        }

        let provider = MockProvider::new(responses);
        let tools = ToolRegistry::new();
        let config = AgentConfig {
            timeout_seconds: 1,
            max_turns: 10000,
            ..Default::default()
        };

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Tool error test").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_token_usage_accumulation() {
        let usage1 = TokenUsage::new(100, 50);
        let _usage2 = TokenUsage::new(150, 75);

        let provider =
            MockProvider::with_token_usage(vec![Message::assistant("First response")], usage1);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Test token tracking").await;

        assert!(result.is_ok());

        // Check token usage was tracked
        let usage = agent.get_token_usage();
        assert!(usage.is_some());
        let u = usage.unwrap();
        assert_eq!(u.prompt_tokens, 100);
        assert_eq!(u.completion_tokens, 50);
        assert_eq!(u.total_tokens, 150);
    }

    #[tokio::test]
    async fn test_agent_context_info_with_provider_tokens() {
        let usage = TokenUsage::new(2000, 1000);
        let provider = MockProvider::with_token_usage(vec![Message::assistant("Response")], usage);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Test context").await;

        assert!(result.is_ok());

        // Get context info - should use accumulated agent usage
        let context = agent.get_context_info(8192);
        assert_eq!(context.max_tokens, 8192);
        assert_eq!(context.used_tokens, 3000); // prompt + completion from accumulated usage
        assert_eq!(context.remaining_tokens, 5192);
        assert!((context.percentage_used - 36.6).abs() < 0.1); // ~36.6%
    }

    #[tokio::test]
    async fn test_conversation_update_from_provider_usage() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Hello");

        let usage1 = TokenUsage::new(50, 25);
        conversation.update_from_provider_usage(&usage1);

        // Check usage is stored
        let stored = conversation.get_provider_token_usage();
        assert!(stored.is_some());
        let u = stored.unwrap();
        assert_eq!(u.prompt_tokens, 50);
        assert_eq!(u.completion_tokens, 25);

        // Add more usage
        let usage2 = TokenUsage::new(100, 50);
        conversation.update_from_provider_usage(&usage2);

        // Check usage accumulated
        let stored = conversation.get_provider_token_usage();
        assert!(stored.is_some());
        let u = stored.unwrap();
        assert_eq!(u.prompt_tokens, 150);
        assert_eq!(u.completion_tokens, 75);
        assert_eq!(u.total_tokens, 225);
    }

    #[test]
    fn test_context_info_creation() {
        let context = ContextInfo::new(8192, 2048);
        assert_eq!(context.max_tokens, 8192);
        assert_eq!(context.used_tokens, 2048);
        assert_eq!(context.remaining_tokens, 6144);
        assert!((context.percentage_used - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_context_info_full_context() {
        let context = ContextInfo::new(8192, 8192);
        assert_eq!(context.remaining_tokens, 0);
        assert!((context.percentage_used - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_context_info_overflow_handling() {
        // Used tokens exceeds max - should clamp to max
        let context = ContextInfo::new(8192, 10000);
        assert_eq!(context.used_tokens, 8192); // clamped
        assert_eq!(context.remaining_tokens, 0);
        assert!((context.percentage_used - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_conversation_get_context_info_with_heuristic() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Hello, world!");

        // Without provider usage, should use heuristic
        let context = conversation.get_context_info(8192);
        assert_eq!(context.max_tokens, 8192);
        assert!(context.used_tokens > 0); // heuristic counted something
        assert!(context.remaining_tokens < 8192);
    }

    #[test]
    fn test_conversation_get_context_info_prefers_provider() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Hello");

        // Add provider usage
        let usage = TokenUsage::new(100, 50);
        conversation.update_from_provider_usage(&usage);

        // Should use provider usage, not heuristic
        let context = conversation.get_context_info(8192);
        assert_eq!(context.max_tokens, 8192);
        assert_eq!(context.used_tokens, 150); // provider total
        assert_eq!(context.remaining_tokens, 8042);
    }

    #[test]
    fn test_conversation_clear_resets_usage() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Hello");

        let usage = TokenUsage::new(100, 50);
        conversation.update_from_provider_usage(&usage);

        // Verify usage is stored
        assert!(conversation.get_provider_token_usage().is_some());

        // Clear conversation
        conversation.clear();

        // Usage should be cleared
        assert!(conversation.get_provider_token_usage().is_none());
        assert_eq!(conversation.len(), 0);
    }

    #[tokio::test]
    async fn test_execute_with_observer_emits_prompt_started() {
        let provider = MockProvider::new(vec![Message::assistant("Hello!")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<String>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: crate::agent::events::AgentExecutionEvent) {
                self.events.push(format!("{:?}", event));
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("Hello", &token, &mut collector)
            .await;
        assert!(result.is_ok());
        assert!(collector.events.iter().any(|e| e.contains("PromptStarted")));
        assert!(collector
            .events
            .iter()
            .any(|e| e.contains("ExecutionCompleted")));
    }

    #[tokio::test]
    async fn test_execute_with_observer_respects_cancellation() {
        let provider = MockProvider::new(vec![Message::assistant("Hello!")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();

        let mut observer = crate::agent::events::NoOpObserver;
        let result = agent
            .execute_with_observer("Hello", &token, &mut observer)
            .await;
        assert!(matches!(result, Err(crate::error::XzatomaError::Cancelled)));
    }

    #[tokio::test]
    async fn test_execute_provider_messages_with_observer_emits_prompt_started() {
        let provider = MockProvider::new(vec![Message::assistant("Hello!")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<String>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: crate::agent::events::AgentExecutionEvent) {
                self.events.push(format!("{:?}", event));
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let messages = vec![crate::providers::Message::user("Hello")];
        let result = agent
            .execute_provider_messages_with_observer(messages, &token, &mut collector)
            .await;
        assert!(result.is_ok());
        assert!(collector.events.iter().any(|e| e.contains("PromptStarted")));
    }

    #[tokio::test]
    async fn test_agent_tools_mut_returns_mutable_registry() {
        struct MockTool;

        #[async_trait]
        impl crate::tools::ToolExecutor for MockTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({
                    "name": "mock_extra",
                    "description": "extra mock tool for tools_mut test",
                    "parameters": {"type": "object"}
                })
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                Ok(ToolResult::success("mock output".to_string()))
            }
        }

        let provider = MockProvider::new(vec![]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        assert_eq!(agent.num_tools(), 0, "registry should start empty");

        agent.tools_mut().register("mock_extra", Arc::new(MockTool));

        assert_eq!(
            agent.num_tools(),
            1,
            "registry should reflect the new tool after tools_mut registration"
        );
    }

    // -------------------------------------------------------------------------
    // combine_reasoning unit tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_combine_reasoning_both_some_concatenates() {
        let result = combine_reasoning(
            Some("raw reasoning".to_string()),
            Some("tag reasoning".to_string()),
        );
        assert_eq!(result, Some("raw reasoning\ntag reasoning".to_string()));
    }

    #[test]
    fn test_combine_reasoning_only_raw_returns_raw() {
        let result = combine_reasoning(Some("raw only".to_string()), None);
        assert_eq!(result, Some("raw only".to_string()));
    }

    #[test]
    fn test_combine_reasoning_only_tags_returns_tags() {
        let result = combine_reasoning(None, Some("tags only".to_string()));
        assert_eq!(result, Some("tags only".to_string()));
    }

    #[test]
    fn test_combine_reasoning_both_none_returns_none() {
        let result = combine_reasoning(None, None);
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // Reasoning event plumbing tests (execute_with_observer)
    // -------------------------------------------------------------------------

    /// A mock provider that returns a single response with an optional reasoning field.
    struct MockProviderWithReasoning {
        message: Message,
        reasoning: Option<String>,
        call_count: Arc<std::sync::Mutex<usize>>,
    }

    impl MockProviderWithReasoning {
        fn new(message: Message, reasoning: Option<String>) -> Self {
            Self {
                message,
                reasoning,
                call_count: Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl Provider for MockProviderWithReasoning {
        fn is_authenticated(&self) -> bool {
            false
        }

        fn current_model(&self) -> Option<&str> {
            None
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> Result<Vec<crate::providers::ModelInfo>> {
            Err(crate::error::XzatomaError::Provider(
                "not supported".to_string(),
            ))
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;
            drop(count);

            if idx == 0 {
                let mut response = CompletionResponse::new(self.message.clone());
                if let Some(r) = &self.reasoning {
                    response = response.set_reasoning(r.clone());
                }
                Ok(response)
            } else {
                Ok(CompletionResponse::new(Message::assistant("Done")))
            }
        }
    }

    #[tokio::test]
    async fn test_execute_with_observer_emits_reasoning_from_completion_response_field() {
        let provider = MockProviderWithReasoning::new(
            Message::assistant("final answer"),
            Some("structured chain-of-thought".to_string()),
        );
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("test", &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let reasoning_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ReasoningEmitted { .. }))
            .collect();
        assert_eq!(
            reasoning_events.len(),
            1,
            "expected exactly one ReasoningEmitted event"
        );
        if let AgentExecutionEvent::ReasoningEmitted { text } = &reasoning_events[0] {
            assert_eq!(text, "structured chain-of-thought");
        } else {
            panic!("expected ReasoningEmitted variant");
        }
    }

    #[tokio::test]
    async fn test_execute_with_observer_emits_reasoning_from_think_tags_in_content() {
        let provider = MockProviderWithReasoning::new(
            Message::assistant("<think>chain of thought</think>final answer"),
            None,
        );
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("test", &token, &mut collector)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "final answer");

        let reasoning_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ReasoningEmitted { .. }))
            .collect();
        assert_eq!(
            reasoning_events.len(),
            1,
            "expected exactly one ReasoningEmitted event"
        );
        if let AgentExecutionEvent::ReasoningEmitted { text } = &reasoning_events[0] {
            assert_eq!(text, "chain of thought");
        } else {
            panic!("expected ReasoningEmitted variant");
        }

        // The AssistantTextEmitted event must carry the clean text, not the raw tagged text.
        let text_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::AssistantTextEmitted { .. }))
            .collect();
        assert_eq!(text_events.len(), 1);
        if let AgentExecutionEvent::AssistantTextEmitted { text } = &text_events[0] {
            assert_eq!(text, "final answer");
        } else {
            panic!("expected AssistantTextEmitted variant");
        }
    }

    #[tokio::test]
    async fn test_execute_with_observer_does_not_store_tags_in_conversation() {
        let provider = MockProviderWithReasoning::new(
            Message::assistant("<think>hidden reasoning</think>clean response"),
            None,
        );
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        let token = tokio_util::sync::CancellationToken::new();
        let mut observer = crate::agent::events::NoOpObserver;
        let result = agent
            .execute_with_observer("test", &token, &mut observer)
            .await;
        assert!(result.is_ok());

        // No message in conversation history may contain a raw thinking tag.
        for msg in agent.conversation().messages() {
            if let Some(content) = &msg.content {
                assert!(
                    !content.contains("<think>"),
                    "conversation must not store raw thinking tags; found: {content}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_execute_with_observer_combines_raw_and_tag_reasoning() {
        let provider = MockProviderWithReasoning::new(
            Message::assistant("<think>from_tags</think>answer"),
            Some("raw_reasoning".to_string()),
        );
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("test", &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let reasoning_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ReasoningEmitted { .. }))
            .collect();
        assert_eq!(
            reasoning_events.len(),
            1,
            "expected exactly one ReasoningEmitted event"
        );
        if let AgentExecutionEvent::ReasoningEmitted { text } = &reasoning_events[0] {
            assert_eq!(text, "raw_reasoning\nfrom_tags");
        } else {
            panic!("expected ReasoningEmitted variant");
        }
    }

    #[tokio::test]
    async fn test_execute_provider_messages_with_observer_emits_reasoning() {
        let provider = MockProviderWithReasoning::new(
            Message::assistant("provider messages answer"),
            Some("provider messages reasoning".to_string()),
        );
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let messages = vec![crate::providers::Message::user("test")];
        let result = agent
            .execute_provider_messages_with_observer(messages, &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let reasoning_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ReasoningEmitted { .. }))
            .collect();
        assert_eq!(
            reasoning_events.len(),
            1,
            "expected exactly one ReasoningEmitted event"
        );
        if let AgentExecutionEvent::ReasoningEmitted { text } = &reasoning_events[0] {
            assert_eq!(text, "provider messages reasoning");
        } else {
            panic!("expected ReasoningEmitted variant");
        }
    }

    // -------------------------------------------------------------------------
    // ContextWindowUpdated event plumbing tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_execute_with_observer_emits_context_window_updated_on_provider_response() {
        let usage = TokenUsage::new(100, 50);
        let provider = MockProvider::with_token_usage(vec![Message::assistant("done")], usage);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("test context window", &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let cw_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ContextWindowUpdated { .. }))
            .collect();
        assert_eq!(
            cw_events.len(),
            1,
            "expected exactly one ContextWindowUpdated event"
        );
        if let AgentExecutionEvent::ContextWindowUpdated {
            used_tokens,
            max_tokens,
        } = &cw_events[0]
        {
            // Provider reported 100 prompt + 50 completion = 150 total
            assert!(
                *used_tokens >= 150,
                "expected used_tokens >= 150, got {used_tokens}"
            );
            // Default AgentConfig uses ConversationConfig::default() max_tokens = 100_000
            assert_eq!(*max_tokens, 100_000u64);
        } else {
            panic!("expected ContextWindowUpdated variant");
        }
    }

    #[tokio::test]
    async fn test_execute_provider_messages_with_observer_emits_context_window_updated() {
        let usage = TokenUsage::new(200, 100);
        let provider = MockProvider::with_token_usage(vec![Message::assistant("done")], usage);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let messages = vec![crate::providers::Message::user("test")];
        let result = agent
            .execute_provider_messages_with_observer(messages, &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let cw_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ContextWindowUpdated { .. }))
            .collect();
        assert_eq!(
            cw_events.len(),
            1,
            "expected exactly one ContextWindowUpdated event"
        );
        if let AgentExecutionEvent::ContextWindowUpdated {
            used_tokens,
            max_tokens,
        } = &cw_events[0]
        {
            // Provider reported 200 prompt + 100 completion = 300 total
            assert!(
                *used_tokens >= 300,
                "expected used_tokens >= 300, got {used_tokens}"
            );
            assert_eq!(*max_tokens, 100_000u64);
        } else {
            panic!("expected ContextWindowUpdated variant");
        }
    }

    #[tokio::test]
    async fn test_context_window_updated_uses_heuristic_when_no_provider_usage() {
        // Provider returns no usage data; heuristic token count must still fire.
        let provider = MockProvider::new(vec![Message::assistant("done")]);
        let tools = ToolRegistry::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(provider, tools, config).unwrap();

        struct EventCollector {
            events: Vec<AgentExecutionEvent>,
        }
        impl crate::agent::events::AgentObserver for EventCollector {
            fn on_event(&mut self, event: AgentExecutionEvent) {
                self.events.push(event);
            }
        }

        let token = tokio_util::sync::CancellationToken::new();
        let mut collector = EventCollector { events: Vec::new() };
        let result = agent
            .execute_with_observer("test heuristic fallback", &token, &mut collector)
            .await;
        assert!(result.is_ok());

        let cw_events: Vec<_> = collector
            .events
            .iter()
            .filter(|e| matches!(e, AgentExecutionEvent::ContextWindowUpdated { .. }))
            .collect();
        assert_eq!(
            cw_events.len(),
            1,
            "ContextWindowUpdated must fire even when provider reports no usage"
        );
        if let AgentExecutionEvent::ContextWindowUpdated {
            used_tokens,
            max_tokens,
        } = &cw_events[0]
        {
            assert!(
                *used_tokens > 0,
                "expected used_tokens > 0 from heuristic, got {used_tokens}"
            );
            assert_eq!(*max_tokens, 100_000u64);
        } else {
            panic!("expected ContextWindowUpdated variant");
        }
    }
}
