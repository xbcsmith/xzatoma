# Phase 4: OpenAI Provider Improvements Implementation

## Overview

Phase 4 modernizes `src/providers/openai.rs` with six focused improvements:
structured streaming via `StreamAccumulator`, reasoning content capture,
finish-reason and token-usage propagation on both completion paths, non-chat
model filtering, capability inference from model identifiers, and a safer
`get_model_info` path that uses percent-encoded direct requests with a list
fallback.

The deprecated `ModelCapability::Completion` variant is no longer constructed
anywhere in `openai.rs`. All capability assignment is now handled by the
`build_capabilities_from_id` function.

---

## Changes by Task

### Task 4.1: StreamAccumulator

The inline per-field buffers (`content_buf`, `tool_call_map`) that lived inside
`post_completions_streaming` have been replaced with a dedicated
`StreamAccumulator` struct.

#### Struct fields

| Field           | Type                                | Purpose                                        |
| --------------- | ----------------------------------- | ---------------------------------------------- |
| `content`       | `String`                            | Accumulated text from `delta.content`          |
| `reasoning`     | `Option<String>`                    | Accumulated text from `delta.reasoning`        |
| `tool_calls`    | `HashMap<u32, AccumulatedToolCall>` | Partial tool-call state keyed by delta `index` |
| `usage`         | `Option<TokenUsage>`                | Token counts from the final usage chunk        |
| `finish_reason` | `FinishReason`                      | Last-seen finish reason, defaults to `Stop`    |

#### Methods

- `new() -> Self` - creates an empty accumulator with default values.
- `apply_chunk(&mut self, chunk: &OpenAIStreamChunk)` - processes one parsed SSE
  chunk: appends content and reasoning, delegates tool-call deltas, captures
  finish reason, and records usage.
- `apply_tool_call_chunk(&mut self, tc_deltas: &[OpenAIStreamToolCallDelta])` -
  applies incremental tool-call data keyed by index, populating id, name, and
  arguments incrementally.
- `finalize(self) -> CompletionResponse` - consumes the accumulator and produces
  a `CompletionResponse`. Tool calls are ordered by index. Usage, finish reason,
  and reasoning are set on the returned response.

The streaming loop in `post_completions_streaming` now reads:

```rust
let mut acc = StreamAccumulator::new();
// ...
acc.apply_chunk(&sse_chunk);
// ...
Ok(acc.finalize())
```

### Task 4.2: reasoning Field on OpenAIStreamDelta

`OpenAIStreamDelta` gained a new field:

```rust
#[serde(default)]
reasoning: Option<String>,
```

`StreamAccumulator::apply_chunk` appends reasoning fragments to a lazily
initialized `Option<String>` buffer. `finalize` sets the buffer on the
`CompletionResponse` via `set_reasoning` when any content was captured.

This supports the o1 family and any other extended-thinking model that emits
`reasoning` tokens over the stream.

### Task 4.3: Token Usage and finish_reason in Streaming

`OpenAIStreamChunk` now includes an optional `usage` field:

```rust
#[serde(default)]
usage: Option<OpenAIUsage>,
```

Some servers (including newer OpenAI model versions) include a usage object in
the final SSE chunk. `StreamAccumulator::apply_chunk` reads this field and
converts it to a `TokenUsage` value stored in the accumulator. `finalize` then
calls `CompletionResponse::with_usage` when the field is present.

`finish_reason` was already present on `OpenAIStreamChoice`. It is now read
inside `apply_chunk` and stored on the accumulator using `map_finish_reason`.

The non-streaming path (`post_completions`) was also updated. The first choice's
`finish_reason` field is mapped and set on the returned `CompletionResponse`
before the response is returned.

### Task 4.4: map_finish_reason

A new private function converts raw OpenAI finish-reason strings to typed
`FinishReason` values:

```rust
fn map_finish_reason(s: &str) -> FinishReason {
    match s {
        "stop"           => FinishReason::Stop,
        "length"         => FinishReason::Length,
        "tool_calls"
        | "function_call"=> FinishReason::ToolCalls,
        "content_filter" => FinishReason::ContentFilter,
        _                => FinishReason::Stop,
    }
}
```

The legacy `"function_call"` string (used by older OpenAI model versions) maps
to `FinishReason::ToolCalls` to preserve semantic equivalence. Any unknown
string, including an empty string, defaults to `Stop`.

### Task 4.5: is_non_chat_model and Filtering

A new private function classifies a model ID as non-chat:

```rust
fn is_non_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    lower.contains("embed")
        || lower.contains("tts")
        || lower.contains("whisper")
        || lower.contains("dall-e")
        || lower.contains("moderation")
}
```

`list_models` applies this filter before constructing `ModelInfo` values:

```rust
.filter(|entry| !is_non_chat_model(&entry.id))
```

