# Chat History and Tool Integrity Implementation Plan

## Overview

**TL;DR**: Implement robust chat-history inspection and storage for special commands (e.g., `/models`), and prevent provider errors by ensuring tool-result messages are always paired with the corresponding assistant `tool_calls`. This improves UX (viewable history that includes commands), fixes the Copilot 400 "orphan tool" error, and improves pruning and persistence so resumed conversations are consistent.

**Why This Matters**:

- Copilot returns 400 errors when tool messages lack corresponding assistant tool_calls
- Users cannot inspect message-level conversation history
- Special commands like `/models` are not saved in conversation history
- Pruning can create orphaned tool messages by removing assistant messages independently

**Approach**: Five phases implementing provider-side validation, history CLI, pruning integrity, cross-provider consistency, and comprehensive documentation with tests.

---

## Terminology

### Orphan Tool Message

A `tool` role message where `tool_call_id` does not match any `tool_calls[].id` from a preceding `assistant` role message in the conversation history.

**Example of Orphan** (INVALID):

```
Message 1: user: "What is 2+2?"
Message 2: tool: {tool_call_id: "call_xyz123", content: "4"}
           ↑ ORPHAN - no assistant message with tool_calls containing id "call_xyz123"
```

**Example of Valid Pair** (VALID):

```
Message 1: user: "What is 2+2?"
Message 2: assistant: {tool_calls: [{id: "call_xyz123", name: "calculator", args: "2+2"}]}
Message 3: tool: {tool_call_id: "call_xyz123", content: "4"}
           ↑ VALID - matches tool_call_id "call_xyz123" from Message 2
```

**Root Causes of Orphans**:

1. Pruning removes assistant message but keeps tool result
2. Manual conversation editing/corruption
3. Provider API changes or deserialization errors
4. Resume from partially-saved conversation state

---

## Design Decisions (RESOLVED)

### Decision 1: Special Command Persistence Format

**Question**: Should special commands be recorded as `user` messages (visible to AI) or `system` messages (metadata only)?

**Decision**: **System Messages** (metadata only, not in model context)

**Rationale**: Special commands are CLI operations, not conversation turns. Recording as `system` messages allows history inspection without polluting the AI context window or influencing model behavior.

**Implementation**:

```rust
// When user types "/models list"
conversation.add_message(Message {
    role: "system".to_string(),
    content: "User executed command: /models list".to_string(),
    // ... other fields
});

// After command execution
conversation.add_message(Message {
    role: "system".to_string(),
    content: "Command result: Models available: gpt-4, claude-3, ...".to_string(),
    // ... other fields
});
```

### Decision 2: Special Command Persistence Default

**Question**: Should command persistence be enabled by default?

**Decision**: **Default ON** (all special commands recorded automatically)

**Rationale**: Users benefit from complete history by default. Power users can opt-out via config if needed.

**Configuration**:

```rust
// src/config.rs
pub struct Config {
    // ... existing fields

    /// Record special commands in conversation history
    #[serde(default = "default_persist_special_commands")]
    pub persist_special_commands: bool,
}

fn default_persist_special_commands() -> bool {
    true  // ON by default
}
```

### Decision 3: Message Timestamps

**Question**: Add per-message timestamps now (requires schema migration) or defer?

**Decision**: **Defer to Future Phase**

**Rationale**: Not required for core functionality. Adds complexity (schema migration, backward compatibility). Can be added in separate phase without affecting current work.

**Future Consideration**: Track in separate issue for Phase 6 or later.

---

## Current State Analysis

### Existing Infrastructure

**Conversation Management** (`src/agent/conversation.rs`):

- `Conversation` struct with `messages: Vec<Message>`
- `prune_if_needed()` method (line ~180) - removes old messages when approaching token limit
- `create_summary()` method - generates summary of pruned content
- Token counting and context window tracking

**Agent Execution** (`src/agent/core.rs`):

- `Agent::execute()` method - main conversation loop
- Calls `provider.complete(messages, tools)` to get AI responses
- Handles tool execution and result integration

**Provider Implementations**:

- `src/providers/copilot.rs::CopilotProvider::complete()` - GitHub Copilot integration
- `src/providers/copilot.rs::convert_messages()` (line ~542) - converts XZatoma messages to Copilot format
- `src/providers/ollama.rs::OllamaProvider::complete()` - Ollama integration
- `src/providers/ollama.rs::convert_messages()` (line ~256) - converts XZatoma messages to Ollama format
- Both implement `Provider` trait from `src/providers/base.rs`

**CLI and Commands** (`src/commands/`):

- `src/commands/mod.rs::run_chat()` - main chat loop
- `src/commands/special_commands.rs::parse_special_command()` - parses `/models`, `/help`, etc.
- `src/commands/history.rs` - currently supports `List` and `Delete` only (no `Show`)
- `src/commands/models.rs` - handles model listing

**Storage** (`src/storage/mod.rs`):

- `SqliteStorage::save_conversation(id, title, model, messages)` - persists conversations
- `SqliteStorage::load(id)` - returns `Option<(String, Option<String>, Vec<Message>)>`
- `SqliteStorage::list()` - returns all stored conversation sessions
- `SqliteStorage::delete(id)` - removes conversation from DB

**Tools** (`src/tools/`):

- Various tool implementations (file_ops, terminal, etc.)
- `ToolResult::to_message()` - converts tool results to `tool` role messages

### Identified Issues

1. **Provider Validation Gap**: `CopilotProvider::convert_messages()` and `OllamaProvider::convert_messages()` forward `tool` messages verbatim without validating that corresponding `tool_calls` exist. Orphaned `tool` messages cause Copilot to return 400 Bad Request errors.

2. **History Inspection Gap**: `src/commands/history.rs` supports `list` and `delete` but no way to inspect message-level content. Users cannot view conversation details or debug issues.

3. **Special Command Persistence Gap**: Special commands (e.g., `/models list`) are handled by CLI before reaching the agent, so they never appear in `conversation.messages` or saved history. Users lose track of what commands they ran.

4. **Pruning Integrity Gap**: `Conversation::prune_if_needed()` can prune an `assistant` message with `tool_calls` while keeping the `tool` result messages, creating orphans. Next resume → provider request → 400 error.

5. **No Error Recovery**: When orphans exist in loaded conversations, system crashes instead of sanitizing and continuing.

---

## Phase Dependencies

```
┌─────────────────────────────────────────────────────────┐
│ Phase 1: Core Validation (BLOCKING)                    │
│ Provider message validation - orphan detection         │
└──────────────┬──────────────────────────────────────────┘
               │
       ┌───────┴────────┬────────────────┐
       ▼                ▼                ▼
┌──────────────┐ ┌─────────────┐ ┌──────────────────┐
│ Phase 2:     │ │ Phase 3:    │ │ Phase 4:         │
│ History UX   │ │ Pruning     │ │ Cross-Provider   │
│              │ │ Integrity   │ │ Consistency      │
└──────┬───────┘ └──────┬──────┘ └────────┬─────────┘
       │                │                  │
       └────────────────┴──────────────────┘
                        │
                        ▼
         ┌──────────────────────────────┐
         │ Phase 5: Documentation & QA  │
         └──────────────────────────────┘
```

**Execution Order**:

1. **Phase 1 (BLOCKING)** - Must complete and pass ALL validation before any other phase
2. **Phases 2, 3, 4 (PARALLEL)** - Can run concurrently after Phase 1 completes
3. **Phase 5 (FINAL)** - Runs after all implementation phases complete

**Critical Path**: Phase 1 → Phase 5 (minimum viable)

---

## Test Coverage Requirements

**Overall Target**: >80% line coverage (AGENTS.md requirement)

**Per-Phase Targets**:

- Phase 1: >85% (core validation logic is critical path)
- Phase 2: >75% (CLI interaction harder to unit test)
- Phase 3: >85% (pruning logic is critical for data integrity)
- Phase 4: >70% (integration tests supplement unit tests)
- Phase 5: N/A (documentation only)

**Measurement**:

```bash
# Install coverage tool (one-time setup)
cargo install cargo-tarpaulin

# Run coverage for entire project
cargo tarpaulin --out Stdout --packages xzatoma --all-features

# Verify >80% overall
cargo tarpaulin --out Stdout --packages xzatoma --all-features | grep "Coverage"
# Expected output: "XX.XX% coverage" where XX.XX >= 80.00

# Record baseline before starting
cargo tarpaulin --out Stdout > coverage_baseline.txt
echo "Baseline coverage recorded"
```

**Coverage Enforcement**: Each phase must maintain or improve overall coverage percentage.

---

## Error Handling Strategy

### New Error Variants

**File**: `src/error.rs`

Add these variants to `XzatomaError` enum:

```rust
#[derive(Error, Debug)]
pub enum XzatomaError {
    // ... existing variants

    #[error("Orphan tool message detected: tool_call_id={0}, no matching assistant message found")]
    OrphanToolMessage(String),

    #[error("Invalid message sequence in conversation: {0}")]
    InvalidMessageSequence(String),

    #[error("Conversation not found: id={0}")]
    ConversationNotFound(String),
}
```

### Error Handling Behaviors

| Scenario                                 | Behavior                            | Log Level | User Impact                |
| ---------------------------------------- | ----------------------------------- | --------- | -------------------------- |
| Orphan tool detected in provider request | DROP message, LOG warning, CONTINUE | WARN      | Transparent recovery       |
| Orphan tool in loaded conversation       | SANITIZE on load, CONTINUE          | DEBUG     | Transparent recovery       |
| Conversation not found                   | RETURN error to CLI                 | INFO      | User sees error message    |
| Provider 400 response                    | BUBBLE UP as Provider error         | ERROR     | User sees error, can retry |
| Pruning creates orphan                   | PREVENT via atomic pair removal     | DEBUG     | Transparent prevention     |

