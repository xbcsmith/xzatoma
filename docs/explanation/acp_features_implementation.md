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

5. **Chat Mode Streaming** -- The `chat`, `run`, and `agent` terminal modes call
   `agent.execute()`, which uses a `NoOpObserver` and blocks until the full
   response arrives. Users cannot see response or reasoning tokens appear as the
   model generates them. There is no `--streaming` CLI flag, no
   `/streaming on|off` special command, and no `ChatStreamingObserver` that
   routes `ReasoningChunkEmitted`, `ThinkingStarted`, and `AssistantTextEmitted`
   events to the terminal.

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

#### Issue 7: Chat mode has no streaming observer

`run_chat` in `src/commands/mod.rs` calls `agent.execute(prompt).await` which
instantiates a `NoOpObserver` internally. Even after Phase 4 wires
`complete_with_callbacks` into the agent loop and emits `ReasoningChunkEmitted`,
`ThinkingStarted`, `ThinkingFinished`, and `AssistantTextEmitted` events, a
`NoOpObserver` silently discards all of them. The full response is only printed
after `execute` returns.

Similarly, `run_plan_with_options` in `pub mod run` and the `handle_agent` entry
point in `src/commands/agent.rs` both call `agent.execute()` with no observer,
so neither can stream live tokens.

The missing parts are:

1. A `--streaming` flag on the `Chat`, `Agent`, and `Run` CLI sub-commands.
2. A `ToggleStreaming(bool)` variant in `SpecialCommand` with `/streaming on`
   and `/streaming off` parser support in `src/commands/special_commands.rs`.
3. A `streaming_enabled: bool` field on `ChatModeState` that the toggle command
   and the `--streaming` flag both write.
4. A `ChatStreamingObserver` struct implementing `AgentObserver` that prints
   `ThinkingStarted`, per-chunk reasoning text, `ThinkingFinished`, and
   per-chunk response text to stdout in real time.
5. Call sites that replace `agent.execute()` with
   `agent.execute_with_observer()` when `streaming_enabled` is true, and
   suppress the redundant post-call `println!` of the full response.

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

### Phase 6: Streaming Token Display in Chat Mode

> **Prerequisite**: Phase 4 must be complete. Phase 6 depends on
> `ReasoningChunkEmitted`, `ThinkingStarted`, `ThinkingFinished`, and
> `AssistantTextEmitted` being emitted by the agent loop, and on
> `complete_with_callbacks` being implemented in the OpenAI and Ollama
> providers.

#### Task 6.1 Add `--streaming` CLI Flag to `Chat`, `Agent`, and `Run`

In `src/cli.rs`, add the following field to `Commands::Chat`, `Commands::Agent`,
and `Commands::Run`:

```rust
/// Stream model output tokens to the terminal as they are generated.
///
/// When set, reasoning tokens (thinking) are printed with a visual
/// indicator before the response tokens. Requires the configured
/// provider to support streaming.
#[arg(long)]
streaming: bool,
```

In `main.rs`, extract the `streaming` field from each matched arm and forward it
to the corresponding command runner:

- `run_chat(config, ..., streaming, ...)` -- new final parameter.
- `run_plan_with_options(config, plan, prompt, cli_system_prompt, streaming)` --
  new final parameter.
- `handle_agent(plan, system_prompt, streaming)` -- new final parameter.

#### Task 6.2 Add `ToggleStreaming` to `SpecialCommand`

In `src/commands/special_commands.rs`, add the variant:

```rust
/// Toggle live streaming of model output tokens.
///
/// When enabled, response and reasoning tokens are printed to the
/// terminal as they arrive. When disabled, the full response is
/// printed only after the model finishes.
ToggleStreaming(bool),
```

In `parse_special_command`, add two cases:

```rust
"/streaming on" | "/streaming enable" => Ok(SpecialCommand::ToggleStreaming(true)),
"/streaming off" | "/streaming disable" => Ok(SpecialCommand::ToggleStreaming(false)),
```

If the input is `/streaming` with no argument, or with an unrecognised argument,
return:

```rust
Err(CommandError::MissingArgument {
    command: "/streaming".to_string(),
    usage: "/streaming <on|off>".to_string(),
})
```

Update `print_help` to document the new command alongside the existing special
commands.

#### Task 6.3 Add `streaming_enabled` to `ChatModeState`

In `src/commands/mod.rs` `pub mod chat`, add a field to `ChatModeState`:

```rust
/// Whether live token streaming is enabled for this session.
pub streaming_enabled: bool,
```

Update `ChatModeState::new` to accept a `streaming: bool` parameter and
initialise the field from it. The initial value comes from the `--streaming` CLI
flag forwarded through `run_chat`.

Add a method:

```rust
/// Enable or disable live token streaming and return the previous state.
pub fn set_streaming(&mut self, enable: bool) -> bool {
    let prev = self.streaming_enabled;
    self.streaming_enabled = enable;
    prev
}
```

#### Task 6.4 Implement `ChatStreamingObserver`

In `src/commands/mod.rs` `pub mod chat`, define:

```rust
/// Observer that writes streaming model output to stdout in real time.
///
/// Used by `run_chat`, `run_plan_with_options`, and `handle_agent` when
/// `streaming_enabled` is true. Receives `AgentExecutionEvent` emissions
/// from the agent loop and immediately flushes each chunk to stdout.
///
/// After execution, callers MUST check `streamed_any_content()` and skip
/// their normal `println!` of the full response to avoid printing it twice.
pub struct ChatStreamingObserver {
    thinking_active: bool,
    content_started: bool,
    streamed_any_content: bool,
}

impl ChatStreamingObserver {
    pub fn new() -> Self {
        Self {
            thinking_active: false,
            content_started: false,
            streamed_any_content: false,
        }
    }

    /// Returns true if at least one content or reasoning chunk was printed.
    pub fn streamed_any_content(&self) -> bool {
        self.streamed_any_content
    }
}
```

Implement `AgentObserver for ChatStreamingObserver`. In `on_event`, handle:

- `ThinkingStarted` -- if not already in thinking mode, print
  `"\nThinking...\n"` and set `thinking_active = true`. Flush stdout.
- `ReasoningChunkEmitted { text }` -- print `text` directly to stdout with
  `print!("{}", text)` and `use std::io::Write; stdout().flush().ok()`. Set
  `streamed_any_content = true`.
- `ThinkingFinished` -- if in thinking mode, print `"\n"` and set
  `thinking_active = false`. Flush stdout.
- `AssistantTextEmitted { text }` -- if this is the first content chunk and
  `thinking_active` was previously true, print `"\n"` (separator after thinking
  block). Print `text` with `print!` and flush. Set `content_started = true` and
  `streamed_any_content = true`.
- All other events -- do nothing (silently accepted).

Note: `ThinkingFinished` is not emitted by all providers. The observer must
handle the case where `AssistantTextEmitted` arrives while `thinking_active` is
still true (Ollama tag-based detection may not emit `ThinkingFinished`
reliably). When that happens, close the thinking block as if `ThinkingFinished`
had fired before printing the content chunk.

#### Task 6.5 Wire `ChatStreamingObserver` into `run_chat`

In `run_chat` in `src/commands/mod.rs`:

1. Accept `streaming: bool` as a new final parameter. Pass the value to
   `ChatModeState::new`.

2. In the main prompt-execution block, replace the existing:

   ```rust
   match agent.execute(augmented_prompt).await {
       Ok(response) => {
           println!("\n{}\n", response);
   ```

   with:

   ```rust
   let cancellation_token = tokio_util::sync::CancellationToken::new();
   let mut observer = ChatStreamingObserver::new();
   let exec_result = if mode_state.streaming_enabled {
       agent
           .execute_with_observer(
               augmented_prompt,
               &cancellation_token,
               &mut observer,
           )
           .await
   } else {
       agent.execute(augmented_prompt).await
   };
   match exec_result {
       Ok(response) => {
           if !observer.streamed_any_content() {
               println!("\n{}\n", response);
           } else {
               // Chunks already printed; only add a trailing newline if
               // the last chunk did not end with one.
               println!();
           }
   ```

3. In the `run_chat` main loop, match the new `ToggleStreaming(enable)` arm:

   ```rust
   Ok(SpecialCommand::ToggleStreaming(enable)) => {
       let prev = mode_state.set_streaming(enable);
       if prev != enable {
           println!(
               "Streaming {}.",
               if enable { "enabled" } else { "disabled" }
           );
       }
       continue;
   }
   ```

#### Task 6.6 Wire Streaming Flag into `run_plan_with_options` and `handle_agent`

In `pub mod run` in `src/commands/mod.rs`:

- Accept `streaming: bool` as a final parameter to `run_plan_with_options`.
- Replace `agent.execute(task).await` with the same `if streaming` branch used
  in Task 6.5. When streaming, use `execute_with_observer` with a
  `ChatStreamingObserver`; suppress the full response print if
  `streamed_any_content()` is true.

In `src/commands/agent.rs`:

- Accept `streaming: bool` as a final parameter to `handle_agent`.
- Replace the internal `agent.execute()` call with the same conditional pattern.

#### Task 6.7 Testing Requirements

In `src/commands/special_commands.rs`:

- `test_parse_streaming_on_returns_toggle_streaming_true` -- verify
  `/streaming on` parses to `ToggleStreaming(true)`.
- `test_parse_streaming_off_returns_toggle_streaming_false` -- verify
  `/streaming off` parses to `ToggleStreaming(false)`.
- `test_parse_streaming_enable_alias` -- verify `/streaming enable` parses to
  `ToggleStreaming(true)`.
- `test_parse_streaming_disable_alias` -- verify `/streaming disable` parses to
  `ToggleStreaming(false)`.
- `test_parse_streaming_no_arg_returns_missing_argument_error` -- verify
  `/streaming` with no argument returns `Err(CommandError::MissingArgument)`.
- `test_parse_streaming_invalid_arg_returns_missing_argument_error` -- verify
  `/streaming maybe` returns `Err(CommandError::MissingArgument)`.

In `src/commands/mod.rs` (chat module tests):

- `test_chat_streaming_observer_prints_thinking_start_on_thinking_started` --
  construct a `ChatStreamingObserver`, call `on_event(ThinkingStarted)`, verify
  `thinking_active` is set.
- `test_chat_streaming_observer_streamed_any_content_true_after_reasoning_chunk`
  -- send a `ReasoningChunkEmitted` event, verify `streamed_any_content()` is
  `true`.
- `test_chat_streaming_observer_streamed_any_content_true_after_content_chunk`
  -- send an `AssistantTextEmitted` event, verify `streamed_any_content()` is
  `true`.
