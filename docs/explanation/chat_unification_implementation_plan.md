# Chat Command Unification Implementation Plan

## Overview

This plan unifies the UX for all chat slash commands so they follow a single
consistent contract: typing a command bare (e.g. `/streaming`) shows per-command
help, and appending `status` (e.g. `/streaming status`) shows the current live
state. Six commands currently violate this contract by returning error messages
on bare invocation or by silently mutating state instead of showing help. This
plan also implements the many ACP-mode commands that are currently gated behind
`handle_not_yet_implemented` stubs, updates the Zed ACP advertisement layer, and
produces a full documentation suite covering how-to guides, reference material,
tutorials, and a self-contained demo.

---

## Current State Analysis

### Existing Infrastructure

- `src/commands/special_commands.rs` -- defines `SpecialCommand`,
  `CommandError`, `parse_special_command()`, `format_help_text()`,
  `format_mention_help_text()`, and `print_models_help()`. The parser handles
  all known slash commands via a flat `match` on the lowercased input string.
- `src/acp/stdio.rs` -- `dispatch_stdio_command()` is the ACP-mode entry point
  for all slash commands. It handles `/tools`, `/skills`, `/mcp`, and `/status`
  with live session state; everything else falls through to the pure
  `resolve_special_command_response()` function. Twelve commands are blocked
  behind `handle_not_yet_implemented()` stubs (phase comments: Phases 3-6 from
  the prior ACP plan).
- `src/acp/available_commands.rs` -- `build_available_commands()` advertises 12
  slash commands to Zed via ACP. Input hints describe the parameter for commands
  that accept arguments.
- `docs/reference/system_prompt.md`, `docs/how-to/use_chat_modes.md`,
  `docs/explanation/chat_modes_architecture.md` -- existing docs that reference
  command syntax and will need updating.

### Identified Issues

#### Issue 1: Bare commands return errors instead of help

`/mode`, `/safety`, `/model`, `/system`, and `/streaming` all return a
`CommandError::MissingArgument` when typed alone. The error text references
`usage:` but is not formatted as proper help. Users must already know the
correct syntax to avoid the error, which is the opposite of discoverable UX.

#### Issue 2: Bare `/subagents` mutates state silently

`/subagents` with no argument currently parses to `ToggleSubagents(true)`,
unconditionally enabling subagents. A user typing `/subagents` expecting
information gets a silent state change instead.

#### Issue 3: No per-command status subcommand

None of the commands support `/<command> status` to show the current value of
the setting they control. The global `/status` command gives an abbreviated
one-liner for mode, safety, model, and subagents, but there is no way to inspect
the active system prompt text or the streaming state from inside a session.

#### Issue 4: ACP mutating commands are not implemented

`SwitchMode`, `SwitchSafety`, `SwitchModel`, `ToggleSubagents`, and
`SetSystemPrompt` all resolve to `handle_not_yet_implemented` in
`resolve_special_command_response()`. Users of the Zed ACP interface cannot
change any session settings via slash commands.

#### Issue 5: ACP informational commands are not implemented

`ListModels`, `ModelsHelp`, `ShowModelInfo`, `ContextInfo`, and `ContextSummary`
also resolve to `handle_not_yet_implemented`. Users cannot inspect models or
manage context from the Zed chat window.

#### Issue 6: Zed ACP advertisement does not reflect unified UX

`build_available_commands()` describes `/system` as requiring a text argument
and describes other commands without mentioning the `status` subcommand or the
bare-equals-help pattern. The hints will mislead users after the UX change.

---

## Implementation Phases

### Phase 1: Parser Foundation -- Bare Help and Status Subcommands

Establish the uniform command contract in `src/commands/special_commands.rs`.
All later phases depend on the new `SpecialCommand` variants added here.

#### Task 1.1 Add Per-Command Help Variants to `SpecialCommand`

Add the following variants to the `SpecialCommand` enum in
`src/commands/special_commands.rs`:

- `ShowModeHelp` -- emitted by bare `/mode`
- `ShowSafetyHelp` -- emitted by bare `/safety`
- `ShowModelHelp` -- emitted by bare `/model`
- `ShowStreamingHelp` -- emitted by bare `/streaming`
- `ShowSystemHelp` -- emitted by bare `/system`
- `ShowSubagentsHelp` -- emitted by bare `/subagents`

Each variant represents "the user wants documentation for this command" and
carries no payload.

#### Task 1.2 Add Per-Command Status Variants to `SpecialCommand`

Add the following variants to the `SpecialCommand` enum:

- `ShowModeStatus` -- emitted by `/mode status`
- `ShowSafetyStatus` -- emitted by `/safety status`
- `ShowModelStatus` -- emitted by `/model status`
- `ShowStreamingStatus` -- emitted by `/streaming status`
- `ShowSystemStatus` -- emitted by `/system status`
- `ShowSubagentsStatus` -- emitted by `/subagents status`

Each `ShowXxxStatus` variant requires live session state in the ACP dispatcher
(analogous to the existing `ShowStatus`, `ListTools`, `ListSkills`, and
`ShowMcpStatus` variants).

#### Task 1.3 Update `parse_special_command()` for All Six Commands

For each of the six commands, update `parse_special_command()` in
`src/commands/special_commands.rs` as follows:

**`/mode`**

