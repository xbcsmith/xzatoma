# Phase 8: Live Plan Tracking

## Overview

Phase 8 adds live plan-tracking to ACP sessions. When the model emits a numbered
list at the start of a multi-step task (e.g.,
`1. Read the file\n2. Edit the function\n3. Run tests`), XZatoma parses those
items in real time and sends `SessionUpdate::Plan` notifications to Zed. Zed
renders them as a live task-checklist panel.

## Architecture

Three components work together to detect, track, and publish plan entries.

### `PlanTracker` (`src/agent/plan_tracker.rs`)

A new standalone struct that accumulates streamed text, detects numbered-list
items matching `^\d+\.\s+.+`, and tracks their status through three transitions:

- `Pending` - first detected
- `InProgress` - when a later entry arrives
- `Completed` - after `finalize()` is called at turn end

Tracking stops after `on_tool_call_started()` to prevent tool output from being
misidentified as plan steps.

### `AcpSessionObserver` (`src/acp/stdio.rs`)

Gained a `plan_tracker` field. Three `on_event` arms are wired:

- `AssistantTextEmitted` - passes each chunk to `plan_tracker.update()`; emits
  `SessionUpdate::Plan` when the plan changes.
- `ToolCallStarted` - calls `plan_tracker.on_tool_call_started()` before the
  existing tool notification.
- `ExecutionCompleted` - calls `plan_tracker.finalize()` and emits a final
  `SessionUpdate::Plan` if entries exist.

### `ACP_PLAN_INSTRUCTION` (`src/acp/stdio.rs`)

A constant system-prompt fragment injected into every new ACP session that
instructs the model to begin multi-step responses with a numbered list. A
deduplication guard prevents double-injection for resumed sessions.

## Status Flow

```text
Initial:             [ ]
1. Step A detected:  [A:Pending]
2. Step B detected:  [A:InProgress, B:Pending]
3. Step C detected:  [A:InProgress, B:InProgress, C:Pending]
4. finalize():       [A:Completed, B:Completed, C:Completed]
```

Each newly detected item promotes the previous `Pending` entry to `InProgress`.
After the turn ends, `finalize()` promotes all remaining entries to `Completed`.

## Files Changed

| File                        | Change                                                                                                                                                     |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/agent/plan_tracker.rs` | New file: `PlanTracker` struct and all methods                                                                                                             |
| `src/agent/mod.rs`          | Added `pub mod plan_tracker;`                                                                                                                              |
| `src/acp/stdio.rs`          | `ACP_PLAN_INSTRUCTION` constant; `plan_tracker` field on `AcpSessionObserver`; wired `on_event` for 3 arms; plan instruction injection in `create_session` |

## Tests

| Test name                                                   | Location                    | What it verifies                                                                                                    |
| ----------------------------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `test_plan_tracker_detects_numbered_list_items`             | `src/agent/plan_tracker.rs` | `update()` returns `true` and adds an entry when text contains a line matching `^\d+\.\s+.+`                        |
| `test_plan_tracker_returns_false_for_plain_text`            | `src/agent/plan_tracker.rs` | `update()` returns `false` and adds no entries for plain prose text                                                 |
| `test_plan_tracker_ignores_text_after_tool_call`            | `src/agent/plan_tracker.rs` | After `on_tool_call_started()` fires, subsequent numbered-list text is not detected                                 |
| `test_plan_tracker_promotes_entries_to_in_progress`         | `src/agent/plan_tracker.rs` | Detecting a second item promotes the first from `Pending` to `InProgress`                                           |
| `test_plan_tracker_finalize_promotes_all_to_completed`      | `src/agent/plan_tracker.rs` | `finalize()` returns `true` and sets every entry status to `Completed`                                              |
| `test_plan_tracker_finalize_returns_false_when_no_entries`  | `src/agent/plan_tracker.rs` | `finalize()` returns `false` when no entries have been detected                                                     |
| `test_plan_tracker_reset_clears_entries_and_buffer`         | `src/agent/plan_tracker.rs` | `reset()` empties the entry list, clears the text buffer, and re-enables detection                                  |
| `test_plan_tracker_ignores_duplicate_items`                 | `src/agent/plan_tracker.rs` | Feeding the same numbered line twice does not create a duplicate entry                                              |
| `test_acp_observer_emits_plan_update_on_numbered_list_text` | `src/acp/stdio.rs`          | `on_event(AssistantTextEmitted)` causes a `SessionUpdate::Plan` notification when a numbered item is detected       |
| `test_acp_observer_plan_tracker_stops_on_tool_call_started` | `src/acp/stdio.rs`          | `on_event(ToolCallStarted)` calls `on_tool_call_started()` so subsequent text does not produce further plan entries |
| `test_acp_observer_finalize_on_execution_completed`         | `src/acp/stdio.rs`          | `on_event(ExecutionCompleted)` calls `finalize()` and emits a final `SessionUpdate::Plan` when entries are present  |
| `test_acp_plan_instruction_constant_is_not_empty`           | `src/acp/stdio.rs`          | `ACP_PLAN_INSTRUCTION` is a non-empty string                                                                        |

## Design Decisions

### Why restrict plan tracking to items before the first tool call?

Tool calls produce structured output (file contents, terminal output) that may
contain arbitrary numbered lists unrelated to the agent's plan. Including
post-tool content would cause spurious plan entries that confuse the user. The
`post_tool_call` flag permanently disables detection after
`on_tool_call_started()` fires.

### Why `PlanEntryPriority::Medium` for all detected entries?

The agent's plain-text numbered list carries no explicit priority metadata.
Using `Medium` for all entries avoids false high/low-priority signals and keeps
the plan display uniform. Finer-grained priority would require structured output
from the model (e.g., a dedicated JSON field), which is outside the scope of
Phase 8.

### Why emit `SessionUpdate::Plan` incrementally instead of only at the end?

Incremental emission lets Zed's checklist panel update in real time as the agent
streams its numbered list. A single end-of-turn emission would show nothing
until the full response arrived, defeating the purpose of the live panel.
