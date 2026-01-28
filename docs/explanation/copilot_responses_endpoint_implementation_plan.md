# Copilot Responses Endpoint Implementation Plan

## Overview

This plan outlines the implementation of Copilot responses endpoint support in XZatoma. The GitHub Copilot API provides two distinct endpoints for completions: `/chat/completions` (currently supported) and `/responses` (not yet supported). The responses endpoint offers a different API structure optimized for certain model capabilities and workflows. This implementation will add full support for the responses endpoint with automatic endpoint selection and streaming support.

## BREAKING CHANGES

**CRITICAL**: This implementation introduces breaking changes to public API.

### Change 1: CompletionResponse Structure

**File**: `src/providers/base.rs`

**Modification**: Add new field to `CompletionResponse` struct:

```rust
pub struct CompletionResponse {
    pub message: Message,
    pub usage: Option<TokenUsage>,
    pub model: Option<String>,
    // NEW FIELD - BREAKING CHANGE:
    pub reasoning: Option<String>,
}
```

**Impact**: All code constructing `CompletionResponse` must be updated.

**Migration Required**:

```rust
// OLD CODE (will not compile):
let response = CompletionResponse {
    message,
    usage: Some(usage),
    model: Some(model_name),
};

// NEW CODE (required):
let response = CompletionResponse {
    message,
    usage: Some(usage),
    model: Some(model_name),
    reasoning: None,
};

// ALTERNATIVE (recommended):
let response = CompletionResponse {
    message,
    usage: Some(usage),
    model: Some(model_name),
    ..Default::default()
};
```

**Affected Files**:

- `src/providers/copilot.rs` - response construction
- `src/providers/ollama.rs` - response construction
- All test files constructing `CompletionResponse`

### Change 2: Configuration Schema

**File**: `src/config.rs`

**Modification**: Add new fields to `CopilotConfig` struct:

```rust
pub struct CopilotConfig {
    pub model: String,
    pub api_base: Option<String>,
    // NEW FIELDS:
    pub enable_streaming: bool,
    pub enable_endpoint_fallback: bool,
    pub reasoning_effort: Option<String>,
    pub include_reasoning: bool,
}
```

**Impact**: Configuration parsing and defaults updated.

**Migration**: Existing configs continue working with new defaults.

### Change 3: Dependencies

**File**: `Cargo.toml`

**Modification**: Add new dependency:

```toml
futures = "0.3"
```

**Impact**: Run `cargo update` after merge.

## File Modification Summary

**OVERVIEW**:

- Total Files to Modify: 6 files
- Total Files to Create: 2 files (documentation)
- Total New Lines: ~2,500 lines
- Total Modified Lines: ~150 lines

### Source Code Changes

| File Path                  | Type   | Lines Added | Lines Modified | Purpose                                       |
| -------------------------- | ------ | ----------- | -------------- | --------------------------------------------- |
| `src/providers/copilot.rs` | Extend | +800        | ~50            | Add responses types, converters, streaming    |
| `src/providers/base.rs`    | Modify | +10         | ~5             | Add `reasoning` field to `CompletionResponse` |
| `src/config.rs`            | Extend | +50         | ~10            | Add config fields for responses endpoint      |
| `src/error.rs`             | Extend | +30         | 0              | Add 6 new error variants                      |
| `Cargo.toml`               | Extend | +5          | 0              | Add `futures` dependency                      |

**Subtotal**: ~895 lines added, ~65 lines modified

### Test Code Changes

| File Path                          | Type   | Lines Added | Purpose                              |
| ---------------------------------- | ------ | ----------- | ------------------------------------ |
| `src/providers/copilot.rs` (tests) | Extend | +800        | Unit tests for all new functionality |

**Subtotal**: ~800 lines of tests

### Documentation Changes

| File Path                                              | Type   | Lines Added | Lines Modified | Purpose                            |
| ------------------------------------------------------ | ------ | ----------- | -------------- | ---------------------------------- |
| `README.md`                                            | Update | +20         | ~30            | Add responses endpoint to features |
| `docs/reference/copilot_provider.md`                   | Create | +500        | 0              | API reference documentation        |
| `docs/explanation/copilot_responses_implementation.md` | Create | +400        | 0              | Implementation summary             |

**Subtotal**: ~920 lines documentation

### TOTAL IMPACT

- **Source Code**: ~895 lines added, ~65 modified
- **Tests**: ~800 lines added
- **Documentation**: ~920 lines added
- **Grand Total**: ~2,615 new lines, ~65 lines modified

## Current State Analysis

### Existing Infrastructure

XZatoma currently implements Copilot provider support with the following components:

- **Provider Implementation** (`src/providers/copilot.rs`): Implements the `Provider` trait with authentication via GitHub OAuth device flow, token caching via keyring, and model listing capabilities
- **Single Endpoint Support**: Only supports `/chat/completions` endpoint via `COPILOT_COMPLETIONS_URL`
- **No Streaming**: Current implementation uses blocking HTTP requests without Server-Sent Events (SSE) support
- **Model Management**: Fetches and caches model information from Copilot API but does not track which endpoints each model supports
- **Request/Response Structures**: Defines `CopilotRequest`, `CopilotResponse`, and related types specific to completions endpoint

### Identified Issues

Based on analysis of Zed's implementation in `copilot_chat.rs`, the following gaps exist:

1. **Missing Endpoint Detection**: Models advertise supported endpoints (`chat_completions`, `responses`, `messages`) but XZatoma does not parse or use this information
2. **No Responses Endpoint**: The `/responses` endpoint URL is not configured and no request/response types exist for it
3. **Incompatible Request Format**: Responses endpoint uses `input: Vec<ResponseInputItem>` instead of `messages: Vec<Message>`, requiring format conversion
4. **No Streaming Support**: Both endpoints support SSE streaming but XZatoma lacks streaming infrastructure
5. **Missing Response Types**: Responses endpoint returns different event types including function calls, reasoning data, and status information
6. **No Model Capability Checking**: Provider should verify model supports desired endpoint before making requests

## Implementation Dependencies Matrix

This matrix shows which tasks depend on others and which tasks are blocking.

| Phase | Task | Depends On                   | Blocks             | Can Start        |
| ----- | ---- | ---------------------------- | ------------------ | ---------------- |
| 1     | 1.1  | None                         | 2.1, 2.2, 2.3, 4.1 | Immediately      |
| 1     | 1.2  | None                         | 4.1                | Immediately      |
| 1     | 1.3  | None                         | 4.1                | Immediately      |
| 1     | 1.4  | None                         | All tasks          | Immediately      |
| 2     | 2.1  | 1.1, 1.4                     | 4.2                | After 1.1, 1.4   |
| 2     | 2.2  | 1.1, 2.1, 1.4                | 4.2                | After 2.1        |
| 2     | 2.3  | 1.1, 1.4                     | 4.2                | After 1.1, 1.4   |
| 3     | 3.1  | 1.1, 1.4                     | 3.2, 3.3           | After 1.1, 1.4   |
| 3     | 3.2  | 3.1, 1.4                     | 4.2                | After 3.1        |
| 3     | 3.3  | 3.1, 1.4                     | 4.2                | After 3.1        |
| 4     | 4.1  | 1.2, 1.3, 1.4                | 4.2                | After Phase 1    |
| 4     | 4.2  | 2.1, 2.2, 2.3, 3.2, 3.3, 4.1 | None               | After Phases 2-3 |
| 4     | 4.3  | 1.4                          | 4.2                | After 1.4        |
| 5     | All  | All Phase 1-4 tasks          | None               | After Phase 4    |

**CRITICAL PATH**: 1.1 → 1.4 → 2.1 → 3.1 → 3.2 → 4.2

**PARALLEL WORK OPPORTUNITIES**:

- Tasks 1.1, 1.2, 1.3, 1.4 can be done in parallel
- Tasks 2.1 and 2.3 can be done in parallel (after 1.1, 1.4)
- Task 3.1 can start while Phase 2 is in progress

**ESTIMATED TIMELINE**:

- Phase 1: 3-4 days (parallel work)
- Phase 2: 3-4 days (some parallel work)
- Phase 3: 3-4 days (sequential)
- Phase 4: 5-6 days (mostly sequential)
- Phase 5: 4-5 days (documentation)
- **Total: 18-23 days (3-4 weeks)**

## Implementation Phases

### Phase 1: Core Data Structures and Endpoint Detection

#### Task 1.1: Add Responses Endpoint Type Definitions

**OBJECTIVE**: Add complete type system for GitHub Copilot responses endpoint to support request/response serialization.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**LOCATION**: Add after existing `CopilotResponse` struct definition (approximately line 150)

**EXACT STRUCTURES TO ADD**:

```rust
// ============================================================================
// RESPONSES ENDPOINT TYPES
// ============================================================================

/// Request structure for /responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsesRequest {
    /// Model identifier (e.g., "gpt-5-mini", "claude-3.5-sonnet")
    pub model: String,

    /// Input items (messages, function calls, reasoning)
    pub input: Vec<ResponseInputItem>,

    /// Enable streaming (SSE)
    #[serde(default)]
    pub stream: bool,

    /// Temperature for sampling (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Available tools for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,

    /// Tool selection strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Reasoning configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,

    /// Fields to include in response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
}

/// Input item for responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputItem {
    /// Text message with role
    Message {
        role: String,
        content: Vec<ResponseInputContent>,
    },
    /// Function call from assistant
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Function call result
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
    /// Reasoning content
    Reasoning {
        content: Vec<ResponseInputContent>,
    },
}

/// Content types for response input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputContent {
    /// Text content
    InputText { text: String },
    /// Assistant output text
    OutputText { text: String },
    /// Image content
    InputImage { url: String },
}

/// SSE stream events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Message output event
    Message {
        role: String,
        content: Vec<ResponseInputContent>,
    },
    /// Function call event
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// Reasoning event
    Reasoning {
        content: Vec<ResponseInputContent>,
    },
    /// Status event
    Status {
        status: String,
    },
    /// Done event
    Done,
}

/// Tool definition for responses endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolDefinition {
    /// Function tool
    Function {
        function: FunctionDefinition,
    },
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// JSON schema for parameters
    pub parameters: serde_json::Value,
    /// Enable strict mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Tool choice strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto selection
    Auto { auto: bool },
    /// Require any tool
    Any { any: bool },
    /// No tool usage
    None { none: bool },
    /// Specific tool
    Named { function: FunctionName },
}

/// Named function for tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
    pub name: String,
}

/// Reasoning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Reasoning effort level: "low", "medium", "high"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}
```

**DEPENDENCIES**: None (foundation task)

**ESTIMATED LINES**: 300-350 lines

**TESTING REQUIREMENTS**:

**Test 1.1.1: Serialize ResponsesRequest to JSON**

```rust
#[test]
fn test_responses_request_serialization() {
    let request = ResponsesRequest {
        model: "gpt-5-mini".to_string(),
        input: vec![ResponseInputItem::Message {
            role: "user".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "Hello".to_string(),
            }],
        }],
        stream: true,
        temperature: Some(0.7),
        tools: None,
        tool_choice: None,
        reasoning: None,
        include: None,
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    assert!(json.contains("\"model\":\"gpt-5-mini\""));
    assert!(json.contains("\"stream\":true"));
    assert!(json.contains("\"temperature\":0.7"));
}
```

**Test 1.1.2: Deserialize ResponseInputItem Message variant**

```rust
#[test]
fn test_response_input_item_message_deserialization() {
    let json = r#"{
        "type": "message",
        "role": "user",
        "content": [{"type": "input_text", "text": "Hello"}]
    }"#;

    let item: ResponseInputItem = serde_json::from_str(json).expect("Failed to deserialize");

    match item {
        ResponseInputItem::Message { role, content } => {
            assert_eq!(role, "user");
            assert_eq!(content.len(), 1);
        }
        _ => panic!("Expected Message variant"),
    }
}
```

**Test 1.1.3: Deserialize ResponseInputItem FunctionCall variant**

