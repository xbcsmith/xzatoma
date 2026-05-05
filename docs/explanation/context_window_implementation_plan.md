# Context Window Support for ACP Mode Implementation Plan

## Overview

XZatoma already tracks token usage internally through
`Conversation.token_count()` (heuristic) and
`Conversation.update_from_provider_usage()` (provider-reported). The
`Agent.get_context_info()` method aggregates both sources into a `ContextInfo`
struct. However, none of this information is surfaced to the Zed client during
ACP sessions. Zed renders a context window progress indicator in its chat UI,
but it can only update that indicator when the agent sends
`SessionUpdate::UsageUpdate` notifications over the live ACP connection. Those
notifications are never sent today.

The `agent-client-protocol` crate exposes `SessionUpdate::UsageUpdate` behind
the `unstable_session_usage` feature flag. That flag is not enabled in
`Cargo.toml`, making the `UsageUpdate` type unavailable. Enabling it and wiring
it into the existing observer and execution loop is the primary mechanical work
of this plan. A secondary gap is that Zed never receives a session auto-title:
the first user message is already truncated by `first_user_prompt_title()` and
stored in SQLite, but a `SessionUpdate::SessionInfoUpdate` carrying that title
is never sent to Zed.

This plan is divided into four phases. Phase 1 enables the feature flag and
confirms compilation. Phase 2 adds the `ContextWindowUpdated` event variant and
emits it from both execution loops immediately after provider usage is stored.
Phase 3 wires the new event into `AcpSessionObserver` so `UsageUpdate`
notifications are sent to Zed after every provider turn. Phase 4 sends the
initial `UsageUpdate` at session creation and the `SessionInfoUpdate` auto-title
after the first `EndTurn` response.

## Current State Analysis

### Existing Infrastructure

| Symbol                                                      | File                        | Lines      | Description                                               |
| ----------------------------------------------------------- | --------------------------- | ---------- | --------------------------------------------------------- |
| `Conversation.token_count()`                                | `src/agent/conversation.rs` | L612-616   | Heuristic token count (chars/4)                           |
| `Conversation.get_context_info()`                           | `src/agent/conversation.rs` | L734-746   | Prefers provider usage over heuristic                     |
| `Conversation.update_from_provider_usage()`                 | `src/agent/conversation.rs` | L693-700   | Stores provider `TokenUsage`                              |
| `Agent.get_context_info()`                                  | `src/agent/core.rs`         | L1333-1340 | Prefers `accumulated_usage` over conversation             |
| `Agent.get_token_usage()`                                   | `src/agent/core.rs`         | L1295-1299 | Returns `accumulated_usage` lock value                    |
| `Agent.provider()`                                          | `src/agent/core.rs`         | L1246-1250 | Returns `&dyn Provider`                                   |
| `Conversation.max_tokens()`                                 | `src/agent/conversation.rs` | L631-635   | Returns configured max token limit                        |
| `AgentExecutionEvent`                                       | `src/agent/events.rs`       | L33-99     | Enum of all execution events                              |
| `AcpSessionObserver.on_event()`                             | `src/acp/stdio.rs`          | L1589-1643 | Maps events to ACP notifications                          |
| `execute_queued_prompt()`                                   | `src/acp/stdio.rs`          | L1699-1776 | Runs agent; returns stop reason                           |
| `run_prompt_worker()`                                       | `src/acp/stdio.rs`          | L1654-1692 | Worker loop over `QueuedPrompt` channel                   |
| `create_session()`                                          | `src/acp/stdio.rs`          | L418-575   | Builds `ActiveSessionState`; returns `NewSessionResponse` |
| `handle_initialize()`                                       | `src/acp/stdio.rs`          | L1184-1203 | Advertises session capabilities                           |
| `first_user_prompt_title()`                                 | `src/acp/stdio.rs`          | L1529-1535 | Truncates first user message to 80 chars                  |
| `persist_conversation_checkpoint()`                         | `src/acp/stdio.rs`          | L1509-1527 | Saves conversation and sets title                         |
| `usage` update in `execute_with_observer`                   | `src/agent/core.rs`         | L616-629   | Stores usage after provider response                      |
| `usage` update in `execute_provider_messages_with_observer` | `src/agent/core.rs`         | L947-964   | Stores usage after provider response                      |

