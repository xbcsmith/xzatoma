# Phase 5: Copilot Provider Improvements Implementation

## Overview

Phase 5 modernizes `src/providers/copilot.rs` with six focused improvements:
expanded model limits and support flags, a structured `CopilotCache` replacing
inline TTL arithmetic, `ResponsesAccumulator` and `ChatCompletionsAccumulator`
for streaming paths, `ResponsesUsage` for the Responses endpoint, and
token-limit propagation in `convert_to_summary`.

All inline mutation of the raw `Arc<RwLock<...>>` TTL state has been removed.
Cache access now goes exclusively through `CopilotCache` methods (`is_valid`,
`invalidate`).

---

## Changes by Task

### Task 5.1: Expand CopilotModelLimits

Three new `Option<usize>` fields were added, each annotated with
`#[serde(default)]`:

| Field                             | Purpose                                       |
| --------------------------------- | --------------------------------------------- |
| `max_output_tokens`               | Maximum output tokens per request             |
| `max_prompt_tokens`               | Maximum prompt tokens accepted per request    |
| `max_non_streaming_output_tokens` | Maximum output tokens with streaming disabled |

The existing `max_context_window_tokens` field is unchanged.

`#[derive(Default)]` was added to the struct so that existing test literals can
use `..Default::default()` without listing every new field.

`convert_to_summary` now populates `ModelInfoSummary::max_prompt_tokens` from
`CopilotModelLimits::max_prompt_tokens` and
`ModelInfoSummary::max_completion_tokens` from
`CopilotModelLimits::max_output_tokens`. The previous `with_limits(None, None)`
placeholder calls were replaced with the actual values.

### Task 5.2: Expand CopilotModelSupports

Seven new fields were added, all with `#[serde(default)]`:

| Field                 | Type            | Purpose                                  |
| --------------------- | --------------- | ---------------------------------------- |
| `streaming`           | `Option<bool>`  | Whether the model supports SSE streaming |
| `parallel_tool_calls` | `Option<bool>`  | Parallel tool-call execution             |
| `structured_outputs`  | `Option<bool>`  | JSON-schema-constrained output           |
| `adaptive_thinking`   | `Option<bool>`  | Extended-thinking / adaptive reasoning   |
| `max_thinking_budget` | `Option<usize>` | Upper token bound for thinking           |
| `min_thinking_budget` | `Option<usize>` | Lower token bound for thinking           |
| `dimensions`          | `Option<bool>`  | Embedding dimensions configuration       |

`#[derive(Default)]` was added for the same backward-compatibility reason as
`CopilotModelLimits`.

Both `fetch_copilot_models` model-building loops (the normal path and the
401-retry path) and `convert_to_summary` now assign `ModelCapability::Streaming`
when `streaming == Some(true)`. The `supports_tools` boolean on `ModelInfo` is
automatically set when `FunctionCalling` is added via `add_capability` (Phase 3
behavior).

### Task 5.3: Introduce CopilotCache

A new private struct replaces the previous `ModelsCache` type alias
(`Arc<RwLock<Option<(Vec<ModelInfo>, u64)>>>`):

```rust
struct CopilotCache {
    models: Option<Vec<ModelInfo>>,
    raw_models: Option<Vec<CopilotModelData>>,
    cached_at: Option<Instant>,
}
```

#### Methods

- `new()` -- creates an empty cache with all fields set to `None`.
- `is_valid()` -- returns `true` when `cached_at` is set and less than
  `MODEL_CACHE_DURATION` (300 seconds) has elapsed.
- `invalidate()` -- resets all three fields to `None`.

The `CopilotProvider` struct field changed from `models_cache: ModelsCache` to
`models_cache: Arc<RwLock<CopilotCache>>`. The `models_cache_ttl_secs` field was
removed; the TTL is now the `MODEL_CACHE_DURATION` constant.

All inline `SystemTime::now().duration_since(UNIX_EPOCH)` / `expires_at`
arithmetic in `fetch_copilot_models` was replaced with `cache.is_valid()` reads
and `cache.cached_at = Some(Instant::now())` writes.

### Task 5.4: ResponsesAccumulator and ChatCompletionsAccumulator

Both accumulators share a private `PartialToolCall` struct:

```rust
struct PartialToolCall {
    call_id: String,
    name: String,
    arguments: String,
}
```

#### ResponsesAccumulator

Replaces the inline accumulation logic in `complete_responses_streaming`.