- `test_chat_streaming_observer_streamed_any_content_false_initially` -- new
  observer returns `false` before any events.
- `test_chat_mode_state_set_streaming_returns_previous_value` -- verify
  `set_streaming(true)` on a `streaming_enabled = false` state returns `false`.

In `src/cli.rs`:

- `test_cli_parse_chat_with_streaming_flag` -- verify `chat --streaming` sets
  `streaming = true`.
- `test_cli_parse_chat_streaming_defaults_false` -- verify `chat` without the
  flag sets `streaming = false`.
- `test_cli_parse_agent_with_streaming_flag` -- verify `agent --streaming` sets
  `streaming = true`.
- `test_cli_parse_run_with_streaming_flag` -- verify
  `run --prompt hello --streaming` sets `streaming = true`.

#### Task 6.8 Deliverables

- [ ] `src/cli.rs` -- `streaming: bool` field on `Commands::Chat`,
      `Commands::Agent`, `Commands::Run`.
- [ ] `src/commands/special_commands.rs` -- `ToggleStreaming(bool)` variant,
      `/streaming on|off|enable|disable` parser, `print_help` update.
- [ ] `src/commands/mod.rs` -- `streaming_enabled` field and `set_streaming`
      method on `ChatModeState`; `ChatStreamingObserver` struct with
      `AgentObserver` impl; updated `run_chat` loop with streaming branch;
      updated `run_plan_with_options` with streaming branch.
- [ ] `src/commands/agent.rs` -- updated `handle_agent` with streaming branch.
- [ ] `main.rs` -- `streaming` forwarded from each CLI arm to its runner.
- [ ] All tests listed in Task 6.7 pass.
- [ ] `cargo fmt`, `cargo check`, `cargo clippy -- -D warnings`, and
      `cargo test --all-features` pass with no new failures.

#### Task 6.9 Success Criteria

- `xzatoma chat --streaming` causes response tokens to appear progressively in
  the terminal as the model generates them.
- `xzatoma chat --streaming` with a thinking-capable model shows a `Thinking...`
  header followed by live reasoning tokens before the response tokens begin.
- `/streaming on` mid-session enables streaming for subsequent prompts;
  `/streaming off` reverts to the post-complete print.
- `xzatoma run --streaming --prompt "..."` streams tokens during plan execution.
- `xzatoma agent --streaming` streams tokens during autonomous execution.
- Providers that do not support streaming (`supports_streaming() == false`)
  still print the full response correctly after the call completes, without
  regression.
- `cargo test --all-features` passes with all new tests green.

---

### Phase 7: Wire-Format Diagnostic Logging

> **Prerequisite**: None. This phase is a standalone observability improvement
> that can be applied at any time.

#### Task 7.1 Add TRACE-Level Wire Logging for `NewSessionResponse`

In `src/acp/stdio.rs`, inside `create_session`, immediately before returning
`Ok(response)`, serialize the `NewSessionResponse` to JSON and emit it at
`TRACE` level:

```rust
if tracing::enabled!(tracing::Level::TRACE) {
    match serde_json::to_string(&response) {
        Ok(json) => tracing::trace!(
            session_id = %session_id,
            response_json = %json,
            "ACP stdio: NewSessionResponse wire format"
        ),
        Err(e) => tracing::trace!(
            session_id = %session_id,
            error = %e,
            "ACP stdio: NewSessionResponse serialization failed"
        ),
    }
}
```

This enables operators to run
`RUST_LOG=xzatoma::acp=trace xzatoma agent 2>trace.log` and confirm the exact
JSON payload Zed receives for `configOptions`, `modes`, `models`, and any other
fields.

#### Task 7.2 Add TRACE-Level Wire Logging for `LoadSessionResponse`

Apply the same pattern in the `LoadSessionRequest` handler in
`run_stdio_agent_with_transport`. After `create_session` returns the session
response and before `responder.respond(response)`, emit the same TRACE log using
the existing pattern:

```rust
if tracing::enabled!(tracing::Level::TRACE) {
    if let Ok(json) = serde_json::to_string(&response) {
        tracing::trace!(
            session_id = %response.session_id,
            response_json = %json,
            "ACP stdio: LoadSessionResponse wire format"
        );
    }
}
```

> Note: xzatoma currently handles session resume inside `create_session`
> (controlled by `AcpStdioConfig::resume_by_workspace`). There is no separate
> `LoadSessionRequest` handler at the protocol level; the single
> `NewSessionRequest` handler covers both fresh and resumed sessions. The Task
> 7.1 log therefore covers both code paths. If a dedicated `LoadSessionRequest`
> handler is added in a future phase, Task 7.2 applies there.

#### Task 7.3 Testing Requirements

In `src/acp/stdio.rs`:

- `test_new_session_response_is_serializable_to_json` -- construct a
  `NewSessionResponse` with modes and config options populated, serialize it to
  JSON with `serde_json::to_string`, verify the output contains the `session_id`
  field and a non-empty `configOptions` array. (The actual TRACE log is not
  testable in unit tests because tracing output is not captured in the normal
  test harness; this test validates the serialization contract instead.)
- `test_new_session_response_json_contains_config_options_key` -- verify the
  JSON string from `serde_json::to_string` contains the string `"configOptions"`
  when the response carries config options.

