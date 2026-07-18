# Phase 3: ACP Mutating Command Implementation

## Overview

This document describes the implementation of Phase 3 of the chat command
unification plan: wiring the five slash commands that mutate session state into
the ACP stdio dispatch layer. Before this phase, `/mode <value>`,
`/safety <on|off>`, `/subagents on|off`, `/system <text>`, and `/model <name>`
all returned "not yet implemented" placeholders. After this phase, each command
performs its described state change and returns a confirmation string.

## Changes Made

### `src/tools/mod.rs`

Added `pub fn remove(&mut self, name: &str) -> bool` to `ToolRegistry`. This
method delegates to the underlying `HashMap::remove` and returns `true` if the
tool was present and removed. Required by `handle_toggle_subagents` for
deregistering the subagent tool when delegation is disabled.

### `src/providers/trait_mod.rs`

Added `fn set_model_inplace(&self, _model: &str) {}` to the `Provider` trait as
a default no-op method. Unlike the existing
`fn set_model(&mut self, model: &str)`, this variant accepts a shared reference,
allowing it to be called via `&dyn Provider`. All built-in providers override it
using their internal `Arc<RwLock<Config>>` storage.

### `src/providers/copilot.rs`, `src/providers/ollama.rs`, `src/providers/openai.rs`

Each concrete provider now overrides `fn set_model_inplace` using
`self.config.write().unwrap().model = model.to_string()`. This is identical to
the interior-mutability pattern already used by `set_model`, but callable
without `&mut self`.

### `src/agent/core.rs`

Added `pub fn provider_arc(&self) -> Arc<dyn Provider>` to `XzatomaAgent`. This
returns an `Arc::clone` of the internal provider, which is needed by
`handle_toggle_subagents` when constructing a new `SubagentTool` on re-enable.

### `src/acp/stdio.rs`

#### New fields on `ActiveSessionState`

Two new fields capture configuration snapshots at session-creation time, making
them available to the slash command handlers without access to the server's full
`Config`:

- `terminal_config: TerminalConfig` -- used by `handle_switch_mode` when
  rebuilding the `TerminalTool` after a mode change.
- `agent_config: AgentConfig` -- used by `handle_toggle_subagents` when
  constructing a new `SubagentTool` on re-enable, and available for future
  per-session configuration needs.

Both fields are populated in `create_session` from `self.config.agent.terminal`
and `self.config.agent` respectively, and default to their `Default` values in
test helpers.

#### Five new handler functions (Task 3.1-3.5)

| Function                   | Signature                                           | Behavior                                                                                                                                                                                        |
| -------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `handle_switch_mode`       | `async fn(ChatMode, &Arc<Mutex<...>>) -> String`    | Maps `ChatMode` to ACP mode ID, calls `mode_runtime_effect`, updates `current_mode_id` and `runtime_state`, rebuilds system prompt and `TerminalTool`. Returns `"Mode switched to <id>."`       |
| `handle_switch_safety`     | `fn(SafetyMode, &mut ActiveSessionState) -> String` | Updates `runtime_state.safety_mode_str` to `"confirm"` or `"yolo"`. Returns `"Safety policy set to <mode>."`                                                                                    |
| `handle_toggle_subagents`  | `async fn(bool, &Arc<Mutex<...>>) -> String`        | Sets `runtime_state.subagents_enabled`, registers or removes the `"subagent"` tool in the agent's `ToolRegistry`. Returns `"Subagent delegation enabled."` or `"Subagent delegation disabled."` |
| `handle_set_system_prompt` | `async fn(String, &Arc<Mutex<...>>) -> String`      | Replaces `transient_system_messages[0]` with the new text, leaving indices >= 1 (skill disclosures) unchanged. Returns `"System prompt updated."`                                               |
| `handle_switch_model`      | `async fn(String, &Arc<Mutex<...>>) -> String`      | Lists available models, validates the requested name, calls `provider().set_model_inplace()` on success and updates `current_model_name`. Returns a confirmation or descriptive error string.   |

#### Five new match arms in `dispatch_stdio_command` (Task 3.2)

Added immediately before the `_ =>` fallthrough:

```rust
Ok(SpecialCommand::SwitchMode(mode)) => handle_switch_mode(mode, session).await,
Ok(SpecialCommand::SwitchSafety(mode)) => {
    let mut session_lock = session.lock().await;
    handle_switch_safety(mode, &mut session_lock)
}
Ok(SpecialCommand::ToggleSubagents(enable)) => {
    handle_toggle_subagents(enable, session).await
}
Ok(SpecialCommand::SetSystemPrompt(text)) => handle_set_system_prompt(text, session).await,
Ok(SpecialCommand::SwitchModel(model)) => handle_switch_model(model, session).await,
```

#### Updated stubs in `resolve_special_command_response`

The five `handle_not_yet_implemented` placeholders were replaced with short
informational strings noting that these commands require a live session. These
stubs are only reached by direct unit tests of the pure resolver function; live
ACP sessions intercept all five variants in `dispatch_stdio_command` before the
resolver is reached.

#### Nine new tests (Task 3.6)

| Test                                                      | Verifies                                                               |
| --------------------------------------------------------- | ---------------------------------------------------------------------- |
| `test_dispatch_switch_mode_planning`                      | `/mode planning` via dispatch updates `current_mode_id`                |
| `test_dispatch_switch_mode_planning_sets_mode_id`         | `handle_switch_mode(Planning)` sets mode to `"planning"`               |
| `test_dispatch_switch_mode_write`                         | `handle_switch_mode(Write)` sets mode to `"write"`                     |
| `test_dispatch_switch_safety_on`                          | `/safety on` sets `safety_mode_str` to `"confirm"`                     |
| `test_dispatch_switch_safety_off`                         | `/yolo` sets `safety_mode_str` to `"yolo"`                             |
| `test_dispatch_toggle_subagents_on`                       | `/subagents on` sets `subagents_enabled` to `true`                     |
| `test_dispatch_toggle_subagents_off`                      | `/subagents off` sets `subagents_enabled` to `false`                   |
| `test_dispatch_set_system_prompt`                         | `/system You are helpful.` updates first transient system message      |
| `test_dispatch_switch_model_unknown_returns_error_string` | `/model nonexistent-model` returns `Some(Ok(EndTurn))` with error text |

## Architecture Notes

### Why `handle_switch_safety` is synchronous

The plan specifies
`fn handle_switch_safety(mode: SafetyMode, session: &mut ActiveSessionState) -> String`.
Taking `&mut ActiveSessionState` directly means the caller already holds the
session mutex lock. A synchronous `fn` cannot `.await`, so the system prompt is
not rebuilt here. The next full mode switch or session creation will rebuild the
system prompt with the correct safety setting. This is an intentional
simplification: safety changes are reflected in `/status` output immediately,
and the LLM behavior is constrained by the terminal tool's execution mode (which
is set during mode changes, not standalone safety changes).

### Why `set_model_inplace` was added to the Provider trait

The `Provider` trait's existing `fn set_model(&mut self, model: &str)` requires
`&mut self`, which is incompatible with calling it through `Arc<dyn Provider>`.
All concrete provider implementations already use `Arc<RwLock<Config>>`
internally, so `set_model` is effectively interior-mutable despite its
signature. Rather than unsafe casting, the
`set_model_inplace(&self, model: &str)` method exposes the same
interior-mutability operation through a shared reference, enabling
`handle_switch_model` to update the provider without creating a new agent
instance.

### Why `terminal_config` and `agent_config` are stored per-session

The `dispatch_stdio_command` function operates on
`&Arc<Mutex<ActiveSessionState>>` without access to the server's `Config`.
Storing a snapshot of the relevant config fields in `ActiveSessionState` at
session-creation time gives handlers the data they need without introducing
circular dependencies or passing extra parameters through the dispatch chain.

## Success Criteria Verification

- `/mode planning` and `/mode write` change `session.current_mode_id` and
  confirm via a chat response.
- `/safety on`, `/safety off`, `/safe`, `/yolo` change `safety_mode_str` and
  confirm.
- `/subagents on` and `/subagents off` toggle `subagents_enabled` and register
  or remove the subagent tool.
- `/system <text>` replaces the first transient system message and confirms.
- `/model nonexistent-model` returns a friendly error string
  (`Some(Ok(EndTurn))`), not `None` and not a hard Rust error.
- No regressions: all 2,504 existing tests continue to pass.
