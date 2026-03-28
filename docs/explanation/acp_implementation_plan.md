# ACP Support Implementation Plan

## Overview

This plan adds Agent Communication Protocol (ACP) support to XZatoma in a
phased, incremental way that fits the current Rust CLI architecture and reuses
existing conversation, storage, provider, watcher, and MCP-related foundations
where they are a good fit. The goal is to let XZatoma act as an ACP server
first, exposing one or more XZatoma agents through a versioned REST API with
sync, async, and streaming run modes, then expand into stronger session
handling, await/resume behavior, discovery metadata, and production hardening.

This plan is written to be reusable outside this repository as a general ACP
server implementation roadmap. To make that reuse practical, the structure below
distinguishes between:

- a canonical transport-independent ACP domain model
- transport adapters such as HTTP handlers and streaming endpoints
- execution adapters that map an existing agent runtime onto ACP semantics
- persistence adapters that map ACP sessions, runs, and events onto storage

The recommended implementation order is:

1. ACP canonical domain model and validation layer
2. ACP HTTP server and agent discovery endpoints
3. ACP run execution lifecycle with sync, async, and streaming modes
4. ACP session persistence, resume/cancel flows, and event history
5. ACP configuration, CLI integration, documentation, and hardening

This phased order minimizes architecture risk by establishing ACP data types,
validation rules, and state transitions before introducing HTTP transport and
long-running execution coordination. It also keeps the initial delivery aligned
with the ACP REST API surface while avoiding premature abstraction or a large
rewrite of existing agent execution code.

---

## Current State Analysis

### Existing Infrastructure

XZatoma already has several pieces that make ACP support feasible without
over-engineering:

- `src/agent/` contains the existing autonomous execution loop and conversation
  handling that can anchor ACP run execution.
- `src/commands/` already supports long-running command flows and includes
  `chat`, `run`, `watch`, and `mcp` integration points where ACP CLI operations
  can later fit naturally.
- `src/storage/mod.rs` already provides SQLite-backed persistence for
  conversations and can be extended for ACP session, run, and event storage
  rather than introducing a second unrelated persistence approach.
- `src/mcp/` already establishes a protocol integration pattern for external
  agent/tool interoperability, which is useful context when defining ACP-facing
  types and boundaries.
- `src/config.rs` already handles YAML-backed configuration with defaults and
  validation hooks, making it a suitable place to add ACP server configuration.
- `Cargo.toml` already includes `tokio`, `reqwest`, `serde`, `serde_json`,
  `rusqlite`, `chrono`, `uuid`, and `ulid`, which cover much of the async,
  serialization, persistence, and identifier groundwork needed for ACP.

### Identified Issues

The following gaps must be addressed by this plan:

1. There is no ACP module, no ACP protocol type model, and no ACP-specific error
   surface.
2. There is no HTTP server framework dependency in the project, so ACP cannot
   currently expose REST endpoints.
3. There is no internal ACP run state machine matching ACP lifecycle states such
   as `created`, `in-progress`, `awaiting`, `completed`, `cancelled`, and
   `failed`.
4. Existing storage is conversation-oriented and does not yet model ACP runs,
   sessions, events, awaiting payloads, or cancellation state.
5. There is no streaming response path for ACP `stream` mode and no event
   serialization layer for ACP event types.
6. There is no manifest/discovery layer describing XZatoma agents in ACP format.
7. There is no explicit await/resume mechanism in the ACP sense, even though the
   project has adjacent concepts such as interactive flows and persisted
   history.
8. There is no ACP-specific CLI or configuration surface to enable, bind, or
   inspect an ACP server.
9. There is no OpenAPI artifact or ACP reference documentation in the repo, even
   though the project rules require documentation and favor versioned APIs.
10. There are no ACP-focused tests yet for protocol serialization, endpoint
    behavior, run lifecycle, streaming, persistence, or recovery.

---

## Implementation Phases

### Phase 1: ACP Domain Model and Core Abstractions

#### Task 1.1 Foundation Work

