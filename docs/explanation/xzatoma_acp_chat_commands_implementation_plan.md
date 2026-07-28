# XZatoma ACP Chat Commands Implementation Plan

## Overview

XZatoma's stdio ACP agent (`xzatoma agent`, implemented in
[`src/acp/stdio.rs`](../../src/acp/stdio.rs)) already handles Zed's native UI
widgets correctly: the mode selector dropdown calls `SetSessionModeRequest`, the
thinking-effort and safety-policy dropdowns call
`SetSessionConfigOptionRequest`, and the model dropdown calls
`SetSessionModelRequest`. Those three handlers are fully implemented and tested.

Three gaps remain. First, `enqueue_prompt` never calls `parse_special_command`
before queuing a message, so every `/mode planning`, `/safety yolo`, `/tools`,
and similar slash command is forwarded to the LLM as plain text instead of
being handled locally. Second, `set_session_model` records the new model name
in `ActiveSessionState.current_model_name` but never calls
`provider.set_model()`, so the model dropdown selection updates the display but
has no effect on inference — and `provider.set_model()` cannot currently be
called through the `Arc<dyn Provider>` that `XzatomaAgent` shares with
`SubagentTool`, because the trait method requires `&mut self`. Third,
`src/commands/special_commands.rs` has no `SpecialCommand` variants for
`/tools`, `/skills`, or `/mcp`, so those three commands — despite already being
advertised by `build_available_commands()` — cannot be routed by a dispatcher
built purely on top of `parse_special_command`.

This plan fixes all three gaps across seven phases, working entirely within the
stdio path. The HTTP server (`src/acp/server.rs`) and `AcpRuntime`
(`src/acp/runtime.rs`) are not touched, except that Phase 4's `Provider` trait
signature change is a shared-crate change and therefore affects every provider
implementation and every caller of `Provider::set_model`, listed explicitly in
Phase 4.

## Current State Analysis

### Existing Infrastructure

- [`src/acp/stdio.rs`](../../src/acp/stdio.rs) - `AcpStdioServerState` owns
  `ActiveSessionRegistry`; each `ActiveSessionState` carries
  `runtime_state: SessionRuntimeState`, `current_mode_id: String`,
  `current_model_name: String`, `xzatoma_agent: Arc<Mutex<XzatomaAgent>>`, and
  `mcp_manager`.
- `run_stdio_agent_with_transport` in the same file routes every Zed JSON-RPC
  message to a handler: `NewSessionRequest`, `PromptRequest`,
  `SetSessionModeRequest`, `SetSessionConfigOptionRequest`,
  `SetSessionModelRequest`, and `CancelNotification`.
- `set_session_mode` (`stdio.rs:751-810`) is fully implemented: applies
  `mode_runtime_effect`, rebuilds transient system messages, replaces the
  terminal tool, updates `session.runtime_state`, and pushes
  `CurrentModeUpdate` + `ConfigOptionUpdate` to the client.
- `set_session_config_option` (`stdio.rs:824+`) is fully implemented: calls
  `apply_config_option_change`, applies every `ConfigChangeEffect` field, and
  for `CONFIG_SESSION_MODE` changes also rebuilds the terminal tool and system
  prompt identically to `set_session_mode`.
- `set_session_model` (`stdio.rs:972-994`) only sets
  `session_lock.current_model_name = model_id.clone()` and logs. It never
  calls `provider.set_model()`.
- `AvailableCommandsUpdate` containing `build_available_commands()` is pushed to
  the client immediately after every successful `NewSessionResponse` (line
  1186-1194 of `stdio.rs`).
- [`src/commands/special_commands.rs`](../../src/commands/special_commands.rs)
  - `pub fn parse_special_command(input: &str) -> Result<SpecialCommand, CommandError>`
    (`special_commands.rs:193`) is fully implemented for the seventeen
    variants that currently exist on `SpecialCommand`: `SwitchMode(ChatMode)`,
    `SwitchSafety(SafetyMode)`, `ShowStatus`, `Help`, `Mentions`,
    `Auth(Option<String>)`, `ListModels`, `ModelsHelp`, `ShowModelInfo(String)`,
    `SwitchModel(String)`, `ContextInfo`, `ContextSummary { model: Option<String> }`,
    `ToggleSubagents(bool)`, `SetSystemPrompt(String)`, `ToggleStreaming(bool)`,
    `Exit`, and `None` (`special_commands.rs:37-137`).
  - There are **no variants for `/tools`, `/skills`, or `/mcp`**. Those three
    inputs fall through to the generic
    `input if input.starts_with('/') => Err(CommandError::UnknownCommand(...))`
    arm (`special_commands.rs:410-414`). Phase 1 adds the missing variants.
  - Bare `/mode` and bare `/model` (no argument) both return
    `Err(CommandError::MissingArgument { command, usage })`
    (`special_commands.rs:208-211`, `345-348`) — **not** `Ok(SpecialCommand::ShowStatus)`
    or `Ok(SpecialCommand::ModelsHelp)`. `ModelsHelp` is produced only by the
    plural command `"/models"` (`special_commands.rs:248`), a separate,
    currently-unadvertised command family from singular `/model`.
  - `print_help()` (`special_commands.rs:433`), `print_models_help()`
    (`special_commands.rs:523`), and `print_mention_help()`
    (`special_commands.rs:569`) all return `()` and write directly to stdout
    via `println!`. In the stdio ACP agent, stdout is the JSON-RPC wire
    channel; calling any of these three functions from the dispatcher would
    corrupt the protocol stream. Phase 2 adds String-returning equivalents.
- [`src/acp/available_commands.rs`](../../src/acp/available_commands.rs) -
  `build_available_commands()` advertises `/mode`, `/model`, `/safety`,
  `/tools`, `/context`, `/summarize`, `/skills`, `/mcp` (8 entries, asserted by
  `test_build_available_commands_returns_eight_entries`,
  `available_commands.rs:194-197`). It is missing `/help`, `/status`,
  `/subagents`, and `/system`.
- `XzatomaAgent::get_context_info(model_context_window: usize)`
  (`src/agent/core.rs:1729`) returns a `ContextInfo` struct
  (`src/agent/conversation.rs:16-23`) with a `used_tokens` field.
  `agent.conversation_mut().replace_first_system_message(text)`
  (`src/agent/conversation.rs:345`) exists for system-prompt mutation.
  `replace_conversation_with_summary` does not exist anywhere and must be
  added in Phase 5.