```text
bare "/mode"              -> Ok(ShowModeHelp)
"/mode status"            -> Ok(ShowModeStatus)
"/mode planning"          -> Ok(SwitchMode(ChatMode::Planning))   (unchanged)
"/mode write"             -> Ok(SwitchMode(ChatMode::Write))      (unchanged)
"/mode <other>"           -> Err(UnsupportedArgument)             (unchanged)
```

**`/safety`**

```text
bare "/safety"            -> Ok(ShowSafetyHelp)
"/safety status"          -> Ok(ShowSafetyStatus)
"/safety on"              -> Ok(SwitchSafety(AlwaysConfirm))      (unchanged)
"/safety off"             -> Ok(SwitchSafety(NeverConfirm))       (unchanged)
"/safety <other>"         -> Err(UnsupportedArgument)             (unchanged)
```

**`/model`**

```text
bare "/model"             -> Ok(ShowModelHelp)
"/model status"           -> Ok(ShowModelStatus)
"/model <name>"           -> Ok(SwitchModel(name))                (unchanged)
```

The `/model status` arm must be matched before the generic `/model <name>` arm.

**`/streaming`**

```text
bare "/streaming"         -> Ok(ShowStreamingHelp)
"/streaming status"       -> Ok(ShowStreamingStatus)
"/streaming on|enable"    -> Ok(ToggleStreaming(true))             (unchanged)
"/streaming off|disable"  -> Ok(ToggleStreaming(false))            (unchanged)
"/streaming <other>"      -> Err(UnsupportedArgument)
```

The current catch-all arm that returns `MissingArgument` for all other
`/streaming ...` input should be replaced with `UnsupportedArgument` so the
error message is accurate.

**`/system`**

```text
bare "/system"            -> Ok(ShowSystemHelp)
"/system status"          -> Ok(ShowSystemStatus)
"/system <text>"          -> Ok(SetSystemPrompt(text))            (unchanged)
```

The `/system status` arm must be matched before the generic `/system <text>` arm
(match against the lowercased input, but extract the actual prompt text from the
original case-preserving `trimmed` variable for `SetSystemPrompt`).

**`/subagents`**

```text
bare "/subagents"         -> Ok(ShowSubagentsHelp)
"/subagents status"       -> Ok(ShowSubagentsStatus)
"/subagents on|enable"    -> Ok(ToggleSubagents(true))            (unchanged)
"/subagents off|disable"  -> Ok(ToggleSubagents(false))           (unchanged)
"/subagents <other>"      -> Err(UnsupportedArgument)             (unchanged)
```

The current bare `/subagents` arm that returns `ToggleSubagents(true)` must be
replaced with `ShowSubagentsHelp`.

#### Task 1.4 Add Per-Command Help Text Formatters

Add the following public `format_*_help_text()` functions to
`src/commands/special_commands.rs`. Each function returns a `String` and is
analogous to the existing `format_help_text()` and `format_mention_help_text()`.

- `format_mode_help_text()` -- usage for `/mode`, lists planning/write, mentions
  `/mode status`
- `format_safety_help_text()` -- usage for `/safety`, lists on/off and shorthand
  aliases (`/safe`, `/yolo`), mentions `/safety status`
- `format_model_help_text()` -- usage for `/model` and `/models`, mentions
  `/model status`
- `format_streaming_help_text()` -- usage for `/streaming`, explains when
  streaming is available (terminal chat mode vs ACP), mentions
  `/streaming status`
- `format_system_help_text()` -- usage for `/system <text>`, explains precedence
  and skill disclosure preservation, mentions `/system status`
- `format_subagents_help_text()` -- usage for `/subagents`, explains delegation,
  lists on/off aliases, mentions `/subagents status`

Each formatter must include:

1. A one-line summary of what the command does.
2. A `USAGE:` block listing all valid invocations.
3. An `EXAMPLES:` block with two to three concrete examples.
4. A `NOTE:` line that calls out the `status` subcommand.

#### Task 1.5 Update `format_help_text()`

In the global `format_help_text()`, add a note at the top of each section
stating the general contract: "Type the command alone for per-command help; add
`status` to see the current value." Update the `STREAMING:` section to reference
`/streaming status`, and the `SYSTEM PROMPT:` section to reference
`/system status`.

#### Task 1.6 Testing Requirements

Add or update tests in `src/commands/special_commands.rs`:

- `test_parse_mode_bare_returns_show_mode_help` -- `/mode` -> `ShowModeHelp`
- `test_parse_mode_status_returns_show_mode_status` -- `/mode status` ->
  `ShowModeStatus`
- `test_parse_safety_bare_returns_show_safety_help` -- `/safety` ->
  `ShowSafetyHelp`
- `test_parse_safety_status_returns_show_safety_status` -- `/safety status` ->
  `ShowSafetyStatus`
- `test_parse_model_bare_returns_show_model_help` -- `/model` -> `ShowModelHelp`
- `test_parse_model_status_returns_show_model_status` -- `/model status` ->
  `ShowModelStatus`
- `test_parse_model_status_not_treated_as_model_name` -- verify `SwitchModel` is
  not returned for `/model status`
- `test_parse_streaming_bare_returns_show_streaming_help` -- `/streaming` ->
  `ShowStreamingHelp`
- `test_parse_streaming_status_returns_show_streaming_status` --
  `/streaming status` -> `ShowStreamingStatus`
- `test_parse_streaming_invalid_arg_returns_unsupported_argument_error` --
  `/streaming maybe` -> `Err(UnsupportedArgument)` (not `MissingArgument`)
- `test_parse_system_bare_returns_show_system_help` -- `/system` ->
  `ShowSystemHelp`
