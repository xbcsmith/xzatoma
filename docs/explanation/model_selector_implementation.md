# Model Selector Session Config Option Implementation

This document explains the design and implementation of the model selector
dropdown added to XZatoma's ACP session configuration.

## Overview

XZatoma advertises session configuration dropdowns to Zed via the Agent-Client
Protocol. The model selector is the ninth such option, allowing users to switch
between models available in the current provider without restarting the agent
subprocess.

## Changes in `src/acp/session_config.rs`

### New Constant: `CONFIG_MODEL`

A new public constant `CONFIG_MODEL: &str = "model"` was added after
`CONFIG_SESSION_MODE`. This is the stable config option ID used in all ACP
payloads exchanged with Zed.

### Extended `ConfigChangeEffect`

A new field `model_name: Option<String>` was added to `ConfigChangeEffect`. When
`Some`, the caller (in `stdio.rs`) must update
`ActiveSessionState.current_model_name` and call the appropriate provider method
to switch models in-flight.

### Extended `SessionRuntimeState`

Two new fields were added:

- `current_model: String` - the active model name, initialized from the provider
  config block (`copilot.model`, `ollama.model`, or `openai.model`) at session
  creation via `SessionRuntimeState::from_config`.

- `available_models: Vec<String>` - the list of models fetched from the
  provider. This starts empty and is populated asynchronously by `stdio.rs` at
  session creation. The model selector falls back gracefully to showing only
  `current_model` when this list is empty.

### New Builder: `build_model_selector_option`

A private function that constructs the `acp::SessionConfigOption` for the model
selector. It:

1. Builds select options from `runtime.available_models`.
2. Inserts `current_model` at the front of the list when it is not already
   present, ensuring the dropdown always shows a valid selection even before the
   async model listing completes.
3. Uses `String::clone()` rather than `&str` references when constructing
   `acp::SessionConfigSelectOption` values, because `SessionConfigValueId` does
   not implement `From<&String>` - only `From<&'static str>` and `From<String>`.

### Updated `apply_config_option_change`

A new `CONFIG_MODEL` arm was added to the match block:

- When `runtime.available_models` is empty (listing failed or not yet
  populated), any model name is accepted so the UI is never blocked.
- When `runtime.available_models` is non-empty, the requested model must appear
  in the list; otherwise an `XzatomaError::Config` error is returned.

### Module Doc Table

The option overview table in the module doc comment was updated to include the
`model` row and reordered so `session_mode` appears first (matching the
`build_session_config_options` return order).

## Changes in `src/acp/stdio.rs`

The session configuration changes in `session_config.rs` are wired into the live
session lifecycle by `stdio.rs`.

### Ollama auto-model resolution

A private async helper `resolve_agent_ollama_model` was added. When the
effective provider is `ollama` and no explicit `--model` flag was passed, it
calls `providers::ollama::resolve_available_model` to fetch the currently
running model from the Ollama API. This ensures that agent mode behaves like
interactive chat mode: the latest available model is used when no model is
specified, rather than falling back to the hard-coded config default
(`llama3.2:latest`).

### Model list fetch at session creation

At the end of `create_session`, after the provider is initialized, the model
list is fetched with a bounded timeout:

```rust
let available_models: Vec<String> = match tokio::time::timeout(
    Duration::from_secs(self.config.acp.stdio.model_list_timeout_seconds),
    provider.list_models(),
)
```

On success, the names are extracted into a `Vec<String>` and stored in
`runtime_state.available_models`. On timeout or error, `available_models`
remains empty so the session is not blocked. The `current_model` field is set to
the resolved model name (including the Ollama auto-resolved value) so the
dropdown always has a sensible initial selection.

### Config-option change handler

`set_session_config_option` now acts on `ConfigChangeEffect.model_name`:

1. Inside the session lock, `session_lock.current_model_name` and
   `session_lock.runtime_state.current_model` are updated to the new name.
2. Outside the session lock (to avoid lock-ordering inversion with the prompt
   worker), `agent_lock.provider().set_model_inplace(new_model)` is called so
   the provider uses the selected model for the next request.

### Slash-command sync

`handle_switch_model` (the `/model <name>` handler) was updated to also write
the new name back into `session_lock.runtime_state.current_model` in addition to
`session_lock.current_model_name`, keeping the ACP config option value in sync
with the slash-command switch path.