### Logging Strategy

| Event                         | Level | Format Example                                                                         |
| ----------------------------- | ----- | -------------------------------------------------------------------------------------- |
| Orphan tool detected          | WARN  | `Orphan tool message detected and dropped: tool_call_id=call_xyz, content_preview="4"` |
| Tool pair preserved           | DEBUG | `Preserved tool call pair: call_id=call_xyz, tool_name=calculator`                     |
| Pruning tool pair             | DEBUG | `Pruning tool call pair atomically: call_id=call_xyz, message_count=2`                 |
| History show executed         | INFO  | `Displaying conversation: id=abc123, message_count=47`                                 |
| Special command persisted     | DEBUG | `Persisted special command: /models list, result_length=234`                           |
| Validation sanitized messages | DEBUG | `Sanitized message sequence: dropped 2 orphans, kept 15 valid`                         |

**Log Configuration**: Use `RUST_LOG=debug` to see all validation details during development.

---

## Implementation Phases

### Phase 1: Core Validation (BLOCKING - HIGH PRIORITY)

**Goal**: Prevent provider requests that contain orphan `tool` messages. Fix immediate Copilot 400 errors.

**Duration**: 8-12 hours

**Dependencies**: None (START HERE)

---

#### Task 1.1: Foundation Work - Validation Helper

**File**: `src/providers/base.rs`

**Objective**: Add shared validation helper function usable by all providers.

**Implementation**:

Add this function to `src/providers/base.rs`:

````rust
/// Validates message sequence and removes orphan tool messages
///
/// Traverses messages to track assistant tool_calls and ensures every tool
/// message has a corresponding tool_call_id. Orphaned tool messages are
/// dropped and logged as warnings.
///
/// # Arguments
///
/// * `messages` - Slice of messages to validate
///
/// # Returns
///
/// Vector of valid messages with orphans removed
///
/// # Examples
///
/// ```
/// use xzatoma::providers::{Message, validate_message_sequence};
///
/// let messages = vec![
///     Message::user("test"),
///     Message::tool("call_123", "result"),  // orphan - no assistant
/// ];
/// let valid = validate_message_sequence(&messages);
/// assert_eq!(valid.len(), 1);  // orphan dropped
/// ```
pub fn validate_message_sequence(messages: &[Message]) -> Vec<Message> {
    use std::collections::HashSet;

    let mut valid_tool_call_ids: HashSet<String> = HashSet::new();
    let mut validated_messages: Vec<Message> = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "assistant" => {
                // Track tool_call ids from this assistant message
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        valid_tool_call_ids.insert(tc.id.clone());
                    }
                }
                validated_messages.push(msg.clone());
            }
            "tool" => {
                // Only include tool message if its tool_call_id is valid
                if let Some(tool_call_id) = &msg.tool_call_id {
                    if valid_tool_call_ids.contains(tool_call_id) {
                        validated_messages.push(msg.clone());
                    } else {
                        // Orphan detected - log and drop
                        log::warn!(
                            "Orphan tool message detected and dropped: tool_call_id={}, content_preview={:?}",
                            tool_call_id,
                            msg.content.chars().take(50).collect::<String>()
                        );
                    }
                } else {
                    // Tool message without tool_call_id - invalid, drop
                    log::warn!("Tool message missing tool_call_id, dropping message");
                }
            }
            _ => {
                // user, system messages pass through
                validated_messages.push(msg.clone());
            }
        }
    }

    log::debug!(
        "Message validation: input={}, output={}, dropped={}",
        messages.len(),
        validated_messages.len(),
        messages.len() - validated_messages.len()
    );

    validated_messages
}
````

**Testing Requirements**:

Add to `src/providers/base.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_message_sequence_drops_orphan_tool() {
        let messages = vec![
            Message::user("What is 2+2?"),
            Message::tool("call_orphan", "4"),  // No assistant with this call_id
        ];

        let valid = validate_message_sequence(&messages);

        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].role, "user");
    }

    #[test]
    fn test_validate_message_sequence_preserves_valid_pair() {
        let tool_call = ToolCall {
            id: "call_valid".to_string(),
            name: "calculator".to_string(),
            arguments: "2+2".to_string(),
        };

        let messages = vec![
            Message::user("What is 2+2?"),
            Message::assistant_with_tools(vec![tool_call]),
            Message::tool("call_valid", "4"),
        ];

        let valid = validate_message_sequence(&messages);

        assert_eq!(valid.len(), 3);
        assert_eq!(valid[2].role, "tool");
        assert_eq!(valid[2].tool_call_id.as_deref(), Some("call_valid"));
    }

    #[test]
    fn test_validate_message_sequence_allows_user_and_system() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        let valid = validate_message_sequence(&messages);

        assert_eq!(valid.len(), 3);
    }

    #[test]
    fn test_validate_message_sequence_drops_tool_without_id() {
        let mut msg = Message::tool("call_123", "result");
        msg.tool_call_id = None;  // Invalid - tool without call_id

        let messages = vec![msg];
        let valid = validate_message_sequence(&messages);

        assert_eq!(valid.len(), 0);  // Dropped
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features validate_message_sequence
# Expected: "test result: ok. 4 passed; 0 failed"
```

**Behavioral Validation**:

- Orphan tool messages are dropped
- Valid tool pairs are preserved
- User/system messages pass through unchanged
- Function logs warnings for dropped messages

**Deliverables**:

- `validate_message_sequence()` function in `src/providers/base.rs`
- 4 unit tests covering orphan, valid, passthrough, and invalid cases
- Documentation with runnable example

---

#### Task 1.2: Integrate Validation into CopilotProvider

**File**: `src/providers/copilot.rs`

**Objective**: Use `validate_message_sequence()` before sending requests to Copilot API.

**Implementation**:

Modify `CopilotProvider::convert_messages()` method (line ~542):

```rust
// BEFORE (current code):
fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
    messages
        .iter()
        .map(|msg| self.convert_single_message(msg))
        .collect()
}

// AFTER (with validation):
fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
    use crate::providers::validate_message_sequence;

    // Validate and sanitize messages before conversion
    let validated_messages = validate_message_sequence(messages);

    validated_messages
        .iter()
        .map(|msg| self.convert_single_message(msg))
        .collect()
}
```

**Testing Requirements**:

Add to `src/providers/copilot.rs` test module:

```rust
#[test]
fn test_convert_messages_drops_orphan_tool() {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config).unwrap();

    let messages = vec![
        Message::user("test"),
        Message::tool("call_orphan", "result"),  // No matching assistant
    ];

    let copilot_messages = provider.convert_messages(&messages);

    // Orphan should be dropped
    assert_eq!(copilot_messages.len(), 1);
    assert_eq!(copilot_messages[0].role, "user");
}

#[test]
fn test_convert_messages_preserves_valid_tool_pair() {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config).unwrap();

    let tool_call = ToolCall {
        id: "call_valid".to_string(),
        name: "calculator".to_string(),
        arguments: "2+2".to_string(),
    };

    let messages = vec![
        Message::user("test"),
        Message::assistant_with_tools(vec![tool_call]),
        Message::tool("call_valid", "4"),
    ];

    let copilot_messages = provider.convert_messages(&messages);

    // All messages should be preserved
    assert_eq!(copilot_messages.len(), 3);
    assert_eq!(copilot_messages[2].role, "tool");
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features --package xzatoma copilot::tests
# Expected: All copilot tests pass including 2 new validation tests
```

**Behavioral Validation**:

- `convert_messages()` calls `validate_message_sequence()` before conversion
- Orphan tool messages do not appear in Copilot API request
- Valid tool pairs remain intact in Copilot API request

**Deliverables**:

- Modified `convert_messages()` in `src/providers/copilot.rs`
- 2 new unit tests for validation behavior
- No regressions in existing Copilot tests

---

#### Task 1.3: Integrate Validation into OllamaProvider

**File**: `src/providers/ollama.rs`

**Objective**: Apply same validation to Ollama provider for consistency.

**Implementation**:

Modify `OllamaProvider::convert_messages()` method (line ~256):

```rust
// BEFORE (current code):
fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
    messages
        .iter()
        .map(|msg| self.convert_single_message(msg))
        .collect()
}

// AFTER (with validation):
fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
    use crate::providers::validate_message_sequence;

    // Validate and sanitize messages before conversion
    let validated_messages = validate_message_sequence(messages);

    validated_messages
        .iter()
        .map(|msg| self.convert_single_message(msg))
        .collect()
}
```

**Testing Requirements**:

Add to `src/providers/ollama.rs` test module:

```rust
#[test]
fn test_convert_messages_drops_orphan_tool() {
    let config = OllamaConfig {
        host: "http://localhost:11434".to_string(),
        model: "llama2".to_string(),
    };
    let provider = OllamaProvider::new(config).unwrap();

    let messages = vec![
        Message::user("test"),
        Message::tool("call_orphan", "result"),
    ];

    let ollama_messages = provider.convert_messages(&messages);

    assert_eq!(ollama_messages.len(), 1);
    assert_eq!(ollama_messages[0].role, "user");
}

#[test]
fn test_convert_messages_preserves_valid_tool_pair() {
    let config = OllamaConfig {
        host: "http://localhost:11434".to_string(),
        model: "llama2".to_string(),
    };
    let provider = OllamaProvider::new(config).unwrap();

    let tool_call = ToolCall {
        id: "call_valid".to_string(),
        name: "calculator".to_string(),
        arguments: "2+2".to_string(),
    };

    let messages = vec![
        Message::user("test"),
        Message::assistant_with_tools(vec![tool_call]),
        Message::tool("call_valid", "4"),
    ];

    let ollama_messages = provider.convert_messages(&messages);

    assert_eq!(ollama_messages.len(), 3);
    assert_eq!(ollama_messages[2].role, "tool");
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features --package xzatoma ollama::tests
# Expected: All ollama tests pass including 2 new validation tests
```

