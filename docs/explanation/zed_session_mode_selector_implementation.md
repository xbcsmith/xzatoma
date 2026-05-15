# Zed Session Mode Selector: Embedded Context, MCP Integration, and Runtime Mode Enforcement

## Overview

This document summarises the three implementation phases that closed the
remaining gaps between XZatoma and the Zed session mode selector. Phase 1
extended the ACP prompt input layer to handle the full range of content blocks
that Zed emits, including embedded text resources and non-image resource links.
Phase 2 connected per-project MCP servers forwarded by Zed on session creation
so that project-scoped tools become available to the agent without requiring
changes to the global configuration file. Phase 3 ensured that selecting a
different mode in the Zed mode selector UI actually propagates the new execution
mode into the live `TerminalTool` registered in the agent's tool registry, so
that subsequent terminal commands are validated against the correct mode
immediately. The three phases together touch `src/acp/prompt_input.rs`,
`src/acp/stdio.rs`, and `src/agent/core.rs`.

## Phase 1: Embedded Context and Rich Content Block Handling

### Advertising Embedded Context Capability

The `handle_initialize` function in `src/acp/stdio.rs` constructs the
`AgentCapabilities` value returned to the Zed client during the ACP handshake.
Prior to Phase 1 it advertised only `image: true` in `PromptCapabilities`. The
`embedded_context` flag was added and set to `true`. When this flag is absent or
`false`, Zed does not include `ContentBlock::Resource` blocks in prompts and
does not offer `#diagnostics`, `#git-diff`, `#rules`, `#thread`, or `#fetch` in
the mention completion menu. Advertising the capability enables the full range
of Zed context attachments.

### Handling TextResourceContents in ContentBlock::Resource

The `acp_content_blocks_to_prompt_input` function in `src/acp/prompt_input.rs`
previously routed every `ContentBlock::Resource` block unconditionally to
`convert_embedded_resource`, a helper designed for binary image blobs. Text
resources such as inline file content, compiler diagnostics, and git diffs
arrived as `TextResourceContents` inside a `Resource` block and caused a vision
policy error or MIME type error.

Phase 1 replaced that unconditional routing with a three-way match on the inner
resource variant. A `TextResourceContents` block whose MIME type is not an image
type is converted to an inline text prompt part. The part begins with a context
header line of the form `[Context: <uri>]` followed by the resource text,
providing the model with clear provenance information. Empty text bodies are
suppressed. A `TextResourceContents` block carrying an image MIME type is
forwarded to `convert_embedded_resource` for backward compatibility.
`BlobResourceContents` blocks continue on the binary image path. Any other
variant returns an unsupported-variant error.

### Non-Image ResourceLink as Text Placeholder

A `ContentBlock::ResourceLink` block whose MIME type is not an image type
previously returned a hard `Provider` error, causing the entire prompt to fail.
Directory mentions, file stubs from older Zed clients, and any resource link
with a non-image MIME type now produce a text placeholder of the form
`[Reference: <name> (<uri>)]`. This informs the model that a resource was
referenced without failing prompt construction. Image resource links continue to
be routed through `convert_image_resource_link` as before.

### Impact on Zed

After Phase 1, XZatoma accepts all content block variants that Zed's session
mode feature emits. The model receives inline text for diagnostic output, git
diffs, and editor rules. Directory and file references appear as named
placeholders rather than causing an error. The mention completion menu in Zed
now shows the full set of context attachment options.

## Phase 2: Zed-Forwarded MCP Server Integration

### Reading NewSessionRequest.mcp_servers

When a user opens a new XZatoma chat session from Zed, the `NewSessionRequest`
sent over the ACP protocol carries an `mcp_servers` field listing MCP servers
configured in the Zed project settings. Before Phase 2, XZatoma ignored that
field entirely. Phase 2 added a processing block inside `create_session` in
`src/acp/stdio.rs` that reads the list and connects each server to the live
`McpClientManager` for the session.

### The convert_acp_mcp_server Helper and ID Sanitization

A private `sanitize_mcp_server_id` function normalises raw server names from Zed
project settings into identifiers that satisfy the constraints imposed by
`McpServerConfig::validate`. It lowercases the input, replaces every character
outside the set `[a-z0-9_-]` with an underscore, and truncates the result to 64
characters. An empty result after sanitization is rejected with a `Config` error
so that servers with names composed entirely of unsupported characters are
refused before any connection is attempted.

A private `convert_acp_mcp_server` function converts a single `acp::McpServer`
variant to an `McpServerConfig`. Stdio servers are mapped to a stdio transport
configuration using the command executable, arguments, and environment variables
provided by Zed. HTTP and SSE servers are both mapped to the HTTP transport
configuration because XZatoma's internal HTTP transport layer handles SSE event
streams natively. A wildcard arm handles future `#[non_exhaustive]` enum
variants without a compilation error.

### Deduplication Against Config-File Servers

Before attempting to connect a converted server, the processing block reads the
connected server list from `McpClientManager` under a shared lock and checks
whether an entry with the same identifier already exists. If it does, the server
is skipped with a debug log entry. This prevents duplicate tool registrations
when Zed re-sends the same server list on reconnect or when a server from Zed's
project settings coincides with one already in `config.yaml`.

### Error Tolerance Policy

Every failure in the Zed-forwarded server processing block is treated as a
non-fatal warning. Conversion errors, connection errors, and tool registration
errors are all logged and skipped; they do not abort session creation. The
entire block is bypassed when `env.mcp_manager` is `None`, which occurs when MCP
is disabled in the global configuration. This design ensures that a single
unreachable or misconfigured Zed project server never prevents the session from
starting or other servers from being connected.

## Phase 3: Runtime Execution Mode Enforcement

### Why CommandValidator.mode Must Be Updated

The `CommandValidator` embedded in the `TerminalTool` registered in the agent's
`ToolRegistry` holds an `ExecutionMode` field that governs which terminal
commands are permitted. When `set_session_mode` was called in
`src/acp/stdio.rs`, it updated the session's `runtime_state.terminal_mode` field
and rebuilt the transient system messages with the new mode constraints, but it
never reached the `CommandValidator` living inside the registered tool object.
The tool object was created once during session initialisation and was never
replaced on mode change. As a result, switching from `planning` to
`full_autonomous` in the Zed mode selector did not unblock terminal commands
because the validator's mode remained `ExecutionMode::Interactive` for the
lifetime of the session.

### Replacing the Registered Terminal Tool via tools_mut

Phase 3 solved this by replacing the entire `TerminalTool` object in the
registry rather than trying to mutate the existing one. Replacing the object
avoids introducing interior mutability into `TerminalTool` or
`CommandValidator`. The `ToolRegistry` stores executors as
`Arc<dyn ToolExecutor>` values keyed by name, so inserting a new `Arc` under the
`"terminal"` key is an O(1) operation that does not affect any other registered
tool. Any in-flight tool execution retains its own `Arc` clone and completes
under the previous mode; the replacement takes effect only for subsequent
dispatches.

The replacement block is placed inside the scoped region that already holds the
agent mutex lock for system prompt regeneration. After updating the transient
system messages, the block acquires a short-lived read lock on the session state
only to copy the workspace root, then drops the lock before constructing the new
tool. This ordering is the same as the existing `drop(session_lock)` that
precedes `agent_lock` acquisition and avoids any lock-ordering inversion.

### The tools_mut Accessor Added to Agent

A new `pub fn tools_mut` method was added to `impl Agent` in
`src/agent/core.rs`. It returns a mutable reference to the agent's internal
`ToolRegistry`. This accessor follows the same pattern as the existing immutable
`tools()` accessor and preserves the module boundary: the ACP stdio layer
updates the registry through the accessor rather than bypassing the agent
abstraction. The doc comment on the method explains its intended use so that
future callers understand the thread-safety contract.

## Files Changed

- `src/acp/prompt_input.rs` - Added three-way dispatch on
  `ContentBlock::Resource` inner variant to handle `TextResourceContents` as
  inline text parts; replaced the hard error for non-image
  `ContentBlock::ResourceLink` with a text reference placeholder.
- `src/acp/stdio.rs` - Added `embedded_context: true` to `PromptCapabilities` in
  `handle_initialize`; added `sanitize_mcp_server_id` and
  `convert_acp_mcp_server` private helpers; added Zed-forwarded MCP server
  connection block in `create_session`; added terminal tool replacement block in
  `set_session_mode`; added imports for `CommandValidator` and `TerminalTool`.
- `src/agent/core.rs` - Added `pub fn tools_mut` accessor returning a mutable
  reference to the agent's `ToolRegistry`.

## Testing

Phase 1 added eight unit tests to `src/acp/prompt_input.rs` covering all new
dispatch paths: text resource contents with various MIME types, absent MIME
types, blob resources that stay on the image path, non-image and typeless
resource link placeholders, image resource links that still route to image
conversion, and mixed-block order preservation. Two tests were also added or
updated in `src/acp/stdio.rs` to assert that `embedded_context` is advertised in
the initialize response both in unit form and over the full protocol stack.
Phase 2 added six unit tests in `src/acp/stdio.rs` covering Stdio config
production, HTTP config production, name sanitization, empty name rejection,
invalid URL rejection, and session creation with an empty `mcp_servers` list.
Phase 3 added four unit tests: one in `src/agent/core.rs` verifying that
`tools_mut` correctly registers a new tool and updates `num_tools`, and three in
`src/acp/stdio.rs` verifying that mode changes to `full_autonomous`, transitions
from `full_autonomous` to `planning`, and unknown mode IDs each produce the
correct outcome over the full protocol stack. All 2086 unit tests pass with zero
warnings.