- `test_parse_system_status_returns_show_system_status` -- `/system status` ->
  `ShowSystemStatus`
- `test_parse_system_status_not_treated_as_prompt_text` -- verify
  `SetSystemPrompt` is not returned for `/system status`
- `test_parse_subagents_bare_returns_show_subagents_help` -- `/subagents` ->
  `ShowSubagentsHelp`
- `test_parse_subagents_status_returns_show_subagents_status` --
  `/subagents status` -> `ShowSubagentsStatus`
- `test_format_mode_help_text_contains_status_note`
- `test_format_safety_help_text_contains_status_note`
- `test_format_streaming_help_text_contains_status_note`
- `test_format_system_help_text_contains_status_note`
- `test_format_subagents_help_text_contains_status_note`
- Update `test_parse_streaming_no_arg_returns_missing_argument_error` to
  `test_parse_streaming_bare_returns_show_streaming_help` (rename and invert).

#### Task 1.7 Deliverables

- [ ] `src/commands/special_commands.rs` -- 12 new `SpecialCommand` variants
      (`ShowModeHelp`, `ShowModeStatus`, `ShowSafetyHelp`, `ShowSafetyStatus`,
      `ShowModelHelp`, `ShowModelStatus`, `ShowStreamingHelp`,
      `ShowStreamingStatus`, `ShowSystemHelp`, `ShowSystemStatus`,
      `ShowSubagentsHelp`, `ShowSubagentsStatus`)
- [ ] `src/commands/special_commands.rs` -- 6 new `format_*_help_text()` public
      functions
- [ ] `src/commands/special_commands.rs` -- updated `parse_special_command()`
      arms for all 6 commands
- [ ] `src/commands/special_commands.rs` -- updated `format_help_text()`
- [ ] All new and updated unit tests pass; `cargo test` is clean

#### Task 1.8 Success Criteria

- Typing `/mode` in any context (terminal or ACP) no longer returns an error; it
  returns the mode help string.
- Typing `/streaming` no longer returns a `MissingArgument` error; it returns
  streaming help.
- Typing `/subagents` no longer silently enables subagents; it returns subagents
  help.
- `/mode status`, `/safety status`, `/model status`, `/streaming status`,
  `/system status`, `/subagents status` all parse to the corresponding
  `ShowXxxStatus` variant without error.
- All existing tests continue to pass.

---

### Phase 2: ACP Status Handlers

Wire the new `ShowXxxStatus` variants and `ShowXxxHelp` variants into the ACP
dispatch layer in `src/acp/stdio.rs`. Status variants require live session state
and are handled inside `dispatch_stdio_command`. Help variants are pure and are
handled in `resolve_special_command_response`.

#### Task 2.1 Add Per-Command Status Handler Functions

Add the following private `async fn handle_*_status` functions to
`src/acp/stdio.rs`, following the existing pattern of `handle_status_command`,
`handle_tools_command`, and `handle_mcp_command`:

**`handle_mode_status(session: &ActiveSessionState) -> String`**

Returns a multi-line block:

```text
Current mode: <session.current_mode_id>
<mode description from session_mode.rs>
```

**`handle_safety_status(session: &ActiveSessionState) -> String`**

Returns a multi-line block:

```text
Current safety policy: <session.runtime_state.safety_mode_str>
<description of what the policy means>
```

**`handle_model_status(session: &ActiveSessionState) -> String`**

Returns a multi-line block:

```text
Current model: <session.current_model_name>
Provider: <provider type from config>
```

**`handle_streaming_status(session: &ActiveSessionState) -> String`**

In ACP mode, streaming is controlled by Zed, not by the agent. Returns:

```text
Streaming: controlled by Zed client (ACP mode)
/streaming on|off has no effect in this session.
```

This is the correct informational response; `/streaming` is a no-op in ACP mode
but the user deserves an explanation rather than silence.

**`handle_system_status(session: &ActiveSessionState) -> String`**

Reads the first system message from the agent's conversation history. Returns:

```text
Current system prompt:
<system prompt text>
```

If no system prompt is set, returns
`"No system prompt is active for this session."`. This is the only status
handler that exposes content the user originally wrote; it must read the system
message from `session.xzatoma_agent.lock().await` analogous to how
`handle_skills_command` reads `transient_system_messages()`.

**`handle_subagents_status(session: &ActiveSessionState) -> String`**

Returns:

```text
Subagent delegation: <enabled|disabled>
```

Uses `session.runtime_state.subagents_enabled`.

#### Task 2.2 Wire Status Variants in `dispatch_stdio_command`

In `dispatch_stdio_command` in `src/acp/stdio.rs`, add match arms for each new
`ShowXxxStatus` variant immediately after the existing `ShowStatus` arm:

```rust
Ok(SpecialCommand::ShowModeStatus) => {
    let session_lock = session.lock().await;
    handle_mode_status(&session_lock)
}
Ok(SpecialCommand::ShowSafetyStatus) => {
    let session_lock = session.lock().await;
    handle_safety_status(&session_lock)
}
Ok(SpecialCommand::ShowModelStatus) => {
    let session_lock = session.lock().await;
    handle_model_status(&session_lock)
}
Ok(SpecialCommand::ShowStreamingStatus) => {
    let session_lock = session.lock().await;
    handle_streaming_status(&session_lock)
}
Ok(SpecialCommand::ShowSystemStatus) => {
    let session_lock = session.lock().await;
    handle_system_status(&session_lock).await
}
Ok(SpecialCommand::ShowSubagentsStatus) => {
    let session_lock = session.lock().await;
    handle_subagents_status(&session_lock)
}
```

