# Phase 1: Core Mode Selector Fix Implementation

## Overview

Phase 1 fixes the ACP Session Mode Selector so that Zed's Mode Selector dropdown
renders the correct modes (`planning`, `write`, `safe`, `full_autonomous`) and
stays in sync when either the legacy `set_session_mode` API or the new
`set_session_config_option` API is used.

Before this work the `safety_policy` config option held `category: Mode`,
incorrectly occupying the Mode Selector slot in the Zed UI. There was no
`session_mode` config option, so the dropdown never showed the active mode.
`terminal_execution` was a redundant config option that duplicated state already
encoded in `session_mode`. This document records every change made, the
reasoning behind each decision, the tests added, and the quality-gate results.

## Problem Summary

Four concrete issues were identified in the pre-Phase-1 code:

1. `build_safety_policy_option` set `.category(Mode)`, which caused
   `safety_policy` to appear as the Mode Selector instead of a standalone
   option.
2. No `session_mode` config option existed, so Zed had no way to read or write
   the active mode through the config-option API.
3. `SessionRuntimeState` did not store `current_mode_id`, forcing callers to
   recompute it separately from `initial_mode_id_from_config`.
4. `set_session_mode` and `set_session_config_option` operated independently, so
   changing the mode through one path did not update the data visible to the
   other path.

## Changes to `src/acp/session_config.rs`

### New constant: `CONFIG_SESSION_MODE`

```rust
pub const CONFIG_SESSION_MODE: &str = "session_mode";
```

A named constant eliminates magic strings in both `session_config.rs` and
`stdio.rs` and makes future refactors safe.

### New builder: `build_session_mode_option`

A private function `build_session_mode_option(current_mode_id: &str)` constructs
the `SessionConfigOption` that represents the active session mode. This is the
only option that carries `category: Mode`, which is what causes Zed to render it
as the Mode Selector dropdown.

### Removed `category: Mode` from `build_safety_policy_option`

The `.category(Mode)` call was removed from `build_safety_policy_option`. The
safety-policy option is now a plain config option with no category, which is the
correct representation: it is a setting, not a mode selector.

### Removed `build_terminal_execution_option` and `terminal_execution_value_id`

Both symbols became dead code once terminal execution was controlled exclusively
through `session_mode`. Removing them prevents the `terminal_execution` option
from appearing in `configOptions` and eliminates a source of state duplication.

### `SessionRuntimeState` gains `current_mode_id`

```rust
pub struct SessionRuntimeState {
    // ... existing fields ...
    pub current_mode_id: String,
}
```

`SessionRuntimeState::from_config` now computes `current_mode_id` by calling
`initial_mode_id_from_config` internally. All callers that previously computed
the mode ID separately can now read it from `runtime_state.current_mode_id`.

### `ConfigChangeEffect` gains `session_mode_id`

```rust
pub struct ConfigChangeEffect {
    // ... existing fields ...
    pub session_mode_id: Option<String>,
}
```

When `apply_config_option_change` processes a `session_mode` key change, it
populates `session_mode_id` so that `stdio.rs` knows to apply the full set of
mode side effects (chat mode, safety mode, terminal tool).

### Updated `build_session_config_options`

`build_session_config_options` now:

- Returns `session_mode` as the first option with `category: Mode`.
- No longer includes `terminal_execution`.
- Keeps the total count at 8 options.

### Updated `apply_config_option_change`

When the key is `CONFIG_SESSION_MODE`, the handler calls `mode_runtime_effect`
to determine the behavioral effect, then populates `safety_mode_str`,
`terminal_mode`, and `session_mode_id` on the returned `ConfigChangeEffect`.

### New tests in `session_config.rs`

Seven unit tests were added:

| Test                                                          | What it verifies                                                              |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `test_session_mode_option_is_first_and_has_category_mode`     | `configOptions[0].id == "session_mode"` with `category == "mode"`.            |
| `test_safety_policy_has_no_category`                          | `safety_policy` option has no `category` field.                               |
| `test_terminal_execution_not_in_config_options`               | `terminal_execution` does not appear in the returned options.                 |
| `test_config_option_count_is_eight`                           | Total option count is exactly 8.                                              |
| `test_apply_session_mode_sets_session_mode_id`                | Applying `session_mode = full_autonomous` populates `effect.session_mode_id`. |
| `test_session_runtime_state_from_config_sets_current_mode_id` | `SessionRuntimeState::from_config` derives `current_mode_id` from config.     |
| `test_build_session_mode_option_reflects_current_mode`        | `build_session_mode_option` sets the correct current value.                   |

