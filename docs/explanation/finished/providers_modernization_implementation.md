# Providers Modernization Implementation Plan

## Overview

This plan modernizes the XZatoma provider layer across five sequential phases.
The work addresses structural fragmentation in `src/providers/base.rs`, gaps in
the `Provider` trait contract, missing type-level metadata in shared structs,
streaming and model-filtering deficiencies in the OpenAI provider, and
incomplete model-metadata coverage in the Copilot provider. Each phase builds
directly on the previous one so that no phase introduces a compilation break
that cannot be resolved within that same phase.

## Current State Analysis

### Existing Infrastructure

The provider layer consists of five files under `src/providers/`:

| File         | Lines | Role                                                                   |
| ------------ | ----- | ---------------------------------------------------------------------- |
| `base.rs`    | ~1990 | Shared types, wire structs, `Provider` trait, and tests mixed together |
| `mod.rs`     | ~310  | Two free-standing factory functions and re-exports                     |
| `copilot.rs` | ~4430 | OAuth device flow, caching, streaming, and `Provider` impl             |
| `ollama.rs`  | ~1200 | Non-streaming only; no vision support                                  |
| `openai.rs`  | ~1380 | Streaming via inline SSE loop; model list includes non-chat IDs        |

### Identified Issues

The following issues are addressed by this plan, listed in the order they must
be resolved:

1. `base.rs` mixes wire types, domain types, the `Provider` trait, and ~770
   lines of tests into a single file, making it difficult to navigate and costly
   to change any one concern without touching unrelated code.

2. The `Provider` trait lacks `is_authenticated`, `current_model`, synchronous
   `set_model`, `supports_streaming`, and `chat_completion_stream` as part of
   its contract. Providers implement these internally but callers cannot rely on
   them through the trait.

3. `CompletionResponse` has no `finish_reason` field, so the agent executor
   cannot distinguish a natural stop from a token-limit truncation or a
   tool-call trigger without inspecting message contents. `ModelInfo` lacks
   convenience booleans for tool and streaming support. `ModelCapability`
   contains a `Completion` variant applied to every model uniformly and a
   `JsonMode` variant that is never referenced, while the useful
   `CodeGeneration` variant is absent entirely.

4. The OpenAI SSE streaming path is implemented as a 130-line inline loop inside
   `post_completions_streaming`, making it untestable in isolation. Streaming
   responses discard token usage, finish reason, and reasoning content. Non-chat
   model IDs (embeddings, TTS, Whisper, DALL-E, moderation) appear in model
   listings. `get_model_info` always performs a full list scan rather than
   attempting the direct `/models/{id}` endpoint first. Model IDs containing
   path separators are not percent-encoded before being placed in URL paths.

5. `CopilotModelLimits` only maps `max_context_window_tokens` and discards the
   three additional limit fields returned by the Copilot API.
   `CopilotModelSupports` maps only `tool_calls` and `vision`, silently dropping
   seven additional capability flags now present in the API response. The model
   cache is a bare `Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` type alias
   with manual TTL arithmetic spread across the implementation. The SSE
   accumulation logic for both the Responses and Chat Completions endpoints
   lives inline inside their respective async methods, making the accumulation
   logic inaccessible to unit tests. Token usage from the Responses endpoint
   uses different field names than the Chat Completions endpoint but is parsed
   through the same struct.

## Implementation Phases

### Phase 1: Module Organization

#### Task 1.1 Create types.rs

Create `src/providers/types.rs` and move all domain types currently in
`src/providers/base.rs` into it: `Message`, `FunctionCall`, `ToolCall`,
`ModelCapability`, `TokenUsage`, `ModelInfo`, `ModelInfoSummary`,
`ProviderCapabilities`, `CompletionResponse`, `ProviderTool`,
`ProviderFunction`, `ProviderFunctionCall`, `ProviderToolCall`,
`ProviderMessage`, and `ProviderRequest`. Move the `convert_tools_from_json` and
`validate_message_sequence` free functions as well. The `impl` blocks and their
tests travel with their types.

