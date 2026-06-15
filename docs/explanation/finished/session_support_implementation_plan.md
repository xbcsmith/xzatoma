# Watcher Session Support Implementation Plan

## Overview

XZatoma watchers currently send an entire plan as a single LLM prompt via
`agent.execute(instruction)`. The agent completes in one iteration because the
LLM treats the instruction as a description to summarize, not a sequence of
tasks to execute with tools. Two root causes: (1) no autonomous system prompt
telling the LLM to act without human confirmation, and (2) multi-task plans
are collapsed into a single string, giving the LLM no structured boundary per
task. This plan adds a watcher-specific autonomous system prompt and a
per-task execution loop that runs tasks sequentially in a shared agent session,
preserving context across tasks.

---

## Current State Analysis

### Existing Infrastructure

- **`Agent::execute(prompt)`** — `src/agent/core.rs:505`. Adds `prompt` as a
  single user message and enters the multi-turn tool-call loop. The loop
  already supports unlimited turns up to `config.max_turns`; the problem is the
  LLM never calls tools in the first place.
- **`run_plan_with_options`** — `src/commands/mod.rs:1972`. Creates a fresh
  agent, builds the instruction from the plan, calls `agent.execute(task)` once.
  Used by both watchers.
- **`execute_plan` (generic)** — `src/watcher/generic/watcher.rs:556`. Calls
  `run_plan_with_options` with `task.instruction` (the full plan as one string).
- **`execute_plan` (xzepr)** — `src/watcher/xzepr/watcher.rs:555`. Calls
  `run_plan_with_options` with a YAML plan string, same single-shot pattern.
- **`Plan::to_instruction()`** — `src/tools/plan.rs`. Collapses all tasks into
  one formatted string; task-based plans produce a numbered list.
- **`build_agent_environment`** — `src/commands/environment.rs`. Initialises
  tools, skills, MCP stack. Uses `ChatMode::Planning` for the `run` command.
  Watchers pass this config unchanged.
- **`prompts::build_system_prompt`** — `src/prompts/mod.rs`. Only supports
  `ChatMode::Planning` and `ChatMode::Write`. No watcher-autonomous variant.
- **`transient_system_messages`** — `Agent::set_transient_system_messages`.
  Existing hook for injecting extra system messages before each LLM call;
  already used for skill disclosure.

### Identified Issues

1. **No autonomous system prompt for watcher mode.** The LLM receives no
   instruction saying it must execute tasks without asking for confirmation. It
   defaults to chat-assistant behaviour and responds with "How would you like
   me to proceed?", completing in 1 iteration with no tool calls.
2. **Full plan as a single prompt.** All tasks are concatenated into one string.
   Multi-step plans with dependencies cannot be executed reliably because the
   LLM must mentally track all state. Small/quantized models fail at this.
3. **No per-task execution loop.** There is no mechanism to drive the agent
   through tasks one by one, check intermediate results, or honour task
   `dependencies` ordering.
4. **`ChatMode::Planning` used in watcher context.** The planning prompt
   instructs the LLM to plan, not act. Watcher execution needs a distinct mode.
5. **Single-shot result event.** `GenericPlanResult` records only overall
   success/failure. There is no per-task outcome in the result event.
6. **XZepr watcher same problem.** `watcher/xzepr/watcher.rs` has the same
   single-shot `run_plan_with_options` call on line 555.

---

## Implementation Phases

### Phase 1: Autonomous Watcher System Prompt

Add a watcher-execution system prompt and a `ChatMode::Watcher` variant so the
LLM knows it is operating autonomously and must call tools immediately.

#### Task 1.1 — Add `ChatMode::Watcher`

- Add `Watcher` variant to the `ChatMode` enum in
  `src/chat_mode.rs`.
- Update `ChatMode::parse_str`, `as_str`, `Display`, and any exhaustive match
  arms that do not already use a wildcard.

#### Task 1.2 — Add Watcher System Prompt

- Create `src/prompts/watcher_prompt.rs` with
  `generate_watcher_prompt() -> String`.
