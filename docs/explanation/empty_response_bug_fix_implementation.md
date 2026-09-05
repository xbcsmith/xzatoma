# Empty Response Bug Fix Implementation

## Problem

When using local Ollama models (such as `gemma4:e4b-mlx`) in Zed ACP, the agent
silently returned an empty response to the IDE. The user would send a prompt and
receive no output -- no text, no tool calls, no error message.

## Root Cause

The bug existed in two symmetric locations inside `src/agent/core.rs`, in the
`execute_with_observer` and `execute_provider_messages_with_observer` methods.

After each provider call, the loop checked whether to treat the response as a
final answer using:

```rust
if message.content.is_some() {
    debug!("Provider returned final response, stopping");
    break;
}
```

`Option::is_some()` returns `true` for `Some("")` (an empty string). Local
Ollama models, including the `gemma4` family, periodically return
`content: Some("")` with no tool calls when the conversation context is large or
ambiguous. Because the check matched `Some("")`, the agent:

1. Broke out of the execution loop treating the empty string as success.
2. Added the empty assistant message to the in-memory conversation.
3. Returned `StopReason::EndTurn` to Zed, which then rendered nothing.
4. Persisted the poisoned conversation (containing the empty assistant turn) to
   disk on `EndTurn`.

On the next user prompt, the model saw accumulated stale empty assistant turns,
became more confused, and again returned `Some("")`. This created a self-
reinforcing failure cycle visible in the agent log as repeated user messages
interleaved with zero-character assistant messages:

```text
msg_index 7  role=assistant  msg_char_count=0
msg_index 8  role=user       msg_char_count=1845  (same prompt repeated)
msg_index 9  role=assistant  msg_char_count=0
...
```

## Fix

### `src/agent/core.rs`

Changed both occurrences of the stopping condition from:

```rust
if message.content.is_some() {
    debug!("Provider returned final response, stopping");
    break;
}
```

to:

```rust
if message.content.as_deref().is_some_and(|c| !c.is_empty()) {
    debug!("Provider returned final response, stopping");
    break;
}
```

When content is `Some("")`, the condition is now `false`. Execution falls
through to the existing error path, which returns `XzatomaError::Provider` with
a descriptive message. Because the error is not `Cancelled` or
`MaxIterationsExceeded`, `map_error_to_stop_reason` returns `None` and the ACP
layer surfaces a protocol error to Zed instead of a silent `EndTurn`. The
conversation is **not** persisted to disk when the stop reason is not `EndTurn`,
preventing propagation of the poisoned history across sessions.

The warning message was also updated from the misleading "Provider returned
neither content nor tool calls" to "Provider returned empty response with no
tool calls".

The `final_message` extraction at the end of both methods was tightened with an
additional `.filter(|c| !c.is_empty())` guard so that an empty assistant content
stored earlier in the turn never escapes as the "final" answer:

```rust
.find(|m| m.role == "assistant")
.and_then(|m| m.content.as_ref())
.filter(|c| !c.is_empty())       // added
.cloned()
.unwrap_or_else(|| "No response from assistant".to_string());
```

### New Test

Added `test_agent_handles_empty_string_content_response` in the `agent::core`
test module. The test supplies a `MockProvider` that returns
`content: Some(String::new())` with no tool calls and asserts that `execute`
returns `Err`, confirming `Some("")` is now rejected the same way `None` is.

## Relationship to Thinking-Tag Stripping

The `extract_thinking` function at the top of each iteration may transform
`Some("<think>...</think>")` into `Some("")` when a model returns only reasoning
tags and no visible content. The fix handles this case correctly as well: the
resulting empty string is treated as an invalid response, not a final answer.

## Limitations and Follow-up Work

- The empty assistant message is still added to the in-memory conversation
  before the error is detected (line ordering: `add_message`, then the content
  check). Within a single session, repeated empty responses accumulate in memory
  but are not persisted to disk. A follow-up improvement would be to remove the
  empty turn from the conversation when the error is returned, preventing any
  within-session compounding.
- The real cause of `gemma4` returning empty strings is often an oversized
  context. Users can mitigate this by using `/context summary` before submitting
  large multi-file prompts, or by switching to a model with a larger context
  window.
