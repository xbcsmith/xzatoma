# Phase 6: Prompt Execution, Streaming Updates, Queueing, and Cancellation

This document explains the design and implementation decisions made during Phase
6 of the XZatoma ACP integration. Phase 6 adds evented agent execution, a
per-prompt cancellation model, FIFO prompt queuing with live session
notifications, and `CancelNotification` handling to the ACP stdio transport.

## Overview

Before Phase 6, the ACP stdio prompt worker called
`Agent::execute_provider_messages` directly and checked the cancellation token
only at two coarse points: before starting execution and after it completed.
There were no live session notifications during prompt execution, so Zed
received no feedback until the entire prompt finished. Phase 6 addresses all of
these limitations.

## Agent Execution Event Layer

### Design

The event layer lives in `src/agent/events.rs` and consists of three parts:

- `AgentExecutionEvent` -- a `Clone + Debug` enum describing every observable
  transition in the agent loop.
- `AgentObserver` -- a `Send`-bound trait with a single `on_event` method that
  is called synchronously during execution.
- `NoOpObserver` -- a zero-cost unit struct that satisfies `AgentObserver` by
  discarding every event.

### Events

| Variant                    | When it fires                                           |
| -------------------------- | ------------------------------------------------------- |
| `PromptStarted`            | Before the first iteration of the execution loop        |
| `ProviderRequestStarted`   | Before each call to the provider's `complete` method    |
| `ProviderResponseReceived` | After each completion response is received              |
| `AssistantTextEmitted`     | When the provider returns non-empty text content        |
| `ToolCallStarted`          | Before each individual tool call executes               |
| `ToolCallCompleted`        | After a tool call returns successfully                  |
| `ToolCallFailed`           | When a tool call returns an error                       |
| `VisionInputAttached`      | When multimodal image content is detected in messages   |
| `CancellationRequested`    | At each cancellation check point when the token fires   |
| `ExecutionCompleted`       | At the end of a successful loop with the final response |
| `ExecutionFailed`          | When the loop exits with an error                       |

### Backward Compatibility

The two existing public methods `Agent::execute` and
`Agent::execute_provider_messages` now delegate to their new `_with_observer`
counterparts using a fresh `CancellationToken::new()` and `NoOpObserver`. No
call sites changed. No behavior changed for existing consumers.

## Cancellation-Aware Execution

### New Methods

Two new methods were added to `Agent` in `src/agent/core.rs`:

- `execute_with_observer(user_prompt, cancellation_token, observer)`
- `execute_provider_messages_with_observer(messages, cancellation_token, observer)`

Both accept a `&CancellationToken` from `tokio_util::sync`.
`XzatomaError::Cancelled` was added as a new error variant to represent clean
cancellation outcomes. It maps to `acp::StopReason::Cancelled` at the protocol
layer.

### Check Points

The cancellation token is checked at every safe boundary:

- Before adding the user prompt to the conversation (pre-flight guard).
- At the top of each loop iteration before the iteration limit and timeout
  checks.
- Using `tokio::select!` to race each provider `complete` call against
  `cancellation_token.cancelled()`.
- Before each tool call.
- Using `tokio::select!` to race each `execute_tool_call` call against
  `cancellation_token.cancelled()`.

This means cancellation can interrupt a long provider call mid-flight but cannot
split a single tool call at an arbitrary byte boundary. A subprocess or external
tool that cannot be interrupted immediately will complete its current call
before the cancellation is observed. This limitation is inherent to the
synchronous tool executor contract and is documented in the method's doc
comments.

## Per-Session Prompt Queuing

### Queue Design

Each ACP stdio session owns an `mpsc::Sender<QueuedPrompt>` (stored in
`ActiveSessionState`) and a background worker task that reads from the
corresponding receiver. The queue is bounded by
`config.acp.stdio.prompt_queue_capacity`. If the queue is full, `enqueue_prompt`
returns an `acp_sdk::Error` describing the capacity limit immediately.

The worker processes prompts serially, holding the `Arc<Mutex<XzatomaAgent>>`
lock for the duration of each prompt. This preserves the conversation state
invariant: no two prompts can interleave within a single session.

### `QueuedPrompt` Structure

`QueuedPrompt` carries three fields:

- `messages: Vec<Message>` -- the multimodal input converted from ACP content
  blocks.
- `response_tx: oneshot::Sender<acp_sdk::Result<acp::PromptResponse>>` -- the
  one-shot channel through which the worker returns the prompt result to the
  waiting `enqueue_prompt` call.
- `connection: Option<ConnectionTo<AcpClientRole>>` -- the live ACP connection
  over which session notifications are sent during execution. `None` in test
  contexts where no real connection is available.

## Per-Prompt Cancellation Token Replacement

### Problem

A single session-level `CancellationToken` cannot support per-prompt
cancellation because cancelling it would also prevent future queued prompts from
running on the same session.

### Solution

The session stores
`current_cancellation_token: watch::Receiver<CancellationToken>`. A
`watch::Sender<CancellationToken>` is held exclusively by the prompt worker.

Before processing each queued prompt the worker:

1. Creates a fresh `CancellationToken`.
2. Publishes it via `token_tx.send(new_token.clone())`.
3. Executes the prompt with the fresh token.

The `CancelNotification` handler reads the latest token from the watch receiver
and calls `.cancel()`. This cancels only the prompt that is currently running.
Prompts that have not yet started will receive their own fresh token when they
begin.