**Behavioral Validation**:

- Ollama provider behaves identically to Copilot provider for validation
- Orphan tool messages do not appear in Ollama API requests
- Valid tool pairs preserved

**Deliverables**:

- Modified `convert_messages()` in `src/providers/ollama.rs`
- 2 new unit tests matching Copilot tests
- Provider consistency verified

---

#### Task 1.4: Integration Test - End-to-End Orphan Handling

**File**: `src/agent/core.rs` (add to test module)

**Objective**: Verify orphan handling works through full agent execution flow.

**Implementation**:

Add integration test to `src/agent/core.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // ... existing tests

    #[tokio::test]
    async fn test_agent_execute_sanitizes_orphan_tool_messages() {
        use crate::providers::{Message, ToolCall};
        use crate::test_utils::MockProvider;

        // Setup mock provider
        let mock_response = CompletionResponse {
            message: Message::assistant("Response after sanitization"),
            token_usage: None,
        };
        let provider = Arc::new(MockProvider::new(mock_response));

        let mut agent = Agent::new_from_shared_provider(
            provider.clone(),
            8000,
            10,
            0.8,
        );

        // Manually inject orphan tool message into conversation
        agent.conversation_mut().add_message(Message::user("test"));
        agent.conversation_mut().add_message(Message::tool("call_orphan", "orphan result"));

        // Execute agent - should sanitize messages before provider call
        let result = agent.execute("Continue", &[]).await;

        // Verify execution succeeded (didn't error on orphan)
        assert!(result.is_ok());

        // Verify provider received sanitized messages (no orphan)
        let received_messages = provider.last_messages();
        assert_eq!(received_messages.len(), 1);  // Only user message
        assert_eq!(received_messages[0].role, "user");
    }

    #[tokio::test]
    async fn test_agent_execute_preserves_valid_tool_pair() {
        use crate::providers::{Message, ToolCall};
        use crate::test_utils::MockProvider;

        let mock_response = CompletionResponse {
            message: Message::assistant("Response"),
            token_usage: None,
        };
        let provider = Arc::new(MockProvider::new(mock_response));

        let mut agent = Agent::new_from_shared_provider(
            provider.clone(),
            8000,
            10,
            0.8,
        );

        // Add valid tool call pair
        let tool_call = ToolCall {
            id: "call_valid".to_string(),
            name: "test_tool".to_string(),
            arguments: "{}".to_string(),
        };

        agent.conversation_mut().add_message(Message::user("test"));
        agent.conversation_mut().add_message(Message::assistant_with_tools(vec![tool_call]));
        agent.conversation_mut().add_message(Message::tool("call_valid", "result"));

        let result = agent.execute("Continue", &[]).await;
        assert!(result.is_ok());

        // Verify all messages preserved
        let received_messages = provider.last_messages();
        assert_eq!(received_messages.len(), 3);
    }
}
```

**Note**: This assumes `MockProvider` exists in `src/test_utils.rs`. If not, create minimal mock:

```rust
// src/test_utils.rs
pub struct MockProvider {
    response: CompletionResponse,
    last_messages: Arc<RwLock<Vec<Message>>>,
}

impl MockProvider {
    pub fn new(response: CompletionResponse) -> Self {
        Self {
            response,
            last_messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn last_messages(&self) -> Vec<Message> {
        self.last_messages.read().unwrap().clone()
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(&self, messages: &[Message], _tools: &[Tool]) -> Result<CompletionResponse> {
        *self.last_messages.write().unwrap() = messages.to_vec();
        Ok(self.response.clone())
    }

    // ... implement other required methods
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run integration tests
cargo test --all-features test_agent_execute_sanitizes
cargo test --all-features test_agent_execute_preserves
# Expected: Both tests pass
```

**Behavioral Validation**:

- Agent execution does not error when conversation contains orphan
- Provider receives sanitized message list (orphans removed)
- Valid tool pairs pass through unchanged
- Integration test covers full code path from conversation → provider

**Deliverables**:

- 2 integration tests in agent module
- MockProvider utility if needed
- Proof that validation works end-to-end

---

#### Task 1.5: Phase 1 Deliverables Summary

**Source Code**:

- `src/providers/base.rs` (+80 lines) - `validate_message_sequence()` function and tests
- `src/providers/copilot.rs` (+5 lines, +40 test lines) - Integrated validation
- `src/providers/ollama.rs` (+5 lines, +40 test lines) - Integrated validation
- `src/agent/core.rs` (+60 test lines) - Integration tests
- `src/test_utils.rs` (+50 lines) - MockProvider if needed

**Tests**:

- 4 unit tests in `base.rs` for validation logic
- 2 unit tests in `copilot.rs` for provider integration
- 2 unit tests in `ollama.rs` for provider integration
- 2 integration tests in `agent/core.rs` for end-to-end flow

**Total**: ~240 lines added, 10 tests created

---

#### Task 1.6: Phase 1 Final Validation

**Validation Commands** (MUST ALL PASS):

```bash
# 1. Format code
cargo fmt --all
# Expected: No output (all files formatted)

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished dev [unoptimized + debuginfo] target(s) in X.XXs"
# Expected: 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished dev [unoptimized + debuginfo] target(s) in X.XXs"
# Expected: 0 warnings

# 4. Run all tests
cargo test --all-features
# Expected: "test result: ok. X passed; 0 failed; 0 ignored; 0 measured"
# Expected: X includes all 10 new tests

# 5. Verify coverage (should maintain or improve)
cargo tarpaulin --out Stdout --packages xzatoma --all-features | grep "Coverage"
# Expected: Coverage >= baseline (from coverage_baseline.txt)
```

**Behavioral Validation Checklist**:

- [ ] Orphan tool message detection logs warning and drops message
- [ ] Valid tool call pairs preserved through validation
- [ ] User and system messages pass through unchanged
- [ ] CopilotProvider uses validation before API calls
- [ ] OllamaProvider uses validation before API calls
- [ ] Agent execution handles orphans without errors
- [ ] Integration tests pass end-to-end
- [ ] No regression in existing tests

**Success Criteria**:

- All cargo commands pass with 0 errors and 0 warnings
- All 10 new tests pass
- Test coverage ≥85% for provider modules
- No Copilot 400 errors when orphan messages present
- Documentation complete with runnable examples

**CHECKPOINT**: Do not proceed to Phase 2 until all Phase 1 validation passes.

---

### Phase 2: History UX & Command Persistence

**Goal**: Enable users to view full message-level history and persist special commands in conversation history.

**Duration**: 8-12 hours

**Dependencies**: Phase 1 complete

---

#### Task 2.1: CLI Schema Changes - Add `history show`

**File**: `src/cli.rs`

**Objective**: Extend `HistoryCommand` enum to support `show` subcommand with formatting options.

**Implementation**:

Modify `HistoryCommand` enum in `src/cli.rs`:

```rust
// BEFORE (current):
#[derive(Debug, Clone, Subcommand)]
pub enum HistoryCommand {
    /// List all saved conversations
    List,

    /// Delete a conversation by ID
    Delete {
        /// Conversation ID to delete
        #[arg(short, long)]
        id: String,
    },
}

// AFTER (with show):
#[derive(Debug, Clone, Subcommand)]
pub enum HistoryCommand {
    /// List all saved conversations
    List,

    /// Show detailed message-level history for a conversation
    Show {
        /// Conversation ID to display
        #[arg(short, long)]
        id: String,

        /// Output raw JSON format instead of formatted display
        #[arg(short, long)]
        raw: bool,

        /// Show only the last N messages (default: all)
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },

    /// Delete a conversation by ID
    Delete {
        /// Conversation ID to delete
        #[arg(short, long)]
        id: String,
    },
}
```

**Testing Requirements**:

Add to `src/cli.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_show_parses_id() {
        let cli = Cli::try_parse_from(&["xzatoma", "history", "show", "--id", "abc123"]).unwrap();

        match cli.command {
            Command::History { command: HistoryCommand::Show { id, raw, limit } } => {
                assert_eq!(id, "abc123");
                assert!(!raw);
                assert_eq!(limit, None);
            }
            _ => panic!("Expected History::Show command"),
        }
    }

    #[test]
    fn test_history_show_parses_raw_flag() {
        let cli = Cli::try_parse_from(&[
            "xzatoma", "history", "show", "--id", "abc123", "--raw"
        ]).unwrap();

        match cli.command {
            Command::History { command: HistoryCommand::Show { id, raw, limit } } => {
                assert_eq!(id, "abc123");
                assert!(raw);
                assert_eq!(limit, None);
            }
            _ => panic!("Expected History::Show command"),
        }
    }

    #[test]
    fn test_history_show_parses_limit() {
        let cli = Cli::try_parse_from(&[
            "xzatoma", "history", "show", "--id", "abc123", "--limit", "10"
        ]).unwrap();

        match cli.command {
            Command::History { command: HistoryCommand::Show { id, raw, limit } } => {
                assert_eq!(id, "abc123");
                assert!(!raw);
                assert_eq!(limit, Some(10));
            }
            _ => panic!("Expected History::Show command"),
        }
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features cli::tests::test_history_show
# Expected: "test result: ok. 3 passed; 0 failed"
```

**Deliverables**:

- Updated `HistoryCommand` enum with `Show` variant
- 3 CLI parsing tests
- Help text updated automatically by clap

---

#### Task 2.2: Implement `history show` Command Handler

**File**: `src/commands/history.rs`

**Objective**: Implement logic to load and display conversation history in formatted or raw JSON mode.

**Implementation**:

Add to `src/commands/history.rs`:

```rust
use crate::storage::SqliteStorage;
use crate::error::{Result, XzatomaError};
use crate::cli::HistoryCommand;

/// Execute history command
pub fn execute_history_command(storage: &SqliteStorage, command: &HistoryCommand) -> Result<()> {
    match command {
        HistoryCommand::List => list_conversations(storage),
        HistoryCommand::Show { id, raw, limit } => show_conversation(storage, id, *raw, *limit),
        HistoryCommand::Delete { id } => delete_conversation(storage, id),
    }
}

/// Show detailed conversation history
fn show_conversation(
    storage: &SqliteStorage,
    id: &str,
    raw: bool,
    limit: Option<usize>,
) -> Result<()> {
    // Load conversation from storage
    let maybe_conv = storage.load(id)?;

    let (title, model, messages) = maybe_conv.ok_or_else(|| {
        XzatomaError::ConversationNotFound(id.to_string())
    })?;

    // Apply limit if specified
    let messages_to_display = if let Some(n) = limit {
        let start = messages.len().saturating_sub(n);
        &messages[start..]
    } else {
        &messages
    };

    if raw {
        // Raw JSON output
        let output = serde_json::json!({
            "id": id,
            "title": title,
            "model": model,
            "message_count": messages.len(),
            "messages": messages_to_display,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Formatted display
        println!("Conversation: {}", title);
        println!("ID: {}", id);
        println!("Model: {}", model.unwrap_or_else(|| "unknown".to_string()));
        println!("Messages: {} total", messages.len());
        if limit.is_some() {
            println!("Showing: last {} messages", messages_to_display.len());
        }
        println!("{}", "=".repeat(80));

        for (idx, msg) in messages_to_display.iter().enumerate() {
            let global_idx = if limit.is_some() {
                messages.len() - messages_to_display.len() + idx
            } else {
                idx
            };

            print_message(global_idx, msg);
        }
    }

    log::info!(
        "Displayed conversation: id={}, message_count={}",
        id,
        messages.len()
    );

    Ok(())
}

/// Print a single message in formatted mode
fn print_message(idx: usize, msg: &crate::providers::Message) {
    use crate::providers::Message;

    println!("\n[{}] {}", idx, msg.role.to_uppercase());

    // Show tool_call_id if present (for tool messages)
    if let Some(tool_call_id) = &msg.tool_call_id {
        println!("    Tool Call ID: {}", tool_call_id);
    }

    // Show tool_calls summary if present (for assistant messages)
    if let Some(tool_calls) = &msg.tool_calls {
        println!("    Tool Calls: {} total", tool_calls.len());
        for tc in tool_calls {
            println!("      - {} (id: {})", tc.name, tc.id);
        }
    }

    // Show content (truncate if very long in formatted mode)
    let content_preview = if msg.content.len() > 500 {
        format!("{}... ({} chars total)", &msg.content[..500], msg.content.len())
    } else {
        msg.content.clone()
    };

    println!("    Content: {}", content_preview);
}

// Keep existing list_conversations and delete_conversation functions...
```

**Testing Requirements**:

Add to `src/commands/history.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Message;
    use tempfile::TempDir;

    #[test]
    fn test_show_conversation_formatted() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = SqliteStorage::new_with_path(&db_path).unwrap();

        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];

        storage.save_conversation("test_id", "Test Conv", Some("gpt-4"), &messages).unwrap();

        let result = show_conversation(&storage, "test_id", false, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_conversation_raw_json() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = SqliteStorage::new_with_path(&db_path).unwrap();

        let messages = vec![Message::user("Test")];
        storage.save_conversation("test_id", "Test", Some("gpt-4"), &messages).unwrap();

        let result = show_conversation(&storage, "test_id", true, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_conversation_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = SqliteStorage::new_with_path(&db_path).unwrap();

        let messages = vec![
            Message::user("Msg 1"),
            Message::user("Msg 2"),
            Message::user("Msg 3"),
        ];

        storage.save_conversation("test_id", "Test", Some("gpt-4"), &messages).unwrap();

        let result = show_conversation(&storage, "test_id", false, Some(2));
        assert!(result.is_ok());
        // Note: Would need to capture stdout to verify only 2 messages shown
    }

    #[test]
    fn test_show_conversation_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = SqliteStorage::new_with_path(&db_path).unwrap();

        let result = show_conversation(&storage, "nonexistent", false, None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XzatomaError::ConversationNotFound(_)));
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features history::tests
# Expected: "test result: ok. X passed; 0 failed" (includes 4 new tests)

# 5. Manual test
cargo build --release
./target/release/xzatoma history show --id <real_id>
# Expected: Formatted output showing conversation details
```

**Behavioral Validation**:

- Formatted mode displays readable conversation history
- Raw mode outputs valid JSON
- Limit option shows only last N messages
- Nonexistent conversation returns clear error
- Log message written at INFO level

**Deliverables**:

- `show_conversation()` function with formatting logic
- `print_message()` helper function
- 4 unit tests covering success and error cases
- Integration with existing history command infrastructure

---

#### Task 2.3: Persist Special Commands to Conversation

**Files**: `src/commands/mod.rs`, `src/config.rs`

**Objective**: Record special commands (e.g., `/models list`) as system messages in conversation history.

