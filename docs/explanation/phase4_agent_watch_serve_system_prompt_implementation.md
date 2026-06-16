# Phase 4: `agent`, `watch`, and `serve` Mode System Prompt Integration

## Overview

Phase 4 completes the dynamic system prompt feature by wiring system prompt
injection into the three remaining LLM-facing modes: `agent` (ACP stdio
subprocess), `watch` (Kafka-triggered watcher backends), and `acp serve` (ACP
HTTP server). After this phase every execution path that creates an agent
session honours the configured system prompt using the same precedence rule
established in earlier phases:

```text
plan.system_prompt > --system-prompt CLI flag > XZATOMA_SYSTEM_PROMPT env var
> agent.system_prompt config file field
```

## Changes by Component

### `src/config.rs` — `AcpConfig.system_prompt` field

A new optional field `system_prompt: Option<String>` was added to `AcpConfig`.
This provides an ACP-specific system prompt override that takes precedence over
the global `agent.system_prompt` field in ACP execution contexts (the HTTP
server and the stdio agent).

- Default: `None` (no override).
- Serialization: omitted when `None` (`skip_serializing_if`).
- Validation: blank (whitespace-only) values are rejected by
  `validate_acp_config`.
- Environment variable: `XZATOMA_SYSTEM_PROMPT` now writes to both
  `config.agent.system_prompt` and `config.acp.system_prompt` so a single env
  var covers all modes.

### `src/commands/acp.rs` — `apply_serve_overrides`

The `--system-prompt` CLI flag for `acp serve` now writes to
`config.acp.system_prompt` (the ACP-specific field) and mirrors the value into
`config.agent.system_prompt` for shared code paths. Previously it only wrote to
`config.agent.system_prompt`.

### `src/commands/agent.rs` — `handle_agent`

The `handle_agent` function previously accepted the `system_prompt` parameter
but discarded it without writing it to the config. It now calls
`crate::agent::resolve(None, system_prompt.as_deref(), config.agent.system_prompt.as_deref())`
and writes the resolved value back to `config.agent.system_prompt` before
calling `run_stdio_agent`. This ensures the ACP stdio session creation path
(which reads `config.agent.system_prompt`) sees the CLI flag value with correct
precedence.

### `src/acp/executor.rs` — `execute_prompt`

After constructing the agent, `execute_prompt` now reads the effective system
prompt (`config.acp.system_prompt` preferred over `config.agent.system_prompt`)
and calls `agent.conversation_mut().add_system_message(sp.to_string())` before
`agent.execute(prompt)`. The injection is skipped for blank prompts.

### `src/acp/stdio.rs` — `AcpStdioServerState::create_session`

After building the agent (whether a fresh session or a resumed one),
`create_session` now reads the effective system prompt and calls
`agent.conversation_mut().replace_first_system_message(sp.to_string())`.

`replace_first_system_message` is used rather than `add_system_message` so that
resumed sessions have their previously stored system message updated in-place
rather than accumulating duplicates across sessions.

The injection happens before the mode-specific `transient_system_messages` block
so the user-defined prompt is always the first message in the conversation.

### `src/watcher/generic/watcher.rs` — `execute_plan`

After creating the agent, the generic watcher resolves the system prompt using
`crate::agent::resolve(task.plan.system_prompt.as_deref(), None, config.agent.system_prompt.as_deref())`.
The plan-level field takes precedence over the config/CLI-level value. The
resolved prompt is injected via `add_system_message` before the skill disclosure
message.

### `src/watcher/xzepr/watcher.rs` — `MessageHandler::handle`

The XZepr watcher previously parsed the plan YAML inside the execution `match`
arm. It now parses the plan early (before agent creation) using a shared
`plan_parse_result` binding so the plan's optional `system_prompt` field is
available for resolution. The same binding is reused for execution, eliminating
the previously redundant second parse.

System prompt injection follows the same pattern as the generic watcher: resolve
plan vs. config, inject before skill disclosure.

## Injection Order in Agent Conversations

For all modes, the ordering of conversation messages at session start is:

1. User-defined system prompt (from plan, CLI, env var, or config file) — added
   via `add_system_message` or `replace_first_system_message`.
2. Skill disclosure message — added via `add_system_message`.
3. Mode-specific base prompt — added as a transient system message (per-call
   injection, not stored in conversation history).
4. Active skill prompt injection — added as a transient system message.

## Precedence Summary for Each Mode

| Mode                | Plan sp                   | CLI flag                                                 | Env var                                                     | Config field                                                  |
| ------------------- | ------------------------- | -------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------- |
| `agent` (ACP stdio) | N/A                       | wins over config                                         | `config.agent.system_prompt`                                | `agent.system_prompt`                                         |
| `watch`             | `plan.system_prompt` wins | `config.agent.system_prompt` (via `apply_cli_overrides`) | same                                                        | same                                                          |
| `acp serve` (HTTP)  | N/A                       | wins, writes `config.acp.system_prompt`                  | `config.acp.system_prompt` AND `config.agent.system_prompt` | `acp.system_prompt` preferred, `agent.system_prompt` fallback |

## Tests Added

All new tests follow the naming convention
`test_<function>_<condition>_<expected>`.

### `src/config.rs`

- `test_acp_config_system_prompt_defaults_none`
- `test_config_acp_system_prompt_deserializes_from_yaml`
- `test_config_acp_system_prompt_absent_in_yaml_gives_none`
- `test_config_validation_rejects_blank_acp_system_prompt`
- `test_config_validation_accepts_nonempty_acp_system_prompt`
- `test_apply_env_vars_sets_acp_system_prompt`
- `test_apply_env_vars_sets_both_agent_and_acp_system_prompt`
- `test_apply_env_vars_ignores_blank_system_prompt` (extended to also assert
  `config.acp.system_prompt` is `None`)

### `src/commands/acp.rs`

- `test_apply_serve_overrides_stores_system_prompt_in_acp_and_agent_config`
  (replaces the placeholder test from Phase 1)

### `src/commands/agent.rs`

- `test_handle_agent_cli_system_prompt_wins_over_config`
- `test_handle_agent_config_system_prompt_used_when_no_cli_flag`
- `test_handle_agent_no_system_prompt_resolves_to_none`

### `src/acp/executor.rs`

- `test_execute_prompt_injects_acp_system_prompt`
- `test_execute_prompt_uses_agent_system_prompt_when_acp_not_set`
- `test_execute_prompt_acp_system_prompt_wins_over_agent_system_prompt`

### `src/acp/stdio.rs`

- `test_create_session_injects_agent_system_prompt_into_conversation`
- `test_create_session_injects_acp_system_prompt_into_conversation`

### `src/watcher/generic/watcher.rs`

- `test_execute_plan_injects_config_system_prompt`
- `test_execute_plan_plan_system_prompt_wins_over_config`
- `test_execute_plan_no_system_prompt_resolves_to_none`

### `src/watcher/xzepr/watcher.rs`

- `test_xzepr_watcher_system_prompt_resolve_plan_wins_over_config`
- `test_xzepr_watcher_system_prompt_config_used_when_plan_has_none`
- `test_xzepr_watcher_system_prompt_none_when_no_sources`