- `Provider::set_model(&mut self, model: &str)`
  (`src/providers/trait_mod.rs:81`) requires exclusive access. `XzatomaAgent`
  stores `provider: Arc<dyn Provider>` (`src/agent/core.rs:54`), and
  `SubagentTool` holds its own `Arc::clone(&provider)`
  (`stdio.rs:558`) for the lifetime of its registration. `Arc::get_mut` is
  therefore unreliable, and `agent.provider()` (`core.rs:1621`) returns only
  `&dyn Provider`. There is no way to call `set_model` through the shared
  handle today. However, all three concrete providers — `CopilotProvider`
  (`src/providers/copilot.rs:2967`), `OllamaProvider`
  (`src/providers/ollama.rs:1107`), `OpenAIProvider`
  (`src/providers/openai.rs:1256`) — already implement `set_model` by writing
  into an `Arc<RwLock<Config>>` field (`self.config.write()`), the same
  interior-mutability pattern already used by
  `set_thinking_effort(&self, ...)` (`copilot.rs:3019`). The `&mut self`
  requirement on the trait method is stricter than any implementation needs.

### Identified Issues

| Item                                                        | Root Cause                                                                                                                                                                          |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| All `/` commands forwarded to LLM as plain text               | `enqueue_prompt` in `src/acp/stdio.rs` never calls `parse_special_command`; there is no dispatch step before `prompt_queue.try_send`                                                |
| `/tools`, `/skills`, `/mcp` are unparseable                   | `SpecialCommand` has no variants for them; `parse_special_command` returns `Err(UnknownCommand)` even though `build_available_commands()` already advertises all three             |
| Model dropdown selection has no effect on inference            | `set_session_model` updates `session.current_model_name` metadata but never calls `provider.set_model()`; the live provider retains its original model                             |
| `provider.set_model()` cannot be called through the shared handle | `Provider::set_model` requires `&mut self`, but `XzatomaAgent.provider` is an `Arc<dyn Provider>` also cloned into `SubagentTool`; no call site can obtain exclusive access          |
| `/mode`, `/safety` typed text has no effect                   | Even though `set_session_mode` / `set_session_config_option` work when called by the native UI, they are never called from the slash command path                                  |
| Bare `/mode` and bare `/model` do not resolve to a status view | `parse_special_command` returns `Err(MissingArgument)` for both, not a variant a dispatcher can route to a "show current value" handler                                             |
| `Help`/`Mentions` text cannot be routed through stdout safely  | `print_help()` and `print_mention_help()` write directly to stdout via `println!`, which is the JSON-RPC wire channel in stdio mode                                                 |
| `Auth`, `ToggleStreaming`, `Exit` have no defined ACP behavior | These are real, parseable `SpecialCommand` variants with no native ACP equivalent; an exhaustive `match` in the dispatcher must still handle them                                    |
| `/subagents on\|off` has no effect in ACP mode                | No code path in the stdio agent re-registers or de-registers `SubagentTool` in response to `runtime_state.subagents_enabled` changes                                                |
| `/system <text>` has no effect in ACP mode                    | No code path calls `agent.conversation_mut().replace_first_system_message(text)` from the stdio prompt path                                                                          |
| `build_available_commands()` is incomplete                    | `/help`, `/status`, `/subagents`, and `/system` are parseable but not advertised, so Zed's `/` autocomplete omits them                                                                |

> **Note:** The Zed mode selector UI bug (mode selector not rendering) was
> addressed in the preceding `acp_features_implementation.md` plan. The mode
> dropdown, thinking-effort dropdown, and safety-policy dropdown all work
> correctly when operated through the Zed native UI. The issues above affect
> only the typed slash command path.

## Implementation Phases

### Phase 1: Slash Command Interception Foundation

Add the command dispatch step inside `enqueue_prompt` in
[`src/acp/stdio.rs`](../../src/acp/stdio.rs), and extend `SpecialCommand` so
every currently-advertised command is parseable. After this phase, every
advertised slash command either returns a formatted text response in Zed's
chat window or a defined "not supported over ACP" message — none of the
seventeen existing variants plus the three added here are forwarded to the LLM,
and the dispatcher's `match` is exhaustive from this phase onward (later
phases replace individual placeholder arms with real handlers; they never add
new arms).

#### Task 1.1 Extract Prompt Text Before Queuing

In `enqueue_prompt`, after the call to `acp_content_blocks_to_prompt_input` and
before `validate_provider_supports_prompt_input`, extract the plain-text portion
of `prompt_input` (`MultimodalPromptInput`, `src/providers/types.rs:90`) using
its existing `as_legacy_text()` method (`src/providers/types.rs:185`). Do not
use `flatten_input_to_prompt` (`src/acp/runtime.rs:1765`) — that function
operates on `&[AcpMessage]`, the `AcpRuntime`/HTTP-server message type, not
`MultimodalPromptInput`, and is not callable here without a conversion step
that does not exist.

Store the extracted text as `let prompt_text: Option<String>` — `None` when the
input contains only images and no text to parse.

#### Task 1.2 Add `/tools`, `/skills`, `/mcp` to `SpecialCommand`

In [`src/commands/special_commands.rs`](../../src/commands/special_commands.rs):

1. Add three variants to the `SpecialCommand` enum (near `ShowStatus`):
   - `ListTools` — no argument. Doc comment: "Display the list of tools
     available to the agent in this session."
   - `ListSkills` — no argument. Doc comment: "Display the list of active
     skills loaded for this workspace."
   - `ShowMcpStatus` — no argument. Doc comment: "Display connected MCP
     servers and the tools they expose."
2. Add three match arms to `parse_special_command`, alongside the existing
   `"/status" => Ok(SpecialCommand::ShowStatus)` arm:
   ```rust
   "/tools" => Ok(SpecialCommand::ListTools),
   "/skills" => Ok(SpecialCommand::ListSkills),
   "/mcp" => Ok(SpecialCommand::ShowMcpStatus),
   ```
3. These three commands take no arguments; do not add `input.starts_with(...)`
   variants for them in this task.

#### Task 1.3 Add `dispatch_stdio_command` Function

Add a private async function in `src/acp/stdio.rs`:

```rust
async fn dispatch_stdio_command(
    prompt_text: &str,
    session: &Arc<Mutex<ActiveSessionState>>,
    connection: Option<&ConnectionTo<AcpClientRole>>,
) -> Option<acp_sdk::Result<acp::PromptResponse>>
```

- Returns `None` when `parse_special_command` returns `Ok(SpecialCommand::None)`.
  The caller proceeds to `prompt_queue.try_send` as today.
- Returns `Some(Ok(response))` for every other outcome: a matched
  `SpecialCommand` variant, a `CommandError`, or the two special-cased
  `MissingArgument` errors described below. The implementation sends an
  `AgentMessageChunk` notification via the connection before returning so Zed
  displays the response text in the chat panel. The LLM is not invoked.
- The `PromptResponse` for a handled command uses `acp::StopReason::EndTurn`
  and zero-value `acp::Usage` (token counts are unavailable without an LLM
  turn).