| Field           | Type                               | Purpose                               |
| --------------- | ---------------------------------- | ------------------------------------- |
| `content`       | `String`                           | Accumulated text from Message events  |
| `reasoning`     | `Option<String>`                   | Accumulated reasoning text            |
| `tool_calls`    | `HashMap<String, PartialToolCall>` | Partial tool calls keyed by `call_id` |
| `usage`         | `Option<TokenUsage>`               | Token usage (not available in SSE)    |
| `finish_reason` | `FinishReason`                     | Defaults to `Stop`                    |

Methods: `new()`, `apply_event(&mut self, event: &StreamEvent)`,
`apply_response_payload(&mut self, payload: &[ResponseInputContent])`,
`finalize(self) -> CompletionResponse`.

The streaming loop in `complete_responses_streaming` now reads:

```rust
let mut acc = ResponsesAccumulator::new();
// ...
acc.apply_event(&event);
// ...
Ok(acc.finalize().set_model(model.to_string()))
```

#### ChatCompletionsAccumulator

Replaces the inline accumulation logic in `complete_completions_streaming`.

| Field           | Type                               | Purpose                               |
| --------------- | ---------------------------------- | ------------------------------------- |
| `content`       | `String`                           | Accumulated text from Message events  |
| `tool_calls`    | `HashMap<String, PartialToolCall>` | Partial tool calls keyed by `call_id` |
| `usage`         | `Option<TokenUsage>`               | Token usage when endpoint provides it |
| `finish_reason` | `FinishReason`                     | Defaults to `Stop`                    |

Methods: `new()`, `apply_chunk(&mut self, event: &StreamEvent)`,
`finalize(self) -> CompletionResponse`.

When the same `call_id` appears in multiple `FunctionCall` events, the
`arguments` strings are concatenated in arrival order.

### Task 5.5: ResponsesUsage

A new private struct models the usage payload from the Responses endpoint:

```rust
struct ResponsesUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}
```

Methods:

- `effective_total()` -- returns `total_tokens` when present; otherwise sums
  `input_tokens` and `output_tokens`; returns `None` when neither combination
  yields a value.
- `to_token_usage()` -- converts to `TokenUsage` when both `input_tokens` and
  `output_tokens` are present.

The `ResponsesResponse` struct inside `complete_responses_blocking` was updated
with a `usage: Option<ResponsesUsage>` field. Token usage is now extracted and
included in the `CompletionResponse` when the API provides it.

---

## Additional Changes

### FinishReason Import

`FinishReason` was added to the imports from `crate::providers`. It is used in
both accumulator structs to track the finish reason of the completion.

### #[cfg(test)] on Test Module

The `mod tests` block was confirmed to have `#[cfg(test)]`, ensuring that test
helpers and test-only struct construction do not trigger dead-code warnings in
non-test compilation.

### #[allow(dead_code)] Annotations

The following items are intentionally designed for future use or are exercised
exclusively by tests:

- `CopilotCache::raw_models` -- will be used for caching raw model data
- `CopilotCache::invalidate` -- called from tests; will be used in token refresh
  paths
- `ResponsesUsage::effective_total` -- tested directly; used for total reporting
- `convert_stream_event_to_message` -- retained as a clean extraction of
  event-to-message conversion logic; exercised by existing tests

---

## Tests Added

All tests follow the `test_<function>_<condition>_<expected>` naming convention.

### CopilotModelLimits deserialization tests (2)

| Test name                                                                       | Condition               |
| ------------------------------------------------------------------------------- | ----------------------- |
| `test_copilot_model_limits_deserialization_all_fields`                          | All four fields present |
| `test_copilot_model_limits_deserialization_only_context_window_backward_compat` | Only context window     |

### CopilotModelSupports deserialization tests (2)

| Test name                                                                | Condition                  |
| ------------------------------------------------------------------------ | -------------------------- |
| `test_copilot_model_supports_deserialization_all_new_fields`             | All nine fields present    |
| `test_copilot_model_supports_deserialization_only_tool_calls_and_vision` | Only tool_calls and vision |

### CopilotCache unit tests (4)

| Test name                                                          | Condition                        |
| ------------------------------------------------------------------ | -------------------------------- |
| `test_copilot_cache_is_valid_returns_false_for_fresh_cache`        | cached_at is None                |
| `test_copilot_cache_is_valid_returns_true_after_population`        | Recent cached_at                 |
| `test_copilot_cache_is_valid_returns_false_after_duration_elapses` | Expired cached_at                |
| `test_copilot_cache_invalidate_resets_all_fields`                  | All fields None after invalidate |

