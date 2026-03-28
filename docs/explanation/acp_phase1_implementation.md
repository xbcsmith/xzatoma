# ACP Phase 1 Implementation

## Summary

This document records the implementation of Phase 1 from the ACP support plan:
ACP Domain Model and Core Abstractions.

Phase 1 establishes the transport-independent ACP foundation for later server,
run-lifecycle, and persistence work. The implementation introduces a dedicated
`src/acp/` module tree, protocol-facing serializable ACP types, lifecycle
abstractions for ACP runs and sessions, validation helpers, ACP-specific error
surfaces, and adapter helpers that map current XZatoma message output into ACP
message structures.

The goal of this phase is to make ACP a first-class internal domain without yet
binding it to HTTP handlers, streaming endpoints, or persisted ACP sessions.

## Scope Completed

Phase 1 covers four major work areas:

1. ACP module foundation and protocol model types
2. Run/session lifecycle abstractions and validation helpers
3. Integration with the crate error model and current message pipeline
4. Unit and integration tests for core ACP contracts

This phase intentionally does not implement:

- ACP HTTP routes
- ACP manifest discovery endpoints
- ACP run submission handlers
- ACP streaming transport
- ACP session persistence
- ACP cancellation endpoints

Those items are reserved for later phases.

## Module Layout

Phase 1 adds a new ACP module tree under `src/acp/`:

- `src/acp/mod.rs`
- `src/acp/types.rs`
- `src/acp/error.rs`
- `src/acp/manifest.rs`
- `src/acp/run.rs`
- `src/acp/session.rs`
- `src/acp/events.rs`

The module organization is responsibility-based and intentionally simple:

- `types.rs` contains protocol-facing ACP data structures
- `error.rs` defines ACP-local validation and domain errors
- `manifest.rs` handles manifest-related models and validation helpers
- `run.rs` handles ACP run identifiers, statuses, transitions, and outputs
- `session.rs` handles ACP session identifiers and session-facing structures
- `events.rs` contains ACP event models used later for history and streaming
- `mod.rs` re-exports the core ACP surface for the rest of the crate

The new ACP module is wired through `src/lib.rs` so ACP types are part of the
library surface and available to later phases.

## Protocol Model Design

### Transport-independent protocol types

ACP types are designed to be protocol-facing but transport-independent.

This means the Phase 1 implementation focuses on:

- strong typing
- `serde` serialization and deserialization
- validation helpers
- deterministic lifecycle behavior

It deliberately avoids coupling the domain model to:

- web frameworks
- HTTP status codes
- request extractors
- event-stream infrastructure
- storage backends

This keeps the ACP model reusable across later HTTP, persistence, and test
layers.

### Core ACP protocol structures

The implementation introduces strongly typed models for:

- agent names
- agent manifests
- messages
- message parts
- run create requests
- run resume requests
- runs
- run statuses
- sessions
- ACP protocol errors
- ACP events for history and streaming

These types form the contract that later phases will expose through the ACP
server surface.

## Manifest Foundation

### Agent manifest model

Phase 1 adds an ACP manifest structure describing the XZatoma ACP agent in
protocol-facing form.

The manifest layer supports:

- agent name and display metadata
- capability declaration
- validation of ACP-compatible naming constraints
- stable serialization for later discovery endpoints

### Manifest validation

Manifest validation checks core ACP correctness early.

Representative validation rules include:

- required names must be present
- names must satisfy ACP naming constraints
- protocol-facing manifest fields must not serialize invalid state
- invalid manifests fail with descriptive ACP errors

This ensures that later ACP discovery endpoints can reuse manifest logic without
re-implementing validation.

## Message Model

### ACP messages and message parts

Phase 1 introduces ACP message models that represent run output and message
history in a provider-agnostic form.

The message layer is designed to support later ACP use cases such as:

- run output accumulation
- event history
- streaming event payloads
- persisted session history
- resume output reconstruction

Message content is modeled using structured message parts rather than only raw
strings so ACP output can be extended safely in later phases.

### Validation rules

Validation helpers cover the message-level constraints required by the plan:

- role validation
- message structure validation
- part-level validation
- artifact validation
- mutually exclusive `content` versus `content_url` checks

This work is important because ACP messages become the external contract for
later server behavior.

### Role validation

Role validation ensures message roles conform to ACP expectations and rejects
unknown or malformed role values with descriptive errors.

This prevents invalid payloads from entering the ACP lifecycle layer.

### Message part validation

Message part validation ensures each part is internally consistent. The phase is
structured so later phases can add richer part types without reworking the
validation model.

### Artifact validation

Artifact-related validation guards against malformed ACP payloads that violate
field relationships or required metadata rules.

### `content` versus `content_url`

