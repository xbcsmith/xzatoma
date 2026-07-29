# Phase 4: ACP Informational Command Implementation

## Overview

This document describes the implementation of Phase 4 of the Chat Command
Unification plan: the ACP informational commands `/models`, `/models list`,
`/models info`, `/context info`, and `/context summary`.

All five commands were previously stubs that returned a
`handle_not_yet_implemented` message. After this phase they are fully
implemented and wired into `dispatch_stdio_command`.

## Changes

### `src/commands/special_commands.rs`

**Added `format_models_help_text() -> String`** (placed before
`print_models_help`).

The models help text was previously embedded directly inside `print_models_help`
as a `println!` argument. The new `format_models_help_text` function owns that
raw-string literal and returns it as a `String`. `print_models_help` now
delegates to it with `println!("{}", format_models_help_text())`.

This pattern mirrors every other `format_*_help_text` / `print_*_help` pair
already in the module (`format_mode_help_text`, `format_safety_help_text`, etc.)
and allows the ACP stdio layer to capture the text without spawning a subprocess
or capturing stdout.

### `src/acp/stdio.rs`

#### New handler functions

Five new functions were added between `handle_mcp_command` and
`dispatch_stdio_command`:

| Function                 | Signature                                                             | Description                                                                                                                                                 |
| ------------------------ | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `handle_models_help`     | `fn() -> String`                                                      | Returns `format_models_help_text()`. Pure, no session needed.                                                                                               |
| `handle_models_list`     | `async fn(&ActiveSessionState) -> String`                             | Calls `provider().list_models()`. Returns a bullet list of model names or a graceful error message.                                                         |
| `handle_models_info`     | `async fn(String, &ActiveSessionState) -> String`                     | Calls `provider().get_model_info()`. Returns a formatted block with name, display name, context window size, and capabilities.                              |
| `handle_context_info`    | `async fn(&ActiveSessionState) -> String`                             | Reads `agent.conversation().max_tokens()` and `agent.get_context_info(window)`. Returns a usage summary with tokens used, limit, remaining, and percentage. |
| `handle_context_summary` | `async fn(Option<String>, &Arc<Mutex<ActiveSessionState>>) -> String` | Calls `agent.conversation_mut().summarize_and_reset()`. Returns `"Conversation summarized. Context window reset."` or a graceful error.                     |

#### `dispatch_stdio_command` wiring

Five new arms were added to the `match parse_special_command(prompt_text)`
block, placed after the existing `SwitchModel` arm and before the catch-all
`_ => resolve_special_command_response(prompt_text)?`:

```rust
Ok(SpecialCommand::ModelsHelp) => handle_models_help(),
Ok(SpecialCommand::ListModels) => {
    let session_lock = session.lock().await;
    handle_models_list(&session_lock).await
}
Ok(SpecialCommand::ShowModelInfo(m)) => {
    let session_lock = session.lock().await;
    handle_models_info(m, &session_lock).await
}
Ok(SpecialCommand::ContextInfo) => {
    let session_lock = session.lock().await;
    handle_context_info(&session_lock).await
}
Ok(SpecialCommand::ContextSummary { model }) => {
    handle_context_summary(model, session).await
}
```

#### `resolve_special_command_response` stub removal

The five `handle_not_yet_implemented` stubs for `ModelsHelp`, `ListModels`,
`ShowModelInfo`, `ContextInfo`, and `ContextSummary` were replaced with
appropriate fallback strings:

- `ModelsHelp`: calls `format_models_help_text()` directly (pure, no session
  needed).
- `ListModels`, `ShowModelInfo`, `ContextInfo`, `ContextSummary`: return a
  `"requires a live session"` message consistent with the style of the
  `SwitchMode` and `SwitchSafety` arms.

After this change, `resolve_special_command_response` contains zero calls to
`handle_not_yet_implemented` for Phase 4 commands. The only remaining uses of
that helper are for `/status`, `/tools`, `/skills`, and `/mcp` which are
correctly intercepted before the resolver in `dispatch_stdio_command`.

#### Import update

`format_models_help_text` was added to the
`use crate::commands::special_commands::{...}` import block.

#### New tests

Three tests were added to the `mod tests` block in `src/acp/stdio.rs`:

- `test_dispatch_models_help_returns_models_help_text`: dispatches `/models`,
  asserts `EndTurn`, and confirms the response contains `"Models Command"`.
- `test_dispatch_context_info_returns_token_stats`: dispatches `/context info`,
  asserts `EndTurn`, and confirms the response contains `"tokens"`.
- `test_dispatch_context_summary_returns_confirmation`: dispatches
  `/context summary`, asserts `EndTurn`, and confirms the response contains
  `"summarized"`.

## Design Decisions

### `handle_models_help` as a one-line wrapper

The function body is a single `format_models_help_text()` call. This keeps the
same delegation pattern used for every other help handler (`handle_mode_help`
calls `format_mode_help_text()`, etc.) and makes the dispatch arm symmetric.

### `handle_context_summary` accepts but ignores `model`

The `ContextSummary` variant carries an optional model name intended for future
provider-based summarization (where the model generates a prose summary via an
API call). The current `Conversation::summarize_and_reset` implementation is
purely local and does not need a model. The parameter is accepted and prefixed
with `_model` to signal intentional non-use without breaking the public API
shape.

### Lock discipline in `handle_context_summary`

The function takes `&Arc<Mutex<ActiveSessionState>>` rather than
`&ActiveSessionState` because it needs to mutate the agent's conversation. To
avoid holding both the session lock and the agent lock simultaneously (which
would differ from every other handler), the session lock is dropped before the
agent lock is acquired:

```rust
let session_lock = session.lock().await;
let agent_handle = session_lock.xzatoma_agent.clone();
drop(session_lock);
let mut agent = agent_handle.lock().await;
```

This matches the pattern used in `handle_switch_model` and
`handle_toggle_subagents`.

## Success Criteria Verification

- `/models` in Zed returns the models help text (contains `"Models Command"`).
- `/models list` returns a list of model names from the current provider, or
  `"No models available."` when the provider returns none.
- `/models info <name>` returns a formatted info block or a graceful error.
- `/context info` returns token usage statistics containing `"tokens"`.
- `/context summary` compacts the conversation and replies with
  `"Conversation summarized. Context window reset."`.
- `resolve_special_command_response` contains no `handle_not_yet_implemented`
  calls for the five Phase 4 variants.

## Quality Gate Results

All four mandatory gates passed:

```text
cargo fmt --all                                         -- pass
cargo check --all-targets --all-features               -- pass
cargo clippy --all-targets --all-features -- -D warnings -- pass
cargo test --lib -- acp::stdio                         -- 127 passed, 0 failed
cargo test --lib -- commands::special_commands         -- 90 passed, 0 failed
```