Match on `parse_special_command(prompt_text)` per this routing table. "Phase 1
behavior" is the literal text this task must implement now, using a shared
helper `fn handle_not_yet_implemented(command_name: &str) -> String` that
returns `format!("{command_name} is not yet implemented in this session.")`
for every row marked "placeholder." Rows marked "final" are permanent — no
later phase touches them.

| `SpecialCommand` variant           | Phase 1 behavior                                          | Real handler       |
| ----------------------------------- | ----------------------------------------------------------- | ------------------- |
| `SwitchMode(ChatMode)`               | placeholder                                                  | Phase 3, Task 3.1   |
| `SwitchSafety(SafetyMode)`           | placeholder                                                  | Phase 3, Task 3.2   |
| `ShowStatus`                         | placeholder                                                  | Phase 2, Task 2.4   |
| `Help`                               | placeholder                                                  | Phase 2, Task 2.5   |
| `Mentions`                           | placeholder                                                  | Phase 2, Task 2.5   |
| `Auth(Option<String>)`               | **final**: `"/auth is not supported in ACP mode. Authentication is managed by your provider configuration outside the chat session."` | Phase 1 (final)     |
| `ListModels`                         | placeholder                                                  | Phase 4, Task 4.5   |
| `ModelsHelp`                         | placeholder                                                  | Phase 4, Task 4.5   |
| `ShowModelInfo(String)`              | placeholder                                                  | Phase 4, Task 4.5   |
| `SwitchModel(String)`                | placeholder                                                  | Phase 4, Task 4.4   |
| `ContextInfo`                        | placeholder                                                  | Phase 5, Task 5.1   |
| `ContextSummary { model }`           | placeholder                                                  | Phase 5, Task 5.2   |
| `ToggleSubagents(bool)`              | placeholder                                                  | Phase 6, Task 6.1   |
| `SetSystemPrompt(String)`            | placeholder                                                  | Phase 6, Task 6.2   |
| `ToggleStreaming(bool)`              | **final**: `"/streaming has no effect over ACP; Zed's client controls response streaming."` | Phase 1 (final)     |
| `Exit`                               | **final**: `"Use Zed's UI to close this session; /exit has no effect over ACP."` | Phase 1 (final)     |
| `ListTools`                          | placeholder                                                  | Phase 2, Task 2.1   |
| `ListSkills`                         | placeholder                                                  | Phase 2, Task 2.2   |
| `ShowMcpStatus`                      | placeholder                                                  | Phase 2, Task 2.3   |

In addition to the `Ok(variant)` arms above, add two special-cased `Err` arms
**before** the generic `Err(e)` fallback, because bare `/mode` and bare
`/model` return `CommandError::MissingArgument`, not a distinguishable `Ok`
variant:

```rust
Err(CommandError::MissingArgument { ref command, .. }) if command == "/mode" => {
    /* Phase 1: placeholder; Phase 3 Task 3.1 replaces with current-mode display */
}
Err(CommandError::MissingArgument { ref command, .. }) if command == "/model" => {
    /* Phase 1: placeholder; Phase 4 Task 4.3 replaces with current-model display */
}
Err(e) => { /* generic error text via e.to_string() */ }
```

> **Behavior change:** because `parse_special_command` treats a chat message
> consisting of exactly `"exit"` or `"quit"` (no leading slash,
> `special_commands.rs:198`) as `SpecialCommand::Exit`, such a message will now
> be intercepted by the dispatcher instead of reaching the LLM. This is
> intentional — see Key Design Decisions.

#### Task 1.4 Wire Dispatcher into `enqueue_prompt`

In `enqueue_prompt`, after extracting `prompt_text` (Task 1.1):

```rust
if let Some(prompt_text) = prompt_text.as_deref() {
    if let Some(result) = dispatch_stdio_command(
        prompt_text, &session, connection.as_ref()
    ).await {
        return result;
    }
}
```

Place this block before the `validate_provider_supports_prompt_input` call.
Multimodal inputs (no text or image-only) skip the dispatch entirely and proceed
to the prompt worker as before.

#### Task 1.5 Add Missing Commands to `build_available_commands()`

In [`src/acp/available_commands.rs`](../../src/acp/available_commands.rs), add
four missing command builders and include them in the returned `Vec`:

- `/help` - no input argument
- `/status` - no input argument
- `/subagents` - optional `on | off | enable | disable` argument
- `/system` - required text argument

`/tools`, `/skills`, and `/mcp` are already advertised (Task 1.2 only fixes
their parseability, not their advertising). Update the
`assert_eq!(commands.len(), 8)` assertion in the test module to
`assert_eq!(commands.len(), 12)`.

#### Task 1.6 Testing Requirements

- `test_parse_special_command_tools_returns_list_tools` - `/tools` parses to
  `Ok(SpecialCommand::ListTools)`.
- `test_parse_special_command_skills_returns_list_skills` - `/skills` parses to
  `Ok(SpecialCommand::ListSkills)`.
- `test_parse_special_command_mcp_returns_show_mcp_status` - `/mcp` parses to
  `Ok(SpecialCommand::ShowMcpStatus)`.
- `test_dispatch_stdio_command_returns_none_for_plain_text` - `"hello agent"`
  returns `None`.
- `test_dispatch_stdio_command_returns_some_for_help` - `/help` returns
  `Some(Ok(...))` with a non-empty `AgentMessageChunk` sent to the connection
  (placeholder text in this phase; Phase 2 strengthens this assertion).
- `test_dispatch_stdio_command_returns_some_for_invalid_command` -
  `/notacommand` returns `Some(Ok(...))` with an error description, not a Rust
  error propagation.
- `test_dispatch_auth_returns_not_supported_message` - `/auth` returns
  `Some(Ok(...))` containing "not supported in ACP mode".
- `test_dispatch_streaming_returns_not_supported_message` - `/streaming on`
  returns `Some(Ok(...))` containing "has no effect over ACP".
- `test_dispatch_exit_returns_not_supported_message` - `/exit` and bare `exit`
  both return `Some(Ok(...))` containing "has no effect over ACP", and neither
  reaches the mock provider.
- `test_dispatch_bare_mode_returns_placeholder` - `/mode` (no argument)
  returns `Some(Ok(...))`, not a Rust `Err`.
- `test_dispatch_bare_model_returns_placeholder` - `/model` (no argument)
  returns `Some(Ok(...))`, not a Rust `Err`.
- `test_enqueue_prompt_short_circuits_on_help_command` - a mock session
  receiving `/help` returns a `PromptResponse` without the prompt reaching the
  mock provider.
- `test_build_available_commands_returns_twelve_entries` - the updated list has
  twelve entries.

#### Task 1.7 Deliverables

- [ ] `prompt_text` extracted from `prompt_input` via `as_legacy_text()` before
      `prompt_queue.try_send`
- [ ] `SpecialCommand::ListTools`, `::ListSkills`, `::ShowMcpStatus` added with
      parser arms
