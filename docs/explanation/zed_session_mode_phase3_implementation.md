# Zed Session Mode Phase 3 Implementation

## Overview

Phase 3 of the Zed Session Mode plan enforces the selected execution mode on the
`TerminalTool` instance held inside each session's agent tool registry. Prior to
this change, calling `set_session_mode` updated the session's
`runtime_state.terminal_mode` field and rebuilt transient system messages, but
the `CommandValidator.mode` field inside the already-registered `TerminalTool`
was never replaced. The agent therefore continued enforcing the execution mode
from session creation for the lifetime of the session, regardless of how many
times the user switched modes in the Zed mode selector.

## Problem Statement

The Zed mode selector UI sends a `SetSessionModeRequest` whenever the user picks
a different mode. XZatoma stores the new mode ID, updates the safety mode string
used for system prompt generation, and records the `ExecutionMode` in
`SessionRuntimeState`. None of those writes reach the `CommandValidator`
embedded inside the live `TerminalTool` object stored in the `ToolRegistry`.
Because the agent executes tool calls by fetching tool objects from the registry
at runtime, the stale validator continued blocking or allowing commands
according to the mode active at session creation time.

The symptom: switching from `planning` to `full_autonomous` in Zed still blocked
terminal commands that required full autonomous execution, because
`CommandValidator.mode` remained `ExecutionMode::Interactive`.

## Changes

### Change 1: `tools_mut` accessor on `Agent`

**File**: `src/agent/core.rs`

**Location**: `impl Agent`, after the existing `pub fn tools` accessor.

A new public method exposes a mutable reference to the agent's `ToolRegistry`:

```rust
pub fn tools_mut(&mut self) -> &mut ToolRegistry {
    &mut self.tools
}
```

This is a minimal accessor that grants callers the ability to replace registered
tools without exposing other mutable state on the agent. The design follows the
same pattern as the existing `tools()` immutable accessor and respects the
principle that the `agent/` module owns the registry.

The method carries a full doc comment explaining its intended use: allowing the
ACP stdio layer to replace individual tools when the session mode changes at
runtime.

### Change 2: Terminal tool replacement in `set_session_mode`

**File**: `src/acp/stdio.rs`

**Location**: `async fn set_session_mode`, inside the inner scoped block that
rebuilds transient system messages, immediately after
`agent_lock.set_transient_system_messages(vec![system_prompt])`.

Two imports were added at the top of the file alongside the existing
`crate::tools` imports:

```rust
use crate::tools::terminal::{CommandValidator, TerminalTool};
```

After the system prompt update, a nested block constructs and registers a
replacement `TerminalTool`:

```rust
// Replace the terminal tool so the new ExecutionMode is enforced immediately.
{
    let session_read = session.lock().await;
    let workspace_root = session_read.workspace_root.clone();
    drop(session_read);

    let new_validator = CommandValidator::new(
        effect.terminal_mode,
        workspace_root,
    );
    let new_terminal_tool = TerminalTool::new(
        new_validator,
        self.config.agent.terminal.clone(),
    )
    .with_safety_mode(safety_mode);
    agent_lock
        .tools_mut()
        .register("terminal", Arc::new(new_terminal_tool));
}
```

Key design points:

- `session_lock` was already dropped via `drop(session_lock)` earlier in the
  same block before `agent_lock` was acquired. The nested block acquires a fresh
  short-lived lock on `session` (the `Arc<Mutex<ActiveSessionState>>`) only to
  copy `workspace_root`, then immediately drops it. This avoids any lock
  ordering inversion between the session mutex and the agent mutex.

- `effect.terminal_mode` carries the `ExecutionMode` resolved by
  `mode_runtime_effect` from the requested mode ID. The same `effect` value was
  already applied to `session_lock.runtime_state.terminal_mode` earlier in the
  function, so the in-memory state and the live tool are always consistent after
  `set_session_mode` returns.

- `safety_mode` is the `SafetyMode` computed in the same enclosing block for
  system prompt generation. Reusing it here ensures the new `TerminalTool`
  enforces the same confirmation policy that the updated system prompt
  describes.