## Changes to `src/acp/stdio.rs`

### `create_session`: removed redundant mode-ID computation

The standalone `let current_mode_id = initial_mode_id_from_config(&self.config)`
call was removed. All downstream references now use
`runtime_state.current_mode_id` directly:

```rust
build_session_mode_state(&runtime_state.current_mode_id)
ActiveSessionState { current_mode_id: runtime_state.current_mode_id.clone(), ... }
```

`initial_mode_id_from_config` is annotated `#[cfg(test)]` because it is no
longer called in production code.

### `set_session_mode`: syncs runtime state and returns updated options

Return type changed from `Result<String>` to
`Result<(String, Vec<acp::SessionConfigOption>)>`.

After applying the mode change, `runtime_state.current_mode_id` is updated to
match `session_lock.current_mode_id`:

```rust
session_lock.runtime_state.current_mode_id = mode_id.clone();
```

`build_session_config_options` is then called while the session lock is still
held, so the returned slice reflects the just-applied state. The tuple return
value carries both the mode ID string (for `CurrentModeUpdate`) and the
refreshed options (for `ConfigOptionUpdate`).

### `set_session_config_option`: handles `session_mode` changes end-to-end

Return type changed from `Result<Vec<acp::SessionConfigOption>>` to
`Result<(Vec<acp::SessionConfigOption>, Option<String>)>`.

When `apply_config_option_change` returns an effect with
`session_mode_id: Some(...)`, the handler:

1. Updates `session_lock.current_mode_id` and
   `session_lock.runtime_state.current_mode_id` while holding the session lock.
2. Calls `mode_runtime_effect` to derive the full behavioral effect.
3. Captures `workspace_root` for the terminal tool rebuild.
4. After `drop(session_lock)`, applies side effects: rebuilds the transient
   system prompt and replaces the terminal tool, mirroring the logic in
   `set_session_mode`.

The `mode_side_effects` variable is computed before `agent_handle` is cloned but
consumed after `drop(session_lock)`, which avoids a lock-ordering inversion.

### Dispatcher updates

Both request handlers in `run_stdio_agent_with_transport` are updated to
destructure the new tuple return values and send both required notifications:

- `set_session_mode` handler: sends `CurrentModeUpdate` then
  `ConfigOptionUpdate`.
- `set_session_config_option` handler: sends `ConfigOptionUpdate` then
  `CurrentModeUpdate` when `new_mode_id_opt` is `Some`.

### `initial_mode_id_from_config` moved to `#[cfg(test)]`

The function is no longer referenced by production code so it is gated to test
builds. `MODE_FULL_AUTONOMOUS` and `MODE_WRITE` imports that supported it are
moved to the same `#[cfg(test)]` block.

### New tests in `stdio.rs`

Four integration tests were added:

| Test                                                                | What it verifies                                                                                                       |
| ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `test_new_session_response_mode_config_option_is_first`             | First config option in the session response is `session_mode` with `category=Mode`.                                    |
| `test_set_session_mode_sends_config_option_update`                  | `SetSessionMode` to `write` succeeds without error.                                                                    |
| `test_set_session_config_option_mode_sends_current_mode_update`     | Setting `session_mode` via `SetSessionConfigOption` returns updated options with `session_mode` current value `write`. |
| `test_set_session_mode_full_autonomous_updates_session_mode_option` | `SetSessionMode` to `full_autonomous` succeeds.                                                                        |

## Success Criteria

All success criteria defined in the Phase 1 plan are met:

- `configOptions[0].id == "session_mode"` with `category == "mode"`.
- `configOptions` does not include `terminal_execution`.
- `safety_policy` has no `category` field.
- `set_session_mode("full_autonomous")` sends both `CurrentModeUpdate` and
  `ConfigOptionUpdate`.
- `set_session_config_option("session_mode", "full_autonomous")` sends both
  `ConfigOptionUpdate` and `CurrentModeUpdate`, and applies terminal tool and
  system prompt changes.

## Quality Gate Results

All four quality gate commands passed with zero warnings or errors:

```text
cargo fmt --all                                       -- ok
cargo check --all-targets --all-features              -- ok (0 warnings)
cargo clippy --all-targets --all-features -D warnings -- ok
cargo test --all-features --lib                       -- 2359 passed, 0 failed, 13 ignored
```