- [ ] `dispatch_stdio_command` function added to `src/acp/stdio.rs` with an
      exhaustive match covering all twenty `Ok` variants plus the two
      special-cased `MissingArgument` errors plus the generic `Err` fallback
- [ ] `enqueue_prompt` wired to call `dispatch_stdio_command`
- [ ] `build_available_commands()` updated to include twelve commands
- [ ] All Task 1.6 tests passing

#### Task 1.8 Success Criteria

- `cargo test --all-features` passes with no regressions.
- Typing `/help` in Zed's chat panel returns a response without triggering an
  LLM request (placeholder text in this phase).
- Typing `hello agent` continues to route to the LLM unchanged.
- Typing `/auth`, `/streaming on`, or `/exit` (or a bare `exit`/`quit` message)
  returns a fixed, defined message instead of an unhandled match error or an
  LLM turn.
- Zed's `/` autocomplete shows twelve commands.

---

### Phase 2: Informational Commands (`/tools`, `/skills`, `/mcp`, `/status`, `/help`, `/mentions`)

Implement the six stateless informational handlers, replacing their Phase 1
placeholder arms. Each handler reads from the locked session state and returns
a formatted text string without mutating any state or calling the provider.

#### Task 2.1 Implement `/tools` Handler

Add `fn handle_tools_command(session: &ActiveSessionState) -> String`:

1. Acquire a read on `session.xzatoma_agent`.
2. Iterate over `agent.tools().tool_names()` (or equivalent iterator).
3. Return a formatted list of tool names, one per line, with a header.

Route `SpecialCommand::ListTools` in `dispatch_stdio_command` to this function,
replacing the Phase 1 placeholder arm.

#### Task 2.2 Implement `/skills` Handler

Add `fn handle_skills_command(session: &ActiveSessionState) -> String`:

1. Check `session.xzatoma_agent` for the active `skill_disclosure` field or
   introspect via the agent's skill list if available.
2. Return a formatted list of active skill names and source paths, or a
   `"No active skills for this workspace."` message if none are loaded.

Route `SpecialCommand::ListSkills` to this function, replacing the Phase 1
placeholder arm.

#### Task 2.3 Implement `/mcp` Handler

Add `fn handle_mcp_command(session: &ActiveSessionState) -> String`:

1. If `session.mcp_manager` is `None`, return `"No MCP servers configured."`.
2. Read `manager.connected_servers()`, listing server IDs, transport types, and
   the tools each server exposes using the `server__tool` naming convention.

Route `SpecialCommand::ShowMcpStatus` to this function, replacing the Phase 1
placeholder arm.

#### Task 2.4 Implement `/status` Handler

Add `fn handle_status_command(session: &ActiveSessionState) -> String`:

1. Read `session.current_mode_id`, `session.runtime_state.safety_mode_str`,
   `session.current_model_name`, and `session.runtime_state.subagents_enabled`.
2. Return a compact formatted status block matching the existing
   `ChatModeState::status()` format in
   [`src/chat_mode.rs:579-592`](../../src/chat_mode.rs).

Route `SpecialCommand::ShowStatus` to this function, replacing the Phase 1
placeholder arm.

#### Task 2.5 Add String-Returning Help Formatters and Implement `/help`, `/mentions`

`print_help()` and `print_mention_help()` cannot be called from the stdio
dispatcher because they write to stdout, corrupting the JSON-RPC channel
(see Current State Analysis). Instead:

1. In `src/commands/special_commands.rs`, add
   `pub fn format_help_text() -> String` that returns the same content
   currently passed to `println!` inside `print_help()`, as an owned `String`.
   Change `print_help()`'s body to `println!("{}", format_help_text());` so
   the terminal chat path is unaffected.
2. Add `pub fn format_mention_help_text() -> String` the same way, refactoring
   `print_mention_help()` to delegate to it.
3. In `dispatch_stdio_command`, route `SpecialCommand::Help` to
   `format_help_text()` and `SpecialCommand::Mentions` to
   `format_mention_help_text()`, replacing both Phase 1 placeholder arms.

`print_models_help()` is out of scope for this task; it is addressed in
Phase 4, Task 4.5.

#### Task 2.6 Testing Requirements

- `test_handle_tools_command_returns_non_empty_string` - result is non-empty.
- `test_handle_tools_command_includes_terminal` - `"terminal"` appears in the
  output for a default session.
- `test_handle_mcp_command_returns_no_servers_when_manager_is_none` - returns
  the graceful empty message.
- `test_handle_status_command_includes_mode_id` - result contains the session
  `current_mode_id`.
- `test_format_help_text_matches_print_help_content` - `format_help_text()`
  returns non-empty text containing the same section headers `print_help()`
  prints.
- `test_dispatch_help_returns_full_help_text` - `dispatch_stdio_command("/help")`
  response now contains actual help content, superseding the Phase 1
  placeholder-only assertion.
- `test_dispatch_routes_tools_to_handler` - `dispatch_stdio_command("/tools")`
  returns `Some(Ok(...))` containing the formatted tool list, not the Phase 1
  placeholder text.

#### Task 2.7 Deliverables

- [ ] `handle_tools_command` implemented and routed from
      `dispatch_stdio_command`
- [ ] `handle_skills_command` implemented and routed
- [ ] `handle_mcp_command` implemented and routed
- [ ] `handle_status_command` implemented and routed
- [ ] `format_help_text` and `format_mention_help_text` added;
      `print_help`/`print_mention_help` refactored to delegate to them
- [ ] `SpecialCommand::Help` and `::Mentions` routed to the new formatters
- [ ] All Task 2.6 tests passing

#### Task 2.8 Success Criteria

- `/tools` in Zed returns the agent's tool list with no LLM turn.
- `/status` returns current mode, safety policy, and model name.
- `/skills` and `/mcp` return graceful messages when nothing is configured.
- `/help` and `/mentions` return their full text content without writing
  anything to stdout outside the JSON-RPC `AgentMessageChunk` notification.

---

### Phase 3: Mode and Safety Text Commands (`/mode`, `/safety`)

Map the `SwitchMode` and `SwitchSafety` special command variants, and the bare
`/mode` error case, to the existing `set_session_mode` and
`set_session_config_option` implementations. After this phase, `/mode
full_autonomous` in the chat window has exactly the same runtime effect as
clicking "Full Autonomous" in Zed's mode dropdown.

#### Task 3.1 Implement `/mode` Command Handler

In `dispatch_stdio_command`, handle `SpecialCommand::SwitchMode(chat_mode)`,
replacing the Phase 1 placeholder arm:

1. Map `ChatMode::Planning → "planning"`, `ChatMode::Write → "write"` using the
   constants in [`src/acp/session_mode.rs:45,48`](../../src/acp/session_mode.rs).
   `ChatMode::Watcher` maps to `"write"` (closest equivalent — there is no
   `"watcher"` mode ID in `mode_runtime_effect`).