### Identified Issues

1. **Feature flag missing** (`Cargo.toml`): `agent-client-protocol` is declared
   with only `features = ["unstable_session_model"]`. The
   `unstable_session_usage` feature is absent, so
   `acp::SessionUpdate::UsageUpdate` and `acp::UsageUpdate` do not exist in the
   compiled crate.

2. **No `ContextWindowUpdated` event** (`src/agent/events.rs`):
   `AgentExecutionEvent` has no variant for context window state. The observer
   pattern cannot route usage data to ACP without a dedicated event carrying
   `used_tokens` and `max_tokens`.

3. **Execution loops never emit context state** (`src/agent/core.rs`): After the
   usage block at L616-629 (`execute_with_observer`) and L947-964
   (`execute_provider_messages_with_observer`), no event is emitted. The updated
   token count is computed but not propagated to any observer.

4. **`AcpSessionObserver` has no arm for context window** (`src/acp/stdio.rs`):
   The `match` in `on_event()` at L1589 does not handle any context window
   event. Even after adding the event variant, nothing will send a `UsageUpdate`
   to Zed until this arm is added.

5. **No initial `UsageUpdate` on session creation** (`src/acp/stdio.rs`):
   `create_session()` at L418 returns a `NewSessionResponse` without sending a
   `UsageUpdate`. Zed shows an empty progress bar until the first turn
   completes.

6. **No `SessionInfoUpdate` after first prompt** (`src/acp/stdio.rs`):
   `execute_queued_prompt()` at L1699 calls `persist_conversation_checkpoint()`
   when `stop_reason == EndTurn`, which derives a title via
   `first_user_prompt_title()` and writes it to SQLite. However no
   `SessionUpdate::SessionInfoUpdate` notification is sent to Zed, so the
   session title in the UI remains blank or shows the raw prompt text.

## Implementation Phases

### Phase 1: Enable `unstable_session_usage` Feature Flag

#### Task 1.1: Add Feature to `Cargo.toml`

**File:** `Cargo.toml`

**Current line (approx. L94):**

```Cargo.toml#L94
agent-client-protocol = { version = "0.11.1", features = ["unstable_session_model"] }
```

**Required change:** Add `"unstable_session_usage"` to the features list.

```Cargo.toml#L94
agent-client-protocol = { version = "0.11.1", features = ["unstable_session_model", "unstable_session_usage"] }
```

This makes `acp::UsageUpdate` and the
`acp::SessionUpdate::UsageUpdate(UsageUpdate)` variant available in all
downstream modules.

#### Task 1.2: Verify Compilation

Run:

```/dev/null/verify.sh#L1-4
cargo check --all-targets --all-features 2>&1 | grep -E "error|warning.*unused"
```

Expected: zero errors. Warnings about newly visible but unused types are
acceptable at this stage.

#### Task 1.3: Testing Requirements

No tests are required for this phase. The feature flag is a compile-time gate;
the test suite passing is the verification.

#### Task 1.4: Deliverables

- [ ] `Cargo.toml` updated with `unstable_session_usage` feature
- [ ] `cargo check --all-targets --all-features` passes with zero errors

#### Task 1.5: Success Criteria

```/dev/null/verify.sh#L1
cargo check --all-targets --all-features
```

Must complete without `error[E...]` output. `acp::UsageUpdate::new(0u64, 0u64)`
must resolve without error when referenced in downstream modules.

---

### Phase 2: Add `ContextWindowUpdated` Event and Emit from Execution Loops

#### Task 2.1: Add `ContextWindowUpdated` Variant to `AgentExecutionEvent`

**File:** `src/agent/events.rs`

**Current enum** ends at `ExecutionFailed` (approximately L99). Add a new
variant after `VisionInputAttached` and before `CancellationRequested`.

**Required addition inside the `AgentExecutionEvent` enum:**