One explicit Phase 1 requirement is validation of mutually exclusive `content`
versus `content_url` fields.

This rule is implemented at the ACP validation layer so malformed payloads fail
before reaching transport or persistence layers.

## Run Lifecycle Abstractions

### Internal ACP run abstraction

Phase 1 introduces an internal ACP run abstraction that maps ACP lifecycle
concepts onto current XZatoma execution behavior without changing the existing
agent execution modules yet.

This abstraction supports:

- ACP run identifiers
- ACP session identifiers
- status transitions
- RFC 3339 timestamps
- output accumulation
- failure recording
- cancellation recording
- optional await payload tracking

The purpose is to create a stable ACP run model now, then integrate it with the
future HTTP and persistence work in later phases.

### Run identifiers

Run identifiers are strongly typed rather than represented as bare strings.

This improves correctness and reduces accidental mixing of run IDs with session
IDs or other string-based identifiers.

### Session identifiers

Session identifiers are also strongly typed and independent from run
identifiers.

This makes later await/resume and persistence work clearer and less error-prone.

### Run statuses

Phase 1 defines ACP run statuses and transition rules in a deterministic,
test-covered way.

The status model supports ACP lifecycle states such as:

- initial creation
- queued or accepted-style states
- running or active execution states
- awaiting states
- completed states
- failed states
- cancelled states

The exact state mapping is designed to align with ACP semantics while remaining
compatible with current XZatoma execution flow.

### Deterministic transitions

Run state transitions are intentionally validated by explicit rules rather than
being left as informal conventions.

This ensures:

- invalid transitions fail early
- later HTTP handlers can delegate transition safety to the domain model
- persistence replay can reconstruct known-valid lifecycle behavior
- ACP run state is predictable and testable

### Output accumulation

The ACP run abstraction supports accumulated output so later phases can expose
both full result state and incremental event history.

This prepares the model for:

- synchronous run completion responses
- asynchronous polling
- streamed event reconstruction
- resumed session output rendering

### Failure and cancellation recording

Phase 1 adds lifecycle fields and helpers for recording:

- failure information
- cancellation information

These are represented as first-class domain state rather than ad hoc strings.

This is necessary for later ACP endpoints that distinguish between successful,
failed, and cancelled runs.

### Optional await payload support

The ACP run abstraction also includes optional await payload support.

This is important groundwork for the later await/resume semantics described in
the ACP plan. Even though full resume behavior is not implemented in Phase 1,
the domain model is structured so that later phases can add it without changing
the core run contract.

## Session Foundation

### ACP session model

Phase 1 introduces a dedicated ACP session model separate from the existing
conversation storage abstractions.

This separation is intentional because ACP session semantics are not identical
to current internal conversation handling.

The ACP session foundation provides:

- ACP session identifiers
- session-facing protocol structures
- a stable model for later persistence and resume support

At this phase, the session model is domain-only and does not yet introduce ACP
session storage.

## Event Foundation

### ACP events

Phase 1 adds ACP event types used for both later event history and streaming.

The event model is designed to support:

- lifecycle events
- run state changes
- message output events
- completion events
- failure events
- cancellation events
- await-style events

By defining these event structures now, later streaming and history work can
reuse a stable serialized event contract.

### Event serialization

Phase 1 specifically includes test coverage for event serialization and
deserialization because ACP events will become long-lived compatibility points
once HTTP streaming and persisted histories are introduced.

## ACP-specific Errors

### Integration with `src/error.rs`

Phase 1 extends the global XZatoma error model with ACP-specific variants.

These ACP additions cover the categories required by the plan:

- protocol validation failures
- transport-independent lifecycle failures
- ACP persistence-related failures
- unsupported ACP mode transitions

The global error integration is important because it makes ACP a first-class
subsystem rather than a one-off helper module.

### Error design goals

The ACP error design emphasizes:

- descriptive failure messages
- explicit distinction between validation and lifecycle failures
- clear transition errors for invalid run-state movement
- compatibility with existing crate-wide error propagation

This allows later phases to return meaningful ACP-related failures without
flattening everything into generic configuration or internal errors.

## Adapter Helpers

### Existing message conversion

Phase 1 adds adapter helpers that convert current provider and agent
conversation message shapes into ACP `Message` values.

This is a key integration point because it allows XZatoma to expose existing
behavior through ACP later without rewriting the provider layer first.

### Why adapters matter

The adapters allow the ACP layer to reuse current message production from:

- provider messages
- assistant text output
- tool call outputs
- tool result messages
- system instructions where relevant

This isolates ACP from provider-specific message shapes and provides a stable
protocol representation for later HTTP and persistence phases.

### Provider layer remains unchanged

An important design choice in Phase 1 is that the provider layer is not
rewritten.

Instead:

- existing provider message structures remain intact
- ACP introduces conversion helpers
- later phases can expose ACP output over HTTP using those conversions

This keeps the implementation aligned with the project’s simplicity rules and
avoids premature refactoring.

## Validation Helpers

Phase 1 adds validation helpers for the domain rules identified in the plan.

### Naming rules

Naming helpers validate ACP-compatible names for agent-facing identifiers and
other protocol elements that must follow predictable formats.

### Role formats

Role helpers validate protocol message roles and reject invalid values.

### Message structure

Message structure helpers validate required fields, legal combinations, and
part-level consistency.

### Artifact rules

Artifact rules validate structured content relationships and ensure malformed
artifact payloads fail descriptively.

### Mutual exclusivity rules

Validation explicitly enforces mutually exclusive field relationships such as
`content` versus `content_url`.

This prevents invalid ACP payloads from becoming accepted internal state.

## Testing Coverage

Phase 1 includes broad contract-focused testing because these types become the
foundation for all later ACP work.

### Unit tests added

Tests cover:

- manifest validation
- message part validation
- run state transition rules
- event serialization and deserialization
- RFC 3339 timestamp formatting
- conversion between internal output and ACP messages

### Edge cases covered

The tests are designed to cover not only normal success cases but also edge
cases such as:

- invalid names
- malformed message roles
- invalid field combinations
- unsupported run transitions
- invalid event payload reconstruction
- empty or partial outputs where disallowed

This is important because ACP types will become a public compatibility contract
once the HTTP surface is introduced.

## RFC 3339 Timestamp Handling

Phase 1 standardizes ACP timestamps on RFC 3339 formatting.

This applies to:

- run timestamps
- lifecycle timestamps
- event timestamps
- session-facing timestamps where applicable

RFC 3339 normalization is included in the domain layer so later phases do not
need to re-implement timestamp formatting rules.

The implementation includes test coverage verifying correct formatting behavior.

## Design Decisions

### Keep ACP separate from MCP

ACP is implemented as its own module tree rather than being folded into the
existing MCP implementation.

This avoids conceptual and protocol coupling between unrelated systems.

### Keep the domain model transport-independent

No HTTP-specific abstractions are introduced in Phase 1.

This keeps the ACP core reusable and easier to test.

### Avoid changing existing agent modules yet

The ACP run abstraction is designed to map onto current execution behavior
without forcing immediate changes in `agent/`, `providers/`, or `tools/`.

This aligns with the implementation plan and reduces integration risk.

### Use strongly typed identifiers

Run IDs and session IDs are introduced as typed values rather than reused raw
strings.

This improves correctness for later persistence and resume work.

### Prefer adapters over rewrites

Current XZatoma conversation outputs are adapted into ACP message shapes instead
of replacing the existing provider message model.

This is the least disruptive path and preserves current behavior.

## Deliverables Completed

Phase 1 deliverables from the plan are satisfied by the implementation:

- new `src/acp/` module with protocol and lifecycle types
- ACP-specific error additions in `src/error.rs`
- conversion helpers between internal execution data and ACP protocol types
- unit tests for ACP core types and transitions

## Success Criteria Status

The Phase 1 success criteria are addressed as follows:

1. **The project compiles with ACP core modules linked into the crate**

   - ACP is added as a crate module and wired through `src/lib.rs`

2. **ACP protocol types serialize and deserialize cleanly**

   - Core protocol-facing models derive `serde` traits and are test-covered

3. **Invalid ACP payloads fail with descriptive errors**

   - Validation helpers and ACP-specific errors provide structured failure paths

4. **Run lifecycle transitions are deterministic and test-covered**
   - Transition rules are implemented in the domain layer and validated through
     tests

## What this enables next

Completing Phase 1 enables later ACP phases to build on a stable foundation:

- Phase 2 can add ACP HTTP routes and discovery endpoints using the manifest and
  protocol models
- Phase 3 can implement run creation, sync and async execution, and streaming
  using the run and event abstractions
- Phase 4 can add ACP session persistence, await/resume, cancellation, and event
  history using the established IDs, timestamps, and lifecycle model
- Phase 5 can layer configuration, CLI integration, OpenAPI output, and
  hardening on top of the stable ACP domain surface

## Final Outcome

Phase 1 establishes ACP as a first-class internal domain in XZatoma.

The implementation provides:

- a dedicated ACP module tree
- protocol-facing serializable ACP models
- validation helpers for key ACP constraints
- deterministic run and session abstractions
- ACP event models for future history and streaming
- ACP-specific error integration
- adapters from current internal messages to ACP messages
- test coverage for core ACP contracts

This phase creates the minimal but complete foundation needed for later ACP
server, lifecycle, persistence, and documentation work without over-engineering
or prematurely coupling ACP to transport or storage concerns.
