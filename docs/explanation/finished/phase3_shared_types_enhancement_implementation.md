# Phase 3: Shared Types Enhancement Implementation

## Overview

Phase 3 of the providers modernisation enriches the shared domain types in
`src/providers/types.rs` with four improvements: a typed `FinishReason` enum
attached to every `CompletionResponse`, convenience boolean fields on
`ModelInfo` that mirror the `capabilities` vector, a new `CodeGeneration`
variant for `ModelCapability` combined with deprecation notices on two stale
variants, and a `with_limits` builder on `ModelInfoSummary`. Downstream files
(`copilot.rs`, `ollama.rs`, `openai.rs`) were updated to integrate these
changes.

---

## Motivation

The shared types module is the contract between providers and all consuming
code. Before Phase 3 it had several gaps:

- Callers that wanted to know _why_ a completion ended had to parse a raw string
  from the provider-specific response struct. There was no normalised
  representation at the domain level.
- Deciding whether a `ModelInfo` supports tool calling required iterating the
  `capabilities` vector at every call site. The boolean was computed repeatedly
  in CLI tables, context-window checks, and agent summarisation.
- `ModelCapability` had a `Completion` variant that overlapped with more
  specific variants (`FunctionCalling`, `Streaming`) and a `JsonMode` variant
  that was never meaningfully acted upon in routing logic. Neither was safe to
  remove without breaking match arms across the codebase.
- There was no first-class model for code-generation capability, so code-focused
  Ollama models were silently lumped with generic models.
- `ModelInfoSummary` had `max_prompt_tokens` and `max_completion_tokens` fields
  but no builder method, forcing callers to use the verbose positional
  constructor even when they only needed to set those two fields.

---

## Changes by Task

### Task 3.1 — FinishReason Enum and CompletionResponse Field

A new public enum `FinishReason` was added to `src/providers/types.rs` between
`ProviderCapabilities` and `CompletionResponse`.

#### Enum definition

```xzatoma/src/providers/types.rs#L530-560
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

The variants cover the five reason codes used across OpenAI-compatible
providers. `#[serde(rename_all = "snake_case")]` means the serialised strings
match the OpenAI wire format exactly (`"stop"`, `"length"`, `"tool_calls"`,
`"content_filter"`, `"other"`), which simplifies future provider mapping code in
Phase 4 and Phase 5.

`Default` derives to `Stop` via the `#[default]` attribute so that
`FinishReason::default()` returns the value used by all current constructors.

#### CompletionResponse changes

`pub finish_reason: FinishReason` was appended to `CompletionResponse`. All
three existing constructors (`new`, `with_usage`, `with_model`) now initialise
this field to `FinishReason::Stop`. A new builder method `with_finish_reason`
allows callers to override the value in a method-chain:

```xzatoma/src/providers/types.rs#L680-695
pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
    self.finish_reason = reason;
    self
}
```

Providers that parse a finish-reason field from the API response (Phase 4 and
Phase 5) can chain this builder rather than reaching into the struct directly.

No existing providers were changed in this phase; they all produce
`FinishReason::Stop` by default, which is correct for the non-streaming
completion paths they currently use.

### Task 3.2 — Convenience Booleans on ModelInfo

Two `bool` fields were added to `ModelInfo`:

| Field                          | Mirrors                                                    |
| ------------------------------ | ---------------------------------------------------------- |
| `pub supports_tools: bool`     | `capabilities.contains(&ModelCapability::FunctionCalling)` |
| `pub supports_streaming: bool` | `capabilities.contains(&ModelCapability::Streaming)`       |

Both are annotated `#[serde(default)]` so that deserialisation of existing JSON
(which lacks these fields) succeeds and defaults both to `false`.

#### Invariant maintenance

Three methods keep the booleans in sync with `capabilities`:

**`ModelInfo::new`** initialises both to `false` because `new` always starts
with an empty capabilities vector.

**`ModelInfo::add_capability`** updates the appropriate boolean immediately
after pushing the new capability:

```xzatoma/src/providers/types.rs#L330-345
if capability == ModelCapability::FunctionCalling {
    self.supports_tools = true;
}
if capability == ModelCapability::Streaming {
    self.supports_streaming = true;
}
```

**`ModelInfo::with_capabilities`** recomputes both booleans from the supplied
vector after replacing `self.capabilities`:

```xzatoma/src/providers/types.rs#L395-400
self.supports_tools = self.capabilities.contains(&ModelCapability::FunctionCalling);
self.supports_streaming = self.capabilities.contains(&ModelCapability::Streaming);
```

This covers the case where `with_capabilities` is called with a vector that
_removes_ a previously-present capability. Without this recomputation, the
boolean could diverge from `capabilities` and return a stale `true`.

#### Usage

Code that previously iterated the capabilities vector to decide whether a model
supports tools can now read the boolean directly:

```xzatoma/src/providers/types.rs#L340-350
// Before Phase 3
if model.supports_capability(ModelCapability::FunctionCalling) { ... }

// After Phase 3 (equivalent, but O(1) and readable)
if model.supports_tools { ... }
```

### Task 3.3 — ModelCapability Revision

#### CodeGeneration variant

A new variant was added to represent models primarily optimised for code:

```xzatoma/src/providers/types.rs#L200-202
/// Model is optimised for code generation and code-related tasks.
CodeGeneration,
```

`ollama.rs::add_model_capabilities` was updated to assign `CodeGeneration` to
seven code-focused model families: `codellama`, `codegemma`, `deepseek-coder`,
`starcoder`, `starcoder2`, `codestral`, and `qwen2.5-coder`.

This makes it possible for downstream code (agent routing, CLI model listing) to
filter or label code-generation-focused models without relying on name
heuristics.

#### Deprecation of Completion and JsonMode

Both variants were marked `#[deprecated]` with a note pointing callers to
`FunctionCalling` and `Streaming`:

```xzatoma/src/providers/types.rs#L185-195
#[deprecated(note = "Use `FunctionCalling` or `Streaming` instead. Will be removed in a future release.")]
Completion,

#[deprecated(note = "Use `FunctionCalling` or `Streaming` instead. Will be removed in a future release.")]
JsonMode,
```

Neither variant is removed in this phase because existing match arms in tests
and provider code reference them. Removal is deferred to a later phase once all
remaining uses have been migrated.

#### Suppressing deprecation warnings at known use sites

Three locations use the deprecated variants and are annotated with
`#[allow(deprecated)]`:

| File                      | Location                              | Reason                                        |
| ------------------------- | ------------------------------------- | --------------------------------------------- |
| `src/providers/types.rs`  | `Display::fmt`                        | Match must cover all variants                 |
| `src/providers/openai.rs` | `list_models` closure                 | Assigns `Completion` until Phase 4            |
| `src/providers/ollama.rs` | `build_model_info_from_show_response` | Maps `"completion"` and `"json_mode"` strings |

The task specification (Phase 3 Task 3.7) explicitly requires that the
deprecation warnings are visible in `cargo check` output but do not fail
`cargo clippy -- -D warnings`. The `#[allow(deprecated)]` annotations at each
known site achieve this.

Tests that assert on `ModelCapability::Completion` or
`ModelCapability::JsonMode` are annotated with `#[allow(deprecated)]` at the
function level.

### Task 3.4 — with_limits Builder on ModelInfoSummary

A `with_limits` builder method was added to `impl ModelInfoSummary`:

```xzatoma/src/providers/types.rs#L510-530
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

This method follows the same pattern as `set_model` and `set_reasoning` on
`CompletionResponse`, enabling clean builder chains:

```xzatoma/src/providers/types.rs#L518-522
ModelInfoSummary::from_model_info(info)
    .with_limits(Some(6144), Some(2048))
```

`CopilotProvider::convert_to_summary` was updated to chain
`.with_limits(None, None)` after the existing `ModelInfoSummary::new(...)` call.
The arguments are `None` because `CopilotModelLimits` currently only exposes
`max_context_window_tokens` (used for `context_window` on `ModelInfo`, not for
prompt or completion limits). Phase 5 (Copilot Provider Improvements, Task 5.1)
will expand `CopilotModelLimits` with `max_prompt_tokens` and
`max_completion_tokens` fields; at that point, the `None` arguments become the
obvious location to substitute real values.

---

## Test Coverage

### New tests in types.rs (10 added)

| Test                                                              | What it verifies                                                                           |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `test_finish_reason_serialization_all_variants`                   | All five variants serialise to the correct snake_case JSON string and round-trip correctly |
| `test_completion_response_with_finish_reason`                     | `with_finish_reason` builder sets the field                                                |
| `test_completion_response_default_finish_reason_is_stop`          | All three constructors default to `FinishReason::Stop`                                     |
| `test_model_info_supports_tools_true_when_function_calling_added` | `add_capability(FunctionCalling)` sets `supports_tools = true`                             |
| `test_model_info_supports_streaming_true_when_streaming_added`    | `add_capability(Streaming)` sets `supports_streaming = true`                               |
| `test_model_info_with_capabilities_syncs_supports_tools`          | `with_capabilities` sets both booleans from the supplied vector                            |
| `test_model_info_with_capabilities_clears_supports_when_missing`  | `with_capabilities` resets booleans to `false` when the relevant capability is absent      |
| `test_model_info_summary_with_limits`                             | `with_limits` stores non-None values                                                       |
| `test_model_info_summary_with_limits_none`                        | `with_limits(None, None)` leaves both fields as `None`                                     |
| `test_model_capability_code_generation_round_trips_json`          | `CodeGeneration` serialises and deserialises correctly                                     |

### New tests in ollama.rs (1 added)

| Test                                          | What it verifies                                                             |
| --------------------------------------------- | ---------------------------------------------------------------------------- |
| `test_add_model_capabilities_code_generation` | All seven code-generation families produce `ModelCapability::CodeGeneration` |

### Existing tests updated

The following existing tests had new assertions or allow annotations added:

| Test                                                                       | Change                                                                          |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `test_model_capability_display`                                            | Added `#[allow(deprecated)]` and `CodeGeneration` assertion                     |
| `test_model_info_creation`                                                 | Added `assert!(!model.supports_tools)` and `assert!(!model.supports_streaming)` |
| `test_model_info_add_capability`                                           | Added `supports_tools` sync assertions                                          |
| `test_model_info_with_capabilities`                                        | Added `supports_tools` and `supports_streaming` assertions                      |
| `test_completion_response_new`                                             | Added `finish_reason == FinishReason::Stop` assertion                           |
| `test_completion_response_with_usage`                                      | Added `finish_reason == FinishReason::Stop` assertion                           |
| `test_build_model_info_from_show_response_parses_context_and_capabilities` | Added `#[allow(deprecated)]`                                                    |
| `test_list_models` (openai)                                                | Added `#[allow(deprecated)]`                                                    |

