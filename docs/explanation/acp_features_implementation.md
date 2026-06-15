# ACP Features Implementation Plan

## Overview

This plan fixes four features in the `xzatoma agent` (ACP stdio) mode:

1. **Session Mode Selector** -- The Mode Selector dropdown that lets users
   switch between planning, write, safe, and full-autonomous modes is not
   rendering in Zed because the wrong config option carries the `category: mode`
   hint and no config option represents the actual session modes.

2. **Context Window Display** -- The context window usage bar is not visible in
   the Zed chat window because `PromptResponse.usage` is never populated and the
   `UsageUpdate` notification mechanism relies on an unstable feature that Zed
   may not support in all versions.

3. **Thinking Stream** -- Reasoning content from thinking-capable models is sent
   to Zed as a single batch after the full response arrives. Users cannot see
   the model's reasoning appear live. The agent loop has no streaming path, and
   neither the OpenAI nor Ollama provider feeds per-chunk reasoning to the
   observer during generation.

4. **Streaming Idle Timeout** -- The OpenAI SSE streaming path uses a
   total-request timeout (600 s default) baked into the reqwest `Client` at
   construction time. This is the wrong timeout for streaming: a healthy
   long-running response can be killed before it finishes, and a stalled
   connection that drips one byte every 599 s is never detected. The correct
   model is an idle/read timeout -- fail only if no bytes arrive within N
   seconds, regardless of total elapsed time.

---

## Current State Analysis

### Existing Infrastructure

- `src/acp/session_mode.rs` -- defines `MODE_PLANNING`, `MODE_WRITE`,
  `MODE_SAFE`, `MODE_FULL_AUTONOMOUS`, `build_session_modes()`,
  `build_session_mode_state()`, and `mode_runtime_effect()`. All four modes are
  advertised in `NewSessionResponse.modes`.
- `src/acp/session_config.rs` -- builds eight `SessionConfigOption` entries
  advertised in `NewSessionResponse.configOptions`. Handles
  `set_session_config_option` changes and returns an updated option list.
- `src/acp/stdio.rs` -- wires `set_session_mode` and `set_session_config_option`
  request handlers. Sends `CurrentModeUpdate` and `ConfigOptionUpdate`
  notifications. Sends initial `UsageUpdate` at session creation and
  `UsageUpdate` on every `ContextWindowUpdated` event.
- `ActiveSessionState.current_mode_id` tracks the active mode per session in
  `stdio.rs`. `SessionRuntimeState` does not include `current_mode_id`.

### Identified Issues

#### Issue 1: Wrong `category: mode` assignment

`build_safety_policy_option` in `session_config.rs` calls
`.category(Some(acp::SessionConfigOptionCategory::Mode))`. The `safety_policy`
option (values: always_confirm, confirm_dangerous, never_confirm) is not a
session mode. Zed sees it as the primary mode selector and shows safety policy
choices where the user expects planning / write / safe / full_autonomous.

ACP spec (Session Config Options): "If an Agent provides both `configOptions`
and `modes` in the session response, Clients that support config options SHOULD
use `configOptions` exclusively and ignore `modes`." Since xzatoma sends both,
Zed discards `modes` and renders only `configOptions`. With no proper mode
config option present, the Mode Selector shows the wrong dropdown.

#### Issue 2: No `session_mode` config option

There is no config option with id `session_mode` and `category: mode` that maps
to the planning / write / safe / full_autonomous values. The `modes` field in
`NewSessionResponse` carries these values but Zed ignores them when
`configOptions` is non-empty.

#### Issue 3: `terminal_execution` duplication

`terminal_execution` config option (interactive, restricted_autonomous,
full_autonomous) is always included in the config options list. The design
intent was "terminal_mode: omitted for Zed sessions" because the Mode Selector
handles it through `mode_runtime_effect`. Having both creates redundant controls
for the same underlying setting.

#### Issue 4: `SessionRuntimeState` missing `current_mode_id`

`build_session_config_options` takes only `&SessionRuntimeState`. To build the
new `session_mode` option with the correct `currentValue`, the function needs
the active mode ID. `SessionRuntimeState` does not track it today, requiring
callers to pass it separately.

#### Issue 6: Thinking content not streamed live in Zed

`ReasoningEmitted` is emitted **once, after the entire provider response
arrives**. The agent loop calls `provider.complete()`, which blocks until the
full text (including any `<think>...</think>` blocks or the `reasoning` field)
has been received. The ACP observer then sends a single `AgentThoughtChunk`
carrying all accumulated reasoning. Zed displays this as a batch -- there is no
live appearance of thinking tokens as the model generates them.

The OpenAI provider already implements an SSE streaming accumulator that
collects `delta.reasoning` per chunk, but it buffers the full result before
returning a `CompletionResponse`. The Ollama provider sets `stream: false`.
Neither provider feeds per-chunk reasoning to the observer during generation.

The missing parts are:

1. A new `ReasoningChunkEmitted` event for per-chunk delivery.
2. Provider-level streaming hooks that call the agent loop per chunk instead of
   returning a complete response.
3. The agent loop using `chat_completion_stream` when the provider supports it.
4. A thinking indicator so users know the model is generating (for non-streaming
   or while waiting for the first streaming chunk).

#### Issue 8: SSE streaming uses a total-request timeout instead of an idle timeout

`OpenAIProvider::new` constructs one `reqwest::Client` with
`.timeout(Duration::from_secs(config.request_timeout_seconds))`. Reqwest's
`timeout` applies to the **entire** request lifecycle -- from sending the first
byte of the request body through reading the last byte of the response body.

For SSE streaming this creates two failure modes:

- **Premature cancellation**: A valid but long response (e.g., an extended
  reasoning model generating 10+ minutes of tokens) is killed when the total
  elapsed time exceeds `request_timeout_seconds` (default 600 s), even though
  the connection is healthy and bytes are still arriving.
- **Late stall detection**: A server that stops sending data keeps the
  connection open. The total timeout fires only after the full 600 s, leaving
  the user waiting with no indication that the stream is stuck.

The correct behavior for SSE streaming is:

- **No total-request timeout** on the streaming connection. A long but healthy
  response must be allowed to complete regardless of elapsed time.
- **Per-chunk idle timeout**: fail as soon as no bytes arrive for N consecutive
  seconds. This catches stalled connections quickly without affecting healthy
  long responses.

`OpenAIConfig` has no `stream_idle_timeout_seconds` field today. The streaming
loop in `post_completions_streaming` calls `stream.next().await` with no
per-iteration deadline. The same gap will exist in the new
`complete_with_callbacks` streaming path added by Phase 4.

#### Issue 9: Context window bar not visible

`PromptResponse.usage` (ACP spec field for per-turn token counts) is never set
in `execute_queued_prompt`. The `SessionUpdate::UsageUpdate` notification is
gated on `#[cfg(feature = "unstable_session_usage")]` in the ACP SDK; Zed
versions compiled without this feature silently discard it. Even when
`UsageUpdate` is processed, Zed may require the `usage` field in
`PromptResponse` to render the context window bar on a per-turn basis.

## Implementation Phases

### Phase 1: Core Mode Selector Fix

#### Task 1.1 Remove Incorrect `category: mode` from Safety Policy

In `src/acp/session_config.rs`, remove
`.category(Some(acp::SessionConfigOptionCategory::Mode))` from
`build_safety_policy_option`. Safety policy is a standalone configuration
control, not a session mode. The function should return a plain
`SessionConfigOption` without a category or with no category set.

Update the doc comment on `build_safety_policy_option` to reflect this change.

#### Task 1.2 Add `current_mode_id` to `SessionRuntimeState`

In `src/acp/session_config.rs`, add `pub current_mode_id: String` to
`SessionRuntimeState`. Update `SessionRuntimeState::from_config` to call
`initial_mode_id_from_config` (or accept the mode id as a parameter) so the
field is populated at session creation. Add the field to the
`ConfigChangeEffect` struct as `pub session_mode_id: Option<String>` so that a
mode change via `set_session_config_option` can report the new mode id back to
the caller.

#### Task 1.3 Add `CONFIG_SESSION_MODE` Config Option

In `src/acp/session_config.rs`:

- Add constant `pub const CONFIG_SESSION_MODE: &str = "session_mode";`
- Add private function
  `build_session_mode_option(current_mode_id: &str) -> acp::SessionConfigOption`
  that builds a `SessionConfigOption` with:
  - `id`: `"session_mode"`
  - `name`: `"Session Mode"`
  - `category`: `Some(acp::SessionConfigOptionCategory::Mode)`
  - `currentValue`: the active `current_mode_id`
  - `options`: planning ("Planning"), write ("Write"), safe ("Safe"),
    full_autonomous ("Full Autonomous")
  - `description`: a one-sentence explanation of the mode options
- Update `build_session_config_options` signature to accept
  `current_mode_id: &str` alongside `runtime: &SessionRuntimeState`.
- Insert the new `session_mode` option as the first element in the returned
  `Vec`.
- Remove `build_terminal_execution_option` from the list (it is now controlled
  by the Mode Selector via `mode_runtime_effect`).
- Keep all other options (`safety_policy`, `tool_routing`, `vision_input`,
  `subagent_delegation`, `mcp_tools`, `max_turns`, `thinking_effort`) unchanged.
- Update `apply_config_option_change` to handle `CONFIG_SESSION_MODE`:
  - Call `mode_runtime_effect(&value_id)` to get the `ModeRuntimeEffect`.
  - Populate `effect.safety_mode_str`, `effect.terminal_mode`, and
    `effect.session_mode_id` from the effect.
  - Update `runtime.current_mode_id` to `value_id`.

