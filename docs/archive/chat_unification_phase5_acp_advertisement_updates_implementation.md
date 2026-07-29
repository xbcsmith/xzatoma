# Phase 5: Zed ACP Advertisement Updates

## Overview

This document describes the implementation of Phase 5 of the Chat Command
Unification plan: updating the Zed ACP advertisement in
`src/acp/available_commands.rs` so that the Zed completion menu accurately
reflects the unified UX introduced in Phases 1-4.

## Changes

### `src/acp/available_commands.rs`

#### Module doc comment (Task 5.4)

The command overview table at the top of the file was updated to:

- Add the `/streaming` row.
- Replace the bare `Optional` / `Required` labels in the `Input` column with
  descriptive values that show `status` as a recognised keyword, matching the UX
  delivered by Phases 1-3.
- Update the doc-test count assertion from `12` to `13`.

#### `build_available_commands` (Tasks 5.1 and 5.3)

- Doc comment updated: "twelve" changed to "thirteen"; count assertion updated;
  `/streaming` added to the example name assertions.
- `build_streaming_command()` call added to the returned `vec![]`.

#### Updated command descriptions (Task 5.1)

| Command      | Old description (abbreviated)                    | New description                                                                                                            |
| ------------ | ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `/mode`      | Show or change the XZatoma operation mode...     | Show help or switch the operation mode. Use `/mode status` to see the current mode. Pass `planning` or `write` to switch.  |
| `/model`     | Show or change the current AI provider model...  | Show help or switch the active model. Use `/model status` to see the current model. Pass a model name to switch.           |
| `/safety`    | Show or change the safety confirmation policy... | Show help or change the safety policy. Use `/safety status` to see the current policy. Pass `on` or `off` to change.       |
| `/subagents` | Enable or disable subagent delegation.           | Show help or toggle subagent delegation. Use `/subagents status` to see current state. Pass `on` or `off` to change.       |
| `/system`    | Set or replace the active system prompt...       | Show help, inspect, or replace the system prompt. Use `/system status` to see the current prompt. Pass text to replace it. |
| `/streaming` | (new)                                            | Show streaming help. Streaming is controlled by the Zed client in ACP mode. Use `/streaming status` for details.           |

#### Updated input hints (Task 5.2)

| Command      | Old hint                                                                | New hint                                |
| ------------ | ----------------------------------------------------------------------- | --------------------------------------- |
| `/mode`      | `Optional mode ID: planning \| write \| safe \| full_autonomous`        | `Optional: planning \| write \| status` |
| `/model`     | `Optional model name, e.g. gpt-4o or llama3.2:latest`                   | `Optional: <model_name> \| status`      |
| `/safety`    | `Optional policy: always_confirm \| confirm_dangerous \| never_confirm` | `Optional: on \| off \| status`         |
| `/subagents` | `Optional toggle: on \| off \| enable \| disable`                       | `Optional: on \| off \| status`         |
| `/system`    | `Required text: the new system prompt`                                  | `Optional: <new prompt text> \| status` |

The `/system` hint change is significant: it no longer says "Required", which
reflects that bare `/system` now shows help (implemented in Phase 1) rather than
producing a missing-argument error.

#### New `build_streaming_command` builder (Task 5.3)

```rust
fn build_streaming_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/streaming",
        "Show streaming help. Streaming is controlled by the Zed client in ACP mode. \
         Use `/streaming status` for details.",
    )
}
```

The command carries no input specification because bare `/streaming` already
shows the help text via the parser, and `/streaming status` is embedded in the
help text itself.

#### Updated builder doc comments (Task 5.4)

All six updated builders (`build_mode_command`, `build_model_command`,
`build_safety_command`, `build_subagents_command`, `build_system_command`,
`build_streaming_command`) have updated `///` doc comments that describe the new
`status` keyword behaviour.

#### Updated tests (Task 5.5)

| Test                                                   | Change                                                                                                    |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `test_build_available_commands_returns_twelve_entries` | Renamed to `test_build_available_commands_returns_thirteen_entries`; assertion changed from `12` to `13`. |
| `test_build_available_commands_names_are_correct`      | `"/streaming"` appended to expected names slice.                                                          |
| `test_build_available_commands_includes_new_commands`  | `"/streaming"` added to the presence check list.                                                          |
| `test_no_arg_commands_have_no_input`                   | `"/streaming"` added to the no-input group.                                                               |
| `test_streaming_command_is_present`                    | New test: asserts `/streaming` is in the command list.                                                    |
| `test_system_command_input_hint_mentions_status`       | New test: asserts hint contains `"status"` and does not contain `"required"`.                             |
| `test_mode_command_input_hint_mentions_status`         | New test: asserts hint contains `"status"`.                                                               |
| `test_model_command_input_hint_mentions_status`        | New test: asserts hint contains `"status"`.                                                               |

### `src/acp/stdio.rs`

The test
`test_build_available_commands_returns_twelve_entries_from_stdio_context` was
renamed to
`test_build_available_commands_returns_thirteen_entries_from_stdio_context` and
its assertion updated from `12` to `13`.

## Design Decisions

### `/streaming` has no input specification

The plan notes that `/streaming` is an ACP-mode no-op but should appear in the
completion menu. Since the parser already handles bare `/streaming` (returning
`ShowStreamingHelp`) and `/streaming status` (returning `ShowStreamingStatus`),
no `AvailableCommandInput` hint is needed. Both cases work without Zed needing
to prompt the user for additional text.

### Input hints use `Optional:` prefix

Changing all five hints to start with `Optional:` aligns with the new UX where
every command now shows help when invoked bare. This removes the misleading
`Required text:` label from `/system` and makes all hint formats consistent.

## Success Criteria Verification

- Zed's `/` completion menu shows 13 commands including `/streaming`.
- The hint for `/system` says `"Optional: <new prompt text> | status"` and no
  longer contains `"required"`.
- The hints for `/mode`, `/model`, `/safety`, `/subagents` all contain
  `"status"`.
- All 18 `available_commands` and `stdio` count tests pass.

## Quality Gate Results

All four mandatory gates passed:

```text
cargo fmt --all                                          -- pass
cargo check --all-targets --all-features                -- pass
cargo clippy --all-targets --all-features -- -D warnings -- pass
cargo test --lib -- available_commands                   -- 18 passed, 0 failed
```