`handle_system_status` is `async` because it must lock the agent to read
conversation history.

#### Task 2.3 Wire Help Variants in `resolve_special_command_response`

In `resolve_special_command_response` in `src/acp/stdio.rs`, add match arms for
each new `ShowXxxHelp` variant, calling the corresponding `format_*_help_text()`
function from `special_commands.rs`:

```rust
Ok(SpecialCommand::ShowModeHelp)       => format_mode_help_text(),
Ok(SpecialCommand::ShowSafetyHelp)     => format_safety_help_text(),
Ok(SpecialCommand::ShowModelHelp)      => format_model_help_text(),
Ok(SpecialCommand::ShowStreamingHelp)  => format_streaming_help_text(),
Ok(SpecialCommand::ShowSystemHelp)     => format_system_help_text(),
Ok(SpecialCommand::ShowSubagentsHelp)  => format_subagents_help_text(),
```

Remove the two `Err(CommandError::MissingArgument)` special-case arms that
currently intercept bare `/mode` and bare `/model` -- those inputs now parse to
`Ok(ShowModeHelp)` and `Ok(ShowModelHelp)` respectively after Phase 1.

#### Task 2.4 Testing Requirements

Add tests in `src/acp/stdio.rs` (in the existing `mod tests` block):

- `test_dispatch_mode_bare_returns_mode_help` -- dispatch `/mode`, assert
  response text contains the mode help header.
- `test_dispatch_mode_status_returns_current_mode` -- dispatch `/mode status`,
  assert response text contains `"Current mode:"`.
- `test_dispatch_safety_status_returns_current_safety` -- dispatch
  `/safety status`, assert response text contains `"Current safety policy:"`.
- `test_dispatch_model_status_returns_current_model` -- dispatch
  `/model status`, assert response text contains `"Current model:"`.
- `test_dispatch_streaming_status_returns_acp_note` -- dispatch
  `/streaming status`, assert response text contains `"controlled by Zed"`.
- `test_dispatch_system_bare_returns_system_help` -- dispatch `/system`, assert
  response text contains the system help header.
- `test_dispatch_system_status_returns_system_prompt` -- dispatch
  `/system status` with a session that has a system prompt, assert response
  contains the prompt text.
- `test_dispatch_system_status_no_prompt_returns_none_message` -- dispatch
  `/system status` with a session that has no system prompt, assert response
  contains `"No system prompt"`.
- `test_dispatch_subagents_bare_returns_subagents_help` -- dispatch
  `/subagents`, assert response text contains the subagents help header.
- `test_dispatch_subagents_status_returns_enabled_state` -- dispatch
  `/subagents status`, assert response text contains `"enabled"` or
  `"disabled"`.
- `test_resolve_bare_mode_returns_mode_help` -- call
  `resolve_special_command_response("/mode")`, verify it returns help text (not
  `None` and not an error message).

Use the existing `dispatch_test_session` and `dispatch_test_state` helpers.

#### Task 2.5 Deliverables

- [ ] `src/acp/stdio.rs` -- 6 new `handle_*_status` private functions
- [ ] `src/acp/stdio.rs` -- 6 new match arms in `dispatch_stdio_command`
- [ ] `src/acp/stdio.rs` -- 6 new match arms in
      `resolve_special_command_response` and removal of the 2 `MissingArgument`
      special-case arms
- [ ] All new tests pass; `cargo test` is clean

#### Task 2.6 Success Criteria

- Typing `/mode`, `/safety`, `/model`, `/streaming`, `/system`, `/subagents` in
  Zed returns the per-command help text, not an error.
- Typing `/<command> status` in Zed returns the current value for that setting.
- `/system status` shows the active system prompt text.
- No regressions in the existing dispatch test suite.

---

### Phase 3: ACP Mutating Command Implementation

Implement the five slash commands that change session state. These are currently
gated behind `handle_not_yet_implemented` in `resolve_special_command_response`.

#### Task 3.1 Implement `/mode <value>` -- `SwitchMode` Handler

The `set_session_mode` function already exists in `src/acp/stdio.rs` and is
called when Zed sends an ACP `SetSessionMode` request. Reuse that path from the
slash command handler:

Add
`async fn handle_switch_mode(mode: ChatMode, session: &Arc<Mutex<ActiveSessionState>>) -> String`
in `src/acp/stdio.rs`. This function must:

1. Lock the session, extract the new mode's runtime effect from
   `mode_runtime_effect(&mode_id)`.
2. Update `session.current_mode_id`, `session.runtime_state`, and the agent's
   tool registry (same steps as `set_session_mode`).
3. Return a confirmation string: `"Mode switched to <mode_id>."`.

Wire into `dispatch_stdio_command`:

```rust
Ok(SpecialCommand::SwitchMode(mode)) => {
    handle_switch_mode(mode, session).await
}
```

Remove the `handle_not_yet_implemented` arm for `SwitchMode` in
`resolve_special_command_response`.

#### Task 3.2 Implement `/safety <on|off>` -- `SwitchSafety` Handler

Add
`fn handle_switch_safety(mode: SafetyMode, session: &mut ActiveSessionState) -> String`
in `src/acp/stdio.rs`. This function must:

1. Update `session.runtime_state.safety_mode_str` to the string representation
   of the new mode.