#### Task 1.2 Create trait_mod.rs

Create `src/providers/trait_mod.rs` and move the `Provider` trait and its
`mod tests` block from `src/providers/base.rs` into it. The trait's imports
should reference `super::types::*` so no public paths change for downstream
consumers.

#### Task 1.3 Create factory.rs

Create `src/providers/factory.rs` containing a `ProviderFactory` unit struct.
Move the `create_provider` and `create_provider_with_override` free functions
from `src/providers/mod.rs` into `ProviderFactory` as associated functions. Keep
the public signatures identical so call sites in `src/agent/` and `src/cli.rs`
require no changes. Move the keyring service-name constants from
`src/providers/copilot.rs` into `ProviderFactory` as private constants so all
credential storage is co-located in one place.

#### Task 1.4 Update mod.rs and base.rs

Update `src/providers/mod.rs` to declare the three new submodules, re-export
`ProviderFactory`, `Provider`, and all types from `types.rs` that were
previously exported from `base.rs`. Remove the factory functions that have
moved. Reduce `base.rs` to a compatibility shim that re-exports everything from
`types.rs` and `trait_mod.rs` with a `#[deprecated]` doc note so no external
code breaks before the later phases complete.

#### Task 1.5 Testing Requirements

All existing tests in `base.rs` travel with their types into the new files. No
new tests are added in this phase. The phase is complete only when
`cargo test --all-features` passes without removing or disabling any test.

#### Task 1.6 Deliverables

- `src/providers/types.rs` containing all shared domain types and their tests
- `src/providers/trait_mod.rs` containing the `Provider` trait and its tests
- `src/providers/factory.rs` containing `ProviderFactory` and factory tests
- `src/providers/mod.rs` updated to declare and re-export the three new modules
- `src/providers/base.rs` reduced to a re-export shim

#### Task 1.7 Success Criteria

- `cargo check --all-targets --all-features` passes with zero errors
- `cargo test --all-features` passes with the same count as before the phase
- No public symbol previously exported from `src/providers/mod.rs` is removed or
  renamed

---

### Phase 2: Provider Trait Modernization

#### Task 2.1 Add Required Trait Methods

In `src/providers/trait_mod.rs`, add the following to the `Provider` trait:

- `fn is_authenticated(&self) -> bool` as a required method with no default. All
  three provider `impl` blocks already have this method; the change just
  promotes it into the trait so callers can use it generically.
- `fn current_model(&self) -> Option<&str>` as a required method returning a
  borrowed string slice.
- `fn set_model(&mut self, model: &str)` as a required synchronous method for
  in-memory model selection without API validation.
- `fn supports_streaming(&self) -> bool` as a provided method defaulting to
  `false`.
- `async fn chat_completion_stream` as a provided method that delegates to
  `complete` by default, matching the same signature as `complete`.

#### Task 2.2 Add fetch_models as Primary Method

Add `async fn fetch_models(&self) -> Result<Vec<ModelInfo>>` as a required
method. Change the default implementation of `list_models` to call
`self.fetch_models().await` instead of returning an error. Each provider's
existing `list_models` implementation becomes the body of `fetch_models`. Remove
the `list_models` override from each provider `impl` block and let it fall
through to the new default.

#### Task 2.3 Fix get_current_model Return Type

Change `fn get_current_model(&self) -> Result<String>` to
`fn get_current_model(&self) -> String` in `src/providers/trait_mod.rs`. Add a
default implementation that calls
`self.current_model().unwrap_or("none").to_string()`. Update the three provider
`impl` blocks in `copilot.rs`, `ollama.rs`, and `openai.rs` to remove their
`get_current_model` overrides. Update every call site in `src/agent/`,
`src/cli.rs`, and tests to drop the `?` operator.

