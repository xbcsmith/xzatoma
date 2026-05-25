# Phase 3 Codebase Cleanup: Tasks 3.1 and 3.2

## Overview

This document describes the MCP changes applied as part of Phase 3 of the
XZatoma codebase cleanup.

- Task 3.1: Resolve MCP task placeholder behavior.
- Task 3.2: Wire or remove MCP sampling and elicitation stubs.

## Task 3.1: Resolve MCP Task Placeholder Behavior

### Decision

Long-running MCP task polling (registering with `tasks/get`, awaiting a terminal
state, delivering the final result to the caller via an async channel) is out of
scope for this cleanup cycle. The decision taken was to make the unsupported
behavior explicit:

- When `McpClientManager::call_tool_as_task` receives a server response that
  contains `_meta.taskId`, it now returns `XzatomaError::McpTask` with a message
  that identifies the server, tool name, and task ID.
- The previous behavior was to log a `tracing::warn!` and return the partial
  initial response as if the call had completed. That was silent and misleading.

### Files Changed

#### `src/mcp/task_manager.rs`

- Module doc rewritten. The "Phase 6 placeholder" label and "Planned API"
  section were replaced with a "Scope" section that explains what the module
  currently does (state registration, tracking, removal) and what requires
  additional wiring (async result delivery via notification callbacks).
- `TaskLifecycleState` doc: Removed the forward reference to "Phase 6 wiring".
  States now simply mirror the MCP 2025-11-25 spec values.
- `TaskEntry` doc: Removed the forward reference to a future `oneshot::Sender`.
  Replaced with a neutral "a future iteration could add" note.
- `TaskManager` struct doc: Removed the "Phase 6 placeholder" self-description.
- `update_task_state` doc: Removed "Phase 6 will extend this". Added "A future
  iteration can add waiter wake-up logic here."

#### `src/mcp/mod.rs`

- `task_manager` description changed from "Long-running task tracking (Phase 6
  placeholder)" to "Long-running task lifecycle tracking".

#### `src/mcp/manager.rs`

- `task_manager` field doc: Removed "Phase 6 placeholder" label and the inline
  `// Retained for Phase 6:` comment. The doc now describes the field's live
  responsibility (keeping the `Arc<Mutex>` alive for notification callbacks).
- `call_tool_as_task`: Replaced the `tracing::warn!` + partial-result-return
  block with an explicit typed error:

```xzatoma/src/mcp/manager.rs#L696-L712
        if let Some(task_id) = response
            .meta
            .as_ref()
            .and_then(|m| m.get("taskId"))
            .and_then(|v| v.as_str())
        {
            return Err(XzatomaError::McpTask(format!(
                "server '{}' tool '{}' returned long-running task '{}'; \
                 task polling is not supported in this session",
                server_id, tool_name, task_id
            )));
        }
```

#### `tests/mcp_tool_bridge_test.rs`

- `test_task_support_required_routes_to_call_tool_as_task` updated to expect the
  new `Err(McpTask(...))` result. The test still verifies:
  1. The outbound request contains a `task` field (routing to
     `call_tool_as_task` rather than `call_tool`).
  2. The error string contains the task ID (`task-abc-123`) and the limitation
     phrase (`task polling is not supported`).
- Module doc de-phased from "Integration tests for Phase 5A" to a feature-based
  description.

## Task 3.2: Wire MCP Sampling and Elicitation Stubs

### Decision

The `XzatomaSamplingHandler` (in `src/mcp/sampling.rs`) and
`XzatomaElicitationHandler` (in `src/mcp/elicitation.rs`) were fully implemented
but not wired into the connection setup. `McpClientManager::connect` contained
two `tracing::warn!` stubs saying "not yet implemented". This task replaced
those stubs with real handler registration.

### New Fields on `McpClientManager`

Three new fields were added:

| Field               | Type                           | Default                      | Purpose                          |
| ------------------- | ------------------------------ | ---------------------------- | -------------------------------- |
| `execution_mode`    | `crate::config::ExecutionMode` | `ExecutionMode::Interactive` | Forwarded to both handlers       |
| `headless`          | `bool`                         | `false`                      | Forwarded to both handlers       |
| `sampling_provider` | `Option<Arc<dyn Provider>>`    | `None`                       | Required by the sampling handler |

### New Public Methods

Two new methods were added:

```xzatoma/src/mcp/manager.rs#L230-L295
pub fn set_execution_context(
    &mut self,
    execution_mode: crate::config::ExecutionMode,
    headless: bool,
) { ... }

pub fn set_sampling_provider(&mut self, provider: Arc<dyn crate::providers::Provider>) { ... }
```

Both must be called before `connect` on any server that has the corresponding
capability enabled.

### Elicitation Wiring

When `McpServerConfig::elicitation_enabled` is `true`, `connect` now creates and
registers an `XzatomaElicitationHandler`:

```xzatoma/src/mcp/manager.rs#L403-L413
        if config.elicitation_enabled {
            let handler = Arc::new(crate::mcp::elicitation::XzatomaElicitationHandler {
                execution_mode: self.execution_mode,
                headless: self.headless,
                browser_opener: crate::mcp::elicitation::open_browser,
            });
            protocol.register_elicitation_handler(handler);
            tracing::debug!(id = %id, "Registered elicitation handler");
        }
```

The handler cancels elicitation requests in headless or `FullAutonomous` mode
and collects interactive form input otherwise. URL-mode elicitation opens the
browser but always returns `Cancel` because the OAuth redirect flow requires a
notification-based async callback.

### Sampling Wiring

When `McpServerConfig::sampling_enabled` is `true`, `connect` checks for a
configured provider:

- **Provider present**: Creates and registers `XzatomaSamplingHandler`, which
  forwards `sampling/createMessage` requests to the configured AI provider.
- **Provider absent**: Marks the entry as `Failed` and returns
  `XzatomaError::Mcp` with a clear message telling the operator to call
  `set_sampling_provider` before connecting.

```xzatoma/src/mcp/manager.rs#L383-L402
        if config.sampling_enabled {
            match &self.sampling_provider {
                Some(provider) => {
                    let handler = Arc::new(crate::mcp::sampling::XzatomaSamplingHandler {
                        provider: Arc::clone(provider),
                        execution_mode: self.execution_mode,
                        headless: self.headless,
                    });
                    protocol.register_sampling_handler(handler);
                    tracing::debug!(id = %id, "Registered sampling handler");
                }
                None => {
                    ...
                    return Err(XzatomaError::Mcp(...));
                }
            }
        }
```

### Updated `handle_url` Comment in `src/mcp/elicitation.rs`

The `tracing::warn!` in `handle_url` previously said "async callback flow not
yet implemented". It now says "URL OAuth redirect handling requires a
notification-based async callback flow that is not active in this session",
which describes the current state rather than implying imminent completion.

### Tests Added

Four tests were added to `src/mcp/manager.rs`:

- `test_set_execution_context_stores_values`: Verifies that calling
  `set_execution_context` does not disturb `sampling_provider`.
- `test_set_execution_context_interactive_non_headless`: Verifies the
  `Interactive` / non-headless variant.
- `test_set_sampling_provider_stores_provider`: Verifies that
  `set_sampling_provider` stores the provider via `sampling_provider.is_some()`.
- `test_new_manager_has_default_execution_context`: Documents and verifies the
  default state of a freshly constructed manager.

## Verification

All four quality gates pass after these changes:

```/dev/null/quality_gates.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

- 2126 unit tests pass, 0 fail.
- 18 integration tests in `tests/mcp_tool_bridge_test.rs` pass, including the
  updated `test_task_support_required_routes_to_call_tool_as_task`.
