# Chat Command Unification Phase 1 Implementation

## Overview

Phase 1 establishes the uniform command contract for all chat slash commands.
The rule is: typing a command bare (e.g. `/mode`) returns per-command help, and
appending `status` (e.g. `/mode status`) returns the current live state. Six
commands previously violated this contract by returning errors on bare
invocation or silently mutating state without confirmation.

## Changes Made

### `src/commands/special_commands.rs`

#### 12 new `SpecialCommand` variants

Six `ShowXxxHelp` variants (emitted by bare invocations) and six `ShowXxxStatus`
variants (emitted by `/<cmd> status` invocations) were added between
`ToggleStreaming(bool)` and `Exit`. Each variant carries a full `///` doc
comment with a runnable doc-test.

| Variant               | Trigger             |
| --------------------- | ------------------- |
| `ShowModeHelp`        | bare `/mode`        |
| `ShowModeStatus`      | `/mode status`      |
| `ShowSafetyHelp`      | bare `/safety`      |
| `ShowSafetyStatus`    | `/safety status`    |
| `ShowModelHelp`       | bare `/model`       |
| `ShowModelStatus`     | `/model status`     |
| `ShowStreamingHelp`   | bare `/streaming`   |
| `ShowStreamingStatus` | `/streaming status` |
| `ShowSystemHelp`      | bare `/system`      |
| `ShowSystemStatus`    | `/system status`    |
| `ShowSubagentsHelp`   | bare `/subagents`   |
| `ShowSubagentsStatus` | `/subagents status` |

#### Updated `parse_special_command()`

Six command blocks were updated:

- `/mode` -- bare now returns `ShowModeHelp`; new `/mode status` arm added
  before the catch-all guard.
- `/safety` -- bare now returns `ShowSafetyHelp`; new `/safety status` arm
  added.
- `/model` -- bare now returns `ShowModelHelp`; `/model status` arm added before
  the generic `/model <name>` guard; empty-rest check removed (unreachable after
  trimming).
- `/subagents` -- bare now returns `ShowSubagentsHelp` (was
  `ToggleSubagents(true)`); `/subagents status` arm added before the catch-all
  guard.
- `/system` -- bare now returns `ShowSystemHelp`; `/system status` arm added
  before the generic text-extraction guard; empty-text branch now returns
  `ShowSystemHelp` instead of `MissingArgument`.
- `/streaming` -- bare now returns `ShowStreamingHelp`; `/streaming status` arm
  added; catch-all now returns `UnsupportedArgument` (was `MissingArgument`,
  which was inaccurate).

#### Six new `format_*_help_text()` public functions

`format_mode_help_text()`, `format_safety_help_text()`,
`format_model_help_text()`, `format_streaming_help_text()`,
`format_system_help_text()`, and `format_subagents_help_text()` each return a
`String` with a one-line summary, `USAGE:` block, `EXAMPLES:` block, and a
`NOTE:` line that calls out the `status` subcommand. The streaming help mentions
that `/streaming on|off` has no effect in Zed (ACP mode). The system help
mentions that skill disclosures are preserved.

#### Updated `format_help_text()`

- Added the general contract note at the top: "Type any command alone for
  per-command help. Add `status` to see the current value."
- Added `/<cmd>` and `/<cmd> status` entries to the CHAT MODE SWITCHING, SAFETY
  MODE SWITCHING, SUBAGENT DELEGATION, MODEL MANAGEMENT, SYSTEM PROMPT, and
  STREAMING sections.
- Updated the NOTES section to restate the contract.

#### Module-level doc comment

Updated to document the unified command contract.

### `src/commands/mod.rs`

Added 12 match arms in the terminal interactive chat loop (inside `run_chat`) to
handle the new variants:

- Help variants print the corresponding `format_*_help_text()` output.
- Status variants print the current value from the available session state
  (`mode_state.chat_mode`, `mode_state.safety_mode`,
  `mode_state.streaming_enabled`, `mode_state.subagents_enabled`,
  `current_model`, and the first system message from
  `agent.conversation().messages()`).

Updated the `use crate::commands::special_commands` import to include all six
new formatter functions.

### `src/acp/stdio.rs`

Updated `resolve_special_command_response()`:

- Added 6 arms for `ShowXxxHelp` variants: each calls the corresponding
  `format_*_help_text()` function (pure, no session state needed).
- Added 6 arms for `ShowXxxStatus` variants: status variants that require live
  session state use `handle_not_yet_implemented` stubs (Phase 2 will replace
  these). `ShowStreamingStatus` is an exception -- it immediately returns the
  ACP-mode note because streaming is permanently controlled by the Zed client.
- Removed the two now-obsolete `Err(CommandError::MissingArgument)` special-case
  arms for bare `/mode` and bare `/model`. These inputs now parse to `Ok`
  variants and no longer reach the error branch.

Updated the `use crate::commands::special_commands` import accordingly.

## Tests

### Removed tests (behavior changed)

| Old test                                                                           | Reason                                                                     |
| ---------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `test_parse_mode_no_arg_returns_error`                                             | Bare `/mode` now returns `Ok(ShowModeHelp)`                                |
| `test_parse_safety_no_arg_returns_error`                                           | Bare `/safety` now returns `Ok(ShowSafetyHelp)`                            |
| `test_parse_model_command_no_args_returns_error`                                   | Bare `/model` now returns `Ok(ShowModelHelp)`                              |
| `test_parse_set_system_prompt_empty_returns_missing_argument_error`                | Bare `/system` now returns `Ok(ShowSystemHelp)`                            |
| `test_parse_set_system_prompt_whitespace_only_text_returns_missing_argument_error` | `/system` trims to `/system`, now returns `Ok(ShowSystemHelp)`             |
| `test_parse_subagents_toggle`                                                      | Bare `/subagents` now returns `Ok(ShowSubagentsHelp)`                      |
| `test_parse_streaming_no_arg_returns_missing_argument_error`                       | Bare `/streaming` now returns `Ok(ShowStreamingHelp)`                      |
| `test_parse_streaming_invalid_arg_returns_missing_argument_error`                  | `/streaming <bad>` now returns `UnsupportedArgument` not `MissingArgument` |

### Added tests (21 new tests)

15 parser tests covering the new variants, 5 formatter tests verifying `USAGE:`
and the `status` entry, and 1 whitespace regression test replacing the old
whitespace-error test.

## Quality Gate Results

All four mandatory quality gates passed:

- `cargo fmt --all` -- clean
- `cargo check --all-targets --all-features` -- clean
- `cargo clippy --all-targets --all-features -- -D warnings` -- clean
- `cargo test --all-features` -- 771 passed, 0 failed