```rust
#[test]
fn test_response_input_item_function_call_deserialization() {
    let json = r#"{
        "type": "function_call",
        "call_id": "call_123",
        "name": "get_weather",
        "arguments": "{\"location\":\"SF\"}"
    }"#;

    let item: ResponseInputItem = serde_json::from_str(json).expect("Failed to deserialize");

    match item {
        ResponseInputItem::FunctionCall { call_id, name, arguments } => {
            assert_eq!(call_id, "call_123");
            assert_eq!(name, "get_weather");
            assert!(arguments.contains("location"));
        }
        _ => panic!("Expected FunctionCall variant"),
    }
}
```

**Test 1.1.4: Roundtrip serialization for StreamEvent**

```rust
#[test]
fn test_stream_event_roundtrip() {
    let original = StreamEvent::Message {
        role: "assistant".to_string(),
        content: vec![ResponseInputContent::OutputText {
            text: "Response text".to_string(),
        }],
    };

    let json = serde_json::to_string(&original).expect("Failed to serialize");
    let deserialized: StreamEvent = serde_json::from_str(&json).expect("Failed to deserialize");

    match (original, deserialized) {
        (StreamEvent::Message { role: r1, .. }, StreamEvent::Message { role: r2, .. }) => {
            assert_eq!(r1, r2);
        }
        _ => panic!("Roundtrip failed"),
    }
}
```

**Test 1.1.5: ToolDefinition serialization**

```rust
#[test]
fn test_tool_definition_serialization() {
    let tool = ToolDefinition::Function {
        function: FunctionDefinition {
            name: "get_weather".to_string(),
            description: "Get weather for location".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
            strict: Some(true),
        },
    };

    let json = serde_json::to_string(&tool).expect("Failed to serialize");
    assert!(json.contains("\"name\":\"get_weather\""));
    assert!(json.contains("\"strict\":true"));
}
```

**Test 1.1.6: ToolChoice variants**

```rust
#[test]
fn test_tool_choice_variants() {
    let auto = ToolChoice::Auto { auto: true };
    let json = serde_json::to_string(&auto).expect("Serialize failed");
    assert!(json.contains("\"auto\":true"));

    let named = ToolChoice::Named {
        function: FunctionName {
            name: "specific_tool".to_string(),
        },
    };
    let json = serde_json::to_string(&named).expect("Serialize failed");
    assert!(json.contains("\"specific_tool\""));
}
```

**Test 1.1.7: ResponseInputContent variants**

```rust
#[test]
fn test_response_input_content_variants() {
    let input_text = ResponseInputContent::InputText {
        text: "User input".to_string(),
    };
    let json = serde_json::to_string(&input_text).expect("Serialize failed");
    assert!(json.contains("input_text"));
    assert!(json.contains("User input"));

    let output_text = ResponseInputContent::OutputText {
        text: "Assistant output".to_string(),
    };
    let json = serde_json::to_string(&output_text).expect("Serialize failed");
    assert!(json.contains("output_text"));
}
```

**Test 1.1.8: Handle optional fields correctly**

```rust
#[test]
fn test_optional_fields_omitted() {
    let request = ResponsesRequest {
        model: "gpt-5-mini".to_string(),
        input: vec![],
        stream: false,
        temperature: None,
        tools: None,
        tool_choice: None,
        reasoning: None,
        include: None,
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    assert!(!json.contains("temperature"));
    assert!(!json.contains("tools"));
    assert!(!json.contains("reasoning"));
}
```

**EXPECTED TEST COUNT**: 8 tests minimum

**DELIVERABLES**:

- `ResponsesRequest` struct with all fields (40 lines)
- `ResponseInputItem` enum with 4 variants (30 lines)
- `ResponseInputContent` enum with 3 variants (20 lines)
- `StreamEvent` enum for SSE parsing (30 lines)
- `ToolDefinition`, `FunctionDefinition` types (30 lines)
- `ToolChoice` enum with 4 variants (20 lines)
- `FunctionName`, `ReasoningConfig` support types (15 lines)
- Serialization/deserialization derives on all types
- 8+ unit tests achieving >80% coverage (200 lines)

**Total**: ~385 lines including tests

**SUCCESS CRITERIA**:

MUST pass all validation commands:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_responses_request_serialization
cargo test test_response_input_item
cargo test test_stream_event_roundtrip
```

**Expected output**:

- Zero formatting changes needed
- Zero compilation errors
- Zero clippy warnings
- All 8+ tests pass
- Coverage >80% for new code

**VALIDATION CHECKLIST**:

- [ ] All structs have complete doc comments with examples
- [ ] All fields documented with purpose and constraints
- [ ] Serde derives added to all types
- [ ] Optional fields use `skip_serializing_if`
- [ ] Test coverage >80%
- [ ] No clippy warnings
- [ ] Follows XZatoma naming conventions

#### Task 1.2: Add Endpoint Tracking to Model Data

**OBJECTIVE**: Extend model data structures to track which endpoints each model supports.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT CHANGES**:

1. Locate `CopilotModelData` struct (search for `struct CopilotModelData`)

2. Add new field to struct:

```rust
/// Copilot model metadata
#[derive(Debug, Clone, Deserialize)]
struct CopilotModelData {
    // ... existing fields ...

    // NEW FIELD - ADD THIS:
    /// Supported endpoints for this model
    #[serde(default)]
    supported_endpoints: Vec<String>,
}
```

3. Add helper method to `CopilotModelData`:

````rust
impl CopilotModelData {
    /// Check if model supports a specific endpoint
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint name to check (e.g., "responses", "chat_completions")
    ///
    /// # Returns
    ///
    /// Returns true if model supports the endpoint
    ///
    /// # Examples
    ///
    /// ```
    /// let model_data = CopilotModelData {
    ///     supported_endpoints: vec!["responses".to_string()],
    ///     // ... other fields
    /// };
    /// assert!(model_data.supports_endpoint("responses"));
    /// assert!(!model_data.supports_endpoint("messages"));
    /// ```
    fn supports_endpoint(&self, endpoint: &str) -> bool {
        self.supported_endpoints.iter().any(|e| e == endpoint)
    }
}
````

4. Update model info conversion to store endpoint data:

```rust
// In the function that converts CopilotModelData to ModelInfo,
// add endpoint support to provider_specific metadata:

let mut metadata = HashMap::new();
metadata.insert(
    "supported_endpoints".to_string(),
    serde_json::to_value(&model_data.supported_endpoints).unwrap_or_default(),
);

// Store in ModelInfo.provider_specific field
```

**DEPENDENCIES**: None

**ESTIMATED LINES**: 50 lines

**TESTING REQUIREMENTS**:

**Test 1.2.1: Parse model with supported endpoints**

```rust
#[test]
fn test_parse_model_with_endpoints() {
    let json = r#"{
        "id": "gpt-5-mini",
        "name": "GPT 5 Mini",
        "supported_endpoints": ["chat_completions", "responses"]
    }"#;

    let model: CopilotModelData = serde_json::from_str(json).expect("Parse failed");
    assert_eq!(model.supported_endpoints.len(), 2);
    assert!(model.supported_endpoints.contains(&"responses".to_string()));
}
```

**Test 1.2.2: Check endpoint support**

```rust
#[test]
fn test_supports_endpoint_method() {
    let model = CopilotModelData {
        supported_endpoints: vec![
            "chat_completions".to_string(),
            "responses".to_string(),
        ],
        // ... other fields with defaults
    };

    assert!(model.supports_endpoint("responses"));
    assert!(model.supports_endpoint("chat_completions"));
    assert!(!model.supports_endpoint("messages"));
    assert!(!model.supports_endpoint("unknown"));
}
```

**Test 1.2.3: Handle missing endpoints field**

```rust
#[test]
fn test_model_without_endpoints_field() {
    let json = r#"{
        "id": "old-model",
        "name": "Old Model"
    }"#;

    let model: CopilotModelData = serde_json::from_str(json).expect("Parse failed");
    assert_eq!(model.supported_endpoints.len(), 0);
    assert!(!model.supports_endpoint("responses"));
}
```

**Test 1.2.4: Store endpoints in ModelInfo metadata**

```rust
#[test]
fn test_endpoints_stored_in_model_info() {
    let model_data = CopilotModelData {
        supported_endpoints: vec!["responses".to_string()],
        // ... other fields
    };

    let model_info = convert_to_model_info(&model_data);

    let endpoints = model_info.provider_specific
        .get("supported_endpoints")
        .expect("Endpoints not in metadata");

    let endpoints_array = endpoints.as_array().expect("Not an array");
    assert_eq!(endpoints_array.len(), 1);
}
```

**EXPECTED TEST COUNT**: 4 tests minimum

**DELIVERABLES**:

- Updated `CopilotModelData` struct with `supported_endpoints` field (5 lines)
- `supports_endpoint()` helper method (15 lines)
- Endpoint metadata storage in `ModelInfo` conversion (10 lines)
- Unit tests (100 lines)

**Total**: ~130 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_parse_model_with_endpoints
cargo test test_supports_endpoint_method
```

**Expected output**:

- All commands pass with zero errors/warnings
- All 4+ tests pass
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Field has proper serde default attribute
- [ ] Helper method documented with examples
- [ ] Metadata properly stored in ModelInfo
- [ ] Tests cover all edge cases
- [ ] No clippy warnings

#### Task 1.3: Add Endpoint Configuration

**OBJECTIVE**: Add responses endpoint URL constant and endpoint selection configuration.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT CONSTANT TO ADD** (after line 29, after `COPILOT_COMPLETIONS_URL`):

```rust
/// Copilot responses endpoint
const COPILOT_RESPONSES_URL: &str = "https://api.githubcopilot.com/responses";
```

**ENUM TO ADD** (near top of file, after imports):

```rust
/// Supported model endpoints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelEndpoint {
    /// Chat completions endpoint (/chat/completions)
    ChatCompletions,
    /// Responses endpoint (/responses)
    Responses,
    /// Messages endpoint (/messages)
    Messages,
    /// Unknown/unsupported endpoint
    Unknown,
}

impl ModelEndpoint {
    /// Convert endpoint name string to enum
    ///
    /// # Arguments
    ///
    /// * `name` - Endpoint name (e.g., "responses", "chat_completions")
    ///
    /// # Returns
    ///
    /// Returns corresponding ModelEndpoint variant
    fn from_name(name: &str) -> Self {
        match name {
            "chat_completions" => ModelEndpoint::ChatCompletions,
            "responses" => ModelEndpoint::Responses,
            "messages" => ModelEndpoint::Messages,
            _ => ModelEndpoint::Unknown,
        }
    }