Create a new `src/acp/` module tree and wire it through `src/lib.rs`. The
initial module layout should stay simple and responsibility-based, for example:

- `src/acp/mod.rs`
- `src/acp/types.rs`
- `src/acp/error.rs`
- `src/acp/manifest.rs`
- `src/acp/run.rs`
- `src/acp/session.rs`
- `src/acp/events.rs`

Use `src/acp/types.rs` as the canonical transport-independent ACP protocol
surface for Phase 1. That file should own the primary protocol-facing types
consumed by the rest of the crate and by tests. Specifically, the canonical
Phase 1 API should include:

- agent names and manifests
- messages and message parts
- run create and resume requests
- run identifiers
- session identifiers
- runs and run statuses
- sessions
- ACP errors
- ACP events used for event history and streaming

Keep these structures protocol-facing and serializable with `serde`, but avoid
binding them directly to transport concerns.

`src/acp/run.rs`, `src/acp/session.rs`, and `src/acp/events.rs` should exist as
responsibility-focused support modules, but they must not introduce a competing
public ACP type system. If they define helpers or internal building blocks, they
should either:

- support the canonical types defined in `src/acp/types.rs`, or
- re-export thin wrappers around that canonical surface

Do not maintain duplicate public run, session, or event models across multiple
ACP modules.

#### Task 1.2 Add Foundation Functionality

Introduce an internal ACP run abstraction that maps ACP lifecycle states onto
XZatoma execution behavior without changing existing agent modules yet. This
layer should define:

- run identifiers
- session identifiers
- run status transitions
- timestamps in RFC 3339 format
- output accumulation
- failure and cancellation recording
- optional await payload support

Implement these lifecycle and validation behaviors against the canonical ACP
types defined in `src/acp/types.rs`. Avoid creating a second lifecycle API in
`src/acp/run.rs` that diverges from the public ACP contract.

Add validation helpers for ACP naming rules, role formats, message structure,
artifact rules, and mutually exclusive `content` versus `content_url` fields.

Validation ownership should be explicit:

- manifest-specific validation in `src/acp/manifest.rs`
- general protocol and message validation in `src/acp/types.rs`
- ACP-local error construction in `src/acp/error.rs`

If helper functions are shared across modules, centralize them once and
re-export them from `src/acp/mod.rs` rather than duplicating equivalent logic.

#### Task 1.3 Integrate Foundation Work

Add ACP-specific error variants to `src/error.rs` so protocol validation,
transport-independent lifecycle failures, persistence failures, and unsupported
mode transitions are first-class errors.

Create adapter helpers that convert existing provider or agent conversation
message shapes into canonical ACP `Message` output so later phases can expose
current XZatoma behavior through ACP without rewriting the provider layer.

Place the primary message-conversion helpers in one canonical location. For
Phase 1, prefer `src/acp/mod.rs` or `src/acp/types.rs` as the single
crate-facing adapter surface. Do not define multiple public conversion helpers
with overlapping responsibilities in different ACP modules.

#### Task 1.4 Testing Requirements

Add unit tests covering:

- manifest validation
- message part validation
- run state transition rules
- event serialization/deserialization
- RFC 3339 timestamp formatting
- conversion between internal output and ACP messages

Target broad edge-case coverage because these types become the contract for all
later phases.

#### Task 1.5 Deliverables

- New `src/acp/` module with protocol and lifecycle types
- One canonical public ACP Phase 1 type surface, centered on `src/acp/types.rs`
- ACP-specific error additions in `src/error.rs`
- Conversion helpers between internal execution data and ACP protocol types
- Unit tests for ACP core types and transitions

#### Task 1.6 Success Criteria

- The project compiles with ACP core modules linked into the crate
- ACP protocol types serialize and deserialize cleanly
- Invalid ACP payloads fail with descriptive errors
- Run lifecycle transitions are deterministic and test-covered
- There is no duplicate public ACP domain model exposed from multiple modules

---

### Phase 2: ACP HTTP Server and Discovery Surface