#### Task 1.4 Update `create_session` to Pass Mode ID

In `src/acp/stdio.rs`, update the call to `build_session_config_options` at
session creation to pass `&current_mode_id`. Ensure
`SessionRuntimeState::from_config` is also updated to carry the initial mode id.

#### Task 1.5 Sync Mode Changes Between `set_session_mode` and `set_session_config_option`

In `src/acp/stdio.rs`:

- In the `set_session_mode` handler, after
  `state.set_session_mode(request).await` succeeds:
  - Rebuild the config options list using the new mode ID.
  - Send a `SessionUpdate::ConfigOptionUpdate` notification so the Mode Selector
    dropdown reflects the updated value.
- In the `set_session_config_option` handler, when `CONFIG_SESSION_MODE`
  changes:
  - Apply `mode_runtime_effect` to update the system prompt and terminal tool
    (same logic as `set_session_mode`).
  - Send a `SessionUpdate::CurrentModeUpdate` notification so the `modes` state
    stays in sync for clients that use the older API.
  - Update `session_lock.current_mode_id` with the new mode value.

#### Task 1.6 Testing Requirements

In `src/acp/session_config.rs`:

- `test_build_session_config_options_first_option_is_session_mode` -- verify the
  first element has `id == CONFIG_SESSION_MODE` and `category == Some(Mode)`.
- `test_build_session_config_options_safety_policy_has_no_mode_category` --
  verify `safety_policy` has no `category` set.
- `test_build_session_config_options_does_not_include_terminal_execution` --
  verify `terminal_execution` is absent from the returned list.
- `test_apply_config_option_change_session_mode_planning` -- set session_mode to
  planning, verify `effect.terminal_mode == ExecutionMode::Interactive`.
- `test_apply_config_option_change_session_mode_full_autonomous` -- verify
  `effect.terminal_mode == ExecutionMode::FullAutonomous` and
  `effect.safety_mode_str == Some("yolo")`.
- `test_apply_config_option_change_session_mode_unknown_value_returns_error` --
  verify invalid mode id returns `Err`.

In `src/acp/stdio.rs`:

- `test_new_session_response_mode_config_option_is_first` -- verify
  `configOptions[0].id == "session_mode"`.
- `test_set_session_config_option_mode_sends_current_mode_update` -- verify
  `CurrentModeUpdate` is sent when session_mode changes.
- `test_set_session_mode_sends_config_option_update` -- verify
  `ConfigOptionUpdate` is sent when `set_session_mode` is called.
- `test_set_session_mode_full_autonomous_updates_session_mode_option` -- verify
  full end-to-end: call `set_session_mode(full_autonomous)`, check
  `ConfigOptionUpdate` carries `currentValue == "full_autonomous"`.

#### Task 1.7 Deliverables

- `src/acp/session_config.rs` -- `CONFIG_SESSION_MODE` constant,
  `build_session_mode_option`, updated `build_session_config_options`, updated
  `apply_config_option_change`, `current_mode_id` field on
  `SessionRuntimeState`, `session_mode_id` field on `ConfigChangeEffect`.
- `src/acp/stdio.rs` -- updated `create_session`, `set_session_mode`, and
  `set_session_config_option` to pass mode ID and send sync notifications.

#### Task 1.8 Success Criteria

- `cargo test --all-features` passes with all new tests green.
- `NewSessionResponse.configOptions[0].id == "session_mode"` and its
  `category == "mode"`.
- `NewSessionResponse.configOptions` does not include `terminal_execution`.
- `safety_policy` option has no `category` field.
- Calling `set_session_mode("full_autonomous")` causes a `ConfigOptionUpdate`
  notification with `configOptions[0].currentValue == "full_autonomous"`.
- Calling `set_session_config_option("session_mode", "full_autonomous")` causes
  a `CurrentModeUpdate("full_autonomous")` notification and updates the terminal
  tool to `ExecutionMode::FullAutonomous`.

---

### Phase 2: Context Window Display

#### Task 2.1 Populate `PromptResponse.usage`

In `src/acp/stdio.rs`, at the end of `execute_queued_prompt`, before returning
`Ok(...)`:

- Call `agent.get_context_info(agent.conversation().max_tokens())` to get used
  and max tokens.
- Construct `acp::Usage::new(total_tokens, input_tokens, output_tokens)` where:
  - `total_tokens` = `used_tokens` (total tokens currently in context)
  - `input_tokens` = `used_tokens` (best approximation; xzatoma does not split
    input/output per-turn without provider-level token accounting)
  - `output_tokens` = 0 (unknown without provider response metadata)
- Set `.usage(usage)` on the `PromptResponse`.
- Gate this code with `#[cfg(feature = "unstable_session_usage")]` to match the
  SDK gating.

Note: input/output token split is an approximation. If providers return token
usage in their responses (e.g., OpenAI `usage` field), a follow-up improvement
can wire accurate counts. The `total_tokens` field is the most important for the
context window bar.