#### Task 2.4 Improve Factory Error Messages

In `src/providers/factory.rs`, update the `AtomaError::Provider` message
returned for unrecognized provider type strings to include the list of supported
values: `copilot`, `ollama`, and `openai`. Update the corresponding test in
`src/providers/mod.rs` that asserts on the error message text.

#### Task 2.5 Testing Requirements

For each provider, add tests that exercise `is_authenticated`, `current_model`,
`set_model`, and `supports_streaming` through the `Provider` trait reference
(`&dyn Provider`) rather than the concrete type. Add a test confirming that
`chat_completion_stream` on a provider that does not override it returns the
same content as `complete`. Add a test confirming the improved error message for
unknown provider type includes each supported provider name.

#### Task 2.6 Deliverables

- Updated `src/providers/trait_mod.rs` with the expanded trait definition
- Updated `impl Provider` blocks in `copilot.rs`, `ollama.rs`, and `openai.rs`
- Updated `src/providers/factory.rs` with improved error messages
- Updated call sites in `src/agent/` and `src/cli.rs`
- New tests described in Task 2.5

#### Task 2.7 Success Criteria

- All four quality gates pass: `cargo fmt --all`,
  `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-features`
- Any code path that previously called `.get_current_model()?` now compiles
  without `?` and without a `let _ =` suppression
- The unknown-provider error message test passes

---

### Phase 3: Shared Types Enhancement

#### Task 3.1 Add FinishReason and finish_reason Field

In `src/providers/types.rs`, define a `FinishReason` enum with variants `Stop`,
`Length`, `ToolCalls`, `ContentFilter`, and `Other`. Derive `Debug`, `Clone`,
`Copy`, `PartialEq`, `Eq`, `Serialize`, and `Deserialize`. Add a
`finish_reason: FinishReason` field to `CompletionResponse`, defaulting to
`FinishReason::Stop` in all existing constructors. Add a `with_finish_reason`
builder method to `CompletionResponse`.

#### Task 3.2 Add Convenience Booleans to ModelInfo

In `src/providers/types.rs`, add `pub supports_tools: bool` and
`pub supports_streaming: bool` to `ModelInfo`. Derive both from the
`capabilities` vector inside `ModelInfo::new` so that
`capabilities.contains(&ModelCapability::FunctionCalling)` sets `supports_tools`
and `capabilities.contains(&ModelCapability::Streaming)` sets
`supports_streaming`. Update the `add_capability` method to keep these fields in
sync whenever a capability is added after construction.

#### Task 3.3 Revise ModelCapability

In `src/providers/types.rs`, add the `CodeGeneration` variant to
`ModelCapability`. Mark the `Completion` variant as `#[deprecated]` with a
message pointing to `FunctionCalling` and `Streaming` as the appropriate
replacements. Mark `JsonMode` as `#[deprecated]` with the same note. Do not
remove either deprecated variant in this phase to avoid breaking existing match
arms in tests. Update `add_model_capabilities` in `ollama.rs` to assign
`CodeGeneration` to code-focused model families.

#### Task 3.4 Add Token Limits to ModelInfoSummary

In `src/providers/types.rs`, add `pub max_prompt_tokens: Option<usize>` and
`pub max_completion_tokens: Option<usize>` to `ModelInfoSummary`. Add a
`with_limits` builder method. Update `CopilotProvider::convert_to_summary` in
`src/providers/copilot.rs` to populate these fields from `CopilotModelLimits`
when the data is available.

#### Task 3.5 Testing Requirements

Add unit tests for:

- `FinishReason` serialization round-trip for all five variants
- `CompletionResponse::with_finish_reason` sets the field correctly
- `ModelInfo::new` sets `supports_tools` and `supports_streaming` correctly for
  a variety of capability combinations
- `ModelInfo::add_capability` keeps `supports_tools` and `supports_streaming` in
  sync