- The prompt must include:
  - "You are an autonomous XZatoma watcher agent operating without a human in
    the loop."
  - "Execute EVERY task now using your available tools (terminal, file
    operations, etc.). Do NOT describe tasks, do NOT ask for confirmation."
  - "When all tasks complete, respond with a one-paragraph summary of outcomes."
  - List of tools available and when to use them (terminal for shell commands,
    write_file for file creation, etc.).
- Wire the new prompt into `prompts::build_system_prompt` for
  `ChatMode::Watcher` in `src/prompts/mod.rs`.

#### Task 1.3 — Inject Watcher Prompt in `build_agent_environment`

- Add an optional `override_mode: Option<ChatMode>` parameter to
  `build_agent_environment` in `src/commands/environment.rs`.
- When `Some(ChatMode::Watcher)` is passed, use it in place of the config
  default when constructing the agent.
- Update all existing call sites to pass `None` (no behaviour change).

#### Task 1.4 — Use `ChatMode::Watcher` in Both Watcher Execute Paths

- In `execute_plan` (`src/watcher/generic/watcher.rs:556`): call
  `build_agent_environment` with `Some(ChatMode::Watcher)` and construct the
  agent directly rather than delegating to `run_plan_with_options`.
- Same change in `src/watcher/xzepr/watcher.rs:555`.
- This is the minimal fix: a single `agent.execute(full_instruction)` call with
  the correct prompt should now produce multi-turn tool use instead of a
  one-shot description.

#### Task 1.5 — Testing Requirements

- Unit test: `generate_watcher_prompt()` contains required phrases
  ("autonomous", "tools", "do not ask").
- Unit test: `build_system_prompt(ChatMode::Watcher, _)` returns watcher prompt.
- Integration test (existing dry-run path): `GenericWatcher` with `dry_run=true`
  still produces `MessageDisposition::Processed` after Phase 1 changes.

#### Task 1.6 — Deliverables and Success Criteria

- [ ] `ChatMode::Watcher` variant compiles with no warnings.
- [ ] `src/prompts/watcher_prompt.rs` exists with `generate_watcher_prompt`.
- [ ] Both watcher `execute_plan` methods pass `ChatMode::Watcher` to the agent
      builder.
- [ ] All existing tests pass (`cargo test --lib`).
- **Success**: Running `./seed_plan.sh hello` against a live watcher produces
  at least 2 agent iterations (LLM calls terminal tool at least once), and
  `tmp/hello-world-report.txt` is created.

---

### Phase 2: Per-Task Execution Loop (Generic Watcher)

Replace the single-prompt execution with a task-by-task loop that calls
`agent.execute(task.description)` for each task in order, in a shared agent
session. Context from earlier tasks is visible to later ones via conversation
history.

#### Task 2.1 — Dependency Resolver

- Add `fn resolve_task_order(tasks: &[PlanTask]) -> Vec<&PlanTask>` to
  `src/tools/plan.rs`.
- Performs a topological sort of `PlanTask::dependencies` using the task `id`
  field. Tasks with no dependencies execute first.
- Returns `Err` if a cycle is detected or a dependency `id` is unknown.
- Unit tests: no-dependency list (original order preserved), linear chain,
  diamond dependency, cycle detection.

#### Task 2.2 — `execute_tasks_sequentially` Method

- Add `async fn execute_tasks_sequentially(&self, plan: &Plan, agent: &mut Agent)
  -> Result<Vec<TaskOutcome>>` to `GenericWatcher`
  (`src/watcher/generic/watcher.rs`).
- `TaskOutcome` is a new private struct: `{ id: String, success: bool, summary:
  String }`.
- Algorithm:
  1. Call `resolve_task_order(&plan.tasks)`.
  2. For each task in order: call `agent.execute(&task.description)`.
  3. Capture the `Result<String>` response; record `TaskOutcome`.
  4. On `Err`, mark task as failed and continue to next task (do not abort the
     entire plan).
  5. Return the full `Vec<TaskOutcome>`.

#### Task 2.3 — Update `execute_plan` to Use Per-Task Loop