2. Update `session.runtime_state.allow_dangerous` based on whether `mode` is
   `NeverConfirm`.
3. Return a confirmation string: `"Safety policy set to <mode>."`.

Wire into `dispatch_stdio_command` with a session lock:

```rust
Ok(SpecialCommand::SwitchSafety(mode)) => {
    let mut session_lock = session.lock().await;
    handle_switch_safety(mode, &mut session_lock)
}
```

Remove the `handle_not_yet_implemented` arm for `SwitchSafety` in
`resolve_special_command_response`.

#### Task 3.3 Implement `/subagents on|off` -- `ToggleSubagents` Handler

Add
`async fn handle_toggle_subagents(enable: bool, session: &Arc<Mutex<ActiveSessionState>>) -> String`
in `src/acp/stdio.rs`. This function must:

1. Lock the session and set `session.runtime_state.subagents_enabled = enable`.
2. Register or deregister the subagent tools on the agent's `ToolRegistry` (the
   same operations described in Task 6.1 of the prior ACP plan).
3. Return a confirmation string: `"Subagent delegation <enabled|disabled>."`.

Wire into `dispatch_stdio_command`.

Remove the `handle_not_yet_implemented` arm for `ToggleSubagents` in
`resolve_special_command_response`.

#### Task 3.4 Implement `/system <text>` -- `SetSystemPrompt` Handler

Add
`async fn handle_set_system_prompt(text: String, session: &Arc<Mutex<ActiveSessionState>>) -> String`
in `src/acp/stdio.rs`. This function must:

1. Lock the session and call `session.xzatoma_agent.lock().await`.
2. Replace the first `Role::System` message in the agent's conversation history
   with `Message::system(text)`. If no system message exists, prepend one.
3. Leave skill disclosure transient messages (indices >= 1) untouched.
4. Return a confirmation string: `"System prompt updated."`.

Wire into `dispatch_stdio_command`.

Remove the `handle_not_yet_implemented` arm for `SetSystemPrompt` in
`resolve_special_command_response`.

#### Task 3.5 Implement `/model <name>` -- `SwitchModel` Handler

The `set_session_model` function already exists in `src/acp/stdio.rs`. Reuse it:

Add
`async fn handle_switch_model(model: String, session: &Arc<Mutex<ActiveSessionState>>) -> Result<String>`
in `src/acp/stdio.rs`. This function must:

1. Call `set_session_model(&model, session).await`.
2. On success, return `"Model switched to <model>."`.
3. On error (model not found or provider rejection), return a descriptive error
   string instead of propagating the error; this keeps the command non-fatal.

Wire into `dispatch_stdio_command`.

Remove the `handle_not_yet_implemented` arm for `SwitchModel` in
`resolve_special_command_response`.

#### Task 3.6 Testing Requirements

Add tests in `src/acp/stdio.rs`:

- `test_dispatch_switch_mode_planning` -- dispatch `/mode planning`, assert
  `session.current_mode_id` becomes `"planning"` and response contains
  `"Mode switched"`.
- `test_dispatch_switch_mode_write` -- dispatch `/mode write`, assert
  `session.current_mode_id` becomes `"write"`.
- `test_dispatch_switch_safety_on` -- dispatch `/safety on`, assert
  `session.runtime_state.allow_dangerous` is `false` and response contains
  `"Safety policy"`.
- `test_dispatch_switch_safety_off` -- dispatch `/yolo`, assert
  `session.runtime_state.allow_dangerous` is `true`.
- `test_dispatch_toggle_subagents_on` -- dispatch `/subagents on`, assert
  `session.runtime_state.subagents_enabled` is `true`.
- `test_dispatch_toggle_subagents_off` -- dispatch `/subagents off`, assert
  `session.runtime_state.subagents_enabled` is `false`.
- `test_dispatch_set_system_prompt` -- dispatch `/system You are helpful.`,
  assert the first system message equals the new text.
- `test_dispatch_switch_model_unknown_returns_error_string` -- dispatch
  `/model nonexistent-model`, assert a `Some(Ok(...))` response with error
  description is returned (not `None`, not a hard error).

#### Task 3.7 Deliverables

- [ ] `src/acp/stdio.rs` -- `handle_switch_mode`, `handle_switch_safety`,
      `handle_toggle_subagents`, `handle_set_system_prompt`,
      `handle_switch_model` functions
- [ ] `src/acp/stdio.rs` -- 5 `handle_not_yet_implemented` stubs removed from
      `resolve_special_command_response` and wired into `dispatch_stdio_command`
- [ ] All new tests pass; `cargo test` is clean

#### Task 3.8 Success Criteria

- `/mode planning` and `/mode write` in Zed change the session mode and confirm
  via a chat response.
- `/safety on`, `/safety off`, `/safe`, `/yolo` in Zed change the safety policy
  and confirm.
- `/subagents on` and `/subagents off` in Zed toggle delegation and confirm.
- `/system <text>` in Zed replaces the active system prompt and confirms.
- `/model <name>` in Zed switches the model and confirms (or returns a friendly
  error if the model is unavailable).

---

### Phase 4: ACP Informational Command Implementation

Implement the remaining informational commands that are currently stubs.

#### Task 4.1 Implement `/models` Family

Add `async fn handle_models_list(session: &ActiveSessionState) -> String` in
`src/acp/stdio.rs`. This function must:

1. Call the provider's model listing API (equivalent to the `models list`
   subcommand in `src/commands/models.rs`).
