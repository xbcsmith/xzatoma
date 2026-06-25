# ACP Zed Agent Features Implementation Plan

## Overview

Two UI features are missing from the `atoma agent` Zed ACP stdio mode: the
terminal-mode selector widget does not appear, leaving the session locked to the
server-default (`restricted`); and the context-window usage bar shows no data,
requiring users to run `/context` manually to see token counts. This plan
diagnoses the mode-selector root cause, adds the missing `CurrentModeUpdate`
confirmation notification, enables the `unstable_session_usage` feature flag,
and emits `UsageUpdate` notifications after every completed turn.

## Current State Analysis

### Existing Infrastructure

- `src/commands/agent.rs` — `handle_agent` entry point for the Zed ACP stdio
  server.
- `SessionState` (`L117`) stores `terminal_mode: TerminalMode` per session and
  `executor: Arc<Mutex<AgentExecutor>>`.
- `NewSessionRequest` handler (`L923`) builds a `SessionModeState` from
  `all_session_modes()` and attaches it via `NewSessionResponse::modes()`.
  **`NewSessionRequest` must always create a fresh `AgentExecutor` with no prior
  conversation history.** Session resume belongs exclusively in
  `LoadSessionRequest`. Do not add `load_zed_session` or `load_conversation`
  calls to `NewSessionRequest`; doing so causes every "New Atoma Agent" click in
  Zed to silently inherit the previous session's context.
- `LoadSessionRequest` handler (`L1873`) repeats the same mode-state pattern for
  `LoadSessionResponse::modes()` and additionally calls `load_zed_session` /
  `load_conversation` to restore any prior conversation. This is the **only**
  handler that should resume a prior conversation.
- `SetSessionModeRequest` handler (`L1690`) updates
  `session_state.terminal_mode` and `executor.set_thinking_mode()` but sends
  **no** `SessionNotification` after the change.
- `PromptRequest` handler (`L1380`) applies `exec.apply_terminal_mode()` on each
  turn and drives the iteration loop, but sends **no** `UsageUpdate` after
  completion.
- `agent-client-protocol` version `0.11.1` with feature `unstable_session_model`
  in `Cargo.toml`; the `unstable_session_usage` feature is **not** enabled.
- `SessionUpdate::UsageUpdate` and `PromptResponse::usage` are compiled away
  because `unstable_session_usage` is absent.
- `AgentExecutor::get_context_info()` exists and returns a `ContextInfo` struct
  with `used_tokens`, `max_tokens`, and `percentage_used` fields.

### Identified Issues

- **Mode Selector not appearing**: `NewSessionResponse.modes` is populated by
  the code, but the `SetSessionModeRequest` handler sends no `CurrentModeUpdate`
  notification after applying the change. Zed's mode selector widget tracks the
  active mode via these push notifications; without them the widget's internal
  state drifts and Zed may suppress the selector entirely after the first
  mode-change attempt fails silently.
- **Diagnostic gap**: there is no TRACE-level log of the serialised
  `NewSessionResponse` or `LoadSessionResponse` JSON, making it impossible to
  confirm what Zed actually receives over the wire.
- **Context window info absent**: `unstable_session_usage` is not enabled, so
  the `UsageUpdate` variant does not exist at compile time and no context window
  data is ever sent to Zed.
- **`PromptResponse::usage` not populated**: even if the feature were enabled,
  no token-usage data is attached to the response.

## Implementation Phases

### Phase 1: Diagnose and Fix Mode Selector

#### 1.1 Add wire-format diagnostic logging in `src/commands/agent.rs`

In the `NewSessionRequest` handler, immediately before
`responder.respond(response)`, add a `TRACE`-level log that serialises the full
`NewSessionResponse` to JSON and emits it with the session ID as a structured
field:

```text
trace!(session_id = %session_id, response_json = %json, "NewSessionResponse wire format");
```

Do the same in the `LoadSessionRequest` handler before its
`responder.respond(response)`. These logs let an operator capture the exact
bytes Zed receives by running `atoma agent` with `--trace` and inspecting
stderr.

