# Thinking Mode Implementation Plan

## Overview

This document describes the implementation plan for adding thinking mode support
to XZatoma. Thinking mode allows the agent to surface extended reasoning content
from AI providers that support chain-of-thought or scratchpad outputs, such as
GitHub Copilot models with adaptive thinking, OpenAI reasoning models (the o1
family and successors), and locally-hosted models such as QwQ and DeepSeek-R1
served through Ollama or a compatible OpenAI endpoint.

The implementation adds a new `AgentExecutionEvent::ReasoningEmitted` variant
that carries extracted reasoning text, a tag-stripping pipeline that removes
three vendor-specific inline thinking formats from response text before it
enters conversation history, a `set_thinking_effort` method on the `Provider`
trait backed by concrete implementations in the Copilot and OpenAI providers, a
`thinking_effort` option in the ACP session configuration panel exposed to Zed,
an `AgentThoughtChunk` ACP notification wired from the new event through
`AcpSessionObserver`, and a `--thinking-effort` CLI flag for the `run` and
`chat` subcommands.

The current state is that reasoning content is partially captured: both the
Copilot `ResponsesAccumulator` and the OpenAI `StreamAccumulator` accumulate
reasoning into `CompletionResponse.reasoning`. However, that field is never read
in either agent execution loop. No tag stripping occurs on response text. No
`ReasoningEmitted` event exists, so no observer can forward reasoning to the
caller. The ACP stdio observer has no arm for reasoning events and therefore
never emits `AgentThoughtChunk` to Zed. The `Provider` trait has no method for
configuring thinking effort at runtime, and `OpenAIConfig` has no
`reasoning_effort` field. The Zed config panel has no thinking effort option.
The CLI has no `--thinking-effort` flag.

---

## Current State Analysis

### Existing Infrastructure

| Component                                                            | File                                  | Status |
| -------------------------------------------------------------------- | ------------------------------------- | ------ |
| `CompletionResponse.reasoning: Option<String>`                       | `src/providers/types.rs` L1575        | Done   |
| `CompletionResponse::set_reasoning()`                                | `src/providers/types.rs` L1697-1700   | Done   |
| `ResponsesAccumulator.reasoning` collects `StreamEvent::Reasoning`   | `src/providers/copilot.rs` L1133      | Done   |
| `StreamAccumulator.reasoning` collects `OpenAIStreamDelta.reasoning` | `src/providers/openai.rs` L346        | Done   |
| `CopilotConfig.reasoning_effort: Option<String>`                     | `src/config.rs` L99                   | Done   |
| `CopilotConfig.include_reasoning: bool`                              | `src/config.rs` L106                  | Done   |
| `CopilotModelSupports.adaptive_thinking`                             | `src/providers/copilot.rs` L480       | Done   |
| `CopilotModelSupports.max_thinking_budget`                           | `src/providers/copilot.rs` L483       | Done   |
| `CopilotModelSupports.min_thinking_budget`                           | `src/providers/copilot.rs` L486       | Done   |
| `CopilotProvider.config: Arc<RwLock<CopilotConfig>>`                 | `src/providers/copilot.rs` L210       | Done   |
| `OpenAIProvider.config: Arc<RwLock<OpenAIConfig>>`                   | `src/providers/openai.rs` L528        | Done   |
| `ACP SessionUpdate::AgentThoughtChunk(ContentChunk)`                 | `agent-client-protocol` v0.12.0       | Done   |
| `Agent.provider(&self) -> &dyn Provider`                             | `src/agent/core.rs` L1248-1250        | Done   |
| `ResponsesAccumulator::finalize` sets reasoning on response          | `src/providers/copilot.rs` L1231-1263 | Done   |
| `StreamAccumulator::finalize` sets reasoning on response             | `src/providers/openai.rs` L454-487    | Done   |

### Identified Issues

| Gap                                                                                   | File                         | Location    |
| ------------------------------------------------------------------------------------- | ---------------------------- | ----------- |
| No `AgentExecutionEvent::ReasoningEmitted` variant                                    | `src/agent/events.rs`        | Entire file |
| `execute_with_observer` never reads `completion_response.reasoning`                   | `src/agent/core.rs`          | L614-641    |
| `execute_provider_messages_with_observer` never reads `completion_response.reasoning` | `src/agent/core.rs`          | L952-970    |
| No tag stripping for `<think>`, `<\|thinking\|>`, or `<\|channel>` formats            | Not yet implemented          | N/A         |
| `AcpSessionObserver.on_event()` has no `ReasoningEmitted` arm                         | `src/acp/stdio.rs`           | L1589-1643  |
| No `set_thinking_effort(&self, effort: Option<&str>)` on `Provider` trait             | `src/providers/trait_mod.rs` | L70-270     |
| `OpenAIConfig` has no `reasoning_effort` field                                        | `src/config.rs`              | L213-304    |
| No `CONFIG_THINKING_EFFORT` constant                                                  | `src/acp/session_config.rs`  | N/A         |
| No `thinking_effort` field on `SessionRuntimeState`                                   | `src/acp/session_config.rs`  | L235-250    |
| No `thinking_effort` field on `ConfigChangeEffect`                                    | `src/acp/session_config.rs`  | L180-197    |
| No `--thinking-effort` CLI flag on `run` or `chat`                                    | `src/cli.rs`                 | L38-69      |

---

