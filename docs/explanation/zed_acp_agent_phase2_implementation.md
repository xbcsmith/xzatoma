# Zed ACP Agent Phase 2 Implementation

## Overview

Phase 2 adds the first working ACP stdio protocol implementation for the
`xzatoma agent` command. Zed can now launch XZatoma as an ACP subprocess and
communicate with it over newline-delimited JSON-RPC on stdin/stdout.

This phase focuses on the protocol handshake and session lifecycle foundation:

- ACP stdio transport setup.
- `initialize` request handling.
- `session/new` request handling.
- In-memory active session tracking.
- Per-session prompt queue scaffolding.
- In-memory ACP protocol tests.

Prompt execution is intentionally still minimal compared with later phases.
Phase 2 establishes the session and queue architecture that Phase 3 through
Phase 6 build on for multimodal input, persistence, richer streaming updates,
runtime controls, and cancellation.

## Implemented Files

### `Cargo.toml`

The existing `tokio-util` dependency now enables the `rt` feature so the ACP
stdio runtime can use `tokio_util::sync::CancellationToken` for session prompt
cancellation scaffolding.

### `src/acp/stdio.rs`

The Phase 1 scaffold was replaced with a real ACP stdio protocol boundary.

The module now owns:

- CLI option application and configuration validation.
- ACP stdio transport construction for production.
- Generic transport serving for in-memory tests.
- ACP initialize response generation.
- Active ACP session registry.
- ACP session state.
- New session construction.
- FIFO prompt queue scaffolding.
- Prompt worker scaffolding.
- ACP protocol tests using in-memory channels.

## ACP Stdio Transport

`run_stdio_agent` now prepares the effective runtime configuration and, in
normal builds, connects the ACP agent role to process stdin/stdout using
`agent_client_protocol::ByteStreams`.

The transport uses Tokio compatibility wrappers so ACP reads JSON-RPC messages
from stdin and writes newline-delimited JSON-RPC responses to stdout. Human
diagnostics continue to go through tracing, which was already configured in
Phase 1 to avoid stdout protocol corruption.

For testability, `run_stdio_agent_with_transport` accepts any ACP transport.
This lets tests use in-memory duplex channels instead of real stdin/stdout.

## Initialize Request

The module now handles `agent_client_protocol::schema::InitializeRequest`.

The response includes:

- The protocol version requested by the client.
- Agent implementation metadata:
  - Name: `xzatoma`
  - Version: `env!("CARGO_PKG_VERSION")`
- Prompt capabilities:
  - Text is supported as the ACP baseline.
  - Image input is advertised for upcoming vision conversion support.
  - Audio is not advertised.
- Empty authentication methods.
- Conservative session capabilities.

The response does not advertise unsupported advanced capabilities such as
session loading, MCP-over-ACP, terminal client operations, or advanced
filesystem client capabilities.

## Active Session Registry

Phase 2 introduces `ActiveSessionRegistry`, an in-memory registry shared by ACP
protocol handlers.

The registry stores active sessions in a `HashMap` keyed by ACP session ID and
guards access with Tokio synchronization primitives. It intentionally remains
simple and avoids introducing a complex actor hierarchy.

Each active session records:

- ACP session ID.
- Workspace root.
- Internal XZatoma conversation UUID.
- Mutable XZatoma agent.
- Provider name.
- Current model name.
- Cancellation token for the current prompt.
- FIFO prompt queue sender.
- Prompt worker handle.
- Optional MCP manager handle to keep MCP tools alive.
- Last activity timestamp.

The registry also exposes basic helper methods for tests and future phases,
including session count and existence checks.

## New Session Request

The module now handles `agent_client_protocol::schema::NewSessionRequest`.

When Zed creates a session, XZatoma resolves the workspace root in this order:

1. The ACP request `cwd`, when present.
2. The `xzatoma agent --working-dir` CLI fallback.
3. The process current directory.

For each new ACP session, Phase 2 creates a fresh internal XZatoma conversation
and agent. It uses the existing XZatoma infrastructure instead of duplicating
setup logic:

- `create_provider_with_override` for provider and model resolution.
- `build_agent_environment` for tools, skills, chat mode, safety mode, and MCP
  setup.
- `Agent::new_from_shared_provider` for the session agent.
- Existing mode-specific system prompt construction.
- Existing startup skill disclosure.
- `SubagentTool::new_with_config` when the environment does not already provide
  the `subagent` tool.

The ACP response returns a generated non-empty session ID. Session mode,
configuration option, model-selection, persistence, and resume metadata are left
for later phases so XZatoma does not advertise capabilities before their
handlers exist.

## Prompt Queue Scaffolding

Each session now owns a FIFO prompt queue.

The queue is represented by a Tokio channel and a prompt worker task. Prompt
requests are enqueued for their target session and processed in arrival order.

The Phase 2 worker establishes the required shape for future prompt execution:

- Prompts are received from a per-session queue.
- The session's mutable XZatoma agent is locked only while executing one prompt.
- Conversation history is preserved in the session agent.
- A cancellation token is present for current-turn cancellation wiring.
- The worker continues after completed or failed prompts.
- The queue shuts down naturally when all senders are dropped.

The current prompt conversion is intentionally conservative. Text blocks are
converted directly to prompt text. Resource links, embedded resources, images,
and audio are represented as textual placeholders. Phase 3 will replace this
with the full internal multimodal input model.

## Unsupported Method Handling

The ACP server now responds to unsupported protocol methods with JSON-RPC
method-not-found errors instead of panicking or leaving the client waiting
indefinitely.

This is important for Zed interoperability because clients may probe optional
methods or send requests for capabilities that XZatoma has not advertised.

## Tests

Phase 2 adds unit and in-memory protocol coverage in `src/acp/stdio.rs`.

The test coverage includes:

- Initialize response includes `xzatoma` implementation metadata.
- Initialize response advertises expected prompt capabilities.
- Workspace root resolution prefers an absolute ACP request path.
- ACP content blocks can be converted into conservative text prompts.
- In-memory ACP `initialize` request returns implementation metadata.
- In-memory ACP `initialize` request returns vision prompt capability.
- In-memory ACP `session/new` returns a non-empty session ID.
- Two in-memory ACP `session/new` requests return distinct session IDs.
- Unsupported methods return protocol errors.

The in-memory protocol tests use ACP duplex channels and do not require real
stdin/stdout, network access, provider API calls, or Zed itself.

## Current Limitations

Phase 2 intentionally does not complete the full Zed ACP feature set.

The following work remains for later phases:

- Full text and vision input conversion.
- Provider-specific vision capability routing.
- Session persistence and resume.
- Rich streaming session updates.
- Tool call rendering updates.
- Zed IDE file and terminal tool bridging.
- Runtime session modes.
- Runtime session configuration options.
- ACP model selection.
- Cancellation notification handling.
- Durable cleanup for closed sessions.

These limitations are intentional because the implementation plan requires
capabilities to be advertised only after their handlers exist.

## Success Criteria

Phase 2 satisfies the planned handshake and session-creation foundation:

- `xzatoma agent` has a real ACP stdio transport in normal builds.
- The agent responds to ACP `initialize`.
- The agent responds to ACP `session/new`.
- Active sessions are tracked in memory.
- Prompt queue scaffolding exists per session.
- Unsupported methods return protocol errors.
- Tests exercise the protocol without real stdin/stdout or external services.

This gives Zed a responsive ACP subprocess foundation and prepares the codebase
for the prompt execution, multimodal input, persistence, IDE tooling, and
streaming phases that follow.