- `ModelInfoSummary::with_limits` stores the values correctly
- `CodeGeneration` round-trips through JSON

#### Task 3.6 Deliverables

- Updated `src/providers/types.rs` with `FinishReason`, updated
  `CompletionResponse`, updated `ModelInfo`, revised `ModelCapability`, and
  updated `ModelInfoSummary`
- Updated `src/providers/copilot.rs` to populate `ModelInfoSummary` limits
- Updated `src/providers/ollama.rs` to assign `CodeGeneration`
- New tests described in Task 3.5

#### Task 3.7 Success Criteria

- All four quality gates pass
- Deprecation warnings for `Completion` and `JsonMode` are visible in
  `cargo check` output but do not trigger `-D warnings` failures because they
  are annotated with `#[allow(deprecated)]` in the one place they are still
  constructed (the OpenAI list-models path) until Phase 4 replaces them
- `ModelInfo` constructed with `FunctionCalling` capability has
  `supports_tools == true`

---

### Phase 4: OpenAI Provider Improvements

#### Task 4.1 Introduce StreamAccumulator

In `src/providers/openai.rs`, define a private `StreamAccumulator` struct with
fields for accumulated `content: String`, `reasoning: Option<String>`,
`tool_calls: Vec<StreamToolCallBuilder>`, `usage: Option<TokenUsage>`, and
`finish_reason: FinishReason`. Add an `apply_chunk` method that processes a
single `StreamChunk` and updates the accumulator state, an
`apply_tool_call_chunk` method that handles partial tool-call deltas, and a
`finalize` method that converts the accumulated state into a
`CompletionResponse`. Replace the inline loop in `post_completions_streaming`
with a call to `StreamAccumulator::new()` followed by calls to `apply_chunk` for
each parsed SSE event and a final call to `finalize`.

#### Task 4.2 Add reasoning to OpenAIStreamDelta

In `src/providers/openai.rs`, add `reasoning: Option<String>` to
`OpenAIStreamDelta` with `#[serde(default)]`. In
`StreamAccumulator::apply_chunk`, accumulate the reasoning field alongside
content. In `finalize`, set the `reasoning` field of the returned
`CompletionResponse` when any reasoning content was accumulated.

#### Task 4.3 Capture Token Usage and finish_reason in Streaming

In `src/providers/openai.rs`, add a `usage` field to `OpenAIStreamChunk` typed
as `Option<ChatUsage>` with `#[serde(default)]`. In
`StreamAccumulator::apply_chunk`, write the usage into the accumulator when the
field is present. Add `finish_reason: Option<String>` to `OpenAIStreamChoice`.
In `apply_chunk`, call `map_finish_reason` on the value and store it. In
`finalize`, include the captured usage and finish reason in the returned
`CompletionResponse`.

#### Task 4.4 Add map_finish_reason

In `src/providers/openai.rs`, add a private `map_finish_reason` function that
maps the strings `"stop"`, `"length"`, `"tool_calls"`, `"function_call"`, and
`"content_filter"` to the corresponding `FinishReason` variants, defaulting
unknown values to `FinishReason::Stop`. Update both the streaming and
non-streaming paths to call this function and set `finish_reason` on the
`CompletionResponse`.

#### Task 4.5 Filter Non-Chat Models

In `src/providers/openai.rs`, add a private `is_non_chat_model` function that
returns `true` when the lowercased model ID contains any of `"embed"`, `"tts"`,
`"whisper"`, `"dall-e"`, or `"moderation"`. In `fetch_models`, apply this filter
with `.filter(|entry| !is_non_chat_model(&entry.id))` before constructing the
`ModelInfo` vector.

#### Task 4.6 Add build_capabilities_from_id and encode_path_segment

