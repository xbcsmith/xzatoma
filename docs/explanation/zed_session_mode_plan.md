# XZatoma Zed Session Mode Selector: Remaining Gaps Implementation Plan

## Overview

This plan covers the four remaining functional gaps in XZatoma's Zed ACP
integration. The core session mode infrastructure has already been implemented:
`src/acp/session_mode.rs` defines four modes (`planning`, `write`, `safe`,
`full_autonomous`), the `SetSessionModeRequest` handler is registered and
working, `NewSessionResponse` includes populated `modes` and `config_options`,
`tool_notifications.rs` sends structured tool-call UI cards, and the IDE bridge
is operational.

The four gaps addressed by this plan are:

1. `PromptCapabilities.embedded_context` is not advertised, causing Zed to hide
   `#diagnostics`, `#git-diff`, `#rules`, `#thread`, and `#fetch` from the
   mention completion menu and to send file references as `ResourceLink` stubs.
2. `ContentBlock::Resource(EmbeddedResource)` blocks containing
   `TextResourceContents` (file contents, diagnostics, git diffs, rules) are
   rejected with an error in `acp_content_blocks_to_prompt_input`, making all
   file and context mentions non-functional even when Zed does send them.
3. Non-image `ContentBlock::ResourceLink` blocks are rejected with an error,
   breaking directory mentions and file references from older Zed clients.
4. Zed-forwarded MCP servers in `NewSessionRequest.mcp_servers` are never read
   or connected, so project-specific context servers (database, GitHub, custom
   indexers) configured in Zed project settings are unavailable to the agent.
5. When `set_session_mode` is called, the `TerminalTool`'s `CommandValidator`
   inside the agent's tool registry still holds the original `ExecutionMode`
   from session creation. Mode changes only rebuild system messages; they do not
   enforce new terminal permission policies at the tool level.

---

## Current State Analysis

### Existing Infrastructure

- `src/acp/session_mode.rs`: Defines `MODE_PLANNING`, `MODE_WRITE`, `MODE_SAFE`,
  `MODE_FULL_AUTONOMOUS` constants, `build_session_modes()`,
  `build_session_mode_state()`, `ModeRuntimeEffect`, and `mode_runtime_effect()`
  with full unit test coverage. No changes needed.

- `src/acp/stdio.rs`: `run_stdio_agent_with_transport` registers handlers for
  `InitializeRequest`, `NewSessionRequest`, `PromptRequest`,
  `SetSessionModeRequest`, `SetSessionConfigOptionRequest`,
  `SetSessionModelRequest`, and `CancelNotification`. All handlers are wired and
  tested. `ActiveSessionState` holds `current_mode_id: String` and
  `runtime_state: SessionRuntimeState`.

- `src/acp/session_config.rs`: Defines `SessionRuntimeState` with fields
  `safety_mode_str`, `terminal_mode`, `tool_routing`, `vision_enabled`,
  `subagents_enabled`, `mcp_enabled`, `max_turns`. Implements
  `build_session_config_options()` and `apply_config_option_change()`.

- `src/acp/tool_notifications.rs`: Implements `tool_kind_for_name()`,
  `build_tool_call_start()`, `build_tool_call_completion()`,
  `build_tool_call_failure()`, `build_tool_call_locations()`, and
  `generate_tool_call_id()`. The `AcpSessionObserver` in `src/acp/stdio.rs`
  routes `ToolCallStarted`, `ToolCallCompleted`, and `ToolCallFailed` events to
  `SessionUpdate::ToolCall` and `SessionUpdate::ToolCallUpdate` notifications.

- `src/acp/prompt_input.rs`: `acp_content_blocks_to_prompt_input()` handles
  `ContentBlock::Text` and `ContentBlock::Image`. Image `ResourceLink` and
  `BlobResourceContents` within `Resource` blocks are routed to image
  conversion. Non-image paths in both variants currently return errors.

- `src/tools/terminal.rs`: `CommandValidator` holds a `pub mode: ExecutionMode`
  field that governs command validation policy. `TerminalTool` holds a
  `pub validator: CommandValidator`. Both fields are public but the tool is
  stored as `Arc<dyn ToolExecutor>` in `ToolRegistry`, providing only immutable
  access through the trait object.

- `src/agent/core.rs`: `Agent` struct stores `tools: ToolRegistry` as a private
  field. The only public accessor is `pub fn tools(&self) -> &ToolRegistry`.
  There is no `tools_mut()` accessor and no method to replace a registered tool.

- `agent-client-protocol` v0.11.1 depends on `agent-client-protocol-schema`
  v0.12.0. `PromptCapabilities.embedded_context` is available unconditionally in
  the schema (no feature flag required). `NewSessionRequest.mcp_servers` is a
  `Vec<acp::McpServer>` field available unconditionally.

### Identified Issues

1. `handle_initialize()` in `src/acp/stdio.rs` L1193-1195 advertises only
   `.image(true)` in `PromptCapabilities`. The `embedded_context` field is
   `false` (the struct default), so Zed suppresses file and context mention
   types from the completion menu.

2. `acp_content_blocks_to_prompt_input()` in `src/acp/prompt_input.rs` L90-98
   routes all `ContentBlock::Resource(resource)` blocks to
   `convert_embedded_resource()`, which rejects `TextResourceContents` with a
   non-image MIME type by returning
   `Err(provider_error("unsupported embedded text resource ..."))`. This makes
   every file mention, `#diagnostics` block, `#git-diff` block, and `#rules`
   block fail at the protocol boundary.

3. `acp_content_blocks_to_prompt_input()` in `src/acp/prompt_input.rs` L73-84
   routes all `ContentBlock::ResourceLink(resource)` blocks to
   `convert_image_resource_link()` when the MIME type is an image type, or
   returns `Err(provider_error("unsupported resource link ..."))` for all other
   MIME types. Directory mentions and non-image file links always fail.

4. `create_session()` in `src/acp/stdio.rs` builds
   `env = build_agent_environment()` from the XZatoma config file and passes
   `env.mcp_manager` directly to `ActiveSessionState`. The `request.mcp_servers`
   field (ACP type `Vec<acp::McpServer>`) is never read. Zed-forwarded MCP
   servers are silently ignored.

5. `set_session_mode()` in `src/acp/stdio.rs` updates
   `session_lock.runtime_state.terminal_mode` and rebuilds the agent's transient
   system messages. It does not update the `CommandValidator.mode` field inside
   the `TerminalTool` stored in the agent's tool registry. After a mode change,
   the terminal tool still enforces the `ExecutionMode` that was set at session
   creation time.

---

## Implementation Phases

---

### Phase 1: Embedded Context Capability and Rich Content Block Handling

This phase fixes the three content-pipeline gaps that prevent Zed's `@mention`
system from working. All three fixes are in two files and must be implemented
together because they share test infrastructure.

#### Task 1.1: Advertise `embedded_context: true` in `PromptCapabilities`

**File**: `src/acp/stdio.rs`

**Location**: `pub fn handle_initialize()`, lines 1184-1203.

**Current code** (lines 1193-1195):

```rust
acp::AgentCapabilities::new()
    .load_session(false)
    .prompt_capabilities(acp::PromptCapabilities::new().image(true))
```

**Required change**: Replace the `.prompt_capabilities(...)` call with:

```rust
.prompt_capabilities(
    acp::PromptCapabilities::new()
        .image(true)
        .embedded_context(true),
)
```

No feature flag or crate upgrade is needed.
`PromptCapabilities::embedded_context` is a stable field in
`agent-client-protocol-schema` v0.12.0, which is already a direct dependency.

**Effect**: Zed enables `ContentBlock::Resource` blocks in prompt requests and
shows `#diagnostics`, `#git-diff`, `#rules`, `#thread`, and `#fetch` in the
mention completion menu.

#### Task 1.2: Handle `TextResourceContents` in `ContentBlock::Resource` Dispatch

**File**: `src/acp/prompt_input.rs`

**Location**: `pub fn acp_content_blocks_to_prompt_input()`, the
`ContentBlock::Resource(resource)` arm, lines 90-98.

**Current behavior**: All `ContentBlock::Resource(resource)` blocks are passed
to `convert_embedded_resource()`. That function checks
`is_image_mime_type(Some(mime_type))` on `TextResourceContents` and returns an
error when the MIME type is not an image type.

**Required change**: Replace the single
`ContentBlock::Resource(resource) => ...` arm with an explicit dispatch on the
inner `EmbeddedResourceResource` variant:

```rust
acp::ContentBlock::Resource(resource) => match &resource.resource {
    acp::EmbeddedResourceResource::TextResourceContents(text) => {
        if is_image_mime_type(text.mime_type.as_deref()) {
            // Image disguised as a text resource (rare); fall back to URI conversion.
            parts.push(PromptInputPart::image(convert_embedded_resource(
                resource,
                config,
                workspace_root,
            )?));
        } else {
            // Inline file content, diagnostics, git diff, rules, etc.
            let header = format!("\n[Context: {}]\n", text.uri);
            let body = format!("{}{}", header, text.text);
            if !body.trim().is_empty() {
                parts.push(PromptInputPart::text(body));
            }
        }
    }
    acp::EmbeddedResourceResource::BlobResourceContents(_) => {
        // Binary blob: always attempt image conversion.
        parts.push(PromptInputPart::image(convert_embedded_resource(
            resource,
            config,
            workspace_root,
        )?));
    }
    _ => {
        return Err(provider_error("unsupported embedded ACP resource variant"));
    }
},
```

**Rationale**: `TextResourceContents.text` holds the full content of the
referenced resource as a UTF-8 string. Prepending a URI header gives the model
the source context. The existing `convert_embedded_resource()` function is
reused for the binary/image path to avoid duplication.

#### Task 1.3: Handle Non-Image `ContentBlock::ResourceLink` as Text Placeholders

**File**: `src/acp/prompt_input.rs`

**Location**: `pub fn acp_content_blocks_to_prompt_input()`, the
`ContentBlock::ResourceLink(resource)` arm, lines 73-84.

**Current behavior**: Non-image MIME types return
`Err(provider_error("unsupported resource link ..."))`.

**Required change**: Replace the current arm with:

```rust
acp::ContentBlock::ResourceLink(resource) => {
    if is_image_mime_type(resource.mime_type.as_deref()) {
        parts.push(PromptInputPart::image(convert_image_resource_link(
            resource,
            config,
            workspace_root,
        )?));
    } else {
        // Directory mentions, file stubs from older Zed clients, and any
        // non-image resource link: emit a text reference placeholder so the
        // model knows a resource was referenced without failing the prompt.
        let placeholder = format!("[Reference: {} ({})]", resource.name, resource.uri);
        parts.push(PromptInputPart::text(placeholder));
    }
}
```

**Rationale**: The placeholder provides context to the model without requiring
XZatoma to perform a live file read at the protocol boundary. If the user
intends the model to read the file, the model can call `read_file` as a tool
call. Failing the entire prompt for a non-image resource link is a worse
default.

#### Task 1.4: Testing Requirements

All tests must be placed in the `mod tests` block inside
`src/acp/prompt_input.rs`. Existing tests must continue to pass without
modification.

- **test_text_resource_contents_converted_to_text_part**: Construct a
  `ContentBlock::Resource` wrapping
  `EmbeddedResourceResource::TextResourceContents` with
  `uri: "file:///src/main.rs"` and `text: "fn main() {}"` and
  `mime_type: Some("text/plain")`. Assert that the returned
  `MultimodalPromptInput` has one text part containing both
  `[Context: file:///src/main.rs]` and `fn main() {}` in the part text.

- **test_text_resource_contents_with_diagnostics_mime_type**: Same as above but
  use `mime_type: Some("text/x-diagnostics")`. Assert the text part is produced
  (not an error).

- **test_text_resource_contents_with_no_mime_type**: Use `mime_type: None`.
  Assert the text part is produced (not an error) because the absence of a MIME
  type is treated as non-image.

- **test_blob_resource_with_image_mime_type_remains_image_path**: Construct a
  `ContentBlock::Resource` wrapping
  `EmbeddedResourceResource::BlobResourceContents` with
  `mime_type: Some("image/png")`. Assert the function either succeeds with an
  image part or returns the existing image-validation error (not a new
  "unsupported resource" error).

- **test_non_image_resource_link_produces_placeholder**: Construct a
  `ContentBlock::ResourceLink` with `name: "src/"`, `uri: "file:///src/"`, and
  `mime_type: Some("inode/directory")`. Assert the returned
  `MultimodalPromptInput` has one text part whose text contains both `src/` and
  `file:///src/`.

- **test_non_image_resource_link_with_no_mime_type_produces_placeholder**: Same
  as above but with `mime_type: None`. Assert a text placeholder is produced.

- **test_image_resource_link_still_routed_to_image_path**: Construct a
  `ContentBlock::ResourceLink` with `mime_type: Some("image/png")` and a valid
  `uri`. Assert the image conversion path is invoked (existing behavior
  preserved).

- **test_mixed_prompt_with_text_resource_and_plain_text**: Construct a
  `PromptRequest` containing both a `ContentBlock::Text` block and a
  `ContentBlock::Resource(TextResourceContents)` block. Assert both parts are
  present in the assembled `MultimodalPromptInput` and their order is preserved.

- **test_handle_initialize_advertises_embedded_context**: In `src/acp/stdio.rs`
  `mod tests`, update the existing test
  `test_handle_initialize_advertises_text_and_vision_prompt_capabilities` to
  also assert
  `response.agent_capabilities.prompt_capabilities.embedded_context == true`.
  Add a new test
  `test_initialize_request_prompt_capabilities_include_embedded_context_over_protocol`
  that uses `run_client_server_test` to send an `InitializeRequest` over the
  full protocol stack and asserts `embedded_context == true` in the response.

#### Task 1.5: Deliverables

- [ ] `src/acp/stdio.rs`: `handle_initialize()` advertises
      `embedded_context: true`.
- [ ] `src/acp/prompt_input.rs`: `ContentBlock::Resource` with
      `TextResourceContents` produces a text part containing the URI header and
      resource text.
- [ ] `src/acp/prompt_input.rs`: `ContentBlock::Resource` with
      `BlobResourceContents` continues to route through the image conversion
      path.
- [ ] `src/acp/prompt_input.rs`: Non-image `ContentBlock::ResourceLink` produces
      a `[Reference: name (uri)]` text placeholder instead of an error.
- [ ] Image `ContentBlock::ResourceLink` behavior is unchanged.
- [ ] All new unit tests pass. All pre-existing tests pass.

#### Task 1.6: Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo check --all-targets --all-features` exits with code 0.
- `cargo clippy --all-targets --all-features -- -D warnings` exits with code 0.
- `cargo test --all-features` exits with code 0, including all new tests listed
  in Task 1.4.
- Manual verification: type `@` in a Zed chat window with XZatoma connected and
  confirm that `#diagnostics`, `#git-diff`, `#rules`, `#thread`, and `#fetch`
  appear in the completion menu. Send a `#file` mention and confirm the file
  content is visible in the agent's context without an error.

---

### Phase 2: Zed-Forwarded MCP Server Integration

This phase connects per-project MCP servers forwarded by Zed in
`NewSessionRequest.mcp_servers` to the session's live `McpClientManager`. These
are the MCP servers the user has configured in their Zed project settings, not
in XZatoma's `config.yaml`.

#### Task 2.1: Add `convert_acp_mcp_server` Helper

**File**: `src/acp/stdio.rs`

**Location**: Add as a private free function near `initial_mode_id_from_config`
(around line 1232).

**Purpose**: Converts a single `acp::McpServer` variant to an XZatoma
`McpServerConfig`. The function must handle all three `acp::McpServer` variants.

**Type signatures required**:

- Input: `&acp::McpServer`
- Output: `Result<crate::mcp::server::McpServerConfig>`

**Conversion rules**:

- `acp::McpServer::Stdio(s)`: Build `McpServerConfig` with:

  - `id`: `s.name.clone()` — sanitize with `s.name.to_ascii_lowercase()` and
    replace all characters that are not ASCII lowercase letters, digits,
    underscores, or hyphens with underscores to satisfy `McpServerConfig`'s ID
    validation regex `^[a-z0-9_-]{1,64}$`. Truncate to 64 characters. If the
    result is empty after sanitization, return an error.
  - `transport`:
    `McpServerTransportConfig::Stdio { executable: s.command.to_string_lossy().into_owned(), args: s.args.clone(), env: s.env.iter().map(|e| (e.name.clone(), e.value.clone())).collect::<HashMap<String, String>>(), working_dir: None }`
  - `enabled: true`, `tools_enabled: true`
  - All capability flags (`resources_enabled`, `prompts_enabled`,
    `sampling_enabled`, `elicitation_enabled`): `false`
  - `timeout_seconds: None` (use the manager's global default)

- `acp::McpServer::Http(h)`: Build `McpServerConfig` with:

  - `id`: sanitized from `h.name` using the same rule as Stdio.
  - `transport`:
    `McpServerTransportConfig::Http { endpoint: h.url.parse::<url::Url>().map_err(|e| XzatomaError::Config(format!(...)))?, headers: h.headers.iter().map(|hdr| (hdr.name.clone(), hdr.value.clone())).collect(), timeout_seconds: None, oauth: None }`
  - `enabled: true`, `tools_enabled: true`, all other flags `false`.

- `acp::McpServer::Sse(s)`: Build `McpServerConfig` with the same shape as
  `Http` using `s.url` and `s.headers` and `s.name`.

Call `cfg.validate()` at the end of each arm before returning `Ok(cfg)`. Return
the validation error as-is on failure.

#### Task 2.2: Connect Zed-Forwarded Servers in `create_session`

**File**: `src/acp/stdio.rs`

**Location**: `async fn create_session()`. Insert the new logic block after
`let workspace_root = normalize_workspace_root(&workspace_root);` and before the
provider creation block. The `env` variable (which holds `env.mcp_manager`) is
built inside this function before the `ActiveSessionState` is constructed, so
the manager is available to modify.

**Implementation steps** (in order):

1. After `build_agent_environment()` returns `env`, check if
   `env.mcp_manager.is_some()`. If none, skip the entire Zed MCP block.
   Zed-forwarded servers are only connected when the global MCP manager was
   successfully initialized.

2. Iterate over `request.mcp_servers` (type `Vec<acp::McpServer>`):

   ```rust
   for acp_server in &request.mcp_servers {
       ...
   }
   ```

3. For each server, call `convert_acp_mcp_server(acp_server)`. On `Err`, log a
   `warn!` including `session_id` and the error, then `continue` to the next
   server. Do not abort session creation.

4. Check deduplication: obtain a read lock on `env.mcp_manager` and call
   `manager.connected_servers()`. If any existing server entry has an `id` equal
   to the converted config's `id`, log a `debug!` message with the collision and
   `continue`.

5. Obtain a write lock on `env.mcp_manager` and call
   `manager.connect(cfg).await`. On `Err`, log a `warn!` including `session_id`,
   the server id, and the error, then `continue`. Do not abort.

6. On success, log an `info!` with `session_id` and the connected server id.

7. After the loop, call `register_mcp_tools` on `env.tool_registry` for the
   Zed-forwarded servers. Use the initial `execution_mode` from
   `config.agent.terminal.default_mode` and `headless: true`.

**Note**: The imports required are already present in `src/acp/stdio.rs`
(`use crate::mcp::manager::McpClientManager` is already imported). Add
`use crate::mcp::server::{McpServerConfig, McpServerTransportConfig}` at the top
of the file if not already imported.

#### Task 2.3: Testing Requirements

All new tests are placed in `mod tests` inside `src/acp/stdio.rs`.

- **test_create_session_with_empty_mcp_servers_list_does_not_error**: Send a
  `NewSessionRequest` with no MCP servers. Assert the session is created
  successfully (same behavior as before).

- **test_convert_acp_mcp_server_stdio_produces_valid_config**: Call
  `convert_acp_mcp_server` with an
  `acp::McpServer::Stdio(McpServerStdio::new( "my-server", "/usr/bin/mcp-tool").args(vec!["--flag".to_string()]))`.
  Assert the returned `McpServerConfig` has `id == "my-server"`,
  `transport == McpServerTransportConfig::Stdio { executable: "/usr/bin/mcp-tool", args: ["--flag"], ... }`,
  and `enabled == true`.

- **test_convert_acp_mcp_server_http_produces_valid_config**: Call
  `convert_acp_mcp_server` with an
  `acp::McpServer::Http(McpServerHttp::new( "http-server", "http://localhost:8080/mcp"))`.
  Assert the returned config has a valid `McpServerTransportConfig::Http` with
  the correct endpoint URL.

- **test_convert_acp_mcp_server_sanitizes_name_with_spaces**: Call
  `convert_acp_mcp_server` with a server whose `name` is `"My MCP Server!"`.
  Assert the returned `id` is `"my_mcp_server_"` (or equivalent sanitized form).

- **test_convert_acp_mcp_server_rejects_empty_name**: Call
  `convert_acp_mcp_server` with `name: ""`. Assert the function returns `Err`.

- **test_convert_acp_mcp_server_rejects_invalid_http_url**: Call
  `convert_acp_mcp_server` with `acp::McpServer::Http(...)` using
  `url: "not a url"`. Assert the function returns `Err`.

#### Task 2.4: Deliverables

- [ ] `src/acp/stdio.rs`: Private function `convert_acp_mcp_server` is
      implemented and handles all three `acp::McpServer` variants with full
      input sanitization.
- [ ] `src/acp/stdio.rs`: `create_session()` reads `request.mcp_servers`,
      converts each server, deduplicates by id, connects each with error
      tolerance (individual failures do not abort session creation), and
      registers their tools.
- [ ] All new unit tests pass. All pre-existing tests pass.

#### Task 2.5: Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo check --all-targets --all-features` exits with code 0.
- `cargo clippy --all-targets --all-features -- -D warnings` exits with code 0.
- `cargo test --all-features` exits with code 0.
- Manual verification: add a stdio MCP server to a Zed project's `settings.json`
  under `"context_servers"`, open a new XZatoma chat session, and confirm the
  server's tools appear in the agent's available tool set.

---

### Phase 3: Runtime Execution Mode Enforcement on Mode Change

This phase ensures that when `set_session_mode` is called, the `TerminalTool`
inside the agent's tool registry actually enforces the new `ExecutionMode`. The
current implementation updates `runtime_state.terminal_mode` and rebuilds system
messages, but the `CommandValidator.mode` field inside the registered
`TerminalTool` is never updated.

#### Task 3.1: Add `tools_mut` Accessor to `XzatomaAgent`

**File**: `src/agent/core.rs`

**Location**: Inside `impl Agent`, near the existing
`pub fn tools(&self) -> &ToolRegistry` accessor.

**Add the following public method**:

````rust
/// Returns a mutable reference to the agent's tool registry.
///
/// This accessor is used by the ACP stdio layer to replace individual tools
/// (such as the terminal tool) when the session mode changes at runtime.
///
/// # Returns
///
/// Returns a mutable reference to the agent's [`ToolRegistry`].
///
/// # Examples
///
/// ```
/// use xzatoma::agent::core::Agent;
/// use xzatoma::tools::ToolRegistry;
///
/// // The registry can be updated to reflect runtime mode changes.
/// // let mut agent = ...;
/// // let registry = agent.tools_mut();
/// ```
pub fn tools_mut(&mut self) -> &mut ToolRegistry {
    &mut self.tools
}
````

**Testing**: Add `test_agent_tools_mut_returns_mutable_registry` in the
`mod tests` block of `src/agent/core.rs` that creates an agent, calls
`tools_mut()`, registers a new tool, and asserts `agent.num_tools()` reflects
the change.

#### Task 3.2: Update `set_session_mode` to Replace the `TerminalTool`

**File**: `src/acp/stdio.rs`

**Location**: `async fn set_session_mode()`, after the block that rebuilds
transient system messages (currently the final block in the function, ending
around line 628).

**Required imports** to add at the top of `src/acp/stdio.rs` if not already
present:

```rust
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::chat_mode::SafetyMode;
```

**Implementation**: After
`agent_lock.set_transient_system_messages(vec![system_prompt])`, add the
following block:

```rust
// Replace the terminal tool so the new ExecutionMode is enforced immediately.
{
    let session_read = session.lock().await;
    let workspace_root = session_read.workspace_root.clone();
    drop(session_read);

    let new_validator = CommandValidator::new(
        effect.terminal_mode,
        workspace_root,
    );
    let new_terminal_tool = TerminalTool::new(
        new_validator,
        self.config.agent.terminal.clone(),
    )
    .with_safety_mode(safety_mode);
    agent_lock
        .tools_mut()
        .register("terminal", Arc::new(new_terminal_tool));
}
```

Where `safety_mode` is the `SafetyMode` value already computed earlier in the
same function body for building the system prompt.

**Note**: The `session` variable in `set_session_mode` is an
`Arc<Mutex<ActiveSessionState>>` retrieved from `self.sessions.get(...)`. At the
point where the new block executes, `session_lock` (the `MutexGuard`) has
already been dropped via `drop(session_lock)` in the existing code. Acquire a
new short-lived read lock only to copy `workspace_root`.

#### Task 3.3: Testing Requirements

Tests are placed in `mod tests` inside `src/acp/stdio.rs`.

- **test_set_session_mode_updates_terminal_tool_execution_mode_to_full_autonomous**:
  Using `run_client_server_test`, create a session (default mode should be
  `write` or `planning`), send a `SetSessionModeRequest` with
  `mode_id: "full_autonomous"`, then send a `PromptRequest` asking the agent to
  execute a command that is blocked in `RestrictedAutonomous` mode but allowed
  in `FullAutonomous` mode. Use a mock provider that returns a tool call for
  that command. Assert the tool executes without a `CommandRequiresConfirmation`
  error. (Alternatively, assert via unit test that the terminal tool in the
  agent's registry has `validator.mode == ExecutionMode::FullAutonomous` after
  the mode change, without going through the full protocol stack.)

- **test_set_session_mode_full_autonomous_to_planning_restricts_terminal**: Same
  setup but switch from `full_autonomous` to `planning`. Assert that the
  terminal tool's validator mode is updated to `ExecutionMode::Interactive`.

- **test_set_session_mode_does_not_change_terminal_for_unknown_mode**: Send a
  `SetSessionModeRequest` with an unknown `mode_id` (e.g., `"turbo"`). Assert
  the request returns a JSON-RPC error and the terminal tool's execution mode in
  the registry is unchanged.

- **test_agent_tools_mut_returns_mutable_registry** (in `src/agent/core.rs`): As
  described in Task 3.1.

#### Task 3.4: Deliverables

- [ ] `src/agent/core.rs`: `Agent` exposes
      `pub fn tools_mut(&mut self) -> &mut ToolRegistry` with doc comment and
      unit test.
- [ ] `src/acp/stdio.rs`: `set_session_mode()` replaces the `terminal` tool in
      the agent's tool registry with a new `TerminalTool` whose
      `CommandValidator` holds the new `ExecutionMode`.
- [ ] Mode changes take effect on the very next prompt turn within the session.
- [ ] All new unit tests pass. All pre-existing tests pass.

#### Task 3.5: Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo check --all-targets --all-features` exits with code 0.
- `cargo clippy --all-targets --all-features -- -D warnings` exits with code 0.
- `cargo test --all-features` exits with code 0.
- Manual verification: switch to `Full Autonomous` mode in the Zed mode selector
  and confirm XZatoma executes a terminal command that would be blocked in
  restricted mode. Switch back to `Safe` mode and confirm the same command is
  blocked.

---

### Phase 4: Documentation

This phase creates the mandatory implementation documentation required by
`AGENTS.md` Rule 5 and updates the existing documentation index.

#### Task 4.1: Create Implementation Summary Document

**File**: `docs/explanation/zed_session_mode_selector_implementation.md`

The document must cover the following sections with prose descriptions (not code
blocks):

1. **Overview**: One paragraph summarizing the four gaps addressed by phases 1
   through 3 of this plan and the files changed.

2. **Phase 1: Embedded Context and Rich Content Block Handling**:

   - Which field was added to `PromptCapabilities` and why.
   - How `ContentBlock::Resource(TextResourceContents)` is now converted to a
     text part with a URI header.
   - How non-image `ContentBlock::ResourceLink` produces a placeholder instead
     of an error.
   - The impact on Zed's mention completion menu.

3. **Phase 2: Zed-Forwarded MCP Server Integration**:

   - How `NewSessionRequest.mcp_servers` is read and converted.
   - The `convert_acp_mcp_server` helper and its ID sanitization rules.
   - The deduplication strategy against config-file servers.
   - The error tolerance policy (individual failures do not abort session
     creation).

4. **Phase 3: Runtime Execution Mode Enforcement**:

   - Why `CommandValidator.mode` must be updated when a session mode changes.
   - The approach taken: replacing the registered `terminal` tool via
     `tools_mut()` rather than introducing interior mutability.
   - The `tools_mut()` accessor added to `Agent`.

5. **Files Changed**: A bullet list of every file modified by this plan with a
   one-sentence description of each change.

6. **Testing**: A single paragraph summarizing the test coverage added.

#### Task 4.2: Update `docs/explanation/implementations.md`

**File**: `docs/explanation/implementations.md`

Add a new entry at the bottom of the file following the existing entry format:

- **Title**: Zed Session Mode Selector: Embedded Context, MCP Integration, and
  Runtime Mode Enforcement
- **Date**: Use the RFC 3339 timestamp format for the date of completion.
- **Summary**: Two to three sentences covering what was implemented.
- **Files changed**: Same bullet list as in Task 4.1.

#### Task 4.3: Testing Requirements

- Run
  `markdownlint --fix --config .markdownlint.json docs/explanation/zed_session_mode_selector_implementation.md`
  and confirm it exits with code 0.
- Run
  `prettier --write --parser markdown --prose-wrap always docs/explanation/zed_session_mode_selector_implementation.md`
  and confirm it exits with code 0.
- Apply the same two commands to `docs/explanation/implementations.md` and
  confirm both exit with code 0.

#### Task 4.4: Deliverables

- [ ] `docs/explanation/zed_session_mode_selector_implementation.md` is created
      with all sections listed in Task 4.1.
- [ ] `docs/explanation/implementations.md` is updated with a new entry
      following the existing format.
- [ ] Both markdown files pass `markdownlint` and `prettier` without errors.

#### Task 4.5: Success Criteria

- `markdownlint --fix --config .markdownlint.json` exits with code 0 on both
  modified markdown files.
- `prettier --write --parser markdown --prose-wrap always` exits with code 0 on
  both modified markdown files.
- All five quality gates pass in sequence without error:
  1. `cargo fmt --all`
  2. `cargo check --all-targets --all-features`
  3. `cargo clippy --all-targets --all-features -- -D warnings`
  4. `cargo test --all-features`

---

## Implementation Order Summary

Execute the phases in the order listed. Each phase must fully pass all quality
gates before beginning the next phase.

| Phase | Files Modified                                                                                        | Depends On |
| ----- | ----------------------------------------------------------------------------------------------------- | ---------- |
| 1     | `src/acp/stdio.rs`, `src/acp/prompt_input.rs`                                                         | None       |
| 2     | `src/acp/stdio.rs`                                                                                    | None       |
| 3     | `src/agent/core.rs`, `src/acp/stdio.rs`                                                               | None       |
| 4     | `docs/explanation/zed_session_mode_selector_implementation.md`, `docs/explanation/implementations.md` | 1, 2, 3    |

Phases 1, 2, and 3 have disjoint or nearly disjoint write sets and may be
implemented in parallel by separate agents if desired, provided each agent runs
the full quality gate sequence before declaring its phase complete.

---

## Reference: Already-Implemented Features

The following items are complete and must NOT be re-implemented:

| Feature                                                                           | Location                                              |
| --------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Four session modes with `ModeRuntimeEffect` mapping                               | `src/acp/session_mode.rs`                             |
| `NewSessionResponse` includes `modes` and `config_options`                        | `src/acp/stdio.rs` `create_session()`                 |
| `SetSessionModeRequest` handler and `CurrentModeUpdate`                           | `src/acp/stdio.rs` `run_stdio_agent_with_transport()` |
| `SetSessionConfigOptionRequest` handler                                           | `src/acp/stdio.rs`                                    |
| `SetSessionModelRequest` handler                                                  | `src/acp/stdio.rs`                                    |
| Structured tool-call UI cards via `AcpSessionObserver`                            | `src/acp/stdio.rs`, `src/acp/tool_notifications.rs`   |
| IDE bridge with `ide_read_text_file`, `ide_write_text_file`, `ide_terminal` tools | `src/acp/ide_bridge.rs`, `src/tools/ide_tools.rs`     |
| Image and vision support in prompt input                                          | `src/acp/prompt_input.rs`                             |
| MCP from config file via `build_agent_environment`                                | `src/commands/environment.rs`                         |
| `AvailableCommandsUpdate` notification after session start                        | `src/acp/stdio.rs`                                    |
| `CancelNotification` handler                                                      | `src/acp/stdio.rs`                                    |
| Session persistence and conversation resume                                       | `src/acp/stdio.rs`, `src/storage/`                    |