## ACP Session Notifications

### `AcpSessionObserver`

The `AcpSessionObserver` struct implements `AgentObserver` and holds a clone of
the `ConnectionTo<AcpClientRole>` and the current session ID. It maps execution
events to ACP `SessionUpdate` variants and sends them as `SessionNotification`
messages over the live connection. Send errors are silently discarded -- a
dropped connection during prompt execution is not a fatal error.

### Event Mapping

| `AgentExecutionEvent`                     | `SessionUpdate`                                                  |
| ----------------------------------------- | ---------------------------------------------------------------- |
| `AssistantTextEmitted { text }`           | `AgentMessageChunk(ContentChunk::new(ContentBlock::from(text)))` |
| `ToolCallStarted { id, name, arguments }` | `ToolCall(build_tool_call_start(...))`                           |
| `ToolCallCompleted { id, output, .. }`    | `ToolCallUpdate(build_tool_call_completion(...))`                |
| `ToolCallFailed { id, error, .. }`        | `ToolCallUpdate(build_tool_call_failure(...))`                   |
| `ExecutionCompleted { response }`         | `AgentMessageChunk` only if no text was already streamed         |
| `VisionInputAttached { count }`           | Debug log only, no ACP update                                    |
| All other events                          | Silently ignored                                                 |

The `text_emitted` flag on `AcpSessionObserver` prevents double-emitting final
assistant text. If the provider streamed text incrementally (via multiple
`AssistantTextEmitted` events), the `ExecutionCompleted` handler does not send
an additional chunk.

## `CancelNotification` Handler

A new `on_receive_notification` handler for `acp::CancelNotification` was added
to `run_stdio_agent_with_transport` before the catch-all `on_receive_dispatch`
handler. When it fires:

1. It looks up the session in the `ActiveSessionRegistry` by session ID.
2. If found, it calls `current_cancellation_token()` on the session state to
   clone the current watch value, then calls `.cancel()` on it.
3. If not found (the session may have ended), it logs a debug message and
   returns. This is not treated as a fatal error.

Cancellation applies only to the currently running prompt. Queued prompts that
have not yet started will receive their own token and will not be affected.

## Stop Reason Mapping

XZatoma execution outcomes map to ACP `StopReason` values as follows:

| Outcome                               | `StopReason`            |
| ------------------------------------- | ----------------------- |
| Normal completion                     | `EndTurn`               |
| `XzatomaError::Cancelled`             | `Cancelled`             |
| `XzatomaError::MaxIterationsExceeded` | `MaxTurnRequests`       |
| Any other provider or tool error      | Protocol error response |

`MaxTurnRequests` is used for max-turns-exceeded because it is the closest
semantic match in the ACP schema. There is no generic error stop reason in the
ACP `StopReason` enum; unrecoverable errors that cannot be mapped to a stop
reason are returned as an `acp_sdk::Error` protocol error response.

A `map_error_to_stop_reason` helper function codifies this mapping and can be
extended as the ACP schema evolves.

## File Changes

### New Files

- `src/agent/events.rs` -- `AgentExecutionEvent` enum, `AgentObserver` trait,
  `NoOpObserver` struct.

### Modified Files

- `src/error.rs` -- Added `XzatomaError::Cancelled` variant.
- `src/agent/mod.rs` -- Added `pub mod events` and re-exported the new types.
- `src/agent/core.rs` -- Added `execute_with_observer`,
  `execute_provider_messages_with_observer`; refactored `execute` and
  `execute_provider_messages` to delegate.
- `src/acp/stdio.rs` -- Changed `current_cancellation_token` to
  `watch::Receiver<CancellationToken>`; added `connection` field to
  `QueuedPrompt`; modified `enqueue_prompt` signature; added
  `AcpSessionObserver`; added `CancelNotification` handler; replaced
  `run_prompt_worker` and `execute_queued_prompt` implementations; added
  `map_error_to_stop_reason`.

## Testing

Phase 6 adds the following tests:

- `agent::events` -- `test_no_op_observer_accepts_all_events`,
  `test_agent_execution_event_is_debug_clone`,
  `test_custom_observer_receives_events`.
- `agent::core` -- `test_execute_with_observer_emits_prompt_started`,
  `test_execute_with_observer_respects_cancellation`,
  `test_execute_provider_messages_with_observer_emits_prompt_started`.
- `error` -- `test_cancelled_error_display`.
- `acp::stdio` -- `test_cancel_notification_cancels_current_prompt_token`,
  `test_execute_queued_prompt_returns_cancelled_when_token_cancelled`,
  `test_prompt_request_returns_end_turn_over_protocol`,
  `test_queue_ordering_multiple_prompts_complete_in_order`,
  `test_stop_reason_cancelled_maps_correctly`.

## Known Limitations

- Tool calls that spawn subprocesses cannot be interrupted at an arbitrary
  point. Cancellation is observed only after the subprocess exits.
- The `CancelNotification` handler cancels the session's current prompt token.
  If the ACP schema introduces prompt-specific cancellation identifiers in a
  future revision, this implementation will need to correlate by prompt ID.
- Vision input attachment is logged but does not produce an ACP session
  notification. A future phase may add a `SessionInfoUpdate` or custom status
  chunk to surface this to the Zed client.