## Implementation Phases

### Phase 1: Reasoning Event and Tag Extraction Utilities

This phase introduces the new event variant and the reusable tag-stripping
module that all subsequent phases depend on.

#### Task 1.1: Add `ReasoningEmitted` Variant to `AgentExecutionEvent`

- **File**: `src/agent/events.rs`
- **Current state**: The `AgentExecutionEvent` enum (L33) has no variant for
  reasoning content. Observers cannot distinguish reasoning text from assistant
  text.
- **Required change**: Add the following variant between `AssistantTextEmitted`
  and `ToolCallStarted`:

```xzatoma/src/agent/events.rs#L1-1
/// The provider returned reasoning or chain-of-thought content.
///
/// Reasoning content is extracted from either `CompletionResponse.reasoning`
/// or from inline thinking tags stripped from the assistant text before it
/// is stored in conversation history. Observers that do not need reasoning
/// can ignore this event; the `NoOpObserver` discards it.
ReasoningEmitted {
    /// The reasoning or chain-of-thought text.
    text: String,
},
```

- The variant must be added to the `#[derive(Debug, Clone)]` enum body so it
  compiles without additional attributes.
- Update the `test_no_op_observer_accepts_all_events` test in the same file to
  call
  `observer.on_event(AgentExecutionEvent::ReasoningEmitted { text: "thinking...".to_string() })`
  so coverage is maintained.

#### Task 1.2: Create `src/agent/thinking.rs` Tag Extraction Module

- **File**: `src/agent/thinking.rs` (new file)
- **Current state**: Does not exist. No tag stripping occurs anywhere in the
  codebase.
- **Required change**: Create a module that exports one public function,
  `extract_thinking`, which accepts a `&str` and returns
  `(String, Option<String>)` where the first element is the cleaned text with
  all thinking blocks removed and the second element is the concatenated
  reasoning content if any blocks were found.

The function must handle three tag formats in a single pass:

| Format   | Open tag         | Close tag         |
| -------- | ---------------- | ----------------- |
| Standard | `<think>`        | `</think>`        |
| XZatoma  | `<\|thinking\|>` | `<\|/thinking\|>` |
| Block    | `<\|channel>`    | `<channel\|>`     |

Rules:

- All three formats are case-insensitive.
- Nested identical tags are treated as a single block (innermost close tag
  terminates the block).
- Tags must never be injected into subsequent prompts; only the clean text is
  stored in conversation history.
- If no tags are present, the function returns
  `(original_text.to_string(), None)` with zero allocations beyond cloning the
  input.
- When multiple blocks are found, their contents are concatenated with a single
  newline separator in the returned reasoning string.

The module must be declared as `pub(crate)` and registered in `src/agent/mod.rs`
with `pub(crate) mod thinking;`.

#### Task 1.3: Testing Requirements

The following test functions must be present in `src/agent/thinking.rs`:

- `test_extract_thinking_with_no_tags_returns_original_unchanged`
- `test_extract_thinking_strips_standard_think_tags`
- `test_extract_thinking_strips_xzatoma_thinking_tags`
- `test_extract_thinking_strips_channel_block_tags`
- `test_extract_thinking_strips_multiple_blocks_and_concatenates_reasoning`
- `test_extract_thinking_is_case_insensitive_for_open_tags`
- `test_extract_thinking_strips_tags_preserves_surrounding_text`
- `test_extract_thinking_empty_tag_content_yields_none_reasoning`
- `test_extract_thinking_all_three_formats_in_single_string`
- `test_no_op_observer_accepts_reasoning_emitted_event` (in
  `src/agent/events.rs`, updates `test_no_op_observer_accepts_all_events`)

#### Task 1.4: Deliverables

- [ ] `AgentExecutionEvent::ReasoningEmitted { text: String }` variant in
      `src/agent/events.rs`
- [ ] `src/agent/thinking.rs` module with `extract_thinking` function
- [ ] `src/agent/mod.rs` declares `pub(crate) mod thinking`
- [ ] All tests in Task 1.3 present and passing