#### Task 7.4 Deliverables

- `src/acp/stdio.rs` -- TRACE log in `create_session` before returning the
  `NewSessionResponse`.

#### Task 7.5 Success Criteria

- Running `RUST_LOG=xzatoma::acp=trace xzatoma agent 2>trace.log` and opening a
  new Zed session produces a log line containing
  `NewSessionResponse wire format` with a `response_json` field.
- The JSON in the log contains `session_mode` in `configOptions` and the four
  modes in `modes`.
- `cargo test --all-features` passes with all new tests green.

---

### Phase 8: Plan Tracking

> **Prerequisite**: Phase 4 must be complete. Plan tracking hooks into
> `AssistantTextEmitted` and `ToolCallStarted` events which are emitted by the
> Phase 4 streaming path. The `acp::Plan`, `acp::PlanEntry`, and
> `acp::PlanEntryStatus` types are available in `agent-client-protocol` version
> 0.11.1 without any feature flag.

Zed's `SessionUpdate::Plan` variant renders a task-checklist panel in the thread
view. Each `PlanEntry` has a `status` (`Pending`, `InProgress`, or `Completed`)
and a `content` string. When the agent begins a multi-step task, users see a
live list of steps that update as work progresses.

#### Task 8.1 Add `PlanTracker` to `src/agent/plan_tracker.rs`

Create a new file `src/agent/plan_tracker.rs` with a `PlanTracker` struct that
accumulates streamed assistant text and extracts numbered-list items as plan
entries.

```rust
/// Parses streamed assistant output for numbered-list plan items and tracks
/// their execution status.
///
/// Only items in the initial assistant response (before the first tool call)
/// are tracked. After a tool call boundary, the tracker enters a
/// `post_tool_call` state and ignores subsequent text.
///
/// Status transitions:
/// - `Pending` when an item is first detected.
/// - `InProgress` when streaming moves past an entry to a later one.
/// - `Completed` after `finalize()` is called at the end of the turn.
pub struct PlanTracker {
    entries: Vec<PlanEntry>,
    buffer: String,
    post_tool_call: bool,
}
```

Public interface:

```rust
impl PlanTracker {
    /// Create a new tracker with no entries.
    pub fn new() -> Self;

    /// Feed a chunk of streamed assistant text.
    ///
    /// Returns `true` if the plan changed (new entries were added or an
    /// entry transitioned from `Pending` to `InProgress`), signalling the
    /// caller to emit a `SessionUpdate::Plan`.
    pub fn update(&mut self, chunk: &str) -> bool;

    /// Notify the tracker that a tool call has started.
    ///
    /// After this call, `update` silently ignores all text, preventing
    /// tool-output numbered lists from being misidentified as plan items.
    pub fn on_tool_call_started(&mut self);

    /// Finalize the plan at the end of a turn.
    ///
    /// Promotes any remaining `InProgress` entries to `Completed`.
    /// Returns `true` if at least one entry exists (so the caller can emit
    /// a final `SessionUpdate::Plan`).
    pub fn finalize(&mut self) -> bool;

    /// Return a snapshot of the current plan entries.
    pub fn entries(&self) -> &[PlanEntry];

    /// Return `true` if any entries have been detected.
    pub fn has_entries(&self) -> bool;

    /// Reset all entries to `Pending` and clear the buffer.
    ///
    /// Called at the start of a new turn to reuse the tracker.
    pub fn reset(&mut self);
}
```

Parsing rules:

- Detect lines matching the pattern `^\d+\.\s+.+` (one or more digits, period,
  whitespace, non-empty text).
- Each distinct detected line creates one `PlanEntry` with
  `PlanEntryPriority::Medium` and `PlanEntryStatus::Pending`.
- When streaming advances past entry `N` to a detected entry `N+1`, entry `N` is
  promoted to `InProgress`.
- Duplicates (same content) are ignored.
- After `on_tool_call_started()`, `update()` is a no-op.

`PlanEntry` is constructed using
`acp::PlanEntry::new(content, acp::PlanEntryPriority::Medium, acp::PlanEntryStatus::Pending)`.
Import types via `use agent_client_protocol::schema as acp`.

Register the new module in `src/agent/mod.rs` with `pub mod plan_tracker;`.

#### Task 8.2 Add `plan_tracker` Field to `AcpSessionObserver`

In `src/acp/stdio.rs`, add a `plan_tracker: PlanTracker` field to
`AcpSessionObserver`:

```rust
struct AcpSessionObserver {
    session_id: acp::SessionId,
    connection: ConnectionTo<AcpClientRole>,
    text_emitted: bool,
    plan_tracker: crate::agent::plan_tracker::PlanTracker,
}
```

Update `AcpSessionObserver::new` to initialize the field:

```rust
plan_tracker: crate::agent::plan_tracker::PlanTracker::new(),
```

#### Task 8.3 Wire `PlanTracker` into `AgentObserver for AcpSessionObserver`

In the `on_event` implementation, wire plan tracking into two existing match
arms:

**`AssistantTextEmitted { text }` arm** -- after emitting the
`AgentMessageChunk`, call the tracker and emit a `Plan` update if the plan
changes:

```rust
AgentExecutionEvent::AssistantTextEmitted { text } => {
    self.text_emitted = true;
    let chunk = acp::ContentChunk::new(acp::ContentBlock::from(text.clone()));
    self.send_update(acp::SessionUpdate::AgentMessageChunk(chunk));

    if self.plan_tracker.update(&text) {
        let plan = acp::Plan::new(self.plan_tracker.entries().to_vec());
        self.send_update(acp::SessionUpdate::Plan(plan));
    }
}
```

**`ToolCallStarted { .. }` arm** -- notify the tracker before the existing
tool-call notification logic:

```rust
AgentExecutionEvent::ToolCallStarted { id, name, arguments } => {
    self.plan_tracker.on_tool_call_started();
    // ... existing tool-call start notification code ...
}
```

**`ExecutionCompleted { response }` arm** -- finalize the plan before the
existing text-emit guard:

```rust
AgentExecutionEvent::ExecutionCompleted { response } => {
    if self.plan_tracker.finalize() {
        let plan = acp::Plan::new(self.plan_tracker.entries().to_vec());
        self.send_update(acp::SessionUpdate::Plan(plan));
    }
    // ... existing text-emit guard ...
}
```

#### Task 8.4 Add System-Prompt Anchor Text for ACP Sessions

In `src/acp/stdio.rs` inside `create_session`, after the existing system-prompt
injection block and before the skill-disclosure injection, append a
planning-instruction paragraph to the agent's conversation when the session is
ACP-mode (i.e., always, since this is `stdio.rs`):

Add a constant:

```rust
/// System-prompt fragment that encourages numbered-list plan output.
///
/// Prepended to every ACP session so that Zed's plan-checklist panel
/// shows live step progress for multi-step tasks.
const ACP_PLAN_INSTRUCTION: &str = \
    "When you have a multi-step task, begin your response with a \
     numbered list of the steps you will take (e.g., \"1. Read the \
     file\n2. Edit the function\n3. Run tests\"). \
     Proceed with execution immediately after the list.";
```

Inject it as a system message only when no existing system message already
contains the instruction text:

```rust
if !agent
    .conversation()
    .messages()
    .iter()
    .any(|m| m.role == "system" && m.content.as_deref().unwrap_or("").contains(ACP_PLAN_INSTRUCTION))
{
    agent.conversation_mut().add_system_message(ACP_PLAN_INSTRUCTION.to_string());
}
```

> **Architecture note**: The deduplication guard prevents double-injection when
> `create_session` is called for a workspace-resumed session that already has
> the instruction in its stored conversation history.

#### Task 8.5 Testing Requirements

In `src/agent/plan_tracker.rs`:

- `test_plan_tracker_detects_numbered_list_items` -- feed a chunk containing
  `"1. Read the file\n2. Edit the code\n"`, verify `entries()` has two items
  with `Pending` status and `update` returned `true`.
- `test_plan_tracker_returns_false_for_plain_text` -- feed a chunk with no
  numbered list; verify `update` returns `false` and `entries()` is empty.
- `test_plan_tracker_ignores_text_after_tool_call` -- call
  `on_tool_call_started()`, then `update("1. Should be ignored")`, verify
  `entries()` is still empty.
- `test_plan_tracker_promotes_entries_to_in_progress` -- feed a two-item list
  one item at a time; after the second item is detected, verify the first entry
  has `InProgress` status.
- `test_plan_tracker_finalize_promotes_all_to_completed` -- feed items, call
  `finalize()`, verify all entries have `Completed` status and `finalize`
  returns `true`.
- `test_plan_tracker_finalize_returns_false_when_no_entries` -- verify
  `finalize()` returns `false` on an empty tracker.
- `test_plan_tracker_reset_clears_entries_and_buffer` -- feed items, call
  `reset()`, verify `entries()` is empty and `has_entries()` returns `false`.
- `test_plan_tracker_ignores_duplicate_items` -- feed the same numbered-list
  line twice; verify only one entry exists.

In `src/acp/stdio.rs`:

- `test_acp_observer_emits_plan_update_on_numbered_list_text` -- call
  `on_event(AssistantTextEmitted { text: "1. Do thing one\n" })` on an
  `AcpSessionObserver`; verify `plan_tracker.has_entries()` is `true`.
- `test_acp_observer_plan_tracker_stops_on_tool_call_started` -- call
  `on_event(ToolCallStarted { .. })` after injecting text; verify subsequent
  `AssistantTextEmitted` events with numbered lists do not add entries.
- `test_acp_observer_finalize_on_execution_completed` -- send a numbered-list
  text event followed by `ExecutionCompleted`; verify all entries have
  `Completed` status via `plan_tracker.entries()`.
- `test_acp_plan_instruction_constant_is_not_empty` -- assert
  `!ACP_PLAN_INSTRUCTION.is_empty()`.

#### Task 8.6 Deliverables

- `src/agent/plan_tracker.rs` -- `PlanTracker` struct, all methods, and full
  unit-test suite.
- `src/agent/mod.rs` -- `pub mod plan_tracker;` registration.
- `src/acp/stdio.rs` -- `plan_tracker` field on `AcpSessionObserver`; wired into
  `AssistantTextEmitted`, `ToolCallStarted`, and `ExecutionCompleted` arms;
  `ACP_PLAN_INSTRUCTION` constant and system-prompt injection in
  `create_session`.
- `docs/explanation/phase8_plan_tracking_implementation.md` -- implementation
  summary.

#### Task 8.7 Success Criteria

