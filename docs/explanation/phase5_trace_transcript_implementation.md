# Phase 5: Structured Trace Transcript

## Overview

Phase 5 adds `TRACE`-level structured logging to the agent execution loop. When
`--trace` is active (or `RUST_LOG=trace`), operators can reconstruct the full
provider-visible conversation, tool invocations, and provider metadata from the
structured log stream. All expensive formatting is guarded by
`tracing::enabled!(Level::TRACE)` so release builds at `INFO` or `DEBUG` pay no
serialisation cost.

## Changes

### `src/agent/core.rs`

#### Task 5.1: Conversation Message Logging

In both `execute_with_observer()` and
`execute_provider_messages_with_observer()`, a TRACE-gated block is added
immediately before each `provider.complete()` call. For each message in
`prompt_messages`, one `trace!` event is emitted with fields:

| Field            | Source                                       |
| ---------------- | -------------------------------------------- |
| `msg_index`      | Position in the slice (0-based)              |
| `msg_role`       | `msg.role` (Display)                         |
| `msg_char_count` | Byte length of `msg.content`                 |
| `msg_content`    | Full content string (empty string if `None`) |

After the `tokio::select!` block, a second `trace!` event is emitted with:

| Field             | Source                                                 |
| ----------------- | ------------------------------------------------------ |
| `has_tool_calls`  | Whether any tool calls are present                     |
| `tool_call_count` | Count of tool calls (0 when `has_tool_calls` is false) |
| `response_chars`  | Byte length of `message.content`                       |

#### Task 5.2: Tool Call and Result Logging

In `execute_tool_call()`, two `trace!` events are added:

**Before dispatching** (after the existing `debug!` call, before registry
lookup):

| Field            | Source                         |
| ---------------- | ------------------------------ |
| `tool_name`      | `tool_call.function.name`      |
| `tool_call_id`   | `tool_call.id`                 |
| `tool_args_json` | `tool_call.function.arguments` |

**After the tool returns and before truncation**:

| Field                 | Source                                  |
| --------------------- | --------------------------------------- |
| `tool_name`           | `tool_call.function.name`               |
| `tool_call_id`        | `tool_call.id`                          |
| `tool_result_bytes`   | `result.output.len()` before truncation |
| `tool_result_preview` | First 200 chars of `result.output`      |

The existing `debug!("Executing tool: {}", tool_name)` at the start of
`execute_tool_call` is unchanged.

#### Task 5.3: Provider Metadata Logging

A new private method `fn log_provider_metadata(&self)` is added on `Agent`. It
is gated by `tracing::enabled!(tracing::Level::TRACE)` and emits:

| Field            | Source                                             |
| ---------------- | -------------------------------------------------- |
| `provider_model` | `self.provider.get_current_model()` (owned String) |
| `provider_type`  | `std::any::type_name::<dyn Provider>()`            |

`get_current_model()` is synchronous and makes no API calls, so the guard
ensures zero cost at `INFO` or `DEBUG` level.

`log_provider_metadata()` is called once at the start of both
`execute_with_observer()` and `execute_provider_messages_with_observer()`.

## Design Decisions

- **Field names use underscores** (`msg_index`, `tool_name`, etc.) to stay
  within Rust identifier constraints while retaining the namespacing intent from
  the spec.
- **Dispatch trace is unconditional** for tool calls (no TRACE guard) since it
  is a single `trace!` call with no allocation — the `trace!` macro itself
  short-circuits at compile time or via `LevelFilter`.
- **Result trace is TRACE-gated** because it collects the first 200 chars of
  output, which requires a `String` allocation.
- **`type_name::<dyn Provider>()`** is used for `provider_type` since
  `Arc<dyn Provider>` does not carry concrete type information without `Any`
  supertrait bounds.

## Usage

```bash
# Full structured transcript to stderr
xzatoma run --trace --prompt "hello"

# Full transcript to a JSON file, clean text to stderr
xzatoma run --trace --logfile /tmp/trace.log --prompt "hello"

# Verify no trace events at DEBUG level (guard is effective)
xzatoma run --debug --prompt "hello"
```

## Testing

Two new tests in `mod tests`:

- `test_log_provider_metadata_no_panic_when_model_is_none`: uses the existing
  `MockProvider` (which returns `None` from `current_model()`); asserts the
  helper completes without panic.
- `test_log_provider_metadata_uses_model_name`: uses a
  `MockProvider::with_model("test-model")` variant; asserts the helper completes
  without panic when a model name is set.

The existing `MockProvider` is extended with an optional `model: Option<String>`
field and a `with_model(model: &str) -> Self` constructor. The `current_model()`
implementation now returns `self.model.as_deref()` instead of always `None`.
