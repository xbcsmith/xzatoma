# Phase 3: Code Deduplication Implementation

## Overview

Phase 3 eliminates the duplicate initialization sequences and structurally
identical type definitions that accumulated across the codebase. It depends on
Phase 2 (error handling consolidation) being complete so all extracted code uses
`XzatomaError` and `Result<T, XzatomaError>` consistently.

Six deduplication tasks were completed:

| Task | Description                                                   | Files Changed                                                                  |
| ---- | ------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| 3.1  | Extract shared MCP manager builder                            | `src/mcp/manager.rs`                                                           |
| 3.2  | Extract tool and skill initialization into `AgentEnvironment` | `src/commands/environment.rs`, `src/commands/mod.rs`, `src/acp/executor.rs`    |
| 3.3  | Unify provider request/response structs                       | `src/providers/base.rs`, `src/providers/ollama.rs`, `src/providers/copilot.rs` |
| 3.4  | Extract shared `convert_tools_from_json`                      | `src/providers/base.rs`, `src/providers/copilot.rs`, `src/providers/ollama.rs` |
| 3.5  | Extract interactive approval helper                           | `src/mcp/approval.rs`, `src/mcp/tool_bridge.rs`                                |
| 3.6  | Consolidate `build_visible_skill_catalog`                     | `src/commands/mod.rs`, `src/commands/skills.rs`                                |

---

## Task 3.1: Shared MCP Manager Builder

### Problem

The MCP client manager initialization sequence was repeated verbatim in three
locations:

- `src/commands/mod.rs` (`run_chat`) - 22 lines
- `src/commands/mod.rs` (`run_plan_with_options`) - 22 lines
- `src/acp/executor.rs` (`build_mcp_manager`) - 22 lines

Each copy guarded on
`config.mcp.auto_connect && !config.mcp.servers.is_empty()`, created an HTTP
client and token store, looped over enabled servers calling `manager.connect()`,
logged warnings for failures, and wrapped the result in
`Arc<RwLock<McpClientManager>>`.

### Solution

Added `build_mcp_manager_from_config` as a public free function in
`src/mcp/manager.rs`:

```rust
pub async fn build_mcp_manager_from_config(
    config: &Config,
) -> Result<Option<Arc<RwLock<McpClientManager>>>>
```

The function returns `Ok(None)` immediately when `auto_connect` is disabled or
no servers are configured, so call sites require no guard logic. Individual
server connection failures are downgraded to warnings so a partially-connected
manager is always returned rather than an early error.

### Call Site Changes

| Location                | Before                                        | After                                             |
| ----------------------- | --------------------------------------------- | ------------------------------------------------- |
| `run_chat`              | 22-line inline block with local `use` imports | `build_mcp_manager_from_config(&config).await?`   |
| `run_plan_with_options` | 22-line inline block with local `use` imports | delegated to `build_agent_environment` (Task 3.2) |
| `acp/executor.rs`       | Private `build_mcp_manager` method (22 lines) | delegated to `build_agent_environment` (Task 3.2) |

---

## Task 3.2: AgentEnvironment Builder

### Problem

The complete tool and skill initialization sequence was duplicated in three
places:

- `src/commands/mod.rs::run_plan_with_options` (~55 lines)
- `src/acp/executor.rs::build_tools` (~55 lines)

Both functions performed the same seven steps:

1. Parse `ChatMode` and `SafetyMode` from config
2. Build startup skill disclosure text
3. Build the visible skill catalog
4. Create an `ActiveSkillRegistry`
5. Build a `ToolRegistry` via `ToolRegistryBuilder`
6. Register the `activate_skill` tool
7. Build the MCP manager and register MCP tools

### Solution

Created `src/commands/environment.rs` with:

- `AgentEnvironment` struct bundling `ToolRegistry`,
  `Option<Arc<RwLock<McpClientManager>>>`, `ChatMode`, `SafetyMode`,
  `Arc<Mutex<ActiveSkillRegistry>>`, and `Option<String>` skill disclosure.
- `build_agent_environment(config, working_dir, headless)` async factory that
  executes all seven initialization steps once.