#### Task 2.2 Ensure `UsageUpdate` Is Sent After Every Turn

In `src/acp/stdio.rs`, in `execute_queued_prompt`, after determining
`stop_reason == EndTurn`:

- Acquire the agent lock.
- Compute used and max tokens.
- Send
  `SessionUpdate::UsageUpdate(acp::UsageUpdate::new(used_tokens, max_tokens))`
  via the connection.

This ensures the context window bar is updated after every completed turn, even
if the `PromptResponse.usage` path is not recognized by the client.

#### Task 2.3 Verify Initial `UsageUpdate` at Session Creation

Confirm the existing block in `create_session` (lines 646-661 of `stdio.rs`)
that sends the initial `UsageUpdate` is working correctly. Add a debug log line:

```rust
tracing::debug!(
    session_id = %session_id,
    used_tokens = %used_tokens,
    max_tokens = %max_tokens,
    "ACP stdio: sending initial context window usage update"
);
```

This makes it possible to confirm in logs that the update is sent when running
with `RUST_LOG=xzatoma::acp=debug`.

#### Task 2.4 Testing Requirements

In `src/acp/stdio.rs`:

- `test_execute_queued_prompt_response_includes_usage` -- mock agent execution,
  verify the returned `PromptResponse.usage` is `Some(_)` with
  `total_tokens > 0`.
- `test_execute_queued_prompt_sends_usage_update_on_end_turn` -- verify that a
  `SessionUpdate::UsageUpdate` notification is sent after a successful
  `EndTurn`.
- `test_execute_queued_prompt_no_usage_update_on_cancelled` -- verify that
  `UsageUpdate` is NOT sent when the prompt is cancelled (to avoid misleading
  token counts).

#### Task 2.5 Deliverables

- `src/acp/stdio.rs` -- `execute_queued_prompt` populates `PromptResponse.usage`
  and sends `UsageUpdate` after every `EndTurn`. Debug log added to the initial
  usage update block.

#### Task 2.6 Success Criteria

- `PromptResponse.usage` is non-null for every completed (non-cancelled) prompt
  turn.
- A `UsageUpdate` notification is sent after every `EndTurn`.
- An initial `UsageUpdate` is sent at session creation.
- `cargo test --all-features` passes with all new tests green.

---

### Phase 3: Integration Verification and Documentation

#### Task 3.1 End-to-End Zed Verification

Manually verify in Zed with `xzatoma agent`:

1. Session Mode Selector:
   - Open a new session. Confirm the Mode Selector dropdown shows: Planning,
     Write, Safe, Full Autonomous.
   - Change to Full Autonomous. Confirm terminal commands run without
     restriction.
   - Change back to Planning. Confirm terminal commands are blocked.
2. Context Window Bar:
   - Send a prompt. Confirm the context window bar updates after the response
     completes.
   - The bar should show `used / max` tokens.

Capture log output with `RUST_LOG=xzatoma::acp=debug` to confirm `UsageUpdate`
and `ConfigOptionUpdate` notifications are sent.

#### Task 3.2 Update Documentation

Update `docs/how-to/zed_acp_agent_setup.md`:

- Add a "Session Mode Selector" section explaining the four modes and how to use
  the dropdown.
- Add a "Context Window Bar" section explaining what the bar shows and how to
  interpret it.

Update `docs/reference/acp_configuration.md`:

- Document the `session_mode` config option and its values.
- Note that `terminal_execution` is no longer advertised as a standalone config
  option; it is controlled through the Mode Selector.

Update `docs/reference/acp_api.md` (if it exists) to reflect the new
`session_mode` option.

#### Task 3.3 Testing Requirements

Run the full quality gate sequence:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Run markdownlint and prettier on all modified documentation files:

```bash
markdownlint --fix --config .markdownlint.json docs/explanation/acp_features_implementation.md
prettier --write --parser markdown --prose-wrap always docs/explanation/acp_features_implementation.md
```

#### Task 3.4 Deliverables

- `docs/how-to/zed_acp_agent_setup.md` -- Mode Selector and Context Window
  sections added.
- `docs/reference/acp_configuration.md` -- `session_mode` config option
  documented, `terminal_execution` removal noted.
- This plan document passes all linting and formatting checks.

#### Task 3.5 Success Criteria

- All four quality gate commands pass with zero errors and zero warnings.
- All markdown files pass `markdownlint` and `prettier` checks.
- Manual Zed verification confirms Mode Selector dropdown and context window bar
  are visible and functional.

---

### Phase 4: Thinking Stream in ACP Mode

#### Task 4.1 Verify Existing `AgentThoughtChunk` Visibility

Before implementing streaming, confirm whether a single batched
`AgentThoughtChunk` already renders in Zed's thinking panel.

- Add `tracing::debug!` in `AcpSessionObserver::on_event` for `ReasoningEmitted`
  logging the byte length of the reasoning text.
