# Phase 5: Session Controls and Available Commands Implementation

## Overview

This document describes three XZatoma ACP modules introduced as part of Phase 5
IDE tooling and runtime controls:

- `src/acp/session_mode.rs` - session mode definitions and runtime effect mapping
- `src/acp/session_config.rs` - session configuration option definitions and
  change handlers
- `src/acp/available_commands.rs` - slash command definitions for the Zed chat
  window

All three modules are registered in `src/acp/mod.rs` as public submodules.

---

## Module 1: `session_mode`

### Purpose

Defines the four XZatoma session modes that are advertised to Zed via the ACP
`SessionModeState` payload. Zed displays these in its mode selector UI. When the
user switches modes, the agent maps the selected mode ID to the concrete runtime
settings that govern file write access, terminal execution permissions, and
safety confirmation behavior.

### Mode Definitions

| Mode ID           | Chat Mode | Safety Mode    | Terminal Mode          |
| ----------------- | --------- | -------------- | ---------------------- |
| `planning`        | Planning  | AlwaysConfirm  | Interactive            |
| `write`           | Write     | AlwaysConfirm  | RestrictedAutonomous   |
| `safe`            | Write     | AlwaysConfirm  | RestrictedAutonomous   |
| `full_autonomous` | Write     | NeverConfirm   | FullAutonomous         |

The `write` and `safe` modes share the same runtime effect. The distinction is
semantic and intended for future phases where `safe` will additionally request
Zed user approval via the `ide_bridge` for risky operations.

### Public Constants

```rust
pub const MODE_PLANNING: &str = "planning";
pub const MODE_WRITE: &str = "write";
pub const MODE_SAFE: &str = "safe";
pub const MODE_FULL_AUTONOMOUS: &str = "full_autonomous";
```

Constants are exported so callers can compare mode IDs without hardcoding
strings. All constants are lowercase to satisfy the ACP protocol requirement
that mode IDs be stable, lowercase identifiers.

### Public API

| Symbol                       | Description                                                     |
| ---------------------------- | --------------------------------------------------------------- |
| `build_session_modes()`      | Returns all four `acp::SessionMode` entries for Zed's UI        |
| `build_session_mode_state()` | Wraps modes in an `acp::SessionModeState` with a current ID set |
| `ModeRuntimeEffect`          | Plain struct holding the resolved chat, safety, terminal values  |
| `mode_runtime_effect()`      | Maps a mode ID string to a `ModeRuntimeEffect` or an error      |

### Design Decisions

`build_session_mode_state` accepts any string as `mode_id` and stores it without
validation. This allows the current mode ID to survive agent restarts where the
stored value might come from a future protocol version that added a mode not
known to this binary. Callers that need to enforce valid IDs should call
`mode_runtime_effect` first and only proceed on `Ok`.

`ModeRuntimeEffect.chat_mode_str` and `.safety_mode_str` carry the exact string
literals expected by `ChatMode::parse_str` and `SafetyMode::parse_str`. This
avoids a direct dependency on those types inside the ACP layer.

### Error Handling

`mode_runtime_effect` returns `XzatomaError::Config` for unknown mode IDs. The
error message includes the unrecognised value and lists all valid options,
following the existing project convention for configuration validation errors.

### Testing

15 tests cover all four modes, the unknown mode state preservation, the empty
string error case, lowercase constant enforcement, and determinism.

---

## Module 2: `session_config`

### Purpose

Defines the seven session configuration options that XZatoma exposes to Zed via
`SessionConfigOption` select drop-downs. When the user changes an option in
Zed's UI, a `session/set_config_option` request is sent to the agent.
`apply_config_option_change` validates the request and returns both a
`ConfigChangeEffect` describing which runtime fields changed and a refreshed
option list for Zed to display.

### Config Option Definitions

