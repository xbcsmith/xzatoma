# Phase 6: Observer Pattern and Cancellation-Aware Execution

## Overview

Phase 6 introduces two new execution methods to `Agent` that add structured
event observation and cooperative cancellation to the agent execution loop. The
legacy `execute` and `execute_provider_messages` methods are refactored to
delegate to these new methods, preserving full backwards compatibility while
enabling richer runtime introspection.

## What Was Added

### New Methods on `Agent`

#### `execute_with_observer`

```rust
pub async fn execute_with_observer(
    &mut self,
    user_prompt: impl Into<String>,
    cancellation_token: &CancellationToken,
    observer: &mut dyn AgentObserver,
) -> Result<String>
```

The evented core execution path for prompt-based agent runs. It is equivalent to
`execute` but emits `AgentExecutionEvent` values at every meaningful boundary
and checks the `CancellationToken` at every safe point.

#### `execute_provider_messages_with_observer`

```rust
pub async fn execute_provider_messages_with_observer(
    &mut self,
    messages: Vec<Message>,
    cancellation_token: &CancellationToken,
    observer: &mut dyn AgentObserver,
) -> Result<String>
```

The evented core execution path for provider-message-based runs. It is
equivalent to `execute_provider_messages` and additionally detects multimodal
image content in the supplied messages, emitting `VisionInputAttached` when
image parts are found before the execution loop begins.

### Refactored Legacy Methods

`execute` and `execute_provider_messages` now delegate entirely to the new
`_with_observer` variants, passing a fresh `CancellationToken::new()` (which is
never cancelled) and a `NoOpObserver` that silently discards all events. The
public interface, error contract, and behaviour are unchanged.

## Architecture

### Event Flow

For a typical single-turn execution the events emitted in order are:

1. `PromptStarted` - emitted once before the conversation is mutated.
2. `ProviderRequestStarted` - emitted immediately before each provider call.
3. `ProviderResponseReceived` - emitted after each provider response arrives,
   carrying the optional text content and a flag indicating whether tool calls
   were present.
4. `AssistantTextEmitted` - emitted when the response contains non-empty text.
5. `ExecutionCompleted` - emitted with the final assistant response text.

For tool-calling turns, between `ProviderResponseReceived` and the next
`ProviderRequestStarted` the following events are emitted for each tool call:

- `ToolCallStarted` - name, id, and raw JSON arguments.
- `ToolCallCompleted` or `ToolCallFailed` - result or error description.

For multimodal (vision) runs using `execute_provider_messages_with_observer`, a
single `VisionInputAttached` event is emitted after `PromptStarted` carrying the
count of image-bearing messages detected.

On any error path the final event is `ExecutionFailed` carrying the error
string, followed immediately by the method returning `Err`.

On cancellation the final event is `CancellationRequested` followed immediately
by `Err(XzatomaError::Cancelled)`.

### Cancellation Boundaries

The `CancellationToken` is checked at the following points in each iteration:

- At the top of the loop body, before incrementing the iteration counter.
- In a `tokio::select!` racing the provider `complete` call against
  `cancellation_token.cancelled()`.
- Before each individual tool call.
- In a `tokio::select!` racing each tool executor call against
  `cancellation_token.cancelled()`.

This means cancellation is cooperative and bounded: an in-progress provider HTTP
request or a running tool will be dropped at the next `await` point when the
token is cancelled, and `XzatomaError::Cancelled` is returned to the caller.

### Observer Contract

The `AgentObserver` trait requires `Send` but is called synchronously on the
async task's thread. Implementations must not block for extended periods. For
non-blocking use cases such as logging or metrics counters this is
straightforward. For use cases that need to forward events across threads,
implementations should use a non-blocking channel send (e.g. `try_send`) and
discard events if the channel is full rather than blocking.

### Vision Detection

Vision detection in `execute_provider_messages_with_observer` uses
`Message::has_image_content()`, which inspects `content_parts` for any
`ProviderMessageContentPart::Image` variant. This is consistent with how the
rest of the provider layer handles multimodal content and avoids coupling the
agent layer to provider-specific serialisation details.

## Module Dependencies

No new module dependencies were introduced. `tokio_util` was already a declared
dependency in `Cargo.toml`. The `events` module was already declared and
exported from `src/agent/mod.rs`.

The import block in `core.rs` was extended with:

```rust
use crate::agent::events::{AgentExecutionEvent, AgentObserver, NoOpObserver};
use tokio_util::sync::CancellationToken;
```

## Testing

Three new tests were added to the `mod tests` block in `core.rs`:

| Test                                                                | What it verifies                                                                         |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `test_execute_with_observer_emits_prompt_started`                   | A successful run emits at least `PromptStarted` and `ExecutionCompleted`.                |
| `test_execute_with_observer_respects_cancellation`                  | A pre-cancelled token causes the method to return `XzatomaError::Cancelled` immediately. |
| `test_execute_provider_messages_with_observer_emits_prompt_started` | A successful run via the provider-messages path emits at least `PromptStarted`.          |

All existing tests continue to pass because the `execute` and
`execute_provider_messages` delegation paths exercise the same logic as before.

## Backwards Compatibility

The public interface is fully backwards compatible. Callers of `execute` and
`execute_provider_messages` require no changes. The new methods are purely
additive public API.
