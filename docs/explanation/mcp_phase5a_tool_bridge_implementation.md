# MCP Phase 5A: Tool Bridge, Resources, and Prompts

## Overview

Phase 5A implements the bridge layer between MCP servers and Xzatoma's internal
`ToolRegistry`. It introduces three `ToolExecutor` adapters, a unified
auto-approval policy, and the `register_mcp_tools` helper used by both the `run`
and `chat` commands.

## Components Delivered

| File                                                         | Action                                                      |
| ------------------------------------------------------------ | ----------------------------------------------------------- |
| `src/mcp/approval.rs`                                        | Created -- auto-approval policy function                    |
| `src/mcp/tool_bridge.rs`                                     | Created -- three ToolExecutor adapters                      |
| `src/mcp/mod.rs`                                             | Updated with `pub mod approval;` and `pub mod tool_bridge;` |
| `tests/mcp_tool_bridge_test.rs`                              | Created -- 18 integration tests                             |
| `docs/explanation/mcp_phase5a_tool_bridge_implementation.md` | Created                                                     |

## Design Decisions

### Single Authoritative Approval Policy

`src/mcp/approval.rs` contains the only definition of `should_auto_approve`. No
other module may embed inline approval logic. All three executors call this
function before contacting an MCP server.

The policy is deliberately minimal:

```rust
pub fn should_auto_approve(execution_mode: ExecutionMode, headless: bool) -> bool {
    headless || execution_mode == ExecutionMode::FullAutonomous
}
```

`RestrictedAutonomous` and `Interactive` modes in a non-headless context both
require a confirmation prompt. The `headless` flag overrides all modes because
the `run` command executing a plan cannot block on stdin.

### Double-Underscore Namespacing

Every MCP tool is registered under `format!("{}__{}", server_id, tool_name)`. A
single underscore is insufficient because both `server_id` and `tool_name` may
independently contain underscores. The double-underscore separator is
unambiguous and makes the server origin visible in tool lists.

### Three ToolExecutor Adapters

#### McpToolExecutor

Wraps a single `tools/call` tool from a specific server. The key fields are:

- `registry_name`: `format!("{}__{}", server_id, tool_name)` -- always set at
  construction, never derived at call time.
- `task_support`: when `Some(TaskSupport::Required)` the executor routes through
  `McpClientManager::call_tool_as_task` instead of `call_tool`.
- `structured_content`: appended after a `"---"` delimiter in the output string
  so callers can parse it separately if needed.

#### McpResourceToolExecutor

Registered permanently under `"mcp_read_resource"`. Accepts `server_id` and
`uri` arguments. Delegates to `McpClientManager::read_resource`.

#### McpPromptToolExecutor

Registered permanently under `"mcp_get_prompt"`. Accepts `server_id`,
`prompt_name`, and optional `arguments`. Delegates to
`McpClientManager::get_prompt` and formats the response messages as
`"[ROLE]\ncontent"` blocks separated by blank lines.

### register_mcp_tools

The function holds the manager read lock only long enough to copy the
`(server_id, tools)` pairs, then drops it before mutating the registry. This
prevents a deadlock if an executor's `execute` method is called while
registration is still in progress in another task.

The function always registers `mcp_read_resource` and `mcp_get_prompt`
regardless of how many servers are connected, so callers can always rely on
these tools being present after calling `register_mcp_tools`.

A `tracing::warn!` is emitted before overwriting an existing registry entry,
enabling operators to detect misconfigured server IDs.

## Module Structure

```text
src/mcp/
    approval.rs       -- should_auto_approve (single policy source)
    tool_bridge.rs    -- McpToolExecutor
                      -- McpResourceToolExecutor
                      -- McpPromptToolExecutor
                      -- register_mcp_tools
                      -- extract_text_content (private)
                      -- format_prompt_messages (private)
```

## Confirmation Prompt Flow

When `should_auto_approve` returns `false`:

1. Print to stderr:
   `"MCP tool call: <server>/<tool> with args: <json>. Allow? [y/N] "`
2. Flush stderr.
3. Read one line from stdin via `std::io::BufRead::read_line`.
4. If the trimmed, lowercased response is not `"y"` or `"yes"`, return
   `ToolResult::error("User rejected MCP tool call: <registry_name>")`.
5. Otherwise proceed with the server call.

Resource and prompt executors follow the same prompt pattern with slightly
different messages.

