# ACP Phase 3 Implementation

## Summary

This document records the implementation of Phase 3 from the ACP support plan:
ACP Run Lifecycle, Sync, Async, and Streaming Execution.

Phase 3 extends the Phase 2 ACP HTTP surface by adding run lifecycle
coordination, run creation and retrieval endpoints, ordered event history, and a
streaming transport surface based on server-sent events.

The design keeps the ACP HTTP layer separate from the existing XZatoma agent
execution loop by introducing a small runtime coordination layer and an
execution bridge. The runtime owns ACP run state, ordered lifecycle events, and
live subscriptions. The executor is responsible for connecting ACP runs to the
existing single-agent XZatoma execution model.

The implementation remains intentionally simple and in-process:

- one in-memory runtime registry
- one run coordinator
- one execution bridge
- one SSE adapter for streaming
- no persistence yet
- no distributed worker abstraction
- no premature transport-generalized event bus

This follows the project architecture rules in `AGENTS.md` and preserves the
existing XZatoma agent as the single source of execution behavior.

## Scope Completed

Phase 3 covers five major work areas:

1. ACP runtime coordination around the existing agent execution model
2. Core ACP run lifecycle endpoints
3. Sync, async, and streaming execution modes
4. Ordered event recording and replay support
5. Unit and integration-oriented run lifecycle coverage

This phase intentionally does not implement:

- persistence-backed run recovery
- ACP cancellation execution
- ACP await/resume persistence
- event history storage beyond in-memory runtime state
- distributed execution workers
- OpenAPI generation or hardening

Those items remain reserved for later phases.

## New Module Layout

Phase 3 adds three runtime-facing ACP modules:

- `src/acp/runtime.rs`
- `src/acp/executor.rs`
- `src/acp/streaming.rs`

These sit alongside the existing Phase 1 domain modules and Phase 2 HTTP
transport modules.

### Responsibilities

The responsibilities are intentionally narrow and explicit.

#### `runtime.rs`

This module owns in-memory ACP lifecycle coordination:

- run creation
- run lookup
- ordered event history
- lifecycle transitions
- output accumulation
- event subscriptions
- ACP input adaptation helpers

#### `executor.rs`

This module bridges ACP runtime execution to the current XZatoma agent loop:

- chooses sync or background execution behavior
- builds the existing provider and tool stack
- runs the current single-agent agent path
- records output and terminal lifecycle updates back into the runtime

#### `streaming.rs`

This module adapts ordered runtime events to server-sent events:

- replay existing event history
- subscribe to live runtime updates
- emit ordered SSE frames
- stop the stream on terminal events

## Architecture Overview

Phase 3 needed a coordinator layer that could create runs, start execution,
collect outputs, and publish lifecycle events without tightly coupling HTTP
handlers to agent internals.

The implementation achieves that with three layers.

### 1. HTTP layer

The HTTP layer remains in `src/acp/server.rs`.

It is responsible for:

- request parsing
- path and mode routing
- mapping runtime errors to HTTP errors
- returning JSON or SSE responses

It is not responsible for execution orchestration internals.

### 2. Runtime layer

The runtime layer is the in-memory source of truth for ACP runs during Phase 3.

It stores:

- current `AcpRun`
- requested execution mode
- ordered runtime events
- terminal completion guard state
- live event broadcast sender
- flattened prompt text used by the current single-agent execution path

This keeps lifecycle ordering and replay behavior centralized.

### 3. Execution layer

The execution layer uses the runtime as its coordination backend and the
existing XZatoma agent as its execution engine.

This preserves the implementation plan requirement that existing XZatoma
execution remains the single source of agent behavior.

## Runtime Design

### In-memory registry

The runtime uses an in-memory registry keyed by run ID.

Each run record contains:

- the current `AcpRun`
- execution mode
- event history
- completion flag
- live sender
- flattened prompt

This registry is wrapped behind shared synchronized state so handlers and
background tasks can safely interact with the same run.