#### Task 1.5: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma agent::thinking
cargo test -p xzatoma agent::events
```

All four commands must exit with code 0.

---

### Phase 2: Agent Execution Loop Reasoning Plumbing

This phase reads reasoning from `CompletionResponse`, strips inline tags from
assistant text, and emits `ReasoningEmitted` events in both execution loops.

#### Task 2.1: Update `execute_with_observer` to Emit Reasoning

- **File**: `src/agent/core.rs`
- **Function**: `execute_with_observer`, L537-754
- **Current state**: After receiving `completion_response` (L614-619), the loop
  extracts `completion_response.message` into `message` (L620) and immediately
  checks `message.content` for assistant text (L636-641). The field
  `completion_response.reasoning` is never read.
- **Required change**: Between the assignment of
  `let message = completion_response.message;` (L620) and the
  `observer.on_event(... ProviderResponseReceived ...)` call (approximately
  L631), insert the following logic:

  1. Capture `let raw_reasoning = completion_response.reasoning;` before moving
     `completion_response.message`.
  2. If `message.content` is `Some(text)`, call
     `crate::agent::thinking::extract_thinking(&text)` to get
     `(clean_text, tag_reasoning)`.
  3. Replace `message.content` with `Some(clean_text)` when the text changed.
  4. Build a combined reasoning string: if both `raw_reasoning` and
     `tag_reasoning` are `Some`, concatenate with a newline; otherwise take
     whichever is `Some`.
  5. If the combined reasoning is non-empty, emit
     `AgentExecutionEvent::ReasoningEmitted { text: combined_reasoning }` before
     the `ProviderResponseReceived` event.
  6. The `AssistantTextEmitted` event (approximately L637-641) must use the
     cleaned text, not the original.
  7. The `self.conversation.add_message(message.clone())` call (approximately
     L643) must receive the message with the cleaned content so thinking tags
     never enter conversation history.

#### Task 2.2: Update `execute_provider_messages_with_observer` to Emit Reasoning

- **File**: `src/agent/core.rs`
- **Function**: `execute_provider_messages_with_observer`, L851-1086
- **Current state**: The loop has an identical structure to
  `execute_with_observer` starting at approximately L945 for the provider call.
  The same gap exists: `completion_response.reasoning` is never read and no tag
  stripping occurs.
- **Required change**: Apply the identical six-step transformation described in
  Task 2.1 at the equivalent location in this function (approximately L952-970).
  The logic is structurally the same; it must not be deduplicated into a shared
  helper in this phase to keep the diff reviewable.

#### Task 2.3: Default Thinking Effort Behavior in Reasoning Combination

When combining `raw_reasoning` (from `CompletionResponse.reasoning`) and
`tag_reasoning` (from tag stripping), the following precedence applies:

- If `raw_reasoning` is `Some`, use it as the primary source.
- If only `tag_reasoning` is `Some`, use it.
- If both are `Some`, concatenate:
  `format!("{}\n{}", raw_reasoning, tag_reasoning)`.
- If neither is `Some`, do not emit `ReasoningEmitted`.

This behavior must be encoded in a private helper function
`combine_reasoning(raw: Option<String>, tags: Option<String>) -> Option<String>`
placed in `src/agent/core.rs` above the `impl Agent` block so both execution
loops can call it without duplication.

#### Task 2.4: Testing Requirements

The following test functions must be present in `src/agent/core.rs` tests
module:

- `test_execute_with_observer_emits_reasoning_from_completion_response_field`
- `test_execute_with_observer_emits_reasoning_from_think_tags_in_content`
- `test_execute_with_observer_does_not_store_tags_in_conversation`
- `test_execute_with_observer_combines_raw_and_tag_reasoning`
- `test_execute_provider_messages_with_observer_emits_reasoning`
- `test_combine_reasoning_both_some_concatenates`
- `test_combine_reasoning_only_raw_returns_raw`
- `test_combine_reasoning_only_tags_returns_tags`
- `test_combine_reasoning_both_none_returns_none`

Each test that verifies reasoning must use an `EventCollector` struct that
collects all emitted events into a `Vec<AgentExecutionEvent>` and asserts that
the vector contains exactly one `ReasoningEmitted` event with the expected text.

#### Task 2.5: Deliverables

- [ ] `combine_reasoning` private helper in `src/agent/core.rs` above
      `impl Agent`
- [ ] `execute_with_observer` strips tags, reads `.reasoning`, emits
      `ReasoningEmitted`
- [ ] `execute_provider_messages_with_observer` strips tags, reads `.reasoning`,
      emits `ReasoningEmitted`
- [ ] Conversation history never stores thinking tag content
- [ ] All tests in Task 2.4 present and passing

#### Task 2.6: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma agent::core::tests::test_combine_reasoning
cargo test -p xzatoma agent::core::tests::test_execute_with_observer_emits_reasoning
cargo test -p xzatoma agent::core::tests::test_execute_provider_messages_with_observer_emits_reasoning
```

All five commands must exit with code 0.

---

### Phase 3: `set_thinking_effort` on the Provider Trait

This phase adds runtime thinking effort control to all providers through a new
trait method backed by concrete implementations in Copilot and OpenAI.

#### Task 3.1: Add Default No-Op `set_thinking_effort` to `Provider` Trait

- **File**: `src/providers/trait_mod.rs`
- **Current state**: The `Provider` trait (L70-270) has no method for
  configuring thinking effort. The trait includes `supports_streaming`,
  `get_provider_capabilities`, and similar capability methods but nothing for
  reasoning control.
- **Required change**: Add the following method with a default no-op
  implementation after `get_provider_capabilities` (L218-220):

```xzatoma/src/providers/trait_mod.rs#L1-1
/// Set the active thinking effort level for subsequent completions.
///
/// Providers that support configurable reasoning (Copilot adaptive thinking,
/// OpenAI o-series reasoning parameter) must override this method to apply
/// the effort level. The default no-op implementation is used by providers
/// that do not support thinking effort control, such as Ollama.
///
/// # Arguments
///
/// * `effort` - One of `"none"`, `"low"`, `"medium"`, `"high"`, or
///   `"extra_high"`. Pass `None` to clear any previously configured effort
///   and revert to the provider default.
///
/// # Errors
///
/// Returns `XzatomaError::Provider` if the effort string is not recognised
/// or if the internal configuration lock cannot be acquired.
fn set_thinking_effort(&self, _effort: Option<&str>) -> crate::error::Result<()> {
    Ok(())
}
```

The signature uses `&self` (not `&mut self`) because both `CopilotProvider` and
`OpenAIProvider` use `Arc<RwLock<Config>>` for interior mutability and can
mutate their configuration without a mutable receiver.

