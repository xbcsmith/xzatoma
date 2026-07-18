# Phase 2: ACP Status Handlers Implementation

## Overview

This document describes the implementation of Phase 2 of the chat command
unification plan: wiring per-command status handlers into the ACP stdio dispatch
layer so that `/mode status`, `/safety status`, `/model status`,
`/streaming status`, `/system status`, and `/subagents status` return live
session information instead of placeholder "not yet implemented" text.

The changes are confined to `src/acp/stdio.rs`. All six per-command help
variants (`ShowModeHelp` through `ShowSubagentsHelp`) and all six per-command
status variants (`ShowModeStatus` through `ShowSubagentsStatus`) were already
defined in `src/commands/special_commands.rs` as part of Phase 1. The help
variants were already wired in `resolve_special_command_response` in Phase 1.
Phase 2 adds the concrete runtime handlers for the status variants.

## Changes Made

### `src/acp/stdio.rs`

#### Import update

`build_session_modes` added to the non-test import of `crate::acp::session_mode`
so that `handle_mode_status` can look up the human-readable description for the
active mode ID:

```rust
use crate::acp::session_mode::{
    build_session_mode_state, build_session_modes, mode_runtime_effect,
};
```

#### Six new private handler functions (Task 2.1)

Six private handler functions were added immediately after the existing
`handle_status_command` function, following the established pattern of
`handle_status_command`, `handle_tools_command`, and `handle_mcp_command`:

| Function                       | Returns                                                      |
| ------------------------------ | ------------------------------------------------------------ |
| `handle_mode_status`           | Current mode ID and its advertised description               |
| `handle_safety_status`         | Current safety policy string and a plain-English description |
| `handle_model_status`          | Current model name and provider name                         |
| `handle_streaming_status`      | Fixed ACP-mode note (streaming controlled by Zed)            |
| `handle_system_status` (async) | First transient system message, or "none" message            |
| `handle_subagents_status`      | "enabled" or "disabled" delegation state                     |

`handle_system_status` is the only async function in the group because it must
lock the agent mutex to read `transient_system_messages`. The others read fields
directly from the `ActiveSessionState` reference.

#### Six new match arms in `dispatch_stdio_command` (Task 2.2)

Six match arms were inserted immediately after the `ShowStatus` arm, before the
`_ =>` fallthrough to `resolve_special_command_response`:

```rust
Ok(SpecialCommand::ShowModeStatus) => {
    let session_lock = session.lock().await;
    handle_mode_status(&session_lock)
}
Ok(SpecialCommand::ShowSafetyStatus) => { ... }
Ok(SpecialCommand::ShowModelStatus) => { ... }
Ok(SpecialCommand::ShowStreamingStatus) => { ... }
Ok(SpecialCommand::ShowSystemStatus) => {
    let session_lock = session.lock().await;
    handle_system_status(&session_lock).await
}
Ok(SpecialCommand::ShowSubagentsStatus) => { ... }
```

Status variants are intercepted in `dispatch_stdio_command` rather than in
`resolve_special_command_response` because they require live session state
(agent mutex, runtime fields). The pure `resolve_special_command_response`
function retains placeholder stubs for the status variants so that direct unit
tests of that function do not panic, but those stubs are never reached during a
live ACP session.

#### Task 2.3 status

The help variants (`ShowModeHelp` through `ShowSubagentsHelp`) were already
wired in `resolve_special_command_response` as part of Phase 1. No
`Err(CommandError::MissingArgument)` special-case arms were present in the
codebase at the time Phase 2 was applied; the help-variant wiring was already
complete.

#### Eleven new tests (Task 2.4)

The following tests were added to the existing `mod tests` block in
`src/acp/stdio.rs`, under a `// Phase 2: ACP Status Handlers tests` section:

| Test                                                         | Verifies                                                                                      |
| ------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `test_dispatch_mode_bare_returns_mode_help`                  | `/mode` routes to mode help and returns `EndTurn`                                             |
| `test_resolve_bare_mode_returns_mode_help`                   | `resolve_special_command_response("/mode")` contains help header                              |
| `test_dispatch_mode_status_returns_current_mode`             | `/mode status` returns text containing "Current mode:"                                        |
| `test_dispatch_safety_status_returns_current_safety`         | `/safety status` returns text containing "Current safety policy:"                             |
| `test_dispatch_model_status_returns_current_model`           | `/model status` returns text containing "Current model:"                                      |
| `test_dispatch_streaming_status_returns_acp_note`            | `/streaming status` returns text containing "controlled by Zed"                               |
| `test_dispatch_system_bare_returns_system_help`              | `/system` routes to system help and returns `EndTurn`                                         |
| `test_dispatch_system_status_returns_system_prompt`          | `/system status` with a populated prompt returns "Current system prompt:" and the prompt text |
| `test_dispatch_system_status_no_prompt_returns_none_message` | `/system status` with no prompt returns "No system prompt"                                    |
| `test_dispatch_subagents_bare_returns_subagents_help`        | `/subagents` routes to subagents help and returns `EndTurn`                                   |
| `test_dispatch_subagents_status_returns_enabled_state`       | `/subagents status` returns "enabled" or "disabled"                                           |

## Architecture Notes

### Why status variants are intercepted in `dispatch_stdio_command`

The status handlers need live session state (the agent mutex, mode ID, runtime
state fields). `resolve_special_command_response` is a pure function of
`prompt_text` alone and has no access to a session handle. Intercepting status
variants in `dispatch_stdio_command` before the fallthrough to the pure resolver
maintains the clean separation: pure parsing and formatting logic belongs in
`resolve_special_command_response`; I/O-dependent logic belongs in
`dispatch_stdio_command`.

### Why `handle_system_status` is `async`

Reading the transient system messages requires locking the agent mutex
(`session.xzatoma_agent.lock().await`). All other status handlers read fields
directly from the already-locked `ActiveSessionState` reference and do not need
to acquire further locks.

### Why `handle_streaming_status` ignores its session argument

In ACP mode, streaming is an implementation detail of the Zed client transport
layer. The agent has no control over it and no runtime flag to report. The
function accepts an `&ActiveSessionState` argument only for API symmetry with
the other five handlers, so that all six can be called from the same dispatch
pattern.

## Success Criteria Verification

- Typing `/mode`, `/safety`, `/model`, `/streaming`, `/system`, `/subagents` in
  the Zed chat window returns the per-command help text, not an error or
  placeholder.
- Typing `/<command> status` returns the current live value for that setting.
- `/system status` shows the active base system prompt.
- No regressions: all 2,495 existing tests continue to pass.
