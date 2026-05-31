# Watcher Session Execution

## Overview

XZatoma watchers execute plans received as Kafka CloudEvents. Before the
autonomous execution model was introduced, plans were collapsed into a single
LLM prompt, causing the model to summarize tasks rather than run them. This
document explains the autonomous execution model: the watcher system prompt, the
per-task execution loop, dependency resolution, and how the two watcher backends
differ.

---

## Why the Autonomous System Prompt Is Necessary

The default XZatoma chat mode (`ChatMode::Planning`) tells the LLM to think and
describe a plan of action. In that mode the LLM responds to a list of tasks with
prose: "I would first run X, then Y." It does not call tools.

Watcher execution requires a fundamentally different mode. When a plan arrives
over Kafka there is no human in the loop to confirm or redirect. The LLM must
act immediately, call tools to perform work, and report results — not ask for
clarification.

`ChatMode::Watcher` solves this by injecting an autonomous system prompt before
every agent call. The prompt establishes three behaviours:

- **Act, do not describe**: the LLM must call tools immediately rather than
  narrating what it would do.
- **No confirmation**: the LLM must not pause to ask for user approval. There is
  no user to answer.
- **Report outcomes**: when all tasks are done the LLM must respond with a short
  paragraph summarising what happened.

Without this system prompt, the watcher completes in one iteration with no tool
calls regardless of what is in the plan.

---

## How the Per-Task Loop Works

### From single-shot to per-task

The legacy single-shot mode sends the entire plan as a single numbered-list
prompt. Multi-step plans fail with small or quantised models because the LLM
must mentally track all state across many steps in one pass.

The per-task loop breaks the plan into individual `agent.execute` calls, one per
task, all within the same agent session. The session preserves conversation
history, so the agent can see what earlier tasks produced when executing later
ones.

### Execution sequence

1. A Kafka CloudEvent is consumed and the `Plan` is extracted from `data`.
2. If `plan.tasks` is non-empty and `execution_mode` is `per_task`:
   1. The task list is sorted by the dependency resolver (see below).
   2. The agent is constructed with `ChatMode::Watcher`.
   3. For each task in order, `agent.execute(task.description)` is called.
   4. The outcome (success, summary, iteration count) is recorded.
   5. A final `agent.execute` call requests a one-paragraph summary.
3. If `execution_mode` is `single_shot`, or the plan has no tasks (only
   `steps`), the full instruction is sent in one `agent.execute` call.
4. The result event, including per-task outcomes, is published to Kafka.

### Context accumulation

Every `agent.execute` call within a single plan execution appends messages to
the same `Conversation`. The agent's conversation history grows as tasks
complete. When task N runs, it can see:

- The system prompt from `ChatMode::Watcher`.
- The user and assistant messages from tasks 1 through N-1.
- The tool call results from those earlier tasks.

This lets a later task reference a file path discovered by an earlier one, or
use the exit code of a command run in a previous step, without the operator
having to thread outputs as inputs.

### Per-task outcome data

Each task produces a `TaskOutcome` with three fields:

- `id`: the task identifier from the plan.
- `success`: whether the agent returned without error.
- `summary`: the agent's final response text, or an error description.
- `iterations`: the number of LLM provider round-trips for this task.

These are serialised into the `task_outcomes` array of the result event:

```json
{
  "task_outcomes": [
    {
      "id": "setup",
      "success": true,
      "summary": "Created tmp/",
      "iterations": 2
    },
    {
      "id": "build",
      "success": true,
      "summary": "Compiled binary",
      "iterations": 4
    }
  ]
}
```

Failed tasks (`success: false`) record `iterations: 0`.

### Failure behaviour

A task failure does not abort the plan. `execute_tasks_sequentially` continues
to the next task regardless. The overall plan is marked failed only if at least
one task failed. This allows later tasks that do not depend on the failed task
to still complete.

---

## How Task Dependencies Are Resolved

The `dependencies` field on each `PlanTask` is a list of task `id` values that
must run before this task. The resolver in `src/tools/plan.rs` performs a
topological sort using Kahn's algorithm:

1. Build a map from task `id` to `PlanTask`.
2. Compute the in-degree (number of unresolved dependencies) for each task.
3. Seed the ready queue with all tasks that have zero dependencies.
4. Pop a task from the ready queue, add it to the ordered list, and decrement
   the in-degree of every task that depends on it.
5. When a task's in-degree reaches zero, add it to the ready queue.
6. If the output list is shorter than the input list, a cycle exists — return an
   error.

Tasks with no dependencies are placed in the output in the order they appear in
the original list (stable relative order). This means a plan with no
`dependencies` fields at all executes in declaration order, which is the
expected legacy behaviour.

### Error conditions

The resolver returns `Err` in two cases:

- **Unknown dependency**: a task lists a `dependency` id that does not match any
  task `id` in the plan.
- **Cycle**: two or more tasks form a circular dependency chain.

Both conditions are reported before any task executes.

---

## Generic vs XZepr Execution Paths

Both watcher backends follow the same execution model but differ in how they
receive the plan.

### Generic watcher

The generic watcher (`src/watcher/generic/watcher.rs`) receives a **CloudEvents
1.0** JSON envelope. The `Plan` is embedded in the `data` field as a structured
JSON object and is parsed directly via `GenericPlanEvent`. The watcher reads
`config.watcher.execution.execution_mode` and branches to the per-task loop or
single-shot path before building the agent.

### XZepr watcher

The XZepr watcher (`src/watcher/xzepr/watcher.rs`) receives an XZepr-specific
CloudEvents envelope. The plan is stored as a YAML string in the `data` field
and must be parsed from YAML before execution can begin.

The XZepr execution path:

1. Attempt to parse `plan_yaml` via `PlanParser::from_yaml`.
2. On success: apply the same `execution_mode` branch as the generic watcher.
3. On parse failure: fall back to single-shot using the raw YAML string as the
   prompt, with a warning log. This preserves backward compatibility with
   hand-crafted or non-standard YAML payloads.

### Shared executor

Both backends call `execute_tasks_sequentially` from
`src/watcher/plan_executor.rs`. This shared module owns `TaskOutcome` and the
full per-task loop. Neither watcher duplicates the loop logic.

### Result event differences

Both backends produce a `GenericPlanResult` with:

- `success`: overall plan success.
- `summary`: the agent's final one-paragraph summary.
- `task_outcomes`: per-task outcome array (present only when `execution_mode` is
  `per_task` and the plan has tasks).

---

## Configuration Reference

```yaml
watcher:
  execution:
    execution_mode: per_task # per_task (default) or single_shot
    execution_timeout_secs: 300
    max_concurrent_executions: 1
```

Environment variable override:

```bash
export XZATOMA_WATCHER_EXECUTION_MODE=per_task
```

---

## Further Reading

- `docs/how-to/setup_watcher.md` — configuration recipes and CLI examples
- `src/watcher/plan_executor.rs` — shared executor rustdoc
- `src/tools/plan.rs` — `resolve_task_order` rustdoc
- `src/prompts/watcher_prompt.rs` — autonomous system prompt text