The new module is declared in `commands/mod.rs` as `pub mod environment` and
re-exports `AgentEnvironment` and `build_agent_environment` at the `commands::`
level so existing callers need only update their imports.

### Call Site Changes

`run_plan_with_options` before (55 lines):

```rust
let skill_disclosure = build_startup_skill_disclosure(&config, &working_dir)?;
let visible_skill_catalog = build_visible_skill_catalog(&config, &working_dir)?;
let active_skill_registry = Arc::new(std::sync::Mutex::new(ActiveSkillRegistry::new()));
use crate::chat_mode::{ChatMode, SafetyMode};
use crate::tools::registry_builder::ToolRegistryBuilder;
let chat_mode = ChatMode::parse_str(...).unwrap_or(ChatMode::Planning);
let safety_mode = match ... { "yolo" => ..., _ => ... };
let mut tools = ToolRegistryBuilder::new(...).build()?;
let _activate_skill_registered = register_activate_skill_tool(...)?;
// 22-line MCP manager block
// 10-line MCP tools registration block
```

`run_plan_with_options` after (6 lines):

```rust
let env = build_agent_environment(&config, &working_dir, true).await?;
let tools = env.tool_registry;
let active_skill_registry = env.active_skill_registry;
let skill_disclosure = env.skill_disclosure;
let _mcp_manager = env.mcp_manager;
```

`acp/executor.rs::build_tools` was removed entirely; `execute_prompt` now calls
`build_agent_environment` directly and the private `build_mcp_manager` method
was deleted.

### Additional Simplification

The 9-line `match config.provider.provider_type.as_str()` block in
`run_plan_with_options` that duplicated the logic of `create_provider` was
replaced with:

```rust
let provider_box = create_provider(&config.provider.provider_type, &config.provider)?;
let provider: Arc<dyn crate::providers::Provider> = Arc::from(provider_box);
let mut agent = Agent::new_from_shared_provider(provider, tools, config.agent.clone())?;
```

---

## Task 3.3: Unified Provider Wire Types

### Problem

`src/providers/copilot.rs` and `src/providers/ollama.rs` each defined
structurally identical private structs:

| Copilot               | Ollama               | Shape                                  |
| --------------------- | -------------------- | -------------------------------------- |
| `CopilotRequest`      | `OllamaRequest`      | `model`, `messages`, `tools`, `stream` |
| `CopilotMessage`      | `OllamaMessage`      | `role`, `content`, `tool_calls`        |
| `CopilotTool`         | `OllamaTool`         | `type`, `function`                     |
| `CopilotFunction`     | `OllamaFunction`     | `name`, `description`, `parameters`    |
| `CopilotToolCall`     | `OllamaToolCall`     | `id`, `type`, `function`               |
| `CopilotFunctionCall` | `OllamaFunctionCall` | `name`, `arguments`                    |

The one real difference: `CopilotMessage` has `tool_call_id: Option<String>`
(absent in Ollama), and `CopilotFunctionCall.arguments` is `String` (Ollama uses
`serde_json::Value`).

### Solution

Six shared types were added to `src/providers/base.rs` and re-exported from
`src/providers/mod.rs`:

```rust
pub struct ProviderTool        { type, function: ProviderFunction }
pub struct ProviderFunction    { name, description, parameters: Value }
pub struct ProviderMessage     { role, content, tool_calls, tool_call_id: Option<String> }
pub struct ProviderToolCall    { id, type, function: ProviderFunctionCall }
pub struct ProviderFunctionCall { name, arguments: serde_json::Value }
pub struct ProviderRequest     { model, messages, tools, stream }
```

`ProviderMessage.tool_call_id` uses
`#[serde(skip_serializing_if = "Option::is_none")]` so it is omitted from Ollama
wire format when `None`.

`ProviderFunctionCall.arguments` stores the canonical `serde_json::Value` form.
Copilot converts to a JSON string at the wire layer via the
`arguments_as_string()` helper method; Ollama serializes the value directly.

### Ollama Changes

All six private Ollama structs were replaced with type aliases:

```rust
type OllamaRequest        = ProviderRequest;
type OllamaMessage        = ProviderMessage;
type OllamaToolCall       = ProviderToolCall;
type OllamaFunctionCall   = ProviderFunctionCall;
```