In `src/providers/openai.rs`, add a private `build_capabilities_from_id`
function that infers `FunctionCalling` and `Streaming` capabilities from the
model ID string. Replace the existing pattern of unconditionally assigning
`Completion` and `FunctionCalling` in `fetch_models` with a call to
`build_capabilities_from_id`. Remove the use of the deprecated `Completion`
variant here.

Add a private `encode_path_segment` function that percent-encodes `%`, `/`, `?`,
and `#` characters in that order. Update `get_model_info` to first attempt
`GET /models/{encoded_id}` and fall back to a list scan only when the direct
request returns a 404 or deserialization fails.

#### Task 4.7 Testing Requirements

Add unit tests for:

- `StreamAccumulator` processes a single text delta correctly
- `StreamAccumulator` accumulates tool-call deltas across multiple chunks
- `StreamAccumulator` terminates on a `[DONE]` sentinel
- `StreamAccumulator` captures reasoning content
- `is_non_chat_model` returns `true` for each of the five excluded patterns and
  `false` for a plain chat model ID
- `encode_path_segment` encodes `/`, `?`, `#`, and `%` correctly and leaves
  alphanumerics unchanged
- `fetch_models` result does not contain any model ID matching
  `is_non_chat_model`
- `map_finish_reason` maps each supported string to the correct variant and maps
  an unknown string to `Stop`
- `get_model_info` for a model ID containing a slash uses the encoded direct
  endpoint and falls back to list scan on 404

#### Task 4.8 Deliverables

- Updated `src/providers/openai.rs` with `StreamAccumulator`,
  `map_finish_reason`, `is_non_chat_model`, `encode_path_segment`,
  `build_capabilities_from_id`, updated `OpenAIStreamDelta`, updated
  `OpenAIStreamChunk`, updated `OpenAIStreamChoice`, and updated
  `get_model_info`
- New tests described in Task 4.7

#### Task 4.9 Success Criteria

- All four quality gates pass
- The deprecated `Completion` variant is no longer constructed anywhere in
  `openai.rs`; all uses are replaced by the capability inference function
- Token usage and finish reason are populated in `CompletionResponse` for both
  the streaming and non-streaming paths
- Model listings do not include embedding, audio, or image model IDs

---

### Phase 5: Copilot Provider Improvements

#### Task 5.1 Expand CopilotModelLimits

In `src/providers/copilot.rs`, add `max_output_tokens: Option<usize>`,
`max_prompt_tokens: Option<usize>`, and
`max_non_streaming_output_tokens: Option<usize>` to `CopilotModelLimits`.
Annotate all four fields with `#[serde(default)]`. Update `convert_to_summary`
to populate `ModelInfoSummary::max_prompt_tokens` and
`ModelInfoSummary::max_completion_tokens` from the newly mapped fields.

#### Task 5.2 Expand CopilotModelSupports

In `src/providers/copilot.rs`, add the following fields to
`CopilotModelSupports`, all with `#[serde(default)]`:

- `streaming: Option<bool>`
- `parallel_tool_calls: Option<bool>`
- `structured_outputs: Option<bool>`
- `adaptive_thinking: Option<bool>`
- `max_thinking_budget: Option<usize>`
- `min_thinking_budget: Option<usize>`
- `dimensions: Option<bool>`

Update `convert_models_to_info` to assign `ModelCapability::Streaming` when
`streaming == Some(true)` and to set `supports_tools` on the resulting
`ModelInfo` from the `tool_calls` boolean.

#### Task 5.3 Introduce CopilotCache Struct

In `src/providers/copilot.rs`, define a private `CopilotCache` struct with
fields `models: Option<Vec<ModelInfo>>`,
`raw_models: Option<Vec<CopilotModelData>>`, and `cached_at: Option<Instant>`.
Add an `is_valid` method that checks whether `cached_at` is set and less than
`MODEL_CACHE_DURATION` old. Add an `invalidate` method that sets all three
fields to `None`. Replace the existing `ModelsCache` type alias and manual TTL
checks throughout `CopilotProvider` with an `Arc<RwLock<CopilotCache>>` field
and calls to `is_valid` and `invalidate`.