**Implementation Step 1 - Configuration** (`src/config.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ... existing fields

    /// Record special commands in conversation history
    #[serde(default = "default_persist_special_commands")]
    pub persist_special_commands: bool,
}

fn default_persist_special_commands() -> bool {
    true  // ON by default per Design Decision 2
}

impl Config {
    /// Check if special commands should be persisted
    pub fn should_persist_commands(&self) -> bool {
        self.persist_special_commands
    }
}
```

**Implementation Step 2 - Command Persistence** (`src/commands/mod.rs`):

Find the `run_chat()` function where special commands are handled. Add persistence:

```rust
// Example location: src/commands/mod.rs, run_chat function
// BEFORE (current - commands not persisted):
if let Some(special_cmd) = parse_special_command(&line) {
    match special_cmd {
        SpecialCommand::ListModels => {
            list_models(&provider).await?;
            continue;
        }
        // ... other commands
    }
}

// AFTER (with persistence):
if let Some(special_cmd) = parse_special_command(&line) {
    // Record command if persistence enabled
    if config.should_persist_commands() {
        let cmd_msg = Message::system(&format!("User executed command: {}", line));
        agent.conversation_mut().add_message(cmd_msg);
    }

    match special_cmd {
        SpecialCommand::ListModels => {
            let result = list_models(&provider).await?;

            // Record result if persistence enabled
            if config.should_persist_commands() {
                let result_msg = Message::system(&format!(
                    "Command result: {} models available",
                    result.len()  // Assuming list_models returns Vec<ModelInfo>
                ));
                agent.conversation_mut().add_message(result_msg);
            }

            continue;
        }
        SpecialCommand::Help => {
            print_help();

            if config.should_persist_commands() {
                let help_msg = Message::system("User viewed help");
                agent.conversation_mut().add_message(help_msg);
            }

            continue;
        }
        // ... other commands with similar pattern
    }
}
```

**Testing Requirements**:

Add to `src/commands/mod.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_special_command_persistence_enabled() {
        // Setup with persistence ON
        let mut config = Config::default();
        config.persist_special_commands = true;

        let provider = Arc::new(MockProvider::new_default());
        let mut agent = Agent::new_from_shared_provider(provider, 8000, 10, 0.8);

        let initial_count = agent.conversation().message_count();

        // Simulate /help command
        // (This would need actual command handler testing setup)
        let cmd_msg = Message::system("User executed command: /help");
        agent.conversation_mut().add_message(cmd_msg);

        assert_eq!(agent.conversation().message_count(), initial_count + 1);

        let messages = agent.conversation().messages();
        assert_eq!(messages.last().unwrap().role, "system");
        assert!(messages.last().unwrap().content.contains("/help"));
    }

    #[test]
    fn test_config_should_persist_commands_default_on() {
        let config = Config::default();
        assert!(config.should_persist_commands());
    }

    #[test]
    fn test_config_should_persist_commands_can_disable() {
        let mut config = Config::default();
        config.persist_special_commands = false;
        assert!(!config.should_persist_commands());
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features config::tests::test_config_should_persist
# Expected: Config tests pass

# 5. Integration test
cargo build --release
./target/release/xzatoma chat
# Type: /models list
# Exit and run: xzatoma history show --id <latest>
# Expected: System messages for command and result visible
```

**Behavioral Validation**:

- Config field `persist_special_commands` defaults to true
- Special commands recorded as system messages when enabled
- Command results recorded as system messages
- Messages do not affect AI model context (system role)
- Commands not persisted when config disabled

**Deliverables**:

- Config field and helper method added
- Command persistence integrated into chat loop
- 3 tests for configuration behavior
- Documentation of system message format

---

#### Task 2.4: Phase 2 Testing Requirements

**Unit Tests** (minimum 11 total for Phase 2):

- 3 CLI parsing tests (Task 2.1)
- 4 history show tests (Task 2.2)
- 3 config tests (Task 2.3)
- 1 integration test (command persistence end-to-end)

**Integration Test**:

Add to `tests/` directory or appropriate module:

```rust
#[tokio::test]
async fn test_special_command_appears_in_history() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Setup
    let storage = SqliteStorage::new_with_path(&db_path).unwrap();
    let mut config = Config::default();
    config.persist_special_commands = true;

    let provider = Arc::new(MockProvider::new_default());
    let mut agent = Agent::new_from_shared_provider(provider, 8000, 10, 0.8);

    // Simulate special command
    agent.conversation_mut().add_message(Message::system("User executed command: /models list"));
    agent.conversation_mut().add_message(Message::system("Command result: 3 models available"));

    // Save conversation
    let conv_id = agent.conversation().id().to_string();
    storage.save_conversation(
        &conv_id,
        "Test",
        Some("gpt-4"),
        agent.conversation().messages(),
    ).unwrap();

    // Load and verify
    let (_, _, messages) = storage.load(&conv_id).unwrap().unwrap();

    assert!(messages.iter().any(|m|
        m.role == "system" && m.content.contains("/models list")
    ));
    assert!(messages.iter().any(|m|
        m.role == "system" && m.content.contains("Command result")
    ));
}
```

**Coverage Target**: >75% for commands module

---

#### Task 2.5: Phase 2 Deliverables Summary

**Source Code**:

- `src/cli.rs` (+15 lines, +30 test lines) - Show command parsing
- `src/commands/history.rs` (+150 lines, +80 test lines) - Show implementation
- `src/commands/mod.rs` (+40 lines, +30 test lines) - Command persistence
- `src/config.rs` (+10 lines, +20 test lines) - Config field

**Tests**:

- 3 CLI parsing tests
- 4 history show tests
- 3 config tests
- 1 integration test

**Total**: ~345 lines added, 11+ tests created

---

#### Task 2.6: Phase 2 Final Validation

**Validation Commands** (MUST ALL PASS):

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run all tests
cargo test --all-features
# Expected: All tests pass including 11 new Phase 2 tests

# 5. Coverage check
cargo tarpaulin --out Stdout --packages xzatoma --all-features | grep "Coverage"
# Expected: Coverage ≥ baseline
```

**Manual Validation**:

```bash
# Build release binary
cargo build --release

# Test history show
./target/release/xzatoma history list
./target/release/xzatoma history show --id <id>
./target/release/xzatoma history show --id <id> --raw
./target/release/xzatoma history show --id <id> --limit 5

# Test command persistence
./target/release/xzatoma chat
> /models list
> /help
> exit
./target/release/xzatoma history show --id <latest_id>
# Expected: System messages for /models and /help visible
```

**Success Criteria**:

- All cargo commands pass
- All 11 tests pass
- `history show` displays formatted output correctly
- `history show --raw` outputs valid JSON
- Special commands appear in history when config enabled
- Config can disable command persistence
- Coverage ≥75% for commands module

---

### Phase 3: Pruning Integrity

**Goal**: Ensure pruning preserves tool-call pairs or removes both atomically to prevent orphans.

**Duration**: 6-10 hours

**Dependencies**: Phase 1 complete

---

#### Task 3.1: Add Helper Method - Find Tool Results

**File**: `src/agent/conversation.rs`

**Objective**: Add helper method to find all tool messages referencing a given tool_call_id.

**Implementation**:

Add to `Conversation` impl in `src/agent/conversation.rs`:

````rust
impl Conversation {
    // ... existing methods

    /// Find indices of tool messages that reference the given tool_call_id
    ///
    /// Used during pruning to ensure tool results are removed atomically
    /// with their corresponding assistant tool_calls.
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - The tool call ID to search for
    ///
    /// # Returns
    ///
    /// Vector of message indices where tool_call_id matches
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    /// use xzatoma::providers::Message;
    ///
    /// let mut conv = Conversation::new(8000, 10, 0.8);
    /// conv.add_message(Message::tool("call_123", "result"));
    ///
    /// let indices = conv.find_tool_results_for_call("call_123");
    /// assert_eq!(indices.len(), 1);
    /// ```
    fn find_tool_results_for_call(&self, tool_call_id: &str) -> Vec<usize> {
        self.messages
            .iter()
            .enumerate()
            .filter_map(|(idx, msg)| {
                if msg.role == "tool" && msg.tool_call_id.as_deref() == Some(tool_call_id) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }
}
````

**Testing Requirements**:

Add to `src/agent/conversation.rs` test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_tool_results_for_call_finds_matching() {
        let mut conv = Conversation::new(8000, 10, 0.8);
        conv.add_message(Message::user("test"));
        conv.add_message(Message::tool("call_123", "result1"));
        conv.add_message(Message::tool("call_456", "result2"));
        conv.add_message(Message::tool("call_123", "result3"));

        let indices = conv.find_tool_results_for_call("call_123");

        assert_eq!(indices.len(), 2);
        assert_eq!(indices[0], 1);
        assert_eq!(indices[1], 3);
    }

    #[test]
    fn test_find_tool_results_for_call_returns_empty_when_none() {
        let mut conv = Conversation::new(8000, 10, 0.8);
        conv.add_message(Message::user("test"));
        conv.add_message(Message::assistant("response"));

        let indices = conv.find_tool_results_for_call("call_nonexistent");

        assert!(indices.is_empty());
    }

    #[test]
    fn test_find_tool_results_for_call_ignores_other_roles() {
        let mut conv = Conversation::new(8000, 10, 0.8);
        conv.add_message(Message::user("call_123"));  // Not a tool message
        conv.add_message(Message::tool("call_123", "result"));

        let indices = conv.find_tool_results_for_call("call_123");

        assert_eq!(indices.len(), 1);
        assert_eq!(indices[0], 1);  // Only the actual tool message
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features conversation::tests::test_find_tool_results
# Expected: "test result: ok. 3 passed; 0 failed"
```

**Deliverables**:

- `find_tool_results_for_call()` helper method
- 3 unit tests covering match, no-match, and ignore cases
- Documentation with runnable example

---

#### Task 3.2: Modify Pruning Logic for Atomic Pair Removal

**File**: `src/agent/conversation.rs`

**Objective**: Update `prune_if_needed()` to identify and remove tool-call pairs atomically.

**Implementation**:

Modify `prune_if_needed()` method (approximately line 180-220):

```rust
/// Prune old messages if approaching token limit
///
/// When token count exceeds prune_threshold * max_tokens, removes older
/// messages while preserving:
/// - System message (if present)
/// - Recent conversation turns (min_retain_turns)
/// - Tool call pairs (assistant tool_calls + corresponding tool results)
///
/// Creates summary of pruned content as new system message.
pub fn prune_if_needed(&mut self, model: &str) -> Result<()> {
    use std::collections::HashSet;

    let threshold = (self.max_tokens as f64 * self.prune_threshold) as usize;

    if self.token_count < threshold {
        return Ok(());  // No pruning needed
    }

    log::info!(
        "Pruning conversation: tokens={}/{}, threshold={}",
        self.token_count,
        self.max_tokens,
        threshold
    );

    // Calculate how many messages to keep
    let to_keep = self.min_retain_turns * 2; // Assume 2 messages per turn (user + assistant)

    if self.messages.len() <= to_keep {
        return Ok(());  // Too few messages to prune
    }

    // Determine prune boundary
    let prune_boundary = self.messages.len() - to_keep;

    // Build set of indices to prune, expanding to include tool pairs
    let mut prune_indices = HashSet::new();

    for idx in 0..prune_boundary {
        prune_indices.insert(idx);
    }

    // Scan for assistant messages with tool_calls in prune zone
    // Add their corresponding tool results to prune set (even if outside boundary)
    for idx in 0..prune_boundary {
        let msg = &self.messages[idx];

        if msg.role == "assistant" {
            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    let result_indices = self.find_tool_results_for_call(&tc.id);
                    for result_idx in result_indices {
                        prune_indices.insert(result_idx);
                        log::debug!(
                            "Pruning tool call pair atomically: call_id={}, tool_name={}",
                            tc.id,
                            tc.name
                        );
                    }
                }
            }
        }
    }

    // Conversely, if a tool message would be pruned, also prune its assistant
    // This handles edge case where tool result is in prune zone but assistant isn't
    for idx in 0..self.messages.len() {
        if prune_indices.contains(&idx) && self.messages[idx].role == "tool" {
            if let Some(tool_call_id) = &self.messages[idx].tool_call_id {
                // Find the assistant message with this tool_call_id
                for (asst_idx, asst_msg) in self.messages.iter().enumerate() {
                    if asst_msg.role == "assistant" {
                        if let Some(tool_calls) = &asst_msg.tool_calls {
                            if tool_calls.iter().any(|tc| &tc.id == tool_call_id) {
                                prune_indices.insert(asst_idx);
                                log::debug!(
                                    "Pruning assistant with tool_calls to match pruned tool result: {}",
                                    tool_call_id
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Extract messages to prune (for summary)
    let mut pruned_messages = Vec::new();
    let mut indices_vec: Vec<_> = prune_indices.iter().copied().collect();
    indices_vec.sort();

    for &idx in &indices_vec {
        if idx < self.messages.len() {
            pruned_messages.push(self.messages[idx].clone());
        }
    }

    // Create summary of pruned content
    if !pruned_messages.is_empty() {
        let summary = self.create_summary(&pruned_messages, model)?;

        // Remove pruned messages (highest index first to preserve indices)
        indices_vec.sort_by(|a, b| b.cmp(a));
        for idx in indices_vec {
            if idx < self.messages.len() {
                self.messages.remove(idx);
            }
        }

        // Insert summary at beginning (after system message if present)
        let insert_pos = if !self.messages.is_empty() && self.messages[0].role == "system" {
            1
        } else {
            0
        };

        self.messages.insert(insert_pos, Message::system(&summary));
    }

    // Recalculate token count
    self.recalculate_tokens();

    log::info!(
        "Pruning complete: removed {} messages, tokens now {}/{}",
        pruned_messages.len(),
        self.token_count,
        self.max_tokens
    );

    Ok(())
}
```

**Testing Requirements**:

Add to `src/agent/conversation.rs` test module:

```rust
#[test]
fn test_prune_preserves_tool_call_pair_when_both_in_retain_window() {
    let mut conv = Conversation::new(100, 2, 0.5);  // Low token limit to force pruning

    // Add many messages to trigger pruning
    for i in 0..10 {
        conv.add_message(Message::user(&format!("msg {}", i)));
    }

    // Add tool call pair in retention window
    let tool_call = ToolCall {
        id: "call_retain".to_string(),
        name: "test".to_string(),
        arguments: "{}".to_string(),
    };
    conv.add_message(Message::assistant_with_tools(vec![tool_call]));
    conv.add_message(Message::tool("call_retain", "result"));

    // Force token count high to trigger pruning
    conv.token_count = 60;  // Above threshold (50)

    conv.prune_if_needed("test-model").unwrap();

    // Verify tool pair preserved
    assert!(conv.messages().iter().any(|m|
        m.role == "assistant" && m.tool_calls.is_some()
    ));
    assert!(conv.messages().iter().any(|m|
        m.role == "tool" && m.tool_call_id.as_deref() == Some("call_retain")
    ));
}

#[test]
fn test_prune_removes_both_when_assistant_in_prune_zone() {
    let mut conv = Conversation::new(1000, 2, 0.5);

    // Add tool call pair in what will be prune zone
    let tool_call = ToolCall {
        id: "call_prune".to_string(),
        name: "test".to_string(),
        arguments: "{}".to_string(),
    };
    conv.add_message(Message::assistant_with_tools(vec![tool_call]));
    conv.add_message(Message::tool("call_prune", "result"));

    // Add many more messages to push tool pair into prune zone
    for i in 0..10 {
        conv.add_message(Message::user(&format!("msg {}", i)));
        conv.add_message(Message::assistant(&format!("response {}", i)));
    }

    conv.token_count = 600;  // Above threshold

    conv.prune_if_needed("test-model").unwrap();

    // Verify BOTH assistant and tool messages removed (no orphan)
    assert!(!conv.messages().iter().any(|m|
        m.tool_call_id.as_deref() == Some("call_prune")
    ));

    // Verify no orphans remain
    for msg in conv.messages() {
        if msg.role == "tool" {
            if let Some(tool_call_id) = &msg.tool_call_id {
                // Find corresponding assistant
                let has_assistant = conv.messages().iter().any(|m| {
                    m.role == "assistant" && m.tool_calls.as_ref().map_or(false, |tcs| {
                        tcs.iter().any(|tc| &tc.id == tool_call_id)
                    })
                });
                assert!(has_assistant, "Orphan tool message found after pruning: {}", tool_call_id);
            }
        }
    }
}

#[test]
fn test_prune_creates_summary_message() {
    let mut conv = Conversation::new(100, 1, 0.5);

    for i in 0..5 {
        conv.add_message(Message::user(&format!("message {}", i)));
    }

    let initial_count = conv.messages().len();
    conv.token_count = 60;  // Force pruning

    conv.prune_if_needed("test-model").unwrap();

    // Should have fewer messages but include summary
    assert!(conv.messages().len() < initial_count);

    // Summary should be system message
    assert!(conv.messages().iter().any(|m|
        m.role == "system" && m.content.contains("summarized")
    ));
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run tests
cargo test --all-features conversation::tests::test_prune
# Expected: All pruning tests pass including 3 new tests
```

**Behavioral Validation**:

- Tool call pairs preserved when both in retention window
- Both assistant and tool messages removed when assistant in prune zone
- No orphan tool messages remain after pruning
- Summary message created for pruned content
- Token count recalculated correctly

**Deliverables**:

- Modified `prune_if_needed()` with atomic pair removal logic
- 3 comprehensive pruning tests
- Debug logging for pruning decisions

---

#### Task 3.3: Phase 3 Testing Requirements

**Unit Tests** (minimum 6 for Phase 3):

- 3 helper method tests (Task 3.1)
- 3 pruning logic tests (Task 3.2)

**Coverage Target**: >85% for conversation module

---

#### Task 3.4: Phase 3 Deliverables Summary

**Source Code**:

- `src/agent/conversation.rs` (+120 lines, +100 test lines) - Helper and pruning logic

**Tests**:

- 3 helper method tests
- 3 pruning integrity tests

**Total**: ~220 lines added, 6 tests created

---

#### Task 3.5: Phase 3 Final Validation

**Validation Commands** (MUST ALL PASS):

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run all tests
cargo test --all-features
# Expected: All tests pass including 6 new Phase 3 tests

# 5. Coverage check
cargo tarpaulin --out Stdout --packages xzatoma --all-features | grep "Coverage"
# Expected: Coverage ≥85% for conversation module
```

**Integration Test** (manual):

```bash
# Create conversation with many messages including tool calls
# Let it prune naturally
# Verify no orphans created
RUST_LOG=debug cargo run -- chat
# (Have long conversation with tool usage)
# Exit and check logs for "Pruning tool call pair atomically"
```

**Success Criteria**:

- All cargo commands pass
- All 6 tests pass
- Pruning preserves or removes tool pairs atomically
- No orphan tool messages after pruning
- Debug logs show pruning decisions
- Coverage ≥85% for conversation module

---

### Phase 4: Cross-Provider Consistency & Integration Tests

**Goal**: Ensure validation works consistently across providers and verify end-to-end resume scenarios.

**Duration**: 6-8 hours

**Dependencies**: Phase 1 complete

---

#### Task 4.1: Verify Provider Parity

**Files**: `src/providers/copilot.rs`, `src/providers/ollama.rs`

**Objective**: Confirm both providers use identical validation logic.

**Implementation**:

Review and verify:

1. Both providers import and call `validate_message_sequence()` from `base.rs`
2. Both providers call validation in same location (before message conversion)
3. Both providers have equivalent test coverage

**Verification Checklist**:

```bash
# Verify imports match
grep -n "use crate::providers::validate_message_sequence" src/providers/copilot.rs
grep -n "use crate::providers::validate_message_sequence" src/providers/ollama.rs

# Verify call location match
grep -B2 -A2 "validate_message_sequence" src/providers/copilot.rs
grep -B2 -A2 "validate_message_sequence" src/providers/ollama.rs

# Verify test count match
grep -c "test_convert_messages" src/providers/copilot.rs
grep -c "test_convert_messages" src/providers/ollama.rs
```

**If Differences Found**: Align implementations to match exactly.

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run provider tests
cargo test --all-features copilot::tests ollama::tests
# Expected: All provider tests pass with same count
```

**Deliverables**:

- Verification that both providers behave identically
- Any alignment fixes needed
- Documentation of provider parity

---

#### Task 4.2: Integration Test - Save/Load/Resume with Orphans

**File**: `tests/integration_history_tool_integrity.rs` (new file)

**Objective**: Test complete cycle of saving conversation with orphan, loading, and resuming without errors.

**Implementation**:

Create new integration test file:

```rust
// tests/integration_history_tool_integrity.rs

use xzatoma::agent::Agent;
use xzatoma::config::Config;
use xzatoma::providers::{Message, ToolCall, CompletionResponse};
use xzatoma::storage::SqliteStorage;
use xzatoma::test_utils::MockProvider;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_save_load_resume_with_orphan_sanitized() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new_with_path(&db_path).unwrap();

    let mock_response = CompletionResponse {
        message: Message::assistant("Resumed response"),
        token_usage: None,
    };
    let provider = Arc::new(MockProvider::new(mock_response));

    // Create agent with conversation containing orphan
    let mut agent = Agent::new_from_shared_provider(
        provider.clone(),
        8000,
        10,
        0.8,
    );

    agent.conversation_mut().add_message(Message::user("Hello"));
    agent.conversation_mut().add_message(Message::assistant("Hi"));
    agent.conversation_mut().add_message(Message::tool("call_orphan", "orphan result"));

    let conv_id = agent.conversation().id().to_string();

    // Save conversation (orphan included in storage)
    storage.save_conversation(
        &conv_id,
        "Test Conversation",
        Some("gpt-4"),
        agent.conversation().messages(),
    ).unwrap();

    // Load conversation
    let (title, model, loaded_messages) = storage.load(&conv_id).unwrap().unwrap();

    assert_eq!(title, "Test Conversation");
    assert_eq!(model, Some("gpt-4".to_string()));
    assert_eq!(loaded_messages.len(), 3);  // Orphan still in storage

    // Resume with new agent
    let mut resumed_agent = Agent::with_history(
        conv_id.parse().unwrap(),
        title,
        loaded_messages,
        provider.clone(),
        8000,
        10,
        0.8,
    );

    // Execute - should sanitize orphan before provider call
    let result = resumed_agent.execute("Continue", &[]).await;

    // Verify success (no error despite orphan in storage)
    assert!(result.is_ok());

    // Verify provider received sanitized messages (orphan removed)
    let received = provider.last_messages();
    assert!(!received.iter().any(|m| m.tool_call_id.as_deref() == Some("call_orphan")));
}

#[tokio::test]
async fn test_save_load_resume_preserves_valid_tool_pair() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new_with_path(&db_path).unwrap();

    let mock_response = CompletionResponse {
        message: Message::assistant("Response"),
        token_usage: None,
    };
    let provider = Arc::new(MockProvider::new(mock_response));

    let mut agent = Agent::new_from_shared_provider(provider.clone(), 8000, 10, 0.8);

    // Add valid tool pair
    let tool_call = ToolCall {
        id: "call_valid".to_string(),
        name: "calculator".to_string(),
        arguments: "2+2".to_string(),
    };

    agent.conversation_mut().add_message(Message::user("What is 2+2?"));
    agent.conversation_mut().add_message(Message::assistant_with_tools(vec![tool_call]));
    agent.conversation_mut().add_message(Message::tool("call_valid", "4"));

    let conv_id = agent.conversation().id().to_string();

    // Save
    storage.save_conversation(
        &conv_id,
        "Math Test",
        Some("gpt-4"),
        agent.conversation().messages(),
    ).unwrap();

    // Load
    let (_, _, loaded_messages) = storage.load(&conv_id).unwrap().unwrap();

    // Resume
    let mut resumed = Agent::with_history(
        conv_id.parse().unwrap(),
        "Math Test".to_string(),
        loaded_messages,
        provider.clone(),
        8000,
        10,
        0.8,
    );

    let result = resumed.execute("Continue", &[]).await;
    assert!(result.is_ok());

    // Verify tool pair preserved
    let received = provider.last_messages();
    assert!(received.iter().any(|m| m.role == "assistant" && m.tool_calls.is_some()));
    assert!(received.iter().any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("call_valid")));
}

#[tokio::test]
async fn test_pruning_during_resume_maintains_integrity() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new_with_path(&db_path).unwrap();

    let mock_response = CompletionResponse {
        message: Message::assistant("Response"),
        token_usage: None,
    };
    let provider = Arc::new(MockProvider::new(mock_response));

    // Create conversation with tool pair + many messages
    let mut agent = Agent::new_from_shared_provider(provider.clone(), 500, 2, 0.5);

    let tool_call = ToolCall {
        id: "call_early".to_string(),
        name: "test".to_string(),
        arguments: "{}".to_string(),
    };

    agent.conversation_mut().add_message(Message::assistant_with_tools(vec![tool_call]));
    agent.conversation_mut().add_message(Message::tool("call_early", "result"));

    // Add many more messages to trigger pruning
    for i in 0..20 {
        agent.conversation_mut().add_message(Message::user(&format!("msg {}", i)));
        agent.conversation_mut().add_message(Message::assistant(&format!("response {}", i)));
    }

    let conv_id = agent.conversation().id().to_string();

    // Save
    storage.save_conversation(
        &conv_id,
        "Prune Test",
        Some("gpt-4"),
        agent.conversation().messages(),
    ).unwrap();

    // Load and resume (will trigger pruning)
    let (_, _, loaded) = storage.load(&conv_id).unwrap().unwrap();
    let mut resumed = Agent::with_history(
        conv_id.parse().unwrap(),
        "Prune Test".to_string(),
        loaded,
        provider.clone(),
        500,
        2,
        0.5,
    );

    // Force pruning
    resumed.conversation_mut().token_count = 400;
    resumed.conversation_mut().prune_if_needed("gpt-4").unwrap();

    // Execute
    let result = resumed.execute("Continue", &[]).await;
    assert!(result.is_ok());

    // Verify no orphans
    let messages = resumed.conversation().messages();
    for msg in messages {
        if msg.role == "tool" {
            if let Some(tool_call_id) = &msg.tool_call_id {
                let has_assistant = messages.iter().any(|m| {
                    m.role == "assistant" && m.tool_calls.as_ref().map_or(false, |tcs| {
                        tcs.iter().any(|tc| &tc.id == tool_call_id)
                    })
                });
                assert!(has_assistant, "Orphan found after pruning during resume");
            }
        }
    }
}
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run integration tests
cargo test --all-features --test integration_history_tool_integrity
# Expected: "test result: ok. 3 passed; 0 failed"
```

**Deliverables**:

- New integration test file with 3 end-to-end tests
- Coverage of save/load/resume cycle
- Verification of orphan sanitization
- Verification of pruning during resume

---

#### Task 4.3: Phase 4 Deliverables Summary

**Source Code**:

- `tests/integration_history_tool_integrity.rs` (+150 lines, new file) - Integration tests
- Any alignment fixes in providers (minimal)

**Tests**:

- 3 integration tests covering save/load/resume scenarios

**Total**: ~150 lines added, 3 integration tests

---

#### Task 4.4: Phase 4 Final Validation

**Validation Commands** (MUST ALL PASS):

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Run all tests
cargo test --all-features
# Expected: All tests pass including 3 new integration tests

# 5. Coverage check
cargo tarpaulin --out Stdout --packages xzatoma --all-features | grep "Coverage"
# Expected: Coverage maintained or improved
```

**Success Criteria**:

- Provider parity verified
- 3 integration tests pass
- Save/load/resume cycle works with orphans
- Pruning maintains integrity during resume
- No regressions in existing functionality

---

### Phase 5: Documentation, QA, and Release

**Goal**: Complete documentation, verify all quality gates, prepare for release.

**Duration**: 4-6 hours

**Dependencies**: Phases 1, 2, 3, 4 complete

---

#### Task 5.1: Implementation Documentation

**File**: `docs/explanation/history_and_tool_integrity_implementation.md`

**Objective**: Document complete implementation with examples and validation results.

**Structure**:

```markdown
# Chat History and Tool Integrity Implementation

## Overview

[2-3 paragraph summary of what was implemented, why it matters, and key outcomes]

## Components Delivered

### Source Code

- `src/providers/base.rs` (+80 lines) - Message validation helper
- `src/providers/copilot.rs` (+5 lines, +40 test lines) - Orphan detection
- `src/providers/ollama.rs` (+5 lines, +40 test lines) - Orphan detection
- `src/commands/history.rs` (+150 lines, +80 test lines) - History show command
- `src/cli.rs` (+15 lines, +30 test lines) - CLI parsing
- `src/agent/conversation.rs` (+120 lines, +100 test lines) - Pruning integrity
- `src/commands/mod.rs` (+40 lines, +30 test lines) - Command persistence
- `src/config.rs` (+10 lines, +20 test lines) - Configuration

**Total**: ~425 source lines, ~340 test lines, ~765 lines total

### Tests

- 10 unit tests - Provider validation (Phase 1)
- 11 unit tests - History UX (Phase 2)
- 6 unit tests - Pruning integrity (Phase 3)
- 3 integration tests - End-to-end scenarios (Phase 4)

**Total**: 30 tests created

### Documentation

- This implementation document
- Updated CLI reference (history show)
- Inline doc comments with examples

## Implementation Details

### Orphan Tool Message Validation

[Technical explanation with code examples of validation logic]

### History Inspection CLI

[Examples of using history show command with screenshots/output]

### Pruning Integrity Preservation

[Algorithm explanation with diagrams showing before/after pruning]

### Special Command Persistence

[Examples of commands appearing in history]

## Testing

**Test Coverage**: XX.X% (baseline: YY.Y%, improvement: +Z.Z%)
```

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured

````

**Key Test Cases**:
- Orphan tool messages detected and dropped
- Valid tool pairs preserved through all operations
- Pruning maintains tool call pair integrity
- Save/load/resume cycle sanitizes orphans
- History show displays all message types correctly

## Usage Examples

### Viewing Conversation History

[Complete examples with commands and output]

### Special Command Persistence

[Examples showing /models appearing in history]

### Debugging Tool Message Issues

[Examples of using validation in development]

## Validation Results

- ✅ `cargo fmt --all` - No changes needed
- ✅ `cargo check --all-targets --all-features` - 0 errors
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` - 0 warnings
- ✅ `cargo test --all-features` - 30 tests passed, XX.X% coverage
- ✅ Integration tests pass all scenarios
- ✅ Manual testing complete
- ✅ Documentation complete and reviewed

## Migration Notes

**Existing Conversations**: Conversations saved before this implementation may contain orphan tool messages. These are automatically sanitized when loaded and resumed. No manual migration required.

**Configuration**: New `persist_special_commands` field defaults to `true`. To disable, add to config file:

```yaml
persist_special_commands: false
````

**Breaking Changes**: None. All changes are backward compatible.

## References

- Provider API: `src/providers/base.rs`
- Conversation Management: `src/agent/conversation.rs`
- Storage API: `src/storage/mod.rs`
- AGENTS.md: Project development rules

````

**Success Criteria**:

```bash
# 1. Documentation complete
ls -la docs/explanation/history_and_tool_integrity_implementation.md
# Expected: File exists with complete content

# 2. Validate markdown
# (If markdownlint installed)
markdownlint docs/explanation/history_and_tool_integrity_implementation.md
# Expected: No errors
````

**Deliverables**:

- Complete implementation document
- Usage examples
- Validation results
- Migration notes

---

#### Task 5.2: Update implementations.md Index

**File**: `docs/explanation/implementations.md`

**Objective**: Add entry for this implementation to the index.

**Implementation**:

Add to `docs/explanation/implementations.md`:

```markdown
## Chat History and Tool Integrity

**Completed**: YYYY-MM-DD
**Duration**: 32-48 hours (4-6 days)
**Components**: Provider validation, history CLI, pruning integrity, command persistence
**Files Modified**: 8 source files, 1 integration test file
**Test Coverage**: XX.X% (baseline YY.Y%, improvement +Z.Z%)
**Documentation**: [history_and_tool_integrity_implementation.md](history_and_tool_integrity_implementation.md)

**Summary**: Implemented robust message validation to prevent provider errors from orphan tool messages, added message-level history inspection CLI, enhanced pruning to preserve tool-call pair integrity, and enabled special command persistence. Fixes Copilot 400 errors, improves debugging capabilities, and ensures conversation resume reliability.

**Key Features**:

- Automatic orphan tool message detection and sanitization
- `history show` command with formatted and JSON output
- Atomic tool-call pair removal during pruning
- Special commands recorded as system messages
- Cross-provider consistency (Copilot and Ollama)
- Comprehensive test coverage (30 new tests)
```

**Success Criteria**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Verify documentation
cat docs/explanation/implementations.md | grep "Chat History and Tool Integrity"
# Expected: Entry found
```

**Deliverables**:

- Updated implementations index
- Dated entry with summary

---

#### Task 5.3: CLI Reference Documentation

**File**: `docs/reference/cli.md` (or create if missing)

**Objective**: Document new `history show` command.

**Implementation**:

Add or update section:

````markdown
## history show

Show detailed message-level history for a conversation.

### Usage

```bash
xzatoma history show --id <CONVERSATION_ID> [OPTIONS]
```
````

### Options

- `--id <ID>` - Conversation ID to display (required)
- `--raw` - Output raw JSON format instead of formatted display
- `--limit <N>` - Show only last N messages (default: all)

### Examples

**Show formatted history**:

```bash
xzatoma history show --id abc123-def456-ghi789
```

Output:

```
Conversation: My Chat Session
ID: abc123-def456-ghi789
Model: gpt-4
Messages: 47 total
================================================================================

[0] USER
    Content: Hello, can you help me?

[1] ASSISTANT
    Content: Of course! I'm here to help. What do you need assistance with?

[2] USER
    Content: I need to calculate 2+2

[3] ASSISTANT
    Tool Calls: 1 total
      - calculator (id: call_xyz123)
    Content: I'll calculate that for you.

[4] TOOL
    Tool Call ID: call_xyz123
    Content: 4

...
```

**Show raw JSON** (useful for piping to jq):

```bash
xzatoma history show --id abc123 --raw | jq '.messages[] | select(.role == "tool")'
```

**Show only last 10 messages**:

```bash
xzatoma history show --id abc123 --limit 10
```

### Error Messages

- `Conversation not found: id=<ID>` - No conversation exists with that ID
- `Failed to load conversation: <reason>` - Database error

### Related Commands

- `xzatoma history list` - List all conversations
- `xzatoma history delete --id <ID>` - Delete a conversation

````

**Success Criteria**:

```bash
# Documentation exists and complete
ls -la docs/reference/cli.md
grep "history show" docs/reference/cli.md
# Expected: Documentation found
````

**Deliverables**:

- CLI reference documentation
- Usage examples
- Error message documentation

---

#### Task 5.4: Final QA and Validation

**Objective**: Run complete validation suite and verify all requirements met.

**Validation Commands** (MUST ALL PASS):

```bash
# 1. Clean build
cargo clean
cargo build --release
# Expected: Build succeeds with 0 warnings

# 2. Format check
cargo fmt --all -- --check
# Expected: No output (all formatted)

# 3. Compilation check
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 4. Lint check (zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 5. All tests
cargo test --all-features
# Expected: "test result: ok. X passed; 0 failed" where X includes all 30 new tests

# 6. Coverage measurement
cargo tarpaulin --out Stdout --packages xzatoma --all-features
# Expected: Overall coverage >80%, provider modules >85%

# 7. Integration tests
cargo test --all-features --test integration_history_tool_integrity
# Expected: All 3 integration tests pass

# 8. Documentation tests
cargo test --all-features --doc
# Expected: All doc examples pass
```

**Manual Testing Checklist**:

```bash
# Build release binary
cargo build --release

# Test 1: History show
./target/release/xzatoma history list
./target/release/xzatoma history show --id <id>
./target/release/xzatoma history show --id <id> --raw
./target/release/xzatoma history show --id <id> --limit 5

# Test 2: Special command persistence
./target/release/xzatoma chat
> /models list
> /help
> exit
./target/release/xzatoma history show --id <latest>
# Verify system messages appear

# Test 3: Orphan handling (requires creating orphan scenario)
# Manually edit DB to inject orphan, verify load doesn't crash

# Test 4: Pruning (long conversation)
./target/release/xzatoma chat
# (Have long conversation with tool usage)
# Verify no errors, check debug logs for pruning messages
```

**Final Checklist**:

- [ ] All cargo commands pass (fmt, check, clippy, test)
- [ ] Test coverage >80% overall, >85% for providers and conversation
- [ ] 30 new tests created and passing
- [ ] Integration tests pass all scenarios
- [ ] Documentation complete (implementation doc, CLI reference, implementations.md)
- [ ] Manual testing complete
- [ ] No regressions in existing functionality
- [ ] Design decisions implemented as specified
- [ ] Error handling complete with proper error types
- [ ] Logging strategy implemented
- [ ] Configuration defaults correct

**Success Criteria**: ALL items checked above.

---

#### Task 5.5: Phase 5 Deliverables Summary

**Documentation**:

- `docs/explanation/history_and_tool_integrity_implementation.md` (complete)
- `docs/explanation/implementations.md` (updated with entry)
- `docs/reference/cli.md` (history show documentation)
- Inline doc comments throughout source code

**Validation**:

- All cargo quality gates passed
- Test coverage verified >80%
- Manual testing completed
- No regressions confirmed

**Total Phase 5**: ~3 documentation files updated/created, complete QA

---

## Implementation Timeline

**Total Estimated Effort**: 32-48 hours (4-6 working days)

| Phase   | Tasks   | Estimated Hours | Dependencies        | Priority |
| ------- | ------- | --------------- | ------------------- | -------- |
| Phase 1 | 6 tasks | 8-12 hours      | None (START HERE)   | CRITICAL |
| Phase 2 | 6 tasks | 8-12 hours      | Phase 1 complete    | HIGH     |
| Phase 3 | 5 tasks | 6-10 hours      | Phase 1 complete    | HIGH     |
| Phase 4 | 4 tasks | 6-8 hours       | Phase 1 complete    | MEDIUM   |
| Phase 5 | 5 tasks | 4-6 hours       | Phases 1-4 complete | REQUIRED |

**Task-Level Effort Estimates**:

- Foundation work tasks: 1-2 hours each
- Implementation tasks: 2-4 hours each
- Testing tasks: 1-2 hours each
- Integration tasks: 2-3 hours each
- Documentation tasks: 1-2 hours each
- QA/validation tasks: 2-3 hours each

**Checkpoint Schedule**:

- **Hour 0**: Begin Phase 1 (provider validation)
- **Hour 8-12**: Phase 1 complete, CHECKPOINT - all tests pass, no warnings
- **Hour 16-24**: Phase 2 OR 3 OR 4 complete (parallel work possible)
- **Hour 32-40**: All implementation phases (1-4) complete
- **Hour 36-48**: Phase 5 complete, FINAL VALIDATION, ready for merge

**Critical Path**: Phase 1 → Phase 5 (minimum 12-18 hours)

---

## Rollback and Recovery Strategy

### If Phase 1 Fails

**Symptoms**:

- Provider validation breaks existing tests
- New warnings or errors in clippy
- Provider requests fail unexpectedly

**Rollback Procedure**:

```bash
git checkout HEAD~1 src/providers/copilot.rs src/providers/ollama.rs src/providers/base.rs
cargo test --all-features
# Verify tests pass
```

**Recovery**:

1. Review test failure output carefully
2. Check if validation logic is too strict
3. Add logging: `RUST_LOG=debug cargo test`
4. Adjust validation to be more permissive
5. Retry validation with additional test cases

**Escalation**: If cannot resolve within 2 hours, escalate to user for review.

---

### If Phase 2 Breaks CLI

**Symptoms**:

- `history show` command errors
- Invalid JSON output
- Database load failures

**Rollback Procedure**:

```bash
git checkout HEAD~1 src/cli.rs src/commands/history.rs src/commands/mod.rs src/config.rs
cargo build --release
./target/release/xzatoma history list  # Verify basic functionality
```

**Recovery**:

1. Test with temporary database: `XZATOMA_HISTORY_DB=/tmp/test.db`
2. Verify `storage.load(id)` returns expected format
3. Check JSON serialization: `serde_json::to_string_pretty()`
4. Add debug logging to identify failure point
5. Retry with simpler test cases

---

### If Phase 3 Causes Data Loss

**Symptoms**:

- Pruning removes too many messages
- Conversation corrupted after pruning
- Token count incorrect

**Rollback Procedure**:

```bash
# Rollback code
git checkout HEAD~1 src/agent/conversation.rs

# Restore conversation from backup (if user has one)
cp ~/.local/share/xzatoma/history.db.backup ~/.local/share/xzatoma/history.db
```

**Recovery**:

1. Add comprehensive logging to pruning: `RUST_LOG=debug`
2. Test with copy of production database
3. Verify message count invariants before/after
4. Add assertions to catch edge cases
5. Use smaller test conversations to isolate issue

**Prevention**: Always test pruning with diverse conversation structures.

---

### If Integration Tests Fail (Phase 4)

**Symptoms**:

- Save/load cycle errors
- Resume fails with provider errors
- Orphans not sanitized

**Recovery**:

1. Run single integration test in isolation
2. Add logging to track message flow
3. Verify MockProvider behaving correctly
4. Check that validation is called at right time
5. Compare actual vs expected message sequences

---

### Emergency Stop Criteria

**STOP IMPLEMENTATION IMMEDIATELY IF**:

1. More than 5 existing tests start failing
2. Coverage drops below 70%
3. Clippy warnings exceed 10
4. Any phase takes >150% of estimated time
5. Unable to rollback successfully

**Emergency Procedure**:

```bash
# 1. Stop all work
# 2. Run full test suite
cargo test --all-features

# 3. Identify broken tests
cargo test --all-features 2>&1 | grep "FAILED"

# 4. Rollback to last known good commit
git log --oneline -10  # Find last good commit
git reset --hard <commit_hash>

# 5. Verify clean state
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings

# 6. Document issue
echo "Emergency stop at $(date): <reason>" >> docs/explanation/implementation_issues.md

# 7. Escalate to user
```

---

## Summary

This implementation plan addresses chat history inspection, tool message integrity, and special command persistence through five coordinated phases:

1. **Phase 1** (CRITICAL): Provider validation prevents orphan tool messages from causing 400 errors
2. **Phase 2**: History CLI enables message-level inspection and command persistence
3. **Phase 3**: Pruning integrity ensures tool-call pairs remain atomic
4. **Phase 4**: Cross-provider consistency and integration testing
5. **Phase 5**: Documentation, QA, and release preparation

**Key Outcomes**:

- No more Copilot 400 errors from orphan tool messages
- Users can inspect full conversation history including commands
- Pruning maintains tool-call pair integrity
- All providers behave consistently
- > 80% test coverage maintained
- Comprehensive documentation

**Design Decisions Resolved**:

- Special commands → system messages (not in AI context)
- Command persistence → ON by default
- Message timestamps → deferred to future phase

**Ready to Begin**: Phase 1 implementation can start immediately.

---

**Plan Version**: 2.0 (Revised)
**Last Updated**: 2025
**Status**: APPROVED FOR IMPLEMENTATION