#### Task 3.2: Implement `set_thinking_effort` in `CopilotProvider`

- **File**: `src/providers/copilot.rs`
- **Current state**: `CopilotProvider` implements `Provider` at L2914-3075. The
  `config` field is `Arc<RwLock<CopilotConfig>>` (L210). `CopilotConfig` already
  has `reasoning_effort: Option<String>` (L99). There is no
  `set_thinking_effort` override.
- **Required change**: Add an override inside
  `impl Provider for CopilotProvider` (L2914-3075) after
  `get_provider_capabilities` (L3041-3050):

```xzatoma/src/providers/copilot.rs#L1-1
fn set_thinking_effort(&self, effort: Option<&str>) -> crate::error::Result<()> {
    let mut config = self.config.write().map_err(|_| {
        crate::error::XzatomaError::Provider(
            "Failed to acquire write lock on CopilotConfig".to_string(),
        )
    })?;
    config.reasoning_effort = effort.map(str::to_string);
    tracing::debug!("Copilot thinking effort set to: {:?}", config.reasoning_effort);
    Ok(())
}
```

The valid effort strings accepted by the Copilot API's `ReasoningConfig.effort`
field are `"low"`, `"medium"`, and `"high"`. The special value `"none"` must be
mapped to `None` by the caller (see Phase 5 Task 5.6) before passing to this
method.

#### Task 3.3: Implement `set_thinking_effort` in `OpenAIProvider`

- **File**: `src/providers/openai.rs`
- **Current state**: `OpenAIProvider` implements `Provider` at L1087-1367. The
  `config` field is `Arc<RwLock<OpenAIConfig>>` (L528). `OpenAIConfig` does not
  yet have a `reasoning_effort` field (added in Phase 4). There is no
  `set_thinking_effort` override.
- **Required change**: Add an override inside `impl Provider for OpenAIProvider`
  after `get_provider_capabilities` (L1357-1366). This task depends on Phase 4
  Task 4.1 having added `reasoning_effort: Option<String>` to `OpenAIConfig`:

```xzatoma/src/providers/openai.rs#L1-1
fn set_thinking_effort(&self, effort: Option<&str>) -> crate::error::Result<()> {
    let mut config = self.config.write().map_err(|_| {
        crate::error::XzatomaError::Provider(
            "Failed to acquire write lock on OpenAIConfig".to_string(),
        )
    })?;
    config.reasoning_effort = effort.map(str::to_string);
    tracing::debug!("OpenAI thinking effort set to: {:?}", config.reasoning_effort);
    Ok(())
}
```

#### Task 3.4: `OllamaProvider` Inherits Default No-Op

- **File**: `src/providers/ollama.rs`
- **Current state**: `OllamaProvider` implements `Provider` but has no thinking
  support. The default no-op from Task 3.1 is sufficient.
- **Required change**: No change to `ollama.rs`. The default no-op
  implementation on the trait handles this provider.

#### Task 3.5: Testing Requirements

The following test functions must be present in the `mod tests` blocks of the
named files:

In `src/providers/trait_mod.rs`:

- `test_set_thinking_effort_default_impl_returns_ok`

In `src/providers/copilot.rs`:

- `test_set_thinking_effort_stores_effort_in_config`
- `test_set_thinking_effort_none_clears_reasoning_effort`
- `test_set_thinking_effort_returns_ok_for_valid_effort`

In `src/providers/openai.rs`:

- `test_set_thinking_effort_stores_effort_in_openai_config`
- `test_set_thinking_effort_none_clears_openai_reasoning_effort`

#### Task 3.6: Deliverables

- [ ] `set_thinking_effort` default no-op on `Provider` trait in
      `src/providers/trait_mod.rs`
- [ ] `CopilotProvider` override sets `config.reasoning_effort`
- [ ] `OpenAIProvider` override sets `config.reasoning_effort` (requires
      Phase 4)
- [ ] `OllamaProvider` uses default no-op without override
- [ ] All tests in Task 3.5 present and passing

#### Task 3.7: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma providers::trait_mod::tests::test_set_thinking_effort
cargo test -p xzatoma providers::copilot::tests::test_set_thinking_effort
cargo test -p xzatoma providers::openai::tests::test_set_thinking_effort
```

All five commands must exit with code 0.

---

### Phase 4: `OpenAIConfig.reasoning_effort` and Request Plumbing

This phase adds the missing `reasoning_effort` field to `OpenAIConfig` and
threads it through the OpenAI request path so models such as `o1` and `o3`
receive the parameter.

#### Task 4.1: Add `reasoning_effort` to `OpenAIConfig`

- **File**: `src/config.rs`
- **Current state**: `OpenAIConfig` (L213-271) has `api_key`, `base_url`,
  `model`, `organization_id`, `enable_streaming`, and `request_timeout_seconds`
  but no `reasoning_effort` field. `CopilotConfig` (L66-107) already has this
  field at L99 with the same semantics.
- **Required change**: Add after `organization_id` (L248):

```xzatoma/src/config.rs#L1-1
/// Reasoning effort level for extended-thinking models.
///
/// Accepted values are `"low"`, `"medium"`, `"high"`, and `"extra_high"`.
/// When set to `"none"` the field is stored as `None` and the parameter is
/// omitted from the request body. When `None`, the parameter is omitted and
/// the server uses its own default.
///
/// Only models that advertise reasoning support (the o1 and o3 families)
/// honour this field. For other models it is silently ignored.
///
/// Set via the `XZATOMA_OPENAI_REASONING_EFFORT` environment variable.
#[serde(default)]
pub reasoning_effort: Option<String>,
```

Update `impl Default for OpenAIConfig` (L293-304) to include
`reasoning_effort: None` in the initializer, and update the doc-comment example
in the struct-level `///` block to include the new field.