2. Build an `acp::SetSessionModeRequest` with the resolved mode ID and the
   session's `session_id`.
3. Call `state.set_session_mode(request).await`. On success, push
   `CurrentModeUpdate` and `ConfigOptionUpdate` to the connection (identical to
   the `SetSessionModeRequest` handler in `run_stdio_agent_with_transport`).
4. Return a confirmation: `"Mode changed to <name>."`.

Additionally, replace the Phase 1 placeholder for the special-cased
`Err(CommandError::MissingArgument { command, .. }) if command == "/mode"` arm
(Task 1.3): return the current `session.current_mode_id` and its description
(reuse `handle_status_command`'s mode-only fields, or a shorter dedicated
string). This is the correct home for "bare `/mode` shows current mode" —
`parse_special_command` never produces `Ok(SpecialCommand::ShowStatus)` for
bare `/mode` (see Current State Analysis); the routing happens entirely on the
`Err` branch.

#### Task 3.2 Implement `/safety` Command Handler

In `dispatch_stdio_command`, handle `SpecialCommand::SwitchSafety(safety_mode)`,
replacing the Phase 1 placeholder arm:

1. Map `SafetyMode::AlwaysConfirm → "always_confirm"`,
   `SafetyMode::NeverConfirm → "never_confirm"` (the `"confirm_dangerous"` value
   has no `SafetyMode` variant; `parse_special_command` cannot produce it —
   only the native `SetSessionConfigOptionRequest` path can select it).
2. Build an `acp::SetSessionConfigOptionRequest` with
   `config_id = CONFIG_SAFETY_POLICY` and the resolved `value_id`.
3. Call `state.set_session_config_option(request).await`. Push
   `ConfigOptionUpdate` to the connection.
4. Return a confirmation string.

#### Task 3.3 Testing Requirements

- `test_dispatch_mode_planning_calls_set_session_mode` - after dispatch,
  `session.current_mode_id == "planning"`.
- `test_dispatch_mode_full_autonomous_updates_terminal_mode` - after dispatch,
  `session.runtime_state.terminal_mode == ExecutionMode::FullAutonomous`.
- `test_dispatch_mode_full_autonomous_sends_current_mode_update` - connection
  received a `CurrentModeUpdate` notification.
- `test_dispatch_bare_mode_returns_current_mode_description` - `/mode` (no
  argument) returns text containing `session.current_mode_id`, superseding the
  Phase 1 placeholder-only assertion.
- `test_dispatch_safety_never_confirm_updates_runtime_state` - after dispatch,
  `session.runtime_state.safety_mode_str == "yolo"`.
- `test_dispatch_mode_invalid_arg_returns_error_message` - `/mode unicorn`
  returns `Some(Ok(...))` with a user-facing error string listing valid modes
  (routed through the generic `Err(CommandError::UnsupportedArgument)` arm from
  Task 1.3, not a new arm).

#### Task 3.4 Deliverables

- [ ] `/mode <id>` handler wired in `dispatch_stdio_command`
- [ ] Bare `/mode` special-cased error arm replaced with a current-mode display
- [ ] `/safety <policy>` handler wired in `dispatch_stdio_command`
- [ ] `CurrentModeUpdate` and `ConfigOptionUpdate` pushed to connection on
      success
- [ ] All Task 3.3 tests passing

#### Task 3.5 Success Criteria

- Typing `/mode write` in Zed updates the mode dropdown to "Write" and changes
  the terminal execution policy for the next prompt turn.
- Typing bare `/mode` shows the current mode instead of a generic usage error.
- Typing `/safety never_confirm` updates the safety indicator and disables
  confirmation prompts.

---

### Phase 4: Fix Model Switch and Wire `/model` Commands

`Provider::set_model` currently requires `&mut self`, which cannot be obtained
through the `Arc<dyn Provider>` shared with `SubagentTool`. This phase relaxes
the trait signature to `&self` (matching every existing implementation's
already-interior-mutable storage), fixes `set_session_model` to call it, and
wires the `/model` text commands.

#### Task 4.1 Change `Provider::set_model` to `&self`

Every concrete `Provider` implementation already mutates its model through an
`Arc<RwLock<Config>>` field via `self.config.write()`, not through a
directly-owned `&mut self.model` field — the `&mut self` requirement on the
trait is unnecessary and is the reason `set_model` cannot be called through a
shared `Arc<dyn Provider>`. Change the signature, not the bodies:

1. `src/providers/trait_mod.rs:81` — change
   `fn set_model(&mut self, model: &str);` to `fn set_model(&self, model: &str);`.
   Update the doc-example trait impl at lines 52 and the two doctest blocks at
   lines 253 and 337 to match (`fn set_model(&self, model: &str) { ... }`).
2. `src/providers/copilot.rs:2967` — change
   `fn set_model(&mut self, model: &str)` to `fn set_model(&self, model: &str)`.
   Body is unchanged (`self.config.write()` already works through `&self`).
3. `src/providers/ollama.rs:1107` — same signature change, body unchanged.
4. `src/providers/openai.rs:1256` — same signature change, body unchanged.
5. Update every test-only mock `Provider` implementation to match the new
   signature (`fn set_model(&mut self, _model: &str) {}` →
   `fn set_model(&self, _model: &str) {}`): `src/agent/core.rs:1793`, `2350`,
   `2397`, and the internal test modules in `src/providers/trait_mod.rs` at
   lines 426, 467, 508, 543, 581, 608, 647, 689, 730, 765, 815, 859, 908.
6. Search for any other caller of `.provider_mut()` or code that assumes
   `set_model` needs exclusive access and update it; there is no `provider_mut`
   accessor on `XzatomaAgent` today, so no additional call sites are expected
   beyond the mocks above.

This is a trait-level change; run `cargo check --all-targets --all-features`
after this task specifically (in addition to the Phase 7 quality gate) to catch
any missed implementor before proceeding to Task 4.2.

#### Task 4.2 Call `provider.set_model()` from `set_session_model`

In `AcpStdioServerState::set_session_model` in
[`src/acp/stdio.rs:972-994`](../../src/acp/stdio.rs):

1. After updating `session_lock.current_model_name`, clone
   `session_lock.xzatoma_agent`.
2. Release the session lock.
3. Acquire the agent lock: `let agent_lock = agent_handle.lock().await;`.
4. Call `agent_lock.provider().set_model(&model_id)` — now valid because
   `provider()` returns `&dyn Provider` and, after Task 4.1, `set_model` takes
   `&self`.
5. Remove the misleading comment
   `"takes effect on next session restart for inference"` from the tracing log.

#### Task 4.3 Implement `/model` Bare Handler