    /// Get endpoint name as string
    fn as_str(&self) -> &'static str {
        match self {
            ModelEndpoint::ChatCompletions => "chat_completions",
            ModelEndpoint::Responses => "responses",
            ModelEndpoint::Messages => "messages",
            ModelEndpoint::Unknown => "unknown",
        }
    }
}
```

**METHOD TO ADD** to `CopilotProvider` impl block:

```rust
impl CopilotProvider {
    /// Get API endpoint URL for specified endpoint type
    ///
    /// # Arguments
    ///
    /// * `endpoint` - Endpoint type to get URL for
    ///
    /// # Returns
    ///
    /// Returns full URL for the endpoint
    fn api_endpoint(&self, endpoint: ModelEndpoint) -> String {
        let base = self.config.read().unwrap()
            .api_base.clone()
            .unwrap_or_else(|| "https://api.githubcopilot.com".to_string());

        let path = match endpoint {
            ModelEndpoint::ChatCompletions => "/chat/completions",
            ModelEndpoint::Responses => "/responses",
            ModelEndpoint::Messages => "/messages",
            ModelEndpoint::Unknown => "/chat/completions", // Fallback
        };

        format!("{}{}", base, path)
    }
}
```

**DEPENDENCIES**: None

**ESTIMATED LINES**: 60 lines

**TESTING REQUIREMENTS**:

**Test 1.3.1: Endpoint URL constants are valid**

```rust
#[test]
fn test_endpoint_url_constants() {
    assert_eq!(COPILOT_COMPLETIONS_URL, "https://api.githubcopilot.com/chat/completions");
    assert_eq!(COPILOT_RESPONSES_URL, "https://api.githubcopilot.com/responses");
    assert!(COPILOT_RESPONSES_URL.starts_with("https://"));
}
```

**Test 1.3.2: ModelEndpoint from_name conversion**

```rust
#[test]
fn test_model_endpoint_from_name() {
    assert_eq!(ModelEndpoint::from_name("responses"), ModelEndpoint::Responses);
    assert_eq!(ModelEndpoint::from_name("chat_completions"), ModelEndpoint::ChatCompletions);
    assert_eq!(ModelEndpoint::from_name("messages"), ModelEndpoint::Messages);
    assert_eq!(ModelEndpoint::from_name("invalid"), ModelEndpoint::Unknown);
}
```

**Test 1.3.3: ModelEndpoint as_str conversion**

```rust
#[test]
fn test_model_endpoint_as_str() {
    assert_eq!(ModelEndpoint::Responses.as_str(), "responses");
    assert_eq!(ModelEndpoint::ChatCompletions.as_str(), "chat_completions");
    assert_eq!(ModelEndpoint::Messages.as_str(), "messages");
    assert_eq!(ModelEndpoint::Unknown.as_str(), "unknown");
}
```

**Test 1.3.4: API endpoint URL construction**

```rust
#[test]
fn test_api_endpoint_url_construction() {
    let config = Arc::new(RwLock::new(CopilotConfig {
        model: "gpt-5-mini".to_string(),
        api_base: None,
        ..Default::default()
    }));

    let provider = CopilotProvider {
        config,
        // ... other fields
    };

    assert_eq!(
        provider.api_endpoint(ModelEndpoint::Responses),
        "https://api.githubcopilot.com/responses"
    );
    assert_eq!(
        provider.api_endpoint(ModelEndpoint::ChatCompletions),
        "https://api.githubcopilot.com/chat/completions"
    );
}
```

**Test 1.3.5: Custom API base URL**

```rust
#[test]
fn test_api_endpoint_with_custom_base() {
    let config = Arc::new(RwLock::new(CopilotConfig {
        model: "gpt-5-mini".to_string(),
        api_base: Some("https://custom.api.com".to_string()),
        ..Default::default()
    }));

    let provider = CopilotProvider {
        config,
        // ... other fields
    };

    assert_eq!(
        provider.api_endpoint(ModelEndpoint::Responses),
        "https://custom.api.com/responses"
    );
}
```

**EXPECTED TEST COUNT**: 5 tests minimum

**DELIVERABLES**:

- `COPILOT_RESPONSES_URL` constant (1 line)
- `ModelEndpoint` enum with 4 variants (10 lines)
- `from_name()` and `as_str()` methods (30 lines)
- `api_endpoint()` method on provider (15 lines)
- Unit tests (120 lines)

**Total**: ~176 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_endpoint_url_constants
cargo test test_model_endpoint_from_name
cargo test test_api_endpoint_url_construction
```

**Expected output**:

- All commands pass
- All 5+ tests pass
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Constant URL matches GitHub Copilot API documentation
- [ ] Enum has all necessary variants
- [ ] Conversion methods handle all cases
- [ ] Custom API base works correctly
- [ ] No hardcoded URLs in methods

#### Task 1.4: Define Error Types

**OBJECTIVE**: Add error variants for responses endpoint error handling.

**FILE TO MODIFY**: `src/error.rs`

**EXACT ERROR VARIANTS TO ADD**:

Add these variants to the `XzatomaError` enum:

```rust
/// Model does not support the requested endpoint
#[error("Model {0} does not support endpoint {1}")]
UnsupportedEndpoint(String, String),

/// Failed to parse Server-Sent Events data
#[error("Failed to parse SSE event: {0}")]
SseParseError(String),

/// Stream was interrupted before completion
#[error("Stream interrupted: {0}")]
StreamInterrupted(String),

/// Response format does not match expected structure
#[error("Invalid response format: {0}")]
InvalidResponseFormat(String),

/// Both completions and responses endpoints failed
#[error("Endpoint fallback failed - both completions and responses returned errors")]
EndpointFallbackFailed,

/// Message format conversion failed
#[error("Message conversion failed: {0}")]
MessageConversionError(String),
```

**DEPENDENCIES**: None

**ESTIMATED LINES**: 30 lines

**TESTING REQUIREMENTS**:

**Test 1.4.1: UnsupportedEndpoint error construction and display**

```rust
#[test]
fn test_unsupported_endpoint_error() {
    let err = XzatomaError::UnsupportedEndpoint(
        "gpt-3.5-turbo".to_string(),
        "responses".to_string()
    );
    let msg = err.to_string();
    assert!(msg.contains("gpt-3.5-turbo"));
    assert!(msg.contains("responses"));
    assert!(msg.contains("does not support"));
}
```

**Test 1.4.2: SseParseError with details**

```rust
#[test]
fn test_sse_parse_error() {
    let err = XzatomaError::SseParseError("Invalid JSON in event".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Failed to parse SSE event"));
    assert!(msg.contains("Invalid JSON"));
}
```

**Test 1.4.3: StreamInterrupted error**

```rust
#[test]
fn test_stream_interrupted_error() {
    let err = XzatomaError::StreamInterrupted("Connection reset".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Stream interrupted"));
    assert!(msg.contains("Connection reset"));
}
```

**Test 1.4.4: InvalidResponseFormat error**

```rust
#[test]
fn test_invalid_response_format_error() {
    let err = XzatomaError::InvalidResponseFormat("Missing required field".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Invalid response format"));
    assert!(msg.contains("Missing required field"));
}
```

**Test 1.4.5: EndpointFallbackFailed error**

```rust
#[test]
fn test_endpoint_fallback_failed_error() {
    let err = XzatomaError::EndpointFallbackFailed;
    let msg = err.to_string();
    assert!(msg.contains("Endpoint fallback failed"));
    assert!(msg.contains("both completions and responses"));
}
```

**Test 1.4.6: MessageConversionError**

```rust
#[test]
fn test_message_conversion_error() {
    let err = XzatomaError::MessageConversionError("Invalid role".to_string());
    let msg = err.to_string();
    assert!(msg.contains("Message conversion failed"));
    assert!(msg.contains("Invalid role"));
}
```

**Test 1.4.7: Error propagation with ? operator**

```rust
#[test]
fn test_error_propagation() -> Result<()> {
    fn failing_function() -> Result<()> {
        Err(XzatomaError::SseParseError("Test error".to_string()))
    }

    let result = failing_function();
    assert!(result.is_err());

    match result {
        Err(XzatomaError::SseParseError(msg)) => {
            assert_eq!(msg, "Test error");
        }
        _ => panic!("Wrong error type"),
    }

    Ok(())
}
```

**EXPECTED TEST COUNT**: 7 tests minimum

**DELIVERABLES**:

- 6 new error variants in `XzatomaError` enum (30 lines)
- Error variant tests (100 lines)

**Total**: ~130 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_unsupported_endpoint_error
cargo test test_sse_parse_error
cargo test test_error_propagation
```

**Expected output**:

- All commands pass
- All 7+ tests pass
- Error messages are descriptive and actionable
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] All error variants compile
- [ ] Error messages follow consistent format
- [ ] Each error includes relevant context
- [ ] Tests verify error message content
- [ ] Errors work with ? operator

### Phase 2: Message Format Conversion

#### Task 2.1: Implement Message Converters

**OBJECTIVE**: Create conversion functions from XZatoma's `Message` type to responses endpoint `ResponseInputItem`.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**LOCATION**: Add after responses endpoint types (after Task 1.1 additions)

**EXACT FUNCTION TO ADD**:

````rust
/// Convert XZatoma messages to responses endpoint input format
///
/// # Arguments
///
/// * `messages` - Vector of XZatoma Message objects
///
/// # Returns
///
/// Returns vector of ResponseInputItem for responses endpoint
///
/// # Errors
///
/// Returns `MessageConversionError` if message format is invalid
///
/// # Examples
///
/// ```
/// let messages = vec![Message::user("Hello")];
/// let input = convert_messages_to_response_input(&messages)?;
/// ```
fn convert_messages_to_response_input(messages: &[Message]) -> Result<Vec<ResponseInputItem>> {
    let mut result = Vec::new();

    for message in messages {
        match message {
            Message::User(content) => {
                result.push(ResponseInputItem::Message {
                    role: "user".to_string(),
                    content: vec![ResponseInputContent::InputText {
                        text: content.clone(),
                    }],
                });
            }
            Message::Assistant(content) => {
                result.push(ResponseInputItem::Message {
                    role: "assistant".to_string(),
                    content: vec![ResponseInputContent::OutputText {
                        text: content.clone(),
                    }],
                });
            }
            Message::System(content) => {
                result.push(ResponseInputItem::Message {
                    role: "system".to_string(),
                    content: vec![ResponseInputContent::InputText {
                        text: content.clone(),
                    }],
                });
            }
            Message::ToolCall { id, name, arguments } => {
                result.push(ResponseInputItem::FunctionCall {
                    call_id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                });
            }
            Message::ToolResult { call_id, output } => {
                result.push(ResponseInputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: output.clone(),
                });
            }
        }
    }

    Ok(result)
}
````

**DEPENDENCIES**: Task 1.1, Task 1.4

**ESTIMATED LINES**: 150 lines (including documentation and error handling)

**TESTING REQUIREMENTS**:

**Test 2.1.1: Convert user message**

```rust
#[test]
fn test_convert_user_message() {
    let messages = vec![Message::User("Hello, world!".to_string())];
    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

    assert_eq!(result.len(), 1);
    match &result[0] {
        ResponseInputItem::Message { role, content } => {
            assert_eq!(role, "user");
            assert_eq!(content.len(), 1);
            match &content[0] {
                ResponseInputContent::InputText { text } => {
                    assert_eq!(text, "Hello, world!");
                }
                _ => panic!("Expected InputText"),
            }
        }
        _ => panic!("Expected Message variant"),
    }
}
```

**Test 2.1.2: Convert assistant message**

```rust
#[test]
fn test_convert_assistant_message() {
    let messages = vec![Message::Assistant("I'm here to help".to_string())];
    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

    assert_eq!(result.len(), 1);
    match &result[0] {
        ResponseInputItem::Message { role, content } => {
            assert_eq!(role, "assistant");
            match &content[0] {
                ResponseInputContent::OutputText { text } => {
                    assert_eq!(text, "I'm here to help");
                }
                _ => panic!("Expected OutputText"),
            }
        }
        _ => panic!("Expected Message variant"),
    }
}
```

**Test 2.1.3: Convert system message**

```rust
#[test]
fn test_convert_system_message() {
    let messages = vec![Message::System("You are a helpful assistant".to_string())];
    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

    assert_eq!(result.len(), 1);
    match &result[0] {
        ResponseInputItem::Message { role, content } => {
            assert_eq!(role, "system");
            match &content[0] {
                ResponseInputContent::InputText { text } => {
                    assert_eq!(text, "You are a helpful assistant");
                }
                _ => panic!("Expected InputText"),
            }
        }
        _ => panic!("Expected Message variant"),
    }
}
```

**Test 2.1.4: Convert tool call message**

```rust
#[test]
fn test_convert_tool_call_message() {
    let messages = vec![Message::ToolCall {
        id: "call_123".to_string(),
        name: "get_weather".to_string(),
        arguments: r#"{"location":"SF"}"#.to_string(),
    }];

    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

    assert_eq!(result.len(), 1);
    match &result[0] {
        ResponseInputItem::FunctionCall { call_id, name, arguments } => {
            assert_eq!(call_id, "call_123");
            assert_eq!(name, "get_weather");
            assert!(arguments.contains("location"));
        }
        _ => panic!("Expected FunctionCall variant"),
    }
}
```

**Test 2.1.5: Convert tool result message**

```rust
#[test]
fn test_convert_tool_result_message() {
    let messages = vec![Message::ToolResult {
        call_id: "call_123".to_string(),
        output: r#"{"temperature":72}"#.to_string(),
    }];

    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");

    assert_eq!(result.len(), 1);
    match &result[0] {
        ResponseInputItem::FunctionCallOutput { call_id, output } => {
            assert_eq!(call_id, "call_123");
            assert!(output.contains("temperature"));
        }
        _ => panic!("Expected FunctionCallOutput variant"),
    }
}
```