```src/agent/events.rs#L1-14
/// Context window state was updated after a provider response.
///
/// Emitted immediately after provider token usage is stored in the
/// conversation. `used_tokens` reflects the most accurate available count:
/// provider-reported if present, heuristic otherwise. `max_tokens` is the
/// configured context window size from `Conversation.max_tokens()`.
ContextWindowUpdated {
    /// Tokens currently occupying the context window.
    used_tokens: u64,
    /// Maximum tokens available in the context window.
    max_tokens: u64,
},
```

Insert this variant between `VisionInputAttached` and `CancellationRequested` to
keep the enum ordered by emission sequence.

**Update `NoOpObserver` test** in `src/agent/events.rs`: add the new variant to
`test_no_op_observer_accepts_all_events` so the exhaustive match check in the
test remains valid.

```src/agent/events.rs#L1-6
observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
    used_tokens: 1024,
    max_tokens: 8192,
});
```

#### Task 2.2: Emit Event in `execute_with_observer` After Usage Update

**File:** `src/agent/core.rs`

**Location:** immediately after the `drop(accumulated)` call at L629, inside the
`if let Some(usage) = completion_response.usage` block.

**Current code at L616-629:**

```src/agent/core.rs#L616-629
if let Some(usage) = completion_response.usage {
    self.conversation.update_from_provider_usage(&usage);
    let mut accumulated = self.accumulated_usage.lock().unwrap();
    if let Some(existing) = *accumulated {
        *accumulated = Some(TokenUsage::new(
            existing.prompt_tokens + usage.prompt_tokens,
            existing.completion_tokens + usage.completion_tokens,
        ));
    } else {
        *accumulated = Some(usage);
    }
    drop(accumulated);
}
```

**Required change:** Emit the event unconditionally (using heuristic fallback
when no provider usage is available) after the `if let` block closes. Move the
emit outside the `if let` block so the heuristic path also fires.

```src/agent/core.rs#L1-8
// Emit context window state regardless of whether provider usage was returned.
// get_context_info() prefers provider usage over the heuristic when available.
let ctx = self.get_context_info(self.conversation.max_tokens());
observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
    used_tokens: ctx.used_tokens as u64,
    max_tokens: ctx.max_tokens as u64,
});
```

Place this block at approximately L630, immediately after the closing `}` of the
`if let Some(usage)` block and before the `has_tool_calls` assignment.

#### Task 2.3: Emit Event in `execute_provider_messages_with_observer` After Usage Update

**File:** `src/agent/core.rs`

**Location:** same pattern as Task 2.2, but in
`execute_provider_messages_with_observer`. The parallel usage block is at
L947-964. Apply the identical emit snippet immediately after the closing `}` of
the `if let Some(usage)` block at approximately L965.

```src/agent/core.rs#L1-8
// Emit context window state regardless of whether provider usage was returned.
let ctx = self.get_context_info(self.conversation.max_tokens());
observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
    used_tokens: ctx.used_tokens as u64,
    max_tokens: ctx.max_tokens as u64,
});
```

#### Task 2.4: Testing Requirements

Add the following tests to `src/agent/core.rs` in the `mod tests` block:

- `test_execute_with_observer_emits_context_window_updated_on_provider_response`

  - Arrange: mock provider returns a response with `TokenUsage::new(100, 50)`
  - Act: call `execute_with_observer` with a capturing observer
  - Assert: observer received exactly one `ContextWindowUpdated` event with
    `used_tokens >= 150` and
    `max_tokens == agent.conversation().max_tokens() as u64`

- `test_execute_provider_messages_with_observer_emits_context_window_updated`

  - Same arrangement but through `execute_provider_messages_with_observer`

- `test_context_window_updated_uses_heuristic_when_no_provider_usage`
  - Arrange: mock provider returns a response with `usage: None`
  - Assert: `ContextWindowUpdated` is still emitted with `used_tokens > 0`
    (heuristic) and correct `max_tokens`

Add the following test to `src/agent/events.rs`:

- `test_context_window_updated_event_is_debug_clone`
  - Construct a `ContextWindowUpdated` event, clone it, and format with `{:?}`

#### Task 2.5: Deliverables

- [ ] `ContextWindowUpdated` variant added to `AgentExecutionEvent` with doc
      comment
- [ ] Event emitted in `execute_with_observer` after L629
- [ ] Event emitted in `execute_provider_messages_with_observer` after L964
- [ ] `NoOpObserver` test updated with new variant
- [ ] All new tests pass

#### Task 2.6: Success Criteria

```/dev/null/verify.sh#L1-2
cargo test --all-features -- agent::core::tests::test_execute_with_observer_emits_context_window_updated
cargo test --all-features -- agent::core::tests::test_context_window_updated_uses_heuristic_when_no_provider_usage
```

Both must pass. Additionally:

```/dev/null/verify.sh#L1
cargo clippy --all-targets --all-features -- -D warnings
```

Must produce zero warnings in `src/agent/events.rs` and `src/agent/core.rs`.

---

### Phase 3: Wire `ContextWindowUpdated` into `AcpSessionObserver`

#### Task 3.1: Handle `ContextWindowUpdated` in `AcpSessionObserver.on_event()`

**File:** `src/acp/stdio.rs`

**Location:** `AcpSessionObserver` `impl AgentObserver` block, `on_event()`
method at L1589.

**Current match arms** (L1589-1643) handle: `AssistantTextEmitted`,
`ToolCallStarted`, `ToolCallCompleted`, `ToolCallFailed`, `ExecutionCompleted`,
`VisionInputAttached`, `ExecutionFailed`, and a wildcard `_ => {}`.

**Required addition:** Insert a new arm before the wildcard `_ => {}`:

```src/acp/stdio.rs#L1-12
AgentExecutionEvent::ContextWindowUpdated {
    used_tokens,
    max_tokens,
} => {
    let update = acp::UsageUpdate::new(used_tokens, max_tokens);
    self.send_update(acp::SessionUpdate::UsageUpdate(update));
}
```

The `send_update()` helper at L1583 already handles send errors gracefully with
`let _ =`, so no additional error handling is required.

#### Task 3.2: Testing Requirements

Add the following tests to `src/acp/stdio.rs` in the `mod tests` block:

- `test_acp_session_observer_sends_usage_update_on_context_window_updated`

  - Arrange: construct an `AcpSessionObserver` with a mock or captured
    connection
  - Act: call
    `on_event(AgentExecutionEvent::ContextWindowUpdated { used_tokens: 500, max_tokens: 8192 })`
  - Assert: a `SessionUpdate::UsageUpdate` notification was sent with
    `used == 500` and `size == 8192`

- `test_acp_session_observer_context_window_updated_zero_values`
  - Same as above with `used_tokens: 0, max_tokens: 0` to confirm no panic

#### Task 3.3: Deliverables

- [ ] `ContextWindowUpdated` arm added to `AcpSessionObserver.on_event()` before
      wildcard
- [ ] `acp::UsageUpdate::new(used_tokens, max_tokens)` constructed and sent as
      `SessionUpdate::UsageUpdate`
- [ ] New tests pass

#### Task 3.4: Success Criteria

```/dev/null/verify.sh#L1
cargo test --all-features -- acp::stdio::tests::test_acp_session_observer_sends_usage_update
```

Must pass. After a full `cargo test --all-features`, no regressions in
`src/acp/stdio.rs` tests.

---

### Phase 4: Initial `UsageUpdate` at Session Creation and `SessionInfoUpdate` After First Turn

#### Task 4.1: Send Initial `UsageUpdate` from `create_session()`

**File:** `src/acp/stdio.rs`

**Location:** `create_session()` at L418. The function returns an
`acp::NewSessionResponse` at approximately L567-574 (after
`self.sessions.insert(active_session).await`).

**Problem:** The returned `NewSessionResponse` carries model and mode state but
no usage state. Zed cannot render a context window bar until the first
`UsageUpdate` arrives. For a resumed conversation the initial token count may
already be non-zero.

