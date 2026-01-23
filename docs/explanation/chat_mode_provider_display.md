# Chat mode provider & model display

## Overview

When running interactively in chat mode it is helpful for users to know which provider
and model the agent is currently using. This document explains the small UX change
that surfaces provider and model information in the interactive prompt and how it
was implemented and tested.

Example (plain):

```/dev/null/example.txt#L1-1
[PLANNING][SAFE][Copilot: gpt-5-mini] >>>
```

Example (colored):

```/dev/null/example.txt#L1-1
[PLANNING][SAFE][Copilot: gpt-5-mini] >>>   (Copilot is cyan, model is bold)
```

## Motivation

- Make it obvious what provider and model are active in an interactive session.
- Surface model switches immediately to avoid confusion during long sessions.
- Keep the prompt compact and non-intrusive while providing important context.

## Components Delivered

- Code changes:
  - `src/chat_mode.rs` — new prompt helpers:
    - `format_prompt_with_provider(provider: Option<&str>, model: Option<&str>) -> String`
    - `format_colored_prompt_with_provider(provider: Option<&str>, model: Option<&str>) -> String`
  - `src/commands/mod.rs` — chat loop updates to query the provider for the current model
    on every prompt render and use the provider-aware prompt helper when available.
- Tests:
  - `src/chat_mode.rs` unit tests:
    - `test_format_prompt_with_provider`
    - `test_format_colored_prompt_with_provider_contains_label`
- Docs:
  - This document: `docs/explanation/chat_mode_provider_display.md`

## Implementation details

- Non-breaking approach:
  - Existing helpers `format_prompt()` and `format_colored_prompt()` remain unchanged and
    continue to provide the original prompt format. New provider-aware helpers were added
    in `ChatModeState` to avoid breaking existing code/tests.
- Prompt construction:
  - In the interactive loop (`commands::chat::run_chat`) the loop now queries the current
    model from the agent's provider on each iteration:
    - `let current_model = agent.provider().get_current_model().ok();`
    - If a model is available the code calls `format_colored_prompt_with_provider(Some(provider_type), current_model.as_deref())`
      so the prompt reflects model switches immediately.
- Display formatting:
  - Provider label is displayed as `Provider: model` with a capitalized provider name
    (simple ASCII-first-letter capitalization).
  - The colored prompt uses the same color scheme as mode/safety tags and presents the
    provider label for improved visibility (provider in cyan, model bold in terminals that support ANSI).
- Fallbacks:
  - If the provider does not expose a current model or `get_current_model()` fails, the
    base prompt (without provider/model) is used — preserving previous behavior.

Relevant code references:

- Prompt helpers are defined in `src/chat_mode.rs` (see `ChatModeState` methods).
- The chat loop uses the provider-aware prompt when available (`src/commands/mod.rs`).

## Testing

- Unit tests cover the new helpers:
  - `test_format_prompt_with_provider` asserts the exact (uncolored) prompt string.
  - `test_format_colored_prompt_with_provider_contains_label` asserts the colored prompt
    contains the provider: model label.
- Manual validation:
  - Run an interactive session locally after authenticating with a provider and verify:
    - Provider and model appear in the prompt.
    - Switching models via `/switch_model` (or the interactive workflow) updates the prompt on the next prompt render.
- Quality gates:
  - `cargo fmt --all`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`

## Usage examples

Programmatic usage:

```/dev/null/example.rs#L1-6
use xzatoma::chat_mode::{ChatModeState, ChatMode, SafetyMode};

let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);

// When provider & model available:
let prompt = state.format_prompt_with_provider(Some("copilot"), Some("gpt-5-mini"));
// prompt == "[PLANNING][SAFE][Copilot: gpt-5-mini] >>> "
```

Interactive behavior:

- On startup the chat welcome banner is unchanged.
- The prompt will show `[Provider: model]` (when available) and will be updated if the
  active model is changed during the session.

## References

- Prompt helpers: `src/chat_mode.rs` (new methods)
- Chat loop integration: `src/commands/mod.rs` (prompt generation inside `run_chat`)
- Tests: see unit tests added to `src/chat_mode.rs`

## Validation / Acceptance criteria

- The prompt includes provider/model when the provider reports a current model.
- The prompt updates on model switches without restarting the session.
- Existing prompt behavior remains unchanged when a provider/model is not available.
- Code is covered by unit tests and follows project style requirements and Clippy rules.

If you'd like, I can:

- Add a small integration test that simulates a model switch and asserts the prompt
  contains the new model (would require a bit of harnessing around the readline mock).
- Make the provider label style configurable through `ChatConfig` (optional).