2. Return a formatted newline-separated list of model names, or a graceful
   `"No models available."` message on error.

Add `fn handle_models_help() -> String` -- a pure function that returns
`format_models_help_text()` from `special_commands.rs` (already exists as
`print_models_help`, add a `format_models_help_text()` counterpart).

Add
`async fn handle_models_info(model: String, session: &ActiveSessionState) -> String`
-- queries model details and returns a formatted info block.

Wire all three into `dispatch_stdio_command`:

```rust
Ok(SpecialCommand::ModelsHelp)         => handle_models_help(),
Ok(SpecialCommand::ListModels)         => {
    let session_lock = session.lock().await;
    handle_models_list(&session_lock).await
}
Ok(SpecialCommand::ShowModelInfo(m))   => {
    let session_lock = session.lock().await;
    handle_models_info(m, &session_lock).await
}
```

Remove the three `handle_not_yet_implemented` stubs in
`resolve_special_command_response`.

#### Task 4.2 Implement `/context info` -- `ContextInfo` Handler

Add `async fn handle_context_info(session: &ActiveSessionState) -> String` in
`src/acp/stdio.rs`. This function must:

1. Lock the agent and read `agent.context_window_usage()` (or compute token
   counts from the conversation history if a dedicated method does not exist).
2. Return a formatted block showing current tokens used, context limit,
   remaining tokens, and usage percentage.

Wire into `dispatch_stdio_command`. Remove the stub in
`resolve_special_command_response`.

#### Task 4.3 Implement `/context summary` -- `ContextSummary` Handler

Add
`async fn handle_context_summary(model: Option<String>, session: &Arc<Mutex<ActiveSessionState>>) -> String`
in `src/acp/stdio.rs`. This function must:

1. Call the agent's summarize-and-reset method (which summarizes the
   conversation with the specified or default model and resets history to the
   summary).
2. Return a confirmation: `"Conversation summarized. Context window reset."`.

Wire into `dispatch_stdio_command`. Remove the stub in
`resolve_special_command_response`.

#### Task 4.4 Testing Requirements

Add tests in `src/acp/stdio.rs`:

- `test_dispatch_models_help_returns_models_help_text` -- dispatch `/models`,
  assert response contains the models help header.
- `test_dispatch_context_info_returns_token_stats` -- dispatch `/context info`,
  assert response contains `"tokens"`.
- `test_dispatch_context_summary_returns_confirmation` -- dispatch
  `/context summary`, assert response contains `"summarized"`.

#### Task 4.5 Deliverables

- [ ] `src/acp/stdio.rs` -- `handle_models_list`, `handle_models_help`,
      `handle_models_info`, `handle_context_info`, `handle_context_summary`
- [ ] `src/commands/special_commands.rs` -- `format_models_help_text()` function
      (extracted from `print_models_help`)
- [ ] All `handle_not_yet_implemented` stubs removed from
      `resolve_special_command_response` (only the permanent ACP-specific
      messages for `/auth`, `/streaming`, and `/exit` should remain)
- [ ] All new tests pass; `cargo test` is clean

#### Task 4.6 Success Criteria

- `/models list` in Zed returns a list of models from the current provider.
- `/context info` in Zed returns token usage statistics.
- `/context summary` compacts the conversation and confirms.
- `resolve_special_command_response` contains no `handle_not_yet_implemented`
  calls except for the three permanent ACP-mode-specific messages.

---

### Phase 5: Zed ACP Advertisement Updates

Update `src/acp/available_commands.rs` so that the Zed command completion menu
accurately reflects the unified UX introduced in Phases 1-4.

#### Task 5.1 Update Command Descriptions

Update the `description` field of each command builder:

| Command      | New Description                                                                                                            |
| ------------ | -------------------------------------------------------------------------------------------------------------------------- |
| `/mode`      | Show help or switch the operation mode. Use `/mode status` to see the current mode. Pass `planning` or `write` to switch.  |
| `/model`     | Show help or switch the active model. Use `/model status` to see the current model. Pass a model name to switch.           |
| `/safety`    | Show help or change the safety policy. Use `/safety status` to see the current policy. Pass `on` or `off` to change.       |
| `/subagents` | Show help or toggle subagent delegation. Use `/subagents status` to see current state. Pass `on` or `off` to change.       |
| `/system`    | Show help, inspect, or replace the system prompt. Use `/system status` to see the current prompt. Pass text to replace it. |
| `/streaming` | Show streaming help. Streaming is controlled by the Zed client in ACP mode. Use `/streaming status` for details.           |

Commands that are already correctly described (`/tools`, `/skills`, `/mcp`,
`/context`, `/summarize`, `/help`, `/status`) need no description change.

#### Task 5.2 Update Input Hints

For `/mode`, `/safety`, `/model`, `/subagents`, and `/system`, update the
`AvailableCommandInput::Unstructured` hint to mention `status` as a recognized
keyword:

```text
"/mode": "Optional: planning | write | status"
"/model": "Optional: <model_name> | status"
"/safety": "Optional: on | off | status"
"/subagents": "Optional: on | off | status"
"/system": "Optional: <new prompt text> | status"
```

`/system` input changes from `"Required text: the new system prompt"` to the
updated optional hint, reflecting that bare `/system` now shows help.

#### Task 5.3 Add `/streaming` to Advertised Commands

Add a `build_streaming_command()` builder and include it in
`build_available_commands()`. `/streaming` is an ACP-mode no-op but it should
appear in the completion menu so users can type `/streaming status` to learn
why. The command has no input (bare invocation shows help; `status` is embedded
in the help text).

