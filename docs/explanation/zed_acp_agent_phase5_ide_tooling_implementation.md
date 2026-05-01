# Phase 5: Zed IDE Tooling and Runtime Controls Implementation

## Overview

Phase 5 adds full Zed IDE integration to the XZatoma ACP stdio agent. The
implementation covers six new modules, one new tool registration file, extensive
changes to the ACP stdio transport layer, and 123 new tests.

After Phase 5, the Zed chat UI shows a mode selector, config option controls,
and a model selector for XZatoma sessions. Changing any of these from Zed
updates XZatoma runtime behavior for subsequent prompts. When Zed advertises
file system or terminal capabilities, XZatoma registers IDE-aware tools that
route operations through Zed's editor buffer and terminal panel instead of the
local file system.

---

## New Files

### `src/acp/session_mode.rs`

Defines the four XZatoma session modes advertised to Zed.

| Mode ID           | Chat mode | Safety mode    | Terminal mode         |
| ----------------- | --------- | -------------- | --------------------- |
| `planning`        | planning  | always confirm | interactive           |
| `write`           | write     | always confirm | restricted autonomous |
| `safe`            | write     | always confirm | restricted autonomous |
| `full_autonomous` | write     | never confirm  | full autonomous       |

Public surface:

- Constants: `MODE_PLANNING`, `MODE_WRITE`, `MODE_SAFE`, `MODE_FULL_AUTONOMOUS`
- `build_session_modes() -> Vec<acp::SessionMode>` - full mode list with names
  and descriptions for Zed's mode selector UI
- `build_session_mode_state(mode_id: &str) -> acp::SessionModeState` - wraps the
  mode list with an active mode ID; unknown IDs round-trip without error
- `ModeRuntimeEffect` - holds `chat_mode_str`, `safety_mode_str`, and
  `terminal_mode` fields produced by a mode change
- `mode_runtime_effect(mode_id: &str) -> Result<ModeRuntimeEffect>` - maps a
  mode ID to its concrete runtime settings; returns `XzatomaError::Config` for
  unknown IDs

Tests: 15 unit tests and 5 doc tests.

### `src/acp/session_config.rs`

Defines seven session config option drop-downs advertised to Zed.

| Config ID             | Values                                                    |
| --------------------- | --------------------------------------------------------- |
| `safety_policy`       | `always_confirm`, `confirm_dangerous`, `never_confirm`    |
| `terminal_execution`  | `interactive`, `restricted_autonomous`, `full_autonomous` |
| `tool_routing`        | `prefer_ide`, `prefer_local`, `require_ide`               |
| `vision_input`        | `enabled`, `disabled`                                     |
| `subagent_delegation` | `enabled`, `disabled`                                     |
| `mcp_tools`           | `enabled`, `disabled`                                     |
| `max_turns`           | `10`, `25`, `50`, `100`, `200`                            |

Public surface:

- Seven `CONFIG_*` string constants for stable option IDs
- `ToolRouting` enum (`PreferIde`, `PreferLocal`, `RequireIde`) with
  `as_value_id()` and `from_value_id()` conversion methods
- `ConfigChangeEffect` - all-`Option` struct populated only for changed fields
- `SessionRuntimeState` - snapshot of live runtime values; `from_config(config)`
  builds the initial state from loaded `Config`
- `build_session_config_options(runtime: &SessionRuntimeState) -> Vec<SessionConfigOption>` -
  builds all seven select options reflecting current runtime state
- `apply_config_option_change(config_id, value_id, runtime) -> Result<(ConfigChangeEffect, Vec<SessionConfigOption>)>` -
  validates, clones runtime, applies change, and returns the effect and
  refreshed option list

Tests: 31 unit tests and 9 doc tests.

### `src/acp/available_commands.rs`

Defines eight Zed chat slash commands shown in the chat input.

| Command      | Input | Description                                   |
| ------------ | ----- | --------------------------------------------- |
| `/mode`      | hint  | Show or change XZatoma operation mode         |
| `/model`     | hint  | Show or change the current provider model     |
| `/safety`    | hint  | Show or change the safety confirmation policy |
| `/tools`     | none  | Summarize available XZatoma and IDE tools     |
| `/context`   | none  | Show current conversation context usage       |
| `/summarize` | none  | Summarize and compact conversation history    |
| `/skills`    | none  | List active skills for the current workspace  |
| `/mcp`       | none  | List connected MCP servers and tools          |

Public surface:

- `build_available_commands() -> Vec<acp::AvailableCommand>` - pure,
  deterministic; commands with arguments use
  `AvailableCommandInput::Unstructured` with descriptive hints

Tests: 10 unit tests and 1 doc test.

