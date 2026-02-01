# Models command — Chat mode help implementation

## Overview

This document describes a small but important usability fix to the interactive chat mode
special commands: when a user types `/models` with no arguments, the chat session now
displays a focused help message for model-management subcommands (e.g., `/models list`,
`/models info <name>`) and does not attempt to execute a command or forward the
input to the agent.

Before this change:
- Typing `/models` returned `None` from the special-command parser and the input was
  treated as a normal prompt (i.e., sent to the agent).

After this change:
- `/models` is recognized as a special command and prints models-specific help text,
  preventing accidental execution.

This improves discoverability of the `models` subcommands and avoids surprising agent
behavior when users omit a subcommand.

---

## Components Delivered

- `xzatoma/src/commands/special_commands.rs`
  - Added `SpecialCommand::ModelsHelp` variant.
  - Map `"/models"` to `SpecialCommand::ModelsHelp` in `parse_special_command`.
  - Added `print_models_help()` which prints usage, flags, and examples for `/models`.
  - Added unit tests for `/models` parsing behavior.

- `xzatoma/src/commands/mod.rs`
  - In the chat loop, added handling for `SpecialCommand::ModelsHelp` which prints
    the models help and continues the loop (no execution).

- `xzatoma/docs/explanation/models_command_chat_mode_help_implementation.md`
  - This document (new file): rationale, implementation details, tests, and examples.

---

## Implementation Details

Problem:
- The special command parser returned `SpecialCommand::None` for the bare `/models`
  input. As a result, `/models` (no args) could be sent to the agent as a prompt, which
  is not what users typically intend and is surprising.

Design:
- Add a dedicated `ModelsHelp` special-command variant to make the intent explicit.
- When the parser sees `/models` (case-insensitive, trimmed), it returns
  `SpecialCommand::ModelsHelp`.
- The chat loop (`run_chat`) recognizes `ModelsHelp` and calls `print_models_help()`,
  which prints a focused help message (usage, flags, examples).
- Keep `/models list` mapped to the existing `ListModels` command (no behavioral change).

Key code locations (high-level snippets):

- `SpecialCommand` enum (new variant):
```xzatoma/src/commands/special_commands.rs#L56-96
    /// List available models
    ///
    /// Shows all available models from the current provider.
    ListModels,

    /// Show help specific to the models command
    ///
    /// Useful when users type `/models` without any subcommand.
    ModelsHelp,
```

- Parser: map `/models` to the new variant:
```xzatoma/src/commands/special_commands.rs#L120-156
        // Model management commands and provider auth
        // `/models` with no subcommand prints help for model-management usage
        "/models" => SpecialCommand::ModelsHelp,
        "/models list" => SpecialCommand::ListModels,
```

- Print helper (`print_models_help`):
```xzatoma/src/commands/special_commands.rs#L200-260
pub fn print_models_help() {
    println!(
        r#"
Models Command - Usage and Examples
===================================

The `/models` command manages and inspects the provider's available models.

USAGE:
  /models                      - Show this help message for model-management
  /models list                 - Show available models from the current provider
      Flags:
        --json      - Output pretty-printed JSON (good for tooling like jq)
        --summary   - Output a compact summary suitable for scripting/comparison

  /models info <name>          - Show detailed information about a specific model
      Flags:
        --json      - Output model info as JSON
        --summary   - Output summarized detail

EXAMPLES:
  /models
  /models list
  /models list --json
  /models list --summary
  /models info gpt-4 --summary

NOTES:
  - `--json` prints pretty JSON (useful with `jq`)
  - `--summary` prints compact, script-friendly summaries
  - Use `/models` to see this help when you don't know which subcommand to run
"#
    );
}
```

- Chat loop handling (stop and print help; do not execute):
```xzatoma/src/commands/mod.rs#L220-228
                        SpecialCommand::ModelsHelp => {
                            print_models_help();
                            continue;
                        }
```

---

## Testing

New unit tests were added to validate parsing behavior and ensure regression safety:

- `test_parse_special_command_models_bare_returns_models_help`
- `test_parse_special_command_models_list_returns_list_models`

Example test snippets:
```xzatoma/src/commands/special_commands.rs#L468-492
    #[test]
    fn test_parse_special_command_models_bare_returns_models_help() {
        assert_eq!(parse_special_command("/models"), SpecialCommand::ModelsHelp);
    }

    #[test]
    fn test_parse_special_command_models_list_returns_list_models() {
        assert_eq!(
            parse_special_command("/models list"),
            SpecialCommand::ListModels
        );
    }
```

Quality gates:
- `cargo fmt --all` — applied
- `cargo check --all-targets --all-features` — passes
- `cargo clippy --all-targets --all-features -- -D warnings` — passes
- `cargo test --all-features` — all tests pass

All checks were run locally and completed successfully.

---

## Usage Examples

Interactive (chat) example — the user types `/models` and receives focused help:

```/dev/null/example.md#L1-8
[PLANNING][SAFE] >>> /models
Models Command - Usage and Examples
===================================

The `/models` command manages and inspects the provider's available models.

USAGE:
  /models                      - Show this help message for model-management
  /models list                 - Show available models from the current provider
      Flags:
        --json      - Output pretty-printed JSON (good for tooling like jq)
        --summary   - Output a compact summary suitable for scripting/comparison
...
```

Notes:
- The `/models` bare command is now an explicit "help" action; it does not attempt
  to list or inspect models.
- Users who want the list should use `/models list` explicitly; add `--json` or
  `--summary` flags as needed.

---

## Follow-ups & Recommendations

- Add a chat-mode parser branch for `/models info <name>` to show model information
  directly from the special-command parser if that is a desirable short-hand.
- Consider adding small integration tests that execute the CLI binary in a
  simulated chat session to assert stdout output for `/models` (end-to-end behavior
  validation).
- Consider adding a short note or example to the user-facing README showing how to
  use `/models` in chat mode for discoverability.

---

## References

- Implementation: `xzatoma/src/commands/special_commands.rs`
- Chat loop handling: `xzatoma/src/commands/mod.rs`
- Tests: `xzatoma/src/commands/special_commands.rs` (tests module)

---

If you'd like, I can follow up by adding an end-to-end integration test that launches
a controlled chat session and asserts that typing `/models` produces the models help
output (this would exercise the actual `run_chat` path and the printed output).
