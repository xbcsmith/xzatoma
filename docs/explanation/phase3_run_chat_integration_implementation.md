# Phase 3: `run` and `chat` Mode Integration

## Overview

Phase 3 wires the system prompt resolver into the two primary interactive modes:
`run` (plan execution) and `chat` (interactive conversation). It also adds the
`/system <text>` chat command, which lets users update the active system prompt
mid-session without restarting.

## Changes

### `run_plan_with_options`

The plan file is now parsed before agent construction so that
`plan.system_prompt` is available for resolution. Resolution follows the
documented precedence order: plan > CLI flag > config/env. The resolved prompt
is injected as the first system message before any skill disclosure message.

The task instruction is now generated via `Plan::to_instruction()`, which
correctly handles both the task-based and step-based plan formats.

### `run_chat` — new sessions

The CLI flag value (`--system-prompt`) is kept separate from
`config.agent.system_prompt` so the two sources can be distinguished. After
agent construction, `resolve(None, cli_flag, config_prompt)` determines the
effective prompt. For new sessions the resolved prompt is injected before the
skill disclosure message.

### `run_chat` — resumed sessions

When `--system-prompt` is supplied on the CLI, the first system message in the
resumed conversation is replaced via
`Conversation::replace_first_system_message`. When only the config/env value is
present, the historical system messages are left intact.

### `/system <text>` chat command

Added in Stage 1 (special_commands.rs). The match arm in the `run_chat` loop
calls `agent.conversation_mut().replace_first_system_message(&text)` and prints
a confirmation. Skill disclosure messages (subsequent system messages) are
unaffected.

### Skill disclosure ordering

Skill disclosure is now injected in a single unified block after all user system
prompt logic, with a deduplication check that works correctly for both new and
resumed sessions.

## Trace Logging

When `--trace` is active, the full resolved system prompt text and its source
(`Plan`, `CliFlag`, or `Config`) are logged at `TRACE` level at session start.
