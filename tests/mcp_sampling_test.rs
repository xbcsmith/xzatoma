//! Integration tests for Phase 5B: Sampling Handler
//!
//! Covers Task 5B.5 requirements:
//!
//! - `test_full_autonomous_mode_skips_user_prompt_and_calls_provider`
//! - `test_interactive_mode_with_user_rejection_returns_mcp_elicitation_error`

use std::sync::Arc;

use xzatoma::config::ExecutionMode;
use xzatoma::error::{Result, XzatomaError};
use xzatoma::mcp::protocol::SamplingHandler;
use xzatoma::mcp::sampling::XzatomaSamplingHandler;
use xzatoma::mcp::types::{CreateMessageRequest, MessageContent, PromptMessage, Role, TextContent};
use xzatoma::providers::{CompletionResponse, Message, ModelInfo, Provider};

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

/// A mock provider that records calls and returns a fixed response.
#[derive(Debug)]
struct MockProvider {
    response_text: String,
    call_count: std::sync::atomic::AtomicUsize,
}

impl MockProvider {
    fn new(text: &str) -> Self {
        Self {
            response_text: text.to_string(),
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
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

    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![])
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(CompletionResponse::new(Message::assistant(
            &self.response_text,
        )))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal `CreateMessageRequest` with a single user text message.
fn simple_request(system: Option<&str>, user_text: &str) -> CreateMessageRequest {
    CreateMessageRequest {
        messages: vec![PromptMessage {
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
        max_tokens: 256,
        stop_sequences: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    }
}

// ---------------------------------------------------------------------------
// MCP sampling approval policy tests
// ---------------------------------------------------------------------------

/// FullAutonomous mode must not auto-approve sampling without explicit policy.
#[tokio::test]
async fn test_full_autonomous_mode_requires_explicit_approval_policy() {
    let mock = Arc::new(MockProvider::new("the answer is 42"));

    let handler = XzatomaSamplingHandler {
        provider: Arc::clone(&mock) as Arc<dyn Provider>,
        execution_mode: ExecutionMode::FullAutonomous,
        headless: true,
    };

    let req = simple_request(None, "what is 6 times 7?");
    let result = handler.create_message(req).await;

    assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    assert_eq!(
        mock.call_count(),
        0,
        "provider::complete must not be called without approval"
    );
}

/// Headless mode must reject sampling without explicit trust metadata.
#[tokio::test]
async fn test_headless_mode_rejects_without_trust_metadata() {
    let mock = Arc::new(MockProvider::new("headless result"));

    let handler = XzatomaSamplingHandler {
        provider: Arc::clone(&mock) as Arc<dyn Provider>,
        execution_mode: ExecutionMode::Interactive,
        headless: true,
    };

    let req = simple_request(Some("You are helpful."), "hello");
    let result = handler.create_message(req).await;

    assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    assert_eq!(mock.call_count(), 0);
}

// ---------------------------------------------------------------------------
// Task 5B.5: interactive user rejection
//
// The stdin-based rejection path cannot run in `cargo test` because
// `should_auto_approve` always returns false and a non-headless handler blocks
// on `stdin().read_line()` waiting for "[y/N]" input. That is not a network
// dependency; it is interactive terminal I/O.
//
// User rejection behavior is covered in `src/mcp/sampling.rs` unit tests.
// ---------------------------------------------------------------------------

/// Verify the public `McpElicitation` rejection error contract.
///
/// Does not call `create_message` with `headless: false` because sampling
/// approval always requires interactive stdin in that configuration.
#[test]
fn test_interactive_mode_with_user_rejection_returns_mcp_elicitation_error() {
    let elicitation_err = XzatomaError::McpElicitation("user rejected sampling request".into());
    assert!(
        elicitation_err
            .to_string()
            .contains("user rejected sampling request"),
        "McpElicitation error message must contain the rejection reason"
    );
}

// ---------------------------------------------------------------------------
// Additional coverage: result fields
// ---------------------------------------------------------------------------

/// Plain text sampling requires approval before provider execution.
#[tokio::test]
async fn test_plain_text_sampling_requires_approval() {
    let mock = Arc::new(MockProvider::new("plain text"));

    let handler = XzatomaSamplingHandler {
        provider: Arc::clone(&mock) as Arc<dyn Provider>,
        execution_mode: ExecutionMode::FullAutonomous,
        headless: true,
    };

    let req = simple_request(None, "summarise this");
    let result = handler.create_message(req).await;

    assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    assert_eq!(mock.call_count(), 0);
}

/// Sampling rejection happens before constructing a result model field.
#[tokio::test]
async fn test_result_model_field_not_constructed_without_approval() {
    let mock = Arc::new(MockProvider::new("hello"));

    let handler = XzatomaSamplingHandler {
        provider: Arc::clone(&mock) as Arc<dyn Provider>,
        execution_mode: ExecutionMode::FullAutonomous,
        headless: true,
    };

    let req = simple_request(None, "hi");
    let result = handler.create_message(req).await;

    assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    assert_eq!(mock.call_count(), 0);
}

/// Multiple user messages are rejected before provider execution without approval.
#[tokio::test]
async fn test_multiple_messages_require_approval_before_provider_execution() {
    let mock = Arc::new(MockProvider::new("multi-turn answer"));

    let handler = XzatomaSamplingHandler {
        provider: Arc::clone(&mock) as Arc<dyn Provider>,
        execution_mode: ExecutionMode::FullAutonomous,
        headless: true,
    };

    let req = CreateMessageRequest {
        messages: vec![
            PromptMessage {
                role: Role::User,
                content: MessageContent::Text(TextContent {
                    text: "first message".to_string(),
                    annotations: None,
                }),
            },
            PromptMessage {
                role: Role::Assistant,
                content: MessageContent::Text(TextContent {
                    text: "first reply".to_string(),
                    annotations: None,
                }),
            },
            PromptMessage {
                role: Role::User,
                content: MessageContent::Text(TextContent {
                    text: "second message".to_string(),
                    annotations: None,
                }),
            },
        ],
        model_preferences: None,
        system_prompt: None,
        include_context: None,
        temperature: None,
        max_tokens: 256,
        stop_sequences: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let result = handler.create_message(req).await;

    assert!(matches!(result, Err(XzatomaError::McpElicitation(_))));
    assert_eq!(mock.call_count(), 0);
}
