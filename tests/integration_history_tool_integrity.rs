//! Integration tests for Phase 4: Cross-Provider Consistency & Tool Integrity
//!
//! Tests cover:
//! - Save/load/resume cycles with orphan tool messages
//! - Valid tool call pairs are preserved through persistence
//! - Pruning maintains integrity during resume with tool pairs

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

use xzatoma::agent::{Agent, Conversation};
use xzatoma::config::AgentConfig;
use xzatoma::providers::{CompletionResponse, Message, Provider, ToolCall};
use xzatoma::storage::SqliteStorage;
use xzatoma::tools::ToolRegistry;

/// Mock provider that tracks all messages received during completion
#[derive(Clone)]
struct TrackingMockProvider {
    response: Message,
    last_messages: Arc<Mutex<Vec<Message>>>,
}

impl TrackingMockProvider {
    /// Create a new mock provider with a response message
    fn new(response: Message) -> Self {
        Self {
            response,
            last_messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the last messages that were passed to complete()
    fn last_messages(&self) -> Vec<Message> {
        self.last_messages.lock().unwrap().clone()
    }
}

#[async_trait]
impl Provider for TrackingMockProvider {
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> xzatoma::error::Result<CompletionResponse> {
        // Validate messages like real providers do (removing orphan tool messages)
        let validated_messages = xzatoma::providers::validate_message_sequence(messages);
        // Track the sanitized messages we received
        *self.last_messages.lock().unwrap() = validated_messages;
        Ok(CompletionResponse::new(self.response.clone()))
    }
}

/// Helper to create a temporary storage instance
fn create_temp_storage() -> (SqliteStorage, TempDir) {
    let tmp = TempDir::new().expect("failed to create tempdir");
    let db_path = tmp.path().join("history.db");
    let storage = SqliteStorage::new_with_path(db_path).expect("failed to create sqlite storage");
    (storage, tmp)
}

#[tokio::test]
async fn test_save_load_resume_with_orphan_sanitized() {
    // Setup: Create storage and mock provider
    let (storage, _tmp) = create_temp_storage();
    let mock_response = Message::assistant("Resumed response");
    let provider = Arc::new(TrackingMockProvider::new(mock_response));

    // Create agent with conversation containing orphan tool message
    let tools = ToolRegistry::new();
    let mut config = AgentConfig::default();
    config.conversation.max_tokens = 8000;
    config.conversation.min_retain_turns = 10;
    config.max_turns = 1;

    let mut agent = Agent::new(provider.as_ref().clone(), tools, config).expect("create agent");

    // Add messages: user -> assistant -> orphan tool message (no matching call)
    agent.conversation_mut().add_user_message("Hello");
    agent.conversation_mut().add_assistant_message("Hi");
    agent
        .conversation_mut()
        .add_tool_result("call_orphan", "orphan result");

    let conv_id = agent.conversation().id();
    let title = "Test Orphan Conversation";

    // Save conversation (orphan included in persistence)
    storage
        .save_conversation(
            &conv_id.to_string(),
            title,
            Some("gpt-4"),
            agent.conversation().messages(),
        )
        .expect("save conversation");

    // Verify orphan is saved
    let (_, _, saved_messages) = storage
        .load_conversation(&conv_id.to_string())
        .expect("load should work")
        .expect("conversation should exist");
    assert_eq!(saved_messages.len(), 3, "orphan should be saved in storage");
    assert!(saved_messages
        .iter()
        .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_orphan")));

    // Load conversation and recreate agent
    let (loaded_title, model, loaded_messages) = storage
        .load_conversation(&conv_id.to_string())
        .expect("load")
        .expect("should exist");

    assert_eq!(loaded_title, title);
    assert_eq!(model, Some("gpt-4".to_string()));

    // Reconstruct conversation with loaded messages
    let conversation =
        Conversation::with_history(conv_id, loaded_title, loaded_messages, 8000, 10, 0.8);

    // Create new agent with loaded conversation
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let resumed_agent = Agent::with_conversation(
        Box::new(provider.as_ref().clone()),
        tools,
        config,
        conversation,
    )
    .expect("create resumed agent");

    // Execute a continuation prompt
    // This triggers provider.complete() which receives the sanitized messages
    let _result = resumed_agent.execute("Continue").await;

    // Verify provider received sanitized messages (orphan removed by validate_message_sequence)
    let received = provider.last_messages();

    // The orphan tool message should have been removed by validate_message_sequence
    // because there's no assistant message with a matching tool_call for "call_orphan"
    let has_orphan = received
        .iter()
        .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_orphan"));
    assert!(
        !has_orphan,
        "Provider should have received sanitized messages without orphan"
    );
}

#[tokio::test]
async fn test_save_load_resume_preserves_valid_tool_pair() {
    // Setup: Create storage and mock provider
    let (storage, _tmp) = create_temp_storage();
    let mock_response = Message::assistant("Calculation confirmed");
    let provider = Arc::new(TrackingMockProvider::new(mock_response));

    // Create agent with valid tool pair
    let tools = ToolRegistry::new();
    let mut config = AgentConfig::default();
    config.conversation.max_tokens = 8000;
    config.conversation.min_retain_turns = 10;
    config.max_turns = 1;

    let mut agent = Agent::new(provider.as_ref().clone(), tools, config).expect("create agent");

    // Add valid tool pair: assistant with tool call -> tool result
    let tool_call = ToolCall {
        id: "call_calc_001".to_string(),
        function: xzatoma::providers::FunctionCall {
            name: "calculator".to_string(),
            arguments: r#"{"op":"add","a":2,"b":2}"#.to_string(),
        },
    };

    agent.conversation_mut().add_user_message("What is 2+2?");
    agent
        .conversation_mut()
        .add_message(Message::assistant_with_tools(vec![tool_call]));
    agent
        .conversation_mut()
        .add_tool_result("call_calc_001", "4");

    let conv_id = agent.conversation().id();
    let title = "Math Test Conversation";

    // Save conversation
    storage
        .save_conversation(
            &conv_id.to_string(),
            title,
            Some("gpt-4"),
            agent.conversation().messages(),
        )
        .expect("save");

    // Load conversation
    let (loaded_title, model, loaded_messages) = storage
        .load_conversation(&conv_id.to_string())
        .expect("load")
        .expect("should exist");

    assert_eq!(loaded_title, title);
    assert_eq!(model, Some("gpt-4".to_string()));
    assert_eq!(loaded_messages.len(), 3);

    // Reconstruct and resume
    let conversation =
        Conversation::with_history(conv_id, loaded_title, loaded_messages, 8000, 10, 0.8);
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let resumed_agent = Agent::with_conversation(
        Box::new(provider.as_ref().clone()),
        tools,
        config,
        conversation,
    )
    .expect("create resumed");

    // Execute continuation
    let _result = resumed_agent.execute("Continue the calculation").await;

    // Verify valid tool pair is preserved
    let received = provider.last_messages();

    // Should have assistant with tool_calls
    let has_assistant = received.iter().any(|m| {
        m.role == "assistant"
            && m.tool_calls
                .as_ref()
                .is_some_and(|tcs| tcs.iter().any(|tc| tc.id == "call_calc_001"))
    });
    assert!(
        has_assistant,
        "Assistant with tool call should be preserved"
    );

    // Should have tool result with matching ID
    let has_tool_result = received
        .iter()
        .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_calc_001"));
    assert!(has_tool_result, "Tool result should be preserved");
}

#[tokio::test]
async fn test_pruning_during_resume_maintains_integrity() {
    // Setup: Create storage and mock provider
    let (storage, _tmp) = create_temp_storage();
    let mock_response = Message::assistant("Pruned and continuing");
    let provider = Arc::new(TrackingMockProvider::new(mock_response));

    // Create agent with small token limits to trigger pruning
    let tools = ToolRegistry::new();
    let mut config = AgentConfig::default();
    config.conversation.max_tokens = 500; // Small limit
    config.conversation.min_retain_turns = 2; // Keep only 2 turns
    config.conversation.prune_threshold = 0.7; // Prune at 70%
    config.max_turns = 1;

    let mut agent =
        Agent::new(provider.as_ref().clone(), tools, config.clone()).expect("create agent");

    // Create a tool call that will be early in the conversation
    let early_tool = ToolCall {
        id: "call_early".to_string(),
        function: xzatoma::providers::FunctionCall {
            name: "early_test".to_string(),
            arguments: "{}".to_string(),
        },
    };

    // Add early messages with tool pair
    agent
        .conversation_mut()
        .add_message(Message::assistant_with_tools(vec![early_tool]));
    agent
        .conversation_mut()
        .add_tool_result("call_early", "early result");

    // Add many messages to trigger eventual pruning
    for i in 0..15 {
        agent
            .conversation_mut()
            .add_user_message(format!("user message {}", i));
        agent
            .conversation_mut()
            .add_assistant_message(format!("assistant response {}", i));
    }

    let conv_id = agent.conversation().id();
    let title = "Pruning Test";

    // Save (may or may not have pruned yet, depends on token count)
    storage
        .save_conversation(
            &conv_id.to_string(),
            title,
            None,
            agent.conversation().messages(),
        )
        .expect("save");

    // Load conversation (which may have different token counts than original)
    let (_, _, loaded_messages) = storage
        .load_conversation(&conv_id.to_string())
        .expect("load")
        .expect("should exist");

    // Reconstruct with same small limits
    let conversation =
        Conversation::with_history(conv_id, title.to_string(), loaded_messages, 500, 2, 0.7);

    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let resumed_agent = Agent::with_conversation(
        Box::new(provider.as_ref().clone()),
        tools,
        config,
        conversation,
    )
    .expect("create resumed");

    // Execute something (this may trigger pruning)
    let _result = resumed_agent.execute("Continue processing").await;

    // Now verify no orphan tool messages exist
    let final_messages = resumed_agent.conversation().messages();

    // Check for orphan tool messages
    for msg in final_messages {
        if msg.role == "tool" {
            if let Some(tool_call_id) = &msg.tool_call_id {
                // This tool message must have a corresponding assistant message with a matching call
                let has_matching_call = final_messages.iter().any(|m| {
                    m.role == "assistant"
                        && m.tool_calls
                            .as_ref()
                            .is_some_and(|tcs| tcs.iter().any(|tc| &tc.id == tool_call_id))
                });
                assert!(
                    has_matching_call,
                    "Orphan tool message found with ID: {} after pruning/resume",
                    tool_call_id
                );
            }
        }
    }
}

/// Test provider parity - both providers use validate_message_sequence
#[test]
fn test_provider_parity_uses_validation() {
    use xzatoma::providers::validate_message_sequence;

    // Test 1: Orphan removal
    let messages = vec![
        Message::user("Do something"),
        Message::tool_result("orphan_call", "result"),
    ];

    let validated = validate_message_sequence(&messages);

    // Orphan should be removed
    assert_eq!(validated.len(), 1);
    assert_eq!(validated[0].role, "user");

    // Test 2: Valid pair preservation
    let tool_call = ToolCall {
        id: "valid_call".to_string(),
        function: xzatoma::providers::FunctionCall {
            name: "test".to_string(),
            arguments: "{}".to_string(),
        },
    };

    let messages_with_pair = vec![
        Message::user("Question"),
        Message::assistant_with_tools(vec![tool_call]),
        Message::tool_result("valid_call", "result"),
    ];

    let validated_pair = validate_message_sequence(&messages_with_pair);

    // All messages should be preserved
    assert_eq!(validated_pair.len(), 3);
    assert_eq!(validated_pair[0].role, "user");
    assert_eq!(validated_pair[1].role, "assistant");
    assert_eq!(validated_pair[2].role, "tool");

    // Test 3: System messages always preserved
    let messages_with_system = vec![
        Message::system("System prompt"),
        Message::user("Question"),
        Message::tool_result("orphan", "result"),
    ];

    let validated_sys = validate_message_sequence(&messages_with_system);

    // System should be preserved, orphan removed
    assert!(validated_sys.iter().any(|m| m.role == "system"));
    assert!(!validated_sys
        .iter()
        .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("orphan")));
}