#### Task 4.2: Add `XZATOMA_OPENAI_REASONING_EFFORT` Environment Variable

- **File**: `src/config.rs`
- **Current state**: `apply_env_vars` (L1375-2205) handles OpenAI env vars
  including `XZATOMA_OPENAI_API_KEY`, `XZATOMA_OPENAI_BASE_URL`,
  `XZATOMA_OPENAI_MODEL`, `XZATOMA_OPENAI_STREAMING`, and
  `XZATOMA_OPENAI_REQUEST_TIMEOUT`. There is no
  `XZATOMA_OPENAI_REASONING_EFFORT` handler.
- **Required change**: In the OpenAI section of `apply_env_vars`, add:

```xzatoma/src/config.rs#L1-1
if let Ok(val) = std::env::var("XZATOMA_OPENAI_REASONING_EFFORT") {
    if val == "none" {
        self.provider.openai.reasoning_effort = None;
    } else {
        self.provider.openai.reasoning_effort = Some(val);
    }
}
```

Place this block after the existing `XZATOMA_OPENAI_REQUEST_TIMEOUT` handler so
the env vars remain in alphabetical order within the OpenAI block.

#### Task 4.3: Thread `reasoning_effort` Through `OpenAIRequest`

- **File**: `src/providers/openai.rs`
- **Current state**: `OpenAIRequest` (L54-60) has `model`, `messages`, `tools`,
  and `stream` fields. The `complete` method (L1141-1177) reads
  `config.enable_streaming` and `config.model` but never reads
  `config.reasoning_effort`. The OpenAI API accepts a `reasoning_effort` field
  at the top level of the Chat Completions request body for o-series models.
- **Required change**:
  1. Add `reasoning_effort: Option<String>` to `OpenAIRequest` (L54-60) with
     `#[serde(skip_serializing_if = "Option::is_none")]`.
  2. In `complete` (L1141-1177), read `config.reasoning_effort.clone()` in the
     same lock guard block that reads `config.model` and
     `config.enable_streaming`.
  3. Populate `reasoning_effort` on the `OpenAIRequest` struct before passing it
     to `post_completions` or `post_completions_streaming`.

#### Task 4.4: Testing Requirements

The following test functions must be present:

In `src/config.rs` test module:

- `test_openai_config_reasoning_effort_defaults_none`
- `test_openai_config_deserialize_reasoning_effort`
- `test_apply_env_vars_overrides_openai_reasoning_effort`
- `test_apply_env_vars_openai_reasoning_effort_none_clears_field`

In `src/providers/openai.rs` test module:

- `test_openai_request_reasoning_effort_omitted_when_none`
- `test_openai_request_reasoning_effort_included_when_set`

#### Task 4.5: Deliverables

- [ ] `OpenAIConfig.reasoning_effort: Option<String>` added to `src/config.rs`
- [ ] `impl Default for OpenAIConfig` initializes `reasoning_effort: None`
- [ ] `XZATOMA_OPENAI_REASONING_EFFORT` env var handled in `apply_env_vars`
- [ ] `OpenAIRequest.reasoning_effort` field with `skip_serializing_if`
- [ ] `complete` passes `reasoning_effort` from config into the request struct
- [ ] All tests in Task 4.4 present and passing

