# Phase 2: Per-Task Execution Loop Implementation

## Overview

Phase 2 adds a per-task execution loop to the generic watcher. Instead of
collapsing all plan tasks into a single LLM prompt, the watcher now drives the
agent through each task individually in a shared session. Task context
accumulates across turns: the agent's conversation history from task N is
visible when it executes task N+1, enabling multi-step plans that build on
prior outputs.

## Changes

### `src/tools/plan.rs` — `resolve_task_order`

New public function that performs a topological sort of `PlanTask` dependencies
using Kahn's BFS algorithm. Returns tasks in an order where every dependency
executes before its dependents. Returns `Err` on unknown dependency IDs or
cycles.

### `src/watcher/generic/result_event.rs` — `GenericPlanResult.task_outcomes`

New optional field `task_outcomes: Option<Vec<serde_json::Value>>` added to
`GenericPlanResult`. Each entry contains `id`, `success`, and `summary` for one
task. The field is omitted from the serialised JSON when `None` (step-based
plans).

### `src/watcher/generic/watcher.rs` — `TaskOutcome` + `execute_tasks_sequentially` + updated `execute_plan`

- `TaskOutcome` struct: captures `id`, `success`, and `summary` per task.
- `execute_tasks_sequentially`: drives the agent through each task in dependency
  order. Failures are recorded and execution continues rather than aborting.
- `execute_plan` branching: when `plan.tasks` is non-empty the per-task loop
  runs; when only `plan.steps` exist the legacy single-shot path is used.
  After all tasks complete a final summarisation call produces the result
  `summary` field.

## Deliverables

- `resolve_task_order` in `src/tools/plan.rs` with 7 unit tests.
- `execute_tasks_sequentially` in `src/watcher/generic/watcher.rs`.
- `execute_plan` branches on `plan.tasks` vs `plan.steps`.
- `GenericPlanResult` serialises `task_outcomes` when present.
- All 2228 lib tests pass; full test suite exits 0.