Update the count assertion in `build_available_commands()` doc-comment from `12`
to `13`, and update all test assertions that check the count.

#### Task 5.4 Update the Module Doc Comment

Update the table at the top of `available_commands.rs` to add the `/streaming`
row and to update the `Input` column for the five commands that now accept
`status`.

#### Task 5.5 Testing Requirements

Update existing tests in `src/acp/available_commands.rs`:

- `test_build_available_commands_returns_twelve_entries` -> rename to
  `test_build_available_commands_returns_thirteen_entries` and update the count.
- `test_build_available_commands_names_are_correct` -> add `"/streaming"` to the
  expected list.
- `test_no_arg_commands_have_no_input` -> move `/streaming` into the no-input
  group.
- Add `test_streaming_command_is_present` -- verify `/streaming` is in the
  command list.
- Add `test_system_command_input_hint_mentions_status` -- verify the `/system`
  hint includes `"status"`.
- Add `test_mode_command_input_hint_mentions_status`.
- Add `test_model_command_input_hint_mentions_status`.

#### Task 5.6 Deliverables

- [ ] `src/acp/available_commands.rs` -- 6 updated command descriptions
- [ ] `src/acp/available_commands.rs` -- 5 updated input hints
- [ ] `src/acp/available_commands.rs` -- new `build_streaming_command()` and
      `/streaming` included in `build_available_commands()`
- [ ] All existing tests updated and new tests pass

#### Task 5.7 Success Criteria

- Zed's `/` completion menu shows 13 commands, including `/streaming`.
- The hint for `/system` no longer says "Required"; it says `"Optional: ..."`.
- The hints for `/mode`, `/model`, `/safety`, `/subagents` all mention `status`.

---

### Phase 6: Documentation and Demos

Create a full documentation suite covering the unified command UX, and update
all existing documents that reference command syntax.

#### Task 6.1 Create `docs/reference/chat_commands.md`

A comprehensive reference for all 13 slash commands following the unified UX.
Structure:

- **Overview table**: command name, one-line description, bare behavior,
  `status` behavior, example actions
- **Per-command sections** for each of the 13 commands, each with:
  - Purpose
  - Usage: `/<command>`, `/<command> status`, `/<command> <action>`
  - All accepted arguments and aliases
  - ACP-mode notes (if behavior differs from terminal chat mode)
- **General contract** section stating the bare-equals-help and status rules

#### Task 6.2 Create `docs/tutorials/chat_commands.md`

A step-by-step tutorial that walks a new user through:

1. Starting a chat session
2. Discovering available commands with `/help`
3. Checking current settings with the `status` subcommand for each command
4. Changing mode, safety, model, system prompt, and subagent delegation
5. Using `/context info` and `/context summary` to manage long sessions
6. Using `/streaming status` in Zed (ACP mode) and understanding why it is
   read-only
7. Exiting the session

All examples must use `granite4:3b` (Ollama) as the model.

#### Task 6.3 Create `docs/how-to/use_chat_commands.md`

A task-oriented guide covering:

- How to check the current mode (`/mode status`)
- How to switch modes mid-session (`/mode planning`, `/mode write`)
- How to inspect and change the safety policy
- How to replace the system prompt mid-session and verify it took effect
- How to enable and disable subagents per-session
- How to inspect the active model and switch it without restarting
- How to manage context window pressure with `/context summary`

#### Task 6.4 Update Existing Documentation

The following existing documents reference command syntax and must be updated to
reflect the new UX contract:

| File                                          | Required Changes                                                                                        |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `docs/reference/quick_reference.md`           | Update command table entries; add `status` column                                                       |
| `docs/how-to/use_chat_modes.md`               | Replace error-message examples for bare `/mode` with help-text examples                                 |
| `docs/how-to/configure_system_prompt.md`      | Add `/system status` as the way to inspect the active prompt; update "empty /system returns error" note |
| `docs/explanation/chat_modes_architecture.md` | Update the `SpecialCommand` enum code block to include the new variants                                 |
| `docs/reference/system_prompt.md`             | Add `/system status` to the interactive command section                                                 |
| `docs/reference/acp_configuration.md`         | Add a note that ACP slash commands now support `status`                                                 |
| `docs/explanation/implementations.md`         | Add an index entry for this plan                                                                        |

#### Task 6.5 Create `demos/chat_commands/` Demo

Create a self-contained demo for the unified command UX following the existing
demo conventions from `demos/chat/` and `demos/mcp/`.

**Directory structure:**

```text
demos/chat_commands/
├── README.md
├── config.yaml
├── setup.sh
├── run.sh
├── reset.sh
├── input/
│   └── commands_demo_script.txt
└── tmp/
    ├── .gitignore
    └── output/
        └── .gitkeep
```

**`config.yaml`** -- Ollama provider, `granite4:3b` model, sandboxed to
`demos/chat_commands/tmp/`.

**`commands_demo_script.txt`** -- a sequence of chat inputs that exercises:

1. `/help` -- show the global help
2. `/mode status` -- see current mode
3. `/mode planning` -- switch mode
4. `/mode status` -- confirm mode changed
5. `/safety status` -- see current safety
6. `/safety off` -- disable safety
7. `/safety status` -- confirm
8. `/model status` -- see current model
9. `/streaming` -- see streaming help (ACP) or streaming status (terminal)
10. `/system status` -- show current system prompt
11. `/system You are a concise assistant. Reply in one sentence.` -- update
    prompt
