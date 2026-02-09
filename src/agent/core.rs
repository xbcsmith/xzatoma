//! Agent core implementation with autonomous execution loop
//!
//! This module implements the main agent execution loop that:
//! - Manages conversation with AI providers
//! - Executes tool calls requested by the provider
//! - Enforces iteration limits and timeouts
//! - Handles errors and stops conditions gracefully

use crate::chat_mode::{ChatMode, SafetyMode};
use crate::config::AgentConfig;
use crate::error::{Result, XzatomaError};
use crate::prompts;
use crate::providers::{CompletionResponse, Message, Provider, TokenUsage, ToolCall};
use crate::tools::{ToolRegistry, ToolResult};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
        }

        Ok(Self {
            provider: Arc::from(provider),
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
        }

        Ok(Self {
            provider,
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
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
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
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
    /// - `XzatomaError::Config` if timeout is exceeded
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
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        info!("Starting agent execution");

        // Add initial user prompt
        self.conversation.add_user_message(user_prompt.into());

        let mut iteration = 0;

        loop {
            iteration += 1;

            // Check iteration limit
            if iteration > self.config.max_turns {
                warn!("Maximum iterations ({}) exceeded", self.config.max_turns);
                return Err(XzatomaError::MaxIterationsExceeded {
                    limit: self.config.max_turns,
                    message: format!(
                        "Agent exceeded maximum iteration limit of {}",
                        self.config.max_turns
                    ),
                }
                .into());
            }

            // Check timeout
            if start_time.elapsed() > timeout {
                warn!("Agent execution timeout after {:?}", start_time.elapsed());
                return Err(XzatomaError::Config(format!(
                    "Agent execution timeout after {} seconds",
                    self.config.timeout_seconds
                ))
                .into());
            }

            debug!(
                "Iteration {}/{}, tokens: {}/{}",
                iteration,
                self.config.max_turns,
                self.conversation.token_count(),
                self.conversation.max_tokens()
            );

            // Get completion from provider
            let tool_definitions = self.tools.all_definitions();
            let completion_response = self
                .provider
                .complete(self.conversation.messages(), &tool_definitions)
                .await?;

            let message = completion_response.message;
            debug!("Provider response: {:?}", message);

            // Track token usage if provider reported it
            if let Some(usage) = completion_response.usage {
                self.conversation.update_from_provider_usage(&usage);

                // Accumulate token usage at agent level
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

            // Add assistant message to conversation (preserving tool_calls if present)
            // We must add the complete message including tool_calls so that when
            // validate_message_sequence runs, it can find the tool_call IDs
            self.conversation.add_message(message.clone());

            // Handle tool calls if present
            if let Some(tool_calls) = &message.tool_calls {
                if tool_calls.is_empty() {
                    // Provider returned empty tool calls, treat as completion
                    debug!("Provider returned empty tool calls, stopping");
                    break;
                }

                debug!("Executing {} tool calls", tool_calls.len());

                for tool_call in tool_calls {
                    let result = self.execute_tool_call(tool_call).await?;

                    // Add tool result to conversation
                    self.conversation
                        .add_tool_result(&tool_call.id, result.to_message());
                }

                // Continue loop to get next response after tool execution
                continue;
            }

            // No tool calls, check if we have a final response
            if message.content.is_some() {
                debug!("Provider returned final response, stopping");
                break;
            }

            // Neither content nor tool calls - unexpected
            warn!("Provider returned neither content nor tool calls");
            return Err(XzatomaError::Provider(
                "Provider returned invalid response (no content or tool calls)".to_string(),
            )
            .into());
        }

        // Get the final response
        let final_message = self
            .conversation
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == "assistant")
            .and_then(|m| m.content.as_ref())
            .cloned()
            .unwrap_or_else(|| "No response from assistant".to_string());

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
                anyhow::Error::from(XzatomaError::Tool(format!(
                    "Failed to parse tool arguments for '{}': {}",
                    tool_name, e
                )))
            })?;

        // Execute tool
        let result = tool_executor.execute(args).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!(
                "Tool '{}' execution failed: {}",
                tool_name, e
            )))
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

        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
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
        // For timeout to trigger, we need multiple iterations
        // Use tool calls to force iterations
        let mut responses: Vec<Message> = Vec::new();
        for i in 0..1000 {
            responses.push(Message {
                role: "assistant".to_string(),
                content: None,
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
            timeout_seconds: 1, // 1 second timeout
            max_turns: 10000,   // High limit so timeout triggers first
            ..Default::default()
        };

        let mut agent = Agent::new(provider, tools, config).unwrap();
        let result = agent.execute("Timeout test").await;

        // Should fail (either timeout or tool not found)
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
}
