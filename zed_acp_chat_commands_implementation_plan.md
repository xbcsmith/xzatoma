# Zed ACP Chat Commands Implementation Plan

## Overview

`atoma agent` (the Zed ACP stdio server in
[src/commands/agent.rs](../../src/commands/agent.rs)) has two independent
problems. First, it never calls `SpecialCommand::parse` (see
[src/chat_mode/commands.rs](../../src/chat_mode/commands.rs)), so typed commands
like `/help`, `/status`, `/subagents on`, or `/mode code` are forwarded to the
LLM as ordinary chat text instead of being executed locally, unlike `atoma chat`
([src/commands/chat.rs](../../src/commands/chat.rs)). Second, two native Zed UI
widgets are broken: the model dropdown never renders at all, and the Thinking
(reasoning-level) dropdown renders but selections have no effect. Investigation
traced both native-widget bugs to a single root cause — a missing
`session/set_config_option` handler — described in "Current State Analysis"
below. This plan fixes the native-widget bugs first (Phase 1), then adds the
missing chat-command surface (Phases 2-8), including making `/mode`'s tool
restriction technically enforced instead of advisory-only, per your direction.
`/context` remains out of scope: Zed's own client already provides equivalent
context-window UI.

> **Update (2026-07-15):** `agent-client-protocol` was upgraded from `0.11.1` to
> `1.2.0` after Phase 1 shipped (see `docs/explanation/implementations.md`,
> "build: upgrade agent-client-protocol to 1.2.0"). Two things in this plan
> changed as a result and are called out inline below rather than rewritten
> silently: (1) all `agent_client_protocol::schema` types are now under the
> versioned `agent_client_protocol::schema::v1` module (`ProtocolVersion` is the
> sole exception, still top-level); (2) the unstable `unstable_session_model`
> mechanism (`ModelInfo`, `SessionModelState`, `SetSessionModelRequest`/
> `SetSessionModelResponse`, `NewSessionResponse::models()`/
> `LoadSessionResponse::models()`) referenced throughout the original Phase 1
> text was removed entirely upstream in `1.0.0` and deleted from this codebase —
> it is **not** kept additively alongside the stable `SessionConfigOption`
> mechanism as Phase 1 originally planned. Line numbers cited below for
> `src/commands/agent.rs` were re-checked against the post-upgrade file and
> updated where they had drifted; phases 2-9 have not started yet, so their line
> numbers should be re-verified again immediately before each phase begins,
> since further drift is expected as earlier phases land.

## Current State Analysis

### Existing Infrastructure

- [src/chat_mode/commands.rs](../../src/chat_mode/commands.rs) —
  `SpecialCommand` enum, `parse()`, and `execute()`. `execute()` already handles
  `Help`, `Status`, and `ListTools` generically against a `ChatModeState`, with
  no side effects and no provider calls, making them directly reusable.
- [src/chat_mode/mod.rs](../../src/chat_mode/mod.rs) — `ChatModeState`,
  `ChatMode` (`Full`/`Code`/`ReadOnly`), `SafetyMode`
  (`Interactive`/`Restricted`/`Full`, maps 1:1 onto `TerminalMode`), and
  subagent/streaming toggle helpers (`enable_subagents`, `disable_subagents`,
  `toggle_streaming`, etc.).
- [src/agent/executor.rs](../../src/agent/executor.rs)
  `pub fn build_system_prompt(user_prefix: Option<&str>, base: &str) -> String`
  (L1578) — the shared merge primitive already used by both
  `src/commands/chat.rs` and `src/commands/agent.rs` (L1423-1432, L2561-2570) to
  combine a user-supplied prefix with a base prompt. Reusable for `/system` and
  `/mode` without modification.
- [src/agent/registry.rs](../../src/agent/registry.rs)
  `pub fn clone_without(&self, tool_name: &str) -> Self` (L201) and
  `pub fn with_subagents_enabled(&self, enabled: bool) -> Self` (L245) — the
  existing primitives `src/commands/chat.rs` uses to rebuild the tool registry
  live. Directly reusable/extendable for real `/mode` enforcement.
- [src/agent/executor.rs](../../src/agent/executor.rs) — `list_models()`,
  `get_current_model()`, `get_current_model_info()`, `get_model_info()`,
  `switch_model()` (L1331-1491) — the same methods already called by both
  `src/commands/chat.rs` and the `session/set_config_option` handler (see below;
  the `SetSessionModelRequest`/`session/set_model` handler that originally also
  called these was removed when the project upgraded to `agent-client-protocol`
  `1.2.0`).
- [src/commands/agent.rs](../../src/commands/agent.rs):
  - `SessionState` (L137-168) tracks `executor`, `cancel_token`,
    `conversation_ulid`, `cwd`, `terminal_mode`, `vision_capable`,
    `mcp_manager`. No fields exist yet for `ChatMode`, subagents-enabled,
    streaming, or a per-session system-prompt prefix.
  - `thought_level_config_option()` (L664-693) and `model_config_option()`
    (L720-733) build the `SessionConfigOption`s (categories `ThoughtLevel` and
    `Model`) included in every `NewSessionResponse` / `LoadSessionResponse`.
    `apply_thinking_mode()` (L764-~790) and `apply_model_switch()` (L828-~865)
    are the shared helpers — introduced in Phase 1 — that both the
    `SetSessionModeRequest`/`SetSessionConfigOptionRequest` handlers (thinking)
    and the `SetSessionConfigOptionRequest` handler (model) call.
  - `SetSessionModeRequest` handler (L2073-2170) fully implements `/safety`
    semantics natively via `session/set_mode`: mutates
    `session_state.terminal_mode`, calls `exec.apply_terminal_mode()` once per
    turn, and emits `CurrentModeUpdate`. This mechanism is confirmed working
    (see `docs/explanation/implementations.md`, "ACP Phase 1: Diagnose and Fix
    Mode Selector") and must not be modified except by Phase 4.1, which extracts
    its `Terminal` branch (L2104-2125) into a shared helper.
  - `SetSessionConfigOptionRequest` handler (registered immediately after
    `SetSessionModeRequest`) is the stable, non-feature-gated counterpart for
    both Thinking and Model selection. It is the **only** model-selection
    mechanism in this codebase: the previous `SetSessionModelRequest` handler
    and the entire `unstable_session_model` mechanism it implemented
    (`ModelInfo`, `SessionModelState`, `NewSessionResponse.models()` /
    `LoadSessionResponse.models()`) were removed from `agent-client-protocol`
    upstream as of `1.0.0` — with no replacement feature flag — and deleted from
    this codebase when it was upgraded to `1.2.0` (see
    `docs/explanation/implementations.md`, "build: upgrade agent-client-protocol
    to 1.2.0"). There is no additive/legacy path left to preserve.
  - `AvailableCommandsUpdate` (L1637-1639, L2748-2750) is built solely from
    `executor.tool_registry().tools()` — tool names, not chat commands.
  - `ChatMode::available_tools()`
    ([src/chat_mode/mod.rs](../../src/chat_mode/mod.rs) L87-100) lists only 6
    tool names (`read_file`, `write_file`, `list_directory`, `execute_command`,
    `grep`, `fetch_url`) and has never been updated for tools added later:
    `edit_file`, `create_directory`, `delete_path`, `move_path`, `copy_path`,
    `find_path`, `subagent`, and any MCP-forwarded tool. Nothing in the codebase
    calls `ChatModeState::is_tool_available` outside its own unit tests — this
    is why `/mode` is advisory only today: the list exists, but no tool-dispatch
    or registry code ever consults it. It only ever fed `get_system_prompt()`'s
    text (an ACP-unsafe function — see Identified Issues).
- MCP-forwarded tools are named with a `server__tool` double-underscore
  convention, and the two built-in bridge tools are named `mcp_read_resource`
  and `mcp_get_prompt` ([src/mcp/tool_bridge.rs](../../src/mcp/tool_bridge.rs)
  L497+, `tests/unit/mcp/mcp_tool_bridge.rs` L259/L319). This gives a reliable
  way to exempt MCP tools from any name-based allow-list.

### Identified Issues

| Item                                                                                          | Root Cause                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reasoning-level dropdown shows values but selecting one has no effect (always reverts to Off) | Zed submits `SessionConfigOption` changes via the stable `session/set_config_option` JSON-RPC method (`SetSessionConfigOptionRequest`/`SetSessionConfigOptionResponse` in `agent-client-protocol-schema` — **not** feature-gated, always compiled in). `src/commands/agent.rs` never registers a handler for this request type at all — only `SetSessionModeRequest`/`session/set_mode` is handled. Zed's selection is silently rejected/ignored, so the UI reverts to the last server-echoed value. That value is "none"/Off whenever `thinking_mode` resolves to `ThinkingMode::Auto` (e.g. `thinking_mode: auto` in config), because `thought_level_config_option()` deliberately maps `Auto → "none"` for _display_ purposes only (L670-676) — this display mapping is correct and unrelated to the actual bug. |
| Model dropdown does not render at all                                                         | Model exposure only uses the **`unstable_session_model`** mechanism (`NewSessionResponse.models()`/`SessionModelState`), explicitly marked not-yet-stable upstream. `SessionConfigOptionCategory::Model` exists in the same schema version as a stable, generic alternative (a `SessionConfigOption` with `category: Model`, submitted through the same `session/set_config_option` method used for Thinking). The evidence is consistent with the installed Zed client rendering config-option-based selectors (Thinking shows up) but not the unstable dedicated model-selection messages (model dropdown does not show up at all).                                                                                                                                                                               |
| All `/` commands                                                                              | `PromptRequest` handler never calls `SpecialCommand::parse`; every command is sent to the LLM as text.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `/help`, `/status`, `/tools`                                                                  | Side-effect-free, but nothing routes them to `SpecialCommand::execute`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `/subagents`                                                                                  | No per-session enabled/disabled state on `SessionState`; no live tool-registry rebuild wired in ACP mode.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `/stream`                                                                                     | No per-session toggle; `stream_notifier` is unconditionally installed every turn.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `/system`                                                                                     | `SystemPromptAction` parses but `execute()` returns placeholder text only; no per-session prefix storage in ACP mode.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `/mode` is advisory only                                                                      | `ChatMode::available_tools()` is stale (6 of 12+ tool names) and nothing enforces it; it only ever fed the ACP-unsafe `get_system_prompt()` text generator (would clobber `AGENT_ACP_BASE_PROMPT`/`PLAN_ANCHOR_INSTRUCTION` and break the Zed Plan/checklist feature if used directly).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `/safety`                                                                                     | Native `session/set_mode` works, but the typed `/safety` alias is not intercepted and shares no helper with the `SetSessionModeRequest` handler.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Discoverability                                                                               | `AvailableCommandsUpdate` only lists tool names; new chat commands are invisible to Zed's `/` autocomplete.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |

> The first two rows above are historical: both were fixed by Phase 1 (already
> complete). Note for future readers that the "unstable `unstable_session_model`
> mechanism" the second row describes as "not-yet-stable upstream" was not
> subsequently stabilized — it was removed outright in `agent-client-protocol`
> `1.0.0`, confirming the stable `SessionConfigOption` approach Phase 1 chose
> was the only durable path.

## Implementation Phases

Two standing rules apply to every phase below:

1. Each phase's Deliverables checklist uses `- [ ]` items. As each item is
   verified complete, edit this document in place and change its checkbox to
   `- [x]` ("completed"). Do not begin the next phase until the current phase's
   deliverables are all checked off and its Success Criteria are met.
2. Each phase's Deliverables list includes adding an entry to
   `docs/explanation/implementations.md` (per Rule 8) documenting that phase's
   changes. This entry is part of the phase's definition of done, not deferred
   to Phase 9.

### Phase 1: Fix Native Model and Reasoning-Level Selectors

#### 1.1 Add the `SetSessionConfigOptionRequest` Handler

Register a new `.on_receive_request()` handler for
`SetSessionConfigOptionRequest` in `make_agent_builder`, matching on
`req.config_id.0.as_ref()`. For `"thought_level"`, extract the body of
`SetSessionModeRequest`'s `SessionModeChange::Thinking` arm into a shared helper
(e.g. `apply_thinking_mode(session_id, new_mode, sessions, cx)`), call it, and
respond with `SetSessionConfigOptionResponse::new(vec![...])` carrying the
_full_ refreshed set of config options (both `thought_level` and `model`, see
1.2 below), per the schema's documented contract for this response. Also emit
the existing `ConfigOptionUpdate` notification for symmetry with
`SetSessionModeRequest`. Unknown `config_id` values return
`agent_client_protocol::Error::invalid_params()`.

#### 1.2 Add a Model `SessionConfigOption`

Add
`model_config_option(current_model: &str, models: &[ModelInfo]) -> SessionConfigOption`
(`ModelInfo` here is `crate::providers::types::ModelInfo`, unrelated to any ACP
schema type) (category `Model`, `Select` kind, one `SessionConfigSelectOption`
per model) and include it in `config_options` at both `NewSessionResponse` and
`LoadSessionResponse` construction, alongside `thought_level_config_option`.

> **Update (agent-client-protocol 1.2.0 upgrade):** this phase originally also
> kept the legacy `.models()`/`SessionModelState` call alongside
> `model_config_option`, additive rather than a replacement, for compatibility
> with clients that only understood the unstable field. That legacy mechanism
> (`ModelInfo`, `SessionModelState`, `NewSessionResponse::models()`/
> `LoadSessionResponse::models()`) was removed entirely upstream in
> `agent-client-protocol` `1.0.0` — not stabilized, deprecated-but-kept, or
> renamed, just deleted — so there is nothing left to keep alongside
> `model_config_option`. It was removed from this codebase's
> `NewSessionRequest`/`LoadSessionRequest` handlers when the project upgraded to
> `1.2.0` (see `docs/explanation/implementations.md`, "build: upgrade
> agent-client-protocol to 1.2.0"). `SessionConfigOption` is now the sole model
> selection mechanism.

#### 1.3 Wire `config_id == "model"` in the Phase 1.1 Handler

Extract the switch-model + capability re-detection body of the model-switching
request handler into a shared helper (e.g.
`apply_model_switch(session_id, model_id, sessions)` returning the refreshed
`Option<crate::providers::types::ModelInfo>`), call it from the
`SetSessionConfigOptionRequest` handler, and echo the updated
`model_config_option` in the response.

> **Update (agent-client-protocol 1.2.0 upgrade):** this phase originally called
> `apply_model_switch` from both a dedicated `SetSessionModelRequest` handler
> (the unstable `session/set_model` method) and the new
> `SetSessionConfigOptionRequest` handler. `SetSessionModelRequest`/
> `SetSessionModelResponse` no longer exist upstream as of
> `agent-client-protocol` `1.0.0`, so that handler was deleted from this
> codebase during the `1.2.0` upgrade; `apply_model_switch` is now called only
> from `SetSessionConfigOptionRequest`.

#### 1.4 Testing Requirements

Unit tests: a `SetSessionConfigOptionRequest` with `config_id: "thought_level"`
and `value: "high"` changes the executor's thinking mode identically to the
equivalent `SetSessionModeRequest`; the response contains both config options
with the new current value reflected; `config_id: "model"` switches the provider
model and updates `vision_capable`; an unrecognized `config_id` returns
`invalid_params`; existing `SetSessionModeRequest` tests still pass after
extracting the shared helper (regression coverage). (The equivalent
`SetSessionModelRequest` regression tests referenced in this phase originally
were removed along with that handler during the `agent-client-protocol` `1.2.0`
upgrade, since the request type no longer exists upstream.)

#### 1.5 Deliverables

- [x] `SetSessionConfigOptionRequest` handler registered and dispatches on
      `config_id`.
- [x] `model_config_option` added to `NewSessionResponse`/`LoadSessionResponse`.
- [x] Shared `apply_thinking_mode`/`apply_model_switch` helpers used by both the
      legacy and new request handlers. Superseded: the legacy
      `SetSessionModelRequest` handler was deleted during the
      `agent-client-protocol` `1.2.0` upgrade (see 1.2/1.3 addenda above), so
      `apply_model_switch` is now called only from
      `SetSessionConfigOptionRequest`. `apply_thinking_mode` is still shared
      between `SetSessionModeRequest` and `SetSessionConfigOptionRequest`,
      unaffected.
- [x] All tests above pass; full Rule 5 quality gate passes.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 1.6 Success Criteria

Selecting a Thinking level in Zed's dropdown takes effect immediately and
persists across turns. A model dropdown appears in Zed and switching models
through it works, verified by inspecting `RUST_LOG=trace` output for an incoming
`session/set_config_option` request and the corresponding `ConfigOptionUpdate`
notification.

### Phase 2: Chat Command Interception Foundation

#### 2.1 Extend `SessionState`

Add `chat_mode: ChatMode` (default `ChatMode::Full`), `subagents_enabled: bool`
(seeded from `config.agent.subagent.chat_enabled` at session creation — the same
field `ChatModeState::from_config` (`src/chat_mode/mod.rs` L353) uses for
`atoma chat`), `stream_enabled: bool` (seeded from `config.agent.chat_streaming`
— matching `ChatModeState::from_config` L354, so a user's
`chat_streaming: false` config setting is honored in Zed/ACP sessions instead of
being silently overridden by a hardcoded default),
`system_prompt_prefix: Option<String>` (seeded from
`config.agent.system_prompt.clone()`), and `base_tool_registry: ToolRegistry`
(the fully-populated registry captured once, before any `/mode`/`/subagents`
filtering, so repeated filtering is always computed from the same canonical set
— see Phase 6). All five fields are reset to these config-derived defaults on
every `NewSessionRequest` **and** `LoadSessionRequest` — none are persisted or
restored from prior session state, per your direction that
`/subagents`/`/stream`/`/system` (and `/mode`) reset on resume.

#### 2.2 Add the Interception Point

At the top of the `PromptRequest` handler, after `assemble_prompt` produces
`prompt_text` but before the session-data lock block (currently the "Lock map
briefly to get session data and reset cancel token" block around L1667;
re-verify this line number immediately before starting this phase, since Phase
1's own line numbers above have already drifted once as a result of later
edits), check `SpecialCommand::parse(prompt_text.trim())`. On `None`, fall
through to existing behavior unchanged. On `Some(Ok(cmd))` or `Some(Err(_))`,
skip the executor iteration loop entirely - no `add_user_message`, no
`execute_iteration`, no conversation persistence. Send the result as one
`SessionUpdate::AgentMessageChunk` notification and respond with
`PromptResponse::new(StopReason::EndTurn)`.

#### 2.3 Add the Dispatcher Module

Create `src/commands/agent_chat_commands.rs` with
`pub async fn handle_special_command(cmd: SpecialCommand, session_id: &SessionId, sessions: &Arc<Mutex<HashMap<SessionId, SessionState>>>, cx: &RequestContext) -> String`,
`match`-ing every `SpecialCommand` variant exhaustively from this phase onward
(variants not yet wired return a "Not supported in Zed agent mode." placeholder
until their phase lands). `SpecialCommand::Exit` (`/exit`/`/quit`) is
**permanently** — not just "not yet" — routed to this placeholder: ACP stdio
sessions are owned and torn down by Zed's client, not by the agent, so there is
no equivalent action to wire in a later phase. Return a fixed string such as
"`/exit` is not supported in Zed. Close this session from Zed's UI." and leave
it unchanged for the rest of this plan.

#### 2.4 Expose Commands for Discovery

Extend the `AvailableCommand` list built at L1637-1639 and L2748-2750 (these
line numbers moved during the `agent-client-protocol` 1.2.0 upgrade and Phase
1's own edits; re-verify before starting this phase) to append one entry per
supported `SpecialCommand` (name + short description sourced from the doc
comments in [src/chat_mode/commands.rs](../../src/chat_mode/commands.rs)
L56-164), merged with the existing tool-derived entries.

#### 2.5 Testing Requirements

Unit tests: new `SessionState` fields have the documented defaults, including
`subagents_enabled` reflecting `config.agent.subagent.chat_enabled` and
`stream_enabled` reflecting `config.agent.chat_streaming` when either is
overridden away from its own default in a test config; `PromptRequest` routes
`/`-prefixed text to the dispatcher instead of `execute_iteration` (mock
provider records zero calls); non-`/`-prefixed text still reaches
`execute_iteration` unchanged; `SpecialCommand::parse` error text is surfaced as
an `AgentMessageChunk`; `/exit` returns the fixed "not supported" string without
mutating `SessionState` or calling `execute_iteration`; the merged
`AvailableCommand` list contains both a known tool name and a known chat-command
name; a `LoadSessionRequest` for a session that previously had `/mode readonly`
applied resets `chat_mode` to `ChatMode::Full` (config default). Per Rule 9, use
`MockProvider`-style in-process fakes only.

#### 2.6 Deliverables

- [x] `SessionState` carries all five new fields with config-derived defaults.
- [x] `PromptRequest` handler intercepts `/`-prefixed text before the executor
      loop.
- [x] `src/commands/agent_chat_commands.rs` dispatcher exists with an exhaustive
      `match` over `SpecialCommand`.
- [x] `AvailableCommandsUpdate` includes chat commands alongside tool names.
- [x] `SpecialCommand::Exit` returns the fixed not-supported string and mutates
      no state.
- [x] All tests above pass; full Rule 5 quality gate passes.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 2.7 Success Criteria

Typing any `/`-prefixed text in Zed no longer reaches the LLM as a raw prompt;
unrecognized commands return the same parse-error text `atoma chat` shows;
resuming a session always starts from config defaults for chat-command state.

### Phase 3: Local Informational Commands (`/help`, `/status`, `/tools`)

#### 3.1 Build an Ephemeral `ChatModeState` Adapter

Add a private helper in `agent_chat_commands.rs` that constructs a
`ChatModeState` on demand from `SessionState` fields (`chat_mode`, `safety_mode`
derived from `terminal_mode`, `subagents_enabled`, `stream_enabled`,
provider/model name from `executor.get_current_model()`), matching the shape
`SpecialCommand::execute(&ChatModeState, model_override)` expects.

#### 3.2 Wire `Help`, `Status`, `ListTools`

Route `SpecialCommand::Help { command }`, `SpecialCommand::Status`, and
`SpecialCommand::ListTools` straight to `cmd.execute(&adapter_state, None)`,
mirroring `src/commands/chat.rs` L1107-1115 (`Status`'s tool count and
`ListTools`'s listing must reflect the _current_ — possibly `/mode`-restricted —
registry from Phase 6, not the static `ChatMode::available_tools()` list). Note
that `SpecialCommand::execute` unconditionally calls `colored::Colorize` methods
(`colored_tag()`, `.bright_cyan()`, etc. — see `src/chat_mode/commands.rs`
L388-472, L720-724) to format this output. This is expected to render as plain,
uncolored text because `colored`'s tty-auto-detection sees the ACP agent's
stdout as a non-terminal JSON-RPC pipe rather than an interactive terminal, but
this assumption is not overridden or asserted anywhere in the codebase today.
Verify it explicitly per 3.3's testing requirement, and apply the same
verification in Phases 4, 6, and 7, which route through the same `execute()`
path.

#### 3.3 Testing Requirements

Unit tests: `/help` with no argument returns general help text; `/help mode`
returns command-specific help; `/status` output contains the session's current
chat mode, safety mode, model name, and _actual_ registered tool count; `/tools`
output lists only tools present in `executor.tool_registry()`; the strings
returned by `/help`, `/status`, and `/tools` contain no raw ANSI escape
sequences (`\x1b[`), confirming the 3.2 color assumption in practice rather than
by inspection alone.

#### 3.4 Deliverables

- [x] Ephemeral `ChatModeState` adapter implemented and unit-tested.
- [x] `/help`, `/status`, `/tools` produce output identical in content to
      `atoma chat`'s equivalents, reflecting live registry state.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 3.5 Success Criteria

`/help`, `/status`, and `/tools` typed in Zed return correctly formatted,
accurate text without invoking the LLM or any tool.

### Phase 4: `/safety` Text Command Parity

#### 4.1 Reuse the Phase 1 Terminal-Mode Path

`SetSessionModeRequest`'s `Terminal` branch was not touched in Phase 1 (only
`Thinking` was extracted). Extract it now into a shared helper, e.g.
`apply_terminal_mode_change(session_id, new_mode, sessions, cx)`, covering the
`SessionModeChange::Terminal(new_mode)` match arm (L2104-2125 as of the
`agent-client-protocol` 1.2.0 upgrade; re-verify before starting this phase)
plus the `CurrentModeUpdate` notification shared by both branches (L2148-2153),
callable from both the existing `SetSessionModeRequest` handler and the
dispatcher.

#### 4.2 Wire `SpecialCommand::Safety`

`Safety { new_mode: None }` returns the current safety mode and its behavior
description without mutating state. `Safety { new_mode: Some(mode) }` calls
`mode.to_execution_mode()`, invokes the 4.1 helper, and returns a confirmation
string. The typed command and the native mode-picker widget now converge on one
code path.

#### 4.3 Testing Requirements

Unit tests: bare `/safety` returns current mode without mutating
`terminal_mode`; `/safety restricted` mutates `terminal_mode` and emits exactly
one `CurrentModeUpdate` with `mode_id == "restricted"`; existing
`SetSessionModeRequest` terminal-mode tests still pass after extraction; the
string returned by both bare and argument forms of `/safety` contains no raw
ANSI escape sequences (`\x1b[`), per the Phase 3.2 color note.

#### 4.4 Deliverables

- [x] `apply_terminal_mode_change` helper extracted and used by both call sites.
- [x] `/safety` (bare and with argument) works identically to `atoma chat`.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 4.5 Success Criteria

Typing `/safety restricted` in Zed changes enforcement immediately and updates
the native mode-picker widget label via the same notification path the widget
already relies on.

### Phase 5: `/system` Text Command

#### 5.1 Wire `SpecialCommand::SystemPrompt`

`Show` returns `session_state.system_prompt_prefix` or "(none)". `Clear` sets it
to `None`. `Set(text)` sets it to `Some(text)` (existing 4096-char validation
already happens in `parse()`). Both mutating branches recompute the merged
system prompt as `build_system_prompt(prefix.as_deref(), &base)`, where `base`
is the same
`AGENT_ACP_BASE_PROMPT + PLAN_ANCHOR_INSTRUCTION [+ Phase 6 mode guidance]`
composition used at session creation, and call `exec.set_system_prompt(merged)`
immediately.

#### 5.2 Testing Requirements

Unit tests: `/system` with no prefix set reports "(none)"; `/system <text>` sets
the prefix and the executor's merged system prompt contains both the new prefix
and `AGENT_ACP_BASE_PROMPT` (regression guard); `/system clear` removes the
prefix and the merged prompt no longer contains the old prefix text but still
contains the base prompt.

#### 5.3 Deliverables

- [x] `/system` (show/clear/set) mutates the per-session prefix and re-applies
      the merged system prompt.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 5.4 Success Criteria

`/system <text>` typed in Zed changes the model's behavior on the next turn
without disturbing the Plan/checklist or thinking-level features.

### Phase 6: `/mode` Text Command with Real Tool-Registry Enforcement

#### 6.1 Audit and Correct `ChatMode::available_tools()`

Update [src/chat_mode/mod.rs](../../src/chat_mode/mod.rs) to list every
currently-registered built-in tool, classified by mode tier: read-only tools
(`read_file`, `list_directory`, `grep`, `find_path`, `get_current_datetime`) in
all three modes; code-editing tools (`write_file`, `edit_file`,
`create_directory`) added for `Code` and `Full`; destructive/execution tools
(`delete_path`, `move_path`, `copy_path`, `execute_command`, `fetch`) reserved
for `Full` only. Note two corrections to the stale list identified in "Existing
Infrastructure": the real tool name registered by
[src/tools/fetch.rs](../../src/tools/fetch.rs) (`FetchTool::name()`) is
`"fetch"`, not `"fetch_url"` — using the wrong name here would cause
`ToolRegistry::restricted_to` (6.2) to silently drop the fetch tool from every
mode, including `Full`; and `get_current_datetime`
([src/tools/get_current_datetime.rs](../../src/tools/get_current_datetime.rs),
registered by `ToolRegistryBuilder::build_for_planning()` and therefore present
in every ACP session) must be added to the read-only tier for all three modes —
it is currently omitted from `available_tools()` entirely, and without this
addition it would also be dropped from every mode once 6.2/6.3 land, breaking
the "`Full` mode is unaffected" invariant in 6.3. Update
[src/chat_mode/prompts.rs](../../src/chat_mode/prompts.rs)'s `get_tool_guidance`
text (fix the same `fetch_url` to `fetch` naming there) and existing tests
(`test_full_mode_has_all_tools`, etc. in `src/chat_mode/mod.rs` L603-742) to
match. `subagent` and any MCP-forwarded tool (`server__tool` or `mcp_*` names)
are deliberately excluded from this list — they are controlled independently (by
`/subagents` and MCP server config, respectively), not by `/mode`.

#### 6.2 Add `ToolRegistry::restricted_to`

Add `pub fn restricted_to(&self, allowed_names: &[&str]) -> Self` to
[src/agent/registry.rs](../../src/agent/registry.rs), built the same way as
`clone_without` (L201): keep a tool if its name is in `allowed_names`, **or**
its name matches the MCP exemption pattern (`contains("__")` or
`starts_with("mcp_")`), **or** it is `"subagent"` (subagent inclusion is
re-applied afterward via `with_subagents_enabled`, so it is not lost here).

#### 6.3 Recompute the Active Registry from the Base Registry

Add a helper (e.g.
`fn recompute_active_registry(session: &SessionState) -> ToolRegistry`) that
always starts from `session.base_tool_registry` and applies
`restricted_to(session.chat_mode.available_tools())` then
`with_subagents_enabled(session.subagents_enabled)`, so repeated `/mode` and/or
`/subagents` changes never compound irreversibly. Call this whenever either
field changes, and call `executor.update_tool_registry(...)` with the result,
followed by a fresh `AvailableCommandsUpdate` (Phase 2.4) reflecting the new
active tool set. Apply the same computation once at `NewSessionRequest`/
`LoadSessionRequest` time using the session's initial (default) `chat_mode`, so
a session that never touches `/mode` is unaffected (equivalent to today's
unrestricted behavior, since `ChatMode::Full` includes every built-in tool).

#### 6.4 ACP-Safe Mode Guidance for the System Prompt

Add `fn acp_mode_guidance(mode: ChatMode) -> &'static str` returning a short
one- or two-line advisory blurb per mode (distinct from, and much shorter than,
`get_system_prompt()`, which must **not** be used directly here since it would
replace `AGENT_ACP_BASE_PROMPT`/`PLAN_ANCHOR_INSTRUCTION` and break the Zed
Plan/checklist feature). Compose `base` as
`AGENT_ACP_BASE_PROMPT + PLAN_ANCHOR_INSTRUCTION + acp_mode_guidance(chat_mode)`
at session creation and recompute it (combined with the Phase 5
`system_prompt_prefix` via `build_system_prompt`) whenever `/mode` changes.

#### 6.5 Wire `SpecialCommand::Mode`

`Mode { new_mode: None }` returns the current chat mode and its _actual_ active
tool list (from the live registry, not the static `available_tools()`), without
mutating state. `Mode { new_mode: Some(mode) }` updates
`session_state.chat_mode`, calls 6.3's recompute, reapplies the system prompt
per 6.4, and returns a confirmation string.

#### 6.6 Testing Requirements

Unit tests: `restricted_to` keeps allow-listed tools, an MCP-style
double-underscore tool, and `subagent`, while dropping everything else;
`/mode readonly` on a session with `write_file`/`execute_command`/`subagent`
registered leaves only the read-only tools (plus `subagent` if
`subagents_enabled`) in `executor.tool_registry()` afterward; a subsequent
`/mode full` restores the full built-in set — including `fetch` and
`get_current_datetime`, proving the 6.1 name/coverage fixes took effect —
without needing to reconnect MCP servers (proves recomputation is from
`base_tool_registry`, not the previously-filtered registry); `ChatMode::Full`,
`Code`, and `ReadOnly` all include `get_current_datetime` (regression guard for
the omission identified in 6.1); the merged system prompt after `/mode readonly`
still contains `AGENT_ACP_BASE_PROMPT` and `PLAN_ANCHOR_INSTRUCTION` verbatim
(regression guard); invalid mode names return the existing parse error text; the
string returned by `Mode { new_mode: None }` and `Mode { new_mode: Some(_) }`
contains no raw ANSI escape sequences (`\x1b[`), per the Phase 3.2 color note.

#### 6.7 Deliverables

- [x] `ChatMode::available_tools()` audited and corrected for the full tool set;
      guidance text and existing tests updated.
- [x] `ToolRegistry::restricted_to` implemented with MCP/subagent exemptions.
- [x] `SessionState.base_tool_registry` captured at session creation; active
      registry always recomputed from it.
- [x] `/mode` (bare and with argument) actually restricts tool availability, not
      just the system-prompt text.
- [x] Plan/checklist regression test passes.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 6.8 Success Criteria

`/mode readonly` typed in Zed makes write/execute tools genuinely unavailable to
the model for subsequent turns (verified via `executor.tool_registry()`, not
just prompt text), `/mode full` cleanly restores them, and MCP/subagent tools
are unaffected by mode changes.

### Phase 7: `/subagents` and `/stream` Toggles

#### 7.1 Wire `SpecialCommand::Subagents`

Mirror `src/commands/chat.rs` L412-460: on `Some(true)`/`Some(false)`, update
`session_state.subagents_enabled` and call the Phase 6.3 recompute (so the mode
restriction and subagent toggle always compose correctly), then re-send
`AvailableCommandsUpdate`. `None` reports current state only.

#### 7.2 Wire `SpecialCommand::Stream`

On `Some(true)`/`Some(false)`, update `session_state.stream_enabled` only. The
`PromptRequest` handler reads this flag immediately before installing the stream
notifier (the "Install streaming notifier" block, L1811-1842 as of the
`agent-client-protocol` 1.2.0 upgrade; re-verify before starting this phase);
when `false`, skip installation so the existing non-streaming fallback
(`if exec.stream_notifier().is_none()`, L1909) emits one final
`AgentMessageChunk`. `None` reports current state only.

#### 7.3 Testing Requirements

Unit tests: `/subagents on` adds `subagent` to `executor.tool_registry()` and to
the next `AvailableCommandsUpdate`; `/subagents off` removes it even when
`chat_mode` is `Full`; bare `/subagents` does not mutate state; `/stream off`
causes the next turn to skip stream-notifier installation (mock provider
captures whether it was installed); bare `/stream` reports current state; the
strings returned by `/subagents` and `/stream` (bare and with argument) contain
no raw ANSI escape sequences (`\x1b[`), per the Phase 3.2 color note.

#### 7.4 Deliverables

- [x] `/subagents on|off` and bare `/subagents` behave identically to
      `atoma chat`, composing correctly with `/mode`.
- [x] `/stream on|off` and bare `/stream` control per-turn notifier
      installation.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 7.5 Success Criteria

Enabling `/subagents` in Zed makes the `subagent` tool appear in the next turn
and in the `/` autocomplete list regardless of the current `/mode`;
`/stream off` causes replies to arrive as one final chunk instead of
incrementally.

### Phase 8: `/model` and `/models` Text Commands

#### 8.1 Wire `ListModels`, `ModelInfo`, `CurrentModel`, `SwitchModel`

Reuse `executor.list_models()`, `executor.get_model_info()`,
`executor.get_current_model_info()`, and the Phase 1.3 `apply_model_switch`
helper. Format output as plain text (no ANSI color codes — Zed renders markdown,
not a terminal). After a text-driven switch, also refresh the Phase 1.2
`model_config_option` via a `ConfigOptionUpdate` notification so the native
dropdown and the typed command stay in sync regardless of which one the user
used.

#### 8.2 Testing Requirements

Unit tests: `/models` lists all models from a `MockProvider`; `/model <name>`
switches the provider, updates `session_state.vision_capable`, and emits a
`ConfigOptionUpdate` reflecting the new current model; `/model` with no argument
shows current model info; invalid model name returns the existing "not found"
tip text.

#### 8.3 Deliverables

- [x] `/models`, `/model`, `/model info <name>` work in Zed with plain-text
      formatting.
- [x] Text-driven model switches refresh the native model dropdown via
      `ConfigOptionUpdate`.
- [x] `docs/explanation/implementations.md` updated with this phase's changes
      (Rule 8).

#### 8.4 Success Criteria

`/model <name>` typed in Zed switches the model and the native dropdown reflects
the change without needing to reopen the session.

### Phase 9: Documentation and Quality Gate

Per-phase `docs/explanation/implementations.md` entries (Rule 8) are added
incrementally as a required deliverable of Phases 1-8; this phase adds the
remaining project-level, end-to-end documentation that only makes sense once
every command is wired.

#### 9.1 Update Documentation

Update `docs/how-to/zed_acp_demo.md` (new "Chat Commands in Zed" section listing
all nine supported command families — `/help`, `/status`, `/tools`,
`/subagents`, `/stream`, `/system`, `/mode`, `/safety`, `/model`/`/models` —
noting `/context` is handled by Zed's own UI and that `/exit`/`/quit` is
explicitly not supported (see Phase 2.3); update the Thinking/model-selector
verification steps in the "Zed UI Features" comment block referenced from
`demo/zed/config.yaml`) and `docs/reference/architecture.md` "Extension Points".
Confirm every phase's `docs/explanation/implementations.md` entry from Phases
1-8 is present and complete; add a final entry for this phase's own
documentation-only changes.

#### 9.2 Full Quality Gate

Run `cargo fmt --all --quiet`,
`cargo check --all-targets --all-features --quiet`,
`cargo clippy --all-targets --all-features --quiet -- -D warnings`,
`cargo nextest run --all-targets --all-features`, `cargo test --doc --quiet`.

#### 9.3 Deliverables

- [x] `docs/how-to/zed_acp_demo.md` and `docs/reference/architecture.md`
      updated.
- [x] All eight prior phases' `docs/explanation/implementations.md` entries
      confirmed present; this phase's own entry added.
- [x] Full quality gate passes with no new warnings.

#### 9.4 Success Criteria

All nine chat-command families (`/help`, `/status`, `/tools`, `/subagents`,
`/stream`, `/system`, `/mode`, `/safety`, `/model`/`/models`) work end-to-end in
Zed without reaching the LLM; `/exit`/`/quit` returns the fixed not-supported
message; the native model and Thinking dropdowns both work; `/mode` technically
restricts tool availability; `/context` is untouched;
`cargo nextest run --all-features` passes with no new clippy warnings.

## Related Documentation

- [docs/how-to/zed_acp_demo.md](../how-to/zed_acp_demo.md)
- [docs/explanation/chat_command_unification_plan.md](chat_command_unification_plan.md)
  (equivalent work already completed for `atoma chat`)
- [docs/reference/architecture.md](../reference/architecture.md)
