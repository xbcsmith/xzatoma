//! MCP sampling handler -- forwards server-initiated LLM sampling to Xzatoma's
//! configured [`Provider`][crate::providers::Provider].
//!
//! When an MCP server sends a `sampling/createMessage` request it is asking the
//! client (Xzatoma) to run an LLM inference on its behalf. This module
//! implements the [`SamplingHandler`][crate::mcp::protocol::SamplingHandler]
//! trait by:
//!
//! 1. Optionally prompting the user for confirmation (unless auto-approved).
//! 2. Converting the MCP [`CreateMessageRequest`] to the provider [`Message`]
//!    format.
//! 3. Delegating to the configured provider via
//!    [`Provider::complete`][crate::providers::Provider::complete].
//! 4. Mapping the [`CompletionResponse`] back to a
//!    [`CreateMessageResult`].
//!
//! # Approval Policy
//!
//! All approval decisions are delegated to
//! [`crate::mcp::approval::should_auto_approve`]. No inline policy checks are
//! permitted here.

use std::io::Write;
use std::sync::Arc;

use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};
use crate::mcp::approval::should_auto_approve;
use crate::mcp::client::BoxFuture;
use crate::mcp::protocol::SamplingHandler;
use crate::mcp::types::{
    CreateMessageRequest, CreateMessageResult, MessageContent, Role, TextContent,
};
use crate::providers::{Message, Provider};

// ---------------------------------------------------------------------------
// XzatomaSamplingHandler
// ---------------------------------------------------------------------------

/// MCP sampling handler that forwards `sampling/createMessage` requests to
/// Xzatoma's configured AI provider.
///
/// When a connected MCP server issues a `sampling/createMessage` request, this
/// handler:
///
/// 1. Checks the auto-approval policy via
///    [`should_auto_approve`][crate::mcp::approval::should_auto_approve]; if
///    approval is required it prompts on stderr and reads one line from stdin.
/// 2. Converts the MCP [`CreateMessageRequest`] messages to provider
///    [`Message`][crate::providers::Message] values.
/// 3. Prepends `system_prompt` as a `system` role message when present.
/// 4. Calls [`Provider::complete`][crate::providers::Provider::complete] with
///    no tool definitions (sampling calls are plain text completions).
/// 5. Maps the [`CompletionResponse`] back to a [`CreateMessageResult`].
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::sampling::XzatomaSamplingHandler;
/// use xzatoma::mcp::protocol::SamplingHandler;
///
/// // The handler requires a real Provider implementation to call complete().
/// // In tests, use a mock provider (see tests module below).
/// ```
/// MCP sampling handler that forwards `sampling/createMessage` requests to
/// Xzatoma's configured AI provider.
///
/// `Debug` is implemented manually because `Arc<dyn Provider>` does not
/// implement `Debug` in the general case.
pub struct XzatomaSamplingHandler {
    /// The AI provider to forward sampling requests to.
    pub provider: Arc<dyn Provider>,
    /// Agent execution mode, used by the approval policy.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
}

impl std::fmt::Debug for XzatomaSamplingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XzatomaSamplingHandler")
            .field("execution_mode", &self.execution_mode)
            .field("headless", &self.headless)
            .finish_non_exhaustive()
    }
}

