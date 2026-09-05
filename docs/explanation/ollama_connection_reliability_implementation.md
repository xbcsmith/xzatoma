# Ollama Connection Reliability Fixes

## Background

Three classes of error were observed in Zed ACP when using the Ollama provider.
All three have now been addressed.

---

## Error 1: Connection-level failures ("error sending request")

```text
Internal error: "prompt execution failed: Provider HTTP request failed:
provider=ollama, endpoint=api/chat:stream: error sending request for url
(http://localhost:11434/api/chat)"
```

### Root cause A: Ollama not running

TCP connection refused. Fix: `ollama serve`.

### Root cause B: IPv6 / IPv4 dual-stack (primary cause when Ollama is running)

On macOS and Linux `localhost` resolves to both `::1` (IPv6) and `127.0.0.1`
(IPv4). Ollama binds to `127.0.0.1` only (`lsof -iTCP:11434` confirms this).
reqwest's Happy Eyeballs tries `::1` first; when a pooled connection to
`127.0.0.1` goes stale, the POST fails because hyper does not retry
non-idempotent requests.

### Root cause C: Stale connection-pool entry (any host)

Same failure when the connection ages past Ollama's keep-alive timeout before
`pool_idle_timeout(90s)` evicts it.

### Fixes applied

**Fix 1 — `normalize_localhost_to_ipv4` in `OllamaProvider::new`**: rewrites
`http://localhost:…` to `http://127.0.0.1:…` after URL validation. Eliminates
the DNS dual-stack lookup so reqwest never tries `::1`.

**Fix 2 — `send_post_with_retry`**: retries a POST exactly once when
`is_connect() || is_request()` (and not a timeout), discarding the broken pool
entry so the second attempt opens a fresh connection. Used in both `complete`
and `complete_streaming_with_callbacks`.

---

## Error 2: Mid-stream body failure (partial responses)

```text
Provider error: Error reading Ollama stream: error decoding response body
```

Ollama was OOM-killed, restarted, or the OS reset the TCP connection after
tokens had already been produced.

### Fix applied

**Fix 3 — partial-content recovery**: the streaming loop uses a labeled
`'stream` loop. If `stream.next()` returns a body error:

- With content accumulated → `break 'stream`, return partial response.
- With no content → propagate error with OOM / context-window guidance.

---

## Error 3: Stream stall / first-chunk failure ("error decoding response body"

before any content)

```text
Provider error: Ollama stream failed before any content arrived:
error decoding response body
```

Observed when prompting with large YAML files. Ollama accepts the connection,
sends `200 OK` headers, then either:

- **Stalls**: spends a long time filling the KV cache for a large context, never
  sending bytes (would hang forever under the old code).
- **OOM-crashes**: the OS kills the process before a single token is produced,
  leaving a TCP RST that reqwest translates to "error decoding response body".

Confirmed with live Ollama: `localhost:11434` is IPv4-only; the models installed
(7.6 GB – 19 GB) consume all available unified memory on a large context.

### Fix applied

**Fix 4 — per-chunk idle timeout (`stream_idle_timeout_seconds`)**: each
`stream.next()` call is wrapped in `tokio::time::timeout(idle_duration, …)`.

- **Timeout fires** (stall, no bytes within N seconds):
  - With content: `break 'stream`, return partial.
  - Without content: return
    `"Ollama stream produced no output within Ns — the prompt may exceed the model's context window …"`.
- **Body error** (OOM crash):
  - With content: `break 'stream`, return partial.
  - Without content: return
    `"Ollama stream failed before generating any content … Try a shorter prompt or a model with a larger context window"`.

**Fix 5 — `OllamaStreamError` parsing**: Ollama sends `{"error":"…"}` as a
streaming JSON chunk for fatal errors (context window exceeded, model not found,
etc.). The parse-error arm now checks for this structure and returns the actual
Ollama error message rather than silently swallowing the chunk.

### New config field

`OllamaConfig.stream_idle_timeout_seconds` (default `120`). Increase for very
large models (19 GB+) where initial KV-cache loading takes more than two
minutes. Set via `XZATOMA_OLLAMA_STREAM_IDLE_TIMEOUT` env var.

---

## Error 4: Empty response from native-thinking models (Gemma 4)

```text
Internal error: "prompt execution failed: Provider error: Provider returned
empty response (no content or tool calls)"
```

Observed with `satgeze/gemma4-12b-uncensored-1.5m:latest` and similar
native-thinking Gemma 4 variants from Ollama. The Ollama logs show the model
producing thousands of eval tokens (3110 tokens in one observed case), but
xzatoma reported an empty response.

### Root cause

Gemma 4 separates chain-of-thought from response at the wire level. Each
streaming chunk from `/api/chat` looks like:

```json
{
  "message": {
    "role": "assistant",
    "content": "",
    "thinking": "let me think …"
  },
  "done": false
}
```

Reasoning tokens are placed in `message.thinking`; `message.content` is empty
for every thinking chunk. The final response token (if any) arrives in
`message.content` with `message.thinking` empty.

