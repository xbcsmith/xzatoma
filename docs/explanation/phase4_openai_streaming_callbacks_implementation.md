# Phase 4: OpenAI Streaming Callbacks Implementation

## Overview

This document describes the Phase 4 changes made to `src/providers/openai.rs` to
implement the `complete_with_callbacks` and `supports_streaming` methods on the
OpenAI provider, enabling per-chunk streaming callbacks for reasoning and
content tokens.

## Changes

### New private method: `post_completions_streaming_with_callbacks`

`post_completions_streaming_with_callbacks` replaces the body of
`post_completions_streaming` and adds two optional callback parameters:

- `on_reasoning_chunk` - invoked for each non-empty `delta.reasoning` fragment
- `on_content_chunk` - invoked for each non-empty `delta.content` fragment

Callbacks are fired before the chunk is handed to `StreamAccumulator`, so the
caller receives each token as it arrives rather than waiting for the full
response.

### Refactored `post_completions_streaming`

`post_completions_streaming` now delegates entirely to
`post_completions_streaming_with_callbacks(request, None, None)`. This
eliminates the duplicate SSE parsing loop and ensures both paths stay in sync
automatically.

### New trait override: `complete_with_callbacks`

`complete_with_callbacks` overrides the default no-op from the `Provider` trait.
Its routing logic is:

1. When `enable_streaming` is false, or tools are present, or neither callback
   is provided, it falls back to `complete` (non-streaming path).
2. Otherwise it constructs an `OpenAIRequest` with `stream: true` and calls
   `post_completions_streaming_with_callbacks` with the caller-supplied
   callbacks.

The condition requiring at least one callback to be non-`None` before taking the
streaming path ensures the provider does not switch to streaming silently when
both callbacks are `None` (which is the existing `complete` behaviour).

### New trait override: `supports_streaming`

`supports_streaming` reads `OpenAIConfig.enable_streaming` from the internal
`RwLock` and returns it. This lets the agent loop query whether streaming
callbacks are available without attempting a completion.

## Design decisions

- Callbacks use `&(dyn Fn(String) + Send + Sync)` references rather than owned
  closures to avoid requiring `'static` bounds and to match the trait signature.
- The streaming path is skipped when tools are present because tool-call deltas
  are accumulated across many chunks. The non-streaming path already handles
  this correctly and the callback overhead adds no value for tool calls.
- No new dependencies were added. The existing `futures::StreamExt` import
  inside the method body is sufficient.

## Tests added

| Test name                                                      | What it verifies                                                                                                                |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `test_complete_with_callbacks_fires_reasoning_chunk_callbacks` | `on_reasoning_chunk` is called once per non-empty `delta.reasoning` chunk (2 calls for 2 chunks)                                |
| `test_complete_with_callbacks_fires_content_chunk_callbacks`   | `on_content_chunk` is called once per non-empty `delta.content` chunk (3 calls) and the accumulated response content is correct |

Both tests use `wiremock` to serve a minimal SSE body and `AtomicUsize` to count
callback invocations without requiring `Mutex`.
