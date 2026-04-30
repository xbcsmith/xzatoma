# Zed Session Mode Selector Implementation Plan

## Overview

This plan adds a mode selector widget to Atoma's Zed IDE chat window that maps
to Atoma's existing `TerminalMode` security levels (Interactive, Restricted,
Full Autonomous). When a user switches modes in Zed's UI, a `session/set_mode`
JSON-RPC request is sent to Atoma over stdio. Atoma will handle this by storing
the mode per-session and threading it into the tool registry at prompt time.
This removes the need to hard-code `terminal_mode: restricted` in the config
file for Zed users.

The plan also addresses three additional gaps discovered when auditing Atoma
against Zed's ACP agent harness: (1) all non-text `ContentBlock` types in
`PromptRequest` are silently discarded, making `@mentions` of files,
diagnostics, and git diffs completely non-functional; (2) MCP servers forwarded
by Zed in `NewSessionRequest.mcp_servers` are ignored, so project-specific
context servers never reach Atoma; and (3) tool calls stream as raw text
instead of structured UI cards, producing noisy output in the chat window.

## Current State Analysis

### Existing Infrastructure

- `TerminalMode` enum (`src/config.rs` L74-85): three variants — `Interactive`,
  `RestrictedAutonomous` (`"restricted"`), `FullAutonomous` (`"full"`) — with
  `serde` rename attributes already matching the ACP mode ID strings Zed will
  send.
- `PermissionManager::set_mode()` (`src/security/permissions.rs` L239-241):
  method already exists to mutate mode at runtime; it is currently never called
  after startup.
- `SessionState` struct (`src/commands/agent.rs` L92-101): holds per-session
  data (executor, cancel token, conversation ULID, cwd) but has no
  `terminal_mode` field.
- `make_agent_builder` (`src/commands/agent.rs` L272-277): registers handlers
  for `InitializeRequest` (L288-307), `NewSessionRequest` (L311-490), and
  `PromptRequest` (L494-672). No `SetSessionModeRequest` handler exists.
- `NewSessionResponse` construction (`src/commands/agent.rs` L471-474):
  optionally chains `.models(state)` but never calls `.modes(...)`.
- `agent-client-protocol` v0.11.1 (Cargo.toml L12): already exports
  `SessionModeState`, `SessionMode`, `SessionModeId`, `SetSessionModeRequest`,
  and `SetSessionModeResponse` with no additional feature flag required. No
  crate upgrade is needed.

### Identified Issues

1. `NewSessionResponse.modes` is always `None` — Zed never receives a mode
   advertisement, so no `ModeSelector` widget appears in the chat window.
2. No `session/set_mode` handler is registered in `make_agent_builder` —
   Zed's RPC call is silently dropped.
3. `SessionState` has no `terminal_mode` field — mode cannot vary per session.
4. `PermissionManager::set_mode()` is unreachable at runtime from the Zed
   stdio path.
5. `register_mcp_tools` is never called in the Zed stdio agent path
   (`src/commands/agent.rs`), while every other execution path calls it —
   this means MCP tools are absent from Zed agent sessions entirely (a
   pre-existing gap that must be addressed to make mode enforcement meaningful).
6. `terminal_mode: restricted` must be set in the config file today for Zed
   users to get safe defaults, which is an undiscoverable footgun.
7. `PromptCapabilities` is advertised with all flags `false` (the
   `PromptCapabilities::new()` default) — `embedded_context: false` causes
   Zed to hide `#diagnostics`, `#git-diff`, `#rules`, and `#thread` from the
   `@mention` completion menu and to send all file references as `ResourceLink`
   stubs rather than inline content.
8. The `PromptRequest` handler (`src/commands/agent.rs` L503-506) matches only
   `ContentBlock::Text` and discards every other variant with `_ => None`,
   meaning `ResourceLink`, `Resource(EmbeddedResource)`, and `Image` blocks
   sent by Zed are silently dropped — `@mentions` are non-functional even when
   Zed does send them.
9. `NewSessionRequest.mcp_servers` (the `Vec<McpServer>` forwarded by Zed
   containing all project-level MCP servers) is never accessed; Atoma only
   uses its own config-file MCP servers, ignoring any project-specific context
   servers the user has configured in Zed.
10. Tool calls stream as raw `AgentMessageChunk` text (`"Tool: {name} |
{preview}"` at `src/commands/agent.rs` L595-600) instead of structured
    `SessionUpdate::ToolCall` notifications, so Zed cannot render collapsible
    tool cards with status indicators.

## Implementation Phases

---

### Phase 1: Session State and Mode Mapping Foundation

#### 1.1 Add `terminal_mode` Field to `SessionState`

In `src/commands/agent.rs`, add a `terminal_mode: TerminalMode` field to the
`SessionState` struct (L92-101). Update the `Debug` impl (L103-113) to include
the new field. The field must be `pub` to match the existing field visibility
pattern.

#### 1.2 Initialize `terminal_mode` from Global Config on Session Creation

In the `NewSessionRequest` handler (`src/commands/agent.rs` L311-490), when
constructing and inserting the new `SessionState` (L479-487), set
`terminal_mode: config_clone.terminal_mode` where `config_clone` is the
`Arc<Config>` already captured in the closure. This preserves backward
compatibility: a config file with `terminal_mode: restricted` continues to
work, and sessions default to whatever the operator configured.

#### 1.3 Add `TerminalMode` to `SessionMode` Mapping

Add a `const` or `impl` block in `src/commands/agent.rs` (near the top, after
the existing constants) that maps each `TerminalMode` variant to an ACP
`SessionModeId` and display name:

- `Interactive` → id `"interactive"`, name `"Interactive"`, description
  `"Requires approval before executing each command"`
- `RestrictedAutonomous` → id `"restricted"`, name `"Restricted"`, description
  `"Blocks dangerous commands; no approval prompts"`
- `FullAutonomous` → id `"full"`, name `"Full Autonomous"`, description
  `"Executes all commands without restriction"`

Add a `From<&str>` or `TryFrom<SessionModeId>` conversion back to
`TerminalMode` for use in the `set_mode` handler. Return an error for
unrecognized mode ID strings.

#### 1.4 Testing Requirements

- Unit test: `terminal_mode` field on `SessionState` initializes from config
  correctly for all three `TerminalMode` variants.
- Unit test: `TerminalMode` round-trips through the string ID mapping (all
  three variants → string → `TerminalMode` without error; unknown string
  returns `Err`).

#### 1.5 Deliverables

- `SessionState` has a `terminal_mode: TerminalMode` field initialized from
  global config at session creation.
- A bidirectional mapping between `TerminalMode` and ACP `SessionModeId`
  strings exists and is covered by unit tests.

#### 1.6 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo test` passes with new unit tests for the mapping and initialization.

---

### Phase 2: Mode Advertisement in `NewSessionResponse`

#### 2.1 Construct `SessionModeState` in the `session/new` Handler

In the `NewSessionRequest` handler (`src/commands/agent.rs` L462-474), before
building the response, construct a `SessionModeState` using the types already
available from `agent-client-protocol` v0.11.1:

- `available_modes`: a `Vec<SessionMode>` with all three entries from the
  Phase 1 mapping, in the order Interactive → Restricted → Full Autonomous.
- `current_mode_id`: the `SessionModeId` derived from the session's resolved
  `terminal_mode` (the value that was just stored in `SessionState`).

#### 2.2 Chain `.modes()` onto `NewSessionResponse`

Replace the existing response construction block (L471-474) with a single
builder chain that always sets both `models` (when available) and `modes`:

```text
NewSessionResponse::new(session_id.clone())
    .modes(session_mode_state)
    .models(state)   // optional, only when model_state.is_some()
```

The `.modes()` call must come before `.models()` because Zed's `config_state()`
function ignores `modes` when `config_options` is present, but modes and models
are not mutually exclusive.

#### 2.3 Testing Requirements

- Unit test: the `NewSessionResponse` produced by the handler includes a
  non-`None` `modes` field.
- Unit test: `current_mode_id` in the response matches the `TerminalMode` set
  in config for each of the three variants.
- Unit test: `available_modes` always contains exactly three entries with the
  correct ids, names, and descriptions.

#### 2.4 Deliverables

- Every `NewSessionResponse` from the Zed stdio agent includes a populated
  `modes` field.
- Zed's `ModeSelector` widget appears in the chat window for Atoma sessions.

#### 2.5 Success Criteria

- `cargo check --all-targets --all-features` passes.
- Manual verification: start `atoma agent` and connect via Zed — a mode
  selector dropdown appears in the chat window showing `Restricted` as the
  default.

---

### Phase 3: `session/set_mode` Handler

#### 3.1 Register `SetSessionModeRequest` Handler

In `make_agent_builder` (`src/commands/agent.rs`), add a fourth
`.on_receive_request()` call after the `PromptRequest` handler (after L672)
using the same `on_receive_request!()` macro pattern. The handler signature
follows the existing pattern:

```text
async move |req: SetSessionModeRequest, responder, _cx| { ... }
```

The handler must capture `Arc::clone(&sessions)` as `sessions_for_set_mode`.

#### 3.2 Implement the Handler Body

Inside the `SetSessionModeRequest` handler:

1. Parse `req.mode_id` using the `TryFrom<SessionModeId>` conversion added in
   Phase 1. Return `responder.respond_with_error(Error::invalid_params())` for
   unrecognized mode IDs.
2. Lock the sessions map and look up `req.session_id`. Return
   `responder.respond_with_error(Error::invalid_request())` if the session does
   not exist (mirrors the pattern in the `PromptRequest` handler at L533-545).
3. Update `session_state.terminal_mode` to the parsed `TerminalMode`.
4. Log the mode change at `info!` level, including `session_id` and the new
   mode string.
5. Respond with `SetSessionModeResponse::default()` (the type serializes as an
   empty JSON object `{}`).

#### 3.3 Testing Requirements

- Unit test: sending a valid `SetSessionModeRequest` updates `SessionState`
  `terminal_mode` to the new value.
- Unit test: sending an unknown `mode_id` returns a JSON-RPC error response and
  does not modify session state.
- Unit test: sending a `SetSessionModeRequest` for a non-existent `session_id`
  returns a JSON-RPC error.
- Unit test: all three valid mode IDs (`"interactive"`, `"restricted"`,
  `"full"`) are accepted and stored correctly.

#### 3.4 Deliverables

- `session/set_mode` is a registered, functional JSON-RPC handler.
- Mode changes are persisted on `SessionState` and survive across subsequent
  `session/prompt` calls within the same session.

#### 3.5 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo test` passes with new handler unit tests.
- Manual verification: switching modes in the Zed `ModeSelector` does not
  produce an error and the selected mode label persists.

---

### Phase 4: Thread Per-Session Mode into Tool Registry

#### 4.1 Fix Missing `register_mcp_tools` Call

In the `NewSessionRequest` handler in `src/commands/agent.rs`, add the
`register_mcp_tools` call when constructing the `ToolRegistry` for each session.
Every other execution path (`src/commands/chat.rs` L164, `src/commands/run.rs`
L747, `src/acp/executor.rs` L377) calls this function; the Zed stdio path is
the only one that does not. Pass the session's resolved `terminal_mode` and
`headless: true` (all Zed stdio sessions are headless — the user interacts via
the Zed UI, not Atoma's CLI prompts).

#### 4.2 Apply Session `terminal_mode` at Prompt Time

In the `PromptRequest` handler, the session's `terminal_mode` must be extracted
alongside the other session data in the `sessions.lock()` block (L511-532) and
included in the tuple returned (currently `(executor_arc, cancel_token,
conversation_ulid, cwd)`). Add `terminal_mode: TerminalMode` to this tuple.

Before calling `exec.run_iteration()` inside the prompt loop, update the
executor's tool registry's `PermissionManager` with the current session mode.
Expose a method on `AgentExecutor` (e.g., `apply_terminal_mode(mode:
TerminalMode)`) in `src/agent/executor.rs` that calls
`PermissionManager::set_mode()` on any registered tools that hold a
`PermissionManager`. Alternatively, store the session's `terminal_mode` on the
`AgentExecutor` itself and apply it at the start of each iteration.

#### 4.3 Rebuild Tool Registry on Mode Change (Optional Enhancement)

If rebuilding the `ToolRegistry` on mode change is simpler than patching
`PermissionManager` in place, the `SetSessionModeRequest` handler from Phase 3
can instead rebuild and replace the executor's tool registry. This trades
simplicity of the set-mode handler for simplicity of the prompt handler. Choose
whichever approach produces fewer unsafe patterns. Document the decision in
`docs/explanation/implementations.md`.

#### 4.4 Testing Requirements

- Unit test: after a `SetSessionModeRequest` changes mode to `"full"`, the next
  prompt execution uses `TerminalMode::FullAutonomous` in the
  `PermissionManager`.
- Unit test: after a `SetSessionModeRequest` changes mode to `"interactive"`,
  the `PermissionManager` reflects `TerminalMode::Interactive`.
- Integration test: end-to-end using `FakeTransport` — send `session/new`,
  then `session/set_mode` to `"restricted"`, then `session/prompt`; assert
  that the tool registry's permission manager is in `RestrictedAutonomous`
  mode.
- Verify `register_mcp_tools` is called during `session/new` by asserting at
  least one tool is registered in the executor's tool registry.

#### 4.5 Deliverables

- MCP tools are registered in the Zed stdio agent path (parity with all other
  execution paths).
- The `PermissionManager` inside the tool registry reflects the session's
  current `terminal_mode` at the start of each prompt iteration.
- Mode changes take effect on the very next prompt turn within the session.

#### 4.6 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo nextest run --all-features` passes with new integration test.
- Manual verification: switch to `Full Autonomous` mode in Zed and confirm
  Atoma executes a terminal command without restriction; switch back to
  `Restricted` and confirm dangerous commands are blocked.

---

### Phase 5: Rich Context Support

Atoma currently advertises `PromptCapabilities::new()` in its
`InitializeResponse`, which defaults `embedded_context`, `image`, and `audio`
all to `false`. This causes two concrete failures: Zed hides high-value mention
types from the completion menu, and `ContentBlock` variants other than `Text`
are silently dropped by the `PromptRequest` handler. This phase enables
embedded context and repairs the content block dispatch.

#### 5.1 Enable `embedded_context` and `image` in `PromptCapabilities`

In the `InitializeRequest` handler (`src/commands/agent.rs` L288-307), replace
the bare `PromptCapabilities::new()` call with:

```text
PromptCapabilities::new()
    .embedded_context(true)
    .image(true)
```

`embedded_context: true` unlocks the full Zed mention menu — `#diagnostics`,
`#git-diff`, `#rules`, `#thread`, and `#fetch` — and switches file mentions
from `ResourceLink` stubs to fully-inlined `Resource(EmbeddedResource)` blocks.
`image: true` allows pasted images to be forwarded to vision-capable providers.

The `audio` flag can remain `false`; no provider Atoma supports accepts audio
content.

#### 5.2 Expand `ContentBlock` Dispatch in `PromptRequest`

Replace the text-only `filter_map` (L503-506) with a full `match` over every
`ContentBlock` variant, assembling a combined prompt string to pass to
`exec.add_user_message()`. The conversion rules are:

- `ContentBlock::Text(t)` — include `t.text` directly (existing behavior).
- `ContentBlock::Resource(embedded)` — match on `embedded.resource`:
  - `EmbeddedResourceResource::TextResourceContents(trc)` — prepend a context
    header `\n[Context: {trc.uri}]\n` followed by `trc.text`. This is the
    primary path for `#file`, `#diagnostics`, `#selection`, `#git-diff`, and
    `#rules` when `embedded_context: true`.
  - `EmbeddedResourceResource::BlobResourceContents(_)` — emit a placeholder
    `[Binary resource: {blob.uri}]` so the model knows content was attached but
    could not be inlined.
- `ContentBlock::ResourceLink(link)` — emit `[Reference: {link.name}
({link.uri})]`. This path is still exercised for directory mentions and
  for sessions on older Zed clients that do not send embedded content.
- `ContentBlock::Image(img)` — if the session's provider is vision-capable,
  pass the image data through to the model using the existing
  `ToolResult::success_with_images` path that `read_file` already exercises.
  If not vision-capable, emit `[Image attachment — vision not available]`.
- `ContentBlock::Audio(_)` — emit `[Audio attachment — not supported]` since
  no Atoma provider accepts audio.

All content blocks from a single `PromptRequest` must be assembled into one
string before calling `exec.add_user_message()`. Preserve the existing join on
`"\n"` between blocks.

#### 5.3 Add Image-Capable Flag to `SessionState`

To decide whether to pass images to the provider at prompt time, `SessionState`
needs to know whether the current provider is vision-capable. Inspect
`executor.get_current_model()` after provider attachment in the
`NewSessionRequest` handler and set a `vision_capable: bool` flag on
`SessionState`. The `read_file` tool already has this logic — reuse the same
model-name check.

#### 5.4 Testing Requirements

- Unit test: a `PromptRequest` containing only `ContentBlock::Text` blocks
  produces the same result as before (no regression).
- Unit test: a `PromptRequest` with a `ContentBlock::Resource` wrapping
  `TextResourceContents { text: "fn main() {}", uri: "file:///src/main.rs" }`
  includes both the URI header and the text body in the assembled prompt string.
- Unit test: a `PromptRequest` with a `ContentBlock::ResourceLink` produces
  a `[Reference: ...]` placeholder (not an empty string or a panic).
- Unit test: a `PromptRequest` mixing `Text` + `Resource` + `ResourceLink`
  blocks assembles all three into one string separated by `"\n"`.
- Unit test: `InitializeResponse` from `make_agent_builder` contains
  `PromptCapabilities` with `embedded_context: true` and `image: true`.

#### 5.5 Deliverables

- `PromptCapabilities` advertises `embedded_context: true` and `image: true`.
- All five `ContentBlock` variants are handled without panicking or silently
  discarding content.
- Zed's `@mention` completion menu shows `#diagnostics`, `#git-diff`, `#rules`,
  `#thread`, and `#fetch` for Atoma sessions.
- File mentions send inline content rather than stubs.

#### 5.6 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo test` passes with all new unit tests.
- Manual verification: type `#diagnostics` in the Zed chat window for an Atoma
  session — the option appears in the completion menu and sending it results in
  LSP diagnostic content appearing in Atoma's received prompt.

---

### Phase 6: Zed MCP Server Integration

Zed forwards all project-configured MCP servers in `NewSessionRequest.mcp_servers`
(`Vec<McpServer>`). Atoma currently never reads this field. This phase connects
those servers per-session and registers their tools, giving the agent access to
project-specific context servers (e.g., a database server, a GitHub server, a
custom indexer) that the user has configured in Zed — without requiring those
servers to also appear in Atoma's `atoma.yaml`.

#### 6.1 Add `McpClientManager` to `SessionState`

Add a `mcp_manager: Option<Arc<RwLock<McpClientManager>>>` field to
`SessionState` (`src/commands/agent.rs` L92-101). This stores a per-session MCP
client manager so that servers connected for one session are isolated from other
sessions. Update the `Debug` impl (L103-113) to handle the new field with a
`"<mcp_manager>"` placeholder, mirroring the executor field pattern.

#### 6.2 Pass `McpClientManager` into `make_agent_builder`

Add a sixth parameter `mcp_manager: Arc<RwLock<McpClientManager>>` to
`make_agent_builder` (`src/commands/agent.rs` L272-277) so the
`NewSessionRequest` closure can clone it per-session. In `handle_agent`
(`src/commands/agent.rs` L761-820), create a shared `McpClientManager` using
the same construction pattern as `execute_plan_task` in `src/commands/run.rs`
(L690-700): `McpClientManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))`.
Call `manager.connect_all(&config.mcp).await` to connect any servers already
present in the config file before handing the manager to `make_agent_builder`.

#### 6.3 Convert and Connect Zed-Forwarded MCP Servers

In the `NewSessionRequest` handler, after step 6 (provider attachment, L454-467)
and before step 8 (model listing, L451-470), iterate over `req.mcp_servers` and
convert each to an Atoma `McpServerConfig` (`src/mcp/server.rs` L219-260):

- `McpServer::Stdio(s)` — construct:
  ```text
  McpServerConfig {
      id: s.name.clone(),
      transport: McpServerTransportConfig::Stdio {
          executable: s.command.to_string_lossy().into_owned(),
          args: s.args.clone(),
          env: s.env.iter().map(|e| (e.name.clone(), e.value.clone())).collect(),
          working_dir: None,
      },
      enabled: true,
      tools_enabled: true,
      .. McpServerConfig::default()
  }
  ```
- `McpServer::Http(h)` — construct with `McpServerTransportConfig::Http`,
  parsing `h.url` as a `url::Url` and building headers from `h.headers`.
- `McpServer::Sse(s)` — same shape as `Http`.

For each converted config, call `mcp_manager_clone.write().await.connect(cfg).await`
and log a `warn!` (do not abort) if any individual server fails to connect,
matching the error-tolerance pattern in `run.rs` L697-704.

After all servers are connected, call `register_mcp_tools` (full signature at
`src/mcp/tool_bridge.rs` L497-501) on the session's `ToolRegistry`, passing the
session's `terminal_mode` and `headless: true`. Store the manager on the
`SessionState.mcp_manager` field.

#### 6.4 Deduplicate Against Config-File Servers

Before connecting a Zed-forwarded server, check whether a server with the same
`id` is already registered in the manager (from the config-file `connect_all`
call in Phase 7.2). Skip forwarded servers whose `name` collides with an
existing server ID and log a `debug!` noting the collision. This prevents
duplicate tool registrations when a user has the same server in both their
Atoma config and their Zed project settings.

#### 6.5 Testing Requirements

- Unit test: a `NewSessionRequest` with an empty `mcp_servers` list produces
  the same `SessionState` as before (no regression; `mcp_manager` holds the
  global manager with zero Zed-added servers).
- Unit test: a `NewSessionRequest` containing one `McpServer::Stdio` entry
  results in `McpClientManager::connect()` being called with the correctly
  converted `McpServerConfig` (use a mock manager).
- Unit test: an `McpServer::Stdio` whose `name` matches an already-registered
  server is skipped without error.
- Unit test: a failed `connect()` call does not abort session creation — the
  session is created and the error is logged.

#### 6.6 Deliverables

- `handle_agent` creates and passes a `McpClientManager` into
  `make_agent_builder`.
- Zed-forwarded MCP servers are connected per-session and their tools are
  registered in the session's `ToolRegistry`.
- Config-file and Zed-forwarded servers coexist without duplicate registrations.

#### 6.7 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test` passes with new unit tests.
- Manual verification: add a test MCP server to a Zed project's `settings.json`,
  open a new Atoma chat, and confirm the server's tools appear in the agent's
  available tool set.

---

### Phase 7: Tool Call UI Cards

Currently, all tool call events stream as plain `AgentMessageChunk` text
(`"Tool: {name} | {preview}"`). Zed renders these as raw text lines instead of
the collapsible tool cards its UI supports. This phase replaces that with
structured `SessionUpdate::ToolCall` and `SessionUpdate::ToolCallUpdate`
notifications. It also requires adding a pre-execution notification hook to
`AgentExecutor`, since tool execution currently happens silently inside
`execute_iteration` with no external visibility before the result is returned.

#### 7.1 Add a Tool Notification Callback to `AgentExecutor`

Add an optional `tool_notifier: Option<Arc<dyn Fn(ToolCallEvent) + Send + Sync>>`
field to `AgentExecutor` in `src/agent/executor.rs`. Define a `ToolCallEvent`
enum in the same file:

```text
pub enum ToolCallEvent {
    Started { call_id: String, tool_name: String, args_preview: String },
    Completed { call_id: String, tool_name: String, output_preview: String },
    Failed { call_id: String, tool_name: String, error: String },
}
```

Add a builder method `with_tool_notifier(cb: Arc<dyn Fn(ToolCallEvent) + Send + Sync>) -> Self`
on `AgentExecutor`. Inside `execute_iteration`, before calling
`self.tool_registry.execute_tool_full()` (L476-478), fire
`notifier(ToolCallEvent::Started { ... })` with a ULID for `call_id` and the
first 120 characters of `args_json` as `args_preview`. After receiving the
`ToolResult`, fire `ToolCallEvent::Completed` or `ToolCallEvent::Failed`
accordingly, reusing the same `call_id`.

When `tool_notifier` is `None` (all existing non-Zed execution paths), the
executor behaves identically to today — no behavioral change for `atoma run`,
`atoma chat`, or `atoma serve`.

#### 7.2 Wire the Notifier in the `PromptRequest` Handler

In the `PromptRequest` handler (`src/commands/agent.rs` L494-672), before
acquiring the executor lock, construct a notifier closure that captures
`cx_clone.clone()` and `session_id.clone()`, and converts each `ToolCallEvent`
to the appropriate `SessionUpdate`:

- `ToolCallEvent::Started { call_id, tool_name, args_preview }`:
  ```text
  SessionUpdate::ToolCall(
      ToolCall::new(call_id, tool_name)
          .kind(tool_kind_from_name(&tool_name))
          .status(ToolCallStatus::InProgress)
          .raw_input(serde_json::from_str(&args_preview).ok())
  )
  ```
- `ToolCallEvent::Completed { call_id, tool_name, output_preview }`:
  ```text
  SessionUpdate::ToolCallUpdate(
      ToolCallUpdate::new(call_id)
          .status(ToolCallStatus::Completed)
          .content(vec![ToolCallContent::from(output_preview)])
  )
  ```
- `ToolCallEvent::Failed { call_id, tool_name, error }`:
  ```text
  SessionUpdate::ToolCallUpdate(
      ToolCallUpdate::new(call_id)
          .status(ToolCallStatus::Failed)
          .content(vec![ToolCallContent::from(error)])
  )
  ```

Call `exec.set_tool_notifier(Arc::new(notifier))` after acquiring the executor
lock and before entering the `'iteration` loop.

Remove the existing `IterationResult::ToolCall` arm at L593-600 that sends the
raw text chunk. The notifier now handles that notification path.

#### 7.3 Add `tool_kind_from_name` Helper

Add a private function `tool_kind_from_name(name: &str) -> ToolKind` near the
top of `src/commands/agent.rs` that maps tool names to `ToolKind` variants
from `agent-client-protocol`. The mapping should cover Atoma's built-in tools:

| Tool name prefix                              | `ToolKind`          |
| --------------------------------------------- | ------------------- |
| `read_file`, `list_directory`                 | `ToolKind::Read`    |
| `edit_file`, `write_file`, `create_directory` | `ToolKind::Edit`    |
| `delete_path`, `move_path`, `copy_path`       | `ToolKind::Move`    |
| `grep`, `find_path`                           | `ToolKind::Search`  |
| `execute_command`                             | `ToolKind::Execute` |
| `fetch`                                       | `ToolKind::Fetch`   |
| `subagent`                                    | `ToolKind::Other`   |
| anything else (MCP tools, ACP tools)          | `ToolKind::Other`   |

#### 7.4 Testing Requirements

- Unit test: when `tool_notifier` is `None`, `execute_iteration` behavior is
  unchanged — existing tests must continue to pass without modification.
- Unit test: when `tool_notifier` is `Some`, a `ToolCallEvent::Started` is
  fired before `execute_tool_full` is called, and `ToolCallEvent::Completed`
  or `ToolCallEvent::Failed` is fired after.
- Unit test: `tool_kind_from_name` returns the correct `ToolKind` for each
  of the mapped tool names and `ToolKind::Other` for an unknown name.
- Unit test: the `PromptRequest` handler no longer emits a raw text chunk for
  a tool call — confirm no `AgentMessageChunk` with `"Tool:"` prefix is sent
  when a tool notifier is installed.

#### 7.5 Deliverables

- `AgentExecutor` has an optional `tool_notifier` field and `ToolCallEvent`
  enum, with zero behavioral change when the notifier is absent.
- The `PromptRequest` handler installs a notifier that sends
  `SessionUpdate::ToolCall` and `SessionUpdate::ToolCallUpdate` notifications.
- Raw `"Tool: {name} | {preview}"` text chunks are no longer emitted for tool
  calls.

#### 7.6 Success Criteria

- `cargo check --all-targets --all-features` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo nextest run --all-features` passes with new unit tests.
- Manual verification: trigger a tool call (e.g., ask Atoma to read a file)
  and confirm the Zed chat window renders a collapsible tool card with the
  tool name, a running spinner while in progress, and a completion status
  when done — with no raw `"Tool:"` text line visible.

---

### Phase 8: Configuration Cleanup and Documentation

#### 8.1 Remove Hard-Coded `terminal_mode` from Zed-Specific Config Examples

Search for any sample config files, README blocks, or documentation that
instructs Zed users to set `terminal_mode: restricted` explicitly. Remove or
replace these with a note that the mode is now controlled through the Zed chat
window and defaults to `Restricted`. Keep the `terminal_mode` field in
`Config` (it remains valid for non-Zed execution paths such as `atoma run` and
`atoma chat`).

#### 8.2 Update `ZedAgentConfig` Documentation

In `src/config.rs` (L1566-1589), update the doc comment on `ZedAgentConfig` to
note that `terminal_mode` for Zed sessions is now initialized from
`Config::terminal_mode` at session creation and can be overridden at runtime
via Zed's mode selector. Add a note that `allow_dangerous: true` in
`ZedAgentConfig` overrides the selected mode for operator-controlled
deployments.

#### 8.3 Update `docs/explanation/implementations.md`

Add an implementation entry covering all preceding phases following the
mandatory format defined in `AGENTS.md` Rule 8. Include the files changed,
a one-paragraph summary, and a brief testing note.

#### 8.4 Testing Requirements

- Verify that an Atoma config file with no `terminal_mode` field results in
  the Zed mode selector defaulting to `Restricted` (the existing
  `#[default]` on `TerminalMode::Interactive` may need to be changed to
  `RestrictedAutonomous` — confirm this is the correct safe default for the
  Zed agent path, or handle the default mapping explicitly in Phase 1 code).
- Confirm that setting `terminal_mode: full` in the config file and connecting
  via Zed shows `Full Autonomous` as the initial mode in the selector.

#### 8.5 Deliverables

- No documentation instructs Zed users to manually configure `terminal_mode`.
- `ZedAgentConfig` doc comments are accurate.
- `docs/explanation/implementations.md` is updated.

#### 8.6 Success Criteria

- `markdownlint --fix --config .markdownlint.json` passes on all modified
  markdown files.
- `prettier --write --parser markdown --prose-wrap always` passes on all
  modified markdown files.
- All five quality gates pass: `cargo fmt --all`, `cargo check --all-targets
--all-features`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo nextest run --all-features`, `cargo test`.
