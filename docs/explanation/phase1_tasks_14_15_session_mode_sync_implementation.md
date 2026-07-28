# Phase 1 Tasks 1.4 and 1.5: Session Mode Sync Implementation

## Overview

Tasks 1.4 and 1.5 ensure that the `session_mode` config option and the active
session mode are kept in sync across all paths that change them. After these
changes, Zed receives accurate `ConfigOptionUpdate` and `CurrentModeUpdate`
notifications whenever either `SetSessionMode` or `SetSessionConfigOption` (with
`session_mode` as the key) is called.

## Task 1.4: Simplify `create_session` Mode Initialization

### Problem

`create_session` previously computed the initial mode ID twice:

```rust
let current_mode_id = initial_mode_id_from_config(&self.config);
let runtime_state = SessionRuntimeState::from_config(&self.config);
```

`SessionRuntimeState::from_config` already computes `current_mode_id` internally
(as `runtime_state.current_mode_id`), so the standalone variable was redundant
and a maintenance hazard.

### Change

The standalone `let current_mode_id = initial_mode_id_from_config(...)` line is
removed. All downstream references are updated to use
`runtime_state.current_mode_id`:

- `build_session_mode_state(&runtime_state.current_mode_id)` — session mode
  state for `NewSessionResponse.modes`
- `ActiveSessionState { current_mode_id: runtime_state.current_mode_id.clone(), ... }`
  — in-memory session state field

`initial_mode_id_from_config` is annotated `#[cfg(test)]` since it is now only
referenced by unit tests.

## Task 1.5a: `set_session_mode` Returns Updated Config Options

### Change

Return type changed from `Result<String>` to
`Result<(String, Vec<acp::SessionConfigOption>)>`.

After updating `session_lock.current_mode_id`, `safety_mode_str`, and
`terminal_mode`, `runtime_state.current_mode_id` is also synced:

```rust
session_lock.runtime_state.current_mode_id = mode_id.clone();
```

Then `build_session_config_options` is called while the session lock is still
held so the returned options reflect the just-applied state:

```rust
let updated_options = build_session_config_options(&session_lock.runtime_state);
```

The dispatcher (`run_stdio_agent_with_transport`) now sends two notifications on
a successful mode change: a `CurrentModeUpdate` (existing) and a new
`ConfigOptionUpdate` carrying the refreshed options. This keeps the Zed
session-mode config-option picker in sync with the mode selector.

## Task 1.5b: `set_session_config_option` Handles `session_mode` Changes

### Change

Return type changed from `Result<Vec<acp::SessionConfigOption>>` to
`Result<(Vec<acp::SessionConfigOption>, Option<String>)>`.

When `apply_config_option_change` returns an effect with
`session_mode_id: Some(...)`, the handler now:

1. Updates `session_lock.current_mode_id` and
   `session_lock.runtime_state.current_mode_id` from the effect while the
   session lock is held.
2. Calls `mode_runtime_effect` to obtain the full behavioral effect (chat mode,
   safety mode, terminal execution mode) for the new session mode.
3. Captures `workspace_root` for the terminal tool rebuild.
4. After `drop(session_lock)`, applies the side effects — rebuilds the transient
   system prompt and replaces the terminal tool — mirroring the logic in
   `set_session_mode`.

The new `mode_side_effects` variable is computed before `agent_handle` is
cloned, but used after `drop(session_lock)`, avoiding a lock-ordering inversion.

## Task 1.5c: Dispatcher Updates

Both request handlers in `run_stdio_agent_with_transport` are updated to
destructure the new tuple return values and send the appropriate additional
notification:

- `set_session_mode` handler: sends `ConfigOptionUpdate` after
  `CurrentModeUpdate`.
- `set_session_config_option` handler: sends `CurrentModeUpdate` when
  `new_mode_id_opt` is `Some`, i.e., when the `session_mode` config key was
  changed.

## Import and Dead-Code Cleanup

- `CONFIG_SESSION_MODE` is imported from `session_config` and used in the debug
  log emitted inside the `mode_side_effects` branch of
  `set_session_config_option`.
- `MODE_FULL_AUTONOMOUS` and `MODE_WRITE` are moved to a `#[cfg(test)]` import
  since they are only referenced by `initial_mode_id_from_config`, which itself
  became test-only in Task 1.4.

## New Tests (Task 1.6 for stdio.rs)

Four integration tests are added to the `mod tests` block in `stdio.rs`:

| Test                                                                | What it verifies                                                                                                       |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `test_new_session_response_mode_config_option_is_first`             | First config option is `session_mode` with `category=Mode`.                                                            |
| `test_set_session_mode_sends_config_option_update`                  | `SetSessionMode` to `write` succeeds without error.                                                                    |
| `test_set_session_config_option_mode_sends_current_mode_update`     | Setting `session_mode` via `SetSessionConfigOption` returns updated options with `session_mode` current value `write`. |
| `test_set_session_mode_full_autonomous_updates_session_mode_option` | `SetSessionMode` to `full_autonomous` succeeds.                                                                        |

## Quality Gate Results

All four quality gate commands passed with zero warnings or errors:

```text
cargo fmt --all                                      -- ok
cargo check --all-targets --all-features             -- ok (0 warnings)
cargo clippy --all-targets --all-features -D warnings -- ok
cargo test --all-features --lib                      -- 2359 passed, 0 failed, 13 ignored
```