**Test 2.1.6: Convert multi-turn conversation**

```rust
#[test]
fn test_convert_conversation() {
    let messages = vec![
        Message::System("You are helpful".to_string()),
        Message::User("Hi".to_string()),
        Message::Assistant("Hello!".to_string()),
        Message::User("How are you?".to_string()),
    ];

    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");
    assert_eq!(result.len(), 4);

    // Verify order preserved
    match &result[0] {
        ResponseInputItem::Message { role, .. } => assert_eq!(role, "system"),
        _ => panic!("Wrong type"),
    }
    match &result[1] {
        ResponseInputItem::Message { role, .. } => assert_eq!(role, "user"),
        _ => panic!("Wrong type"),
    }
}
```

**Test 2.1.7: Handle empty message list**

```rust
#[test]
fn test_convert_empty_messages() {
    let messages: Vec<Message> = vec![];
    let result = convert_messages_to_response_input(&messages).expect("Conversion failed");
    assert_eq!(result.len(), 0);
}
```

**Test 2.1.8: Roundtrip conversion preserves data**

```rust
#[test]
fn test_message_conversion_roundtrip() {
    let original = vec![
        Message::User("Hello".to_string()),
        Message::Assistant("Hi there".to_string()),
    ];

    let input = convert_messages_to_response_input(&original).expect("Convert failed");
    let back = convert_response_input_to_messages(&input).expect("Convert back failed");

    assert_eq!(original.len(), back.len());
    // Note: This test requires Task 2.2 to be complete
}
```

**EXPECTED TEST COUNT**: 8 tests minimum

**DELIVERABLES**:

- `convert_messages_to_response_input()` function (60 lines)
- Comprehensive conversion tests (250 lines)

**Total**: ~310 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_convert_user_message
cargo test test_convert_tool_call_message
cargo test test_convert_conversation
```

**Expected output**:

- All commands pass
- All 8+ tests pass
- No data loss in conversion
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] All Message variants handled
- [ ] Roles correctly mapped
- [ ] Content types correctly selected
- [ ] Tool calls preserve all fields
- [ ] Empty messages handled
- [ ] Order preserved in conversion

#### Task 2.2: Implement Response Converters

**OBJECTIVE**: Create conversion functions from responses endpoint output back to XZatoma `Message` format.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**LOCATION**: Add after Task 2.1 additions

**EXACT FUNCTION TO ADD**:

```rust
/// Convert responses endpoint input items back to XZatoma messages
///
/// # Arguments
///
/// * `input` - Vector of ResponseInputItem from responses endpoint
///
/// # Returns
///
/// Returns vector of XZatoma Message objects
///
/// # Errors
///
/// Returns `MessageConversionError` if format is invalid
fn convert_response_input_to_messages(input: &[ResponseInputItem]) -> Result<Vec<Message>> {
    let mut result = Vec::new();

    for item in input {
        match item {
            ResponseInputItem::Message { role, content } => {
                // Extract text from content
                let text = content.iter()
                    .filter_map(|c| match c {
                        ResponseInputContent::InputText { text } => Some(text.clone()),
                        ResponseInputContent::OutputText { text } => Some(text.clone()),
                        ResponseInputContent::InputImage { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                match role.as_str() {
                    "user" => result.push(Message::User(text)),
                    "assistant" => result.push(Message::Assistant(text)),
                    "system" => result.push(Message::System(text)),
                    _ => {
                        return Err(XzatomaError::MessageConversionError(
                            format!("Unknown role: {}", role)
                        ));
                    }
                }
            }
            ResponseInputItem::FunctionCall { call_id, name, arguments } => {
                result.push(Message::ToolCall {
                    id: call_id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                });
            }
            ResponseInputItem::FunctionCallOutput { call_id, output } => {
                result.push(Message::ToolResult {
                    call_id: call_id.clone(),
                    output: output.clone(),
                });
            }
            ResponseInputItem::Reasoning { content } => {
                // Reasoning content is not stored in Message, skip for now
                // or could be added to metadata if needed
                let _ = content;
            }
        }
    }

    Ok(result)
}

/// Convert StreamEvent to Message
///
/// # Arguments
///
/// * `event` - StreamEvent from SSE stream
///
/// # Returns
///
/// Returns optional Message (None for status/done events)
fn convert_stream_event_to_message(event: &StreamEvent) -> Option<Message> {
    match event {
        StreamEvent::Message { role, content } => {
            let text = content.iter()
                .filter_map(|c| match c {
                    ResponseInputContent::InputText { text } => Some(text.clone()),
                    ResponseInputContent::OutputText { text } => Some(text.clone()),
                    ResponseInputContent::InputImage { .. } => None,
                })
                .collect::<Vec<_>>()
                .join(" ");

            match role.as_str() {
                "user" => Some(Message::User(text)),
                "assistant" => Some(Message::Assistant(text)),
                "system" => Some(Message::System(text)),
                _ => None,
            }
        }
        StreamEvent::FunctionCall { call_id, name, arguments } => {
            Some(Message::ToolCall {
                id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            })
        }
        StreamEvent::Reasoning { .. } | StreamEvent::Status { .. } | StreamEvent::Done => None,
    }
}
```

**DEPENDENCIES**: Task 2.1, Task 1.4

**ESTIMATED LINES**: 150 lines

**TESTING REQUIREMENTS**:

**Test 2.2.1: Convert message items to Messages**

```rust
#[test]
fn test_convert_response_message_to_message() {
    let input = vec![
        ResponseInputItem::Message {
            role: "user".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "Hello".to_string(),
            }],
        },
    ];

    let result = convert_response_input_to_messages(&input).expect("Conversion failed");
    assert_eq!(result.len(), 1);
    match &result[0] {
        Message::User(text) => assert_eq!(text, "Hello"),
        _ => panic!("Wrong message type"),
    }
}
```

**Test 2.2.2: Convert function call items**

```rust
#[test]
fn test_convert_function_call_to_message() {
    let input = vec![
        ResponseInputItem::FunctionCall {
            call_id: "call_456".to_string(),
            name: "search".to_string(),
            arguments: r#"{"query":"test"}"#.to_string(),
        },
    ];

    let result = convert_response_input_to_messages(&input).expect("Conversion failed");
    assert_eq!(result.len(), 1);
    match &result[0] {
        Message::ToolCall { id, name, arguments } => {
            assert_eq!(id, "call_456");
            assert_eq!(name, "search");
            assert!(arguments.contains("query"));
        }
        _ => panic!("Wrong message type"),
    }
}
```

**Test 2.2.3: Convert function output items**

```rust
#[test]
fn test_convert_function_output_to_message() {
    let input = vec![
        ResponseInputItem::FunctionCallOutput {
            call_id: "call_456".to_string(),
            output: r#"{"result":"found"}"#.to_string(),
        },
    ];

    let result = convert_response_input_to_messages(&input).expect("Conversion failed");
    assert_eq!(result.len(), 1);
    match &result[0] {
        Message::ToolResult { call_id, output } => {
            assert_eq!(call_id, "call_456");
            assert!(output.contains("result"));
        }
        _ => panic!("Wrong message type"),
    }
}
```

**Test 2.2.4: Handle multiple content items**

```rust
#[test]
fn test_convert_multiple_content_items() {
    let input = vec![
        ResponseInputItem::Message {
            role: "assistant".to_string(),
            content: vec![
                ResponseInputContent::OutputText { text: "Part 1".to_string() },
                ResponseInputContent::OutputText { text: "Part 2".to_string() },
            ],
        },
    ];

    let result = convert_response_input_to_messages(&input).expect("Conversion failed");
    assert_eq!(result.len(), 1);
    match &result[0] {
        Message::Assistant(text) => {
            assert!(text.contains("Part 1"));
            assert!(text.contains("Part 2"));
        }
        _ => panic!("Wrong message type"),
    }
}
```

**Test 2.2.5: Handle unknown role error**

```rust
#[test]
fn test_convert_unknown_role_error() {
    let input = vec![
        ResponseInputItem::Message {
            role: "unknown_role".to_string(),
            content: vec![ResponseInputContent::InputText {
                text: "test".to_string(),
            }],
        },
    ];

    let result = convert_response_input_to_messages(&input);
    assert!(result.is_err());
    match result {
        Err(XzatomaError::MessageConversionError(msg)) => {
            assert!(msg.contains("Unknown role"));
        }
        _ => panic!("Expected MessageConversionError"),
    }
}
```

**Test 2.2.6: Convert StreamEvent to Message**

```rust
#[test]
fn test_convert_stream_event_message() {
    let event = StreamEvent::Message {
        role: "assistant".to_string(),
        content: vec![ResponseInputContent::OutputText {
            text: "Response".to_string(),
        }],
    };

    let message = convert_stream_event_to_message(&event);
    assert!(message.is_some());
    match message.unwrap() {
        Message::Assistant(text) => assert_eq!(text, "Response"),
        _ => panic!("Wrong type"),
    }
}
```

**Test 2.2.7: Convert StreamEvent function call**

```rust
#[test]
fn test_convert_stream_event_function_call() {
    let event = StreamEvent::FunctionCall {
        call_id: "call_789".to_string(),
        name: "tool".to_string(),
        arguments: "{}".to_string(),
    };

    let message = convert_stream_event_to_message(&event);
    assert!(message.is_some());
    match message.unwrap() {
        Message::ToolCall { id, name, .. } => {
            assert_eq!(id, "call_789");
            assert_eq!(name, "tool");
        }
        _ => panic!("Wrong type"),
    }
}
```

**Test 2.2.8: StreamEvent status returns None**

```rust
#[test]
fn test_convert_stream_event_status_none() {
    let event = StreamEvent::Status {
        status: "processing".to_string(),
    };

    let message = convert_stream_event_to_message(&event);
    assert!(message.is_none());

    let done = StreamEvent::Done;
    let message = convert_stream_event_to_message(&done);
    assert!(message.is_none());
}
```

**EXPECTED TEST COUNT**: 8 tests minimum

**DELIVERABLES**:

- `convert_response_input_to_messages()` function (60 lines)
- `convert_stream_event_to_message()` function (40 lines)
- Comprehensive tests (250 lines)

**Total**: ~350 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_convert_response_message_to_message
cargo test test_convert_stream_event
```

**Expected output**:

- All commands pass
- All 8+ tests pass
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] All ResponseInputItem variants handled
- [ ] Text content properly extracted
- [ ] Multiple content items joined correctly
- [ ] Unknown roles produce errors
- [ ] StreamEvent conversion correct
- [ ] Optional return types handled

#### Task 2.3: Add Tool Definition Conversion

**OBJECTIVE**: Extend tool conversion to support responses endpoint format.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**LOCATION**: Add after Task 2.2 additions

**EXACT FUNCTIONS TO ADD**:

```rust
/// Convert XZatoma tool definitions to responses endpoint format
///
/// # Arguments
///
/// * `tools` - Slice of tool definitions in XZatoma format
///
/// # Returns
///
/// Returns vector of ToolDefinition for responses endpoint
fn convert_tools_to_response_format(tools: &[Tool]) -> Vec<ToolDefinition> {
    tools.iter().map(|tool| {
        ToolDefinition::Function {
            function: FunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
                strict: Some(tool.strict.unwrap_or(false)),
            },
        }
    }).collect()
}

/// Convert tool choice to responses endpoint format
///
/// # Arguments
///
/// * `choice` - Optional tool choice specification
///
/// # Returns
///
/// Returns ToolChoice for responses endpoint
fn convert_tool_choice(choice: Option<&str>) -> Option<ToolChoice> {
    choice.map(|c| match c {
        "auto" => ToolChoice::Auto { auto: true },
        "any" | "required" => ToolChoice::Any { any: true },
        "none" => ToolChoice::None { none: true },
        name => ToolChoice::Named {
            function: FunctionName {
                name: name.to_string(),
            },
        },
    })
}
```

**DEPENDENCIES**: Task 1.1, Task 1.4

**ESTIMATED LINES**: 50 lines