This ensures that embedding, text-to-speech, speech-to-text, image generation,
and moderation models do not appear in the chat model listing returned to the
agent.

### Task 4.6: build_capabilities_from_id and encode_path_segment

#### build_capabilities_from_id

Replaces the previous unconditional assignment of the deprecated `Completion`
and `FunctionCalling` capabilities. It never constructs `Completion`.

```rust
fn build_capabilities_from_id(id: &str) -> Vec<ModelCapability> {
    let id_lower = id.to_lowercase();
    let mut caps = vec![ModelCapability::Streaming];
    let no_fc_patterns = ["babbage", "davinci", "curie", "ada", "text-"];
    if !no_fc_patterns.iter().any(|p| id_lower.contains(p)) {
        caps.push(ModelCapability::FunctionCalling);
    }
    caps
}
```

All OpenAI chat models receive `Streaming`. Older completion-only families
(babbage, davinci, curie, ada, text- prefix) do not receive `FunctionCalling`
because the OpenAI function-calling API is not available for those models.

`list_models` now calls `build_capabilities_from_id` for every entry that
survives the non-chat filter. `get_model_info` calls it when constructing
`ModelInfo` from a direct model endpoint response.

#### encode_path_segment

A new private function percent-encodes characters that are unsafe in URL path
segments:

```rust
fn encode_path_segment(s: &str) -> String {
    s.replace('%', "%25")
     .replace('/', "%2F")
     .replace('?', "%3F")
     .replace('#', "%23")
}
```

The `%` character is encoded first to prevent double-encoding any existing
percent sequences in the input.

#### Updated get_model_info

The previous implementation fell through directly to a list scan for every call.
The new implementation:

1. Percent-encodes the model name with `encode_path_segment`.
2. Issues a direct `GET /models/{encoded_id}` request.
3. On HTTP 404, delegates to `find_in_model_list` (the list-scan fallback).
4. On any other non-success status, returns an error.
5. On deserialization failure, logs a debug message and delegates to
   `find_in_model_list`.

A new private method `find_in_model_list` encapsulates the list-scan logic
previously inlined in `get_model_info`.

This approach avoids a full model-list fetch for providers whose `/models/{id}`
endpoint is functional and performs better for callers that already know the
model ID.

---

## Module-Level Change: FinishReason Re-export

`src/providers/mod.rs` was updated to re-export `FinishReason` alongside the
other shared types:

```rust
pub use types::{
    ..., FinishReason, ...
};
```

This makes `crate::providers::FinishReason` available to all provider
implementations and matches the `use xzatoma::providers::FinishReason` path
documented in the type's doc comments.

---

## Tests Added

All tests follow the `test_<function>_<condition>_<expected>` naming convention.

### map_finish_reason tests (6)

| Test name                                                      | Condition                      |
| -------------------------------------------------------------- | ------------------------------ |
| `test_map_finish_reason_stop_returns_stop`                     | Input `"stop"`                 |
| `test_map_finish_reason_length_returns_length`                 | Input `"length"`               |
| `test_map_finish_reason_tool_calls_returns_tool_calls`         | Input `"tool_calls"`           |
| `test_map_finish_reason_function_call_maps_to_tool_calls`      | Input legacy `"function_call"` |
| `test_map_finish_reason_content_filter_returns_content_filter` | Input `"content_filter"`       |
| `test_map_finish_reason_unknown_string_defaults_to_stop`       | Unknown and empty inputs       |

### is_non_chat_model tests (6)

| Test name                                      | Condition                              |
| ---------------------------------------------- | -------------------------------------- |
| `test_is_non_chat_model_true_for_embed`        | Embedding model IDs                    |
| `test_is_non_chat_model_true_for_tts`          | TTS model IDs                          |
| `test_is_non_chat_model_true_for_whisper`      | Whisper model IDs                      |
| `test_is_non_chat_model_true_for_dall_e`       | DALL-E model IDs                       |
| `test_is_non_chat_model_true_for_moderation`   | Moderation model IDs                   |
| `test_is_non_chat_model_false_for_chat_models` | GPT-4o, o1-mini, o3-mini, o4-mini etc. |

### encode_path_segment tests (5)

| Test name                                                 | Condition                       |
| --------------------------------------------------------- | ------------------------------- |
| `test_encode_path_segment_encodes_slash`                  | `/` becomes `%2F`               |
| `test_encode_path_segment_encodes_question_mark`          | `?` becomes `%3F`               |
| `test_encode_path_segment_encodes_hash`                   | `#` becomes `%23`               |
| `test_encode_path_segment_encodes_percent_first`          | `%` encoded before others       |
| `test_encode_path_segment_leaves_alphanumerics_unchanged` | Alphanumeric and `-` unchanged  |
| `test_encode_path_segment_encodes_multiple_specials`      | Multiple specials in one string |