#### Task 4.6: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma config::tests::test_openai_config_reasoning_effort
cargo test -p xzatoma config::tests::test_apply_env_vars_overrides_openai_reasoning_effort
cargo test -p xzatoma providers::openai::tests::test_openai_request_reasoning_effort
```

All five commands must exit with code 0.

---

### Phase 5: ACP Session Config, Observer Wiring, and Zed Integration

This phase exposes thinking effort as a Zed config panel option, wires the new
`ReasoningEmitted` event through `AcpSessionObserver` to emit
`AgentThoughtChunk` to Zed, and calls `set_thinking_effort` when the user
changes the config panel option.

#### Task 5.1: Add `CONFIG_THINKING_EFFORT` Constant

- **File**: `src/acp/session_config.rs`
- **Current state**: Seven constants exist (L44-62): `CONFIG_SAFETY_POLICY`,
  `CONFIG_TERMINAL_EXECUTION`, `CONFIG_TOOL_ROUTING`, `CONFIG_VISION_INPUT`,
  `CONFIG_SUBAGENT_DELEGATION`, `CONFIG_MCP_TOOLS`, `CONFIG_MAX_TURNS`. There is
  no constant for thinking effort.
- **Required change**: Add after `CONFIG_MAX_TURNS` (L62):

```xzatoma/src/acp/session_config.rs#L1-1
/// Config option ID for the provider thinking effort level.
pub const CONFIG_THINKING_EFFORT: &str = "thinking_effort";
```

#### Task 5.2: Add `thinking_effort` to `SessionRuntimeState`

- **File**: `src/acp/session_config.rs`
- **Current state**: `SessionRuntimeState` (L235-250) has `safety_mode_str`,
  `terminal_mode`, `tool_routing`, `vision_enabled`, `subagents_enabled`,
  `mcp_enabled`, and `max_turns`. It does not have a thinking effort field.
- **Required change**: Add a `thinking_effort: String` field as the last member
  of the struct. The type is `String` rather than `Option<String>` because the
  config panel always displays a selected value; the sentinel value `"none"`
  represents "disabled / not configured". Update
  `SessionRuntimeState::from_config` (L276-296) to initialize
  `thinking_effort: "none".to_string()` because the base `Config` does not have
  a top-level thinking effort field.

#### Task 5.3: Add `thinking_effort` to `ConfigChangeEffect`

- **File**: `src/acp/session_config.rs`
- **Current state**: `ConfigChangeEffect` (L180-197) has seven `Option` fields,
  one per config option. There is no field for thinking effort.
- **Required change**: Add as the last field:

```xzatoma/src/acp/session_config.rs#L1-1
/// New thinking effort value if the setting was changed.
///
/// The string is one of `"none"`, `"low"`, `"medium"`, `"high"`, or
/// `"extra_high"`. When `Some("none")`, callers must pass `None` to
/// `Provider::set_thinking_effort` to disable reasoning parameters.
pub thinking_effort: Option<String>,
```

Update `ConfigChangeEffect::none()` (L200-210) to set `thinking_effort: None`.

#### Task 5.4: Add `build_thinking_effort_option` and Wire Into Builder

- **File**: `src/acp/session_config.rs`
- **Current state**: `build_session_config_options` (L327-339) calls seven
  private builder functions and returns a `Vec` of length 7. The
  `test_build_session_config_options_returns_seven_options` test (L665-668)
  asserts `len() == 7`.
- **Required change**:

  1. Add a private function
     `build_thinking_effort_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption`
     that returns a select option with values `"none"` (display: "None"),
     `"low"` (display: "Low"), `"medium"` (display: "Medium"), `"high"`
     (display: "High"), and `"extra_high"` (display: "Extra High"). The selected
     value is `runtime.thinking_effort.clone()`.

  2. Append a call to `build_thinking_effort_option(runtime)` at the end of the
     `vec!` in `build_session_config_options`. The function now returns a `Vec`
     of length 8.

  3. Update the error message in the `other =>` arm of
     `apply_config_option_change` (L432) to include `CONFIG_THINKING_EFFORT` in
     the list of known IDs.

  4. Update the doc comment example on `build_session_config_options` to assert
     `options.len() == 8`.

  5. Update `test_build_session_config_options_returns_seven_options` to assert
     `options.len() == 8` and rename it to
     `test_build_session_config_options_returns_eight_options`.

#### Task 5.5: Add `thinking_effort` Arm to `apply_config_option_change`

- **File**: `src/acp/session_config.rs`
- **Current state**: `apply_config_option_change` (L381-437) has arms for the
  seven existing config IDs and an `other =>` error arm. There is no arm for
  `CONFIG_THINKING_EFFORT`.
- **Required change**: Add before the `other =>` arm:

```xzatoma/src/acp/session_config.rs#L1-1
CONFIG_THINKING_EFFORT => {
    let effort = parse_thinking_effort_value(value_id)?;
    effect.thinking_effort = Some(effort.clone());
    updated.thinking_effort = effort;
}
```

Add a private function:

```xzatoma/src/acp/session_config.rs#L1-1
fn parse_thinking_effort_value(value_id: &str) -> Result<String> {
    match value_id {
        "none" | "low" | "medium" | "high" | "extra_high" => Ok(value_id.to_string()),
        other => Err(XzatomaError::Config(format!(
            "unknown thinking_effort value: '{other}'; \
             expected one of: 'none', 'low', 'medium', 'high', 'extra_high'"
        ))),
    }
}
```

#### Task 5.6: Wire `ReasoningEmitted` in `AcpSessionObserver`

- **File**: `src/acp/stdio.rs`
- **Current state**: `AcpSessionObserver.on_event` (L1589-1643) matches on
  `AssistantTextEmitted`, `ToolCallStarted`, `ToolCallCompleted`,
  `ToolCallFailed`, `ExecutionCompleted`, `VisionInputAttached`,
  `ExecutionFailed`, and a catch-all `_ => {}`. There is no arm for
  `ReasoningEmitted`.
- **Required change**: Add before the `ExecutionCompleted` arm:

```xzatoma/src/acp/stdio.rs#L1-1
AgentExecutionEvent::ReasoningEmitted { text } => {
    let chunk = acp::ContentChunk::new(acp::ContentBlock::from(text));
    self.send_update(acp::SessionUpdate::AgentThoughtChunk(chunk));
}
```

`acp::SessionUpdate::AgentThoughtChunk` is available in
`agent-client-protocol-schema` v0.12.0 and requires no feature flag.

#### Task 5.7: Wire `ConfigChangeEffect.thinking_effort` to `set_thinking_effort`

- **File**: `src/acp/stdio.rs`
- **Current state**: The `set_session_config_option()` function acquires the
  agent lock and calls `apply_config_option_change`. It then checks each field
  of `ConfigChangeEffect` and applies the corresponding runtime change. There is
  no branch for `thinking_effort`.
- **Required change**: Locate the block inside `set_session_config_option()`
  that handles the returned `ConfigChangeEffect`. After the existing branches
  for `max_turns`, `mcp_enabled`, etc., add:

```xzatoma/src/acp/stdio.rs#L1-1
if let Some(ref effort_str) = effect.thinking_effort {
    let effort_opt = if effort_str == "none" {
        None
    } else {
        Some(effort_str.as_str())
    };
    if let Err(e) = agent.provider().set_thinking_effort(effort_opt) {
        tracing::warn!(
            session_id = %session_id,
            error = %e,
            "Failed to apply thinking effort change"
        );
    }
}
```

Note that `Agent.provider()` returns `&dyn Provider` (L1248-1250), and
`set_thinking_effort` takes `&self`, so no mutability is required on the agent.

#### Task 5.8: Testing Requirements

The following test functions must be present:

In `src/acp/session_config.rs` test module:

- `test_session_runtime_state_thinking_effort_defaults_none`
- `test_build_session_config_options_returns_eight_options`
- `test_build_session_config_options_ids_include_thinking_effort`
- `test_apply_config_option_change_thinking_effort_high`
- `test_apply_config_option_change_thinking_effort_none`
- `test_apply_config_option_change_thinking_effort_extra_high`
- `test_apply_config_option_change_invalid_thinking_effort_returns_error`
- `test_parse_thinking_effort_value_rejects_unknown`

#### Task 5.9: Deliverables

- [ ] `CONFIG_THINKING_EFFORT` constant in `src/acp/session_config.rs`
- [ ] `thinking_effort: String` field on `SessionRuntimeState` defaulting to
      `"none"`
- [ ] `thinking_effort: Option<String>` field on `ConfigChangeEffect` defaulting
      to `None`
- [ ] `build_thinking_effort_option` private builder function
- [ ] `build_session_config_options` returns 8 options
- [ ] `apply_config_option_change` handles `CONFIG_THINKING_EFFORT`
- [ ] `parse_thinking_effort_value` private validator function
- [ ] `AcpSessionObserver.on_event` has `ReasoningEmitted` arm emitting
      `AgentThoughtChunk`
- [ ] `set_session_config_option()` calls `provider.set_thinking_effort()` on
      effect
- [ ] All tests in Task 5.8 present and passing

#### Task 5.10: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma acp::session_config::tests::test_apply_config_option_change_thinking_effort
cargo test -p xzatoma acp::session_config::tests::test_build_session_config_options_returns_eight_options
```

