use async_trait::async_trait;
use std::time::Duration;
mod common;
use tokio::time::sleep;
use uuid::Uuid;

use xzatoma::agent::Agent;
use xzatoma::config::AgentConfig;
use xzatoma::providers::{CompletionResponse, Message, Provider};
use xzatoma::tools::ToolRegistry;

/// Simple mock provider that returns predetermined messages (in order).
#[derive(Clone)]
struct MockProvider {
    responses: Vec<Message>,
    idx: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl MockProvider {
    fn new(responses: Vec<Message>) -> Self {
        Self {
            responses,
            idx: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> xzatoma::error::Result<CompletionResponse> {
        let mut lock = self.idx.lock().unwrap();
        let i = *lock;
        *lock = i + 1;
        let msg = if i < self.responses.len() {
            self.responses[i].clone()
        } else {
            Message::assistant("Done")
        };
        Ok(CompletionResponse::new(msg))
    }
}

#[tokio::test]
async fn test_conversation_auto_saves_after_message() {
    let (storage, _tmp) = common::create_temp_storage();

    // Provider returns a single assistant message
    let provider = MockProvider::new(vec![Message::assistant("Hello from provider")]);
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let mut agent = Agent::new(provider, tools, config).expect("create agent");

    let prompt = "Hello, this is a test conversation";

    // Simulate interactive mutation: add user and assistant messages directly
    agent.conversation_mut().add_user_message(prompt);
    agent
        .conversation_mut()
        .add_assistant_message("Hello from provider");

    // Simulate run_chat save logic: if first turn (<= 2 messages), title from prompt (trimmed/truncated)
    let conv = agent.conversation();
    let should_update = conv.messages().len() <= 2;
    let title = if should_update {
        let mut t = prompt.trim().to_string();
        if t.len() > 50 {
            t.truncate(47);
            t.push_str("...");
        }
        t
    } else {
        conv.title().to_string()
    };

    storage
        .save_conversation(&conv.id().to_string(), &title, None, conv.messages())
        .expect("save conversation");

    let loaded = storage
        .load_conversation(&conv.id().to_string())
        .expect("load conversation")
        .expect("conversation should be present");
    let (loaded_title, _model, messages) = loaded;
    assert_eq!(loaded_title, title);
    // Should contain at least 2 messages (user + assistant)
    assert!(messages.iter().any(|m| m.role == "user"));
    assert!(messages.iter().any(|m| m.role == "assistant"));
}

#[tokio::test]
async fn test_resume_loads_conversation_history() {
    let (storage, _tmp) = common::create_temp_storage();

    let id = Uuid::new_v4().to_string();
    let title = "Saved session";
    let messages = vec![Message::user("hi"), Message::assistant("hello")];

    storage
        .save_conversation(&id, title, Some("gpt-test"), &messages)
        .expect("save conversation");

    // Load and ensure messages are present and can be used to build a Conversation
    let loaded = storage
        .load_conversation(&id)
        .expect("load failed")
        .expect("should exist");
    let (loaded_title, model, loaded_messages) = loaded;
    assert_eq!(loaded_title, title.to_string());
    assert_eq!(model, Some("gpt-test".to_string()));
    assert_eq!(loaded_messages.len(), 2);

    // Reconstruct conversation using public constructor
    let _conv = xzatoma::agent::Conversation::with_history(
        Uuid::parse_str(&id).expect("valid uuid"),
        loaded_title,
        loaded_messages,
        8000,
        10,
        0.8,
    );
    // If no panic, reconstruction succeeded
}

#[tokio::test]
async fn test_resume_invalid_id_starts_new() {
    let (storage, _tmp) = common::create_temp_storage();

    // Use a random id which was not saved
    let random_id = Uuid::new_v4().to_string();
    let loaded = storage.load_conversation(&random_id).expect("load failed");
    assert!(loaded.is_none(), "expected no conversation for random id");

    // In application flow, this would cause a new Agent to be created. Ensure new agent starts empty.
    let provider = MockProvider::new(vec![Message::assistant("Hi")]);
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let agent = Agent::new(provider, tools, config).expect("create agent");
    assert!(agent.conversation().is_empty());
}

#[tokio::test]
async fn test_title_generated_from_first_user_message() {
    let (storage, _tmp) = common::create_temp_storage();

    let provider = MockProvider::new(vec![Message::assistant("Reply")]);
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let mut agent = Agent::new(provider, tools, config).expect("agent");

    let prompt = "Short title";
    let _ = agent.execute(prompt).await.expect("execute ok");

    let conv = agent.conversation();
    let should_update = conv.messages().len() <= 2;
    assert!(should_update, "should be first-turn conversation");

    let mut expected = prompt.trim().to_string();
    if expected.len() > 50 {
        expected.truncate(47);
        expected.push_str("...");
    }

    storage
        .save_conversation(&conv.id().to_string(), &expected, None, conv.messages())
        .expect("save");

    let loaded = storage
        .load_conversation(&conv.id().to_string())
        .expect("load")
        .expect("present");
    assert_eq!(loaded.0, expected);
}

#[tokio::test]
async fn test_title_truncates_long_first_message() {
    let (storage, _tmp) = common::create_temp_storage();

    let provider = MockProvider::new(vec![Message::assistant("Reply")]);
    let tools = ToolRegistry::new();
    let config = AgentConfig::default();
    let mut agent = Agent::new(provider, tools, config).expect("agent");

    let long_prompt = "This is a very long prompt that should be truncated by the title generation logic because it exceeds the fifty character limit.";
    let _ = agent.execute(long_prompt).await.expect("execute ok");

    let conv = agent.conversation();
    let mut expected = long_prompt.trim().to_string();
    if expected.len() > 50 {
        expected.truncate(47);
        expected.push_str("...");
    }

    storage
        .save_conversation(&conv.id().to_string(), &expected, None, conv.messages())
        .expect("save");

    let loaded = storage
        .load_conversation(&conv.id().to_string())
        .expect("load")
        .expect("present");
    assert_eq!(loaded.0.len(), expected.len());
    assert_eq!(loaded.0, expected);
}

#[tokio::test]
async fn test_history_list_displays_sessions() {
    let (storage, _tmp) = common::create_temp_storage();

    let id1 = Uuid::new_v4().to_string();
    let id2 = Uuid::new_v4().to_string();

    storage
        .save_conversation(
            &id1,
            "First",
            None,
            &[Message::user("one"), Message::assistant("uno")],
        )
        .expect("save1");
    // ensure different timestamps
    sleep(Duration::from_millis(10)).await;
    storage
        .save_conversation(
            &id2,
            "Second",
            None,
            &[
                Message::user("two"),
                Message::assistant("dos"),
                Message::system("s"),
            ],
        )
        .expect("save2");

    let sessions = storage.list_sessions().expect("list");
    assert!(sessions.iter().any(|s| s.id == id1));
    assert!(sessions.iter().any(|s| s.id == id2));

    // Session for id2 should have message_count 3
    let s2 = sessions.iter().find(|s| s.id == id2).unwrap();
    assert_eq!(s2.message_count, 3);
}

#[tokio::test]
async fn test_history_delete_removes_session() {
    let (storage, _tmp) = common::create_temp_storage();

    let id = Uuid::new_v4().to_string();
    storage
        .save_conversation(&id, "ToDelete", None, &[Message::user("x")])
        .expect("save");

    // Exists now
    assert!(storage.load_conversation(&id).expect("load").is_some());
    storage.delete_conversation(&id).expect("delete");
    assert!(storage.load_conversation(&id).expect("load").is_none());
    let sessions = storage.list_sessions().expect("list");
    assert!(!sessions.iter().any(|s| s.id == id));
}