### `src/acp/ide_bridge.rs`

Wraps `ConnectionTo<AcpClientRole>` and exposes a clean async interface for
every Zed-provided client capability.

`ConnectionTo<AcpClientRole>` is `Clone + Send` but not `Sync`. Each method
clones the connection into a `tokio::task::spawn` task and awaits `block_task()`
there. This is safe because the spawned task runs concurrently with the ACP
event loop rather than blocking it.

`IdeCapabilities` is a `Debug + Clone` struct constructed from
`acp::ClientCapabilities` at initialize time. Methods that require a capability
that the client did not advertise return `XzatomaError::Internal` immediately.

| Method                   | ACP request              | Capability gate   |
| ------------------------ | ------------------------ | ----------------- |
| `read_text_file`         | `fs/read_text_file`      | `read_text_file`  |
| `read_text_file_range`   | `fs/read_text_file`      | `read_text_file`  |
| `write_text_file`        | `fs/write_text_file`     | `write_text_file` |
| `create_terminal`        | `terminal/create`        | `terminal`        |
| `terminal_output`        | `terminal/output`        | (implicit)        |
| `wait_for_terminal_exit` | `terminal/wait_for_exit` | (implicit)        |
| `kill_terminal`          | `terminal/kill`          | (implicit)        |
| `release_terminal`       | `terminal/release`       | (implicit)        |

Tests: 8 unit tests and 7 doc tests.

### `src/acp/tool_notifications.rs`

Pure, stateless builder functions that map XZatoma tool execution events to ACP
`ToolCall` and `ToolCallUpdate` notifications.

| Function                     | ACP type                | Purpose                                          |
| ---------------------------- | ----------------------- | ------------------------------------------------ |
| `generate_tool_call_id`      | `ToolCallId`            | Unique `xzatoma-call-{uuid_v4_simple}` ID        |
| `tool_kind_for_name`         | `ToolKind`              | Maps tool names to Zed icon categories           |
| `tool_call_title`            | `String`                | Human-readable title with extracted key argument |
| `build_tool_call_start`      | `ToolCall`              | `InProgress` notification before execution       |
| `build_tool_call_completion` | `ToolCallUpdate`        | `Completed` notification with raw output         |
| `build_tool_call_failure`    | `ToolCallUpdate`        | `Failed` notification with error string          |
| `build_tool_call_locations`  | `Vec<ToolCallLocation>` | File path locations for file-targeting tools     |

`ToolKind` mapping:

| Kind      | Tool names                                                                                                    |
| --------- | ------------------------------------------------------------------------------------------------------------- |
| `Read`    | `read_file`, `list_directory`, `find_path`, `grep`, `file_metadata`, `ide_read_text_file`                     |
| `Edit`    | `write_file`, `edit_file`, `create_directory`, `copy_path`, `delete_path`, `move_path`, `ide_write_text_file` |
| `Execute` | `terminal`, `ide_open_terminal`, `ide_terminal_output`, `ide_wait_for_terminal_exit`, `ide_kill_terminal`     |
| `Fetch`   | `fetch`                                                                                                       |
| `Think`   | `plan`                                                                                                        |
| `Other`   | `subagent`, `parallel_subagent`, `ide_request_permission`, any unknown name                                   |

Tests: 42 unit tests and 7 doc tests.

### `src/tools/ide_tools.rs`

Seven IDE-aware `ToolExecutor` implementations registered when the Zed client
advertises IDE capabilities. All tools hold an `Arc<IdeBridge>` and delegate to
the bridge on execution.

| Tool name                    | Registered when    | Operation                                    |
| ---------------------------- | ------------------ | -------------------------------------------- |
| `ide_read_text_file`         | `read_text_file`   | Read file through Zed's open project buffers |
| `ide_write_text_file`        | `write_text_file`  | Write file through Zed's editor              |
| `ide_open_terminal`          | `terminal`         | Run command in Zed's terminal UI             |
| `ide_terminal_output`        | `terminal`         | Read buffered terminal output                |
| `ide_wait_for_terminal_exit` | `terminal`         | Wait for terminal command to finish          |
| `ide_kill_terminal`          | `terminal`         | Kill a running terminal                      |
| `ide_request_permission`     | any IDE capability | Ask user to approve a risky action           |

`register_ide_tools(registry: &mut ToolRegistry, bridge: Arc<IdeBridge>)` is the
entry point. It checks `bridge.capabilities()` and registers only the tools that
the Zed client supports.

`ide_open_terminal` with `wait_for_exit: true` (the default) creates a terminal,
waits for exit, reads output, and releases the terminal in one atomic tool
invocation. With `wait_for_exit: false` it returns a terminal ID for subsequent
polling with `ide_terminal_output` and `ide_wait_for_terminal_exit`.

