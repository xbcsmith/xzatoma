# Phase 4: Configuration and Observability Implementation

## Overview

Phase 4 adds operator-facing configuration control over how watcher backends
execute task-based plans, and improves per-task observability by recording LLM
iteration counts in both structured logs and result events. It builds on the
per-task execution loop introduced in Phases 2 and 3.

---

## What Was Added

### WatcherPlanExecutionMode Enum

A new `WatcherPlanExecutionMode` enum was added to `src/config.rs`:

```rust
pub enum WatcherPlanExecutionMode {
    PerTask,    // default
    SingleShot, // legacy
}
```

`PerTask` drives the per-task execution loop introduced in Phase 2: each task in
`plan.tasks` is sent as a separate `agent.execute` call within a shared agent
session. Conversation history accumulates so later tasks can reference outputs
from earlier ones.

`SingleShot` restores the pre-Phase-1 behaviour: the full plan is collapsed into
one string and sent to the agent in a single prompt. This is available for
operators who need backward-compatible execution or whose plans have no
structured tasks.

### WatcherExecutionConfig.execution_mode Field

The `execution_mode: WatcherPlanExecutionMode` field was added to
`WatcherExecutionConfig` in `src/config.rs`. It defaults to `PerTask` and
serializes as `per_task` / `single_shot` in YAML.

The `XZATOMA_WATCHER_EXECUTION_MODE` environment variable overrides the config
file value at runtime. Accepted values are `per_task` and `single_shot`.

### Agent::iteration_count() Accessor

A `last_iteration_count: usize` private field was added to the `Agent` struct in
`src/agent/core.rs`. After each call to `execute_with_observer` or
`execute_provider_messages_with_observer` completes, the final value of the
local `iteration` counter is stored in this field. The new public accessor
`Agent::iteration_count() -> usize` returns it.

This enables callers to observe how many LLM provider round-trips the most
recent `execute` call required, without threading an extra return value through
the existing API.

### TaskOutcome.iterations Field

The `iterations: usize` field was added to `TaskOutcome` in
`src/watcher/plan_executor.rs`. `execute_tasks_sequentially` captures
`agent.iteration_count()` immediately after each successful `agent.execute` call
and stores it in the outcome. Failed tasks record `0`.

The structured log emitted at task completion now includes `iterations` as a
field:

```text
info!(task_id, success, iterations, "Task execution complete")
```

The per-task outcome JSON emitted in both `GenericPlanResult.task_outcomes` and
the XZepr result event now includes an `"iterations"` key alongside `"id"`,
`"success"`, and `"summary"`.

### Watcher Branching on execution_mode

Both `src/watcher/generic/watcher.rs` and `src/watcher/xzepr/watcher.rs` were
updated to read `config.watcher.execution.execution_mode` and branch
accordingly:

```rust
let use_per_task = matches!(
    config.watcher.execution.execution_mode,
    WatcherPlanExecutionMode::PerTask
) && !plan.tasks.is_empty();
```

When `use_per_task` is false (either `SingleShot` mode, or `PerTask` with no
tasks), the single-shot path is taken: the plan is formatted via
`plan.to_instruction()` and sent as a single `agent.execute` call.

### Demo Configuration

`demos/watcher/generic/config.yaml` was updated to document the new field
explicitly:

```yaml
execution:
  execution_mode: per_task
```

`demos/watcher/generic/README.md` received a new `## Plan Execution Mode`
section explaining both modes, when to use each, and the environment variable
override.

---

## Deliverables Checklist

- [x] `WatcherPlanExecutionMode` enum in `src/config.rs` with YAML round-trip
- [x] `WatcherExecutionConfig.execution_mode` field with `PerTask` default
- [x] `XZATOMA_WATCHER_EXECUTION_MODE` env var override in `apply_env_vars`
- [x] Both watcher `execute_plan` methods branch on `execution_mode`
- [x] `Agent::iteration_count()` accessor in `src/agent/core.rs`
- [x] `TaskOutcome.iterations` field in `src/watcher/plan_executor.rs`
- [x] Structured per-task log lines include `iterations` at task end
- [x] Task outcome JSON includes `"iterations"` key
- [x] Demo config updated with `execution_mode: per_task`
- [x] Demo README updated with execution mode documentation
- [x] All quality gates pass (`cargo fmt`, `cargo check`, `cargo clippy`,
      `cargo test`)

---

## Verification

Setting `execution_mode: single_shot` in config or
`XZATOMA_WATCHER_EXECUTION_MODE=single_shot` reverts to Phase 0 behaviour: one
`agent.execute` call with the full plan as the prompt. The default `per_task`
drives the per-task loop from Phases 2 and 3, with per-task iteration counts
visible in logs and result events.
