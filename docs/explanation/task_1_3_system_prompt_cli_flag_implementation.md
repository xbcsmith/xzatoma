# Task 1.3: System Prompt CLI Flag Implementation

## Summary

This document describes the implementation of Task 1.3 from the dynamic system
prompts feature: adding the `--system-prompt` CLI flag to all LLM-facing
sub-commands and updating function signatures to accept and forward the new
parameter without yet using it.

## Scope

The following files were modified:

- `src/cli.rs` - Added `system_prompt: Option<String>` field to five command
  variants
- `src/main.rs` - Updated all match arms to destructure and forward
  `system_prompt`
- `src/commands/agent.rs` - Updated `handle_agent` signature and tests
- `src/commands/acp.rs` - Updated `handle_acp`, `apply_serve_overrides`, and
  tests
- `src/commands/mod.rs` - Updated `WatchCliOverrides`, `run_chat`,
  `run_plan_with_options`, and `run_plan`
- `tests/eval_run_command.rs` - Updated call site for `run_plan_with_options`

## Changes

### CLI Flag Additions (`src/cli.rs`)

The `--system-prompt` flag was added to five command variants in `Commands` and
`AcpCommand`:

| Variant             | Flag              | Type             |
| ------------------- | ----------------- | ---------------- |
| `Commands::Chat`    | `--system-prompt` | `Option<String>` |
| `Commands::Run`     | `--system-prompt` | `Option<String>` |
| `Commands::Agent`   | `--system-prompt` | `Option<String>` |
| `Commands::Watch`   | `--system-prompt` | `Option<String>` |
| `AcpCommand::Serve` | `--system-prompt` | `Option<String>` |

### Function Signature Updates

All LLM-facing entry points now accept `system_prompt: Option<String>`:

- `commands::chat::run_chat` - new last parameter
- `commands::run::run_plan_with_options` - new last parameter
- `commands::agent::handle_agent` - inserted before `config`
- `commands::acp::apply_serve_overrides` - new last parameter
- `watch::WatchCliOverrides` struct - new `system_prompt` field

### Phase Boundary

In this phase (Task 1.3), `system_prompt` is:

- Accepted by all CLI commands
- Forwarded through all function call chains
- Logged at `DEBUG` level when provided
- Suppressed with `let _ = system_prompt;` where not yet wired to config

Wiring `system_prompt` into the agent runtime and conversation context is
deferred to Phase 3.

### Design Decisions

- `handle_agent` places `system_prompt` before `config` to keep all override
  parameters grouped together before the configuration argument.
- `apply_serve_overrides` uses `let _ = system_prompt` (after the debug log)
  rather than attempting to set `config.acp.system_prompt` since the `AcpConfig`
  struct does not yet have that field.
- `WatchCliOverrides` derives `Default`, so adding the new field does not break
  any test that uses `..WatchCliOverrides::default()`.

## Tests Added

### `src/cli.rs`

- `test_cli_parse_chat_with_system_prompt_flag`
- `test_cli_parse_chat_system_prompt_defaults_none`
- `test_cli_parse_run_with_system_prompt_flag`
- `test_cli_parse_run_system_prompt_defaults_none`
- `test_cli_parse_agent_with_system_prompt_flag`
- `test_cli_parse_agent_system_prompt_defaults_none`

### `src/commands/agent.rs`

- `test_handle_agent_accepts_system_prompt_override`

### `src/commands/acp.rs`

- `test_apply_serve_overrides_accepts_system_prompt_parameter`

## Quality Gates

All four gates pass:

1. `cargo fmt --all` - clean
2. `cargo check --all-targets --all-features` - clean
3. `cargo clippy --all-targets --all-features -- -D warnings` - clean
4. `cargo test --all-features` - 2295 passed, 1 pre-existing failure in
   `config::tests::test_config_system_prompt_absent_in_yaml_gives_none` (not
   caused by this task)