`ide_request_permission` logs the operation at INFO level and auto-grants
permission. Full Zed elicitation API integration is deferred to a later phase.

Tests: 17 unit tests and 2 doc tests.

---

## Modified Files

### `src/acp/mod.rs`

Added `pub mod` declarations for all six new modules:

- `pub mod available_commands;`
- `pub mod ide_bridge;`
- `pub mod session_config;`
- `pub mod session_mode;`
- `pub mod tool_notifications;`

### `src/tools/mod.rs`

Added `pub mod ide_tools;`.

### `src/acp/stdio.rs`

The ACP stdio transport received the largest changes.

#### New imports

```rust
use crate::acp::available_commands::build_available_commands;
use crate::acp::ide_bridge::{IdeBridge, IdeCapabilities};
use crate::acp::session_config::{build_session_config_options, SessionRuntimeState};
use crate::acp::session_mode::{
    build_session_mode_state, mode_runtime_effect, MODE_FULL_AUTONOMOUS, MODE_WRITE,
};
use crate::tools::ide_tools::register_ide_tools;
```

#### `ActiveSessionState` new fields

| Field             | Type                     | Purpose                                                 |
| ----------------- | ------------------------ | ------------------------------------------------------- |
| `current_mode_id` | `String`                 | Active session mode; updated by `SetSessionModeRequest` |
| `runtime_state`   | `SessionRuntimeState`    | Live config option values for the session               |
| `ide_bridge`      | `Option<Arc<IdeBridge>>` | Bridge when Zed advertised IDE capabilities             |

New accessors: `current_mode_id()`, `runtime_state()`, `has_ide_bridge()`.

#### `AcpStdioServerState` new field

`client_capabilities: Arc<Mutex<Option<acp::ClientCapabilities>>>` stores the
capabilities snapshot from `InitializeRequest` for use during session creation.

#### `create_session` changes

- Signature extended with `connection: Option<ConnectionTo<AcpClientRole>>`.
- Session ID is now generated early (before tool registry construction) so the
  IDE bridge can reference it.
- If client capabilities advertise any IDE feature, an `IdeBridge` is
  constructed and passed to `register_ide_tools`.
- `initial_mode_id_from_config` derives the initial mode from the effective
  config (`full_autonomous` when terminal mode is `FullAutonomous`, `planning`
  when default chat mode is `"planning"`, otherwise `write`).
- `SessionRuntimeState::from_config` builds the initial runtime state.
- `NewSessionResponse` is returned with `.modes(mode_state)` and
  `.config_options(config_options)` in addition to the existing
  `.models(model_state)`.

#### `InitializeRequest` handler changes

Stores `initialize.client_capabilities` in `state.client_capabilities` before
responding. The stored snapshot is used in subsequent `create_session` calls.

#### `NewSessionRequest` handler changes

Passes `Some(cx.clone())` as the connection to `create_session`. After the
response is sent, dispatches an `AvailableCommandsUpdate` notification so Zed
populates its slash command list immediately.

#### New request handlers

Three new handlers are registered in `run_stdio_agent_with_transport`:

**`SetSessionModeRequest`**

1. Validates the session ID and mode ID via `mode_runtime_effect`.
2. Updates `current_mode_id`, `runtime_state.safety_mode_str`, and
   `runtime_state.terminal_mode` inside the session lock.
3. Drops the session lock before acquiring the agent lock.
4. Rebuilds agent transient system messages with the new `ChatMode` and
   `SafetyMode`.
5. Responds with `SetSessionModeResponse::new()`.
6. Sends `CurrentModeUpdate` notification.

**`SetSessionConfigOptionRequest`**

1. Validates the session ID.
2. Calls `apply_config_option_change` to validate the config ID and value and
   compute the effect.
3. Applies all non-`None` effect fields to `runtime_state`.
4. Responds with `SetSessionConfigOptionResponse::new(updated_options)`.
5. Sends `ConfigOptionUpdate` notification with the same updated options.

**`SetSessionModelRequest`**

1. Validates the session ID.
2. Updates `current_model_name` in the session state (used for persistence and
   display).
3. Responds with `SetSessionModelResponse::new()`.

Note: the live `Arc<dyn Provider>` is not rebuilt in-flight because `Provider`
requires `&mut self` for `set_model` and the trait object is wrapped in `Arc`.
The new model name takes effect for persistence immediately and for inference on
the next session start.

#### `initial_mode_id_from_config` helper