- When xzatoma executes a multi-step task where the model emits a leading
  numbered list, Zed's thread panel displays a live checklist that updates as
  each step begins and completes.
- `cargo test --all-features` passes with all new tests green.
- `cargo check --all-targets --all-features` passes with zero errors.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with zero
  warnings.

---

### Phase 9: Cross-Reference Bug Fixes from Atoma Agent

> **Source**: Bugs fixed in the Atoma agent (`atoma_implementation_plan_v2.md`)
> were cross-referenced against XZatoma to identify the same or similar issues.

#### Issue Inventory

The following bugs were found in Atoma and investigated in XZatoma:

| Bug                                                                            | Atoma Status | XZatoma Status                                                                                         |
| ------------------------------------------------------------------------------ | ------------ | ------------------------------------------------------------------------------------------------------ |
| Initial `UsageUpdate` size uses config default instead of model context window | Fixed        | Fixed in Phase 9                                                                                       |
| `ToolCallCompleted` emits empty output for non-zero exit tools                 | Fixed        | Fixed in Phase 9                                                                                       |
| OpenAI provider reports `0` context window for all models                      | Fixed        | Fixed in Phase 9                                                                                       |
| `unstable_session_usage` feature not in `default = [...]`                      | Fixed        | Not applicable: XZatoma enables the feature directly on the dependency, not as a re-exportable feature |
| `NewSessionRequest` resumes prior conversations instead of always being fresh  | Fixed        | Design difference: XZatoma intentionally resumes by workspace (no `LoadSession` mechanism)             |
| Ollama context-window extraction misses versioned architecture names           | Fixed        | Not applicable: XZatoma already uses a three-tier approach                                             |
| UTF-8 panic in streamed thinking parser                                        | Fixed        | Not applicable: XZatoma uses `String::from_utf8_lossy` and no raw byte-offset slicing                  |
| ACP diff viewer support for file-writing tools                                 | Added        | Not yet implemented (future phase)                                                                     |
| Agent identity missing from ACP system prompt                                  | Fixed        | Not applicable until Phase 8 (Plan Tracking) is implemented                                            |

---

#### Task 9.1 Initial `UsageUpdate` Size Uses Model Context Window

**Problem**: In `create_session`, the initial `UsageUpdate` used
`agent.conversation().max_tokens()` (a config value, default 100,000) as the
`size` field. For providers that report the actual model context window (Ollama
via `/api/show`, llama.cpp via `meta.n_ctx`), the Zed context bar denominator
would show the config default instead of the true capacity.

**Fix**: In `src/acp/stdio.rs`, a new helper function
`model_context_window_from_state` extracts the `contextWindow` key from the
`meta` map of the matching model in the already-fetched `SessionModelState`. The
initial `UsageUpdate` uses this value, falling back to
`agent.conversation().max_tokens()` when:

- The model listing failed (fallback model from
  `acp_model_info_from_current_model` does not include `contextWindow` in meta).
- The provider reports `0` as the context window.
- The current model is not found in `available_models`.

This reuses the already-fetched model listing with no additional network
requests. The debug log now also includes `config_max_tokens` to make it clear
which source was used.

**Files changed**: `src/acp/stdio.rs`

**Tests added** (5):

- `test_model_context_window_from_state_returns_value_from_meta`
- `test_model_context_window_from_state_falls_back_when_no_meta`
- `test_model_context_window_from_state_falls_back_when_context_window_zero`
- `test_model_context_window_from_state_falls_back_for_unknown_model`
- `test_model_context_window_from_state_ignores_non_current_models`

---

#### Task 9.2 `ToolCallCompleted` Emits Full Output for Non-Zero Exit Tools

**Problem**: In `src/agent/core.rs`, both `execute_with_observer` and
`execute_provider_messages_with_observer` emitted the `ToolCallCompleted` agent
event with `output: tool_result.output.clone()`. For terminal commands that exit
non-zero, the terminal tool stores the combined captured output in the `error`
field and leaves `output` as an empty string. The ACP observer forwards `output`
to Zed's tool call card, so Zed displayed an empty body for every failed command
— hiding all diagnostic output from the user.

The model's conversation was unaffected because `add_tool_result` already used
`tool_result.to_message()` (which includes the output inside the error string),
but the Zed UI card showed nothing.

**Fix**: Change `output: tool_result.output.clone()` to
`output: tool_result.to_message()` in both execution paths. For the success
case, `to_message()` returns the same value as `output` (with an optional
truncation note), so existing behavior is preserved. For the failure case,
`to_message()` returns `"Error: Exit code N: <full captured output>"`, which Zed
now displays in the tool card.

**Files changed**: `src/agent/core.rs`

**Tests added** (2):

- `test_tool_call_completed_output_uses_to_message_for_success`
- `test_tool_call_completed_output_uses_to_message_for_failure`

---

#### Task 9.3 OpenAI Provider Reports Actual Context Windows

**Problem**: `src/providers/openai.rs` always passed `0` as the context window
when constructing `ModelInfo` in both `list_models` and `get_model_info`. A zero
context window causes `model_context_window_from_state` to fall through to the
config fallback. It also means the model listing in Zed shows `0` for all OpenAI
models.

**Fix**: Add a `pub fn context_window_for_model_id(id: &str) -> usize` function
that pattern-matches on model ID prefix and returns the known context window:

| Pattern             | Context window |
| ------------------- | -------------- |
| `gpt-4.1*`          | 1,047,576      |
| `o1*`, `o3*`, `o4*` | 200,000        |
| `gpt-4-32k*`        | 32,768         |
| `gpt-3.5*`          | 16,385         |
| all others          | 128,000        |

All three call sites that previously used `0` now call
`context_window_for_model_id(&entry.id)`. The function is `pub` so it can be
used by the initial `UsageUpdate` fallback chain via the model listing.

**Files changed**: `src/providers/openai.rs`

**Tests added** (5):

- `test_context_window_for_model_id_gpt4_1_returns_million`
- `test_context_window_for_model_id_o_series_returns_200k`
- `test_context_window_for_model_id_gpt35_returns_16k`
- `test_context_window_for_model_id_gpt4_returns_128k`
- `test_context_window_for_model_id_unknown_returns_128k`

---

#### Task 9.4 Bugs Not Present in XZatoma

**`unstable_session_usage` feature gating**: In Atoma, this feature was defined
in `[features]` but not included in `default = [...]`, so all
`#[cfg(feature = "unstable_session_usage")]` blocks were dead code in normal
builds. XZatoma enables the feature directly on the `agent-client-protocol`
dependency line (`features = ["unstable_session_usage"]`), so it is always
enabled unconditionally. No fix needed.

**Ollama context-window extraction**: Atoma's `extract_context_window` function
used a hardcoded architecture prefix list that missed versioned names (e.g.
`gemma4.context_length`). XZatoma already uses a three-tier chain: dynamic key
construction from `general.architecture`, bare `context_length` fallback, then a
scan of all keys ending in `context_length`. No fix needed.

**UTF-8 panic in streamed thinking parser**: Atoma had a `StreamThinkingParser`
that sliced `String` values at raw byte offsets, causing panics on multibyte
characters. XZatoma's OpenAI streaming path uses `String::from_utf8_lossy` for
HTTP chunk conversion and only appends to strings via `push_str` — no raw
byte-offset slicing is performed. No fix needed.

**`NewSessionRequest` resuming prior conversations**: In Atoma this was a bug
because a separate `LoadSessionRequest` handler exists for explicit session
resumption. XZatoma's `create_session` intentionally resumes prior conversations
(controlled by `persist_sessions` and `resume_by_workspace` config options, both
defaulting to `true`) because XZatoma does not advertise the `load_session`
capability (`load_session(false)` in `handle_initialize`). This is a deliberate
design difference, not a bug.

---

#### Task 9.5 Testing Requirements

See tasks 9.1–9.3 for per-fix test lists. All 12 new tests are in
`src/acp/stdio.rs`, `src/agent/core.rs`, and `src/providers/openai.rs`.

#### Task 9.6 Deliverables

- `src/acp/stdio.rs` - `model_context_window_from_state` helper; updated initial
  `UsageUpdate` in `create_session`; 5 new tests.
- `src/agent/core.rs` - `ToolCallCompleted` uses `to_message()` in both
  execution paths; 2 new tests.
- `src/providers/openai.rs` - `context_window_for_model_id` function; updated 3
  call sites in `list_models` and `get_model_info`; 5 new tests.
- `docs/explanation/acp_features_implementation.md` - This phase added.

#### Task 9.7 Success Criteria

- Zed's context bar denominator reflects the model's actual context window for
  Ollama and llama.cpp providers (not the config default).
- Zed's tool call card shows full captured output (including diagnostics) for
  commands that exit non-zero.
- OpenAI model listing shows accurate context windows in Zed's model selector.
- `cargo test --all-features --lib` passes with all new tests green.

---

## File Change Summary