### Why in-memory only for Phase 3

The implementation plan explicitly says to connect async run execution to Tokio
tasks with a tracked in-memory registry for active runs while planning for
persistence-backed recovery in the next phase.

That is exactly what this phase does.

This keeps the implementation simple and avoids introducing persistence logic
before Phase 4.

### Run and session creation

Run creation validates the incoming ACP request, derives a run ID and session ID
when needed, creates the canonical `AcpRun`, stores the record, and emits the
initial `run.created` event.

This means every run starts with:

- a canonical run record
- an ordered first event
- replayable event history
- a live event sender for future streaming

## Execution Modes

Phase 3 required support for three ACP execution modes:

- `sync`
- `async`
- `stream`

The implementation models these through `AcpRuntimeExecuteMode`.

### Sync mode

In sync mode, the server waits for execution to complete and returns the final
run state.

Intended behavior:

- create the run
- execute immediately
- record ordered events
- append output
- mark terminal success or failure
- return the final run payload

### Async mode

In async mode, the server accepts the run and returns immediately while
execution continues in the background.

Intended behavior:

- create the run
- record initial lifecycle events
- spawn a Tokio task
- return accepted state
- allow status polling and event playback through later requests

### Stream mode

In stream mode, the server exposes incremental lifecycle events over SSE.

Intended behavior:

- create the run
- begin execution
- replay any already-recorded events
- stream new events incrementally
- terminate the stream once a terminal event is reached

The streaming layer is built so that event ordering remains consistent with the
polling and replay surfaces.

## Core Run Endpoints

Phase 3 required these endpoints:

- `POST /runs`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/events`

These are implemented in the ACP server surface.

### `POST /runs`

This endpoint accepts ordered ACP input messages plus an optional execution
mode.

The endpoint supports:

- sync creation and completion response
- async creation with accepted status behavior
- stream creation with SSE response behavior

It also validates requested agent name when provided and rejects unknown ACP
agents early.

### `GET /runs/{run_id}`

This endpoint returns the current run snapshot.

The snapshot includes current lifecycle state and related metadata so ACP
clients can poll for progress or completion.

### `GET /runs/{run_id}/events`

This endpoint returns the stored ordered runtime events for the run.

This is important for replayability and aligns with the implementation plan
requirement that event streams be well-ordered and replayable.

## Input Adaptation

The plan required adapters from ACP input messages into the prompt and
conversation structures already used by XZatoma agent execution.

Phase 3 implements that through text-first adaptation helpers.

### Flattening to prompt text

The current XZatoma agent execution path is prompt-oriented, so the runtime
provides a helper that flattens ordered ACP messages into a single text prompt.

The flattening behavior:

- preserves message ordering
- preserves sender role labels
- joins text parts deterministically
- rejects unsupported non-text content

This keeps the current single-agent execution model intact while still allowing
ACP request input to map into it.

### Provider message adaptation

Phase 3 also introduces ACP-to-provider message conversion helpers.

These preserve:

- ordering
- ACP roles
- text content

Unsupported artifact and multimodal cases are rejected with ACP-oriented errors.

### Unsupported multimodal handling

The plan required rejecting unsupported multimodal cases with ACP-compliant
errors until fully implemented.

Phase 3 does this directly in validation and adaptation.

Current behavior:

- text parts are supported
- artifact parts in ACP run input are rejected
- artifact-only or multimodal inputs fail validation with ACP-oriented errors

This is deliberate and matches the plan.

## Event Model Mapping

The implementation plan requested mapping XZatoma execution into ACP events such
as:

- `run.created`
- `run.in-progress`
- `message.created`
- `message.part`
- `message.completed`
- `run.completed`
- `run.failed`
- `error`

Phase 3 records those lifecycle-facing event names inside ordered runtime
events.

### Event ordering

Each runtime event receives a per-run monotonic sequence number.

This makes event playback deterministic for:

- polling consumers
- history retrieval
- SSE replay
- regression testing

### Event emission stages

Typical successful lifecycle order is:

1. `run.created`
2. `run.in-progress` for queued
3. `run.in-progress` for running
4. `message.created`
5. one or more `message.part`
6. `message.completed`
7. `run.completed`

Typical failure lifecycle order is:

1. `run.created`
2. `run.in-progress`
3. `run.in-progress`
4. optional `error`
5. `run.failed`

### Duplicate completion prevention

The runtime tracks whether terminal completion has already been recorded.

Once a run has been completed or failed, later terminal lifecycle recording is
rejected.

This satisfies the implementation plan requirement for regression coverage
around duplicate completion prevention.

## Output Recording

The plan required recording output messages in ACP format for both polling and
event playback.

Phase 3 does this through runtime output accumulation.

When an output message is appended:

- the canonical ACP output is stored in the run
- a `message.created` event is emitted
- one `message.part` event is emitted per text part
- a `message.completed` event is emitted

This means the same output information is available through:

- current run state
- event history replay
- streaming SSE transport

That consistency is one of the core goals of the runtime design.

## Streaming Design

### Why SSE

The implementation plan explicitly called for streaming ACP events incrementally
as server-sent events.

SSE is a good fit here because:

- the transport is one-way server-to-client
- event ordering is important
- the web framework already supports it cleanly
- replay plus live subscription semantics are easy to model

### Replay then live streaming

The streaming adapter first replays previously recorded events in order.

Then it switches to the live subscription stream.

This matters because a client may connect after some lifecycle events have
already occurred, but it still needs a consistent ordered view of the run.

### Stream termination

The streaming adapter terminates once a terminal runtime event is observed.

That means a completed or failed run produces a naturally bounded event stream.

### Keep-alives

The SSE surface uses keep-alive comments to reduce the risk of quiet streams
being treated as dead connections by intermediaries or clients.

## Executor Design

The implementation plan called for a coordinator around the existing agent
execution loop and explicitly warned against tightly coupling handlers to agent
internals.

The executor is the bridge that satisfies that requirement.

### What the executor does

The executor:

- reads prompt input from the runtime
- constructs the normal provider and tool stack
- builds MCP integrations when configured
- registers subagent support just like normal execution paths
- invokes the current `Agent` execution flow
- records output and terminal lifecycle state back into the runtime

### What the executor does not do

The executor does not:

- own HTTP request or response logic
- own persistent state
- own distributed scheduling
- replace the agent loop with a custom ACP-specific execution engine

This preserves the project’s simplicity constraints.

## Tool and Provider Reuse

Phase 3 intentionally reuses the existing XZatoma runtime behavior rather than
creating a special ACP-only execution path.

That reuse includes:

- provider construction through existing provider factories
- normal tool registry construction
- MCP tool registration when configured
- skill activation support
- subagent tool registration
- the current `Agent` execution loop

This is important because the implementation plan explicitly requires that the
existing XZatoma execution remain the single source of agent behavior.

## Error Handling

Phase 3 keeps error handling explicit and ACP-oriented.

### Validation errors

Invalid ACP input fails early when:

- input is empty
- message parts are unsupported
- flattened content would be empty
- agent name is empty or unknown
- unsupported execution mode is requested

These are surfaced as ACP validation-style failures.

### Runtime errors

Runtime not-found and lifecycle errors cover cases such as:

- missing run ID
- invalid lifecycle transitions
- attempts to modify terminal runs
- duplicate completion attempts
- poisoned runtime lock conditions

### Streaming errors

The streaming module wraps streaming-specific failures with ACP lifecycle-style
wording so they fit the current crate error model.

### HTTP error mapping

The server maps runtime and validation failures into HTTP responses using stable
JSON error payloads.

This keeps the transport behavior predictable for ACP clients.

## Testing

Phase 3 required tests for:

- sync run success and failure
- async run creation and status polling
- streaming event ordering
- invalid input handling
- not-found run lookups
- large output event accumulation behavior
- lifecycle ordering regression
- duplicate completion prevention

The implementation adds test coverage across runtime, executor, and streaming
modules.

### Runtime tests

Runtime tests cover:

- execution mode parsing
- empty input rejection
- ordered prompt flattening
- ACP-to-provider message mapping
- initial run creation event
- lifecycle ordering across queued, running, output, and completion
- duplicate completion prevention
- not-found run lookup behavior
- large output event accumulation
- non-terminal error event recording
- run snapshot field shape

### Executor tests

Executor tests cover:

- async execution returning accepted state
- background spawn behavior
- missing run rejection
- runtime handle exposure

### Streaming tests

Streaming tests cover:

- SSE event conversion
- streaming response creation
- replay ordering
- live terminal stream behavior
- payload event name preference
- ACP streaming error wrapping

### Regression intent

The tests specifically protect against:

- terminal completion being emitted twice
- output not being replayable
- event sequence disorder
- not-found runs silently succeeding
- unsupported ACP content being accepted unexpectedly

## Deliverables Mapping

The Phase 3 implementation plan listed the following deliverables.

### ACP run coordinator

Completed.

This is implemented through the in-memory runtime in `src/acp/runtime.rs`.

### Sync, async, and streaming run execution paths

Completed at the architectural level.

The implementation introduces all three execution modes, the execution bridge,
and SSE streaming support.

### Run status and event retrieval handlers

Completed.

These are implemented through:

- `POST /runs`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/events`

