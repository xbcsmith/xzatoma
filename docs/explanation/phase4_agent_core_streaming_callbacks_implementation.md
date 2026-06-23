# Phase 4: Agent Core Streaming Callbacks Implementation

## Overview

This document describes the changes made to `src/agent/core.rs` as part of Phase
4 of the ACP Features implementation. The goal is to route per-chunk streaming
events from providers that support live streaming to the `AgentObserver` in real
time, before the batch event processing that happens after the provider call
returns.

## Problem

Before this change, both `execute_with_observer` and
`execute_provider_messages_with_observer` called `provider.complete()` and then
emitted a single `ReasoningEmitted` event and a single `AssistantTextEmitted`
event after the full response was available. Providers that support incremental
streaming had no mechanism to push per-chunk events to the observer during
generation.

## Solution

Both functions were modified with the same three-part pattern.

### Part 1: Conditional streaming dispatch

After `ProviderRequestStarted` is emitted, the function checks two conditions:

- `self.provider.supports_streaming()` returns `true`.
- `!observer.is_noop()` is `true` (a real observer is listening).

When both are satisfied the function calls `provider.complete_with_callbacks()`
with two closures. Each closure appends `AgentExecutionEvent` variants to a
shared `Arc<Mutex<Vec<AgentExecutionEvent>>>` buffer:

- The reasoning closure pushes `ThinkingStarted` on the first chunk (via an
  `AtomicBool` guard), then pushes a `ReasoningChunkEmitted` for every chunk.
- The content closure pushes `ThinkingFinished` on the first content chunk if
  reasoning was active (via a second `AtomicBool`), then pushes
  `AssistantTextEmitted` for every content chunk.

When either condition is false the function falls back to `provider.complete()`.

Both paths are guarded by the same `tokio::select!` for cancellation.

### Part 2: Streaming event replay

After the provider call returns, the buffer is drained and each collected event
is forwarded to the observer in insertion order. Two boolean flags are derived
from the buffer before draining:

- `reasoning_was_streamed`: `true` if any `ReasoningChunkEmitted` event was
  collected.
- `content_was_streamed`: `true` if any `AssistantTextEmitted` event was
  collected.

### Part 3: Guarded batch events

The existing batch reasoning and content emission logic is now conditional:

- `ReasoningEmitted` is only emitted when `!reasoning_was_streamed`. For
  non-streaming providers the batch path now also wraps the `ReasoningEmitted`
  event with `ThinkingStarted` and `ThinkingFinished` so the Zed thinking panel
  opens for all providers, not just streaming ones.
- The batch `AssistantTextEmitted` is only emitted when `!content_was_streamed`.

This prevents double-delivery of content to the observer.

## Files Changed

- `src/agent/core.rs` - both `execute_with_observer` and
  `execute_provider_messages_with_observer` updated with the three-part pattern
  described above.

## New Test

`test_execute_with_observer_emits_reasoning_chunk_events_for_streaming_provider`
verifies the full streaming path end-to-end using a `MockStreamingProvider` that
implements `supports_streaming() -> true` and fires reasoning and content chunks
through `complete_with_callbacks`. The test asserts:

- `ThinkingStarted` is emitted before the first `ReasoningChunkEmitted`.
- Exactly two `ReasoningChunkEmitted` events are emitted (matching the two
  chunks supplied by the mock).
- `ThinkingFinished` is emitted.
- At least one `AssistantTextEmitted` event is emitted.

## Design Decisions

The streaming event buffer uses `Arc<Mutex<Vec<AgentExecutionEvent>>>` rather
than a channel because the callbacks are synchronous `Fn` closures invoked
inside the provider implementation. A channel would require the closures to be
`async` or would introduce unnecessary complexity. The lock cannot deadlock
because the closures are called sequentially within a single-threaded async task
and the lock is only held for the duration of a single `push` call; no await
point is crossed while holding it.

The `is_noop()` check avoids constructing the `Arc`, closures, and `AtomicBool`
guards when no real observer is present, which is the common case for CLI
invocations that use `NoOpObserver`.