- In `execute_queued_prompt`, add a debug trace when `ReasoningEmitted` fires so
  the round-trip can be verified with `RUST_LOG=xzatoma::acp=debug`.
- If Zed shows no thinking panel after a model with thinking mode produces
  reasoning content, file the gap as a Zed client issue to investigate
  separately. The xzatoma side is already correct.

#### Task 4.2 Add `ReasoningChunkEmitted` to `AgentExecutionEvent`

In `src/agent/events.rs`, add a new event variant:

```rust
/// A single streaming chunk of reasoning or chain-of-thought content.
///
/// Emitted during streaming generation when the provider delivers reasoning
/// tokens incrementally. Observers that receive this event SHOULD forward each
/// chunk separately for live display. The accumulated full reasoning is also
/// delivered via `ReasoningEmitted` at the end of the turn for non-streaming
/// providers and as a fallback.
ReasoningChunkEmitted {
    /// The incremental reasoning text for this chunk.
    text: String,
},
```

Update `NoOpObserver` to accept and discard the new event. Update all
`match event { ... }` exhaustive arms.

#### Task 4.3 Add Thinking-Start and Thinking-End Indicator Events

In `src/agent/events.rs`, add two lightweight marker events:

```rust
/// The model has started a reasoning / thinking phase.
///
/// Fired when the first reasoning chunk or opening think-tag is detected.
/// ACP observers SHOULD send a visual indicator so the user knows the model
/// is actively thinking before any content appears.
ThinkingStarted,

/// The model has finished its reasoning / thinking phase.
///
/// Fired after the last reasoning chunk before the model transitions to
/// producing the final response.
ThinkingFinished,
```

In `AcpSessionObserver::on_event` in `src/acp/stdio.rs`, handle both events:

- `ThinkingStarted` -- send an empty `AgentThoughtChunk` (or a
  `ContentBlock::from("...".to_string())` placeholder) so Zed opens the thinking
  panel before content arrives. The placeholder is replaced by real chunks as
  they arrive.
- `ThinkingFinished` -- no explicit ACP message is needed; the thinking panel
  closes automatically when the first `AgentMessageChunk` arrives.

#### Task 4.4 Add Streaming Callback to Provider Trait

In `src/providers/trait_mod.rs`, add an optional streaming context parameter to
`chat_completion_stream`:

```rust
async fn complete_with_callbacks(
    &self,
    messages: &[Message],
    tools: &[serde_json::Value],
    on_reasoning_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
    on_content_chunk: Option<&(dyn Fn(String) + Send + Sync)>,
) -> Result<CompletionResponse> {
    // Default: delegate to complete, callbacks never fire.
    self.complete(messages, tools).await
}
```

Providers that support streaming override this method to call
`on_reasoning_chunk` for each `delta.reasoning` token and `on_content_chunk` for
each `delta.content` token as the SSE stream is consumed.

Keep the existing `complete` and `chat_completion_stream` methods unchanged to
preserve backward compatibility.

#### Task 4.5 Wire Streaming Callbacks in OpenAI Provider

In `src/providers/openai.rs`, override `complete_with_callbacks`. When
`on_reasoning_chunk` or `on_content_chunk` is `Some(_)` and `enable_streaming`
is true (and no tools are requested), parse the SSE stream and call the
callbacks per chunk as the `StreamAccumulator` processes each `delta`.

Per-chunk firing order:

1. When `delta.reasoning` is non-empty: call `on_reasoning_chunk(chunk)`.
2. When `delta.content` is non-empty: call `on_content_chunk(chunk)`.
3. After the stream ends: return the final `CompletionResponse` as before.

The `StreamAccumulator` already buffers both fields; the change is to also call
the callbacks during accumulation, not only at the end.

#### Task 4.6 Wire Streaming Callbacks in Ollama Provider

In `src/providers/ollama.rs`, override `complete_with_callbacks`. Enable Ollama
streaming (`stream: true`) for this path, parse the newline-delimited JSON
chunks, and call `on_content_chunk` for each partial `message.content` value.
For thinking-mode models (e.g., DeepSeek-R1, Qwen3), detect opening `<think>` or
`<|thinking|>` tags in the content stream and:

1. Switch to a "reasoning" accumulation mode and call `on_reasoning_chunk` per
   chunk until the closing tag is seen.
2. On the closing tag: switch back to content mode and call `on_content_chunk`
   for subsequent chunks.

The final assembled `CompletionResponse` should have `reasoning` pre-populated
from the accumulated thinking content so the non-streaming code path still works
correctly.

#### Task 4.7 Update Agent Loop to Use `complete_with_callbacks`

In `src/agent/core.rs`, in `execute_provider_messages_with_observer` and
`execute_with_observer`, replace the `provider.complete()` call with
`provider.complete_with_callbacks(...)` when both of the following are true:

- The provider's `supports_streaming()` returns `true`.
- The `observer` is not a `NoOpObserver` (use a marker trait or a `bool` flag on
  the observer to skip overhead when no one is listening).

The callback closures should call `observer.on_event(...)` on the outer
observer:

- `on_reasoning_chunk`: emit `ThinkingStarted` on first call (if not already
  emitted), then `ReasoningChunkEmitted { text: chunk }`.
- `on_content_chunk`: emit `ThinkingFinished` if in reasoning mode, then
  `AssistantTextEmitted { text: chunk }` (reuse existing event).

For providers where `supports_streaming()` is false, fall back to the current
`provider.complete()` path. After the blocking call returns, emit
`ThinkingStarted`, then ONE `ReasoningEmitted` with the full reasoning text,
then `ThinkingFinished` in sequence before emitting `AssistantTextEmitted`.

#### Task 4.8 Testing Requirements

In `src/agent/events.rs`:

- `test_no_op_observer_accepts_reasoning_chunk_emitted` -- verify `NoOpObserver`
  handles `ReasoningChunkEmitted` without panicking.
- `test_no_op_observer_accepts_thinking_started_and_finished` -- verify both
  marker events are silently accepted.

In `src/acp/stdio.rs`:

- `test_acp_observer_sends_agent_thought_chunk_on_reasoning_chunk_emitted` --
  mock observer receives `ReasoningChunkEmitted`, verify `AgentThoughtChunk`
  notification is sent.
- `test_acp_observer_sends_thought_placeholder_on_thinking_started` -- verify
  `ThinkingStarted` triggers an `AgentThoughtChunk` notification.
- `test_acp_observer_no_update_on_thinking_finished` -- verify
  `ThinkingFinished` alone sends no notification.

In `src/providers/openai.rs`:

- `test_complete_with_callbacks_fires_reasoning_chunk_callbacks` -- mock SSE
  stream with `delta.reasoning` chunks, verify callback is called once per
  chunk.
- `test_complete_with_callbacks_fires_content_chunk_callbacks` -- mock SSE with
  `delta.content` chunks, verify content callback fires per chunk.

In `src/agent/core.rs`:

- `test_execute_with_observer_emits_reasoning_chunk_events_for_streaming_provider`
  -- mock a streaming provider that fires callbacks, verify
  `ReasoningChunkEmitted` events are emitted in order.

#### Task 4.9 Deliverables

- `src/agent/events.rs` -- `ReasoningChunkEmitted`, `ThinkingStarted`,
  `ThinkingFinished` variants.
- `src/providers/trait_mod.rs` -- `complete_with_callbacks` default method.
- `src/providers/openai.rs` -- `complete_with_callbacks` override with per-chunk
  callbacks.
- `src/providers/ollama.rs` -- `complete_with_callbacks` override with streaming
  and tag-detection.
- `src/agent/core.rs` -- agent loop uses `complete_with_callbacks` when provider
  supports streaming.
- `src/acp/stdio.rs` -- `AcpSessionObserver` handles `ReasoningChunkEmitted`,
  `ThinkingStarted`, `ThinkingFinished`.

#### Task 4.10 Success Criteria

- `cargo test --all-features` passes with all new tests green.
- When using an OpenAI-compatible model with extended reasoning, thinking tokens
  appear progressively in Zed's thinking panel as the model generates them.
- When using an Ollama model with `<think>` tags, thinking content appears in
  Zed's thinking panel (batch, then progressive once streaming is enabled).
- Non-streaming providers (e.g., Copilot) still display thinking content via the
  single `AgentThoughtChunk` batch path without regression.
- `ThinkingStarted` causes Zed's thinking panel to open before any thinking text
  arrives.

---

### Phase 5: Streaming Idle Timeout

#### Task 5.1 Add `stream_idle_timeout_seconds` to `OpenAIConfig`

In `src/config.rs`:

- Add `pub stream_idle_timeout_seconds: u64` to `OpenAIConfig`.
- Add `fn default_openai_stream_idle_timeout() -> u64 { 30 }` and decorate the
  field with `#[serde(default = "default_openai_stream_idle_timeout")]`.
- Update `OpenAIConfig::default()` to set
  `stream_idle_timeout_seconds: default_openai_stream_idle_timeout()`.
- Update the doc-comment example block in the `OpenAIConfig` module-level doc to
  include the new field.
- Add env-var support in the `apply_env_overrides` block:

```rust
if let Ok(timeout) = std::env::var("XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT") {
    if let Ok(value) = timeout.parse::<u64>() {
        self.provider.openai.stream_idle_timeout_seconds = value;
    } else {
        tracing::warn!("Invalid XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT: {}", timeout);
    }
}
```

#### Task 5.2 Build a Separate Streaming `Client` in `OpenAIProvider`

In `src/providers/openai.rs`, update `OpenAIProvider` to hold two reqwest
clients:

```rust
pub struct OpenAIProvider {
    client: Client,            // non-streaming: total-request timeout
    streaming_client: Client,  // streaming: connect_timeout only, no total timeout
    config: Arc<RwLock<OpenAIConfig>>,
    model_cache: ModelCache,
}
```

In `OpenAIProvider::new`, build the streaming client alongside the existing one:

```rust
let streaming_client = Client::builder()
    .connect_timeout(Duration::from_secs(10))
    .user_agent("xzatoma/0.1.0")
    .build()
    .map_err(|e| XzatomaError::Provider(
        format!("Failed to create streaming HTTP client: {}", e)
    ))?;
```

The streaming client has `connect_timeout` to bound initial TCP handshake and
TLS negotiation, but no `timeout` so the SSE body is never cancelled by reqwest
regardless of how long the response takes. Stall detection is handled at the
application level in Task 5.3.

#### Task 5.3 Wrap Each `stream.next()` with a Per-Chunk Idle Timeout

In `post_completions_streaming`, replace the current `stream.next().await` call
in the loop body with a `tokio::time::timeout`-wrapped variant:

```rust
use tokio::time::timeout;

let idle_duration = Duration::from_secs(
    self.config
        .read()
        .map(|c| c.stream_idle_timeout_seconds)
        .unwrap_or(30),
);

'stream: loop {
    let chunk_result = match timeout(idle_duration, stream.next()).await {
        Ok(Some(result)) => result,
        Ok(None) => break 'stream,
        Err(_elapsed) => {
            return Err(XzatomaError::Provider(format!(
                "OpenAI SSE stream idle timeout: no data received for {}s",
                idle_duration.as_secs()
            )));
        }
    };
    // existing per-chunk processing ...
}
```

Also switch the `self.client.post(...)` call in `post_completions_streaming` to
`self.streaming_client.post(...)` so the no-total-timeout client is used for the
body.

#### Task 5.4 Apply the Same Pattern in `complete_with_callbacks` (Phase 4)

When Phase 4 Task 4.5 is implemented, the `complete_with_callbacks` override in
`OpenAIProvider` MUST use `self.streaming_client` and MUST wrap every
`stream.next().await` with `tokio::time::timeout(idle_duration, ...)` using the
same `stream_idle_timeout_seconds` value. This is a stated precondition for
Phase 4 acceptance.

#### Task 5.5 Testing Requirements

In `src/config.rs`:

- `test_openai_config_default_stream_idle_timeout_is_30` -- verify
  `OpenAIConfig::default().stream_idle_timeout_seconds == 30`.

In `src/providers/openai.rs`:

- `test_streaming_client_has_no_total_timeout` -- construct an `OpenAIProvider`,
  confirm it builds without error. (Full timeout-behavior testing requires a
  mock server that pauses mid-stream.)
- `test_post_completions_streaming_returns_idle_timeout_error_when_stream_stalls`
  -- mount a mock HTTP server that sends the response status and headers, then
  pauses for longer than `stream_idle_timeout_seconds` without sending any body
  bytes. Verify the provider returns an error whose message contains
  `"idle timeout"`.
- `test_post_completions_streaming_succeeds_with_slow_but_active_stream` --
  mount a mock server that sends chunks at 0.5 s intervals (below the idle
  timeout threshold). Verify the full response is returned correctly.

#### Task 5.6 Deliverables

- `src/config.rs` -- `stream_idle_timeout_seconds` field on `OpenAIConfig`,
  default function, `Default` impl update, env-var override.
- `src/providers/openai.rs` -- `streaming_client` field on `OpenAIProvider`,
  idle-timeout loop in `post_completions_streaming`.

#### Task 5.7 Success Criteria

- `cargo test --all-features` passes with all new tests green.
- A mock server that stalls mid-stream is detected and reported within
  `stream_idle_timeout_seconds` + 1 s, not after 600 s.
- A mock server that sends chunks at irregular intervals (but always within
  `stream_idle_timeout_seconds`) completes without error.
- `OpenAIConfig` serializes and deserializes the new field with the correct
  default.
- The `XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT` env var overrides the field
  correctly.

---

## File Change Summary

