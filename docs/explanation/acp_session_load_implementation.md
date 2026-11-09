# ACP `session/load` Implementation

## Overview

This document explains the implementation of the ACP `session/load` protocol
handler in `src/acp/stdio.rs`. The `session/load` method allows Zed to
reconstruct the visible chat thread with prior conversation history when
reconnecting to a previously stored session.

## Background: `session/resume` vs `session/load`

The ACP protocol defines two session-restore methods:

| Method           | Restores context | Streams history back | Zed use case                      |
| ---------------- | ---------------- | -------------------- | --------------------------------- |
| `session/resume` | Yes              | No                   | Silent reconnect after restart    |
| `session/load`   | Yes              | Yes                  | Populate chat thread with history |

`session/resume` (implemented earlier) silently rehydrates the agent's context
without sending prior messages to Zed. `session/load` goes further: after
restoring context it iterates over all non-system conversation turns and streams
each one back as a `UserMessageChunk` or `AgentMessageChunk` notification, so
the Zed chat UI is populated with the prior thread.

## Changes Made

### 1. Capability advertisement (`handle_initialize`)

`acp::AgentCapabilities::load_session` changed from `false` to `true` so that
Zed knows it can call `session/load` to obtain history replay.

### 2. `handle_load_session` method on `AcpStdioServerState`

The method follows the same two-phase pattern as `resume_session`:

1. **Rehydrate** - delegates to `create_session` with the caller-supplied
   `session_id` as an override so the registry stores it under the key Zed will
   use for all subsequent requests.
2. **Replay** - when a live connection is present, locks the freshly created
   agent, clones the full `Conversation::messages()` slice, then sends each
   non-system turn as a `SessionUpdate::UserMessageChunk` or
   `SessionUpdate::AgentMessageChunk` notification via
   `send_session_update_best_effort`. System, tool-call, and tool-result
   messages are silently skipped because Zed has no UI slot for them.

Each chunk carries a `message_id` of the form `hist-{idx}` (zero-indexed
position in the stored message list) so chunks from the same logical message
share the same identifier and Zed can group them correctly.

### 3. Handler registration (`run_stdio_agent_with_transport`)

A new `on_receive_request` block is added immediately after the `session/resume`
handler. On success it:

- Responds with the `LoadSessionResponse` (contains current mode and config
  state).
- Sends an `AvailableCommandsUpdate` notification so Zed's slash-command
  completion menu is populated after load, mirroring the post-resume behavior.

On failure it responds with an `acp_internal_error`.

### 4. Test

`test_handle_load_session_creates_session_and_returns_response` calls
`handle_load_session` with `connection = None` (no live transport) against a
fresh workspace. It asserts:

- The call returns `Ok`.
- The session is registered in `AcpStdioServerState::sessions` after the call.

The test is marked `#[ignore = "requires system keyring"]` because
`handle_load_session` calls `create_session`, which calls
`create_provider_with_override`. With the default Copilot provider that function
attempts device-code authentication over the network, following the same
constraint as all other `create_session` tests in the module.

## Key Design Choices

- **Re-uses `create_session`** rather than duplicating session-construction
  logic. This keeps the rehydration path (conversation restore, MCP setup,
  provider construction, mode init) in a single place.
- **`send_session_update_best_effort`** is used for history notifications: a
  failed notification must not abort the load response. Zed will degrade
  gracefully to an empty thread rather than seeing an error.
- **No duplicate system messages** - the `match msg.role.as_str()` guard skips
  `"system"`, `"tool"`, and `"tool_result"` roles, keeping the replayed history
  clean for display in the Zed UI.