#### 1.2 Emit `CurrentModeUpdate` after `SetSessionModeRequest`

The `SetSessionModeRequest` handler (`L1690-1754`) applies a terminal or
thinking mode change but never notifies Zed of the new state. Zed's mode
selector requires a `CurrentModeUpdate` notification to stay in sync.

After step 3 of the handler (the `match change { ... }` block at L1726), send:

```rust
let _ = _cx.send_notification(SessionNotification::new(
    req.session_id.clone(),
    SessionUpdate::CurrentModeUpdate(CurrentModeUpdate::new(req.mode_id.clone())),
));
```

The `_cx` context is available in the handler closure via the third handler
argument; import `CurrentModeUpdate` and `SessionUpdate` from
`agent_client_protocol::schema`.

Update the handler closure signature to accept `_cx` (currently unused):

```rust
async move |req: SetSessionModeRequest, responder, _cx| {
```

#### 1.3 Testing Requirements

- Unit test: `SetSessionModeRequest` with a valid terminal mode ID emits a
  `CurrentModeUpdate` notification whose `current_mode_id` matches the requested
  mode.
- Unit test: `SetSessionModeRequest` with a valid thinking mode ID also emits
  `CurrentModeUpdate`.
- Unit test: TRACE log output for `NewSessionResponse` contains the `modes` JSON
  key when modes are populated.

#### 1.4 Deliverables

- [x] TRACE-level `NewSessionResponse` and `LoadSessionResponse` JSON logs.
- [x] `CurrentModeUpdate` notification emitted after every successful
      `SetSessionModeRequest`.
- [x] All Phase 1 tests pass.

#### 1.5 Success Criteria

Running `atoma agent --trace 2>trace.log` and opening the Zed project shows the
mode selector widget. Changing mode in Zed produces a `CurrentModeUpdate` line
in the trace log. `cargo test` passes.

---

### Phase 2: Enable Usage Tracking

#### 2.1 Enable `unstable_session_usage` in `Cargo.toml`

Use `cargo add` to add the feature flag to the existing dependency (do NOT
manually edit version numbers):

```bash
cargo add agent-client-protocol --features unstable_session_usage
```

Verify the resulting `Cargo.toml` entry reads:

```text
agent-client-protocol = { version = "0.11.1", features = ["unstable_session_model", "unstable_session_usage"] }
```

#### 2.2 Emit `UsageUpdate` at the end of each `PromptRequest` turn

In the `PromptRequest` handler (`src/commands/agent.rs` L1380), after the
`'iteration` loop breaks and before `responder.respond(...)`, call
`exec.get_context_info().await` to obtain the current token counts and send a
`UsageUpdate` notification:

```rust
if let Ok(ctx) = exec.get_context_info().await {
    let _ = cx_clone.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::UsageUpdate(
            agent_client_protocol::schema::UsageUpdate::new(
                ctx.used_tokens as u64,
                ctx.max_tokens as u64,
            )
        ),
    ));
}
```

Import `UsageUpdate` from `agent_client_protocol::schema` (gated on the
`unstable_session_usage` feature using `#[cfg(feature = "...")]` if needed to
keep compilation clean when the feature is absent).

#### 2.3 Populate `PromptResponse::usage` (token-per-turn data)

The `PromptResponse` carries an optional `usage` field under
`unstable_session_usage`. After the iteration loop, build a
`agent_client_protocol::schema::Usage` value from the executor's `total_usage`
field (available via `exec.session_token_usage()`) and attach it to the
response:

```rust
#[cfg(feature = "unstable_session_usage")]
let usage = agent_client_protocol::schema::Usage::new(
    exec.total_usage().total_tokens as u64,
    exec.total_usage().prompt_tokens as u64,
    exec.total_usage().completion_tokens as u64,
);

let response = {
    let base = PromptResponse::new(stop_reason);
    #[cfg(feature = "unstable_session_usage")]
    let base = base.usage(usage);
    base
};
```

Add a public `total_usage()` accessor to `AgentExecutor` in
`src/agent/executor.rs` if one does not already exist, following the pattern of
the existing `thinking_mode()` accessor.

#### 2.4 Testing Requirements

- Unit test: `unstable_session_usage` feature is enabled; `UsageUpdate::new`
  compiles and has `used` and `size` fields.
- Unit test: after the `PromptRequest` iteration loop, a `UsageUpdate`
  notification with non-zero `size` is sent.
- Unit test: `PromptResponse::usage` is populated with the session's cumulative
  token counts when the feature is enabled.
- Unit test: `AgentExecutor::total_usage()` returns a `TokenUsage` with all
  three fields.

#### 2.5 Deliverables

- [x] `Cargo.toml` feature list updated via `cargo add`.
- [x] `UsageUpdate` notification emitted after each `PromptRequest` turn.
- [x] `PromptResponse::usage` populated with cumulative session token counts.
- [x] `AgentExecutor::total_usage()` accessor (if missing).
- [x] All Phase 2 tests pass.

#### 2.6 Success Criteria

The Zed context-window bar shows a non-zero token count after the first prompt
turn. `cargo check --all-features` passes. `cargo test` passes.

---

### Phase 3: Integration Hardening

#### 3.1 Send initial `UsageUpdate` after session creation

Send an initial `UsageUpdate` at the end of both the `NewSessionRequest` and
`LoadSessionRequest` handlers, immediately after the session state is inserted
into the map. Use `executor.conversation().token_count()` as `used` and
`max_conversation_tokens` from config as `size`.

`NewSessionRequest` always creates a **fresh** conversation, so its initial
`UsageUpdate` always has `used = 0`. `LoadSessionRequest` resumes a prior
conversation, so its `used` reflects the loaded token count.

**Do NOT add `load_zed_session` or `load_conversation` logic to
`NewSessionRequest`** to make `used` non-zero. That would re-introduce the bug
where clicking "New Atoma Agent" silently resumes the previous session.

#### 3.2 Send `ConfigOptionUpdate` after thinking mode change

When `SetSessionModeRequest` triggers a `SessionModeChange::Thinking` change,
the Thinking dropdown in Zed's UI should reflect the new selection. After
applying the thinking mode, send a `ConfigOptionUpdate` notification containing
the refreshed `thought_level_config_option(new_mode)`:

```rust
let _ = _cx.send_notification(SessionNotification::new(
    req.session_id.clone(),
    SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(vec![
        thought_level_config_option(new_mode),
    ])),
));
```

This mirrors how the mode selector is refreshed and ensures the two Zed
dropdowns stay in sync after any mode change.

#### 3.3 Testing Requirements

- Unit test: a fresh `NewSessionRequest` sends an initial `UsageUpdate` whose
  `used` value equals `executor.conversation().token_count()` (zero for a new
  executor, because `NewSessionRequest` never loads prior history).
- Unit test: a thinking-mode change via `SetSessionModeRequest` sends both a
  `CurrentModeUpdate` and a `ConfigOptionUpdate`.
- Unit test: the `ConfigOptionUpdate` contains a `thought_level` option whose
  `current_value` matches the newly applied thinking mode.

#### 3.4 Deliverables

- [x] Initial `UsageUpdate` on session create (`NewSessionRequest`, always
      `used = 0`, fresh session) and on session load (`LoadSessionRequest`,
      `used` = loaded token count).
- [x] `ConfigOptionUpdate` notification after thinking mode change.
- [x] All Phase 3 tests pass.

#### 3.5 Success Criteria

Opening an existing Zed project with a prior conversation immediately shows
context usage. Changing the Thinking dropdown updates both the dropdown and the
mode selector coherently. `cargo test` passes.

---

### Phase 4: Documentation

#### 4.1 Update `docs/explanation/implementations.md`

Record all changed files, summarise the two features added, and note which tests
were added.

#### 4.2 Update `demo/zed/config.yaml`

Add a comment block explaining the two new UI features and how to verify them
are working (run with `--trace`, look for `UsageUpdate` and `CurrentModeUpdate`
lines in stderr).

#### 4.3 Deliverables

- [x] `docs/explanation/implementations.md` updated.
- [x] `demo/zed/config.yaml` comment updated.

#### 4.4 Success Criteria

All documentation accurately reflects the new feature behaviour. `markdownlint`
and `prettier` pass on all changed markdown files.

---

### Phase 5: Session Metadata and Tool Command Discovery

This phase closes the three `SessionUpdate` variants that Zed's
`handle_session_update` processes but Atoma never sends: `SessionInfoUpdate`
(conversation title), `AvailableCommandsUpdate` (registered tool list), and
`Plan` (task-plan checklist). The first two are straightforward additions; plan
tracking requires a significant architectural change and is deferred with a
design note.

#### 5.1 Share title derivation between `chat.rs` and `agent.rs`

`chat.rs` contains a private `derive_conversation_title` function that truncates
the first user message to form a short conversation title. The same logic is
needed in `agent.rs`. To avoid duplication, move `derive_conversation_title`
from `src/commands/chat.rs` to `src/agent/conversation.rs` as a free
`pub(crate)` function with the same truncation logic. Update `chat.rs` to call
the shared version.

The existing guard in `chat.rs` is the correct pattern to replicate:

```rust
if conv.title() == "New Conversation" && conv.message_count() >= 1 {
    let new_title = derive_conversation_title(conv);
    executor.conversation_mut().set_title(new_title);
}
```

Add a `conversation_mut()` accessor to `AgentExecutor` in
`src/agent/executor.rs` if one does not already exist, mirroring the existing
`conversation()` accessor.

#### 5.2 Send `SessionInfoUpdate` after completed turns

In the `PromptRequest` handler (`src/commands/agent.rs`), after the
`save_zed_session` checkpoint block and before the `UsageUpdate`, derive and
push the title when it has been set for the first time, then send a
`SessionInfoUpdate`:

```rust
{
    let needs_title = exec.conversation().title() == "New Conversation"
        && exec.conversation().message_count() >= 1;
    if needs_title {
        let new_title = derive_conversation_title(exec.conversation());
        exec.conversation_mut().set_title(new_title);
    }
    let title = exec.conversation().title().to_string();
    if title != "New Conversation" {
        let _ = cx_clone.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::SessionInfoUpdate(
                SessionInfoUpdate::new().title(title),
            ),
        ));
    }
}
```

Also send `SessionInfoUpdate` in the `LoadSessionRequest` handler immediately
after loading the conversation, but only when the saved title is not
`"New Conversation"` — genuinely untitled resumed sessions must not push a
misleading default string to Zed.

Add `SessionInfoUpdate` to the existing
`use agent_client_protocol::schema::{ ... }` import block in `agent.rs`.

#### 5.3 Send `AvailableCommandsUpdate` on session start

After the tool registry is fully populated in both `NewSessionRequest` and
`LoadSessionRequest` (all built-in, MCP, resource, and prompt tools registered),
build an `AvailableCommandsUpdate` and send it so Zed's slash-command completion
menu reflects the session's actual toolset.

Add a `tools()` method to `ToolRegistry` in `src/agent/registry.rs`:

```rust
pub fn tools(&self) -> Vec<Arc<dyn Tool>> {
    self.tools.values().cloned().collect()
}
```

Then, in both session handlers after the registry build step:

```rust
let commands: Vec<_> = tool_registry
    .tools()
    .iter()
    .map(|t| AvailableCommand::new(
        t.name().to_string(),
        t.description().to_string(),
    ))
    .collect();
if !commands.is_empty() {
    let _ = _cx.send_notification(SessionNotification::new(
        session_id.clone(),
        SessionUpdate::AvailableCommandsUpdate(
            AvailableCommandsUpdate::new(commands),
        ),
    ));
}
```

Add `AvailableCommand` and `AvailableCommandsUpdate` to the schema import block
in `agent.rs`.

#### 5.4 Plan tracking (deferred — architectural note)

Zed's `SessionUpdate::Plan` renders a task-checklist UI with `PlanEntry` items
that can be `Pending`, `InProgress`, or `Completed`. Atoma's executor runs a
flat iteration loop with no concept of a structured plan. Implementing plan
tracking requires:

1. A `PlanTracker` component in `src/agent/` that parses the assistant's
   streamed output for numbered-list items or structured markers and maps them
   to `PlanEntry` records.
2. A hook in the `PromptRequest` streaming path that calls
   `plan_tracker.update(chunk)` on each `AgentMessageChunk` and emits
   `SessionUpdate::Plan(...)` when the tracked plan changes.
3. A strategy for distinguishing plan-intent text from ordinary numbered lists
   in the model's output, which may require prompt engineering or a structured
   output mode.

This work is scoped as a follow-on phase and is not implemented here. The `_`
catch-all arm in Zed's `handle_session_update` silently ignores absent `Plan`
updates, so omitting them causes no runtime error.

#### 5.5 Testing Requirements

- Unit test: after a `PromptRequest` turn where
  `exec.conversation().message_count() >= 1`, a `SessionInfoUpdate` notification
  is sent whose `title` field equals the derived title (not
  `"New Conversation"`).
- Unit test: `SessionInfoUpdate` is NOT sent when the conversation title is
  still `"New Conversation"` (no messages yet).
- Unit test: `SessionInfoUpdate` is sent during `LoadSessionRequest` when the
  resumed conversation has a non-default saved title.
- Unit test: `SessionInfoUpdate` is NOT sent during `LoadSessionRequest` when
  the loaded title is `"New Conversation"`.
- Unit test: after `NewSessionRequest`, an `AvailableCommandsUpdate`
  notification is sent containing at least one `AvailableCommand` whose `name`
  appears in `tool_registry.tool_names()`.
- Unit test: after `LoadSessionRequest`, the same `AvailableCommandsUpdate`
  pattern holds.
- Unit test: `ToolRegistry::tools()` returns all registered tools (count matches
  `tool_registry.count()`).
- Unit test: `derive_conversation_title` (shared version) truncates the first
  user message at 50 characters.

#### 5.6 Deliverables

- [x] `derive_conversation_title` moved to `src/agent/conversation.rs` as
      `pub(crate)` and `chat.rs` updated to call the shared version.
- [x] `AgentExecutor::conversation_mut()` accessor added if absent.
- [x] `ToolRegistry::tools()` method added to `src/agent/registry.rs`.
- [x] `SessionInfoUpdate` emitted after each `PromptRequest` turn when the title
      is first derived from conversation content.
- [x] `SessionInfoUpdate` emitted in `LoadSessionRequest` for non-default saved
      titles.
- [x] `AvailableCommandsUpdate` emitted in both `NewSessionRequest` and
      `LoadSessionRequest` after the tool registry is fully populated.
- [x] `SessionInfoUpdate`, `AvailableCommand`, and `AvailableCommandsUpdate`
      added to the schema import block in `agent.rs`.
- [x] All Phase 5 tests pass.

#### 5.7 Success Criteria

The Zed thread tab shows a meaningful title derived from the first user message
after the first prompt turn. The Zed slash-command completion menu lists the
tools registered for the session. `cargo test` passes.
`cargo check --all-features` passes. `markdownlint` and `prettier` pass on all
changed documentation files.

---

### Phase 6: Plan Tracking

Zed's `SessionUpdate::Plan` renders a task-checklist UI with `PlanEntry` items
that can be `Pending`, `InProgress`, or `Completed`. This phase implements the
plan tracking component deferred in Phase 5 section 5.4.

#### 6.1 Add `PlanTracker` to `src/agent/plan_tracker.rs`

Create a `PlanTracker` struct that accumulates streamed assistant output and
extracts numbered-list items as `PlanEntry` records.

```rust
use acp_core::schema::{PlanEntry, PlanEntryStatus};

pub struct PlanTracker {
    entries: Vec<PlanEntry>,
    buffer: String,
}

impl PlanTracker {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            buffer: String::new(),
        }
    }

    /// Feed a chunk of streamed assistant text and return true when the plan
    /// changes (new entries added or status updated).
    pub fn update(&mut self, chunk: &str) -> bool { ... }

    /// Return the current snapshot of plan entries.
    pub fn entries(&self) -> &[PlanEntry] { &self.entries }

    /// Reset all entries to Pending at the start of a new turn.
    pub fn reset_to_pending(&mut self) { ... }
}
```

The parser looks for lines that match the pattern `^\d+\.\s+.+` (a numbered list
item) in the accumulated buffer. Each distinct line seed produces one
`PlanEntry`. Status transitions follow this rule:

- Entry is `Pending` when first detected.
- Entry transitions to `InProgress` when streaming moves past it to a later
  entry.
- Entry transitions to `Completed` at the end of the turn (stream closed) when
  the tool-call result for the associated step is observed, or when all entries
  have been seen and the turn is done.

To avoid false positives from ordinary numbered lists in tool output or
explanatory text, the tracker only promotes list items that appear in the first
assistant message of a turn (before any `tool_use` block). Items that appear
after a `tool_use` or `tool_result` boundary are ignored.

#### 6.2 Integrate `PlanTracker` into the streaming path in `src/commands/agent.rs`

Instantiate one `PlanTracker` per `PromptRequest` turn. Hook it into the
streaming loop that iterates over `AgentMessageChunk` values:

```rust
let mut plan_tracker = PlanTracker::new();

for chunk in stream {
    // existing chunk handling ...
    if let AgentMessageChunk::Text(text) = &chunk {
        if plan_tracker.update(text) {
            let entries = plan_tracker.entries().to_vec();
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::Plan(entries),
            ));
        }
    }
}
```

After the stream ends, perform a final status sweep: any entry that is still
`InProgress` is marked `Completed`, and a final `SessionUpdate::Plan` is emitted
so the Zed checklist reflects completion.

#### 6.3 Prompt engineering to anchor plan output

Add a brief instruction to the system prompt (in `src/agent/executor.rs` or
wherever the system prompt is assembled) that encourages the model to emit its
step-by-step plan as a leading numbered list before taking actions:

```text
When you have a multi-step task, begin your response with a numbered list of
the steps you will take (e.g., "1. Read the file\n2. Edit the function\n3.
Run tests"). Proceed with execution immediately after the list.
```

This increases the probability that numbered-list items at the top of the
response represent genuine plan intent. The tracker's boundary rule (section
6.1) prevents tool-output lists from being misidentified as plan entries.

#### 6.4 Testing Requirements

- Unit test: `PlanTracker::update` detects numbered-list items in a streamed
  chunk and returns `true` only when new entries appear.
- Unit test: items that arrive after a simulated `tool_use` boundary are NOT
  added to the plan.
- Unit test: after a full turn, all `InProgress` entries are promoted to
  `Completed`.
- Unit test: `PlanTracker::reset_to_pending` resets all entry statuses to
  `Pending` without clearing the entry list.
- Integration test: a `PromptRequest` whose assistant response starts with a
  two-item numbered list results in exactly two `SessionUpdate::Plan`
  notifications and a final notification where both entries are `Completed`.
- Integration test: a `PromptRequest` whose response contains no numbered list
  at the top emits no `SessionUpdate::Plan` notifications.

#### 6.5 Deliverables

- [x] `src/agent/plan_tracker.rs` created with `PlanTracker` struct and unit
      tests.
- [x] `PlanTracker` integrated into the `PromptRequest` streaming loop in
      `src/commands/agent.rs`.
- [x] System-prompt anchor text added to encourage leading numbered-list plans.
- [x] `Plan` variant added to the schema import block in `agent.rs` if not
      already present.
- [x] `docs/explanation/implementations.md` updated.
- [x] All Phase 6 tests pass.

#### 6.6 Success Criteria

When Atoma executes a multi-step task, the Zed thread panel displays a live
checklist that updates as each step begins and completes. `cargo test` passes.
`cargo check --all-features` passes. `markdownlint` and `prettier` pass on all
changed documentation files.