All four commands must exit with code 0.

---

### Phase 6: CLI Flags for `chat` and `run` Subcommands

This phase adds `--thinking-effort` flags to the `chat` and `run` subcommands so
users can configure thinking effort at the command line without editing the YAML
configuration file.

#### Task 6.1: Add `--thinking-effort` to `Commands::Chat`

- **File**: `src/cli.rs`
- **Current state**: `Commands::Chat` (L38-54) has `provider`, `mode`, `safe`,
  and `resume` fields. There is no `thinking_effort` field.
- **Required change**: Add after the `resume` field (L53):

```xzatoma/src/cli.rs#L1-1
/// Thinking effort level for models that support extended reasoning.
///
/// Accepted values: none, low, medium, high, extra_high.
/// When omitted, the value from the configuration file is used.
/// When set to "none", reasoning parameters are cleared even if the
/// configuration file specifies a level.
#[arg(long)]
thinking_effort: Option<String>,
```

#### Task 6.2: Add `--thinking-effort` to `Commands::Run`

- **File**: `src/cli.rs`
- **Current state**: `Commands::Run` (L57-69) has `plan`, `prompt`, and
  `allow_dangerous` fields. There is no `thinking_effort` field.
- **Required change**: Add after the `allow_dangerous` field (L68):

```xzatoma/src/cli.rs#L1-1
/// Thinking effort level for models that support extended reasoning.
///
/// Accepted values: none, low, medium, high, extra_high.
/// When omitted, the value from the configuration file is used.
#[arg(long)]
thinking_effort: Option<String>,
```

#### Task 6.3: Thread `thinking_effort` into Command Handlers

- **Files**: `src/main.rs` or the command handler modules that process
  `Commands::Chat` and `Commands::Run`
- **Current state**: The command handlers for `chat` and `run` build a provider,
  construct an `Agent`, and start execution. They do not call
  `set_thinking_effort` on the provider.
- **Required change**: In each handler, after the provider is constructed and
  before the first `agent.execute` call, check whether `thinking_effort` is
  `Some` and call `provider.set_thinking_effort(Some(effort))` or
  `provider.set_thinking_effort(None)` when the value is `"none"`. Log a warning
  if the call returns an error but do not abort execution.

#### Task 6.4: Validate `--thinking-effort` Flag Values

- The CLI must accept any string value for `--thinking-effort` without rejecting
  at parse time (validation is deferred to the provider).
- If the handler receives an unrecognised value, it must log a warning using
  `tracing::warn!` and proceed with the provider default rather than returning
  an error. This keeps the CLI forward-compatible with new effort levels added
  to providers in future releases.

#### Task 6.5: Testing Requirements

The following test functions must be present in `src/cli.rs` test module:

- `test_cli_parse_chat_with_thinking_effort_high`
- `test_cli_parse_chat_with_thinking_effort_none`
- `test_cli_parse_chat_thinking_effort_defaults_none`
- `test_cli_parse_run_with_thinking_effort_medium`
- `test_cli_parse_run_thinking_effort_defaults_none`

