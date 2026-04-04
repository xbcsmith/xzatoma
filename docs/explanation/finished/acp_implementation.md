# ACP implementation summary

## Overview

This document summarizes the ACP implementation delivered for XZatoma across the
ACP phases, with Phase 5 focused on operator-facing usability, configuration,
CLI ergonomics, reference documentation, and hardening.

The ACP support turns XZatoma into an ACP-compatible agent server with:

- discovery endpoints
- run creation and lifecycle tracking
- streaming event delivery
- session persistence and restoration
- await, resume, and cancellation flows
- configurable routing and persistence behavior
- CLI support for ACP server operation and inspection

## What was implemented

### ACP domain and lifecycle model

XZatoma now includes a dedicated ACP module that defines the transport-neutral
ACP model:

- messages and message parts
- roles
- artifacts
- manifests and capabilities
- runs, run status, and output
- sessions
- runtime events
- await and resume payloads

This gives the project a stable ACP core that is independent of HTTP concerns
and reusable across runtime, persistence, and transport layers.

### ACP HTTP server

The ACP HTTP surface is implemented with `axum` and exposes:

- `GET /ping`
- `GET /agents`
- `GET /agents/{name}`
- `POST /runs`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/events`
- `POST /runs/{run_id}`
- `POST /runs/{run_id}/cancel`
- `GET /sessions/{session_id}`

The server supports two route layouts:

- versioned mode under a configurable base path such as `/api/v1/acp`
- root-compatible mode exposing ACP-style root paths such as `/ping`

The versioned mode is the default because it avoids path collisions with other
application surfaces.

### Run execution modes

The runtime and executor support three operator-visible execution modes:

- `sync`
- `async`
- `stream`

`sync` returns the completed run when execution finishes.

`async` accepts the run and allows clients to poll run status and event history.

`stream` returns ordered SSE event output so clients can consume lifecycle and
message events in real time.

### Persistence and recovery

ACP state is backed by the existing SQLite storage layer. Persisted state
includes:

- sessions
- runs
- events
- await state
- cancellation metadata

This allows ACP sessions and runs to survive process restarts and enables:

- run restoration
- event replay
- session lookup
- session-scoped run history
- resume and cancellation after persistence

## Phase 5 additions

Phase 5 focused on making ACP operable and understandable for users and
operators.

### Configuration support

ACP configuration lives in `src/config.rs` under the `acp` section and supports:

- `enabled`
- `host`
- `port`
- `compatibility_mode`
- `base_path`
- `default_run_mode`
- persistence tuning

Environment variable overrides are supported for ACP fields, including host,
port, routing mode, base path, default run mode, and persistence-related
settings.

Validation was added so invalid ACP configuration fails fast, including checks
for:

- empty host values
- invalid or missing base path configuration
- invalid persistence limits
- compatibility mode specific path requirements

### CLI support

ACP CLI support was extended to improve server lifecycle and introspection
operations. The ACP command surface now covers:

- starting the ACP server
- printing effective ACP configuration
- listing active or recent ACP runs
- validating ACP configuration
- optionally validating an ACP manifest file

This makes ACP usable without needing to inspect internal code or storage
manually.

### OpenAPI and reference documentation

Phase 5 also requires reference-facing documentation for the ACP service,
including an OpenAPI description and ACP reference material under
`docs/reference/`.

The implementation is documented alongside operator guidance and configuration
reference so users can understand:

- what endpoints are exposed
- how routing compatibility works
- how to configure the ACP server
- what persistence guarantees exist
- which compatibility caveats still apply

### Hardening

Hardening work focused on making the ACP surface more production-usable:

- more configuration validation
- better CLI ergonomics
- persistence-aware inspection paths
- stronger integration coverage
- explicit compatibility caveats in documentation

## Key design choices

### Versioned routes by default

XZatoma defaults to versioned ACP routes rather than root ACP paths. This was
chosen to avoid collisions with existing or future CLI and API surfaces while
still allowing root-compatible mode when deployments require it.

### Reuse of existing storage infrastructure

Rather than introducing a second persistence system, ACP persistence was added
to the existing SQLite-backed storage layer. This keeps the architecture simple
and consistent with the project’s general design principles.

### Transport-independent ACP core

The ACP model is intentionally not coupled to HTTP. This separation keeps the
ACP domain reusable and makes the server layer easier to evolve without
rewriting lifecycle logic.

### Minimal abstraction

The implementation follows the project rule to avoid over-engineering. The ACP
feature is composed of focused modules for:

- domain types
- manifests
- runtime coordination
- execution
- HTTP handlers and routes
- persistence integration
- streaming

## Compatibility caveats

The ACP implementation is intentionally practical and not over-claimed. Current
caveats include:

- multimodal gaps may remain for some ACP artifact flows
- await and resume semantics are implemented, but some advanced future ACP
  patterns may still be partial
- routing compatibility is configurable, and root-compatible mode should be
  chosen deliberately
- persistence guarantees are tied to the SQLite storage behavior used by the
  runtime
- authentication for the ACP HTTP surface may remain a future enhancement
  depending on deployment mode

These caveats are documented explicitly so operators know what is available
today and what remains future work.

## Testing and quality gates

The ACP work includes unit, integration, and functional coverage across:

- discovery endpoints
- run lifecycle behavior
- streaming flows
- persistence and restart restoration
- session retrieval
- await, resume, and cancellation
- CLI parsing and ACP command behavior

Phase 5 also requires the full project quality gates to pass:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

New Markdown documentation is also expected to be linted and formatted with the
project’s required Markdown tooling.

## Deliverables summary

The ACP implementation now provides:

- ACP CLI and configuration polish
- ACP discovery and execution endpoints
- session persistence and lifecycle recovery
- OpenAPI and ACP reference documentation
- explanation documentation
- end-to-end and integration test coverage
- a hardened, configurable ACP server mode for XZatoma

## Outcome

With this implementation, XZatoma can operate as an ACP-compatible agent server
with discoverable agent metadata, executable runs, event streaming, persisted
sessions, and operator-facing configuration and documentation.

This phase completes the work needed to make ACP support not just implemented,
but usable, inspectable, and maintainable.