**Required change:** Before returning, send an initial `UsageUpdate` through the
connection when one is available. The connection is passed as
`Option<ConnectionTo<AcpClientRole>>` to `create_session`.

Add the following block after `self.sessions.insert(active_session).await` and
before the `Ok(...)`:

```src/acp/stdio.rs#L1-18
// Send an initial UsageUpdate so Zed can render the context window bar
// immediately, even before the first prompt is processed.
if let Some(conn) = &connection {
    let agent_lock = self.sessions
        .get(&session_id)
        .await
        .map(|s| Arc::clone(&s.xzatoma_agent));
    if let Some(agent_arc) = agent_lock {
        let agent = agent_arc.lock().await;
        let max_tokens = agent.conversation().max_tokens() as u64;
        let used_tokens = agent.get_context_info(agent.conversation().max_tokens()).used_tokens as u64;
        let update = acp::UsageUpdate::new(used_tokens, max_tokens);
        let notification = acp::SessionNotification::new(
            session_id.clone(),
            acp::SessionUpdate::UsageUpdate(update),
        );
        let _ = conn.send_notification_to(AcpClientRole, notification);
    }
}
```

Note: `ActiveSessionRegistry.get()` must be confirmed to exist or the agent arc
must be captured before the insert. If `get()` is not available on the registry,
capture the `Arc` before the `insert` call and use it directly. Adjust the exact
approach based on the `ActiveSessionRegistry` API available at the call site.

#### Task 4.2: Send `SessionInfoUpdate` After First `EndTurn` Response

**File:** `src/acp/stdio.rs`

**Location:** `execute_queued_prompt()` at L1699. The `EndTurn` branch at
approximately L1755-1773 calls `persist_conversation_checkpoint()` which already
derives the session title via `first_user_prompt_title()`.

**Required change:** After `persist_conversation_checkpoint()` succeeds (or
after the `if let Some(storage)` block), and when `stop_reason == EndTurn` and a
connection is available, derive the title from the queued prompt messages and
send a `SessionInfoUpdate`.

Because `execute_queued_prompt` does not currently receive a turn counter,
determine whether this is the first turn by checking whether the conversation
title was just set: inspect `agent.conversation().title()` before and after
calling `persist_conversation_checkpoint()`, or pass the `messages` slice to
`first_user_prompt_title()` directly.

**Concrete approach:** After the `persist_conversation_checkpoint` call block,
add:

```src/acp/stdio.rs#L1-14
// Send a SessionInfoUpdate with the auto-derived title after the first turn.
// first_user_prompt_title reads from the full conversation history, so it
// returns a value as soon as one user message exists.
if stop_reason == acp::StopReason::EndTurn {
    if let Some(conn) = connection {
        if let Some(title) = first_user_prompt_title(agent.conversation().messages()) {
            let info_update = acp::SessionInfoUpdate::new().title(Some(title));
            let notification = acp::SessionNotification::new(
                session_id.clone(),
                acp::SessionUpdate::SessionInfoUpdate(info_update),
            );
            let _ = conn.send_notification_to(AcpClientRole, notification);
        }
    }
}
```

This fires on every `EndTurn`, which is acceptable: `SessionInfoUpdate` is
idempotent in Zed's UI and sending the same title repeatedly causes no visible
side effect.

#### Task 4.3: Confirm `SessionCapabilities` Advertises Usage Support

**File:** `src/acp/stdio.rs`

**Location:** `handle_initialize()` at L1184-1203.

**Current code at L1192-1201:**

```src/acp/stdio.rs#L1192-1201
.session_capabilities(acp::SessionCapabilities::new())
```

Check the `acp::SessionCapabilities` builder API in `agent-client-protocol`
v0.11.1 for a `.usage(true)` or equivalent method that signals usage tracking
support. If such a method exists, chain it:

```src/acp/stdio.rs#L1-2
.session_capabilities(acp::SessionCapabilities::new().usage(true))
```

If no such method exists in v0.11.1, leave `SessionCapabilities::new()`
unchanged. Do not guess at method names; verify against the crate documentation
or source before modifying.