Replace the Phase 1 placeholder for the special-cased
`Err(CommandError::MissingArgument { command, .. }) if command == "/model"` arm
(Task 1.3):

1. Read `session.current_model_name` from the locked session state.
2. Return a formatted string: `"Active model: <name>"`.

`parse_special_command` never produces `Ok(SpecialCommand::ModelsHelp)` for
bare `/model` — `ModelsHelp` comes only from the plural `"/models"` command
(Task 4.5). Do not attempt to match `SpecialCommand::ModelsHelp` for this
handler.

#### Task 4.4 Implement `/model <name>` Switch Handler

In `dispatch_stdio_command`, handle `SpecialCommand::SwitchModel(model_name)`,
replacing the Phase 1 placeholder arm:

1. Build an `acp::SetSessionModelRequest` with the model name and session ID.
2. Call `state.set_session_model(request).await` (which now calls
   `provider.set_model()` after Task 4.2).
3. Return `"Model switched to <name>."`.

#### Task 4.5 Handle the `/models` Family with a Graceful Fallback

`SpecialCommand::ListModels`, `::ModelsHelp`, and `::ShowModelInfo(String)` are
produced by the plural `"/models"`, `"/models list"`, and `"/models info <name>"`
commands, which are not advertised by `build_available_commands()` and are out
of scope for full implementation in this plan (provider model-listing calls are
too expensive to make inline in a command handler, and the model dropdown
already provides full model discovery). To keep `dispatch_stdio_command`'s
match exhaustive and give a coherent response if a user finds these commands
anyway, replace all three Phase 1 placeholder arms with a single shared
message: `"Use Zed's model dropdown to list or inspect available models."` Do
not add `/models` to `build_available_commands()`.

#### Task 4.6 Testing Requirements

- `test_set_model_signature_compiles_with_shared_reference` - `provider()`
  followed by `.set_model(...)` compiles without a mutable borrow (compile-time
  check, satisfied by `cargo check` passing).
- `test_set_session_model_calls_provider_set_model` - after `set_session_model`
  returns, the mock provider's active model name equals the requested model ID.
- `test_set_session_model_updates_current_model_name` -
  `session.current_model_name` equals the new model after the call.
- `test_dispatch_bare_model_returns_current_model` - `/model` (bare) returns a
  string containing `session.current_model_name`, superseding the Phase 1
  placeholder-only assertion.
- `test_dispatch_switch_model_updates_provider_model` - after
  `dispatch_stdio_command("/model gpt-4o", ...)`, the mock provider's active
  model is `"gpt-4o"`.
- `test_dispatch_models_list_returns_fallback_message` - `/models list`
  returns `Some(Ok(...))` containing "model dropdown", not an unhandled-match
  compile error or a provider call.

#### Task 4.7 Deliverables

- [ ] `Provider::set_model` signature changed to `&self` across the trait,
      all three concrete providers, and all mock implementations
- [ ] `set_session_model` calls `provider.set_model()` after updating metadata
- [ ] Bare `/model` special-cased error arm replaced with a current-model
      display
- [ ] `/model <name>` switch handler wired
- [ ] `ListModels`, `ModelsHelp`, `ShowModelInfo` routed to the shared
      fallback message
- [ ] All Task 4.6 tests passing

#### Task 4.8 Success Criteria

- `cargo check --all-targets --all-features` passes after Task 4.1 with no
  remaining `&mut self` call sites for `set_model`.
- Clicking a model in Zed's model dropdown and then submitting a prompt uses
  that model for inference, not the original startup model.
- `/model gpt-4o` in the chat window switches the active provider model for
  subsequent turns.
- Typing bare `/model` shows the current model instead of a generic usage
  error.

---

### Phase 5: Context Commands (`/context`, `/summarize`)

Implement the two conversation-history commands. Both read from the live agent
without mutating session configuration.

#### Task 5.1 Implement `/context` Handler

Add `fn handle_context_info_command(agent: &XzatomaAgent) -> String`:

1. Call `agent.get_context_info(agent.conversation().max_tokens())`
   (parameter name in the real signature is `model_context_window`; call it
   positionally).
2. Read `used_tokens` from the result and `agent.conversation().max_tokens()`
   for the budget.
3. Compute `remaining = max - used` and `pct = used * 100 / max`.
4. Return a formatted multi-line status block consistent with the existing
   terminal-chat `/context` output in `src/commands/special_commands.rs`.

In `dispatch_stdio_command`, handle `SpecialCommand::ContextInfo`, replacing
the Phase 1 placeholder arm: acquire the agent lock, call
`handle_context_info_command`, release the lock, return the string.

#### Task 5.2 Implement `/summarize` Handler

Add
`async fn handle_summarize_command(agent: &mut XzatomaAgent, model_override: Option<&str>) -> Result<String>`:

1. Build a summarization prompt from `agent.conversation().messages()`.
2. Submit it to `agent.provider().complete(...)` using `model_override` if
   present, otherwise the current model.
3. Replace the stored conversation with a single system message containing the
   summary via
   `agent.conversation_mut().replace_conversation_with_summary(summary)`. This
   method does not currently exist anywhere in the codebase — add it to
   `src/agent/conversation.rs` alongside `replace_first_system_message`.
4. Return a `"Conversation summarized. N messages replaced with summary."`
   string.

In `dispatch_stdio_command`, handle `SpecialCommand::ContextSummary { model }`,
replacing the Phase 1 placeholder arm: acquire the agent lock, call
`handle_summarize_command`, release the lock, return the string.

#### Task 5.3 Testing Requirements

- `test_handle_context_info_command_returns_non_empty_string` - result is not
  empty for a freshly created agent.
- `test_handle_context_info_command_includes_tokens_label` - result contains
  `"tokens"` or `"used"`.
- `test_dispatch_context_info_returns_some` -
  `dispatch_stdio_command("/context")` returns `Some(Ok(...))`.
- `test_replace_conversation_with_summary_reduces_message_count` - after
  calling the new `replace_conversation_with_summary` method directly, the
  conversation has exactly one system message.
- `test_handle_summarize_command_reduces_message_count` - after the call the
  agent's conversation has fewer messages than before.

#### Task 5.4 Deliverables

- [ ] `replace_conversation_with_summary` added to
      `src/agent/conversation.rs`
- [ ] `handle_context_info_command` implemented and routed
- [ ] `handle_summarize_command` implemented and routed
- [ ] All Task 5.3 tests passing

#### Task 5.5 Success Criteria

- `/context` returns the current token usage without an LLM turn.
- `/summarize` compacts the conversation and reports the before/after count.

---

### Phase 6: Subagents Toggle and System Prompt (`/subagents`, `/system`)

Implement the two remaining state-mutating commands that have no equivalent
native Zed UI widget.

#### Task 6.1 Implement `/subagents on|off` Handler