- Update `execute_plan` (`src/watcher/generic/watcher.rs:556`):
  - Build the agent using `ChatMode::Watcher` (Phase 1 output).
  - If `plan.tasks` is non-empty: call `execute_tasks_sequentially`; derive
    overall `success` from whether all tasks succeeded.
  - If `plan.steps` only (legacy): keep existing single-shot `agent.execute`
    path.
  - After all tasks: call `agent.execute("Summarise the results of all tasks
    completed above in one paragraph.")` to produce the `summary` field of
    `GenericPlanResult`.

#### Task 2.4 — Update `GenericPlanResult` with Per-Task Outcomes

- Add `task_outcomes: Option<Vec<serde_json::Value>>` field to
  `GenericPlanResult` in `src/watcher/generic/result_event.rs` (optional,
  skip-if-empty serialization).
- Populate it in `execute_plan` from the `Vec<TaskOutcome>`.
- Each entry: `{ "id": "...", "success": true/false, "summary": "..." }`.

#### Task 2.5 — Testing Requirements

- Unit test `resolve_task_order`: no deps, chain, diamond, cycle.
- Unit test `execute_tasks_sequentially` with a `FakeGenericConsumer` / mock
  agent: all succeed, first fails, last fails.
- Existing watcher unit tests must still pass.

#### Task 2.6 — Deliverables and Success Criteria

- [ ] `resolve_task_order` in `src/tools/plan.rs` with ≥4 unit tests.
- [ ] `execute_tasks_sequentially` implemented and covered by unit tests.
- [ ] `execute_plan` branches on `plan.tasks` vs `plan.steps`.
- [ ] `GenericPlanResult` serializes `task_outcomes` when present.
- **Success**: `./seed_plan.sh audit` (the multi-task doc-audit plan with
  dependencies) executes all 4 tasks in order; `tmp/xzatoma-audit-report.md`
  is created; the result event on `xzatoma.results` contains a `task_outcomes`
  array with 4 entries.

---

### Phase 3: Per-Task Execution Loop (XZepr Watcher)

Apply the same per-task loop pattern to the XZepr watcher, which receives a
plan as a YAML string from the CloudEvent data field.

#### Task 3.1 — Parse Plan in XZepr `execute_plan`

- In `src/watcher/xzepr/watcher.rs`, parse `plan_yaml` into a `Plan` using
  `PlanParser::from_yaml(&plan_yaml)` before creating the agent.
- If parsing fails, fall back to the existing single-shot path (pass the raw
  YAML as the prompt) with a warning log.

#### Task 3.2 — Reuse `execute_tasks_sequentially`

- Move `execute_tasks_sequentially` and `TaskOutcome` from
  `src/watcher/generic/watcher.rs` to a shared location:
  `src/watcher/plan_executor.rs` (new file).
- Re-export from both `watcher::generic` and `watcher::xzepr`.
- Update the generic watcher to use the moved location.

#### Task 3.3 — Update XZepr `execute_plan`

- Replace the `run_plan_with_options` call with:
  1. Build agent with `ChatMode::Watcher`.
  2. If `plan.tasks` non-empty: call `execute_tasks_sequentially`.
  3. Else: `agent.execute(plan.to_instruction())`.
- The XZepr result (`GenericPlanResult`) already has a `plan_output` field;
  include `task_outcomes` there.

#### Task 3.4 — Testing Requirements

- Unit tests for the `plan_executor` module: same tests as Task 2.5, but via
  the shared module.
- XZepr watcher integration test (dry-run path) still passes.

#### Task 3.5 — Deliverables and Success Criteria

- [ ] `src/watcher/plan_executor.rs` with `execute_tasks_sequentially` and
      `TaskOutcome`.
- [ ] Both watchers import from `plan_executor`.
- [ ] XZepr watcher `execute_plan` uses per-task loop when `plan.tasks`
      non-empty.
- [ ] All existing XZepr watcher tests pass.
- **Success**: An XZepr CloudEvent carrying a multi-task plan executes each
  task sequentially with tool calls; the result event contains per-task
  outcomes.

---

### Phase 4: Configuration and Observability

Add config knobs to control execution mode and improve per-task logging.