#### Task 5.4 Introduce ResponsesAccumulator and ChatCompletionsAccumulator

In `src/providers/copilot.rs`, define a private `ResponsesAccumulator` struct
with fields for accumulated content, reasoning, refusal, tool calls, usage, and
finish reason. Add an `apply_event` method that accepts a single `StreamEvent`
and updates accumulator state, an `apply_response_payload` method for
`ResponsePayload` items, and a `finalize` method that returns a
`CompletionResponse`. Replace the inline accumulation logic in
`complete_responses_streaming` with calls to these methods.

Define a private `ChatCompletionsAccumulator` struct with fields for content,
tool calls, usage, and finish reason. Add `apply_chunk` and `finalize` methods.
Replace the inline accumulation logic in `complete_completions_streaming` with
calls to these methods.

#### Task 5.5 Introduce ResponsesUsage

In `src/providers/copilot.rs`, define a private `ResponsesUsage` struct with
fields `input_tokens: Option<u64>`, `output_tokens: Option<u64>`, and
`total_tokens: Option<u64>`, all with `#[serde(default)]`. Add an
`effective_total` method that returns `total_tokens` if present, or the sum of
`input_tokens` and `output_tokens`. Update the Responses endpoint response
deserialization to use `ResponsesUsage` and convert it to `TokenUsage` via
`effective_total`.

#### Task 5.6 Testing Requirements

Add unit tests for:

- `CopilotModelLimits` deserialization with all four fields present
- `CopilotModelLimits` deserialization with only `max_context_window_tokens`
  present (backward compatibility)
- `CopilotModelSupports` deserialization with all new fields present
- `CopilotModelSupports` deserialization with only `tool_calls` and `vision`
  present
- `CopilotCache::is_valid` returns `false` for a freshly created cache
- `CopilotCache::is_valid` returns `true` after `models` and `cached_at` are
  populated with a recent timestamp
- `CopilotCache::is_valid` returns `false` after the cache duration elapses
- `CopilotCache::invalidate` resets all three fields to `None`
- `ResponsesAccumulator::apply_event` accumulates text deltas from successive
  `OutputTextDelta` events
- `ResponsesAccumulator::finalize` produces the expected `CompletionResponse`
  content
- `ChatCompletionsAccumulator::apply_chunk` accumulates tool-call deltas across
  chunk boundaries
- `ChatCompletionsAccumulator::apply_chunk` accumulates tool-call deltas where
  the same index appears in multiple chunks
- `ResponsesUsage::effective_total` returns `total_tokens` when present
- `ResponsesUsage::effective_total` sums `input_tokens` and `output_tokens` when
  `total_tokens` is absent
- `convert_to_summary` populates `max_prompt_tokens` and `max_completion_tokens`
  from the expanded limits struct

#### Task 5.7 Deliverables

- Updated `src/providers/copilot.rs` with expanded `CopilotModelLimits`,
  expanded `CopilotModelSupports`, new `CopilotCache` struct, new
  `ResponsesAccumulator`, new `ChatCompletionsAccumulator`, and new
  `ResponsesUsage`
- Updated `docs/explanation/` with this plan marked complete for Phase 5
- New tests described in Task 5.6

#### Task 5.8 Success Criteria

- All four quality gates pass
- `CopilotCache` has at least four dedicated unit tests covering both branches
  of `is_valid` and the post-`invalidate` state
- `ResponsesAccumulator` and `ChatCompletionsAccumulator` each have at least
  three dedicated unit tests that do not require a live HTTP connection
- `ResponsesUsage::effective_total` has tests for both the `total_tokens`
  present and absent cases
- No inline mutation of raw `Arc<RwLock<...>>` TTL state remains in
  `CopilotProvider`; all cache access goes through `CopilotCache` methods