`ToolRegistry` is defined in [`src/tools/mod.rs:398`](../../src/tools/mod.rs)
(not `src/agent/tool_registry.rs`, which does not exist). Its `register`
method signature is `register(&mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>)`
(`mod.rs:428`) — there is no `Tool` trait in this codebase, only
`ToolExecutor` (`mod.rs:358`).

In `dispatch_stdio_command`, handle `SpecialCommand::ToggleSubagents(enable)`,
replacing the Phase 1 placeholder arm:

1. Acquire the session lock.
2. Set `session.runtime_state.subagents_enabled = enable`.
3. Acquire the agent lock (after releasing the session lock).
4. If `enable`:
   - Build a new `SubagentTool` using the session config (same code path as
     `create_session`, `stdio.rs:557-563`).
   - Call `agent.tools_mut().register("subagent", Arc::new(subagent_tool) as Arc<dyn ToolExecutor>)`.
5. If `!enable`:
   - Call `agent.tools_mut().deregister("subagent")`. Add
     `pub fn deregister(&mut self, name: &str) -> Option<Arc<dyn ToolExecutor>>`
     to `ToolRegistry` in `src/tools/mod.rs` — no equivalent removal method
     exists today (`clone_without` returns a new filtered registry rather than
     mutating in place).
6. Return `"Subagent delegation enabled."` or `"Subagent delegation disabled."`.

#### Task 6.2 Implement `/system <text>` Handler

In `dispatch_stdio_command`, handle `SpecialCommand::SetSystemPrompt(text)`,
replacing the Phase 1 placeholder arm:

1. Acquire the agent lock.
2. Call `agent.conversation_mut().replace_first_system_message(text)`.
3. Return `"System prompt updated for this session."`.

This mirrors the system-prompt injection in `create_session` and allows the user
to adjust the system prompt mid-session without restarting.

#### Task 6.3 Testing Requirements

- `test_tool_registry_deregister_removes_entry` - after calling
  `deregister("subagent")`, `registry.get("subagent")` returns `None`.
- `test_dispatch_subagents_off_deregisters_tool` - after dispatch, the agent's
  tool registry does not contain `"subagent"`.
- `test_dispatch_subagents_on_registers_tool` - after dispatch, the agent's tool
  registry contains `"subagent"`.
- `test_dispatch_subagents_off_sets_runtime_state` -
  `session.runtime_state.subagents_enabled == false` after disabling.
- `test_dispatch_set_system_prompt_replaces_first_system_message` - after
  dispatch, the agent's first system message equals the text passed to
  `/system`.

#### Task 6.4 Deliverables

- [ ] `/subagents on|off` handler implemented and routed
- [ ] `ToolRegistry::deregister` added to `src/tools/mod.rs`
- [ ] `/system <text>` handler implemented and routed
- [ ] All Task 6.3 tests passing

#### Task 6.5 Success Criteria

- `/subagents off` prevents the subagent tool from being invoked for subsequent
  prompts. `/subagents on` restores it.
- `/system You are a pirate.` replaces the system prompt for the rest of the
  session; the next prompt turn reflects the new instruction.

---

### Phase 7: Documentation and Quality Gate

#### Task 7.1 Add an Index Entry to `docs/explanation/implementations.md`

`docs/explanation/implementations.md` is an index of bolded markdown links,
each with a one-line description (e.g.
`- **[phase5_deprecation_and_migration_implementation.md](...)** - Phase 5: ...`).
Add one entry in this format pointing at this plan document itself:
`- **[xzatoma_acp_chat_commands_implementation_plan.md](xzatoma_acp_chat_commands_implementation_plan.md)** - ACP chat command dispatch, model-switch fix, and Provider::set_model signature change.`
Do not duplicate the phase-by-phase detail into `implementations.md`; this plan
document is the source of truth and is not archived, since it is the current
explanation doc for this feature area.

#### Task 7.2 Create `docs/reference/acp_chat_commands.md`

Document each of the nineteen real slash commands (twelve advertised via
`build_available_commands()`, plus `/auth`, `/streaming`, `/exit`/`/quit`,
`/mentions`, `/models` family, and bare-argument error cases) with:

- Syntax and optional arguments
- Runtime effect and which session state fields it mutates
- Whether a native Zed UI widget provides equivalent functionality
- A concrete example interaction

Use `docs/reference/model_management.md` and `docs/reference/subagent_api.md`
as structural templates (`## Overview`, typed section headers, field bullet
lists). Include a summary table mapping each `SpecialCommand` variant to its
handler function and Phase 1's "final"/"placeholder" disposition, and note
explicitly that `Auth`, `ToggleStreaming`, and `Exit` are intentionally
unsupported over ACP rather than unimplemented.

#### Task 7.3 Add `///` Doc Comments to All New Public Symbols

Every new `pub` function, struct, enum variant, and module added across
Phases 1-6 must carry a complete `///` doc comment with `# Arguments`,
`# Returns`, `# Errors`, and `# Examples` sections where applicable. This
includes the three new `SpecialCommand` variants, `format_help_text`,
`format_mention_help_text`, `ToolRegistry::deregister`, and
`replace_conversation_with_summary`.

#### Task 7.4 Full Quality Gate

Run in order, stopping on first failure:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Then lint all new or modified Markdown files:

```bash
markdownlint --fix --config .markdownlint.json docs/explanation/xzatoma_acp_chat_commands_implementation_plan.md
markdownlint --fix --config .markdownlint.json docs/reference/acp_chat_commands.md
prettier --write --parser markdown --prose-wrap always docs/explanation/xzatoma_acp_chat_commands_implementation_plan.md
prettier --write --parser markdown --prose-wrap always docs/reference/acp_chat_commands.md
```

#### Task 7.5 Deliverables

- [ ] `docs/explanation/implementations.md` updated with one index entry
- [ ] `docs/reference/acp_chat_commands.md` created
- [ ] All new public symbols carry complete `///` doc comments
- [ ] `cargo test --all-features` passes with zero failures
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes with
      zero warnings
- [ ] All Markdown files pass `markdownlint` and `prettier`

#### Task 7.6 Success Criteria

- All nineteen slash commands (twelve advertised plus seven unadvertised but
  parseable) produce correct or intentionally-fixed responses in Zed's chat
  window without triggering LLM requests, and `dispatch_stdio_command`'s match
  has no placeholder arms remaining.
- Mode, safety, subagents, and system prompt changes made via slash commands
  persist for the rest of the session and match the behavior of the equivalent
  native Zed UI widget.
- Model dropdown selection and `/model <name>` both switch the live provider
  model and affect the next inference turn, with `Provider::set_model` taking
  `&self` everywhere in the codebase.
- Test coverage across all modified modules exceeds 80%.
- Zero clippy warnings and zero test failures.

---

## File Change Summary