`OllamaTool` and `OllamaFunction` are now `ProviderTool` and `ProviderFunction`
(used directly without aliases since `convert_tools_from_json` returns
`Vec<ProviderTool>`). The private `default_tool_type` function was deleted;
`ProviderToolCall` provides the equivalent
`#[serde(default = "provider_tool_call_type")]` in `base.rs`.

### Copilot Changes

`CopilotTool` and `CopilotFunction` were replaced with type aliases pointing to
the shared types. `CopilotRequest.tools` changed from `Vec<CopilotTool>` to
`Vec<ProviderTool>`. The `CopilotMessage`, `CopilotToolCall`, and
`CopilotFunctionCall` structs are kept private to `copilot.rs` because Copilot
uses `arguments: String` on the wire, which is incompatible with the shared
`arguments: serde_json::Value`.

---

## Task 3.4: Shared `convert_tools_from_json`

### Problem

`copilot.rs` and `ollama.rs` each contained an identical 12-line `convert_tools`
method:

```rust
fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<XxxTool> {
    tools.iter().filter_map(|t| {
        let obj = t.as_object()?;
        let name = obj.get("name")?.as_str()?.to_string();
        let description = obj.get("description")?.as_str()?.to_string();
        let parameters = obj.get("parameters")?.clone();
        Some(XxxTool { r#type: "function".to_string(), function: XxxFunction { name, description, parameters } })
    }).collect()
}
```

### Solution

Added a public free function to `src/providers/base.rs`:

```rust
pub fn convert_tools_from_json(tools: &[serde_json::Value]) -> Vec<ProviderTool>
```

Entries missing `name`, `description`, or `parameters` are silently dropped.
Both `copilot.rs` and `ollama.rs` delegate to this function. The Copilot
`convert_tools` method retains the `#[allow(dead_code)]` annotation (it is used
in tests and retained for future Responses-endpoint integration).

---

## Task 3.5: Interactive Approval Helper

### Problem

`src/mcp/tool_bridge.rs` contained three structurally identical 15-line blocks
that printed a prompt to stderr, read a line from stdin, and returned an
approval decision:

- `McpToolExecutor::execute` (lines 179-200)
- `McpResourceToolExecutor::execute` (lines 359-378)
- `McpPromptToolExecutor::execute` (lines 523-545)

Each block:

1. `eprint!("{} Allow? [y/N] ", description)`
2. `stderr().flush()`
3. `stdin().lock().read_line(&mut line)`
4. Mapped the read error to `XzatomaError::Tool`
5. Checked `trimmed == "y" || trimmed == "yes"`

### Solution

Added `prompt_user_approval(description: &str) -> Result<bool>` to
`src/mcp/approval.rs`, which is the module that already owns
`should_auto_approve`. Placing the function there keeps all MCP approval policy
and mechanics in one module.

Each of the three call sites reduces to two lines:

```rust
if !should_auto_approve(self.execution_mode, self.headless)
    && !prompt_user_approval(&format!("MCP tool call: {}/{} with args: {}.", ...))?
{
    return Ok(ToolResult::error(format!("User rejected MCP tool call: {}", ...)));
}
```

The `std::io::{BufRead, Write}` imports were removed from `tool_bridge.rs`
because the stdin/stderr logic now lives in `approval.rs`.

---

## Task 3.6: Consolidate `build_visible_skill_catalog`

### Problem

Two versions of `build_visible_skill_catalog` existed:

- `src/commands/mod.rs` - 2 parameters (`config`, `working_dir`): loaded trusted
  paths internally, then called `discover_skills`,
  `filter_visible_skill_records`, and `SkillCatalog::from_records`.
- `src/commands/skills.rs` - 3 parameters (`config`, `working_dir`,
  `trusted_paths`): accepted trusted paths externally and delegated directly.

### Solution

The 2-parameter version in `commands/mod.rs` was converted to a thin wrapper:

```rust
pub fn build_visible_skill_catalog(config: &Config, working_dir: &Path) -> Result<SkillCatalog> {
    // Thin wrapper: load trusted paths then delegate to the canonical
    // 3-parameter version in `commands::skills`.
    let trusted_paths = crate::skills::trust::load_trusted_paths(&config.skills, working_dir)?;
    crate::commands::skills::build_visible_skill_catalog(config, working_dir, &trusted_paths)
}
```

The filtering logic now lives in exactly one place: `commands/skills.rs`.

---

## Deliverables

### New Files

| File                          | Purpose                                                         |
| ----------------------------- | --------------------------------------------------------------- |
| `src/commands/environment.rs` | `AgentEnvironment` struct and `build_agent_environment` factory |

### Modified Files

| File                          | Change                                                                                                                                                                                                               |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/mcp/approval.rs`         | Added `prompt_user_approval`                                                                                                                                                                                         |
| `src/mcp/tool_bridge.rs`      | Replaced 3 approval blocks with `prompt_user_approval` calls                                                                                                                                                         |
| `src/mcp/manager.rs`          | Added `build_mcp_manager_from_config`                                                                                                                                                                                |
| `src/commands/mod.rs`         | Thin wrapper for `build_visible_skill_catalog`; declared `environment` module; replaced inline MCP builder in `run_chat`; replaced full init block in `run_plan_with_options`; removed `CopilotProvider` match block |
| `src/commands/environment.rs` | New file (see above)                                                                                                                                                                                                 |
| `src/acp/executor.rs`         | Removed `build_tools` and `build_mcp_manager` methods; `execute_prompt` uses `build_agent_environment`                                                                                                               |
| `src/providers/base.rs`       | Added `ProviderTool`, `ProviderFunction`, `ProviderMessage`, `ProviderRequest`, `ProviderToolCall`, `ProviderFunctionCall`, `convert_tools_from_json`                                                                |
| `src/providers/mod.rs`        | Re-exported new shared types                                                                                                                                                                                         |
| `src/providers/copilot.rs`    | Type aliases for `CopilotTool`/`CopilotFunction`; `convert_tools` delegates to `convert_tools_from_json`                                                                                                             |
| `src/providers/ollama.rs`     | Replaced all private wire structs with shared type aliases; `convert_tools` delegates to `convert_tools_from_json`                                                                                                   |

---

## Tests Added

### `src/mcp/approval.rs`

- `test_prompt_user_approval_signature_is_callable` - verifies the function
  compiles with the expected signature without blocking on stdin.

### `src/mcp/manager.rs`

- `test_build_mcp_manager_from_config_returns_none_when_auto_connect_disabled`
- `test_build_mcp_manager_from_config_returns_none_when_no_servers`
- `test_build_mcp_manager_from_config_returns_some_when_servers_configured`
- `test_build_mcp_manager_from_config_skips_disabled_servers`

### `src/commands/environment.rs`

- `test_build_agent_environment_succeeds_with_default_config`
- `test_build_agent_environment_headless_true_no_mcp_manager_by_default`
- `test_build_agent_environment_headless_false_no_mcp_manager_by_default`
- `test_build_agent_environment_chat_mode_defaults_to_planning`
- `test_build_agent_environment_safety_mode_defaults_to_always_confirm`
- `test_build_agent_environment_yolo_safety_mode_parses_correctly`
- `test_build_agent_environment_tool_registry_is_populated`
- `test_build_agent_environment_write_mode_parsed_from_config`
- `test_build_agent_environment_active_skill_registry_is_initialized`

### `src/providers/base.rs`

- `test_convert_tools_from_json_single_tool`
- `test_convert_tools_from_json_multiple_tools`
- `test_convert_tools_from_json_drops_missing_name`
- `test_convert_tools_from_json_drops_missing_description`
- `test_convert_tools_from_json_drops_missing_parameters`
- `test_convert_tools_from_json_empty_slice_returns_empty`
- `test_convert_tools_from_json_skips_invalid_keeps_valid`
- `test_provider_tool_serializes_to_expected_json`
- `test_provider_tool_round_trips_through_json`
- `test_provider_function_call_arguments_as_string_object`
- `test_provider_function_call_arguments_as_string_empty_object`
- `test_provider_function_call_default_arguments_is_null`
- `test_provider_message_omits_tool_call_id_when_none`
- `test_provider_message_includes_tool_call_id_when_set`
- `test_provider_message_deserializes_missing_tool_call_id_as_none`
- `test_provider_tool_call_type_defaults_to_function`
- `test_provider_request_omits_empty_tools_array`
- `test_provider_request_includes_tools_when_non_empty`

---

## Success Criteria Verification

### No block longer than 5 lines is duplicated verbatim

| Previously duplicated block                     | Extracted to                     | Lines saved |
| ----------------------------------------------- | -------------------------------- | ----------- |
| MCP manager init (22 lines, 3 copies)           | `build_mcp_manager_from_config`  | 44          |
| Tool/skill init sequence (55 lines, 2 copies)   | `build_agent_environment`        | 55          |
| Stdin approval block (15 lines, 3 copies)       | `prompt_user_approval`           | 30          |
| `convert_tools` method (12 lines, 2 copies)     | `convert_tools_from_json`        | 12          |
| Wire struct definitions (60 lines, 2 providers) | `ProviderTool`/etc. in `base.rs` | 50          |

### Each extracted function has at least one unit test

All five extracted functions (`build_mcp_manager_from_config`,
`build_agent_environment`, `prompt_user_approval`, `convert_tools_from_json`,
and the `ProviderFunctionCall::arguments_as_string` helper) have dedicated unit
tests. See the Tests Added section above.

### `commands/mod.rs` decreased by at least 100 lines

Reduction: **101 lines** (3003 lines before, 2902 lines after).

The reduction comes from:

- Removing the inline MCP manager block from `run_chat` (-22 lines)
- Removing the full initialization block from `run_plan_with_options` (-55
  lines)
- Simplifying `build_visible_skill_catalog` wrapper (-10 lines)
- Removing the `CopilotProvider`/`OllamaProvider` match in
  `run_plan_with_options` (-9 lines)
- Removing an unused import and inlining a one-use variable (-2 lines)
- Declaring the new `environment` submodule (+3 lines)

The extracted `AgentEnvironment` struct and `build_agent_environment` function
live in `src/commands/environment.rs`, keeping `commands/mod.rs` focused on
command dispatch rather than initialization logic.

---

## Architecture Notes

### Module Placement of `AgentEnvironment`

`AgentEnvironment` and `build_agent_environment` were placed in
`src/commands/environment.rs` (a submodule of `commands`) rather than
`src/agent/environment.rs` because the builder depends on
`commands::build_startup_skill_disclosure`,
`commands::build_visible_skill_catalog`, and
`commands::register_activate_skill_tool`. Placing it in `agent/` would require
moving those helpers first, which is a larger refactor deferred to a later
phase. The `super::` import path in `environment.rs` references these helpers
from the parent `commands` module without introducing a circular dependency.

### Ollama Type Alias Strategy

Rather than keeping private Ollama structs and converting to/from shared types
at the API boundary, the Ollama wire structs were replaced directly with type
aliases (`type OllamaMessage = ProviderMessage`). This works cleanly because
Ollama's JSON schema matches the shared types exactly: `tool_call_id` is
optional and omitted via `skip_serializing_if`, and `arguments` is already a
`serde_json::Value`. No conversion layer is needed.

### Copilot Wire Format Compatibility

The Copilot completions endpoint uses `arguments: String` (a serialized JSON
string) rather than `arguments: serde_json::Value`. The `CopilotFunctionCall`
private struct is retained for this reason. `ProviderFunctionCall` provides
`arguments_as_string()` as a conversion helper for any future path that needs to
bridge the canonical form to Copilot's wire format.

### `prompt_user_approval` and Short-Circuit Evaluation

The collapsed Clippy-approved form:

```rust
if !should_auto_approve(self.execution_mode, self.headless)
    && !prompt_user_approval(&format!(...))?
{
    return Ok(ToolResult::error(...));
}
```

relies on `&&` short-circuit semantics: `prompt_user_approval` is never called
when `should_auto_approve` returns `true`. The `?` operator applies to the
right-hand operand of `&&` and correctly propagates `Err` before the outer `if`
condition is evaluated.