### ResponsesAccumulator unit tests (4)

| Test name                                                            | Condition                      |
| -------------------------------------------------------------------- | ------------------------------ |
| `test_responses_accumulator_apply_event_accumulates_text_deltas`     | Multiple OutputText events     |
| `test_responses_accumulator_finalize_produces_expected_content`      | Single message event           |
| `test_responses_accumulator_captures_reasoning_from_reasoning_event` | Reasoning and content together |
| `test_responses_accumulator_empty_finalize_produces_empty_message`   | Zero events finalized          |

### ChatCompletionsAccumulator unit tests (3)

| Test name                                                                    | Condition                 |
| ---------------------------------------------------------------------------- | ------------------------- |
| `test_chat_completions_accumulator_apply_chunk_accumulates_tool_call_deltas` | Two different call_ids    |
| `test_chat_completions_accumulator_apply_chunk_same_index_multiple_chunks`   | Same call_id, args concat |
| `test_chat_completions_accumulator_text_content_from_message_event`          | Message text accumulation |

### ResponsesUsage unit tests (5)

| Test name                                                             | Condition                     |
| --------------------------------------------------------------------- | ----------------------------- |
| `test_responses_usage_effective_total_returns_total_when_present`     | total_tokens takes precedence |
| `test_responses_usage_effective_total_sums_when_total_absent`         | Sums input + output           |
| `test_responses_usage_effective_total_none_when_both_absent`          | All fields None               |
| `test_responses_usage_to_token_usage_returns_none_when_partial`       | Only input present            |
| `test_responses_usage_to_token_usage_returns_usage_when_both_present` | Both input and output present |

### convert_to_summary expanded limits tests (2)

| Test name                                                            | Condition                      |
| -------------------------------------------------------------------- | ------------------------------ |
| `test_convert_to_summary_populates_max_prompt_and_completion_tokens` | All limit fields populated     |
| `test_convert_to_summary_limits_none_when_fields_absent`             | Only context window, rest None |

---

## Success Criteria Verification

| Criterion                                                                      | Status |
| ------------------------------------------------------------------------------ | ------ |
| All four quality gates pass (`fmt`, `check`, `clippy -D warnings`, `test`)     | Pass   |
| `CopilotCache` has at least four dedicated unit tests                          | Pass   |
| `ResponsesAccumulator` has at least three dedicated unit tests (no HTTP)       | Pass   |
| `ChatCompletionsAccumulator` has at least three dedicated unit tests (no HTTP) | Pass   |
| `ResponsesUsage::effective_total` tested for total present and absent cases    | Pass   |
| No inline `Arc<RwLock<...>>` TTL state mutation remains in `CopilotProvider`   | Pass   |

---

## Design Decisions

**CopilotCache uses `Instant` instead of epoch seconds.** The previous approach
used `SystemTime::now().duration_since(UNIX_EPOCH).as_secs()` for the cache
timestamp. `Instant` is monotonic and cannot be affected by system clock
adjustments, making cache validity checks more reliable.

**MODEL_CACHE_DURATION is a constant.** The previous `models_cache_ttl_secs`
field on `CopilotProvider` was configurable at construction time but was always
set to 300. Making it a constant simplifies the cache logic and removes a
mutable state coupling between the provider and the cache.

**PartialToolCall is shared between both accumulators.** Both
`ResponsesAccumulator` and `ChatCompletionsAccumulator` need to accumulate
tool-call fragments from `StreamEvent::FunctionCall` events. A single shared
struct avoids duplication. The `call_id` is used as the map key because the
Copilot `StreamEvent::FunctionCall` variant does not carry an integer index like
the OpenAI streaming protocol.

**ResponsesUsage has both `effective_total` and `to_token_usage`.** The
`effective_total` method provides a single number for logging and display. The
`to_token_usage` method provides the prompt/completion breakdown needed by
`CompletionResponse`. Having both avoids overloading a single method with two
different semantic intents.

**convert_stream_event_to_message retained with #[allow(dead_code)].** The
function is no longer called from production code (replaced by the
accumulators), but it remains exercised by existing tests and provides a clean,
well-tested conversion path that may be useful for future refactoring.