### build_capabilities_from_id tests (4)

| Test name                                                              | Condition                           |
| ---------------------------------------------------------------------- | ----------------------------------- |
| `test_build_capabilities_from_id_modern_model_gets_streaming_and_fc`   | Modern GPT ID                       |
| `test_build_capabilities_from_id_gpt4o_mini_gets_streaming_and_fc`     | gpt-4o-mini                         |
| `test_build_capabilities_from_id_old_model_gets_streaming_only`        | babbage, davinci, curie, ada, text- |
| `test_build_capabilities_from_id_never_produces_deprecated_completion` | No Completion variant produced      |

### StreamAccumulator unit tests (8)

| Test name                                                        | Condition                            |
| ---------------------------------------------------------------- | ------------------------------------ |
| `test_stream_accumulator_processes_single_text_delta`            | Single content chunk                 |
| `test_stream_accumulator_concatenates_multiple_text_deltas`      | Multiple content chunks              |
| `test_stream_accumulator_accumulates_tool_call_deltas`           | Multi-chunk tool-call accumulation   |
| `test_stream_accumulator_orders_tool_calls_by_index`             | Out-of-order index delivery          |
| `test_stream_accumulator_captures_reasoning_content`             | Reasoning and content in same stream |
| `test_stream_accumulator_no_reasoning_when_absent`               | No reasoning field in chunks         |
| `test_stream_accumulator_captures_usage_from_chunk`              | Usage object in final chunk          |
| `test_stream_accumulator_empty_produces_empty_assistant_message` | Zero chunks finalized                |

### Integration tests (7, using wiremock)

| Test name                                                 | Condition                              |
| --------------------------------------------------------- | -------------------------------------- |
| `test_complete_non_streaming`                             | finish_reason set to Stop              |
| `test_complete_non_streaming_length_finish_reason`        | finish_reason set to Length            |
| `test_complete_streaming`                                 | finish_reason set via SSE chunk        |
| `test_complete_streaming_captures_finish_reason`          | finish_reason Length via SSE           |
| `test_stream_accumulator_done_sentinel_terminates_stream` | Content after DONE is ignored          |
| `test_fetch_models_filters_non_chat_models`               | Non-chat models excluded from list     |
| `test_get_model_info_direct_hit_returns_model_info`       | Direct GET succeeds                    |
| `test_get_model_info_falls_back_to_list_on_404`           | Direct GET 404 falls back to list scan |

The existing `test_list_models` test was updated to assert `Streaming` and
`FunctionCalling` capabilities (replacing the removed `Completion` check). The
`#[allow(deprecated)]` attribute on that test was removed from the capability
assertion. A new assertion confirms the deprecated `Completion` variant is not
assigned by the new implementation.

---

## Success Criteria Verification

| Criterion                                                                  | Status |
| -------------------------------------------------------------------------- | ------ |
| All four quality gates pass (`fmt`, `check`, `clippy -D warnings`, `test`) | Pass   |
| Deprecated `Completion` variant not constructed in `openai.rs`             | Pass   |
| Token usage populated in streaming `CompletionResponse` when present       | Pass   |
| `finish_reason` populated in both streaming and non-streaming paths        | Pass   |
| Model listings exclude embedding, audio, and image model IDs               | Pass   |
| `get_model_info` uses percent-encoded direct endpoint with 404 fallback    | Pass   |
| `FinishReason` re-exported from `crate::providers`                         | Pass   |

---

## Design Decisions

**StreamAccumulator uses HashMap internally.** The plan specified
`Vec<StreamToolCallBuilder>` but the OpenAI protocol delivers tool-call deltas
by sparse integer index, requiring random-access insertion. A
`HashMap<u32, AccumulatedToolCall>` is the correct data structure. The public
interface (`apply_chunk`, `finalize`) matches the plan specification exactly.

**encode_path_segment is a simple string replacement.** The function only needs
to handle the four characters that are meaningful as URL delimiters (`%`, `/`,
`?`, `#`). Pulling in a full URL encoding crate for four replacements would be
an unnecessary dependency.

**build_capabilities_from_id uses a deny-list for FunctionCalling.** All modern
OpenAI chat models support function calling. A deny-list of older families is
more future-proof than an allow-list: new models default to receiving
`FunctionCalling` without requiring a code change.

**OpenAIUsage is reused for the streaming usage field.** The plan referenced
`ChatUsage` but no such type exists in the codebase. `OpenAIUsage` has the
identical schema (`prompt_tokens`, `completion_tokens`, `total_tokens`) and
reusing it avoids an unnecessary type alias or duplicate struct.

**find_in_model_list extracted as a private method.** The list-scan logic is now
shared between the previous `get_model_info` path and the new 404 fallback,
eliminating duplication.
