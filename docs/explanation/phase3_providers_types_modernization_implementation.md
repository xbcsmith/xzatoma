# Phase 3: Providers Types Modernization Implementation

## Overview

Phase 3 modernizes `src/providers/types.rs` by enriching the shared domain types
used across all providers. The changes fall into five tasks:

- Task 3.1: `FinishReason` enum and `finish_reason` field on
  `CompletionResponse`
- Task 3.2: Convenience boolean fields on `ModelInfo`
- Task 3.3: `CodeGeneration` variant and deprecations in `ModelCapability`
- Task 3.4: `with_limits` builder on `ModelInfoSummary`
- Task 3.5: Test additions and updates

No files other than `src/providers/types.rs` were modified.

---

## Task 3.1: FinishReason Enum and finish_reason Field

### Motivation

Previously callers that needed to inspect why a model stopped generating had to
match raw provider strings such as `"stop"`, `"length"`, or `"tool_calls"`.
Different providers use slightly different spellings, so every callsite had to
handle provider-specific details that should be abstracted away.

### Design

A new `FinishReason` enum normalises these strings into a typed value:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    #[default]
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Other,
}
```

Key design choices:

- `Copy` because the type carries no heap data and is frequently passed by
  value.
- `Default` derives to `Stop`, the most common finish reason, so callers that do
  not set an explicit reason get a sensible value automatically.
- `#[serde(rename_all = "snake_case")]` serialises variants to `"stop"`,
  `"tool_calls"`, `"content_filter"` etc., matching the wire format used by
  OpenAI-compatible providers.

`CompletionResponse` gained a new `pub finish_reason: FinishReason` field. All
three existing constructors (`new`, `with_usage`, `with_model`) initialise it to
`FinishReason::Stop`. A new builder method `with_finish_reason` follows the same
builder pattern already used by `set_model` and `set_reasoning`:

```rust
pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
    self.finish_reason = reason;
    self
}
```

### Placement

`FinishReason` is declared between the `ProviderCapabilities` struct and the
`CompletionResponse` struct, grouping it naturally with the response types it
annotates.

---

## Task 3.2: Convenience Booleans on ModelInfo

### Motivation

Code that needs to know whether a model supports tools or streaming previously
had to call `model.supports_capability(ModelCapability::FunctionCalling)`. This
is verbose at every callsite and requires importing `ModelCapability`. Two
derived boolean fields provide a faster, self-documenting alternative.

### Design

Two fields were added to `ModelInfo`:

```rust
#[serde(default)]
pub supports_tools: bool,

#[serde(default)]
pub supports_streaming: bool,
```

The `#[serde(default)]` attribute means existing serialised `ModelInfo` values
that lack these fields deserialise correctly with `false` rather than failing.

The fields are kept in sync automatically by two methods:

- `add_capability`: after pushing the new capability, the booleans are updated
  unconditionally (outside the duplicate-guard `if !contains` block) so they
  always reflect the current state.
- `with_capabilities`: after replacing the capabilities vector, both booleans
  are recomputed from the new vector using `Vec::contains`.

### Invariant

At all times:

```text
supports_tools     == capabilities.contains(&ModelCapability::FunctionCalling)
supports_streaming == capabilities.contains(&ModelCapability::Streaming)
```

This invariant is enforced by construction; there is no public way to mutate
`capabilities` directly without going through `add_capability` or
`with_capabilities`.

---

## Task 3.3: ModelCapability Revisions

### New Variant: CodeGeneration

A `CodeGeneration` variant was added to represent models optimised for
code-related tasks. It follows the same pattern as the other capability variants
and participates in `Display`, serialisation, and the capability checks already
used by callers.

### Deprecations: Completion and JsonMode

`Completion` and `JsonMode` were marked deprecated with a note directing users
to `FunctionCalling` or `Streaming` instead. The deprecation uses the standard
Rust `#[deprecated(note = "...")]` attribute so the compiler emits a warning at
every use site.

All code within `types.rs` that mentions these variants is annotated with
`#[allow(deprecated)]` to keep the build warning-free under `-D warnings`.
Specifically:

- `impl std::fmt::Display for ModelCapability` has `#[allow(deprecated)]` on the
  `fn fmt` function so the match arms for `Completion` and `JsonMode` do not
  trigger warnings.
- The test `test_model_capability_display` has `#[allow(deprecated)]` on the
  test function for the same reason.

### Display Update

The `CodeGeneration` arm was added to the match in `Display::fmt`:

```rust
Self::CodeGeneration => write!(f, "CodeGeneration"),
```

---

## Task 3.4: with_limits Builder on ModelInfoSummary

### Motivation

`ModelInfoSummary::new` requires all seven fields including `max_prompt_tokens`
and `max_completion_tokens`. Callers that start from `from_model_info` and only
need to set token limits previously had no ergonomic way to do so without
constructing the full struct literal.

### Design

A `with_limits` builder method was added after `from_model_info`:

```rust
pub fn with_limits(
    mut self,
    max_prompt_tokens: Option<usize>,
    max_completion_tokens: Option<usize>,
) -> Self {
    self.max_prompt_tokens = max_prompt_tokens;
    self.max_completion_tokens = max_completion_tokens;
    self
}
```

This enables a clean builder chain:

```rust
let summary = ModelInfoSummary::from_model_info(info)
    .with_limits(Some(6144), Some(2048));
```

---

## Task 3.5: Test Updates and New Tests

### Updated Tests

| Test                                  | Change                                                                                                                             |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `test_model_capability_display`       | Added `#[allow(deprecated)]`; added assertion for `CodeGeneration`                                                                 |
| `test_model_info_creation`            | Added assertions `!supports_tools` and `!supports_streaming` on a fresh instance                                                   |
| `test_model_info_add_capability`      | Added assertion `!supports_tools` before adding capability; added `supports_tools` true and `!supports_streaming` assertions after |
| `test_model_info_with_capabilities`   | Added assertions `supports_tools` true and `!supports_streaming` after `with_capabilities` with `FunctionCalling` and `Vision`     |
| `test_completion_response_new`        | Added assertion `finish_reason == FinishReason::Stop`                                                                              |
| `test_completion_response_with_usage` | Added assertion `finish_reason == FinishReason::Stop`                                                                              |

### New Tests

| Test                                                              | What it covers                                                                                    |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `test_finish_reason_serialization_all_variants`                   | All five variants round-trip through `serde_json` and produce the correct snake_case JSON strings |
| `test_completion_response_with_finish_reason`                     | `with_finish_reason` builder sets the field correctly                                             |
| `test_completion_response_default_finish_reason_is_stop`          | All three constructors default to `FinishReason::Stop`                                            |
| `test_model_info_supports_tools_true_when_function_calling_added` | `add_capability(FunctionCalling)` sets `supports_tools`                                           |
| `test_model_info_supports_streaming_true_when_streaming_added`    | `add_capability(Streaming)` sets `supports_streaming`                                             |
| `test_model_info_with_capabilities_syncs_supports_tools`          | `with_capabilities` containing both capabilities sets both booleans                               |
| `test_model_info_with_capabilities_clears_supports_when_missing`  | Replacing capabilities without `FunctionCalling` clears `supports_tools`                          |
| `test_model_info_summary_with_limits`                             | `with_limits` sets both token limit fields                                                        |
| `test_model_info_summary_with_limits_none`                        | `with_limits(None, None)` leaves both fields `None`                                               |
| `test_model_capability_code_generation_round_trips_json`          | `CodeGeneration` survives a `serde_json` round-trip                                               |

Total test count for `providers::types`: 60 (previously 48).

---

## Quality Gate Results

All four mandatory gates passed without modification to any file outside
`src/providers/types.rs`:

```text
cargo fmt --all                                        OK
cargo check --all-targets --all-features               OK
cargo clippy --all-targets --all-features -- -D warnings   OK
cargo test --all-features --lib -- providers::types::  60 passed, 0 failed
```

---

## Files Changed

| File                     | Change type                  |
| ------------------------ | ---------------------------- |
| `src/providers/types.rs` | Modified (only file touched) |