impl SamplingHandler for XzatomaSamplingHandler {
    /// Handle a `sampling/createMessage` request from a connected MCP server.
    ///
    /// Converts the MCP message format to provider messages, runs a completion,
    /// and returns the result as a [`CreateMessageResult`].
    ///
    /// # Arguments
    ///
    /// * `params` - The sampling parameters sent by the MCP server.
    ///
    /// # Returns
    ///
    /// Returns [`CreateMessageResult`] containing the provider's response text.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpElicitation`] if the user rejects the
    /// sampling request in interactive mode.
    ///
    /// Returns any error propagated from the provider's `complete` call.
    fn create_message<'a>(
        &'a self,
        params: CreateMessageRequest,
    ) -> BoxFuture<'a, Result<CreateMessageResult>> {
        Box::pin(async move {
            // Step 1: Interactive confirmation check.
            if !should_auto_approve(self.execution_mode, self.headless) {
                if self.headless {
                    return Err(XzatomaError::McpElicitation(
                        "MCP sampling requires explicit server trust metadata in headless mode"
                            .into(),
                    ));
                }

                eprint!(
                    "MCP server requests LLM sampling. System prompt: {:?}. \
                     Max tokens: {}. Allow? [y/N] ",
                    params.system_prompt, params.max_tokens
                );
                if let Err(error) = std::io::stderr().flush() {
                    tracing::warn!(%error, "Failed to flush MCP sampling approval prompt");
                }

                let mut line = String::new();
                std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line).map_err(
                    |e| {
                        XzatomaError::Tool(format!(
                            "Failed to read sampling approval from stdin: {}",
                            e
                        ))
                    },
                )?;

                if !matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
                    return Err(XzatomaError::McpElicitation(
                        "user rejected sampling request".into(),
                    ));
                }
            }

            // Step 2: Convert MCP messages to provider Message format.
            let mut messages: Vec<Message> = params
                .messages
                .iter()
                .filter_map(|msg| {
                    let text = match &msg.content {
                        MessageContent::Text(t) => t.text.clone(),
                        // Skip non-text content items for sampling.
                        _ => return None,
                    };
                    let role = match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    };
                    Some(Message {
                        role: role.to_string(),
                        content: Some(text),
                        content_parts: None,
                        tool_calls: None,
                        tool_call_id: None,
                    })
                })
                .collect();

            // Step 3: Prepend system_prompt as a system message if present.
            if let Some(ref system) = params.system_prompt {
                if !system.is_empty() {
                    messages.insert(0, Message::system(system));
                }
            }

            // Ensure there is at least one message so the provider call is
            // meaningful.
            if messages.is_empty() {
                return Err(XzatomaError::Mcp(
                    "sampling/createMessage: no usable text content in request messages".into(),
                ));
            }

            // Step 4: Call the provider with no tool definitions.
            let response = self.provider.complete(&messages, &[]).await?;

            // Step 5: Map the CompletionResponse to CreateMessageResult.
            let text = response.message.content.unwrap_or_default();

            // Determine stop_reason: "toolUse" when the response contains tool
            // calls, "endTurn" otherwise.
            let stop_reason = if response
                .message
                .tool_calls
                .as_ref()
                .is_some_and(|tc| !tc.is_empty())
            {
                Some("toolUse".to_string())
            } else {
                Some("endTurn".to_string())
            };

            let model = response.model.unwrap_or_else(|| "unknown".to_string());

            Ok(CreateMessageResult {
                role: Role::Assistant,
                content: MessageContent::Text(TextContent {
                    text,
                    annotations: None,
                }),
                model,
                stop_reason,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::providers::{CompletionResponse, Message as ProviderMessage, Provider};

    // ------------------------------------------------------------------
    // Mock provider
    // ------------------------------------------------------------------

    /// A mock provider that returns a fixed response string.
    #[derive(Debug)]
    struct MockProvider {
        response_text: String,
    }

    impl MockProvider {
        fn new(text: &str) -> Self {
            Self {
                response_text: text.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        fn is_authenticated(&self) -> bool {
            false
        }

        fn current_model(&self) -> Option<&str> {
            Some("mock-model")
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> Result<Vec<crate::providers::ModelInfo>> {
            Ok(vec![])
        }

        async fn complete(
            &self,
            _messages: &[ProviderMessage],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            Ok(CompletionResponse::new(ProviderMessage::assistant(
                &self.response_text,
            )))
        }
    }

    // ------------------------------------------------------------------
    // Helper
    // ------------------------------------------------------------------

    fn make_handler(
        execution_mode: ExecutionMode,
        headless: bool,
        response_text: &str,
    ) -> XzatomaSamplingHandler {
        XzatomaSamplingHandler {
            provider: Arc::new(MockProvider::new(response_text)),
            execution_mode,
            headless,
        }
    }

    fn simple_request(system: Option<&str>, user_text: &str) -> CreateMessageRequest {
        CreateMessageRequest {
            messages: vec![crate::mcp::types::PromptMessage {
                role: Role::User,
                content: MessageContent::Text(TextContent {
                    text: user_text.to_string(),
                    annotations: None,
                }),
            }],
            model_preferences: None,
            system_prompt: system.map(|s| s.to_string()),
            include_context: None,
            temperature: None,
            max_tokens: 512,
            stop_sequences: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        }
    }

    // ------------------------------------------------------------------
    // Tests
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_headless_mode_rejects_without_trust_metadata() {
        let handler = make_handler(ExecutionMode::FullAutonomous, true, "provider reply");

        let req = simple_request(None, "what is 2+2?");
        let result = handler.create_message(req).await;

        assert!(matches!(
            result,
            Err(XzatomaError::McpElicitation(message)) if message.contains("headless")
        ));
    }

    #[tokio::test]
    async fn test_headless_mode_rejects_prompt_without_trust_metadata() {
        let handler = make_handler(ExecutionMode::Interactive, true, "headless reply");

        let req = simple_request(Some("You are a test assistant."), "ping");
        let result = handler.create_message(req).await;

        assert!(matches!(
            result,
            Err(XzatomaError::McpElicitation(message)) if message.contains("headless")
        ));
    }

    #[tokio::test]
    async fn test_system_prompt_request_requires_interactive_approval() {
        let handler = make_handler(ExecutionMode::FullAutonomous, true, "ok");

        let req = simple_request(Some("System context here."), "user question");
        let result = handler.create_message(req).await;

        assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    }

    #[tokio::test]
    async fn test_empty_messages_returns_error() {
        // An empty messages list in headless mode still fails before provider
        // execution rather than passing an empty slice to the provider.
        let handler = make_handler(ExecutionMode::FullAutonomous, true, "ok");

        let req = CreateMessageRequest {
            messages: vec![],
            model_preferences: None,
            system_prompt: None,
            include_context: None,
            temperature: None,
            max_tokens: 100,
            stop_sequences: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };

        let result = handler.create_message(req).await;
        assert!(
            result.is_err(),
            "empty messages must produce an error, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_sampling_request_does_not_auto_approve_in_full_autonomous() {
        let handler = make_handler(ExecutionMode::FullAutonomous, true, "hello");
        let req = simple_request(None, "question");
        let result = handler.create_message(req).await;

        assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    }
}
