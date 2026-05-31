# Phase 1: Autonomous Watcher System Prompt Implementation

## Overview

Phase 1 adds a watcher-specific autonomous system prompt and a new
`ChatMode::Watcher` variant so that the LLM operating inside a watcher knows it
must execute tasks immediately using tools, without asking for human
confirmation. Before Phase 1, watchers used `ChatMode::Planning`, which
instructs the LLM to plan and describe rather than act, causing execution to
complete in one iteration with no tool calls.

---

## What Was Added

### ChatMode::Watcher Variant

A new `Watcher` variant was added to the `ChatMode` enum in `src/chat_mode.rs`.
All exhaustive match arms (`parse_str`, `as_str`, `Display`, and the safety mode
resolution in `build_tools_for_mode`) were updated to handle the new variant.
`ChatMode::Watcher` maps to the string `"watcher"` and is paired with
`SafetyMode::NeverConfirm` so tool calls require no user confirmation.

### Watcher System Prompt

`src/prompts/watcher_prompt.rs` was created with
`generate_watcher_prompt() -> String`. The generated prompt contains three
mandatory instructions:

1. The agent is autonomous and operating without a human in the loop.
2. It must execute every task immediately using available tools, never describe
   tasks or ask for confirmation.
3. When all tasks complete, it must respond with a one-paragraph summary of
   outcomes.

The prompt also names the available tools (terminal, file operations, read file,
write file, grep) and describes when to use each. This gives the LLM the context
it needs to act rather than plan.

`prompts::build_system_prompt` in `src/prompts/mod.rs` was updated to return the
watcher prompt when `ChatMode::Watcher` is passed.

### build_agent_environment Override Mode

`build_agent_environment` in `src/commands/environment.rs` gained an optional
`override_mode: Option<ChatMode>` parameter. When `Some(ChatMode::Watcher)` is
passed, it overrides the mode derived from the config file. All existing call
sites pass `None`, preserving their prior behavior unchanged.

### Watcher Execute Paths

Both watcher backends were updated to construct their agents with
`ChatMode::Watcher`:

- `src/watcher/generic/watcher.rs` — `execute_plan` now calls
  `build_agent_environment` with `Some(ChatMode::Watcher)` and constructs the
  agent directly using `Agent::new_with_mode`.
- `src/watcher/xzepr/watcher.rs` — same change in the spawned async task.

---

## Deliverables Checklist

- [x] `ChatMode::Watcher` variant compiles with no warnings
- [x] `src/prompts/watcher_prompt.rs` exists with `generate_watcher_prompt`
- [x] `build_system_prompt(ChatMode::Watcher, _)` returns the watcher prompt
- [x] Both watcher `execute_plan` methods pass `ChatMode::Watcher` to the agent
      builder
- [x] Unit tests verify the watcher prompt contains the required phrases
      ("autonomous", "tools", "do not ask for confirmation")
- [x] All existing tests pass

---

## Testing

Unit tests in `src/prompts/watcher_prompt.rs` verify:

- `generate_watcher_prompt()` contains the word "autonomous"
- `generate_watcher_prompt()` references tool usage
- `generate_watcher_prompt()` instructs the LLM not to ask for confirmation
- `build_system_prompt(ChatMode::Watcher, config)` returns the watcher prompt
- The prompt is non-empty

The generic watcher dry-run integration test verifies that the `execute_plan`
changes do not break the existing `MessageDisposition::Processed` outcome.

---

## Impact

With `ChatMode::Watcher` in place, the agent receives a system prompt that
directs it to act immediately. A single `agent.execute(full_instruction)` call
now produces multi-turn tool use instead of a one-shot text description. This is
the prerequisite for all subsequent phases: per-task loop (Phase 2), dependency
resolution, XZepr watcher extension (Phase 3), and configuration control (Phase
4).