---

## Files Modified

| File                       | Summary of changes                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/providers/types.rs`   | Added `FinishReason` enum; added `finish_reason` field to `CompletionResponse`; added `with_finish_reason` builder; added `supports_tools` and `supports_streaming` to `ModelInfo`; synced booleans in `add_capability` and `with_capabilities`; added `CodeGeneration` variant; deprecated `Completion` and `JsonMode`; updated `Display`; added `with_limits` builder to `ModelInfoSummary`; updated existing tests and added 10 new tests |
| `src/providers/ollama.rs`  | Added `CodeGeneration` arm to `add_model_capabilities`; added `#[allow(deprecated)]` in `build_model_info_from_show_response`; added `test_add_model_capabilities_code_generation` test                                                                                                                                                                                                                                                      |
| `src/providers/copilot.rs` | Chained `.with_limits(None, None)` in `convert_to_summary`; added explanatory comment for Phase 5                                                                                                                                                                                                                                                                                                                                            |
| `src/providers/openai.rs`  | Added `#[allow(deprecated)]` at the `Completion` usage site in `list_models` and on the relevant test                                                                                                                                                                                                                                                                                                                                        |

---

## Quality Gate Results

All four mandatory quality gates passed after all changes were applied:

```xzatoma/docs/explanation/phase3_shared_types_enhancement_implementation.md#L1-1
cargo fmt --all                                           # clean
cargo check --all-targets --all-features                 # clean
cargo clippy --all-targets --all-features -- -D warnings # clean
cargo test --all-features --lib -- providers::           # 233 passed, 0 failed, 2 ignored
```

The full cross-module test run (providers, agent, tools, commands, mcp, cli)
produced **1,046 tests passing, 0 failures, 5 ignored** (the 5 ignored tests
require live network services).

---

## Design Decisions

### Why FinishReason is Copy

`FinishReason` is a simple C-like enum with no heap allocations. Making it
`Copy` means it can be stored in `CompletionResponse` and passed by value in
builder chains without requiring cloning. The `Copy` bound is consistent with
`ModelCapability` and `TokenUsage`, which also derive `Copy`.

### Why supports_tools and supports_streaming are not computed properties

Rust does not have computed properties; a method
`fn supports_tools(&self) -> bool` is the idiomatic equivalent. The decision to
use struct fields instead was made to keep JSON serialisation round-trippable (a
method would not be serialised) and to make the boolean available in contexts
where the `ModelInfo` is treated as plain data (CLI table rendering, agent
context checks). The `#[serde(default)]` annotation on both fields preserves
backwards-compatible deserialisation of JSON written before this phase.

### Why Completion and JsonMode are deprecated rather than removed

Removing enum variants is a breaking change in Rust: every exhaustive match arm
that names the variant fails to compile. The existing test suite has match arms
and direct assertions on both variants. The deprecation approach lets CI catch
new uses while existing code continues to compile. Removal is safe only after
all reference sites are updated, which is the responsibility of a future phase.

### Why with_limits uses None in copilot.rs

The Copilot API endpoint for model listing returns a `limits` object with
`max_context_window_tokens` only. The fields `max_prompt_tokens` and
`max_completion_tokens` are absent from the current API response schema. Passing
`None` is therefore accurate, not a stub. Phase 5 (Task 5.1) will add the
corresponding fields to `CopilotModelLimits` once the API provides them, at
which point the `None` arguments in `with_limits` are the obvious change points.
