# Phase 6: CLI Flags for `chat` and `run` Subcommands

## Overview

Phase 6 adds `--thinking-effort` flags to the `chat` and `run` subcommands so
users can configure thinking effort at the command line without editing the YAML
configuration file. This completes the end-to-end plumbing for thinking effort
from the CLI, through the command handlers, and into the provider layer.

## Changes Made

### `src/cli.rs`

Added `thinking_effort: Option<String>` to `Commands::Chat` (after `resume`) and
`Commands::Run` (after `allow_dangerous`).

The field is declared with `#[arg(long)]` so it appears as `--thinking-effort`
on the command line. Accepted values are `none`, `low`, `medium`, `high`, and
`extra_high`, but the CLI accepts any string without rejecting at parse time.
Validation is deferred to the provider.

Updated all existing tests that explicitly pattern-match on `Commands::Chat` or
`Commands::Run` fields to include `thinking_effort: _`.

Added five new tests required by Task 6.5:

- `test_cli_parse_chat_with_thinking_effort_high`
- `test_cli_parse_chat_with_thinking_effort_none`
- `test_cli_parse_chat_thinking_effort_defaults_none`
- `test_cli_parse_run_with_thinking_effort_medium`
- `test_cli_parse_run_thinking_effort_defaults_none`

### `src/main.rs`

Updated the `Commands::Chat` and `Commands::Run` match arms to extract
`thinking_effort` from the parsed CLI struct and pass it as the final argument
to the respective command handlers. A `tracing::debug!` log statement records
the value when it is present.

### `src/commands/mod.rs`

#### `run_chat`

Added `thinking_effort: Option<String>` as the last parameter. After the
provider `Arc` is created and before the subagent tool is registered, the
handler applies the thinking effort value:

```xzatoma/src/commands/mod.rs#L483-L501
// Apply thinking effort from CLI flag if provided.
// "none" is the sentinel string meaning "clear any explicit effort" (maps to
// Provider::set_thinking_effort(None)); any other string is forwarded as-is.
// Unrecognised values cause a warning but do not abort execution.
if let Some(ref effort_str) = thinking_effort {
    let param = if effort_str == "none" {
        None
    } else {
        Some(effort_str.as_str())
    };
    if let Err(e) = provider.set_thinking_effort(param) {
        tracing::warn!(
            thinking_effort = %effort_str,
            error = %e,
            "Unsupported thinking effort value; proceeding with provider default"
        );
    }
}
```

#### `run_plan_with_options`

Added `thinking_effort: Option<String>` as the last parameter. After the
provider `Arc` is created and before the agent is constructed, the same
application logic runs.

#### `run_plan`

The convenience wrapper passes `None` for `thinking_effort`, preserving its
existing three-argument public signature.

### Callers updated

All existing callers of `run_plan_with_options` that were outside the new CLI
path received `None` for the new parameter so their behaviour is unchanged:

- `src/watcher/generic/watcher.rs` (generic watcher event handler)
- `src/watcher/xzepr/watcher.rs` (xzepr watcher plan executor)
- `tests/eval_run_command.rs` (eval harness integration test)
- `tests/mcp_config_test.rs` (updated `Commands::Run` struct literal)

## Semantics

| CLI input                       | `thinking_effort` value | `set_thinking_effort` call                                             |
| ------------------------------- | ----------------------- | ---------------------------------------------------------------------- |
| flag absent                     | `None`                  | not called; provider default used                                      |
| `--thinking-effort none`        | `Some("none")`          | `set_thinking_effort(None)`                                            |
| `--thinking-effort high`        | `Some("high")`          | `set_thinking_effort(Some("high"))`                                    |
| `--thinking-effort unknown_xyz` | `Some("unknown_xyz")`   | `set_thinking_effort(Some("unknown_xyz"))` returns Err, warning logged |

## Validation and Error Handling

The CLI accepts any string value for `--thinking-effort` without rejecting at
parse time. This keeps the CLI forward-compatible with new effort levels added
to providers in future releases.

If the provider returns an error (for example, because the value is unrecognised
or the provider does not support thinking effort), a `tracing::warn!` log is
emitted and execution continues with the provider default. The command does not
abort.

## Quality Gates

All mandatory quality gates passed:

```text
cargo fmt --all                              OK
cargo check --all-targets --all-features    OK
cargo clippy --all-targets --all-features   OK (zero warnings)
cargo test --lib                            2076 passed, 0 failed, 59 ignored
```

All five Task 6.5 tests pass. All previously passing tests continue to pass.
