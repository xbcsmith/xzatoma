# Phase 3: Dead Code and Annotation Cleanup Implementation

This document summarizes the work completed for Phase 3 of the
[codebase cleanup plan](./codebase_cleanup_plan.md): removing confirmed-dead
public items, converting `run_chat` to an options struct, and beginning the
incremental visibility-tightening pass.

## Overview

Phase 3 is a zero-to-low-risk cleanup that shrinks the public surface. The
changes are purely subtractive (dead-code deletion) plus one signature refactor
and one module's worth of visibility tightening. No behavior changes were
introduced. All quality gates pass.

## Task 3.1: Dead Public Items Removed

### MCP `tasks/*` DTO cluster (`src/mcp/types.rs`)

The unimplemented MCP task/resource DTO cluster was deleted. These types had no
call sites anywhere in the crate:

- `CreateTaskResult`
- `TasksListResponse` (was exercised only by one integration test; see below)
- `TasksGetParams`
- `TasksResultParams`
- `TasksCancelParams`
- `TasksListParams`
- `ResourceTemplate`
- `CancelledParams`

`Task` and `TaskStatus` were intentionally kept per the plan (the parenthetical
deletion list did not include them). They remain exercised by
`test_task_roundtrip` and `test_task_status_serializes_snake_case`, so they are
not orphaned in the test surface.

The `test_tasks_list_response_roundtrip` integration test in
`tests/mcp_types_test.rs` and the `TasksListResponse` import were removed since
that type no longer exists.

### Confirmed-dead functions and methods

Each item below was verified dead by whole-crate reference counting (the only
match was its own definition; no call sites, doctests, or re-exports):

| Item                                            | Location                        |
| ----------------------------------------------- | ------------------------------- |
| `system_text_message`                           | `src/acp/runtime.rs`            |
| `Agent::new_boxed`                              | `src/agent/core.rs`             |
| `Agent::clear_transient_system_messages`        | `src/agent/core.rs`             |
| `FetchTool::with_rate_limit`                    | `src/tools/fetch.rs`            |
| `ToolRegistryBuilder::with_activate_skill_tool` | `src/tools/registry_builder.rs` |
| `TerminalTool::set_safety_mode`                 | `src/tools/terminal.rs`         |
| `ActiveSessionState::workspace_root`            | `src/acp/stdio.rs`              |
| `ActiveSessionState::provider_name`             | `src/acp/stdio.rs`              |
| `ActiveSessionState::current_model_name`        | `src/acp/stdio.rs`              |
| `ActiveSessionState::has_mcp_manager`           | `src/acp/stdio.rs`              |
| `ProviderMessage::ollama_native_images`         | `src/providers/types.rs`        |
| `create_summary_provider_if_needed`             | `src/commands/mod.rs`           |
| `create_provider_for_model`                     | `src/commands/mod.rs`           |

The last three (`ollama_native_images`, `create_summary_provider_if_needed`,
`create_provider_for_model`) were confirmed dead during the audit beyond the
originally named items and are removed here since Phase 3 is explicitly a
dead-code cleanup. `create_provider_for_model` was only called by
`create_summary_provider_if_needed` (transitively dead); both were remnants of
an unimplemented `/summarize` feature.

Total: 8 DTO types + 13 functions/methods = **21 dead public items removed**
(exceeding the plan's target of 19).

### Getters intentionally retained

`ActiveSessionState::last_activity` and
`ActiveSessionState::prompt_worker_finished` are uncalled getters, but each is
the sole reader of a private struct field (`last_activity` and
`prompt_worker_handle` respectively). Removing them would turn those fields
write-only and trigger `dead_code` under `clippy -D warnings`. Removing the
fields as well would be a larger, behavior- adjacent change (activity tracking /
worker-handle ownership), so both getters were retained to keep this phase
scoped to zero-risk deletions.

## Task 3.2: `run_chat` Converted to `RunChatOptions`

`commands::chat::run_chat` previously took eight positional arguments and
carried `#[allow(clippy::too_many_arguments)]` plus a dead `_safe` parameter.

It now takes a `RunChatOptions` struct:

```rust
#[derive(Debug, Clone, Default)]
pub struct RunChatOptions {
    pub provider_name: Option<String>,
    pub mode: Option<String>,
    pub resume: Option<String>,
    pub thinking_effort: Option<String>,
    pub system_prompt: Option<String>,
    pub streaming: bool,
}

pub async fn run_chat(mut config: Config, options: RunChatOptions) -> Result<()>
```

The dead `_safe` parameter was dropped entirely. The
`#[allow(clippy::too_many_arguments)]` attribute is gone. The struct mirrors the
existing `WatchCliOverrides` pattern in the same file (a `#[derive(Default)]`
struct with public fields constructed via struct-update syntax), satisfying the
"with Default/builder" requirement without introducing new unused builder
methods (which would themselves be dead code).

Call sites updated:

- `src/main.rs` (the `Commands::Chat` arm) now builds a `RunChatOptions`.
- `test_run_chat_unknown_provider` uses `RunChatOptions::default()`.

## Task 3.3: Visibility Tightening (First Module Batch)

The first batch of the incremental `pub` -> `pub(crate)` pass targeted
`src/tools/ide_tools.rs`. The seven IDE tool structs and their constructors were
downgraded from `pub` to `pub(crate)`:

- `IdeReadTextFileTool`, `IdeWriteTextFileTool`, `IdeOpenTerminalTool`,
  `IdeTerminalOutputTool`, `IdeWaitForTerminalExitTool`, `IdeKillTerminalTool`,
  `IdeRequestPermissionTool` (and each `new`).

These types are only ever constructed inside `register_ide_tools` (same module)
and are registered as `Arc<dyn ToolExecutor>`, so they were never part of the
externally reachable API in practice. They have no doctests and no cross-module
or test references. `register_ide_tools` itself remains `pub` because it is
called from `src/acp/stdio.rs` and carries a doctest.

`acp/tool_notifications.rs` was evaluated as a candidate but left unchanged: six
of its seven public functions carry external-crate doctests, so they are
legitimately part of the documented public surface. This confirms the pass must
remain incremental and per-item, and is tracked as an ongoing effort.

## Task 3.5: Documentation Updates

An audit of `docs/reference/` (including `mcp_configuration.md`, `api.md`, and
`subagent_api.md`) found no references to any of the deleted items. The MCP
`tasks/*` DTO cluster was never documented as a capability, and the removed
functions/IDE tool types were not named in the reference docs. No reference-doc
edits were required.

## Validation

All AGENTS.md Rule 4 quality gates pass:

- `cargo fmt --all`
- `cargo check --all-targets --all-features` — clean
- `cargo clippy --all-targets --all-features -- -D warnings` — clean
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth` —
  2342 passed, 0 failed
- `cargo test -p xzatoma --test mcp_types_test` — 37 passed, 0 failed
- `cargo test -p xzatoma --doc -- --skip providers::copilot --skip mcp::auth` —
  752 passed, 0 failed

## Annotation State

- No `#[allow(clippy::too_many_arguments)]` remains in `src/`.
- No `#[allow(dead_code)]`, `#[allow(unused_mut)]`, or `#[allow(deprecated)]`
  exist in `src/`.
- The remaining `#[allow(clippy::unwrap_used)]` /
  `#[allow(clippy::expect_used)]` attributes are Phase 2 SAFETY-justified
  suppressions, and `#[allow(clippy::module_inception)]` on
  `watcher/xzepr/mod.rs` is intentional.
