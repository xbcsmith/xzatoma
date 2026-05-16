# Phase 3: Placeholder and Phase Reference Cleanup

## Overview

This document summarizes the complete Phase 3 codebase cleanup for XZatoma.
Phase 3 addressed four categories of technical debt in source comments, error
messages, API surfaces, and test names:

1. MCP task placeholder behavior (Task 3.1)
2. MCP sampling and elicitation stubs (Task 3.2)
3. Unsupported provider and ACP behavior classification (Task 3.3)
4. Stale phase terminology removal (Task 3.4)
5. Harmless placeholder language renaming (Task 3.5)

Detailed per-task documentation is available in the `docs/explanation/`
directory.

## Deliverables Status

All deliverables from the cleanup plan are complete.

### Task 3.1: MCP Task Placeholder

- `src/mcp/task_manager.rs`: "Phase 6 placeholder" label and all forward
  references to "Phase 6" removed. Module describes current scope and
  limitations.
- `src/mcp/mod.rs`: Description updated from "Phase 6 placeholder" to
  "Long-running task lifecycle tracking".
- `src/mcp/manager.rs`: `call_tool_as_task` now returns `XzatomaError::McpTask`
  when a server response includes `_meta.taskId` instead of silently returning a
  partial result.
- `tests/mcp_tool_bridge_test.rs`: Test updated to assert the typed error.

### Task 3.2: Sampling and Elicitation Wiring

- `src/mcp/manager.rs`: `McpClientManager` gained three new fields
  (`execution_mode`, `headless`, `sampling_provider`) and two new methods
  (`set_execution_context`, `set_sampling_provider`).
- `McpClientManager::connect` now registers `XzatomaElicitationHandler` when
  `elicitation_enabled` is true, and `XzatomaSamplingHandler` when
  `sampling_enabled` is true and a provider has been configured. Missing
  provider with `sampling_enabled` produces `XzatomaError::Mcp` rather than a
  silent warning.
- `src/mcp/elicitation.rs`: `handle_url` warning updated from "not yet
  implemented" to a stable description of why URL OAuth redirects require async
  notification flow.

### Task 3.3: Unsupported Feature Errors

- `src/providers/copilot.rs`: Messages endpoint arm in `complete()` returns
  `XzatomaError::UnsupportedEndpoint` instead of silently falling back to the
  completions endpoint. Image input error message updated to stable
  present-tense wording.
- `src/acp/runtime.rs`: Three artifact error messages changed from "not yet
  supported" to "unsupported". Phase labels removed from four doc comments.
- `src/config.rs`: Phase labels removed from four doc comment lines and one test
  section header.

### Task 3.4: Stale Phase Terminology

Phase labels were removed from production source comments, module docs, and test
names across all ACP, MCP, CLI, provider, and skills modules.

Files updated:

- `src/acp/events.rs`
- `src/acp/executor.rs`
- `src/acp/manifest.rs`
- `src/acp/mod.rs`
- `src/acp/routes.rs`
- `src/acp/run.rs`
- `src/acp/server.rs` (includes test rename:
  `test_manifest_contains_required_metadata`)
- `src/acp/session.rs`
- `src/acp/stdio.rs`
- `src/acp/streaming.rs`
- `src/agent/conversation.rs`
- `src/cli.rs` (test rename: `test_cli_parse_watch_with_extended_flags`)
- `src/commands/agent.rs`
- `src/commands/mod.rs`
- `src/commands/skills.rs`
- `src/config.rs`
- `src/mcp/auth/flow.rs`
- `src/mcp/manager.rs`
- `src/mcp/mod.rs`
- `src/mcp/task_manager.rs`
- `src/prompts/mod.rs`
- `src/providers/copilot.rs`
- `src/skills/activation.rs`
- `src/skills/disclosure.rs`
- `src/skills/discovery.rs`
- `src/skills/mod.rs`
- `src/skills/parser.rs`
- `src/skills/trust.rs`
- `src/skills/types.rs`
- `src/skills/validation.rs`
- `src/tools/activate_skill.rs`
- `src/tools/plan.rs`
- `src/watcher/generic/event.rs`
- `src/watcher/generic/matcher.rs`
- `src/watcher/generic/watcher.rs`
- `src/watcher/mod.rs`
- `src/watcher/xzepr/filter.rs`
- `src/watcher/xzepr/plan_extractor.rs`
- `src/watcher/xzepr/watcher.rs`
- `src/xzepr/mod.rs`

### Task 3.5: Placeholder Language

Three comment-level uses of "placeholder" describing runtime behavior (not
unfinished work) were renamed to more precise terms:

| File                            | Old                          | New                        |
| ------------------------------- | ---------------------------- | -------------------------- |
| `src/acp/prompt_input.rs`       | "text reference placeholder" | "text reference marker"    |
| `src/acp/available_commands.rs` | "descriptive placeholder"    | "descriptive display hint" |
| `src/mcp/tool_bridge.rs`        | "placeholder string"         | "fallback text"            |

## Success Criteria Verification

All success criteria from the plan are satisfied.

**Source comments no longer describe code as Phase 1 through Phase 6 work.**

Confirmed: all Phase-N labels in production source comments have been removed or
replaced with feature-based descriptions.

**Production code no longer exposes placeholder APIs that silently return
partial behavior.**

Confirmed:

- `call_tool_as_task` returns `XzatomaError::McpTask` instead of a partial
  result.
- The Copilot Messages endpoint returns `XzatomaError::UnsupportedEndpoint`
  instead of silently routing to a different endpoint.
- Sampling with no provider configured returns `XzatomaError::Mcp` at connect
  time instead of a silent warning.

**Unsupported provider, ACP, and MCP capabilities fail explicitly and are
covered by tests.**

Confirmed:

- `test_complete_returns_unsupported_endpoint_for_messages_model` in copilot
  tests.
- `test_task_support_required_routes_to_call_tool_as_task` updated to assert
  `McpTask` error.
- Existing ACP artifact rejection tests still pass with the updated error
  messages.
- New manager tests confirm handler wiring API.

**Remaining fallback text comments describe current behavior, not unfinished
implementation work.**

Confirmed: all updated comments describe what the code currently does.

## Quality Gate Results

```/dev/null/quality_gates.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

- 2126 unit tests: 0 failed
- All integration test suites: 0 failed

## Detailed Documentation

- Tasks 3.1 and 3.2:
  `docs/explanation/phase3_mcp_task_and_handler_wiring_implementation.md`
- Task 3.3:
  `docs/explanation/task_3_3_classify_unsupported_behaviour_implementation.md`
- Tasks 3.4 and 3.5:
  `docs/explanation/phase3_tasks_3_4_and_3_5_implementation.md`
