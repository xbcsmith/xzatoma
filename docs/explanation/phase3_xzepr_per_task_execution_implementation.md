# Phase 3: XZepr Per-Task Execution Loop Implementation

## Overview

Phase 3 extracts the per-task execution logic added in Phase 2 into a shared
module (`plan_executor`) and extends it to the XZepr watcher. Both watcher
backends now drive a single agent session through a task-based plan one task
at a time, with per-task outcomes recorded in the result event.

## Changes

### `src/watcher/plan_executor.rs` (new file)

Canonical home for `TaskOutcome` and `execute_tasks_sequentially`. Both were
previously private to `GenericWatcher`. Moving them here lets the XZepr watcher
reuse the same logic without duplication.

`execute_tasks_sequentially(plan: &Plan, agent: &mut Agent) -> Result<Vec<TaskOutcome>>`
is a public free async function. It resolves task order, drives the agent
through each task, records outcomes, and continues on failure rather than
aborting.

### `src/watcher/mod.rs`

Added `pub mod plan_executor` and updated the module-level doc to describe the
new shared module.

### `src/watcher/generic/watcher.rs`

- Removed the `TaskOutcome` struct definition and `execute_tasks_sequentially`
  method (both moved to `plan_executor`).
- Added `use crate::watcher::plan_executor::execute_tasks_sequentially`.
- Updated the call site in `execute_plan` from `self.execute_tasks_sequentially`
  to the free function `execute_tasks_sequentially`.

### `src/watcher/xzepr/watcher.rs`

Task 3.1 and 3.3:

- Before creating the agent, attempts to parse `plan_yaml` into a `Plan` using
  `PlanParser::from_yaml`.
- If parsing succeeds and `plan.tasks` is non-empty: calls
  `execute_tasks_sequentially`, requests a final summary, and returns per-task
  outcomes in the result event's `task_outcomes` field.
- If parsing succeeds but `plan.tasks` is empty: calls `agent.execute` with
  `plan.to_instruction()` (legacy step-based path).
- If parsing fails: logs a warning and falls back to passing the raw YAML
  string as a single-shot prompt.
- The spawn block return type changed from `Result<()>` to
  `Result<(bool, String, Option<Vec<serde_json::Value>>)>` to carry per-task
  outcomes back to the result-publishing code.

## Deliverables

- `src/watcher/plan_executor.rs` with `TaskOutcome` and
  `execute_tasks_sequentially`, including 3 unit tests.
- Both watchers import from `plan_executor`.
- XZepr watcher parses plan YAML and uses per-task loop when tasks are present.
- All 2231 lib tests pass; full test suite exits 0.