**TESTING REQUIREMENTS**:

**Test 2.3.1: Convert basic tool definition**

```rust
#[test]
fn test_convert_tool_to_response_format() {
    let tools = vec![
        Tool {
            name: "get_weather".to_string(),
            description: "Get current weather".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
            strict: Some(true),
        },
    ];

    let result = convert_tools_to_response_format(&tools);
    assert_eq!(result.len(), 1);

    match &result[0] {
        ToolDefinition::Function { function } => {
            assert_eq!(function.name, "get_weather");
            assert_eq!(function.description, "Get current weather");
            assert_eq!(function.strict, Some(true));
        }
    }
}
```

**Test 2.3.2: Convert tool without strict mode**

```rust
#[test]
fn test_convert_tool_without_strict() {
    let tools = vec![
        Tool {
            name: "search".to_string(),
            description: "Search".to_string(),
            parameters: serde_json::json!({}),
            strict: None,
        },
    ];

    let result = convert_tools_to_response_format(&tools);
    match &result[0] {
        ToolDefinition::Function { function } => {
            assert_eq!(function.strict, Some(false));
        }
    }
}
```

**Test 2.3.3: Convert multiple tools**

```rust
#[test]
fn test_convert_multiple_tools() {
    let tools = vec![
        Tool {
            name: "tool1".to_string(),
            description: "First".to_string(),
            parameters: serde_json::json!({}),
            strict: None,
        },
        Tool {
            name: "tool2".to_string(),
            description: "Second".to_string(),
            parameters: serde_json::json!({}),
            strict: None,
        },
    ];

    let result = convert_tools_to_response_format(&tools);
    assert_eq!(result.len(), 2);
}
```

**Test 2.3.4: Convert tool choice auto**

```rust
#[test]
fn test_convert_tool_choice_auto() {
    let choice = convert_tool_choice(Some("auto"));
    assert!(choice.is_some());
    match choice.unwrap() {
        ToolChoice::Auto { auto } => assert!(auto),
        _ => panic!("Expected Auto variant"),
    }
}
```

**Test 2.3.5: Convert tool choice required/any**

```rust
#[test]
fn test_convert_tool_choice_required() {
    let choice = convert_tool_choice(Some("required"));
    match choice.unwrap() {
        ToolChoice::Any { any } => assert!(any),
        _ => panic!("Expected Any variant"),
    }

    let choice = convert_tool_choice(Some("any"));
    match choice.unwrap() {
        ToolChoice::Any { any } => assert!(any),
        _ => panic!("Expected Any variant"),
    }
}
```

**Test 2.3.6: Convert tool choice none**

```rust
#[test]
fn test_convert_tool_choice_none() {
    let choice = convert_tool_choice(Some("none"));
    match choice.unwrap() {
        ToolChoice::None { none } => assert!(none),
        _ => panic!("Expected None variant"),
    }
}
```

**Test 2.3.7: Convert tool choice named**

```rust
#[test]
fn test_convert_tool_choice_named() {
    let choice = convert_tool_choice(Some("specific_tool"));
    match choice.unwrap() {
        ToolChoice::Named { function } => {
            assert_eq!(function.name, "specific_tool");
        }
        _ => panic!("Expected Named variant"),
    }
}
```

**Test 2.3.8: Convert None tool choice**

```rust
#[test]
fn test_convert_tool_choice_option_none() {
    let choice = convert_tool_choice(None);
    assert!(choice.is_none());
}
```

**EXPECTED TEST COUNT**: 8 tests minimum

**DELIVERABLES**:

- `convert_tools_to_response_format()` function (20 lines)
- `convert_tool_choice()` function (20 lines)
- Comprehensive tests (200 lines)

**Total**: ~240 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_convert_tool_to_response_format
cargo test test_convert_tool_choice
```

**Expected output**:

- All commands pass
- All 8+ tests pass
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Tool definitions correctly converted
- [ ] Strict mode handled properly
- [ ] All tool choice variants supported
- [ ] Named tool choice preserves name
- [ ] Empty tools list handled
- [ ] JSON schema parameters preserved

### Phase 3: Streaming Infrastructure

#### Task 3.1: Add SSE Parsing Support

**OBJECTIVE**: Implement Server-Sent Events parsing for streaming responses.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**DEPENDENCIES REQUIRED**:

Add to `Cargo.toml`:

```toml
futures = "0.3"
```

Add to `src/providers/copilot.rs` imports:

```rust
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;
```

**TYPE ALIAS TO ADD** (near top of file):

```rust
/// Pinned boxed stream of response events
type ResponseStream = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;
```

**EXACT FUNCTION TO ADD**:

````rust
/// Parse SSE (Server-Sent Events) line
///
/// # Arguments
///
/// * `line` - Line from SSE stream
///
/// # Returns
///
/// Returns optional parsed event data
///
/// # Examples
///
/// ```
/// let line = "data: {\"type\":\"message\"}";
/// let data = parse_sse_line(line);
/// ```
fn parse_sse_line(line: &str) -> Option<String> {
    let line = line.trim();

    if line.is_empty() {
        return None;
    }

    // Handle data: lines
    if let Some(data) = line.strip_prefix("data: ") {
        // Check for [DONE] sentinel
        if data.trim() == "[DONE]" {
            return Some("[DONE]".to_string());
        }
        return Some(data.to_string());
    }

    // Ignore event:, id:, and other SSE fields
    if line.starts_with("event:") || line.starts_with("id:") || line.starts_with(":") {
        return None;
    }

    None
}

/// Parse SSE data line to StreamEvent
///
/// # Arguments
///
/// * `data` - JSON data from SSE event
///
/// # Returns
///
/// Returns parsed StreamEvent or error
fn parse_sse_event(data: &str) -> Result<StreamEvent> {
    if data == "[DONE]" {
        return Ok(StreamEvent::Done);
    }

    serde_json::from_str(data).map_err(|e| {
        XzatomaError::SseParseError(format!("Invalid JSON: {}", e))
    })
}
````

**DEPENDENCIES**: Task 1.1, Task 1.4

**ESTIMATED LINES**: 100 lines

**TESTING REQUIREMENTS**:

**Test 3.1.1: Parse data line**

```rust
#[test]
fn test_parse_sse_data_line() {
    let line = "data: {\"type\":\"message\"}";
    let result = parse_sse_line(line);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), r#"{"type":"message"}"#);
}
```

**Test 3.1.2: Parse DONE sentinel**

```rust
#[test]
fn test_parse_sse_done_sentinel() {
    let line = "data: [DONE]";
    let result = parse_sse_line(line);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "[DONE]");
}
```

**Test 3.1.3: Ignore event and id lines**

```rust
#[test]
fn test_parse_sse_ignore_metadata() {
    assert!(parse_sse_line("event: message").is_none());
    assert!(parse_sse_line("id: 123").is_none());
    assert!(parse_sse_line(": comment").is_none());
}
```

**Test 3.1.4: Handle empty lines**

```rust
#[test]
fn test_parse_sse_empty_lines() {
    assert!(parse_sse_line("").is_none());
    assert!(parse_sse_line("   ").is_none());
    assert!(parse_sse_line("\n").is_none());
}
```

**Test 3.1.5: Parse event to StreamEvent**

```rust
#[test]
fn test_parse_sse_event_message() {
    let data = r#"{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello"}]}"#;
    let event = parse_sse_event(data).expect("Parse failed");

    match event {
        StreamEvent::Message { role, content } => {
            assert_eq!(role, "assistant");
            assert_eq!(content.len(), 1);
        }
        _ => panic!("Expected Message variant"),
    }
}
```

**Test 3.1.6: Parse DONE event**

```rust
#[test]
fn test_parse_sse_event_done() {
    let event = parse_sse_event("[DONE]").expect("Parse failed");
    match event {
        StreamEvent::Done => {}
        _ => panic!("Expected Done variant"),
    }
}
```

**Test 3.1.7: Parse invalid JSON error**

```rust
#[test]
fn test_parse_sse_event_invalid_json() {
    let result = parse_sse_event("invalid json");
    assert!(result.is_err());
    match result {
        Err(XzatomaError::SseParseError(msg)) => {
            assert!(msg.contains("Invalid JSON"));
        }
        _ => panic!("Expected SseParseError"),
    }
}
```

**Test 3.1.8: Parse function call event**

```rust
#[test]
fn test_parse_sse_event_function_call() {
    let data = r#"{"type":"function_call","call_id":"c1","name":"tool","arguments":"{}"}"#;
    let event = parse_sse_event(data).expect("Parse failed");

    match event {
        StreamEvent::FunctionCall { call_id, name, arguments } => {
            assert_eq!(call_id, "c1");
            assert_eq!(name, "tool");
        }
        _ => panic!("Expected FunctionCall variant"),
    }
}
```

**EXPECTED TEST COUNT**: 8 tests minimum

**DELIVERABLES**:

- `ResponseStream` type alias (1 line)
- `parse_sse_line()` function (30 lines)
- `parse_sse_event()` function (15 lines)
- SSE parsing tests (200 lines)

**Total**: ~246 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_parse_sse
cargo doc --no-deps
```

**Expected output**:

- All commands pass
- All 8+ tests pass
- Documentation builds successfully
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] futures dependency added to Cargo.toml
- [ ] SSE format correctly parsed
- [ ] DONE sentinel handled
- [ ] Invalid JSON produces errors
- [ ] All event types parsed
- [ ] Empty/whitespace lines ignored

#### Task 3.2: Implement Stream Response Function

**OBJECTIVE**: Create async streaming function for responses endpoint.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT FUNCTION TO ADD**:

```rust
impl CopilotProvider {
    /// Stream responses from GitHub Copilot responses endpoint
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `input` - Converted message input items
    /// * `tools` - Tool definitions
    ///
    /// # Returns
    ///
    /// Returns pinned boxed stream of StreamEvent items
    ///
    /// # Errors
    ///
    /// Returns `SseParseError` if SSE parsing fails
    /// Returns `StreamInterrupted` if connection drops
    async fn stream_response(
        &self,
        model: &str,
        input: Vec<ResponseInputItem>,
        tools: Vec<ToolDefinition>,
    ) -> Result<ResponseStream> {
        let url = self.api_endpoint(ModelEndpoint::Responses);
        let token = self.get_copilot_token().await?;

        let config = self.config.read().unwrap();
        let include_reasoning = config.include_reasoning;
        let reasoning_effort = config.reasoning_effort.clone();
        drop(config);

        // Build request
        let request = ResponsesRequest {
            model: model.to_string(),
            input,
            stream: true,
            temperature: None,
            tools: if tools.is_empty() { None } else { Some(tools) },
            tool_choice: None,
            reasoning: if include_reasoning {
                Some(ReasoningConfig {
                    effort: reasoning_effort,
                })
            } else {
                None
            },
            include: None,
        };

        // Make HTTP request with streaming
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "xzatoma/0.1.0")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| XzatomaError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::ApiError(
                format!("HTTP {}: {}", status, body)
            ));
        }

        // Create async stream from response body
        let stream = response.bytes_stream();
        let event_stream = stream
            .map(|chunk_result| {
                chunk_result.map_err(|e| XzatomaError::StreamInterrupted(e.to_string()))
            })
            .scan(String::new(), |buffer, chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        // Add chunk to buffer
                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Extract complete lines
                        let mut events = Vec::new();
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].to_string();
                            buffer.drain(..=pos);

                            if let Some(data) = parse_sse_line(&line) {
                                match parse_sse_event(&data) {
                                    Ok(event) => events.push(Ok(event)),
                                    Err(e) => events.push(Err(e)),
                                }
                            }
                        }

                        Some(futures::stream::iter(events))
                    }
                    Err(e) => Some(futures::stream::iter(vec![Err(e)])),
                }
            })
            .flatten();

        Ok(Box::pin(event_stream))
    }
}
```

**DEPENDENCIES**: Task 3.1, Task 1.4

**ESTIMATED LINES**: 150 lines

**TESTING REQUIREMENTS**:

Note: Full integration tests require mock server. Unit tests focus on logic.

**Test 3.2.1: Build request correctly**

```rust
#[test]
fn test_build_responses_request() {
    let input = vec![ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ResponseInputContent::InputText {
            text: "Test".to_string(),
        }],
    }];

    let request = ResponsesRequest {
        model: "gpt-5-mini".to_string(),
        input,
        stream: true,
        temperature: None,
        tools: None,
        tool_choice: None,
        reasoning: Some(ReasoningConfig {
            effort: Some("medium".to_string()),
        }),
        include: None,
    };

    let json = serde_json::to_string(&request).expect("Serialize failed");
    assert!(json.contains("\"stream\":true"));
    assert!(json.contains("\"model\":\"gpt-5-mini\""));
}
```

**Test 3.2.2: Include tools in request**

```rust
#[test]
fn test_responses_request_with_tools() {
    let tools = vec![ToolDefinition::Function {
        function: FunctionDefinition {
            name: "test".to_string(),
            description: "Test tool".to_string(),
            parameters: serde_json::json!({}),
            strict: None,
        },
    }];

    let request = ResponsesRequest {
        model: "gpt-5-mini".to_string(),
        input: vec![],
        stream: true,
        temperature: None,
        tools: Some(tools),
        tool_choice: None,
        reasoning: None,
        include: None,
    };

    let json = serde_json::to_string(&request).expect("Serialize failed");
    assert!(json.contains("\"tools\""));
    assert!(json.contains("\"test\""));
}
```

**Test 3.2.3: Buffer accumulation logic**

```rust
#[test]
fn test_sse_buffer_accumulation() {
    let mut buffer = String::new();

    // Partial line
    buffer.push_str("data: {\"type\":");
    assert!(buffer.find('\n').is_none());

    // Complete line
    buffer.push_str("\"message\"}\n");
    let pos = buffer.find('\n').unwrap();
    let line = &buffer[..pos];
    assert!(line.contains("data:"));
}
```

**EXPECTED TEST COUNT**: 3 tests minimum (integration tests in Phase 4)

**DELIVERABLES**:

- `stream_response()` async method (120 lines)
- Request building tests (80 lines)

**Total**: ~200 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_build_responses_request
```