#### Task 4.1 — `execution_mode` Config Field

- Add `execution_mode: ExecutionMode` to `WatcherExecutionConfig` in
  `src/config.rs`.
- `ExecutionMode` is a new enum: `PerTask` (default) | `SingleShot` (legacy).
- `SingleShot` preserves the pre-Phase 1 behaviour (single `agent.execute`
  call with full instruction) for operators who need it.
- Wire into both watcher `execute_plan` methods: check `execution_mode` and
  branch accordingly.

#### Task 4.2 — Per-Task Structured Logging

- Emit a structured log line at the start and end of each task:
  - Start: `info!(task_id, task_index, total_tasks, "Starting task execution")`
  - End: `info!(task_id, success, iterations, "Task execution complete")`
- Add `iterations` count to `TaskOutcome` by capturing it from
  `agent.iteration_count()` (or equivalent accessor if not yet exposed).
  Expose via a new `Agent::iteration_count() -> usize` method in
  `src/agent/core.rs` if it does not already exist.

#### Task 4.3 — Demo Config Update

- Update `demos/watcher/generic/config.yaml` to include `execution_mode: per_task`
  in the `execution:` section (documenting the new default).
- Update `demos/watcher/generic/README.md` to describe execution mode options.

#### Task 4.4 — Testing Requirements

- Unit test: `ExecutionMode` serializes/deserializes from YAML.
- Unit test: `SingleShot` mode in `GenericWatcher` produces single
  `agent.execute` call.
- Unit test: `PerTask` mode in `GenericWatcher` produces N `agent.execute`
  calls for N tasks.

#### Task 4.5 — Deliverables and Success Criteria

- [ ] `ExecutionMode` enum in `src/config.rs` with YAML round-trip.
- [ ] `WatcherExecutionConfig.execution_mode` field with `PerTask` default.
- [ ] Both watcher `execute_plan` methods branch on `execution_mode`.
- [ ] Structured per-task log lines emitted at start and end of each task.
- [ ] Demo config updated.
- **Success**: Setting `execution_mode: single_shot` in config reverts to
  Phase 0 behaviour; `per_task` (default) drives the per-task loop.

---

### Phase 5: Documentation

Update all affected documentation to reflect the new execution model.

#### Task 5.1 — `docs/how-to/setup_watcher.md`

- Add a "Plan Execution Model" section describing `per_task` vs `single_shot`.
- Add a troubleshooting entry: "Agent completes in 1 iteration without using
  tools" → check `execution_mode` and watcher system prompt.

#### Task 5.2 — `docs/explanation/` Entry

- Create `docs/explanation/watcher_session_execution.md` explaining:
  - Why the autonomous system prompt is necessary.
  - How the per-task loop works and how context accumulates.
  - How task dependencies are resolved.
  - The difference between generic and xzepr execution paths.

#### Task 5.3 — `src/watcher/generic/mod.rs` and `plan_executor.rs` Doc Comments

- Update the module-level doc in `src/watcher/generic/mod.rs` to describe the
  per-task execution model.
- Add full rustdoc to `src/watcher/plan_executor.rs` including examples.

#### Task 5.4 — Deliverables and Success Criteria

- [ ] `docs/how-to/setup_watcher.md` updated with execution mode section.
- [ ] `docs/explanation/watcher_session_execution.md` created.
- [ ] `src/watcher/plan_executor.rs` has complete rustdoc.
- **Success**: A new engineer can read the documentation and understand why
  per-task execution exists and how to configure it.

---

## Implementation Order

Phases must be executed in order: Phase 1 is the prerequisite for the live
system to start working at all. Phase 2 is the core improvement for the generic
watcher. Phase 3 extends it to xzepr. Phase 4 adds operator control. Phase 5
documents the completed system.

| Phase | Scope | Estimated effort |
|-------|-------|-----------------|
| 1 | Autonomous system prompt (both watchers) | Small |
| 2 | Per-task loop, generic watcher | Medium |
| 3 | Per-task loop, xzepr watcher + shared executor | Small |
| 4 | Config + observability | Small |
| 5 | Documentation | Small |
