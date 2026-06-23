# Phase 5 and Phase 6: Streaming Implementation

## Overview

Phases 5 and 6 add two orthogonal streaming features to XZatoma:

- **Phase 5** fixes a correctness bug where SSE streams could hang indefinitely
  by adding a per-chunk idle timeout to the OpenAI provider.
- **Phase 6** adds visible streaming output for chat, run, and agent commands,
  letting tokens appear in the terminal as the model generates them.

## Phase 5: Streaming Idle Timeout

### Problem

The OpenAI provider previously used a single `reqwest` client with a
`timeout = request_timeout_seconds` (default 600 s). This timeout is a
_total-request_ timeout: it fires only after the entire request (including the
full response body) takes longer than 600 s. For SSE streams, the body is
consumed incrementally, so the timeout never fired on a connection that
established successfully but then silently stalled mid-stream.

### Solution

Two changes were made:

1. A second `Client` (`streaming_client`) is built in `OpenAIProvider::new` with
   only a `connect_timeout` of 10 s and no total timeout. The streaming POST
   request uses this client so that long model responses are never cut off by a
   wall-clock total timeout.

2. Every `stream.next().await` call inside
   `post_completions_streaming_with_callbacks` is wrapped with
   `tokio::time::timeout(idle_duration, ...)`. If no SSE chunk arrives within
   `stream_idle_timeout_seconds` (default 30 s), the provider returns an error
   with message `"OpenAI SSE stream idle timeout: no data received for Ns"`.

### Configuration

| Field                                         | Default | Env var                              |
| --------------------------------------------- | ------- | ------------------------------------ |
| `provider.openai.stream_idle_timeout_seconds` | `30`    | `XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT` |

### Files Changed

- `src/config.rs` - new `stream_idle_timeout_seconds` field on `OpenAIConfig`
- `src/providers/openai.rs` - `streaming_client` field on `OpenAIProvider`;
  idle-timeout loop in `post_completions_streaming_with_callbacks`

## Phase 6: Streaming Token Display in Chat Mode

### Overview

Phase 6 lets users see model output tokens as they stream in, rather than
waiting for the full response. Three entry points support streaming: `chat`,
`run`, and `agent`.

### CLI Flag

Pass `--streaming` to any of the three commands:

```bash
xzatoma chat --streaming
xzatoma run --prompt "summarize this file" --streaming
xzatoma agent --streaming    # accepted but no-op for ACP mode
```

### Runtime Toggle

Inside an interactive chat session, use the `/streaming` special command:

```text
/streaming on     # enable streaming for subsequent prompts
/streaming off    # revert to post-complete print
/streaming enable # alias for on
/streaming disable # alias for off
```

### Streaming Observer

The `ChatStreamingObserver` struct implements `AgentObserver`. It handles:

| Event                            | Behaviour                                                         |
| -------------------------------- | ----------------------------------------------------------------- |
| `ThinkingStarted`                | Prints `"\nThinking...\n"` and sets `thinking_active = true`      |
| `ReasoningChunkEmitted { text }` | Prints `text` to stdout with flush                                |
| `ThinkingFinished`               | Prints a newline, clears `thinking_active`                        |
| `AssistantTextEmitted { text }`  | Closes thinking block if open, prints `text` to stdout with flush |
| All other events                 | Silently ignored                                                  |

After `execute_with_observer` returns, the caller checks
`observer.streamed_any_content()`. When `true`, the final `println!` of the full
response is suppressed to avoid double-printing.

### Chat Mode State

`ChatModeState` gained a `streaming_enabled: bool` field and a `set_streaming`
method. The initial value comes from the `--streaming` CLI flag. The
`/streaming` special command calls `set_streaming` at runtime.

### Files Changed

- `src/config.rs` - `stream_idle_timeout_seconds` on `OpenAIConfig`
- `src/providers/openai.rs` - `streaming_client`, idle-timeout loop
- `src/cli.rs` - `streaming: bool` on `Chat`, `Run`, `Agent`
- `src/commands/special_commands.rs` - `ToggleStreaming(bool)` variant,
  `/streaming` parsing
- `src/chat_mode.rs` - `streaming_enabled` field, `set_streaming` method
- `src/commands/mod.rs` - `ChatStreamingObserver`, updated `run_chat`, updated
  `run_plan_with_options`
- `src/commands/agent.rs` - `_streaming: bool` parameter on `handle_agent`
- `src/main.rs` - `streaming` extracted and forwarded from each CLI arm

## Design Decisions

### Why a separate `streaming_client`?

Removing the total timeout from the single client would affect non-streaming
requests too, potentially allowing non-streaming completions to run
indefinitely. Keeping two clients provides precise control: non-streaming calls
retain their 600 s hard limit; streaming calls have only a connect timeout plus
a per-chunk idle check.

### Why `tokio::time::timeout` per chunk?

`reqwest` read timeouts apply to the entire response body, not to individual
chunks. Only a per-chunk application-level timeout can detect a stream that has
stalled after sending some chunks.

### Why default 30 s?

30 s is long enough for any reasonable model inference step but short enough to
surface problems before the user gives up. It is configurable via
`XZATOMA_OPENAI_STREAM_IDLE_TIMEOUT`.

### Why `bool` for `--streaming`?

A boolean flag is the simplest API for an opt-in feature. Users who want
streaming pass `--streaming`; users who do not want it omit it. An enum would
add complexity without benefit at this stage.

### Why default `--streaming` to `false`?

Streaming changes terminal output behaviour visibly. Defaulting to `false`
preserves backward compatibility: existing scripts that parse or display output
are unaffected unless the user explicitly opts in.

### Why suppress the final response print when `streamed_any_content()` is true?

The final response string produced by `execute_with_observer` is the same text
that was already written to stdout chunk-by-chunk. Printing it again would
produce duplicated output. The `streamed_any_content()` flag lets the caller
make this decision without coupling the observer to the output logic.
