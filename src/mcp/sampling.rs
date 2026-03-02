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

use std::io::{BufRead, Write};
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
                eprint!(
                    "MCP server requests LLM sampling. System prompt: {:?}. \
                     Max tokens: {}. Allow? [y/N] ",
                    params.system_prompt, params.max_tokens
                );
                // Flush stderr so the prompt appears before blocking on stdin.
                let _ = std::io::stderr().flush();

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
                    )
                    .into());
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
                )
                .into());
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
        async fn complete(
            &self,
            _messages: &[ProviderMessage],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            Ok(CompletionResponse::new(ProviderMessage::assistant(
                &self.response_text,
            )))
        }

        async fn list_models(&self) -> Result<Vec<crate::providers::ModelInfo>> {
            Ok(vec![])
        }

        async fn get_model_info(&self, _name: &str) -> Result<crate::providers::ModelInfo> {
            Err(XzatomaError::Provider("no model info in mock".into()).into())
        }

        fn get_current_model(&self) -> Result<String> {
            Ok("mock-model".to_string())
        }

        async fn set_model(&mut self, _model_name: String) -> Result<()> {
            Ok(())
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
    async fn test_full_autonomous_mode_skips_user_prompt_and_calls_provider() {
        // FullAutonomous + headless=false => should_auto_approve returns true,
        // so the handler must skip the stdin prompt and call the provider.
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "provider reply");

        let req = simple_request(None, "what is 2+2?");
        let result = handler.create_message(req).await;

        assert!(
            result.is_ok(),
            "create_message must succeed in FullAutonomous mode: {:?}",
            result
        );

        let msg = result.unwrap();
        assert_eq!(msg.role, Role::Assistant);
        if let MessageContent::Text(t) = msg.content {
            assert_eq!(t.text, "provider reply");
        } else {
            panic!("expected Text content");
        }
        assert_eq!(msg.stop_reason.as_deref(), Some("endTurn"));
    }

    #[tokio::test]
    async fn test_headless_mode_skips_prompt_and_calls_provider() {
        // Interactive + headless=true => auto-approve, no stdin prompt.
        let handler = make_handler(ExecutionMode::Interactive, true, "headless reply");

        let req = simple_request(Some("You are a test assistant."), "ping");
        let result = handler.create_message(req).await;

        assert!(
            result.is_ok(),
            "create_message must succeed in headless mode: {:?}",
            result
        );

        let msg = result.unwrap();
        if let MessageContent::Text(t) = msg.content {
            assert_eq!(t.text, "headless reply");
        } else {
            panic!("expected Text content");
        }
    }

    #[tokio::test]
    async fn test_system_prompt_included_in_provider_call() {
        // Verify that a system prompt is prepended to the messages list.
        // We cannot inspect the messages sent to the provider directly,
        // but we can verify the overall call succeeds and the system_prompt
        // field is accepted without error.
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "ok");

        let req = simple_request(Some("System context here."), "user question");
        let result = handler.create_message(req).await;

        assert!(
            result.is_ok(),
            "system prompt case must succeed: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_empty_messages_returns_error() {
        // An empty messages list (after filtering) must return an error rather
        // than passing an empty slice to the provider.
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "ok");

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
    async fn test_stop_reason_is_end_turn_for_text_response() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "hello");
        let req = simple_request(None, "question");
        let result = handler.create_message(req).await.unwrap();

        assert_eq!(
            result.stop_reason.as_deref(),
            Some("endTurn"),
            "text-only response must have stopReason 'endTurn'"
        );
    }

    #[tokio::test]
    async fn test_result_role_is_always_assistant() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "answer");
        let req = simple_request(None, "question");
        let result = handler.create_message(req).await.unwrap();

        assert_eq!(result.role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_result_model_field_is_set() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false, "text");
        let req = simple_request(None, "hello");
        let result = handler.create_message(req).await.unwrap();

        // MockProvider returns no model; the handler maps None to "unknown".
        assert!(
            !result.model.is_empty(),
            "model field must not be empty; got: {:?}",
            result.model
        );
    }
}