| File                                              | Change                                                                                                                                                                                                                             |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/acp/session_config.rs`                       | Add `CONFIG_SESSION_MODE`, `build_session_mode_option`, `current_mode_id` on `SessionRuntimeState`, `session_mode_id` on `ConfigChangeEffect`, update `build_session_config_options` and `apply_config_option_change`              |
| `src/acp/stdio.rs`                                | Update `create_session`, `set_session_mode`, `set_session_config_option`, `execute_queued_prompt`, and `AcpSessionObserver`; Phase 9: `model_context_window_from_state` helper, use model context window for initial `UsageUpdate` |
| `src/agent/events.rs`                             | Add `ReasoningChunkEmitted`, `ThinkingStarted`, `ThinkingFinished` variants                                                                                                                                                        |
| `src/providers/trait_mod.rs`                      | Add `complete_with_callbacks` default method                                                                                                                                                                                       |
| `src/providers/openai.rs`                         | Add `streaming_client`, idle-timeout loop, `complete_with_callbacks` override with per-chunk callbacks; Phase 9: `context_window_for_model_id` lookup table, update 3 `ModelInfo` call sites                                       |
| `src/providers/ollama.rs`                         | Override `complete_with_callbacks` with streaming and `<think>` tag detection                                                                                                                                                      |
| `src/agent/core.rs`                               | Use `complete_with_callbacks` when provider supports streaming; Phase 9: `ToolCallCompleted` uses `to_message()` in both execution paths                                                                                           |
| `src/config.rs`                                   | Add `stream_idle_timeout_seconds` to `OpenAIConfig`, default function, `Default` impl, env-var override                                                                                                                            |
| `docs/how-to/zed_acp_agent_setup.md`              | Add Mode Selector, Context Window, and Thinking Stream sections                                                                                                                                                                    |
| `docs/reference/acp_configuration.md`             | Document `session_mode` option, note `terminal_execution` removal, document `stream_idle_timeout_seconds`                                                                                                                          |
| `src/cli.rs`                                      | Add `streaming: bool` to `Commands::Chat`, `Commands::Agent`, `Commands::Run`                                                                                                                                                      |
| `src/commands/special_commands.rs`                | Add `ToggleStreaming(bool)` variant, `/streaming on\|off\|enable\|disable` parser, `print_help` entry                                                                                                                              |
| `src/commands/mod.rs`                             | Add `streaming_enabled` and `set_streaming` to `ChatModeState`; add `ChatStreamingObserver`; update `run_chat` and `run_plan_with_options` streaming branches                                                                      |
| `src/commands/agent.rs`                           | Accept and forward `streaming` flag in `handle_agent`                                                                                                                                                                              |
| `src/main.rs`                                     | Extract and forward `streaming` from `Chat`, `Agent`, and `Run` CLI arms                                                                                                                                                           |
| `src/agent/plan_tracker.rs`                       | New file: `PlanTracker` struct with numbered-list parser, status-transition logic, and unit tests                                                                                                                                  |
| `src/agent/mod.rs`                                | Register `pub mod plan_tracker`                                                                                                                                                                                                    |
| `docs/explanation/acp_features_implementation.md` | This plan document                                                                                                                                                                                                                 |

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

### Why use `bool` for `--streaming` instead of an enum?

The flag controls a binary choice: either tokens stream to the terminal as they
arrive, or the full response is printed after the call completes. A `bool` is
the simplest correct type. An enum would add no semantic value at this stage. If
a third mode is needed in future, the flag can be replaced at that time.

### Why default `--streaming` to `false`?

Many terminal environments render partial lines poorly, and some CI or piped
outputs break on carriage returns embedded in streaming output. Defaulting to
`false` keeps the existing batch-print behaviour as the safe default. Users who
want live tokens opt in explicitly. In interactive sessions, `/streaming on` can
be used without restarting the session.

### Why add `ChatStreamingObserver` in `src/commands/mod.rs` instead of a new file?

The observer is tightly coupled to the chat rendering concerns already in
`src/commands/mod.rs`. It uses no public API that would make it useful to other
callers. Keeping it in the same file avoids a new module boundary for a small
struct that is a rendering detail. If a second caller outside `commands/` needed
it, moving it to `src/agent/` would be appropriate.

### Why is the special command named `/streaming` instead of `/stream`?

The `--streaming` CLI flag uses the word `streaming`, so the special command
mirrors it exactly to avoid a discrepancy between the flag and the interactive
command. Consistency is more important than brevity here because the command is
not typed frequently enough to make length a concern.

### Why suppress the final `println!` when `streamed_any_content()` is true?

The agent loop returns the full accumulated response as a `String` regardless of
whether streaming was used. Without suppression the response would be printed
twice: once token-by-token during streaming, and once in full after
`execute_with_observer` returns. The `streamed_any_content` guard is the minimal
check needed to prevent this. Using the observer flag is safer than using
`streaming_enabled` because it handles the edge case where streaming is enabled
but the provider emits no chunks and falls back to batch delivery.

### Why TRACE level for wire-format logging instead of DEBUG?

DEBUG is already used for operational events (session creation, prompt
processing, token counts). Adding `NewSessionResponse` JSON at DEBUG level would
flood the log for every session creation and make normal debug output harder to
read. TRACE is reserved for data-level inspection. An operator who needs to
diagnose Zed wire-format issues runs with `--trace` or
`RUST_LOG=xzatoma::acp=trace`; all other users see no extra noise.

### Why not log both `modes` and `configOptions` separately?

Serializing the full `NewSessionResponse` JSON is simpler and more complete than
extract-and-log approaches. It captures everything Zed receives in one line,
including any future fields added to the response struct. The downside (slightly
larger log line) is insignificant at TRACE level since TRACE is never enabled in
production.

### Why restrict plan tracking to items before the first tool call?

Numbered lists appear throughout model output: explanations, tool outputs,
troubleshooting guides, and changelog entries all use `1. ... 2. ...` syntax.
Without a boundary rule, the tracker would produce false-positive plan entries
for every numbered list in tool results or the model's final explanation. The
boundary rule (stop tracking when `ToolCallStarted` fires) is a pragmatic
heuristic: genuine multi-step plans almost always appear in the model's opening
statement before any tool execution begins, while post-tool numbered lists are
usually incidental.

### Why use `PlanEntryPriority::Medium` for all detected entries?

The numbered-list parser has no information about relative priority; all
detected steps are treated equally. Using `Medium` as the default avoids
overloading any step with a `High` or `Low` signal the agent did not explicitly
express. A future extension could parse prefixes like `[CRITICAL]` or
`(optional)` to derive priority, but this adds complexity that is not justified
until there is evidence users need priority differentiation.

### Why emit `SessionUpdate::Plan` incrementally instead of only at the end?

Zed renders the checklist in real time as plan events arrive. Emitting only a
final plan (after the turn) defeats the purpose: users see nothing during the
potentially long execution phase and the checklist appears only at completion.
Incremental emission means the checklist is visible from the moment the model
outputs its first numbered item, giving users early confirmation of what steps
are planned.