The old streaming loop deserialized each chunk as `OllamaResponse`, which
aliases to `ProviderMessage`. `ProviderMessage` has no `thinking` field, so all
82+ thinking chunks were silently discarded. When a complex task (large YAML
reorganization) consumed the entire token budget on reasoning, `content_acc`
stayed empty and the agent raised the "empty response" error.

There was also a secondary bug in the response-assembly code:

```rust
// BUG: content_acc is always used even when empty and reasoning_acc is not
let message = if content_acc.is_empty() && reasoning_acc.is_empty() {
    Message::assistant("")
} else {
    Message::assistant(&content_acc) // still empty for native-thinking models
};
```

### Fix applied

**Fix 6 — `OllamaStreamMessage` and `OllamaStreamChunk` types**: a dedicated
stream-chunk type with a `thinking` field is used in the streaming parse loop.
`ProviderMessage` / `OllamaMessage` are unchanged (they are also used for OpenAI
and Copilot, so adding `thinking` would pollute those paths).

Thinking tokens are now routed to `reasoning_acc` alongside `<think>`-tag
reasoning (DeepSeek-R1 / Qwen3 style), and content tokens continue through the
existing `process_ollama_think_chunk` state machine.

**Fix 7 — reasoning-to-content promotion**: when the stream completes with
`content_acc` empty but `reasoning_acc` non-empty (Gemma 4 reasoning-only
responses), the accumulated reasoning is promoted to `final_content` and
`final_reasoning` is cleared. This prevents:

1. The "empty response" error in `agent/core.rs`.
2. The Zed UI rendering the same text twice (once as a reasoning block, once as
   a response).

```rust
let (final_content, final_reasoning) = if !content_acc.is_empty() {
    (content_acc, reasoning_acc) // normal path: content wins
} else if !reasoning_acc.is_empty() {
    (reasoning_acc, String::new()) // Gemma 4 path: reasoning promoted
} else {
    (String::new(), String::new()) // truly empty response
};
```

---

## Regression: Tool calls silently dropped in ACP streaming mode

Tool calling stopped working in Zed ACP sessions after commit `12b5009` ("feat:
thinking stream in acp mode") set `supports_streaming() -> true` on
`OllamaProvider`.

### Root cause

With `supports_streaming() == true`, the agent's execution loop sets
`use_streaming_callbacks = true` whenever a real ACP connection is present. This
routes every provider call through `complete_with_callbacks`, which calls
`complete_streaming_with_callbacks`. That function accumulated `message.content`
from each chunk but **never read `message.tool_calls`**. The tool calls emitted
by the model were deserialized into a dead field and silently discarded. The
final response was always `Message::assistant("")`, which the agent treated as
an empty response.

Before `12b5009`, `supports_streaming` used the trait default (`false`), so the
agent always called `complete()` directly. `complete()` passes the full response
through `convert_response_message()`, which correctly routes `tool_calls` to
`Message::assistant_with_tools()`.

The Zed Agent Harness (which talks directly to Ollama, not through xzatoma) was
unaffected, which confirmed the issue was inside xzatoma's streaming pipeline
rather than the model or Ollama.

### Fix applied

**Fix 8 -- tool call collection in `complete_streaming_with_callbacks`**:
`OllamaStreamMessage._tool_calls` was renamed to `tool_calls` so the field is
actually readable. A `tool_calls_acc: Vec<OllamaToolCall>` accumulator collects
tool calls from every stream chunk (both `done: false` and `done: true` -- some
Ollama versions place tool calls on the done chunk).

When the stream ends, if any tool calls were collected they take priority over
accumulated content. The calls are converted with the same id-generation logic
as `convert_response_message()` and returned as
`Message::assistant_with_tools()`:

```rust
if !tool_calls_acc.is_empty() {
    let converted = tool_calls_acc.into_iter().enumerate().map(|(idx, tc)| ToolCall {
        id: if tc.id.is_empty() { format!("call_{}_{}", ...) } else { tc.id },
        function: FunctionCall {
            name: tc.function.name,
            arguments: serde_json::to_string(&tc.function.arguments)?,
        },
    }).collect();
    return Ok(CompletionResponse::new(Message::assistant_with_tools(converted)));
}
```

Three new tests cover: tool calls on a `done: false` chunk, tool calls on a
`done: true` chunk, and content-only chunks having an empty `tool_calls` vec.

---

## Files changed

- `src/config.rs` — added `stream_idle_timeout_seconds` to `OllamaConfig` with
  default 120, env-var override, and tests.
- `src/providers/ollama.rs` — `normalize_localhost_to_ipv4`,
  `send_post_with_retry`, `OllamaStreamError`, partial-content recovery,
  per-chunk idle timeout, Ollama error JSON detection, `OllamaStreamMessage`,
  `OllamaStreamChunk`, native-thinking field routing, reasoning-to-content
  promotion, streaming tool-call collection (regression fix); updated tests.
- `src/agent/core.rs` -- diagnostic `warn!` when the model returns text on turn
  1 without calling any registered tools.
