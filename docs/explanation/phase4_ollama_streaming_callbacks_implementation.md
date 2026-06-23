# Phase 4: Ollama Streaming Callbacks Implementation

## Overview

This document explains the changes made to `src/providers/ollama.rs` as part of
Phase 4 of the XZatoma ACP Features work. The goal was to override
`complete_with_callbacks` so that Ollama streams responses token-by-token to
callers rather than buffering the entire reply before returning.

## What Changed

### `supports_streaming()` override

The `OllamaProvider` now overrides `supports_streaming()` to return `true`. This
lets the `Agent` loop in `agent/core.rs` opt into the live streaming path when
the observer is active.

The `ProviderCapabilities.supports_streaming` field is left `false`
intentionally. That field represents a static capability declaration used by
tooling; the trait method `supports_streaming()` is the runtime gate used by the
agent loop.

### `complete_streaming_with_callbacks` (private method)

Added to `impl OllamaProvider`. It sends the Ollama `/api/chat` request with
`"stream": true`, consumes the newline-delimited JSON byte stream, and routes
each incremental `message.content` delta through a state machine that detects
think tags.

Key design decisions:

- **Byte-level line buffering.** The HTTP stream is chunked at the TCP level,
  not at JSON-object boundaries. A local `line_buf: Vec<u8>` accumulates bytes
  until a newline is received, then the complete line is parsed as a single
  `OllamaResponse`.
- **`ProviderMessage.content` is `String`, not `Option<String>`.** The
  `OllamaMessage` type alias maps to `ProviderMessage`, which uses
  `#[serde(default)]` for `content`. An empty string in a streaming chunk is
  treated as a no-content frame and skipped.
- **`done: true` terminates the inner loop.** When the final chunk has
  `done: true`, the token counts are captured and the inner `for byte in chunk`
  loop breaks. The outer `while let` loop continues until the stream closes,
  which happens immediately after the done frame in normal Ollama operation.

### `process_ollama_think_chunk` (free function)

A module-level helper that implements the think-tag state machine. It is kept
separate from the streaming method to make unit testing straightforward.

Supported tag pairs:

| Open tag         | Close tag         | Purpose                 |
| ---------------- | ----------------- | ----------------------- |
| `<think>`        | `</think>`        | DeepSeek-R1 and similar |
| `<\|thinking\|>` | `<\|/thinking\|>` | Qwen3 and similar       |

Text before the first open tag and after each close tag is routed to
`on_content_chunk` and accumulated in `content_acc`. Text between an open/close
pair is routed to `on_reasoning_chunk` and accumulated in `reasoning_acc`.

The state machine handles tags split across multiple streaming chunks because
`in_think_block` is passed by mutable reference between calls.

### `complete_with_callbacks` override

Added to `impl Provider for OllamaProvider`. When at least one callback is
`Some`, it delegates to `complete_streaming_with_callbacks`. When both callbacks
are `None`, it falls back to `complete` to avoid the overhead of streaming for
callers that do not need incremental output.

## Tests Added

Three unit tests exercise `process_ollama_think_chunk` directly:

- `test_process_ollama_think_chunk_routes_content_outside_think_tags` - verifies
  plain text is accumulated in `content_acc` and delivered to the content
  callback.
- `test_process_ollama_think_chunk_routes_reasoning_inside_think_tags` -
  verifies a complete `<think>...</think>` block in a single chunk routes
  correctly.
- `test_process_ollama_think_chunk_spans_multiple_calls` - verifies that
  `in_think_block` state persists across calls, covering the common case where
  the model emits the open tag in one chunk and the content in subsequent
  chunks.

The test closures capture a `Mutex<Vec<String>>` rather than a `Vec<String>`
directly, because the callback parameter is `&dyn Fn(String) + Send + Sync`. A
closure that mutates a plain `Vec` only implements `FnMut`, not `Fn`; routing
mutation through `Mutex::lock()` satisfies the `Fn` bound.

## Quality Gate Results

All four quality gates passed:

```text
cargo fmt --all              # clean
cargo check --all-targets --all-features  # clean
cargo clippy --all-targets --all-features -- -D warnings  # clean
cargo test --all-features --lib -- providers::ollama  # 32 passed, 0 failed
```