| Config ID             | ACP Kind | Values                                               |
| --------------------- | -------- | ---------------------------------------------------- |
| `safety_policy`       | Select   | always_confirm, confirm_dangerous, never_confirm     |
| `terminal_execution`  | Select   | interactive, restricted_autonomous, full_autonomous  |
| `tool_routing`        | Select   | prefer_ide, prefer_local, require_ide                |
| `vision_input`        | Select   | enabled, disabled                                    |
| `subagent_delegation` | Select   | enabled, disabled                                    |
| `mcp_tools`           | Select   | enabled, disabled                                    |
| `max_turns`           | Select   | 10, 25, 50, 100, 200                                 |

All options use `SessionConfigKind::Select` because the `unstable_boolean_config`
feature is not enabled in this crate. The five binary options (`vision_input`,
`subagent_delegation`, `mcp_tools`) use a two-element `enabled`/`disabled`
select rather than a boolean toggle.

### `ToolRouting`

```rust
pub enum ToolRouting { PreferIde, PreferLocal, RequireIde }
```

`as_value_id` returns `&'static str` rather than `&str` because all match arms
are string literals. This satisfies `Into<SessionConfigValueId>` without
requiring an extra `.to_string()` allocation at the call site. The same
`&'static str` return is used on `safety_policy_value_id` and
`terminal_execution_value_id` for the same reason.

### `SessionRuntimeState`

`SessionRuntimeState::from_config` derives the initial state from the loaded
`Config`:

| Field              | Config source                        |
| ------------------ | ------------------------------------ |
| `safety_mode_str`  | `config.agent.chat.default_safety`   |
| `terminal_mode`    | `config.agent.terminal.default_mode` |
| `tool_routing`     | Hardcoded to `PreferIde`             |
| `vision_enabled`   | `config.acp.stdio.vision_enabled`    |
| `subagents_enabled`| `config.agent.subagent.chat_enabled` |
| `mcp_enabled`      | `config.mcp.auto_connect`            |
| `max_turns`        | `config.agent.max_turns`             |

`tool_routing` has no corresponding `Config` field in the current schema; it
defaults to `PreferIde` because that matches Zed-native IDE integration and can
be overridden at runtime.

`mcp_enabled` uses `config.mcp.auto_connect` as the top-level MCP liveness
signal. Individual `McpServerConfig` entries carry their own `enabled` flags,
but `auto_connect` governs whether the session manager attempts connections at
all.

### `ConfigChangeEffect`

All fields are `Option<T>` and default to `None`. Only the field corresponding
to the changed config option is populated. This lets callers apply targeted
runtime mutations without re-reading the entire state.

### `apply_config_option_change`

The function clones `runtime`, applies the change to the clone, builds a fresh
option list from the updated clone, and returns both the effect and the new
option list. The original `runtime` is not mutated. The caller is responsible
for persisting the new state.

### Safety Policy Value Mapping

The `safety_policy` config option exposes three human-friendly select values
(`always_confirm`, `confirm_dangerous`, `never_confirm`) that map to the
internal safety mode strings accepted by `SafetyMode::parse_str`:

| Select value      | Internal string    |
| ----------------- | ------------------ |
| `always_confirm`  | `"confirm"`        |
| `confirm_dangerous` | `"confirm_dangerous"` |
| `never_confirm`   | `"yolo"`           |

The current value displayed in the select is derived by the private
`safety_policy_value_id` helper, which maps the current `safety_mode_str` back
to the select value ID using pattern matching.

### Error Handling

Both `apply_config_option_change` and `ToolRouting::from_value_id` return
`XzatomaError::Config` with descriptive messages naming the unrecognised value
and listing valid options.

### Testing

31 tests cover `SessionRuntimeState::from_config`, all seven
`build_session_config_options` properties, all valid
`apply_config_option_change` paths for each config ID, error cases for unknown
config IDs and invalid values, and full round-trip coverage for `ToolRouting`.

---

## Module 3: `available_commands`

### Purpose

Defines the eight slash commands that XZatoma exposes in Zed's chat input
completion menu via `acp::AvailableCommand`. Commands that accept an optional
argument carry an `AvailableCommandInput::Unstructured` hint so Zed can display
a descriptive placeholder in the input field.

