# Phase 5: ACP Session Config, Observer Wiring, and Zed Integration Implementation

## Overview

Phase 5 exposes the `thinking_effort` setting in Zed's config panel, wires the
`ReasoningEmitted` event through `AcpSessionObserver` so Zed receives chain-of-
thought content as `AgentThoughtChunk` notifications, and calls
`Provider::set_thinking_effort` whenever the user changes the config panel
option at runtime.

After Phase 5, a Zed user can:

- Select a reasoning effort level (None / Low / Medium / High / Extra High) from
  the XZatoma config panel without restarting the agent.
- See model reasoning/thinking output appear in a dedicated thought panel.

## Changes

### Task 5.1: `CONFIG_THINKING_EFFORT` Constant

Added to `src/acp/session_config.rs` after `CONFIG_MAX_TURNS`:

```src/acp/session_config.rs#L64-74
pub const CONFIG_THINKING_EFFORT: &str = "thinking_effort";
```

The module-level doc table was also updated to document the new option.

### Task 5.2: `thinking_effort` Field on `SessionRuntimeState`

Added `thinking_effort: String` as the last field of `SessionRuntimeState`. The
type is `String` (not `Option<String>`) because the config panel always shows a
selected value; the sentinel `"none"` means "no explicit effort — use model
default".

`SessionRuntimeState::from_config` initializes it to `"none".to_string()` since
the base `Config` has no top-level thinking effort setting.

### Task 5.3: `thinking_effort` Field on `ConfigChangeEffect`

Added `thinking_effort: Option<String>` as the last field of
`ConfigChangeEffect`. `ConfigChangeEffect::none()` initializes it to `None`.

The doc comment documents the sentinel semantics: when `Some("none")`, callers
must pass `None` to `Provider::set_thinking_effort`.

### Task 5.4: `build_thinking_effort_option` and Builder Wiring

Added the private function `build_thinking_effort_option` that returns a select
option with five choices: `"none"` / `"low"` / `"medium"` / `"high"` /
`"extra_high"`. The currently selected value is taken from
`runtime.thinking_effort`.

`build_session_config_options` now calls it as the eighth entry, making the
returned `Vec` length 8. The doc-comment example assertion and all affected
existing tests were updated from `len() == 7` to `len() == 8`.

### Task 5.5: `thinking_effort` Arm in `apply_config_option_change`

Added:

```src/acp/session_config.rs#L446-453
CONFIG_THINKING_EFFORT => {
    let effort = parse_thinking_effort_value(value_id)?;
    effect.thinking_effort = Some(effort.clone());
    updated.thinking_effort = effort;
}
```

Added the private validator
`parse_thinking_effort_value(value_id: &str) -> Result<String>` that accepts
`"none"`, `"low"`, `"medium"`, `"high"`, `"extra_high"` and rejects anything
else with a descriptive error.

The `other =>` error arm was updated to include `CONFIG_THINKING_EFFORT` in the
known-IDs list.

### Task 5.6: `ReasoningEmitted` in `AcpSessionObserver`

Added a new match arm before `ExecutionCompleted` in
`AcpSessionObserver::on_event`:

```src/acp/stdio.rs#L1858-1862
AgentExecutionEvent::ReasoningEmitted { text } => {
    let chunk = acp::ContentChunk::new(acp::ContentBlock::from(text));
    self.send_update(acp::SessionUpdate::AgentThoughtChunk(chunk));
}
```

This forwards reasoning content (stripped inline thinking-tag text and/or raw
provider reasoning fields) to Zed as `AgentThoughtChunk` notifications, which
Zed renders in its dedicated thought panel.

### Task 5.7: `ConfigChangeEffect.thinking_effort` to `set_thinking_effort`

`set_session_config_option` was extended to apply the thinking effort change to
the live provider. The pattern follows the lock-ordering discipline already
established by `set_session_mode`:

1. While holding the session lock, update `runtime_state.thinking_effort`.
2. Clone the `Arc<Mutex<XzatomaAgent>>` handle and the effort string.
3. Update `last_activity`, log, and explicitly `drop(session_lock)`.
4. Lock the agent and call `provider().set_thinking_effort(effort_opt)`.

This ordering prevents a lock-ordering inversion with the prompt worker (which
holds the agent lock but never the session lock while holding it).

The `"none"` sentinel is converted to `None` before the call so the provider
clears any previously set effort parameter.

## Tests Added

### `src/acp/session_config.rs` (8 new tests)

| Test                                                                    | Verifies                                              |
| ----------------------------------------------------------------------- | ----------------------------------------------------- |
| `test_session_runtime_state_thinking_effort_defaults_none`              | `from_config` sets `thinking_effort` to `"none"`      |
| `test_build_session_config_options_returns_eight_options`               | `build_session_config_options` returns 8 options      |
| `test_build_session_config_options_ids_include_thinking_effort`         | `CONFIG_THINKING_EFFORT` is present in option IDs     |
| `test_apply_config_option_change_thinking_effort_high`                  | `"high"` sets effect and returns 8 options            |
| `test_apply_config_option_change_thinking_effort_none`                  | `"none"` sets effect to `Some("none")`                |
| `test_apply_config_option_change_thinking_effort_extra_high`            | `"extra_high"` round-trips correctly                  |
| `test_apply_config_option_change_invalid_thinking_effort_returns_error` | unknown value returns error with bad value in message |
| `test_parse_thinking_effort_value_rejects_unknown`                      | direct validator rejects unknown                      |

### Existing tests updated

- `test_build_session_config_options_returns_seven_options` renamed to
  `test_build_session_config_options_returns_eight_options`
- `test_build_session_config_options_ids_are_stable` extended with
  `CONFIG_THINKING_EFFORT` assertion
- `test_apply_config_option_change_safety_policy_never_confirm`: `len()` 7 -> 8
- `test_apply_config_option_change_returns_refreshed_option_list`: `len()` 7 ->
  8

## Design Decisions

**`"none"` sentinel instead of `Option`**: The config panel always needs a
selected value. Using `"none"` as a first-class option value avoids special-
casing `None` in the UI-facing struct while still allowing callers to map it
back to `None` for the provider API.

**Lock-ordering discipline**: The session lock and agent lock must never be held
simultaneously to avoid deadlock with the prompt worker. The session lock is
dropped before the agent lock is acquired, matching the pattern in
`set_session_mode`.

**Warning-only provider error**: Failure to apply a thinking effort change to
the provider is logged as a warning but does not fail the config change RPC. The
runtime state has already been updated, so the config panel stays in sync with
the user's intent; the provider may simply not support the parameter for the
current model.

## Quality Gate Results

```text
cargo fmt --all                                    -- OK
cargo check --all-targets --all-features           -- OK
cargo clippy --all-targets --all-features          -- OK (no warnings)
cargo test --lib acp::session_config::tests        -- 38 passed, 0 failed
cargo test --doc session_config                    -- 9 passed, 0 failed
```