## Testing

### Inline Unit Tests (`src/mcp/tool_bridge.rs`)

16 tests covering:

- `should_auto_approve` policy for all three execution modes and both headless
  values.
- `registry_name` double-underscore format.
- `tool_definition` JSON structure (`name`, `description`, `parameters` keys
  present).
- `extract_text_content` helper (joins text, skips non-text, empty slice).
- `format_prompt_messages` helper (single message, multiple messages separated
  by blank line).
- `register_mcp_tools` with no servers returns zero and still registers the two
  built-in executors.

### Inline Unit Tests (`src/mcp/approval.rs`)

8 tests covering all combinations of execution mode and headless flag.

### Integration Tests (`tests/mcp_tool_bridge_test.rs`)

18 tests using `FakeTransport`-backed wired protocols:

| Test                                                               | What it verifies                                     |
| ------------------------------------------------------------------ | ---------------------------------------------------- |
| `test_should_auto_approve_false_in_full_autonomous_mode`           | `FullAutonomous` + `headless=false` => `true`        |
| `test_should_auto_approve_true_when_headless`                      | `headless=true` always approves                      |
| `test_should_auto_approve_false_in_interactive_mode`               | `Interactive` + `headless=false` => `false`          |
| `test_should_auto_approve_false_in_restricted_autonomous_mode`     | `RestrictedAutonomous` + `headless=false` => `false` |
| `test_registry_name_uses_double_underscore_separator`              | `my_server__search_items`                            |
| `test_registry_name_double_underscore_with_underscored_ids`        | Compound underscored IDs                             |
| `test_tool_definition_format_matches_xzatoma_convention`           | Three keys present                                   |
| `test_tool_definition_name_equals_registry_name`                   | `def["name"]` == `registry_name`                     |
| `test_execute_returns_success_for_text_response`                   | `ToolResult.success == true`, text present           |
| `test_execute_maps_is_error_to_error_result`                       | `isError:true` => `ToolResult.success == false`      |
| `test_structured_content_appended_after_delimiter`                 | `"---"` present, JSON after delimiter                |
| `test_task_support_required_routes_to_call_tool_as_task`           | `"task"` field in JSON-RPC params                    |
| `test_no_task_support_does_not_include_task_field_in_params`       | No `"task"` field for plain call                     |
| `test_register_mcp_tools_with_no_servers_registers_zero_mcp_tools` | count=0, built-ins present                           |
| `test_register_mcp_tools_registers_one_executor_per_tool`          | count=2 for two tools                                |
| `test_register_mcp_tools_executor_definition_uses_namespaced_name` | Definition carries `__` name                         |
| `test_register_mcp_tools_multiple_servers_all_registered`          | Two servers, two tools total                         |
| `test_execute_headless_interactive_does_not_prompt`                | Headless path completes without blocking stdin       |

## Validation Results

All quality gates pass:

```text
cargo fmt --all                                          -- clean
cargo check --all-targets --all-features                -- 0 errors
cargo clippy --all-targets --all-features -- -D warnings -- 0 warnings
cargo test --all-features --lib mcp::approval           -- 8 passed
cargo test --all-features --lib mcp::tool_bridge        -- 16 passed
cargo test --all-features --test mcp_tool_bridge_test   -- 18 passed
```

The single pre-existing failure
(`providers::copilot::tests::test_copilot_config_defaults`) is unrelated to
Phase 5A; it is a stale model-name assertion that predates this phase.

## Success Criteria Review

| Criterion                                                                                      | Status                              |
| ---------------------------------------------------------------------------------------------- | ----------------------------------- |
| `registry_name` is always `<server_id>__<tool_name>`                                           | Verified by 3 tests                 |
| `tool_definition()` returns `{ "name", "description", "parameters" }`                          | Verified by 2 tests                 |
| `should_auto_approve` returns `true` in `FullAutonomous` and headless contexts                 | Verified by 4 tests                 |
| `should_auto_approve` returns `false` in `Interactive` and `RestrictedAutonomous` non-headless | Verified by 3 tests                 |
| `structured_content` appended after `"---"` delimiter                                          | Verified by 1 test                  |
| Confirmation prompt shown when `should_auto_approve` is `false`                                | Verified by headless inversion test |
| All Phase 5A tests pass                                                                        | 42 tests pass (8 + 16 + 18)         |