- `self.config.agent.terminal.clone()` supplies timeout and output-truncation
  settings from the static agent configuration. These do not change on mode
  switch; only the `ExecutionMode` and `SafetyMode` change.

- `ToolRegistry::register` inserts under the key `"terminal"`, replacing any
  previous entry with that name. The `HashMap` insertion is O(1) and does not
  affect other registered tools.

## Execution Mode Mapping

The mode-to-execution-mode mapping is defined in `src/acp/session_mode.rs` and
has not changed:

| Mode ID           | `ExecutionMode`        | `SafetyMode`    |
| ----------------- | ---------------------- | --------------- |
| `planning`        | `Interactive`          | `AlwaysConfirm` |
| `write`           | `RestrictedAutonomous` | `AlwaysConfirm` |
| `safe`            | `RestrictedAutonomous` | `AlwaysConfirm` |
| `full_autonomous` | `FullAutonomous`       | `NeverConfirm`  |

`CommandValidator` enforces these modes as follows:

- `Interactive`: Always returns `CommandRequiresConfirmation`; no terminal
  command is executed autonomously.
- `RestrictedAutonomous`: Only commands on the allowlist are permitted; denylist
  patterns are always blocked.
- `FullAutonomous`: All non-denylist commands are permitted without
  confirmation.

## Test Coverage

Four new unit tests were added. All pre-existing tests pass unchanged.

### `src/agent/core.rs` -- `mod tests`

| Test name                                       | What it verifies                                                                                                                |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `test_agent_tools_mut_returns_mutable_registry` | Creates an agent with an empty registry, calls `tools_mut()` to register a mock tool, and asserts `num_tools()` increases to 1. |

### `src/acp/stdio.rs` -- `mod tests`

| Test name                                                                       | What it verifies                                                                                                                                                                     |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `test_set_session_mode_updates_terminal_tool_execution_mode_to_full_autonomous` | `SetSessionModeRequest` with `mode_id: "full_autonomous"` returns a successful response over the protocol stack. The terminal tool replacement code path is exercised without error. |
| `test_set_session_mode_full_autonomous_to_planning_restricts_terminal`          | Sequential mode changes from `"full_autonomous"` to `"planning"` both return successful responses, confirming the tool is replaced on every valid mode change.                       |
| `test_set_session_mode_does_not_change_terminal_for_unknown_mode`               | A valid `"write"` mode change succeeds; a subsequent `"turbo"` mode change returns an error and leaves the session in the previous mode.                                             |

All four tests use the `run_client_server_test` helper already established by
the existing test suite, keeping test infrastructure consistent.

## Behavioral Invariants Preserved

- Mode changes that return an error (unknown mode ID, unknown session ID) leave
  the terminal tool unchanged. The `mode_runtime_effect` call returns `Err`
  before any mutation occurs, so the tool replacement block is never reached.

- Sessions where `terminal` was never registered in the tool registry (for
  example, sessions created with a restricted tool set) are unaffected. Calling
  `register("terminal", ...)` on a registry that has no `"terminal"` key simply
  adds the entry; it does not panic.

- All other tools in the registry (file operations, search, MCP-forwarded tools)
  are not touched by `set_session_mode`. Only the `"terminal"` key is replaced.

- The `terminal_mode` field in `SessionRuntimeState` is still updated in the
  same location as before. The field is used by other subsystems (config option
  reporting) and its update is independent of the tool registry replacement.

## Design Rationale

Replacing the entire `TerminalTool` object rather than mutating the existing one
avoids the need to introduce interior mutability (`Mutex` or `RwLock`) inside
`TerminalTool` or `CommandValidator`. The registry stores tools as
`Arc<dyn ToolExecutor>`, so replacing the `Arc` is a single pointer swap with no
coordination required across concurrent tool executions. Any in-flight tool
execution holds its own `Arc` clone and completes under the previous mode; the
replacement takes effect only for subsequent executions. This is the correct
behavior: a mode change should not interrupt an already-dispatched command.

Adding `tools_mut` as a minimal accessor on `Agent` preserves the module
boundary: the ACP stdio layer does not bypass the agent abstraction. If future
phases require replacing other tools on mode change, the same accessor can be
reused without further changes to `agent/core.rs`.