### Integration tests for run lifecycle behavior

Completed at the module and runtime level, with lifecycle ordering, event
history, and stream behavior covered in tests.

## Success Criteria Mapping

The implementation plan defined four success criteria.

### ACP clients can create runs against XZatoma

Satisfied.

The ACP server now exposes run creation through `POST /runs`.

### All three ACP modes behave consistently with the spec intent

Satisfied in the Phase 3 scope.

The server and runtime now distinguish between:

- synchronous completion
- asynchronous accepted execution
- streaming event transport

### Event streams are well-ordered and replayable

Satisfied.

The runtime stores ordered per-run events with monotonic sequence numbers and
the streaming surface replays history before switching to live events.

### Existing XZatoma execution remains the single source of agent behavior

Satisfied.

The executor reuses the existing provider, tool, MCP, skills, subagent, and
agent execution stack rather than replacing it.

## Design Constraints and Tradeoffs

### Why no persistence yet

Persistence belongs to Phase 4. Adding it here would have violated the planned
phase boundaries and increased complexity too early.

### Why no distributed worker abstraction

The implementation plan explicitly says to avoid prematurely introducing
distributed worker abstractions. Phase 3 follows that rule and keeps everything
in-process.

### Why text-first input only

The current XZatoma execution model is text-first. Rather than pretending full
multimodal support exists, Phase 3 rejects unsupported multimodal ACP input
cleanly and explicitly.

### Why ordered runtime history matters

Without a replayable ordered runtime history, the polling, event retrieval, and
streaming surfaces would diverge. Phase 3 centralizes this in the runtime so all
three views remain consistent.

## Limitations and Deferred Work

Phase 3 intentionally leaves some areas for later phases:

- session persistence
- await/resume persistence
- cancellation execution
- persisted event history
- persistence-backed server recovery
- full multimodal ACP input support
- more advanced stream resumption semantics
- distributed execution

These are deferred by design, not omitted accidentally.

## Conclusion

Phase 3 turns the ACP server from a discovery-only surface into a run-capable
ACP endpoint set with lifecycle coordination, ordered event history, sync/async
execution modes, and replayable streaming event transport.

The implementation stays faithful to the XZatoma architecture principles:

- simple responsibility-based modules
- no premature abstraction
- reuse of the existing execution engine
- clean separation between transport, runtime coordination, and execution

That makes Phase 3 a strong foundation for Phase 4 persistence, await/resume,
cancellation, and event history durability.
