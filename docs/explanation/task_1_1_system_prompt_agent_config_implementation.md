# Task 1.1: system_prompt Field in AgentConfig

## Summary

This document explains the implementation of Task 1.1 from the dynamic system
prompts feature: adding `system_prompt: Option<String>` to `AgentConfig` in
`src/config.rs` and wiring it through the places that already expected it.

## What Changed

### `src/config.rs`

- Added `pub system_prompt: Option<String>` to `AgentConfig` with
  `#[serde(skip_serializing_if = "Option::is_none")]`.
- Initialized the field to `None` in `Default for AgentConfig`.
- Added an `XZATOMA_SYSTEM_PROMPT` environment variable handler inside
  `apply_env_vars`. Blank (whitespace-only) values are silently ignored.
- Added a validation rule in `Config::validate` that rejects a whitespace-only
  `system_prompt` with a descriptive error message.
- Added seven unit tests covering default value, YAML deserialization (present
  and absent), blank validation rejection, non-empty validation acceptance, and
  the two `apply_env_vars` paths.

### `src/commands/mod.rs`

The `run_chat` and `run_plan_with_options` functions already accepted
`system_prompt: Option<String>` as a parameter but silently discarded it with
`let _ = system_prompt`. Both functions now apply the value to
`config.agent.system_prompt` before the agent session starts.

`WatchCliOverrides` already had a `system_prompt: Option<String>` field. The
`apply_cli_overrides` function now propagates it to
`config.agent.system_prompt`.

### `src/commands/acp.rs`

The `AcpCommand::Serve` enum variant already declared `system_prompt` in the CLI
definition. The `handle_acp` match arm now destructures and applies the field to
`config.agent.system_prompt` before starting the server.

## Precedence Model

After this change, the system prompt priority from lowest to highest is:

1. Config file (`agent.system_prompt` key)
2. `XZATOMA_SYSTEM_PROMPT` environment variable
3. CLI flag (`--system-prompt`) applied by the command handler
4. Plan file `system_prompt` field (applied downstream during execution)

## Validation Rules

- `None` is always valid (feature is opt-in).
- A non-empty string is valid.
- A whitespace-only string is rejected by `Config::validate` with the message
  `agent.system_prompt cannot be blank`.
- Environment variable handling ignores whitespace-only values rather than
  storing them, so blank env vars do not trigger a validation error.
