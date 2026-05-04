# Zed ACP agent command implementation

This document describes the architecture and key decisions behind
`xzatoma agent`, XZatoma's ACP stdio subprocess mode for Zed and other
ACP-compatible IDE clients.

## What was built

`xzatoma agent` is a subprocess command that speaks the Agent Client Protocol
(ACP) over stdin/stdout. Zed launches it as a child process and communicates
using newline-delimited JSON-RPC. The command was implemented across seven
phases tracked in `docs/explanation/zed_acp_agent_command_plan.md`.

## Why `xzatoma agent` is separate from `xzatoma acp serve`

`xzatoma acp serve` is an HTTP server intended for networked or REST-based
clients. It binds a TCP port and is long-lived.

`xzatoma agent` is a stdio subprocess intended for a single IDE client. It reads
from stdin, writes to stdout, and exits when the parent closes the pipes.
Keeping them separate means:

- the stdio transport boundary is explicit and cannot accidentally write HTTP
  response framing or diagnostic banners to stdout
- the HTTP server can continue running for programmatic access while the IDE
  uses the subprocess channel
- each mode has a focused configuration surface

## Stdio transport architecture

The stdio agent is built on the `agent-client-protocol` Rust SDK
(`agent-client-protocol = "0.11.1"` with the `unstable_session_model` feature).
The SDK provides a typed JSON-RPC builder that dispatches incoming messages to
registered handler closures.

The main entry points are:

- `src/commands/agent.rs` -- CLI dispatch; applies CLI flags and calls
  `run_stdio_agent`
- `src/acp/stdio.rs` -- all stdio protocol logic: initialization handshake,
  session creation, prompt handling, session modes, config options, model
  selection, IDE tool bridge, and cancellation

All tracing and diagnostic output is forced to stderr before the stdio server
starts. This ensures no non-JSON bytes reach stdout.

## Session lifecycle

Each ACP session created by Zed has its own isolated state:

1. Zed sends `InitializeRequest`. XZatoma responds with its name, protocol
   version, and capability advertisement (text and vision prompts).
2. Zed sends `NewSessionRequest` with a workspace path. XZatoma creates a fresh
   XZatoma agent instance with a loaded tool registry, optionally rehydrates the
   most recent conversation for that workspace, and starts a background prompt
   worker task.
3. Zed sends `PromptRequest` messages. Each prompt is enqueued in the session's
   FIFO queue and processed serially by the worker.
4. Zed can send `CancelNotification` at any time to interrupt the current
   prompt.

## Prompt queue behavior

Each session has an `mpsc` channel bounded by `acp.stdio.prompt_queue_capacity`
(default 8). Prompt requests are enqueued by the handler and the handler awaits
a response channel. The worker processes prompts one at a time, holding the
agent mutex for the duration of each prompt. This serializes all prompts within
a session.

When the queue is full, `enqueue_prompt` returns a protocol error immediately.
The queue never drops a prompt silently.

## Agent execution events and live notifications

The agent execution loop emits `AgentExecutionEvent` values via the
`AgentObserver` trait (defined in `src/agent/events.rs`). The
`AcpSessionObserver` in `src/acp/stdio.rs` maps these events to ACP
`SessionNotification` messages sent over the active connection:

- `AssistantTextEmitted` becomes an `AgentMessageChunk` notification.
- `ToolCallStarted` becomes a `ToolCall` notification (with kind, title, and
  file locations).
- `ToolCallCompleted` becomes a `ToolCallUpdate` (Completed status).
- `ToolCallFailed` becomes a `ToolCallUpdate` (Failed status).
- `ExecutionCompleted` emits a final text chunk only if no text was already
  streamed.

This gives the Zed agent panel live visibility into what XZatoma is doing.

## Text and vision support

ACP prompt content blocks are converted to XZatoma's internal
`MultimodalPromptInput` type by `acp_content_blocks_to_prompt_input` in
`src/acp/prompt_input.rs`. This function handles:

- plain text blocks
- inline base64 image blocks
- resource-link blocks referencing local file paths
- embedded resource blocks with blob data

Vision support is gated by `acp.stdio.vision_enabled` and by per-provider model
capability checks. If a text-only model receives an image prompt, XZatoma
returns a clear error rather than silently dropping the image.

## Session persistence and resume

When `acp.stdio.persist_sessions` is true, XZatoma writes session metadata and
conversation history to SQLite. On the next `NewSessionRequest` for the same
workspace path, the most recent conversation is rehydrated into the new agent
instance. This gives Zed persistent conversation context across editor restarts.

Persistence is best-effort. Storage failures are logged to stderr but do not
abort session creation.

## Cancellation limitations

The `CancelNotification` handler cancels the current prompt's
`CancellationToken`. The agent loop checks the token:

- before adding the user prompt
- at each loop iteration
- using `tokio::select!` around each provider completion call
- using `tokio::select!` around each tool execution call

Subprocess-based tools (file operations, terminal commands) cannot be
interrupted at an arbitrary byte boundary. Cancellation is observed only when
the tool call returns to the executor. If a terminal command is running, it will
complete before the cancellation is processed.

Cancellation applies only to the currently running prompt. Queued prompts that
have not started receive a fresh token and are unaffected.

## Session modes

XZatoma advertises session modes to Zed during session creation. The available
modes are:

| Mode ID           | Behavior                                                 |
| ----------------- | -------------------------------------------------------- |
| `planning`        | Read-only analysis; no file writes or terminal execution |
| `write`           | File reads and writes; no terminal execution             |
| `full_autonomous` | Unrestricted tool use including terminal commands        |

Zed can switch the active mode at any time via `SetSessionModeRequest`.

## IDE tool bridge

When Zed advertises IDE capabilities during `InitializeRequest`, XZatoma
registers IDE-native tool variants (read file, write file, terminal) that route
operations through the Zed client rather than the local filesystem. This lets
XZatoma work correctly when Zed is editing remote files over SSH.

## Provider and model limitations

- Vision support is model-dependent. Not all Copilot, OpenAI, or Ollama models
  support image input.
- GitHub Copilot model availability changes based on subscription tier. Vision
  capability cannot be determined without attempting a completion.
- Ollama vision models must be pulled before they can be used.
- Model listing is attempted during session creation with a configurable
  timeout. If listing fails, XZatoma falls back to advertising the current model
  only.

## Follow-up work

- Streaming text output: the current implementation emits full assistant
  messages rather than token-by-token streaming. Future work could add a
  streaming observer that emits multiple `AgentMessageChunk` notifications per
  turn.
- Prompt-specific cancellation: the ACP schema may add prompt-level cancellation
  identifiers in a future revision. The current implementation cancels by
  session.
- Distributed session state: sessions are in-memory per process. Multi-process
  deployments (load balancers, container restarts) cannot share session state.

## Related documentation

- `docs/reference/acp_configuration.md` -- configuration field reference
- `docs/how-to/zed_acp_agent_setup.md` -- Zed setup instructions
- `demos/zed_acp/README.md` -- demo with example prompts
- `docs/explanation/phase6_prompt_execution_implementation.md` -- Phase 6
  evented execution and cancellation details