#### Task 2.1 Foundation Work

Add a minimal async HTTP server dependency compatible with the current Tokio
stack. Use a mainstream Rust web framework that supports JSON endpoints,
streaming responses, and middleware without introducing unnecessary complexity.

Create transport-facing modules such as:

- `src/acp/server.rs`
- `src/acp/routes.rs`
- `src/acp/handlers.rs`

Define the ACP API under a versioned route base such as `api/v1/acp/...` while
also evaluating whether direct ACP root-compatible paths like `/agents`,
`/runs`, and `/ping` should be exposed by a dedicated ACP server mode for spec
compatibility. The implementation should document the chosen path strategy and
why.

#### Task 2.2 Add Foundation Functionality

Implement the first read-only ACP endpoints:

- `GET /ping`
- `GET /agents`
- `GET /agents/{name}`

Build manifest generation from XZatoma’s current capabilities. Start with one
primary XZatoma agent manifest that describes the main autonomous agent, then
leave room for future multi-agent exposure such as specialized run/chat/watcher
agents if needed.

Manifest metadata should include:

- name
- description
- supported input/output content types
- implementation metadata
- documentation links
- language/framework information
- timestamps where appropriate

#### Task 2.3 Integrate Foundation Work

Add ACP configuration to `src/config.rs` for server enablement and bind
settings, for example:

- enabled flag
- host
- port
- base path or compatibility mode
- default run mode behavior
- optional persistence tuning

Wire ACP server startup into `src/main.rs` and `src/cli.rs` through a dedicated
command or server mode that keeps ACP concerns separate from normal CLI task
execution.

#### Task 2.4 Testing Requirements

Add endpoint tests covering:

- `/ping` success
- `/agents` pagination or list shape
- `/agents/{name}` success and not-found responses
- manifest JSON schema shape
- configuration loading and defaulting

Add integration tests that boot the HTTP server in-process and verify REST
responses.

#### Task 2.5 Deliverables

- HTTP server dependency and ACP server bootstrap
- Discovery handlers for ping and agent manifests
- ACP config entries in `src/config.rs`
- Initial CLI/server entrypoint for ACP mode
- Integration tests for discovery endpoints

#### Task 2.6 Success Criteria

- XZatoma can start an ACP server process
- ACP discovery endpoints return valid JSON responses
- At least one XZatoma agent is discoverable via ACP manifest output
- Configuration and startup behavior are documented and test-covered

---

### Phase 3: ACP Run Lifecycle, Sync, Async, and Streaming Execution

#### Task 3.1 Foundation Work

Implement ACP run orchestration around the existing agent execution loop. Add a
coordinator layer that can create a run record, start execution, collect
outputs, and publish lifecycle events without tightly coupling HTTP handlers to
agent internals.

Recommended modules:

- `src/acp/executor.rs`
- `src/acp/runtime.rs`
- `src/acp/streaming.rs`

#### Task 3.2 Add Foundation Functionality

Implement the core run endpoints:

- `POST /runs`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/events`

Support ACP execution modes:

- `sync`: wait for completion and return final run
- `async`: create run, return accepted state, and process in background
- `stream`: stream ACP events incrementally as server-sent events

Map XZatoma execution into ACP events such as:

- `run.created`
- `run.in-progress`
- `message.created`
- `message.part`
- `message.completed`
- `run.completed`
- `run.failed`
- `error`

Keep the initial implementation focused on the current XZatoma “single agent
run” model instead of prematurely introducing distributed worker abstractions.

#### Task 3.3 Integrate Foundation Work

Create adapters from ACP input messages into the prompt/conversation structures
already used by XZatoma agent execution. Ensure the implementation can:

- flatten or preserve ordered message parts appropriately
- support text-first input cleanly
- reject unsupported multimodal cases with ACP-compliant errors until fully
  implemented
- record output messages in ACP format for both polling and event playback

Connect async run execution to Tokio tasks with a tracked in-memory registry for
active runs, while planning for persistence-backed recovery in the next phase.

#### Task 3.4 Testing Requirements

Add tests covering:

- sync run success and failure
- async run creation and status polling
- streaming event ordering
- invalid input handling
- not-found run lookups
- large output event accumulation behavior

Include regression tests for lifecycle ordering and duplicate completion
prevention.

#### Task 3.5 Deliverables

- ACP run coordinator
- Sync, async, and streaming run execution paths
- Run status and event retrieval handlers
- Integration tests for run lifecycle behavior

#### Task 3.6 Success Criteria

- ACP clients can create runs against XZatoma
- All three ACP modes behave consistently with the spec intent
- Event streams are well-ordered and replayable
- Existing XZatoma execution remains the single source of agent behavior

---

### Phase 4: Session Persistence, Await/Resume, Cancellation, and Event History

#### Task 4.1 Foundation Work

Extend the storage layer in `src/storage/mod.rs` and related types so ACP data
has durable backing. Prefer extending the existing SQLite path instead of
introducing an unrelated second datastore.

Add tables or equivalent persisted structures for:

- ACP sessions
- ACP runs
- ACP run events
- await payloads or pending resume state
- cancellation requests or terminal-state audit data

#### Task 4.2 Add Foundation Functionality

Implement the remaining core ACP lifecycle endpoints:

- `POST /runs/{run_id}`
- `POST /runs/{run_id}/cancel`
- `GET /sessions/{session_id}`

Support session-linked runs so repeated calls with the same session ID can
retrieve prior history and present stateful behavior through ACP.

Design an explicit await/resume contract that fits XZatoma’s current execution
model. Because ACP awaiting is richer than the project’s current model, this
phase should define a minimal but correct behavior, for example:

- allow internal execution to pause with an `awaiting` run state
- persist `await_request`
- accept `await_resume` payloads
- resume execution from the coordinator instead of restarting from scratch where
  practical
- fall back to a documented constrained implementation if full continuation
  cannot be done safely in the first pass

Implement cancellation semantics that move runs through `cancelling` to
`cancelled` when a background task acknowledges termination.

#### Task 4.3 Integrate Foundation Work

Link ACP sessions to existing conversation persistence where possible so session
history does not diverge from the current XZatoma storage model. Define a single
authoritative mapping between:

- stored conversation IDs
- ACP session IDs
- ACP run IDs
- persisted output/event history

If necessary, introduce a dedicated ACP storage namespace while still reusing
the same SQLite backend and shared storage utilities.

#### Task 4.4 Testing Requirements

Add tests covering:

- session creation and retrieval
- history continuity across multiple runs in one session
- persisted run restoration
- resume of awaiting runs
- cancellation behavior for in-progress runs
- event history replay after restart

Add failure-path tests for invalid resume payloads, cancelling completed runs,
and loading missing sessions.

#### Task 4.5 Deliverables

- Extended SQLite schema for ACP persistence
- Session lookup endpoint
- Resume and cancel handlers
- Awaiting run support
- Persistence-backed event and run history

#### Task 4.6 Success Criteria

- ACP sessions survive process restarts
- Runs and events can be queried after completion
- Await/resume and cancel flows are implemented with clear state transitions
- Stateful ACP usage is possible through persisted session history

---

### Phase 5: Configuration, CLI, OpenAPI, Documentation, and Hardening

#### Task 5.1 Feature Work

Finalize operator-facing ACP support with configuration, CLI ergonomics, and
documentation. Add CLI support in `src/cli.rs` and `src/commands/` for ACP
server lifecycle and introspection, such as:

- starting the ACP server
- printing effective ACP configuration
- listing active or recent ACP runs
- optionally validating ACP manifests

Add ACP configuration structures and validation rules in `src/config.rs` with
environment variable overrides where appropriate.

#### Task 5.2 Integrate Feature

Generate and maintain an OpenAPI document for the ACP-facing service in a
reference location under `docs/reference/` and document the server behavior,
configuration, and endpoint compatibility choices.

Create the mandatory implementation summary in:

- `docs/explanation/acp_implementation.md`

Also add ACP usage documentation in Diataxis-appropriate locations, likely:

- `docs/how-to/run_xzatoma_as_an_acp_server.md`
- `docs/reference/acp_api.md`
- `docs/reference/acp_configuration.md`

If the implementation exposes a server mode suitable for containerized
operation, document readiness/liveness strategy and deployment expectations.

#### Task 5.3 Configuration Updates

Update `Cargo.toml` for any new HTTP or streaming dependencies and ensure all
new public modules, structs, enums, and functions have full `///` doc comments
with runnable examples where feasible.

