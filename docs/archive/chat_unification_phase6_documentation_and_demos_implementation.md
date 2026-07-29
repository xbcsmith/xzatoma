# Phase 6: Documentation and Demos

## Overview

This document describes the implementation of Phase 6 of the Chat Command
Unification plan: the full documentation suite for the unified slash command UX.

Phase 6 creates three new documentation files, a complete demo directory,
updates seven existing documents, and creates this implementation summary.

## New Files Created

### `docs/reference/chat_commands.md`

A comprehensive reference for all 13 advertised slash commands. Structure:

- Overview table with bare behavior, status behavior, and example action
- Unified UX contract section (bare = help, `status` = inspect, action = change)
- Per-command sections for each of the 13 commands, each with Purpose, Usage,
  Arguments/aliases, and ACP notes where relevant
- Commands without a status subcommand table

### `docs/tutorials/chat_commands.md`

A step-by-step tutorial for new users walking through seven steps:

1. Starting a chat session
2. Discovering commands with `/help`
3. Checking current settings with the `status` subcommand
4. Changing mode, safety, model, system prompt, and subagent delegation
5. Managing long sessions with `/context info` and `/context summary`
6. Understanding `/streaming` in Zed ACP mode
7. Exiting the session

All model examples use `granite4:3b` (Ollama).

### `docs/how-to/use_chat_commands.md`

A task-oriented how-to guide covering seven tasks:

1. Check the current mode (`/mode status`)
2. Switch modes mid-session (`/mode planning`, `/mode write`)
3. Inspect and change the safety policy (`/safety status`, `/safety on|off`)
4. Replace the system prompt and verify it (`/system <text>`, `/system status`)
5. Enable and disable subagents per-session (`/subagents on|off`)
6. Inspect and switch the active model (`/model status`, `/model <name>`)
7. Manage context window pressure (`/context summary`, `/context info`)

## New Demo Directory: `demos/chat_commands/`

A self-contained demo following the exact conventions of `demos/chat/`.

| File                             | Description                                                                                      |
| -------------------------------- | ------------------------------------------------------------------------------------------------ |
| `README.md`                      | Prerequisites, setup, run, expected output, reset, troubleshooting                               |
| `config.yaml`                    | Ollama provider, `granite4:3b` model, sandboxed to `tmp/`                                        |
| `setup.sh`                       | Creates `tmp/output/`, checks xzatoma, Ollama, and model                                         |
| `run.sh`                         | Pipes `input/commands_demo_script.txt` to `xzatoma chat`, tees to `tmp/output/commands_demo.txt` |
| `reset.sh`                       | Removes `tmp/xzatoma.db` and `tmp/output/` files                                                 |
| `input/commands_demo_script.txt` | 15-command script exercising the unified UX                                                      |
| `tmp/.gitignore`                 | Excludes generated runtime files                                                                 |
| `tmp/output/.gitkeep`            | Preserves output directory in git                                                                |

The demo script exercises all key unified commands in sequence:

```text
/help
/mode status
/mode planning
/mode status
/safety status
/safety off
/safety status
/model status
/streaming
/system status
/system You are a concise assistant. Reply in one sentence.
/system status
/subagents status
/context info
exit
```

## Existing Documents Updated

### `docs/reference/quick_reference.md`

Added a "Chat Slash Commands" subsection under "Interactive Chat" with a
three-column table (Command, Status form, Action form) covering all 13 commands
and a link to `docs/reference/chat_commands.md`.

### `docs/how-to/use_chat_modes.md`

- Replaced the old 7-row command reference table with an 18-row table that
  includes all unified commands and the `status` forms for each.
- Added a sentence above the table explaining the unified UX contract.
- Fixed all pre-existing MD040 lint errors (15 unlabeled fenced code blocks
  changed from bare ` ``` ` to ` ```text ` or ` ```bash `).
- Removed a documentation-generation artifact at the end of the file.

### `docs/how-to/configure_system_prompt.md`

Added a "Inspect the current prompt" example showing `/system status` and its
expected output immediately after the existing "Change the prompt" example.
Added a note that bare `/system` now shows help text instead of returning an
error.

### `docs/explanation/chat_modes_architecture.md`

- Replaced the 7-variant `SpecialCommand` enum code block with a 32-variant
  block showing all variants added in Phases 1-5, grouped by category.
- Added the unified UX contract bullet list.
- Updated the `/system` Command section to show all three forms (bare, status,
  text argument) and corrected the "returns an error" note.

### `docs/reference/system_prompt.md`

Replaced the "Interactive `/system` Command" section with a three-section
expansion covering:

- Replacing the system prompt (existing behaviour)
- Inspecting the current prompt (`/system status` - new)
- Bare invocation (now shows help text instead of an error - new)

### `docs/reference/acp_configuration.md`

Added a new "ACP slash commands" section before "Related documentation"
describing the unified UX contract and the 13 advertised commands, with a link
to `docs/reference/chat_commands.md`.

### `docs/explanation/implementations.md`

Added a new "Chat Command Unification (2026-07-18)" index entry summarising all
six phases, the key files changed, and links to the four phase documentation
files.

## Quality Gate Results

All mandatory gates passed:

```text
cargo fmt --all                                          -- pass (no Rust changes)
cargo check --all-targets --all-features                -- pass
cargo clippy --all-targets --all-features -- -D warnings -- pass
cargo test --lib                                         -- 2511 passed, 0 failed
markdownlint + prettier on all new and updated files     -- pass
```