| File                                              | Change                                                                                                                                                                                                                |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/acp/session_config.rs`                       | Add `CONFIG_SESSION_MODE`, `build_session_mode_option`, `current_mode_id` on `SessionRuntimeState`, `session_mode_id` on `ConfigChangeEffect`, update `build_session_config_options` and `apply_config_option_change` |
| `src/acp/stdio.rs`                                | Update `create_session`, `set_session_mode`, `set_session_config_option`, `execute_queued_prompt`, and `AcpSessionObserver`                                                                                           |
| `src/agent/events.rs`                             | Add `ReasoningChunkEmitted`, `ThinkingStarted`, `ThinkingFinished` variants                                                                                                                                           |
| `src/providers/trait_mod.rs`                      | Add `complete_with_callbacks` default method                                                                                                                                                                          |
| `src/providers/openai.rs`                         | Add `streaming_client`, idle-timeout loop, `complete_with_callbacks` override with per-chunk callbacks                                                                                                                |
| `src/providers/ollama.rs`                         | Override `complete_with_callbacks` with streaming and `<think>` tag detection                                                                                                                                         |
| `src/agent/core.rs`                               | Use `complete_with_callbacks` when provider supports streaming                                                                                                                                                        |
| `src/config.rs`                                   | Add `stream_idle_timeout_seconds` to `OpenAIConfig`, default function, `Default` impl, env-var override                                                                                                               |
| `docs/how-to/zed_acp_agent_setup.md`              | Add Mode Selector, Context Window, and Thinking Stream sections                                                                                                                                                       |
| `docs/reference/acp_configuration.md`             | Document `session_mode` option, note `terminal_execution` removal, document `stream_idle_timeout_seconds`                                                                                                             |
| `docs/explanation/acp_features_implementation.md` | This plan document                                                                                                                                                                                                    |

## Key Design Decisions

### Why add `session_mode` as a config option instead of fixing `modes`?

The ACP spec states: "If an Agent provides both `configOptions` and `modes` in
the session response, Clients that support config options SHOULD use
`configOptions` exclusively and ignore `modes`." Since xzatoma sends both, Zed
ignores `modes`. Adding `session_mode` as a config option with `category: mode`
makes it appear in the Mode Selector dropdown. The `modes` field is kept for
backward compatibility with clients that do not support config options.

### Why remove `terminal_execution` from config options?

The design intent was "terminal_mode: omitted for Zed sessions -- Zed provides a
Mode Selector UI that controls terminal_mode at runtime." Now that the Mode
Selector config option is properly implemented, the `terminal_execution` option
is redundant. Removing it avoids confusion between two controls that affect the
same underlying setting. Users who need fine-grained terminal execution control
can use the Mode Selector to pick the correct mode.

### Why keep `safety_policy` as a config option without `category: mode`?

Safety policy (always confirm, confirm dangerous, never confirm) is a separate
concern from session mode. It remains as a standalone config option so users can
tune confirmation behavior independently of the mode. For example, a user may
want `write` mode with `always_confirm` for extra caution, which requires the
two settings to remain independent.

### Why a separate `streaming_client` instead of removing `timeout` from the existing client?

Removing `.timeout(...)` from the single shared `client` would also remove the
total-request timeout from non-streaming requests (model listing, token
validation, non-streaming completions). Those paths benefit from a hard budget
because they are expected to return quickly. Keeping a separate
`streaming_client` without a total timeout means each path gets the right
timeout semantics with no interference between them. The additional memory cost
is negligible (two reqwest clients share the same connection pool by default).

### Why 30 s as the default idle timeout?

Thirty seconds balances two concerns. It is long enough to absorb normal jitter
between tokens on a busy GPU inference server (which may batch several tokens
then pause briefly) without falsely timing out healthy streams. It is short
enough to report a stalled connection within half a minute, rather than leaving
the user waiting for the 600 s total-request timeout. Operators running
highly-loaded local servers or throttled remote APIs can increase this value via
`XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT` or the config file field.

### Why use `tokio::time::timeout` per chunk rather than a reqwest read timeout?

Reqwest does not expose a per-read or per-chunk deadline separate from the
total-request timeout. The only way to implement "fail if no bytes arrive for N
seconds" in reqwest is at the Rust level, wrapping each `.next()` poll with
`tokio::time::timeout`. This gives precise control: the deadline resets after
every received byte, not after the initial handshake or after the first SSE
chunk.

### Why use callbacks instead of a streaming return type for `complete_with_callbacks`?

Changing `complete` to return a stream (e.g., `impl Stream<Item = ChunkEvent>`)
would require refactoring every call site and breaking the existing blocking
contract that callers depend on. The callback approach is additive: the default
implementation ignores the callbacks and delegates to `complete`, so all
non-streaming providers continue working without changes. Streaming providers
can override `complete_with_callbacks` without affecting the rest of the
codebase. A full stream-based provider trait can be considered as a separate
future refactor once the callback approach has been validated.

### Why emit `ThinkingStarted` before any chunk arrives?

For slow models, there can be a significant delay between the user sending a
prompt and the first thinking token arriving. Without a `ThinkingStarted`
indicator, the Zed chat window shows nothing during this period. Sending a
placeholder `AgentThoughtChunk` on `ThinkingStarted` opens the thinking panel
immediately, giving users visual confirmation that the model has started
processing.

The `usage` field on `PromptResponse` requires `input_tokens` and
`output_tokens`. Without provider-level per-turn token accounting, exact values
are not available. Using `used_tokens` as a proxy for `total_tokens` provides a
useful approximation and ensures the context window bar renders. Accurate
per-turn token tracking can be added when provider responses surface token
counts (OpenAI already does this; Ollama and Copilot may vary).