12. `/system status` -- verify new prompt
13. `/subagents status` -- see subagent state
14. `/context info` -- see token usage
15. `exit` -- exit

**`README.md`** -- walks the user through prerequisites (Ollama running with
`granite4:3b` pulled), setup, running, and expected output.

**`run.sh`** -- pipes `commands_demo_script.txt` to `xzatoma chat` using the
demo config; captures output to `tmp/output/commands_demo.txt`.

**`setup.sh`** -- creates `tmp/output/`, writes the demo config.

**`reset.sh`** -- removes `tmp/output/`.

#### Task 6.6 Full Quality Gate

Run the mandatory quality checks from `AGENTS.md` in order:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Run markdownlint and prettier on all new and modified Markdown files:

```bash
markdownlint --fix --config .markdownlint.json docs/reference/chat_commands.md
prettier --write --parser markdown --prose-wrap always docs/reference/chat_commands.md
# Repeat for each new and updated .md file
```

#### Task 6.7 Deliverables

- [ ] `docs/reference/chat_commands.md` -- new unified command reference
- [ ] `docs/tutorials/chat_commands.md` -- new step-by-step tutorial
- [ ] `docs/how-to/use_chat_commands.md` -- new task-oriented guide
- [ ] 7 existing documentation files updated
- [ ] `demos/chat_commands/` demo directory with `README.md`, `config.yaml`,
      `setup.sh`, `run.sh`, `reset.sh`, input script, and `tmp/.gitignore`
- [ ] All quality gates pass
- [ ] `docs/explanation/implementations.md` updated with index entry

#### Task 6.8 Success Criteria

- All new documentation files pass markdownlint and prettier.
- `demos/chat_commands/run.sh` completes without error with Ollama running and
  `granite4:3b` available.
- The unified command UX is described consistently across reference, tutorial,
  how-to, and demo materials.
- No existing documentation still describes bare `/mode` as returning an error.

---

## File Change Summary

| File                                          | Change                                                                 |
| --------------------------------------------- | ---------------------------------------------------------------------- |
| `src/commands/special_commands.rs`            | 12 new variants, 6 new formatters, updated parser and global help      |
| `src/acp/stdio.rs`                            | 12 new handler functions, updated dispatch and resolver, removed stubs |
| `src/acp/available_commands.rs`               | Updated descriptions and hints, new `/streaming` entry                 |
| `docs/reference/chat_commands.md`             | New file                                                               |
| `docs/tutorials/chat_commands.md`             | New file                                                               |
| `docs/how-to/use_chat_commands.md`            | New file                                                               |
| `docs/reference/quick_reference.md`           | Updated command table                                                  |
| `docs/how-to/use_chat_modes.md`               | Updated bare-command examples                                          |
| `docs/how-to/configure_system_prompt.md`      | Added `/system status`                                                 |
| `docs/explanation/chat_modes_architecture.md` | Updated SpecialCommand block                                           |
| `docs/reference/system_prompt.md`             | Added `/system status` section                                         |
| `docs/reference/acp_configuration.md`         | Added unified UX note                                                  |
| `docs/explanation/implementations.md`         | New index entry                                                        |
| `demos/chat_commands/`                        | New demo directory (7 files)                                           |

---

## Key Design Decisions

### Why `ShowXxxHelp` and `ShowXxxStatus` variants instead of a generic `CommandHelp(String)` and `CommandStatus(String)` pair?

Typed variants preserve exhaustive matching. If a new command is added later,
the compiler will flag any `dispatch_stdio_command` or
`resolve_special_command_response` that does not handle the new variant. A
generic string-keyed approach would silently fall through at runtime.

### Why are help variants pure (handled in `resolve_special_command_response`) while status variants are async (handled in `dispatch_stdio_command`)?

Help text is static and requires no session state. Separating it into the pure
resolver keeps `dispatch_stdio_command` focused on handlers that genuinely need
session access. This mirrors the existing split between `Help`/`Mentions` (pure)
and `ShowStatus`/`ListTools`/`ListSkills`/`ShowMcpStatus` (session-aware).

### Why does `/streaming status` return an ACP-mode note rather than a live flag value?

In ACP mode, the client (Zed) controls streaming; the agent has no streaming
toggle. Reporting a flag value that the agent cannot control would be
misleading. The note explains the behavior honestly and helps users who expect a
toggle that does not exist in this context.

### Why is `/system` input changed from Required to Optional in `available_commands.rs`?

After Phase 1, bare `/system` returns help rather than an error. Advertising it
as `Required` would cause Zed to display a mandatory input prompt, which
contradicts the new behavior. `Optional` with a hint that mentions `status` is
accurate.

### Why add `/streaming` to `build_available_commands()` if it is a no-op in ACP mode?

Discoverability. Users who type `/streaming` in Zed expect to see it in the
completion menu. Without it, the command appears to be unknown. The help text
and `status` output explain why the toggle has no effect, turning a confusing
silence into useful documentation.

### Why are the three ACP-permanent messages (`/auth`, `/streaming on|off`, `/exit`) retained unchanged?

These commands have genuine behavioral constraints in ACP mode that cannot be
lifted by implementation work alone. `/auth` is a CLI-only flow;
`/streaming on|off` is controlled by Zed; `/exit` requires Zed UI interaction.
The fixed messages are correct and do not violate the unified UX contract
because they explain the constraint rather than silently failing.