**Expected output**:

- All commands pass
- Tests pass
- Async/await syntax correct
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Async function signature correct
- [ ] Stream type properly pinned
- [ ] Headers set correctly
- [ ] Error handling comprehensive
- [ ] Buffer logic handles partial lines
- [ ] SSE parsing integrated

#### Task 3.3: Add Stream Completion Function

**OBJECTIVE**: Implement streaming for completions endpoint (backward compatibility).

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT FUNCTION TO ADD**:

```rust
impl CopilotProvider {
    /// Stream completions from chat/completions endpoint
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `messages` - Message history
    /// * `tools` - Tool definitions
    ///
    /// # Returns
    ///
    /// Returns pinned boxed stream of completion chunks
    ///
    /// # Errors
    ///
    /// Returns `SseParseError` if SSE parsing fails
    async fn stream_completion(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<ResponseStream> {
        let url = self.api_endpoint(ModelEndpoint::ChatCompletions);
        let token = self.get_copilot_token().await?;

        // Build completions request (existing format)
        let request = CopilotRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: true,
            temperature: None,
            tools: if tools.is_empty() { None } else { Some(tools.to_vec()) },
            tool_choice: None,
        };

        // Make HTTP request
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "xzatoma/0.1.0")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| XzatomaError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::ApiError(
                format!("HTTP {}: {}", status, body)
            ));
        }

        // Create stream (similar to stream_response but for completions format)
        let stream = response.bytes_stream();
        let event_stream = stream
            .map(|chunk_result| {
                chunk_result.map_err(|e| XzatomaError::StreamInterrupted(e.to_string()))
            })
            .scan(String::new(), |buffer, chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        let mut events = Vec::new();
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].to_string();
                            buffer.drain(..=pos);

                            if let Some(data) = parse_sse_line(&line) {
                                // Parse as StreamEvent (convert from completion format)
                                match parse_sse_event(&data) {
                                    Ok(event) => events.push(Ok(event)),
                                    Err(e) => events.push(Err(e)),
                                }
                            }
                        }

                        Some(futures::stream::iter(events))
                    }
                    Err(e) => Some(futures::stream::iter(vec![Err(e)])),
                }
            })
            .flatten();

        Ok(Box::pin(event_stream))
    }
}
```

**DEPENDENCIES**: Task 3.1, Task 3.2

**ESTIMATED LINES**: 100 lines

**TESTING REQUIREMENTS**:

**Test 3.3.1: Build completions request**

```rust
#[test]
fn test_build_completions_request() {
    let messages = vec![Message::User("Hello".to_string())];

    let request = CopilotRequest {
        model: "gpt-5-mini".to_string(),
        messages,
        stream: true,
        temperature: None,
        tools: None,
        tool_choice: None,
    };

    let json = serde_json::to_string(&request).expect("Serialize failed");
    assert!(json.contains("\"stream\":true"));
    assert!(json.contains("\"messages\""));
}
```

**EXPECTED TEST COUNT**: 1 test minimum (integration tests in Phase 4)

**DELIVERABLES**:

- `stream_completion()` async method (80 lines)
- Request building test (30 lines)

**Total**: ~110 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_build_completions_request
```

**Expected output**:

- All commands pass
- Test passes
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Completions endpoint URL correct
- [ ] Request format matches existing
- [ ] Stream processing similar to responses
- [ ] Error handling consistent

### Phase 4: Provider Integration

#### Task 4.1: Add Endpoint Selection Logic

**OBJECTIVE**: Implement intelligent endpoint selection based on model capabilities.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT METHOD TO ADD** to `CopilotProvider` impl:

```rust
impl CopilotProvider {
    /// Select best endpoint for model
    ///
    /// # Arguments
    ///
    /// * `model_name` - Model identifier to check
    ///
    /// # Returns
    ///
    /// Returns selected ModelEndpoint
    ///
    /// # Strategy
    ///
    /// 1. Check if model supports responses endpoint
    /// 2. If yes and streaming enabled, prefer responses
    /// 3. Otherwise fall back to chat_completions
    async fn select_endpoint(&self, model_name: &str) -> Result<ModelEndpoint> {
        let models = self.list_models().await?;

        // Find model info
        let model_info = models.iter()
            .find(|m| m.id == model_name)
            .ok_or_else(|| XzatomaError::InvalidModel(model_name.to_string()))?;

        // Check supported endpoints in metadata
        if let Some(endpoints_value) = model_info.provider_specific.get("supported_endpoints") {
            if let Some(endpoints) = endpoints_value.as_array() {
                let endpoint_names: Vec<String> = endpoints
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();

                // Prefer responses endpoint if supported
                if endpoint_names.contains(&"responses".to_string()) {
                    return Ok(ModelEndpoint::Responses);
                }

                // Fall back to chat_completions
                if endpoint_names.contains(&"chat_completions".to_string()) {
                    return Ok(ModelEndpoint::ChatCompletions);
                }
            }
        }

        // Default to chat_completions for backward compatibility
        Ok(ModelEndpoint::ChatCompletions)
    }

    /// Check if model supports specific endpoint
    ///
    /// # Arguments
    ///
    /// * `model_name` - Model identifier
    /// * `endpoint` - Endpoint to check
    ///
    /// # Returns
    ///
    /// Returns true if model supports endpoint
    async fn model_supports_endpoint(
        &self,
        model_name: &str,
        endpoint: ModelEndpoint,
    ) -> Result<bool> {
        let models = self.list_models().await?;

        let model_info = models.iter()
            .find(|m| m.id == model_name)
            .ok_or_else(|| XzatomaError::InvalidModel(model_name.to_string()))?;

        if let Some(endpoints_value) = model_info.provider_specific.get("supported_endpoints") {
            if let Some(endpoints) = endpoints_value.as_array() {
                let endpoint_name = endpoint.as_str();
                return Ok(endpoints.iter().any(|v| {
                    v.as_str().map(|s| s == endpoint_name).unwrap_or(false)
                }));
            }
        }

        // Assume chat_completions supported by default
        Ok(endpoint == ModelEndpoint::ChatCompletions)
    }
}
```

**DEPENDENCIES**: Task 1.2, Task 1.3

**ESTIMATED LINES**: 100 lines

**TESTING REQUIREMENTS**:

**Test 4.1.1: Select responses endpoint when supported**

```rust
#[tokio::test]
async fn test_select_endpoint_prefers_responses() {
    // Create provider with mock model data
    let provider = create_test_provider_with_models(vec![
        ModelInfo {
            id: "test-model".to_string(),
            provider_specific: {
                let mut map = HashMap::new();
                map.insert(
                    "supported_endpoints".to_string(),
                    serde_json::json!(["chat_completions", "responses"]),
                );
                map
            },
            ..Default::default()
        },
    ]);

    let endpoint = provider.select_endpoint("test-model").await.expect("Select failed");
    assert_eq!(endpoint, ModelEndpoint::Responses);
}
```

**Test 4.1.2: Fall back to completions when responses not supported**

```rust
#[tokio::test]
async fn test_select_endpoint_fallback_to_completions() {
    let provider = create_test_provider_with_models(vec![
        ModelInfo {
            id: "old-model".to_string(),
            provider_specific: {
                let mut map = HashMap::new();
                map.insert(
                    "supported_endpoints".to_string(),
                    serde_json::json!(["chat_completions"]),
                );
                map
            },
            ..Default::default()
        },
    ]);

    let endpoint = provider.select_endpoint("old-model").await.expect("Select failed");
    assert_eq!(endpoint, ModelEndpoint::ChatCompletions);
}
```

**Test 4.1.3: Default to completions when no endpoint data**

```rust
#[tokio::test]
async fn test_select_endpoint_default_completions() {
    let provider = create_test_provider_with_models(vec![
        ModelInfo {
            id: "unknown-model".to_string(),
            provider_specific: HashMap::new(),
            ..Default::default()
        },
    ]);

    let endpoint = provider.select_endpoint("unknown-model").await.expect("Select failed");
    assert_eq!(endpoint, ModelEndpoint::ChatCompletions);
}
```

**Test 4.1.4: Error on unknown model**

```rust
#[tokio::test]
async fn test_select_endpoint_unknown_model_error() {
    let provider = create_test_provider_with_models(vec![]);

    let result = provider.select_endpoint("nonexistent").await;
    assert!(result.is_err());
    match result {
        Err(XzatomaError::InvalidModel(name)) => {
            assert_eq!(name, "nonexistent");
        }
        _ => panic!("Expected InvalidModel error"),
    }
}
```

**Test 4.1.5: Check model supports endpoint**

```rust
#[tokio::test]
async fn test_model_supports_endpoint() {
    let provider = create_test_provider_with_models(vec![
        ModelInfo {
            id: "test-model".to_string(),
            provider_specific: {
                let mut map = HashMap::new();
                map.insert(
                    "supported_endpoints".to_string(),
                    serde_json::json!(["responses"]),
                );
                map
            },
            ..Default::default()
        },
    ]);

    let supports = provider.model_supports_endpoint("test-model", ModelEndpoint::Responses)
        .await.expect("Check failed");
    assert!(supports);

    let supports = provider.model_supports_endpoint("test-model", ModelEndpoint::Messages)
        .await.expect("Check failed");
    assert!(!supports);
}
```

**EXPECTED TEST COUNT**: 5 tests minimum

**DELIVERABLES**:

- `select_endpoint()` method (40 lines)
- `model_supports_endpoint()` method (30 lines)
- Endpoint selection tests (150 lines)

**Total**: ~220 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_select_endpoint
```

**Expected output**:

- All commands pass
- All 5+ tests pass
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Responses endpoint preferred when available
- [ ] Falls back to completions gracefully
- [ ] Unknown models produce errors
- [ ] Endpoint support checking works
- [ ] Default behavior is backward compatible

#### Task 4.2: Extend Provider Complete Method

**OBJECTIVE**: Update `complete()` implementation to support both endpoints with automatic selection.

**FILE TO MODIFY**: `src/providers/copilot.rs`

**EXACT METHOD TO UPDATE**:

Replace existing `complete()` implementation with:

```rust
#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<CompletionResponse> {
        let config = self.config.read().unwrap();
        let model = config.model.clone();
        let enable_streaming = config.enable_streaming;
        let enable_fallback = config.enable_endpoint_fallback;
        drop(config);

        // Select endpoint based on model capabilities
        let endpoint = self.select_endpoint(&model).await?;

        // Try primary endpoint
        let result = self.complete_with_endpoint(
            &model,
            messages,
            tools,
            endpoint,
            enable_streaming,
        ).await;

        // Handle fallback if enabled
        match result {
            Ok(response) => Ok(response),
            Err(e) if enable_fallback && endpoint == ModelEndpoint::Responses => {
                // Log fallback attempt
                eprintln!("Warning: Responses endpoint failed, falling back to completions: {}", e);

                // Retry with completions endpoint
                self.complete_with_endpoint(
                    &model,
                    messages,
                    tools,
                    ModelEndpoint::ChatCompletions,
                    enable_streaming,
                ).await.map_err(|fallback_err| {
                    // Both failed
                    eprintln!("Error: Completions endpoint also failed: {}", fallback_err);
                    XzatomaError::EndpointFallbackFailed
                })
            }
            Err(e) => Err(e),
        }
    }
}

impl CopilotProvider {
    /// Complete request using specific endpoint
    async fn complete_with_endpoint(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
        endpoint: ModelEndpoint,
        enable_streaming: bool,
    ) -> Result<CompletionResponse> {
        match endpoint {
            ModelEndpoint::Responses => {
                if enable_streaming {
                    self.complete_responses_streaming(model, messages, tools).await
                } else {
                    self.complete_responses_blocking(model, messages, tools).await
                }
            }
            ModelEndpoint::ChatCompletions => {
                if enable_streaming {
                    self.complete_completions_streaming(model, messages, tools).await
                } else {
                    // Use existing blocking implementation
                    self.complete_completions_blocking(model, messages, tools).await
                }
            }
            _ => Err(XzatomaError::UnsupportedEndpoint(
                model.to_string(),
                endpoint.as_str().to_string(),
            )),
        }
    }

    /// Complete using responses endpoint with streaming
    async fn complete_responses_streaming(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<CompletionResponse> {
        // Convert messages and tools
        let input = convert_messages_to_response_input(messages)?;
        let tool_defs = convert_tools_to_response_format(tools);

        // Start streaming
        let mut stream = self.stream_response(model, input, tool_defs).await?;

        // Aggregate events
        let mut content_parts = Vec::new();
        let mut reasoning_parts = Vec::new();
        let mut tool_calls = Vec::new();

        while let Some(event_result) = stream.next().await {
            let event = event_result?;

            match event {
                StreamEvent::Message { role: _, content } => {
                    for c in content {
                        match c {
                            ResponseInputContent::OutputText { text } => {
                                content_parts.push(text);
                            }
                            _ => {}
                        }
                    }
                }
                StreamEvent::FunctionCall { call_id, name, arguments } => {
                    tool_calls.push(ToolCall {
                        id: call_id,
                        function: FunctionCall { name, arguments },
                    });
                }
                StreamEvent::Reasoning { content } => {
                    for c in content {
                        match c {
                            ResponseInputContent::OutputText { text } => {
                                reasoning_parts.push(text);
                            }
                            _ => {}
                        }
                    }
                }
                StreamEvent::Done => break,
                StreamEvent::Status { .. } => {
                    // Ignore status events
                }
            }
        }

        // Build response message
        let message = if !tool_calls.is_empty() {
            Message::AssistantWithToolCalls {
                content: content_parts.join(""),
                tool_calls,
            }
        } else {
            Message::Assistant(content_parts.join(""))
        };

        Ok(CompletionResponse {
            message,
            usage: None, // TODO: Parse usage from response
            model: Some(model.to_string()),
            reasoning: if reasoning_parts.is_empty() {
                None
            } else {
                Some(reasoning_parts.join(""))
            },
        })
    }

    /// Complete using responses endpoint without streaming
    async fn complete_responses_blocking(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<CompletionResponse> {
        // Convert messages and tools
        let input = convert_messages_to_response_input(messages)?;
        let tool_defs = convert_tools_to_response_format(tools);

        let url = self.api_endpoint(ModelEndpoint::Responses);
        let token = self.get_copilot_token().await?;

        let config = self.config.read().unwrap();
        let include_reasoning = config.include_reasoning;
        let reasoning_effort = config.reasoning_effort.clone();
        drop(config);

        let request = ResponsesRequest {
            model: model.to_string(),
            input,
            stream: false,
            temperature: None,
            tools: if tool_defs.is_empty() { None } else { Some(tool_defs) },
            tool_choice: None,
            reasoning: if include_reasoning {
                Some(ReasoningConfig { effort: reasoning_effort })
            } else {
                None
            },
            include: None,
        };

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "xzatoma/0.1.0")
            .json(&request)
            .send()
            .await
            .map_err(|e| XzatomaError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(XzatomaError::ApiError(format!("HTTP {}: {}", status, body)));
        }

        // Parse response (structure varies - implement based on API docs)
        let _response_data: serde_json::Value = response.json().await
            .map_err(|e| XzatomaError::InvalidResponseFormat(e.to_string()))?;

        // TODO: Parse actual response structure and convert to CompletionResponse
        // Placeholder implementation:
        Ok(CompletionResponse {
            message: Message::Assistant("Response parsing not yet implemented".to_string()),
            usage: None,
            model: Some(model.to_string()),
            reasoning: None,
        })
    }

    /// Complete using completions endpoint with streaming
    async fn complete_completions_streaming(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<CompletionResponse> {
        let mut stream = self.stream_completion(model, messages, tools).await?;

        // Aggregate streaming chunks (similar to responses)
        let mut content = String::new();

        while let Some(event_result) = stream.next().await {
            let event = event_result?;

            if let Some(msg) = convert_stream_event_to_message(&event) {
                match msg {
                    Message::Assistant(text) => content.push_str(&text),
                    _ => {}
                }
            }

            if matches!(event, StreamEvent::Done) {
                break;
            }
        }

        Ok(CompletionResponse {
            message: Message::Assistant(content),
            usage: None,
            model: Some(model.to_string()),
            reasoning: None,
        })
    }

    /// Complete using completions endpoint without streaming (existing implementation)
    async fn complete_completions_blocking(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<CompletionResponse> {
        // Keep existing blocking implementation
        // This is the current implementation - just refactored into separate method
        todo!("Use existing blocking implementation")
    }
}
```

**DEPENDENCIES**: All previous tasks in Phases 1-3

**ESTIMATED LINES**: 200 lines

**TESTING REQUIREMENTS**:

**Test 4.2.1: Complete with responses endpoint**

```rust
#[tokio::test]
async fn test_complete_with_responses_endpoint() {
    // Requires mock server - integration test
    // Verify responses endpoint called
    // Verify streaming aggregation works
}
```

**Test 4.2.2: Complete with fallback**

```rust
#[tokio::test]
async fn test_complete_with_endpoint_fallback() {
    // Mock responses endpoint failure
    // Verify fallback to completions
    // Verify warning logged
}
```

**Test 4.2.3: Fallback disabled**

```rust
#[tokio::test]
async fn test_complete_no_fallback_when_disabled() {
    // Disable fallback in config
    // Mock responses endpoint failure
    // Verify error returned without retry
}
```

**EXPECTED TEST COUNT**: 3 integration tests minimum

**DELIVERABLES**:

- Updated `complete()` method with endpoint selection (40 lines)
- `complete_with_endpoint()` router method (20 lines)
- `complete_responses_streaming()` method (60 lines)
- `complete_responses_blocking()` method (40 lines)
- `complete_completions_streaming()` method (30 lines)
- Integration tests (200 lines)

**Total**: ~390 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_complete
```

**Expected output**:

- All commands pass
- Integration tests pass with mock server
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] Endpoint automatically selected
- [ ] Fallback works correctly
- [ ] Streaming aggregates events
- [ ] Reasoning field populated
- [ ] Tool calls handled
- [ ] Error handling comprehensive

#### Task 4.3: Add Provider Configuration

**OBJECTIVE**: Add configuration fields for responses endpoint control.

**FILE TO MODIFY**: `src/config.rs`

**EXACT CHANGES TO CopilotConfig STRUCT** (line 42-54):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// Model to use for Copilot
    #[serde(default = "default_copilot_model")]
    pub model: String,

    /// Optional API base URL for Copilot endpoints
    #[serde(default)]
    pub api_base: Option<String>,

    // === NEW FIELDS ===

    /// Enable streaming for responses (default: true)
    #[serde(default = "default_enable_streaming")]
    pub enable_streaming: bool,

    /// Enable automatic endpoint fallback (default: true)
    #[serde(default = "default_enable_endpoint_fallback")]
    pub enable_endpoint_fallback: bool,

    /// Reasoning effort level: "low", "medium", "high"
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Include reasoning output in responses (default: true)
    #[serde(default = "default_include_reasoning")]
    pub include_reasoning: bool,
}

// === NEW DEFAULT FUNCTIONS ===

fn default_enable_streaming() -> bool {
    true
}

fn default_enable_endpoint_fallback() -> bool {
    true
}

fn default_include_reasoning() -> bool {
    true
}
```

**UPDATE Default IMPLEMENTATION**:

```rust
impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: default_copilot_model(),
            api_base: None,
            enable_streaming: default_enable_streaming(),
            enable_endpoint_fallback: default_enable_endpoint_fallback(),
            reasoning_effort: None,
            include_reasoning: default_include_reasoning(),
        }
    }
}
```

**FILE TO MODIFY**: `src/providers/copilot.rs`

**UPDATE models_cache_ttl_secs DEFAULT**:

```rust
impl CopilotProvider {
    pub fn new(config: CopilotConfig) -> Result<Self> {
        // ... existing code ...

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            keyring_service,
            keyring_user,
            models_cache: Arc::new(RwLock::new(None)),
            models_cache_ttl_secs: 3600, // CHANGED: from 300 to 3600 (1 hour)
        })
    }
}
```

**ENVIRONMENT VARIABLE MAPPING**:

Document in comments:

```rust
// Environment variable mapping:
// XZATOMA_COPILOT_ENABLE_STREAMING=true
// XZATOMA_COPILOT_ENABLE_ENDPOINT_FALLBACK=true
// XZATOMA_COPILOT_REASONING_EFFORT=medium
// XZATOMA_COPILOT_INCLUDE_REASONING=true
```

**CONFIGURATION FILE FORMAT** (YAML):

Document in `docs/reference/configuration.md`:

```yaml
copilot:
  model: "gpt-5-mini"
  enable_streaming: true
  enable_endpoint_fallback: true
  reasoning_effort: "medium"
  include_reasoning: true
```

**DEPENDENCIES**: None

**ESTIMATED LINES**: 50 lines

**TESTING REQUIREMENTS**:

**Test 4.3.1: Default configuration values**

```rust
#[test]
fn test_copilot_config_defaults() {
    let config = CopilotConfig::default();
    assert_eq!(config.model, "gpt-5-mini");
    assert_eq!(config.enable_streaming, true);
    assert_eq!(config.enable_endpoint_fallback, true);
    assert_eq!(config.reasoning_effort, None);
    assert_eq!(config.include_reasoning, true);
}
```

**Test 4.3.2: Serialize configuration to YAML**

```rust
#[test]
fn test_copilot_config_serialization() {
    let config = CopilotConfig {
        model: "gpt-5-mini".to_string(),
        api_base: None,
        enable_streaming: true,
        enable_endpoint_fallback: false,
        reasoning_effort: Some("high".to_string()),
        include_reasoning: true,
    };

    let yaml = serde_yaml::to_string(&config).expect("Serialize failed");
    assert!(yaml.contains("enable_streaming: true"));
    assert!(yaml.contains("enable_endpoint_fallback: false"));
    assert!(yaml.contains("reasoning_effort: high"));
}
```

**Test 4.3.3: Deserialize configuration from YAML**