### Command Definitions

| Command      | Input        | Purpose                                          |
| ------------ | ------------ | ------------------------------------------------ |
| `/mode`      | Unstructured | Show or change the XZatoma operation mode        |
| `/model`     | Unstructured | Show or change the current provider model        |
| `/safety`    | Unstructured | Show or change the safety confirmation policy    |
| `/tools`     | None         | Summarize available XZatoma and IDE tools        |
| `/context`   | None         | Show current conversation context usage          |
| `/summarize` | None         | Summarize and compact conversation history       |
| `/skills`    | None         | List active skills for the current workspace     |
| `/mcp`       | None         | List connected MCP servers and tools             |

The three commands that accept input (`/mode`, `/model`, `/safety`) take an
optional value. The ACP schema does not have an "optional input" variant;
`Unstructured` is used for all value-accepting commands, and the agent handles
the absent-argument case at dispatch time.

### Non-Exhaustive Enum Handling

`acp::AvailableCommandInput` is marked `#[non_exhaustive]`. Test match
statements include a `_ => panic!(...)` arm to satisfy the compiler while
asserting that all commands we construct use the expected variant.

### Design Decisions

Commands are named with a leading `/` to match Zed's chat input convention.
The `build_available_commands` function is pure and deterministic; it allocates
a new `Vec` on every call. Callers that need the list infrequently (for example,
once at session creation) can call it directly. A future phase could cache the
result if profiling shows allocation pressure.

All descriptions are written from the user's perspective and describe the
outcome rather than the mechanism.

### Error Handling

`build_available_commands` is infallible. No validation is required because all
data is statically constructed from string literals.

### Testing

10 tests cover entry count, name correctness, no-empty-descriptions invariant,
slash prefix invariant, unstructured input presence for the three value-accepting
commands, no-input for the five argument-free commands, no duplicate names, and
determinism.

---

## ACP SDK Types Used

| Type                          | Source  | Usage                                             |
| ----------------------------- | ------- | ------------------------------------------------- |
| `acp::SessionMode`            | schema  | One entry in the mode selector list               |
| `acp::SessionModeState`       | schema  | Wraps current mode ID and available modes list    |
| `acp::SessionModeId`          | schema  | Opaque mode identifier                            |
| `acp::SessionConfigOption`    | schema  | One drop-down in Zed's config panel               |
| `acp::SessionConfigOptionCategory` | schema | UX hint for icon / placement                 |
| `acp::SessionConfigKind`      | schema  | Discriminated union; only `Select` used here      |
| `acp::SessionConfigSelect`    | schema  | Current value + options list for a select         |
| `acp::SessionConfigSelectOption` | schema | One selectable value in a drop-down             |
| `acp::SessionConfigSelectOptions` | schema | Flat list or grouped list of select options   |
| `acp::SessionConfigId`        | schema  | Stable string identifier for a config option      |
| `acp::SessionConfigValueId`   | schema  | Stable string identifier for a config value       |
| `acp::AvailableCommand`       | schema  | One slash command shown in Zed's completion menu  |
| `acp::AvailableCommandInput`  | schema  | Non-exhaustive enum; `Unstructured` variant used  |
| `acp::UnstructuredCommandInput` | schema | Free-text hint shown when no input is typed     |

---

## Files Created

| File                               | Change       |
| ---------------------------------- | ------------ |
| `src/acp/session_mode.rs`          | Created (new) |
| `src/acp/session_config.rs`        | Created (new) |
| `src/acp/available_commands.rs`    | Created (new) |
| `src/acp/mod.rs`                   | Added three `pub mod` declarations |

---

## Quality Gate Results

```text
cargo fmt --all                          PASS
cargo check --all-targets --all-features PASS
cargo clippy --all-targets --all-features -- -D warnings  PASS
cargo test --lib --all-features (session_mode)       15/15 PASS
cargo test --lib --all-features (session_config)     31/31 PASS
cargo test --lib --all-features (available_commands) 10/10 PASS
```

Total new tests added: 56.