```rust
fn initial_mode_id_from_config(config: &Config) -> String {
    if config.agent.terminal.default_mode == ExecutionMode::FullAutonomous {
        MODE_FULL_AUTONOMOUS.to_string()
    } else if config.agent.chat.default_mode == "planning" {
        MODE_PLANNING.to_string()
    } else {
        MODE_WRITE.to_string()
    }
}
```

---

## Protocol Flow

The complete session startup flow with Phase 5 additions:

```text
Zed                                        XZatoma
 |-- InitializeRequest ------------------>|
 |                                        | store client_capabilities
 |<-- InitializeResponse -----------------|
 |
 |-- NewSessionRequest ----------------->|
 |                                        | create IDE bridge (if caps advertised)
 |                                        | register IDE tools
 |                                        | build mode state + config options
 |<-- NewSessionResponse -----------------|  (includes modes, config_options, models)
 |<-- AvailableCommandsUpdate notification|
 |
 |-- SetSessionModeRequest -------------->|
 |                                        | validate mode, update state, rebuild prompts
 |<-- SetSessionModeResponse -------------|
 |<-- CurrentModeUpdate notification -----|
 |
 |-- SetSessionConfigOptionRequest ------>|
 |                                        | validate config_id + value, update runtime
 |<-- SetSessionConfigOptionResponse -----|  (full updated options list)
 |<-- ConfigOptionUpdate notification ----|
 |
 |-- SetSessionModelRequest ------------->|
 |                                        | update current_model_name
 |<-- SetSessionModelResponse ------------|
```

---

## Testing

### Unit tests (new modules)

| Module               | Tests   |
| -------------------- | ------- |
| `session_mode`       | 15      |
| `session_config`     | 31      |
| `available_commands` | 10      |
| `ide_bridge`         | 8       |
| `tool_notifications` | 42      |
| `ide_tools`          | 17      |
| **Total**            | **123** |

### Protocol tests (`acp::stdio::tests`)

Thirteen new in-memory protocol tests added to `stdio.rs`:

- `test_new_session_response_includes_session_modes` - advertises all four modes
- `test_new_session_response_includes_config_options` - advertises all config
  option IDs including `safety_policy`, `terminal_execution`, `tool_routing`
- `test_set_session_mode_changes_current_mode` - happy path for planning mode
- `test_set_session_mode_unknown_session_returns_error` - unknown session ID
- `test_set_session_mode_invalid_mode_returns_error` - invalid mode ID
- `test_set_session_config_option_returns_updated_options` - sets
  `safety_policy` to `never_confirm` and verifies response reflects the change
- `test_set_session_config_option_invalid_value_returns_error` - invalid value
- `test_set_session_model_changes_model` - uses advertised current model ID
- `test_set_session_mode_all_valid_modes_succeed` - cycles through all four
  modes
- `test_create_session_includes_current_mode_id` - initial mode is valid
- `test_initial_mode_id_from_config_default_is_planning` - default config maps
  to planning
- `test_initial_mode_id_from_config_full_autonomous_when_allow_dangerous` -
  FullAutonomous terminal mode maps to full_autonomous
- `test_active_session_state_includes_mode_and_runtime_state` - session state
  carries non-empty mode ID and safety string after creation

---

## Known Limitations

### Model switching does not change the live provider

`Provider::set_model` requires `&mut self` but the provider is stored as
`Arc<dyn Provider>`. Model changes from `SetSessionModelRequest` update the
persisted model name and the session display immediately but do not change which
LLM is used for inference until the session is restarted. A future phase can
address this by wrapping the model name in an `Arc<Mutex<String>>` and reading
it at prompt execution time.

### `ide_request_permission` auto-grants permission

The Zed elicitation API (`request_permission` ACP method) is not yet integrated.
The `ide_request_permission` tool logs the request and returns
`{ "approved": true }` unconditionally. Full elicitation support is deferred to
a later phase.

### Tool call notifications are not emitted during prompt execution

`tool_notifications.rs` provides all the builder functions needed to emit rich
ACP `ToolCall` and `ToolCallUpdate` notifications during prompt execution.
Wiring these builders into the agent execution loop (`execute_queued_prompt`)
requires streaming infrastructure that is planned for Phase 6.

### Config option changes do not rebuild the tool registry

When `vision_input` or `subagent_delegation` config options are changed at
runtime, the updated value is stored in `runtime_state` but the tool registry
registered at session creation is not rebuilt. Affected tools continue to use
their original configuration until the session is restarted.

---

## Quality Gates

```text
cargo fmt --all               PASS
cargo check --all-targets     PASS
cargo clippy -- -D warnings   PASS
cargo test (phase 5 modules)  123/123 PASS
cargo test (stdio protocol)    36/36 PASS
```
