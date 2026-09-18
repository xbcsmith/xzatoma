# ACP Session Resume Implementation

## Problem

When Zed restarts and reconnects to a workspace where xzatoma was previously
running as an ACP agent, users saw:

```text
Loading or resuming sessions is not supported by this agent.
```

This blocked conversation continuity across Zed restarts.

## Root Cause

Two gaps in the `session/resume` protocol path:

### 1. Missing capability advertisement

`handle_initialize` built `AgentCapabilities` with:

```rust
.session_capabilities(acp::SessionCapabilities::new())
```

`SessionCapabilities::new()` creates an empty struct — all capability sub-fields
default to `None`. Zed reads `session_capabilities.resume` to decide whether to
attempt `session/resume`. Because it was `None`, Zed showed the error message
immediately on reconnect without sending any request.

### 2. No `session/resume` handler

Even if Zed had sent a `ResumeSessionRequest` (`session/resume` method), the
transport had no handler for it. Unhandled messages fall through to
`.on_receive_dispatch` which returns `method_not_found`.

## Fix

### `handle_initialize`

Added `session/resume` to the advertised session capabilities:

```rust
.session_capabilities(
    acp::SessionCapabilities::new()
        .resume(acp::SessionResumeCapabilities::new()),
)
```

### `create_session` signature change

Added an optional `session_id_override: Option<acp::SessionId>` parameter. When
the caller supplies a session ID (the resume path), the existing session ID from
Zed is preserved. All existing callers pass `None` and receive the previous
auto-generated UUID behavior unchanged.

### New `resume_session` method

Added `AcpStdioServerState::resume_session` which handles two cases:

**Session still alive** (same process, session not yet evicted):

Reads the current `runtime_state` from the live session and immediately returns
`ResumeSessionResponse` with the active mode state and config options. No new
provider is created.

**Session not found** (process restarted or session evicted):

Calls `create_session` passing:

- The `cwd` from the resume request as the workspace root (so
  `load_resumable_conversation` can look up conversation history in storage)
- `session_id_override = Some(request.session_id)` to ensure the session is
  stored under the ID Zed expects

After creation, reads the mode/config state from the newly registered session
and returns it in the `ResumeSessionResponse`.

### `run_stdio_agent_with_transport` handler

Registered a new `on_receive_request` handler for `acp::ResumeSessionRequest`
that calls `resume_session` and sends an `AvailableCommandsUpdate` notification
after the session is ready, consistent with the `session/new` path.

## Behavior After Fix

| Scenario                                              | Before      | After                                       |
| ----------------------------------------------------- | ----------- | ------------------------------------------- |
| Zed reconnects, session still alive                   | Error shown | Instantly resumed with current state        |
| Zed reconnects, process restarted, history in storage | Error shown | Conversation rehydrated from storage        |
| Zed reconnects, process restarted, no history         | Error shown | New session created transparently           |
| `persist_sessions: false`                             | Error shown | New session created (no history to restore) |

## Files Changed

- `src/acp/stdio.rs`
  - `handle_initialize`: added `session/resume` capability
  - `AcpStdioServerState::create_session`: added `session_id_override` param
  - `AcpStdioServerState::resume_session`: new method
  - `run_stdio_agent_with_transport`: new `ResumeSessionRequest` handler
  - `test_handle_initialize_advertises_text_and_vision_prompt_capabilities`:
    added assertion for `session_capabilities.resume`