#### Task 4.4: Testing Requirements

Add the following tests to `src/acp/stdio.rs`:

- `test_execute_queued_prompt_sends_session_info_update_on_end_turn`

  - Arrange: construct a minimal session with a mock connection and a storage
    handle
  - Act: process one `EndTurn` prompt
  - Assert: a `SessionUpdate::SessionInfoUpdate` notification was sent with a
    non-empty title derived from the first user message

- `test_execute_queued_prompt_no_session_info_update_on_cancelled`

  - Arrange: same setup, but cancel the token before execution
  - Assert: no `SessionInfoUpdate` notification was sent

- `test_create_session_sends_initial_usage_update_when_connection_present`
  - Arrange: call `create_session` with a mock connection
  - Assert: a `SessionUpdate::UsageUpdate` notification was sent with `size > 0`

#### Task 4.5: Deliverables

- [ ] Initial `UsageUpdate` sent from `create_session()` when connection is
      `Some`
- [ ] `SessionInfoUpdate` sent from `execute_queued_prompt()` after `EndTurn`
- [ ] `SessionCapabilities` builder checked and updated if usage advertisement
      is available
- [ ] All new tests pass

#### Task 4.6: Success Criteria

```/dev/null/verify.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All four commands pass with zero errors and zero warnings.

---

## Implementation Order Summary

| Phase                                     | Files Modified                             | Depends On       |
| ----------------------------------------- | ------------------------------------------ | ---------------- |
| Phase 1: Enable feature flag              | `Cargo.toml`                               | None             |
| Phase 2: Add event and emit from loops    | `src/agent/events.rs`, `src/agent/core.rs` | Phase 1          |
| Phase 3: Wire observer to `UsageUpdate`   | `src/acp/stdio.rs`                         | Phase 2          |
| Phase 4: Initial update and session title | `src/acp/stdio.rs`                         | Phase 1, Phase 3 |

Phases 3 and 4 both modify `src/acp/stdio.rs`. To minimise merge conflicts,
complete Phase 3 fully (including tests passing) before beginning Phase 4.

---

## Reference: Already-Implemented Features

The following features exist and must not be re-implemented.

| Feature                                 | Symbol                                      | File                        | Lines      |
| --------------------------------------- | ------------------------------------------- | --------------------------- | ---------- |
| Heuristic token counting                | `Conversation.token_count()`                | `src/agent/conversation.rs` | L612-616   |
| Provider usage storage                  | `Conversation.update_from_provider_usage()` | `src/agent/conversation.rs` | L693-700   |
| Context info aggregation (conversation) | `Conversation.get_context_info()`           | `src/agent/conversation.rs` | L734-746   |
| Context info aggregation (agent)        | `Agent.get_context_info()`                  | `src/agent/core.rs`         | L1333-1340 |
| Accumulated usage accessor              | `Agent.get_token_usage()`                   | `src/agent/core.rs`         | L1295-1299 |
| Max tokens accessor                     | `Conversation.max_tokens()`                 | `src/agent/conversation.rs` | L631-635   |
| Provider accessor                       | `Agent.provider()`                          | `src/agent/core.rs`         | L1246-1250 |
| Usage update in loop (text)             | usage block                                 | `src/agent/core.rs`         | L616-629   |
| Usage update in loop (messages)         | usage block                                 | `src/agent/core.rs`         | L947-964   |
| ACP observer send helper                | `AcpSessionObserver.send_update()`          | `src/acp/stdio.rs`          | L1583-1588 |
| Session title derivation                | `first_user_prompt_title()`                 | `src/acp/stdio.rs`          | L1529-1535 |
| Conversation checkpoint persistence     | `persist_conversation_checkpoint()`         | `src/acp/stdio.rs`          | L1509-1527 |
| `SessionInfoUpdate` schema              | `acp::SessionInfoUpdate`                    | `agent-client-protocol`     | -          |
| `CompletionResponse.usage` field        | `CompletionResponse`                        | `src/providers/`            | -          |