| File                                    | Change                                                                                                                                                       |
| ---------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/acp/stdio.rs`                       | Add `dispatch_stdio_command` and `handle_not_yet_implemented`; wire into `enqueue_prompt`; fix `set_session_model` to call `provider.set_model()`; add all command handlers |
| `src/acp/available_commands.rs`          | Add `/help`, `/status`, `/subagents`, `/system` command entries; update length assertion to 12                                                                |
| `src/commands/special_commands.rs`       | Add `ListTools`, `ListSkills`, `ShowMcpStatus` variants and parser arms; add `format_help_text`, `format_mention_help_text`; refactor `print_help`/`print_mention_help` to delegate |
| `src/providers/trait_mod.rs`             | Change `Provider::set_model` signature from `&mut self` to `&self`; update doc examples and internal test mocks                                               |
| `src/providers/copilot.rs`               | Change `set_model` signature to `&self` (body unchanged)                                                                                                       |
| `src/providers/ollama.rs`                | Change `set_model` signature to `&self` (body unchanged)                                                                                                       |
| `src/providers/openai.rs`                | Change `set_model` signature to `&self` (body unchanged)                                                                                                       |
| `src/agent/core.rs`                      | Update mock `Provider` test impls to `&self` `set_model`                                                                                                       |
| `src/agent/conversation.rs`              | Add `replace_conversation_with_summary` method                                                                                                                 |
| `src/tools/mod.rs`                       | Add `deregister(name)` method to `ToolRegistry`                                                                                                                |
| `docs/explanation/implementations.md`    | Add one index entry for this plan                                                                                                                              |
| `docs/reference/acp_chat_commands.md`    | New file: command reference                                                                                                                                    |

## Key Design Decisions

### Why intercept in `enqueue_prompt` rather than `run_prompt_worker`?

`enqueue_prompt` holds the `session` Arc and the connection before the message
enters the queue. Intercepting there lets command responses return on the same
call path as normal prompts — the caller receives `Ok(PromptResponse)` and Zed
sees a properly formed `EndTurn` response. Intercepting inside
`run_prompt_worker` or `execute_queued_prompt` would require passing command
metadata through `QueuedPrompt`, coupling the prompt worker to the command
parser unnecessarily.

### Why use `set_session_mode` and `set_session_config_option` from the slash command path?

These handlers already contain the full runtime-effect logic: terminal tool
replacement, system prompt rebuild, and `SessionUpdate` notification dispatch.
Calling them from `dispatch_stdio_command` instead of duplicating the logic
keeps the two paths (native UI dropdown and slash command) in sync automatically
when either is modified in the future.

### Why relax `Provider::set_model` to `&self` instead of restructuring `XzatomaAgent`'s provider storage?

The alternative — wrapping `provider` in `Arc<Mutex<dyn Provider>>` or similar
— would require every one of the nine existing `.provider()` call sites
(`src/commands/mod.rs` and `src/acp/stdio.rs`) to become async-lock-aware, and
would serialize every `complete()` call behind a single lock even though
`complete` is the hot path. It is unnecessary: every concrete provider already
stores its mutable state in an `Arc<RwLock<Config>>` field and mutates through
`&self` (the same pattern `set_thinking_effort(&self, ...)` already uses on
`CopilotProvider`). The `&mut self` on the trait method is a leftover
restriction that doesn't match any implementation's actual storage. Relaxing
the signature is a small, mechanical, low-risk change confined to five files
plus test mocks.

### Why add `ListTools`/`ListSkills`/`ShowMcpStatus` to `SpecialCommand` instead of special-casing raw text in `dispatch_stdio_command`?

Keeping a single parsing entry point avoids two parallel code paths for what
are conceptually the same category of command as `/status` and `/help`, which
are already parsed this way. It also means `build_available_commands()`'s
claim that `/tools`, `/skills`, `/mcp` are supported slash commands becomes
true at the parser level, not just the autocomplete-advertising level.

### Why give `Auth`, `ToggleStreaming`, and `Exit` fixed "not supported" messages instead of implementing them?

None of the three has a meaningful ACP-mode equivalent: authentication is
managed via CLI/config outside a chat session and has no `PromptResponse`-level
UI to drive it; response streaming is controlled by the Zed client, not the
agent process, so toggling it server-side has no observable effect; and there
is no ACP mechanism to terminate a session from inside a `PromptResponse` — the
IDE owns the session lifecycle. Returning a clear, permanent message is safer
than silently forwarding these to the LLM (which would be surprising) or
leaving the dispatcher's `match` non-exhaustive (which would not compile).

### Why keep the `/models` family (plural) out of scope beyond a fallback message?

`/models`, `/models list`, and `/models info <name>` are a distinct,
currently-unadvertised command family from the advertised singular `/model`.
Implementing them fully requires a live `fetch_models()` call, which is
expensive to make synchronously inside a command dispatch and is already
covered by the Zed model dropdown's own discovery UI. A graceful fallback
keeps the dispatcher's match exhaustive without expanding this plan's scope
into full model-listing support.

### Why fix `set_session_model` in Phase 4 rather than Phase 1?

`set_session_model` is called by the native Zed model dropdown, which is
already broken for inference. Fixing it in Phase 4 (alongside the `Provider`
trait signature change and the `/model` text commands) groups all three
related model-switch changes into a single coherent phase, making the change
easier to test and review together, and ensures the trait-level signature
change (Task 4.1) is fully validated with `cargo check` before any command
handler depends on it.

### Why add `deregister` to `ToolRegistry` for subagents?

The subagent tool must be removed from the registry, not just replaced with a
no-op, so that the LLM does not see `subagent` in its tool list at all when
delegation is disabled. `ToolRegistry::clone_without` already exists but
returns a new filtered registry rather than mutating in place, which does not
fit the in-place toggle this command needs. A `deregister(name)` method is the
minimal, explicit, mutating API for this.

### Why keep `/context` and `/summarize` as read-only LLM-free operations?

`/context` reads agent state that is available without an LLM call. `/summarize`
does make a provider call, but it does so under the agent lock synchronously
from the dispatch path rather than through the prompt worker. This avoids
introducing an async task that could race with a concurrent prompt. The
summarization model can be overridden via the `--model` flag on the command.

### Why intercept bare `exit`/`quit` chat messages, and is that a problem?

`parse_special_command` already treats a chat message consisting of exactly
`"exit"` or `"quit"` as `SpecialCommand::Exit` (`special_commands.rs:198`),
independent of this plan — that behavior exists today in the terminal chat
path and is inherited automatically once `enqueue_prompt` starts calling the
same parser. It only matches an entire message equal to `"exit"` or `"quit"`,
not a substring, so ordinary prose containing those words is unaffected. Phase
1 makes this explicit and tested rather than leaving it as a silent side
effect of reusing the shared parser.