Each test must parse a command line using `Cli::try_parse_from`, extract the
matching `Commands` variant, and assert that the `thinking_effort` field equals
the expected `Option<String>` value.

#### Task 6.6: Deliverables

- [ ] `thinking_effort: Option<String>` field on `Commands::Chat`
- [ ] `thinking_effort: Option<String>` field on `Commands::Run`
- [ ] Command handlers call `provider.set_thinking_effort()` when the flag is
      set
- [ ] Unrecognised effort values log a warning and do not abort
- [ ] All tests in Task 6.5 present and passing

#### Task 6.7: Success Criteria

```text
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma cli::tests::test_cli_parse_chat_with_thinking_effort
cargo test -p xzatoma cli::tests::test_cli_parse_run_with_thinking_effort
```

All four commands must exit with code 0.

---

## Implementation Order Summary

| Phase                                           | Files Modified                                                                      | Depends On            |
| ----------------------------------------------- | ----------------------------------------------------------------------------------- | --------------------- |
| Phase 1: Reasoning event and tag extraction     | `src/agent/events.rs`, `src/agent/thinking.rs`, `src/agent/mod.rs`                  | None                  |
| Phase 2: Agent loop reasoning plumbing          | `src/agent/core.rs`                                                                 | Phase 1               |
| Phase 3: `set_thinking_effort` trait method     | `src/providers/trait_mod.rs`, `src/providers/copilot.rs`, `src/providers/openai.rs` | Phase 4 (OpenAI impl) |
| Phase 4: `OpenAIConfig.reasoning_effort`        | `src/config.rs`, `src/providers/openai.rs`                                          | None                  |
| Phase 5: ACP session config and observer wiring | `src/acp/session_config.rs`, `src/acp/stdio.rs`                                     | Phases 1, 2, 3        |
| Phase 6: CLI flags                              | `src/cli.rs`, command handler modules                                               | Phase 3               |

Phases 1 and 4 have no dependencies and can be implemented in parallel. Phase 2
must follow Phase 1. Phase 3's Copilot implementation has no blocking
dependency; the OpenAI implementation requires Phase 4. Phase 5 requires Phases
1, 2, and 3. Phase 6 requires Phase 3.

The recommended sequential order for a single implementer is:

1. Phase 4 (config struct change, low risk)
2. Phase 1 (new module, isolated)
3. Phase 3 (trait method, depends on Phase 4 for OpenAI)
4. Phase 2 (plumbing in agent core, depends on Phase 1)
5. Phase 5 (ACP wiring, depends on Phases 1, 2, 3)
6. Phase 6 (CLI flags, depends on Phase 3)

---

## Reference: Already-Implemented Features

The following components are fully implemented and must not be re-implemented or
modified as part of this plan except as explicitly instructed in the tasks
above.

| Component                                                                                | File                       | Lines      |
| ---------------------------------------------------------------------------------------- | -------------------------- | ---------- |
| `CompletionResponse.reasoning: Option<String>`                                           | `src/providers/types.rs`   | L1575      |
| `CompletionResponse::set_reasoning(reasoning: String)`                                   | `src/providers/types.rs`   | L1697-1700 |
| `CompletionResponse::with_usage`                                                         | `src/providers/types.rs`   | L1627-1635 |
| `ResponsesAccumulator` collects `StreamEvent::Reasoning` into `.reasoning`               | `src/providers/copilot.rs` | L1163-1204 |
| `ResponsesAccumulator::finalize` calls `base.set_reasoning(reasoning)`                   | `src/providers/copilot.rs` | L1231-1263 |
| `StreamAccumulator` appends `delta.reasoning` to `.reasoning`                            | `src/providers/openai.rs`  | L378-406   |
| `StreamAccumulator::finalize` calls `base.set_reasoning(reasoning)`                      | `src/providers/openai.rs`  | L454-487   |
| `CopilotConfig.reasoning_effort: Option<String>`                                         | `src/config.rs`            | L99        |
| `CopilotConfig.include_reasoning: bool`                                                  | `src/config.rs`            | L106       |
| `CopilotModelSupports.adaptive_thinking`                                                 | `src/providers/copilot.rs` | L480       |
| `CopilotModelSupports.max_thinking_budget`                                               | `src/providers/copilot.rs` | L483       |
| `CopilotModelSupports.min_thinking_budget`                                               | `src/providers/copilot.rs` | L486       |
| `CopilotProvider.config: Arc<RwLock<CopilotConfig>>`                                     | `src/providers/copilot.rs` | L210       |
| `OpenAIProvider.config: Arc<RwLock<OpenAIConfig>>`                                       | `src/providers/openai.rs`  | L528       |
| `complete_responses_blocking` reads `config.reasoning_effort` and sets `ReasoningConfig` | `src/providers/copilot.rs` | L2510-2644 |
| `ACP SessionUpdate::AgentThoughtChunk(ContentChunk)` in `agent-client-protocol` v0.12.0  | Dependency                 | N/A        |
| `Agent.provider(&self) -> &dyn Provider`                                                 | `src/agent/core.rs`        | L1248-1250 |
| `AcpSessionObserver.send_update` helper                                                  | `src/acp/stdio.rs`         | L1580-1586 |