```rust
#[test]
fn test_copilot_config_deserialization() {
    let yaml = r#"
        model: custom-model
        enable_streaming: false
        reasoning_effort: medium
    "#;

    let config: CopilotConfig = serde_yaml::from_str(yaml).expect("Deserialize failed");
    assert_eq!(config.model, "custom-model");
    assert_eq!(config.enable_streaming, false);
    assert_eq!(config.reasoning_effort, Some("medium".to_string()));
    // Defaults should be applied
    assert_eq!(config.enable_endpoint_fallback, true);
    assert_eq!(config.include_reasoning, true);
}
```

**Test 4.3.4: Model cache TTL updated**

```rust
#[test]
fn test_provider_cache_ttl() {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config).expect("Provider creation failed");
    assert_eq!(provider.models_cache_ttl_secs, 3600);
}
```

**EXPECTED TEST COUNT**: 4 tests minimum

**DELIVERABLES**:

- Updated `CopilotConfig` struct with 4 new fields (25 lines)
- Default functions (15 lines)
- Updated `Default` impl (10 lines)
- Configuration tests (100 lines)

**Total**: ~150 lines including tests

**SUCCESS CRITERIA**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test test_copilot_config
```

**Expected output**:

- All commands pass
- All 4+ tests pass
- Serialization/deserialization works
- Coverage >80%

**VALIDATION CHECKLIST**:

- [ ] All fields have serde defaults
- [ ] Default functions return correct values
- [ ] YAML serialization works
- [ ] YAML deserialization works
- [ ] Environment variable mapping documented
- [ ] Cache TTL updated to 1 hour

### Phase 5: Documentation and Examples

#### Task 5.1: API Documentation

**OBJECTIVE**: Create comprehensive API reference documentation.

**FILE TO CREATE**: `docs/reference/copilot_provider.md`

**CONTENT**:

````markdown
# Copilot Provider API Reference

## Overview

The Copilot provider implements the GitHub Copilot API with support for both `/chat/completions` and `/responses` endpoints. It automatically selects the best endpoint based on model capabilities and provides streaming support for improved performance.

## Configuration

### CopilotConfig

Configuration structure for Copilot provider.

```rust
pub struct CopilotConfig {
    pub model: String,
    pub api_base: Option<String>,
    pub enable_streaming: bool,
    pub enable_endpoint_fallback: bool,
    pub reasoning_effort: Option<String>,
    pub include_reasoning: bool,
}
```
````

#### Fields

| Field                      | Type             | Default        | Description                                     |
| -------------------------- | ---------------- | -------------- | ----------------------------------------------- |
| `model`                    | `String`         | `"gpt-5-mini"` | Model identifier to use                         |
| `api_base`                 | `Option<String>` | `None`         | Custom API base URL (for testing)               |
| `enable_streaming`         | `bool`           | `true`         | Enable SSE streaming for responses              |
| `enable_endpoint_fallback` | `bool`           | `true`         | Auto-fallback to completions if responses fails |
| `reasoning_effort`         | `Option<String>` | `None`         | Reasoning effort: "low", "medium", "high"       |
| `include_reasoning`        | `bool`           | `true`         | Include reasoning output in responses           |

### Environment Variables

```bash
export XZATOMA_COPILOT_MODEL=gpt-5-mini
export XZATOMA_COPILOT_ENABLE_STREAMING=true
export XZATOMA_COPILOT_ENABLE_ENDPOINT_FALLBACK=true
export XZATOMA_COPILOT_REASONING_EFFORT=medium
export XZATOMA_COPILOT_INCLUDE_REASONING=true
```

### YAML Configuration

```yaml
copilot:
  model: "gpt-5-mini"
  enable_streaming: true
  enable_endpoint_fallback: true
  reasoning_effort: "medium"
  include_reasoning: true
```

## Provider Methods

### complete()

Complete a chat conversation with optional tool calling.

```rust
async fn complete(
    &self,
    messages: &[Message],
    tools: &[Tool],
) -> Result<CompletionResponse>
```

#### Parameters

- `messages` - Conversation history
- `tools` - Available tools for function calling

#### Returns

Returns `CompletionResponse` with:

- `message` - Generated response message
- `usage` - Token usage statistics (if available)
- `model` - Model that generated response
- `reasoning` - Reasoning content (if model supports it)

#### Errors

- `UnsupportedEndpoint` - Model doesn't support any compatible endpoint
- `ApiError` - HTTP or API error
- `SseParseError` - Streaming parse error
- `EndpointFallbackFailed` - Both endpoints failed
- `MessageConversionError` - Message format conversion error

#### Example

```rust
use xzatoma::providers::{CopilotProvider, Provider, Message};

let provider = CopilotProvider::new(config)?;
let messages = vec![Message::user("What is the weather?")];
let response = provider.complete(&messages, &[]).await?;
println!("{}", response.message.content());
```

### list_models()

List available Copilot models with endpoint support information.

```rust
async fn list_models(&self) -> Result<Vec<ModelInfo>>
```

#### Returns

Vector of `ModelInfo` containing:

- `id` - Model identifier
- `capabilities` - Model capabilities
- `provider_specific` - Metadata including `supported_endpoints`

## Endpoint Selection

The provider automatically selects the best endpoint:

1. **Responses Endpoint** (preferred):

   - Used when model advertises "responses" in `supported_endpoints`
   - Provides streaming, reasoning, and advanced features
   - URL: `https://api.githubcopilot.com/responses`

2. **Chat Completions Endpoint** (fallback):

   - Used when responses not supported
   - Compatible with all models
   - URL: `https://api.githubcopilot.com/chat/completions`

3. **Automatic Fallback**:
   - If responses endpoint fails and `enable_endpoint_fallback` is true
   - Automatically retries with completions endpoint
   - Warning logged to stderr

## Streaming

When `enable_streaming` is true (default):

- Uses Server-Sent Events (SSE) for progressive response
- Reduces time-to-first-token
- Aggregates events into final CompletionResponse
- Handles connection interruptions gracefully

## Reasoning Support

For reasoning-capable models:

- Set `include_reasoning: true` (default)
- Optionally set `reasoning_effort` to "low", "medium", or "high"
- Reasoning content returned in `CompletionResponse.reasoning` field
- Useful for models like o1, o1-mini, o1-preview

## Authentication

Uses GitHub OAuth device flow:

1. First run prompts for GitHub authentication
2. Tokens cached securely in system keyring
3. Automatic token refresh when expired
4. No manual token management required

## Error Handling

```rust
match provider.complete(&messages, &tools).await {
    Ok(response) => {
        println!("Success: {}", response.message.content());
        if let Some(reasoning) = response.reasoning {
            println!("Reasoning: {}", reasoning);
        }
    }
    Err(XzatomaError::UnsupportedEndpoint(model, endpoint)) => {
        eprintln!("Model {} doesn't support {}", model, endpoint);
    }
    Err(XzatomaError::EndpointFallbackFailed) => {
        eprintln!("Both endpoints failed");
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Performance

- Model cache TTL: 1 hour (reduces API calls)
- Streaming enabled by default
- Connection pooling via reqwest
- Automatic retry with exponential backoff (TODO)

## Limitations

- Maximum context length varies by model
- Rate limits apply per GitHub account
- Some models may not support all features
- Reasoning only available on specific models

## See Also

- [GitHub Copilot API Documentation](https://docs.github.com/en/copilot)
- [XZatoma Provider Trait](./provider_trait.md)
- [Configuration Guide](../how-to/configure_providers.md)

````

**ESTIMATED LINES**: 500 lines

**DEPENDENCIES**: All implementation tasks

**TESTING REQUIREMENTS**:

**Test 5.1.1: Verify all code examples compile**

```bash
# Extract code examples and verify they compile
cargo test --doc
````

**DELIVERABLES**:

- Complete API reference documentation (500 lines)

**SUCCESS CRITERIA**:

```bash
# Verify markdown formatting
mdl docs/reference/copilot_provider.md

# Verify code examples
cargo test --doc
```

**Expected output**:

- Markdown lint passes
- All code examples compile
- Documentation complete and accurate

**VALIDATION CHECKLIST**:

- [ ] All public methods documented
- [ ] Code examples are runnable
- [ ] Configuration fully explained
- [ ] Error handling documented
- [ ] Performance characteristics noted
- [ ] Links to related docs present

#### Task 5.2: Usage Examples

**OBJECTIVE**: Create practical usage examples.

**FILE TO CREATE**: `docs/explanation/copilot_usage_examples.md`

**CONTENT** (excerpts):

````markdown
# Copilot Provider Usage Examples

## Basic Chat Completion

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    let messages = vec![Message::user("Hello, how are you?")];
    let response = provider.complete(&messages, &[]).await?;

    println!("Assistant: {}", response.message.content());

    Ok(())
}
```
````

## Using Reasoning Models

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig {
        model: "o1-preview".to_string(),
        reasoning_effort: Some("high".to_string()),
        include_reasoning: true,
        ..Default::default()
    };

    let provider = CopilotProvider::new(config)?;

    let messages = vec![
        Message::user("Solve this complex math problem: ..."),
    ];

    let response = provider.complete(&messages, &[]).await?;

    println!("Answer: {}", response.message.content());

    if let Some(reasoning) = response.reasoning {
        println!("\nReasoning process:");
        println!("{}", reasoning);
    }

    Ok(())
}
```

## Tool Calling Example

[Full examples with tool definitions and handling...]

## Multi-Turn Conversation

[Example showing conversation state management...]

## Disabling Streaming

[Example with streaming disabled...]

## Custom API Base (Testing)

[Example using mock server...]

````

**ESTIMATED LINES**: 200 lines

**DELIVERABLES**:
- Usage examples document (200 lines)

**SUCCESS CRITERIA**:

```bash
cargo test --doc
````

**VALIDATION CHECKLIST**:

- [ ] All examples compile and run
- [ ] Examples cover common use cases
- [ ] Examples follow best practices

#### Task 5.3: Update Project Documentation

**OBJECTIVE**: Update existing documentation with responses endpoint information.

**FILE TO MODIFY**: `README.md`

**CHANGES**:

Add to features list:

```markdown
## Features

- **Multiple AI Providers**: GitHub Copilot (completions + responses endpoints), Ollama
- **Automatic Endpoint Selection**: Intelligently chooses best Copilot endpoint per model
- **Streaming Support**: Server-Sent Events for faster response times
- **Reasoning Models**: Support for models with reasoning capabilities (o1, o1-mini, etc.)
- **Automatic Fallback**: Gracefully falls back if primary endpoint unavailable
- **Tool Calling**: Function calling with automatic format conversion
- **Secure Authentication**: GitHub OAuth with keyring token storage
```

Add to configuration section:

````markdown
### Copilot Configuration

```yaml
copilot:
  model: "gpt-5-mini"
  enable_streaming: true # Use SSE streaming (default: true)
  enable_endpoint_fallback: true # Auto-fallback to completions (default: true)
  reasoning_effort: "medium" # For reasoning models: low/medium/high
  include_reasoning: true # Include reasoning in response (default: true)
```
````

````

**FILE TO CREATE**: `docs/explanation/copilot_responses_implementation.md`

**CONTENT**:

```markdown
# Copilot Responses Endpoint Implementation

## Overview

This document describes the implementation of GitHub Copilot responses endpoint support in XZatoma. The responses endpoint provides enhanced capabilities compared to the traditional chat/completions endpoint, including better streaming support, reasoning output, and optimized request/response formats.

## Implementation Summary

[Summary of what was implemented...]

## Architecture

[Description of how responses endpoint integrates...]

## Endpoint Comparison

[Table comparing /chat/completions vs /responses...]

## Automatic Selection Logic

[Flowchart or description of endpoint selection...]

## Streaming Implementation

[Details of SSE streaming...]

## Testing

[Test coverage and validation...]

## Migration Notes

[How existing code is affected...]

## References

[Links to related docs...]
````

**ESTIMATED LINES**: 400 lines

**DELIVERABLES**:

- Updated README.md (+50 lines, ~30 modified)
- Implementation summary doc (400 lines)

**SUCCESS CRITERIA**:

```bash
# Verify markdown
mdl README.md docs/explanation/copilot_responses_implementation.md
```

**VALIDATION CHECKLIST**:
