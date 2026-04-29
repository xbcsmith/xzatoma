# Zed ACP Agent Command Implementation Plan

## Overview

XZatoma already exposes an ACP-compatible HTTP server through
`xzatoma acp serve`, but Zed launches external agents as subprocesses and
communicates over stdin/stdout using newline-delimited JSON-RPC from the
`agent-client-protocol` SDK. These are different transports with different
framing rules. The existing HTTP ACP implementation cannot satisfy Zed's
subprocess protocol without a dedicated stdio adapter.

This plan adds a new top-level `xzatoma agent` command that speaks the Zed ACP
stdio protocol while reusing XZatoma's existing provider factory, tool registry,
MCP tool bridge, skills system, storage layer, and agent execution loop. The
HTTP ACP server remains available for HTTP clients. The new command becomes the
Zed-facing subprocess entry point.

Resolved product decisions:

- The public command is `xzatoma agent`.
- Multiple prompts for the same ACP session are queued and executed in arrival
  order.
- Initial Zed support includes text and vision input. Text must work for all
  providers. Vision input must be accepted at the ACP layer and routed to
  providers that support it; unsupported provider/model combinations must return
  a clear protocol error rather than silently dropping image content.

## Current State Analysis

### Existing Infrastructure

- `Cargo.toml` already includes `tokio`, `futures`, `tokio-util`, `serde`,
  `serde_json`, `uuid`, `rusqlite`, `reqwest`, `axum`, `bytes`, and other
  foundations needed for protocol work. It does not yet include
  `agent-client-protocol`, and `tokio-util` currently needs the `compat` feature
  for `ByteStreams` over stdin/stdout.
- `src/cli.rs` defines the top-level `Commands` enum. It includes `Chat`, `Run`,
  `Watch`, `Auth`, `Models`, `History`, `Replay`, `Mcp`, `Acp`, and `Skills`,
  but not a Zed-compatible stdio agent command.
- `src/main.rs` dispatches CLI commands and initializes tracing globally. For
  ACP stdio, tracing must be forced to stderr because stdout is reserved
  exclusively for JSON-RPC frames.
- `src/acp/` contains the current ACP HTTP implementation: domain types, HTTP
  server, routes, runtime, executor, persistence integration, and Server-Sent
  Events streaming. It binds a TCP listener through Axum and does not read
  stdin.
- `src/acp/executor.rs` already demonstrates the bridge from ACP-style requests
  into XZatoma's single-agent execution path using `build_agent_environment`,
  `create_provider`, `SubagentTool`, and `Agent::execute`.
- `src/commands/environment.rs` centralizes tool registry construction, skills
  disclosure, active skill registry initialization, and MCP tool registration.
  The new stdio command should reuse this factory instead of duplicating tool
  initialization.
- `src/providers/factory.rs` exposes `create_provider_with_override`, which
  supports provider and model overrides for `copilot`, `ollama`, and `openai`.
- `src/providers/types.rs` currently models provider messages as text/tool
  messages. It does not yet provide a first-class multimodal message
  representation for images.
- `src/agent/core.rs` exposes `Agent::execute`, conversation accessors,
  transient system messages, shared-provider constructors, and conversation
  restoration constructors. It does not yet expose turn-level progress events,
  queued prompt handling, cancellation-aware execution, or multimodal prompt
  input.
- `src/agent/conversation.rs` uses `uuid::Uuid` for conversation identifiers and
  provides `Conversation::with_history` for rehydrating persisted conversations.
- `src/storage/mod.rs` persists conversations in SQLite and includes HTTP ACP
  persistence tables such as `acp_sessions`, `acp_runs`, `acp_run_events`,
  `acp_await_states`, and `acp_cancellations`. It does not yet contain a mapping
  from Zed/ACP stdio sessions or workspace roots to XZatoma conversation IDs.
- `docs/reference/acp_api.md`, `docs/reference/acp_configuration.md`, and
  `docs/how-to/run_xzatoma_as_an_acp_server.md` document the HTTP ACP server,
  not the Zed stdio protocol.
- Zed's ACP client implementation exposes IDE-side integration surfaces that
  XZatoma should explicitly support: editor-aware text-file reads and writes,
  IDE terminal creation and lifecycle operations, user permission requests, rich
  tool-call rendering with metadata, diffs, raw input, raw output, file
  locations, terminal output, available commands, session mode selection,
  session config option selection, and model selection.
- Existing demos under `demos/` cover chat, MCP, provider-specific flows, and
  watchers, but no Zed ACP subprocess integration.

### Identified Issues

- Zed expects a subprocess that reads ACP JSON-RPC requests from stdin and
  writes responses/notifications to stdout. `xzatoma acp serve` is an HTTP
  server and cannot satisfy this protocol.
- Stdout contamination will break JSON-RPC framing. All logging, status text,
  warnings, and diagnostics for `xzatoma agent` must go to stderr.
- The new stdio adapter should not replace or destabilize the HTTP ACP
  implementation. The two transports should coexist.
- XZatoma's current `Agent::execute` returns only the final response. Zed
  expects session notifications while a prompt is running, including content and
  tool-call progress.
- XZatoma has no cancellation-aware agent execution path. Zed can send
  cancellation notifications, so prompt execution must observe cancellation at
  safe boundaries.
- Zed can send multiple prompts to one session. The chosen behavior is to queue
  prompts per session and execute them in arrival order.
- Vision input must be part of initial Zed support. The current provider message
  model is text-oriented, so XZatoma needs a small multimodal input abstraction
  and provider-specific routing for image content.
- Provider/model vision support varies. The ACP layer should accept image
  content, but execution should fail clearly when the selected provider/model
  cannot process images.
- Zed exposes IDE-side capabilities to ACP agents, including reading text files,
  writing text files, creating terminals, waiting for terminal output,
  requesting permission, selecting models, selecting session modes, setting
  session config options, and rendering tool calls with diffs, terminals,
  locations, raw input, raw output, and command metadata. XZatoma needs explicit
  support for these ACP client/IDE integration surfaces instead of relying only
  on its local file and terminal tools.
- Zed's chat window can display mode selectors and config option selectors when
  an ACP agent returns `SessionModeState` and `SessionConfigOption` data from
  session creation or load/resume responses. XZatoma must expose modes such as
  planning, write, safe, and full autonomous operation through those protocol
  surfaces so the user can change XZatoma runtime behavior directly from Zed.
- Session state must survive multiple prompt requests during one subprocess
  lifetime and should resume from SQLite when Zed restarts the subprocess for
  the same workspace.
- Existing conversation IDs are UUIDs. The plan should preserve the current
  conversation storage model rather than forcing a broad migration to ULIDs.
- Model listing exists through the provider trait, but Zed stdio sessions need
  protocol-specific model advertisement and configuration mapping.
- The old plan was written for a different agent and referenced Atoma-specific
  commands, files, and structs. This plan replaces those references with
  XZatoma's actual architecture.

## Implementation Phases

### Phase 1: Dependency, CLI, and Stdio Safety Foundation

#### Task 1.1 Add ACP stdio SDK dependencies

Add `agent-client-protocol` to `Cargo.toml`, targeting version `0.11.1` unless
compatibility testing shows that a newer Zed-compatible version is required.

Update the existing `tokio-util` dependency to include the `compat` feature. The
ACP SDK's `ByteStreams` transport uses futures-compatible async IO wrappers
around Tokio stdin/stdout.

Do not remove or replace the existing HTTP ACP dependencies. The HTTP ACP server
and the Zed stdio command are separate transports.

#### Task 1.2 Add `Commands::Agent` to `src/cli.rs`

Add a top-level `Agent` command that is invoked as `xzatoma agent`.

Recommended flags:

- `--provider <provider>`: override `config.provider.provider_type`.
- `--model <model>`: override the selected provider model.
- `--allow-dangerous`: allow fully autonomous terminal execution for the
  subprocess.
- `--working-dir <path>`: optional fallback workspace root when the ACP client
  does not provide one.

The command documentation must clearly state that `xzatoma agent` speaks ACP
over stdin/stdout and is intended to be launched by Zed or another ACP stdio
client.

The documentation must also clarify that `xzatoma acp serve` remains the HTTP
ACP server.

#### Task 1.3 Create `src/commands/agent.rs`

Create a small CLI-facing command handler for `xzatoma agent`.

The command handler should:

1. Apply provider and model overrides to the effective configuration.
2. Apply `--allow-dangerous` by setting the relevant terminal/safety
   configuration for this process only.
3. Resolve and store the optional fallback working directory.
4. Delegate protocol serving to a new ACP stdio module.
5. Avoid printing anything to stdout.

Keep this file thin. The protocol implementation should live under `src/acp/`.

#### Task 1.4 Register the command

Register the new command module in `src/commands/mod.rs`.

Add a `Commands::Agent` dispatch arm in `src/main.rs` that calls the handler.

The dispatch arm must not print banners, progress text, or human-readable status
to stdout.

#### Task 1.5 Force tracing and diagnostics to stderr

Update tracing initialization in `src/main.rs` so tracing output explicitly
writes to stderr.

Audit all code paths reachable from `xzatoma agent` for `println!` or direct
stdout writes. Any human-readable diagnostics must be changed to `eprintln!` or
tracing output.

This is a protocol correctness requirement, not just a style preference.

#### Task 1.6 Add ACP stdio module boundaries

Add `src/acp/stdio.rs` or a small `src/acp/stdio/` module group.

The ACP stdio module should own:

- Transport setup.
- ACP SDK role builder setup.
- Request and notification handlers.
- Session registry.
- Prompt queue coordination.
- Conversion between ACP schema types and XZatoma internal types.

Expose only a small public entry point from this module for the CLI handler to
call.

#### Task 1.7 Testing Requirements

Extend CLI parsing tests in `src/cli.rs` to cover:

- `xzatoma agent`
- `xzatoma agent --provider ollama`
- `xzatoma agent --provider copilot --model gpt-4o`
- `xzatoma agent --provider openai --model gpt-4o --allow-dangerous`
- `xzatoma agent --working-dir /tmp/example`

Add command-level tests or integration assertions verifying that `xzatoma agent`
startup does not emit tracing or banners to stdout before JSON-RPC begins.

#### Task 1.8 Deliverables

- `Cargo.toml` includes `agent-client-protocol`.
- `tokio-util` has the `compat` feature enabled.
- `src/cli.rs` includes `Commands::Agent`.
- `src/commands/agent.rs` exists and is registered.
- `src/main.rs` dispatches `xzatoma agent`.
- Tracing writes to stderr.
- `src/acp/mod.rs` exposes the stdio module.

#### Task 1.9 Success Criteria

- `xzatoma agent` starts without binding an HTTP port.
- Stdout is reserved for ACP JSON-RPC frames.
- Existing `xzatoma acp serve` behavior remains unchanged.
- CLI tests cover the new command and flags.

### Phase 2: ACP Stdio Handshake and Session Creation

#### Task 2.1 Build the stdio transport

In the ACP stdio module, construct an `agent_client_protocol::ByteStreams`
transport using Tokio stdin/stdout compatibility wrappers.

Use the ACP SDK's agent role builder for the protocol role. Avoid naming
collisions with XZatoma's internal `crate::agent::Agent` by using clear import
aliases.

The transport must read from stdin and write newline-delimited JSON-RPC messages
to stdout.

#### Task 2.2 Implement `InitializeRequest`

Handle `agent_client_protocol::schema::InitializeRequest`.

The response should include:

- ACP protocol version compatible with Zed.
- Implementation name `xzatoma`.
- Implementation version from `env!("CARGO_PKG_VERSION")`.
- Prompt capabilities for text and vision input.
- No authentication methods for the first implementation.
- Session capabilities that accurately reflect what is implemented in the
  current phase.

Do not advertise load-session, terminal client operations, MCP-over-ACP, or
advanced file-system client capabilities until handlers exist.

#### Task 2.3 Define active session state

Create an in-memory registry of active ACP stdio sessions. The registry should
be shared across protocol handlers with simple synchronization.

Each session state should include:

- ACP session ID.
- Workspace root.
- XZatoma conversation UUID.
- Mutable XZatoma agent.
- Provider name.
- Current model name.
- Cancellation token for the currently running prompt.
- Prompt queue sender.
- Prompt worker handle or equivalent task state.
- MCP manager handle needed to keep MCP tools alive.
- Last activity timestamp in RFC 3339 format or a comparable internal timestamp.

Keep this registry simple. Do not introduce a complex actor hierarchy.

#### Task 2.4 Implement per-session prompt queues

For each session, create a FIFO prompt queue. When a `PromptRequest` arrives,
enqueue it for that session and return its response only after its turn
completes.

The queue worker must:

1. Process prompts in arrival order.
2. Hold mutable access to the XZatoma agent only while executing one prompt.
3. Preserve conversation history between prompts.
4. Respect cancellation for the currently running prompt.
5. Continue processing later prompts after a completed, failed, or cancelled
   prompt.
6. Shut down when the ACP connection closes.

If queue submission fails because the session is closing, return a protocol
error.

#### Task 2.5 Implement `NewSessionRequest`

Handle `NewSessionRequest` by resolving the workspace root from the request when
available. Fall back to `--working-dir`, then to `std::env::current_dir()`.

For Phase 2, create a fresh internal conversation and XZatoma agent for each new
session.

Use existing XZatoma infrastructure:

- `create_provider_with_override` from `src/providers/factory.rs`.
- `build_agent_environment` from `src/commands/environment.rs`.
- `Agent::new_from_shared_provider` or a conversation-aware constructor.
- Existing skill disclosure and transient system message patterns from the chat
  and run commands.
- `SubagentTool::new_with_config` if the shared environment does not already
  register it.

Return the ACP session ID and supported session metadata required by the SDK.

#### Task 2.6 Add in-memory protocol tests

Use `agent_client_protocol::Channel::duplex()` for protocol tests instead of
real stdin/stdout.

Cover:

- `InitializeRequest` returns `xzatoma` implementation metadata.
- Prompt capabilities include text and vision support.
- `NewSessionRequest` returns a non-empty session ID.
- Two new sessions create distinct session IDs.
- Unsupported methods return protocol errors rather than panicking.

#### Task 2.7 Deliverables

- ACP stdio transport setup.
- Initialize request handler.
- New session request handler.
- Active session registry.
- Per-session prompt queue scaffolding.
- In-memory protocol tests for handshake and session creation.

#### Task 2.8 Success Criteria

- Zed no longer waits indefinitely during initialization.
- `xzatoma agent` responds to ACP initialize and session creation over stdio.
- Tests do not require real network, real stdin/stdout, or provider API calls.

### Phase 3: Text and Vision Input Model

#### Task 3.1 Add internal multimodal prompt input types

Add a small internal representation for prompt input that can contain text and
images.

Recommended types:

- A prompt input struct containing ordered content parts.
- A text part containing UTF-8 text.
- An image part containing MIME type, optional filename/path, and either bytes,
  base64 data, or a resolved URL/reference depending on ACP input.
- A validation helper that rejects empty prompts and malformed image content.

Keep this abstraction focused. Do not redesign the entire provider message
system unless required.

#### Task 3.2 Convert ACP content blocks to XZatoma prompt input

Implement conversion from ACP `PromptRequest` content blocks into the internal
multimodal prompt input.

The conversion must support:

- Plain text content.
- Image content sent inline.
- Image content that references a file or resource when Zed provides a supported
  reference.
- Multiple ordered content blocks in one prompt.

The conversion must reject:

- Unsupported binary content types.
- Image content with missing media type or unreadable data.
- Resource references that cannot be resolved safely.
- Inputs exceeding configured size limits.

Error messages should be clear enough to show in Zed.

#### Task 3.3 Extend provider message handling for vision

Extend the provider layer to represent multimodal user content, or add
provider-specific request conversion that can consume the internal multimodal
prompt input.

Provider behavior should be explicit:

- Providers and models that support vision should receive image content in the
  provider's native request format.
- Providers or selected models that do not support vision should return a clear
  `XzatomaError::Provider` or protocol error before execution begins.
- Text-only prompts should continue to use the existing message flow without
  regression.

Avoid silently converting images into plain text descriptions. If an image
cannot be sent to the provider, fail clearly.

#### Task 3.4 Define vision support by provider

Document and implement initial support rules:

- OpenAI-compatible providers may support vision when the selected model
  supports image input and the API configuration can send multimodal content.
- Copilot may support vision only if the current provider implementation and
  selected Copilot model can accept image content.
- Ollama may support vision only for models and local API endpoints that support
  image input.
- If support cannot be reliably detected from the provider API, require a
  conservative allowlist or configuration flag.

The ACP initialize response can advertise that XZatoma accepts vision input, but
prompt execution must still validate the selected provider/model before sending
image content.

#### Task 3.5 Add vision-related configuration

Add configuration fields for ACP stdio vision behavior, nested under the ACP
stdio configuration.

Recommended fields:

- `vision_enabled`, default `true`.
- `max_image_bytes`, default suitable for local IDE use.
- `allowed_image_mime_types`, default including `image/png`, `image/jpeg`,
  `image/webp`, and `image/gif` if supported.
- `allow_image_file_references`, default `true`.
- `allow_remote_image_urls`, default `false` unless a provider path safely
  supports them.

Add environment variable overrides using the existing `XZATOMA_ACP_...` naming
style.

#### Task 3.6 Add tests for multimodal conversion

Add unit tests for:

- Text-only prompt conversion.
- Single image prompt conversion.
- Mixed text and image prompt conversion preserving order.
- Unsupported MIME type rejection.
- Oversized image rejection.
- Missing image data rejection.
- Vision-disabled configuration rejection.
- Provider/model without vision support returning a clear error.

Use small in-memory image bytes in tests. Do not rely on external network
images.

#### Task 3.7 Deliverables

- Internal multimodal prompt input types.
- ACP content conversion helpers.
- Provider vision support validation.
- Configuration for vision limits and policy.
- Tests for text and vision conversion.

#### Task 3.8 Success Criteria

- Zed text prompts work.
- Zed image prompts are accepted by the ACP layer.
- Vision-capable providers receive image content correctly.
- Non-vision provider/model combinations fail clearly instead of dropping
  images.

### Phase 4: Session Persistence, Resume, and Model Advertisement

#### Task 4.1 Add ACP stdio session persistence

Extend `src/storage/mod.rs` with a table dedicated to ACP stdio sessions. Use a
name such as `acp_stdio_sessions` rather than reusing HTTP `acp_sessions`,
because the HTTP runtime tables have different lifecycle semantics.

Recommended fields:

- `session_id` as text primary key.
- `workspace_root` as text and indexed.
- `conversation_id` as text referencing the existing conversation UUID.
- `provider_type` as text.
- `model` as nullable text.
- `created_at` as RFC 3339 text.
- `updated_at` as RFC 3339 text.
- `metadata_json` as text with a default empty object.

Add indexes for `workspace_root` and `updated_at`.

#### Task 4.2 Add storage methods

Add focused methods to `SqliteStorage`:

- Save or update an ACP stdio session mapping.
- Load the most recent ACP stdio session by workspace root.
- Load an ACP stdio session by session ID.
- Update last activity.
- Optionally prune old stdio sessions based on configuration.

Use existing `Result` and `XzatomaError::Storage` patterns.

#### Task 4.3 Rehydrate conversations on new sessions

Update `NewSessionRequest` handling to check for an existing
`acp_stdio_sessions` mapping for the normalized workspace root.

If a mapping exists, load the associated conversation from the existing
`conversations` table and construct the XZatoma agent with
`Conversation::with_history`.

If loading fails or the mapping points to a missing conversation, log a warning
to stderr and create a new conversation. Do not fail session creation solely
because resume is unavailable.

#### Task 4.4 Persist conversation checkpoints

After each completed prompt, save the current XZatoma conversation through
`SqliteStorage::save_conversation`.

Use the provider's current model where available.

Use the first user prompt, truncated safely, as the conversation title if the
conversation still has the default title. Preserve existing title behavior for
resumed conversations.

If a prompt fails or is cancelled after modifying conversation state, save a
checkpoint only when doing so preserves a coherent message sequence.

#### Task 4.5 Advertise available models

During session creation, call the selected provider's model listing API when
supported and map XZatoma `ModelInfo` values to ACP session model or
configuration data.

Include model metadata useful to Zed when available:

- Model ID.
- Display name.
- Context window.
- Tool support.
- Vision support.
- Streaming support.

If model listing fails due to provider authentication, local Ollama downtime, or
network errors, log a warning to stderr and continue with an empty model list or
current-model-only fallback.

#### Task 4.6 Add ACP stdio configuration

Extend `src/config.rs` with stdio-specific ACP configuration nested under the
existing `acp` section.

Recommended fields:

- `persist_sessions`, default `true`.
- `resume_by_workspace`, default `true`.
- `max_active_sessions`, default `32`.
- `session_timeout_seconds`, default `3600`.
- `prompt_queue_capacity`, default `16`.
- `model_list_timeout_seconds`, default `5`.
- `vision_enabled`, default `true`.
- `max_image_bytes`, default suitable for IDE use.
- `allowed_image_mime_types`, default common web image types.

Add environment variable overrides using names such as:

- `XZATOMA_ACP_STDIO_PERSIST_SESSIONS`
- `XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE`
- `XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS`
- `XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS`
- `XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY`
- `XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS`
- `XZATOMA_ACP_STDIO_VISION_ENABLED`
- `XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES`
- `XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES`

#### Task 4.7 Testing Requirements

Add storage unit tests for:

- Schema creation includes `acp_stdio_sessions`.
- Saving and loading a mapping by workspace root.
- Updating the mapping when a session is reused.
- Missing conversation fallback does not fail session creation.
- Last activity updates are persisted.

Add protocol integration tests for:

- Session creation persists a mapping.
- Reopening a session for the same workspace rehydrates conversation history.
- Model listing failure still returns a successful `NewSessionResponse`.
- Queue capacity errors are returned clearly.

#### Task 4.8 Deliverables

- ACP stdio session schema and storage methods.
- ACP stdio configuration fields and env overrides.
- Workspace-based resume.
- Conversation checkpointing.
- Model advertisement.
- Persistence and model tests.

#### Task 4.9 Success Criteria

- Zed restarts can reconnect to a workspace and continue the same XZatoma
  conversation.
- Provider/model overrides from CLI are reflected in session state.
- Model listing failures do not prevent Zed from opening an agent session.

### Phase 5: Zed IDE Tooling and Runtime Controls

#### Task 5.1 Review and map Zed ACP client capabilities

Review Zed's ACP client behavior and map each IDE capability to XZatoma support.

Zed currently initializes ACP agents with client capabilities for:

- Reading text files from the open project.
- Writing text files through the editor buffer system.
- Creating terminals in the IDE.
- Reading terminal output.
- Waiting for terminal exit.
- Killing and releasing terminals.
- Requesting permission from the user.
- Receiving session notifications for tool calls, diffs, terminals, mode
  changes, config option changes, available commands, and session metadata.

XZatoma should support these capabilities intentionally. The stdio agent must
prefer Zed client requests when the user is operating through Zed and should
keep local filesystem/terminal fallback behavior for non-Zed contexts only when
it is safe and explicitly configured.

#### Task 5.2 Add an IDE tool bridge abstraction

Add an IDE tool bridge layer under the ACP stdio implementation that can call
Zed-provided client methods through the active ACP connection.

The bridge should support:

- `read_text_file` for editor-aware file reads.
- `write_text_file` for editor-aware file writes and diff display.
- `create_terminal` for commands that should appear in Zed's terminal UI.
- `terminal_output` for reading terminal content.
- `wait_for_terminal_exit` for command completion.
- `kill_terminal` and `release_terminal` for cleanup.
- `request_permission` for user approval decisions in the Zed UI.

The bridge should expose a small internal interface that XZatoma tools can use
without depending directly on the ACP SDK types everywhere.

#### Task 5.3 Add Zed-aware tools to XZatoma's tool registry

Add tools that use the IDE bridge when an ACP stdio session is active.

Recommended initial tools:

- `ide_read_text_file`: read a file through Zed's current project buffers.
- `ide_write_text_file`: write a full text file through Zed so edits appear in
  the editor and are saved consistently.
- `ide_open_terminal`: create a terminal command visible in Zed.
- `ide_terminal_output`: inspect the current terminal output.
- `ide_wait_for_terminal_exit`: wait for a command to finish.
- `ide_kill_terminal`: terminate a running IDE terminal command.
- `ide_request_permission`: ask the Zed user to approve a risky operation.
- `ide_apply_diff`: emit an ACP diff/tool-call update for review before writing,
  then apply through `ide_write_text_file` when approved.

Keep the tool set generic. These are IDE transport variants of file, terminal,
permission, and diff operations, not specialized application tools.

#### Task 5.4 Route existing file and terminal operations through the IDE bridge

When `xzatoma agent` is running inside Zed, configure the agent environment so
standard file and terminal operations can use the IDE bridge.

The routing rules should be explicit:

- Planning/read-only mode can call `read_text_file` and terminal output
  inspection but must not write files or create destructive terminal commands.
- Write mode can write files through Zed after satisfying the configured safety
  policy.
- Safe mode must request permission through Zed before dangerous writes or
  commands.
- Full autonomous mode may perform allowed writes and commands without
  prompting, subject to workspace and configuration limits.
- If a Zed client capability is not advertised, fall back only when the fallback
  is safe and configured; otherwise return a clear tool error.

This routing keeps Zed's editor state, open buffers, diffs, and terminals in
sync with agent actions.

#### Task 5.5 Advertise XZatoma session modes to Zed

Return `SessionModeState` from `NewSessionResponse`, `LoadSessionResponse`, and
`ResumeSessionResponse` once those responses are implemented.

Recommended initial modes:

- `planning`: read-only analysis. No file writes and no destructive terminal
  commands.
- `write`: file edits and non-destructive terminal commands allowed subject to
  safety policy.
- `safe`: write-capable mode that requests Zed user approval for risky actions.
- `full_autonomous`: write-capable mode that skips confirmations within
  configured limits.

Choose mode IDs that are stable and lowercase. Mode names and descriptions
should be clear because Zed displays them in the mode selector.

The current mode should be derived from effective XZatoma configuration and CLI
flags at session creation time.

#### Task 5.6 Implement `SetSessionModeRequest`

Handle Zed's session mode changes by implementing `SetSessionModeRequest`.

The handler should:

1. Validate the session ID.
2. Validate the requested mode ID.
3. Update the session's runtime chat mode, safety mode, and terminal execution
   mode.
4. Rebuild or reconfigure the session tool registry if the mode changes tool
   availability.
5. Persist the selected mode in ACP stdio session metadata.
6. Send `CurrentModeUpdate` notifications when the mode changes.
7. Return a successful response or a protocol error with a clear message.

The mode change must affect subsequent queued prompts. It should not mutate a
prompt that is already running unless the implementation explicitly supports
that safely.

#### Task 5.7 Advertise session config options to Zed

Return ACP `SessionConfigOption` entries for settings the user should be able to
change from the Zed chat window.

Recommended config options:

- Provider/model selection if not fully covered by ACP model state.
- Safety policy: always confirm, confirm dangerous actions, never confirm.
- Terminal execution mode: interactive, restricted autonomous, full autonomous.
- Tool routing: prefer Zed IDE tools, prefer local tools, or require IDE tools.
- Vision input: enabled or disabled.
- Subagent delegation: enabled or disabled.
- MCP tools: enabled or disabled for the session.
- Max turns for the current session.

Use Zed's select config option UI for discrete values. Keep option IDs stable so
Zed settings can store defaults.

#### Task 5.8 Implement `SetSessionConfigOptionRequest`

Handle `SetSessionConfigOptionRequest` for every advertised config option.

The handler should:

1. Validate the session ID.
2. Validate the config option ID and selected value.
3. Update session runtime state.
4. Rebuild affected providers, tools, or prompt settings when required.
5. Persist the setting in ACP stdio session metadata.
6. Return the full updated config option list.
7. Send `ConfigOptionUpdate` notifications when options change outside the
   direct request path.

Invalid values should return protocol errors and leave the previous setting
unchanged.

#### Task 5.9 Add ACP model selection support

Use Zed's ACP model selector support in addition to the existing provider model
listing.

The stdio agent should:

- Return `SessionModelState` with available models and current model.
- Handle `SetSessionModelRequest`.
- Update the provider/model used for subsequent prompts.
- Rebuild subagent tool configuration if it depends on the provider.
- Persist selected model in session metadata.
- Reject model changes while a prompt is running unless the change can be safely
  queued for the next prompt.

This allows users to switch models from the Zed chat window without restarting
the agent subprocess.

#### Task 5.10 Emit rich ACP tool call updates for Zed rendering

Map XZatoma tool execution events to ACP `ToolCall` and `ToolCallUpdate`
notifications that Zed can render richly.

Tool call notifications should include:

- Stable tool call IDs.
- Tool kind, such as read, edit, execute, search, or other.
- Human-readable title.
- Tool name in ACP metadata using Zed's `tool_name` convention.
- Locations for file reads and writes when available.
- Diff content for file edits.
- Terminal content for IDE terminal commands.
- Raw input and raw output for debugging.
- Status transitions from pending to in progress to completed, failed, rejected,
  or cancelled.

This phase should make Zed's chat UI show XZatoma tool activity as first-class
IDE interactions instead of plain text logs.

#### Task 5.11 Support available commands in the chat window

Expose ACP available commands for XZatoma capabilities that are useful from the
Zed chat input.

Recommended commands:

- `mode`: show or change XZatoma mode.
- `model`: show or change the current model.
- `safety`: show or change the safety policy.
- `tools`: summarize available XZatoma and IDE tools.
- `context`: show current conversation context usage.
- `summarize`: summarize and compact conversation history.
- `skills`: list active skills.
- `mcp`: list connected MCP servers and tools.

Send `AvailableCommandsUpdate` notifications when available commands change due
to mode, MCP, skills, or provider changes.

#### Task 5.12 Testing Requirements

Add unit tests for:

- Mode definition conversion to ACP `SessionModeState`.
- Mode changes updating chat mode, safety mode, and terminal execution mode.
- Config option generation and validation.
- Config option updates preserving previous values on invalid input.
- Model state generation and model update validation.
- IDE tool bridge routing decisions.
- Tool call metadata, locations, raw input, raw output, diffs, and terminal
  content mapping.
- Available command generation.

Add in-memory ACP protocol tests for:

- `NewSessionRequest` returns modes, current mode, models, and config options.
- `SetSessionModeRequest` changes the current mode and emits
  `CurrentModeUpdate`.
- `SetSessionConfigOptionRequest` returns updated config options and emits
  `ConfigOptionUpdate` when appropriate.
- `SetSessionModelRequest` changes the current model.
- Zed client filesystem requests are used when IDE file tools run.
- Zed client terminal requests are used when IDE terminal tools run.
- Permission requests are sent to the client for risky operations in safe mode.
- Tool calls render as ACP tool call notifications with useful metadata.
- Available commands update when session capabilities change.

#### Task 5.13 Deliverables

- IDE tool bridge abstraction.
- Zed-aware file, terminal, permission, and diff tools.
- Tool registry integration for ACP stdio sessions.
- Session mode definitions and state.
- `SetSessionModeRequest` handler.
- Session config option definitions and state.
- `SetSessionConfigOptionRequest` handler.
- ACP model state and `SetSessionModelRequest` support.
- Rich ACP tool call notification mapping.
- Available command generation and update notifications.
- Tests for mode, config, model, IDE tools, permissions, and tool rendering.

#### Task 5.14 Success Criteria

- Zed's chat window shows a mode selector for XZatoma sessions.
- Changing mode in Zed updates XZatoma runtime behavior for subsequent prompts.
- Zed's config option controls can update safety, terminal, vision, subagent,
  MCP, and tool-routing settings.
- Zed's model selector can change the model used by later prompts.
- File reads and writes can go through Zed's editor-aware client APIs.
- Terminal commands can appear in Zed's terminal UI.
- Risky actions can request approval through the Zed UI.
- XZatoma tool activity renders as rich ACP tool calls with status, metadata,
  locations, diffs, and terminal output.
- Existing local CLI behavior remains unchanged outside `xzatoma agent`.

### Phase 6: Prompt Execution, Streaming Updates, Queueing, and Cancellation

#### Task 6.1 Add an agent execution event layer

Refactor `src/agent/core.rs` so `Agent::execute` remains available but delegates
to a lower-level execution path that can emit events.

Recommended events:

- Prompt started.
- Provider request started.
- Provider response received.
- Assistant text emitted.
- Tool call started.
- Tool call completed.
- Tool call failed.
- Vision input attached.
- Cancellation requested.
- Execution completed.
- Execution failed.

Keep existing `Agent::execute` behavior by using a no-op observer and returning
the same final response string.

#### Task 6.2 Add cancellation-aware execution

Add an execution path that accepts a `tokio_util::sync::CancellationToken`.

Check cancellation:

- Before adding a new user prompt.
- Before resolving image content.
- Before each provider completion.
- Before each tool call.
- After each awaited provider or tool operation.
- Before saving conversation state.

Use `tokio::select!` around provider and tool futures when practical so
cancellation can interrupt long awaits. If a subprocess or external tool cannot
be interrupted immediately, stop at the next safe boundary and document the
limitation.

#### Task 6.3 Implement queued `PromptRequest` handling

Handle `PromptRequest` by enqueuing work into the target session's FIFO prompt
queue.

The request should not execute concurrently with another prompt for the same
session. The response should complete when that queued prompt finishes.

The handler should:

1. Validate the session ID.
2. Convert ACP text and image content into internal multimodal prompt input.
3. Validate vision support and size limits.
4. Create a response channel for this prompt.
5. Enqueue the prompt.
6. Await the queued prompt result.
7. Return `PromptResponse` with the correct stop reason.

If the queue is full, return a protocol error explaining that the session is
busy.

#### Task 6.4 Run queued prompt workers

Each session's prompt worker should:

1. Receive queued prompt jobs in order.
2. Replace the session cancellation token for the current job.
3. Execute the XZatoma agent with event observation.
4. Send ACP session notifications through the connection as events arrive.
5. Save conversation checkpoints after successful completion.
6. Send the final prompt result back to the waiting request handler.
7. Continue to the next queued prompt even if the previous prompt failed or was
   cancelled.

The worker must keep the MCP manager handle alive while prompts execute so MCP
tools remain callable.

#### Task 6.5 Map XZatoma events to ACP session updates

Map XZatoma execution events into ACP session notifications conservatively:

- Prompt start becomes a status or progress update.
- Vision input attachment becomes a status update indicating image content was
  received.
- Provider assistant text becomes text content.
- Tool call start becomes a running tool call update.
- Tool call completion becomes a completed or failed tool call update.
- Final assistant text becomes final text content if it was not already emitted.
- Execution error becomes an error update and failed prompt response.
- Cancellation becomes a cancelled stop reason.

Avoid double-emitting final assistant text.

#### Task 6.6 Implement `CancelNotification`

Handle cancellation notifications by locating the session's current cancellation
token and calling `cancel()`.

Cancellation applies to the currently running prompt. Queued prompts remain
queued unless the ACP schema provides a prompt-specific cancellation target and
Zed supplies it.

If no prompt is currently running, record the cancellation request as a no-op
and log a debug message to stderr. Do not treat it as fatal.

#### Task 6.7 Define stop reason mapping

Map XZatoma execution outcomes to ACP stop reasons:

- Normal completion maps to end-turn or equivalent normal stop.
- User cancellation maps to cancelled.
- Max-turns exceeded maps to max-turns or error depending on SDK schema support.
- Provider/tool failure maps to error.
- Unsupported vision input maps to error.
- Queue closed maps to error.
- Queue full maps to error.

Use exact enum values from the `agent-client-protocol` schema during
implementation.

#### Task 6.8 Testing Requirements

Add unit tests for:

- ACP content-to-prompt conversion.
- Stop reason mapping.
- Cancellation token replacement per prompt.
- Queue ordering for multiple prompts.
- Queue capacity behavior.
- Observer events from the refactored agent loop using a mock provider and mock
  tool.
- Vision input event emission.

Add in-memory protocol tests for:

- Prompt request returns a response.
- Session notifications are emitted during prompt execution.
- Text prompts execute successfully.
- Vision prompts execute successfully with a mock vision-capable provider.
- Vision prompts fail clearly with a mock text-only provider.
- Unknown session prompt returns a protocol error.
- Cancellation changes the prompt response to a cancelled stop reason.
- Multiple prompts for one session complete in arrival order.

#### Task 6.9 Deliverables

- Evented agent execution.
- Cancellation-aware execution.
- FIFO prompt queue per session.
- `PromptRequest` handling.
- `CancelNotification` handling.
- ACP session notification mapping.
- Text and vision prompt tests.
- Queue ordering tests.

#### Task 6.10 Success Criteria

- Zed can send text prompts and receive visible output from XZatoma.
- Zed can send vision prompts to supported providers/models.
- Unsupported vision prompts fail clearly.
- Multiple prompts for one Zed session execute in arrival order.
- Zed cancellation stops prompt processing at supported boundaries.
- Existing `chat`, `run`, and HTTP ACP execution paths remain backward
  compatible.

### Phase 7: Documentation, Demo, and Quality Gates

#### Task 7.1 Update ACP documentation

Update `docs/reference/acp_configuration.md` to distinguish between:

- HTTP ACP server mode through `xzatoma acp serve`.
- Stdio ACP agent mode through `xzatoma agent`.

Document all new stdio ACP configuration fields and environment variable
overrides.

Update `docs/reference/acp_api.md` to clarify that it describes the HTTP ACP
API, not the stdio ACP protocol.

#### Task 7.2 Add Zed setup how-to

Create `docs/how-to/zed_acp_agent_setup.md`.

The guide should include:

- Prerequisites for XZatoma provider configuration.
- Example Zed `agent_servers` settings for `xzatoma agent`.
- Examples for Copilot, Ollama, and OpenAI provider overrides.
- Notes explaining that stdout is reserved for JSON-RPC.
- How to enable or disable vision support.
- Troubleshooting guidance for authentication, Ollama availability, corrupted
  stdout, session resume, queue backpressure, and unsupported vision models.

Use lowercase underscore filename per project rules.

#### Task 7.3 Add final implementation explanation

After implementation, create
`docs/explanation/zed_acp_agent_command_implementation.md`.

The implementation summary should cover:

- Stdio transport architecture.
- Why `xzatoma agent` is separate from `xzatoma acp serve`.
- Prompt queue behavior.
- Text and vision support.
- Session persistence and resume.
- Cancellation limitations.
- Provider/model limitations.
- Follow-up work.

Update `docs/explanation/implementations.md` with a link to the new
implementation document.

#### Task 7.4 Add a Zed ACP demo

Create `demos/zed_acp/`.

Recommended contents:

- `README.md` explaining how to configure Zed to launch XZatoma.
- Provider-specific `.yaml` configuration examples.
- A safe fixture workspace for tool testing.
- Example prompts for text-only usage.
- Example prompts for vision usage.
- Reset/setup scripts only if they follow existing demo conventions and are
  deterministic.

All YAML files must use `.yaml`. All Markdown files must use lowercase
underscore names except `README.md`.

#### Task 7.5 Update high-level project docs

Update `README.md` or the appropriate high-level documentation to mention Zed
ACP stdio integration.

Document `xzatoma agent` as a subprocess command for ACP clients, not as an
interactive user command.

Avoid overstating provider vision coverage. Document support as provider/model
dependent.

#### Task 7.6 Testing Requirements

Prefer in-memory ACP protocol tests for most coverage.

Add end-to-end subprocess tests only when they are deterministic and avoid real
provider calls.

Ensure documentation and examples use:

- `xzatoma`, not `atoma`.
- `XZatoma`, not `Atoma`.
- `.yaml`, not `.yml`.
- Lowercase underscore Markdown filenames, except `README.md`.
- No emojis.

#### Task 7.7 Quality Gates

Before considering the implementation complete, run the required project quality
gates in order:

1. `cargo fmt --all`
2. `cargo check --all-targets --all-features`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo test --all-features`

For every changed Markdown file, run:

1. `markdownlint --fix --config .markdownlint.json <file>`
2. `prettier --write --parser markdown --prose-wrap always <file>`

If a required tool is unavailable locally, document that explicitly in the final
implementation notes rather than claiming the gate passed.

#### Task 7.8 Deliverables

- `docs/reference/acp_configuration.md` updated.
- `docs/reference/acp_api.md` clarified.
- `docs/how-to/zed_acp_agent_setup.md` added.
- `docs/explanation/zed_acp_agent_command_implementation.md` added after
  implementation.
- `docs/explanation/implementations.md` updated after implementation.
- `demos/zed_acp/` added with safe examples.
- `README.md` or CLI reference mentions `xzatoma agent`.

#### Task 7.9 Success Criteria

- A user can configure Zed to launch `xzatoma agent`.
- Documentation clearly distinguishes HTTP ACP from stdio ACP.
- Demo materials cover text and vision scenarios.
- Provider/model limitations are documented honestly.
- Required Rust and Markdown quality gates pass or failures are documented with
  actionable remediation.

## Recommended Implementation Order

1. Add the dependency, CLI command, stderr tracing guarantee, and stdio module
   boundary.
2. Implement initialize and new-session handlers with in-memory protocol tests.
3. Add text and vision input conversion before prompt execution so protocol
   capability claims are truthful.
4. Add session persistence, resume, model advertisement, and ACP stdio
   configuration.
5. Add Zed IDE tool bridge, runtime modes, session config options, model
   selection, and rich tool-call rendering.
6. Add evented/cancellable agent execution and queued prompt handling.
7. Add documentation, demos, and final quality-gate cleanup.

## Summary of File Changes

### New Files

- `src/commands/agent.rs` — CLI-facing handler for `xzatoma agent`.
- `src/acp/stdio.rs` or `src/acp/stdio/mod.rs` — ACP stdio protocol
  implementation.
- `tests/acp_stdio_handshake.rs` — in-memory initialize and new-session tests.
- `tests/acp_stdio_prompt.rs` — in-memory prompt, notification, queue, vision,
  and cancellation tests.
- `tests/acp_stdio_ide_tools.rs` — in-memory tests for Zed IDE client tools,
  modes, config options, model selection, permissions, and rich tool rendering.
- `docs/how-to/zed_acp_agent_setup.md` — Zed setup guide.
- `docs/explanation/zed_acp_agent_command_implementation.md` — post-build
  implementation summary.
- `demos/zed_acp/README.md` — demo instructions.
- `demos/zed_acp/config.yaml` or provider-specific `.yaml` examples.

### Modified Files

- `Cargo.toml` — add `agent-client-protocol` and enable `tokio-util` compat.
- `src/cli.rs` — add `Commands::Agent` and parser tests.
- `src/main.rs` — dispatch `Commands::Agent` and force tracing to stderr.
- `src/commands/mod.rs` — register the new `agent` command module.
- `src/acp/mod.rs` — expose the stdio ACP module.
- `src/agent/core.rs` — add evented and cancellation-aware execution while
  preserving `Agent::execute`.
- `src/agent/conversation.rs` — update only if multimodal persistence requires
  additional helpers.
- `src/providers/types.rs` — add or adapt multimodal message/input types for
  vision.
- `src/providers/trait_mod.rs` — expose provider capability checks needed for
  vision.
- `src/providers/copilot.rs` — add vision request mapping if supported.
- `src/providers/openai.rs` — add vision request mapping if supported.
- `src/providers/ollama.rs` — add vision request mapping if supported.
- `src/tools/` — add Zed-aware IDE file, terminal, permission, and diff tools,
  or add bridge-backed variants of existing tools for ACP stdio sessions.
- `src/acp/stdio.rs` — add IDE client method bridge, session modes, config
  options, model selection handlers, available commands, and rich tool-call
  notification mapping.
- `src/storage/mod.rs` — add ACP stdio session schema and methods.
- `src/storage/types.rs` — add stored stdio session record types if useful.
- `src/config.rs` — add ACP stdio and vision configuration plus environment
  overrides.
- `docs/reference/acp_configuration.md` — document stdio ACP settings.
- `docs/reference/acp_api.md` — clarify HTTP-only scope.
- `docs/explanation/implementations.md` — link final implementation summary.
- `README.md` — mention Zed ACP stdio support at a high level.

## Open Questions

All product decisions requested for this plan are resolved:

1. The command is `xzatoma agent`.
2. Prompt concurrency is handled by a per-session FIFO queue.
3. Initial Zed support includes text and vision input.

Implementation may still need to confirm exact type and enum names from the
selected `agent-client-protocol` crate version before coding the handlers.