Document any required compatibility caveats, especially around:

- multimodal support gaps
- partial await/resume semantics
- path compatibility mode versus strict ACP root-path mode
- persistence and recovery guarantees
- authentication, if left for future work

#### Task 5.4 Testing Requirements

Add higher-level integration and functional coverage for:

- end-to-end server startup
- ACP discovery and run flows
- persistence across restart
- CLI-driven ACP server startup
- OpenAPI/reference documentation consistency where feasible

Run and fix all required project quality gates in the mandated order:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Also run markdown formatting and linting on all newly added documentation files.

#### Task 5.5 Deliverables

- ACP CLI and configuration polish
- OpenAPI/reference documentation
- Explanation document for the implementation
- End-to-end functional tests
- Quality-gate-clean ACP feature set

#### Task 5.6 Success Criteria

- ACP support is configurable, documented, and operable
- All required quality gates pass
- The project has clear ACP reference and explanation documentation
- XZatoma can be used as an ACP-compatible agent server with discoverable and
  executable agent behavior

---

## Recommended Phase Boundaries and Rationale

### Why this order works

- **Phase 1 first** establishes the ACP contract and avoids transport-driven
  design mistakes.
- **Phase 2 next** gives quick visible value through discovery endpoints without
  the complexity of long-running execution.
- **Phase 3 then** introduces the most important ACP capability: executable
  runs.
- **Phase 4 after that** adds durability and richer lifecycle semantics once the
  live run model is proven.
- **Phase 5 last** consolidates operator experience, docs, and production
  quality.

### Explicit non-goals for the first ACP delivery

To keep the implementation appropriately scoped, the first ACP release should
avoid committing to the following unless they become necessary during execution:

- multi-tenant ACP auth/authz
- distributed ACP worker orchestration
- full multimodal binary artifact generation beyond current XZatoma capabilities
- multiple independent ACP-exposed agent personas unless clearly justified
- speculative abstraction layers shared between ACP and MCP

---

## Risks and Mitigations

### Risk 1: ACP await/resume semantics may not map cleanly to current execution

**Mitigation:** Introduce a minimal coordinator-owned await boundary and
document any limitations in the first release instead of forcing deep execution
rewrites early.

### Risk 2: Streaming implementation may become tightly coupled to one web stack

**Mitigation:** Keep ACP event generation transport-agnostic and isolate SSE
serialization in a thin transport layer.

### Risk 3: Session persistence may diverge from existing conversation storage

**Mitigation:** Reuse the existing SQLite backend and define a clear mapping
between conversation, session, and run identifiers before schema expansion.

### Risk 4: ACP path compatibility may conflict with the project’s general API

versioning rule

**Mitigation:** Support a clearly documented ACP server mode, and if both
versioned and spec-native paths are needed, implement one as the canonical path
and one as a compatibility alias with tests.

### Risk 5: Scope creep from “server” and “client” ACP support in one effort

**Mitigation:** Treat ACP server support as the initial project scope. ACP
client support can be a later plan after server compatibility is stable.

---

## Final Recommended Implementation Order

1. Phase 1: ACP domain model and core abstractions
2. Phase 2: ACP HTTP server and discovery surface
3. Phase 3: ACP run lifecycle, sync, async, and streaming execution
4. Phase 4: Session persistence, await/resume, cancellation, and event history
5. Phase 5: Configuration, CLI, OpenAPI, documentation, and hardening
