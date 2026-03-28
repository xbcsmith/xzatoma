# ACP Phase 2 Implementation

## Summary

This document records the implementation of Phase 2 from the ACP support plan:
ACP HTTP Server and Discovery Surface.

Phase 2 adds a minimal ACP HTTP transport for XZatoma using a mainstream async
Rust web stack that fits the existing Tokio runtime. The implementation exposes
the first read-only ACP discovery endpoints, introduces ACP server
configuration, wires ACP startup through a dedicated CLI command, and adds
endpoint and integration tests for the discovery surface.

The Phase 2 work keeps the ACP domain model transport-independent while adding a
small, focused HTTP layer around it. The implementation does not yet execute ACP
runs, stream output, or persist ACP sessions. Those items remain reserved for
later phases.

## Scope Completed

Phase 2 covers five major work areas:

1. ACP HTTP server bootstrap using an async Rust web framework
2. Transport-facing ACP route and handler modules
3. Read-only discovery endpoints for ping and agent manifests
4. ACP configuration and CLI/server startup integration
5. Unit and integration tests for discovery and configuration behavior

This phase intentionally does not implement:

- ACP run creation handlers
- ACP run retrieval or polling handlers
- ACP event streaming
- ACP session persistence
- ACP cancellation execution
- OpenAPI output

Those items are reserved for later phases.

## HTTP Stack Selection

### Why Axum was chosen

Phase 2 required a minimal async HTTP server dependency compatible with the
current Tokio stack and capable of handling JSON endpoints, middleware, and
future streaming responses without unnecessary complexity.

The implementation uses `axum` for the ACP server because it:

- integrates cleanly with Tokio
- keeps routing and handler code small and explicit
- supports JSON responses naturally
- supports future streaming use cases cleanly
- avoids introducing a heavy abstraction layer

The implementation also adds `tower` utility support for in-process route
testing.

This choice matches the project architecture principles from `AGENTS.md`:

- simple responsibility-based modules
- no premature abstraction
- minimal additional complexity
- clear transport separation from domain logic

## Module Layout

Phase 2 adds three transport-facing ACP modules:

- `src/acp/server.rs`
- `src/acp/routes.rs`
- `src/acp/handlers.rs`

These modules sit alongside the existing transport-independent ACP domain files
from Phase 1.

The responsibilities are intentionally simple:

- `server.rs` contains server bootstrap, route assembly, state, manifest
  generation, HTTP payloads, and handler implementations
- `routes.rs` exposes the route-building surface for callers that should depend
  on routing rather than server internals
- `handlers.rs` exposes the handler surface for ACP endpoint logic without
  requiring direct coupling to bootstrap code

The existing `src/acp/mod.rs` now exports these modules so the ACP HTTP layer is
available through the crate ACP namespace.

## Path Strategy

### Requirement

The plan required defining the ACP API under a versioned route base such as
`/api/v1/acp/...` while also evaluating whether direct ACP root-compatible paths
such as `/agents`, `/runs`, and `/ping` should be exposed by a dedicated ACP
server mode.

### Chosen strategy

The implementation supports both strategies through configuration, but defaults
to the versioned mode.

Supported modes:

- `versioned`
- `root_compatible`

The effective strategy is represented by `AcpPathStrategy`.

### Default behavior

By default, XZatoma serves ACP discovery routes under:

`/api/v1/acp`

That means the initial endpoints are:

- `GET /api/v1/acp/ping`
- `GET /api/v1/acp/agents`
- `GET /api/v1/acp/agents/{name}`

### Root-compatible mode

When configured for root-compatible ACP routing, XZatoma serves:

- `GET /ping`
- `GET /agents`
- `GET /agents/{name}`

### Why versioned mode is the default

The implementation documents and encodes the reasoning directly in manifest
metadata and route strategy behavior.

Versioned mode is the default because it:

- avoids collisions with unrelated future CLI or HTTP surfaces
- keeps ACP concerns isolated
- allows a spec-compatible mode without forcing it globally
- makes future server expansion safer as the project grows

Root-compatible mode still exists for deployments that want ACP-style direct
paths.

This satisfies the plan requirement to evaluate and document the path strategy
rather than hardcode only one behavior.

## Discovery Endpoints

Phase 2 implements the first read-only ACP endpoints.

### `GET /ping`

This endpoint returns a simple health and identity payload.

Representative response shape:

- `status`
- `service`
- `timestamp`

The server returns:

- HTTP `200 OK`
- JSON body
- RFC 3339 timestamp

The response is intentionally small and stable so clients can use it for server
liveness checks.

### `GET /agents`

This endpoint returns the currently discoverable ACP agent manifests.

The response is a list wrapper rather than a bare array so the surface can grow
without breaking clients. The response currently includes:

- `agents`
- `total`
- `offset`
- `limit`

Phase 2 only exposes one primary manifest, but the list wrapper leaves room for
future multi-agent exposure.

The endpoint also accepts optional list-shape query parameters:

- `offset`
- `limit`

This is not full pagination machinery, but it satisfies the plan requirement to
cover pagination or list shape and provides a forward-compatible response model.

### `GET /agents/{name}`

This endpoint returns a single ACP manifest when the named agent exists.

Current behavior:

- returns HTTP `200 OK` and manifest JSON for `xzatoma`
- returns HTTP `404 Not Found` with structured JSON error for unknown names

The error payload includes:

- `code`
- `message`

This gives the endpoint a stable machine-readable error shape without
introducing broader ACP transport complexity yet.

## Manifest Generation

### Primary manifest

Phase 2 builds one primary ACP manifest representing the main XZatoma autonomous
agent.

The discoverable agent name is:

- `xzatoma`

The display name is:

- `XZatoma ACP Agent`

This is intentionally a single manifest for now, but the server state stores a
manifest list so future phases can expose additional specialized ACP agents if
needed.

### Manifest metadata included

The implementation plan required manifest metadata to include:

- name
- description
- supported input/output content types
- implementation metadata
- documentation links
- language/framework information
- timestamps where appropriate

The Phase 2 manifest includes these categories.

Representative fields include:

- stable ACP name
- version
- display name
- description
- capability list
- implementation metadata
- language metadata
- framework metadata
- supported input content types
- supported output content types
- route strategy metadata
- discovery base metadata
- default run mode metadata
- manifest generation timestamp
- documentation link
- repository/homepage link

### Capability declaration

The manifest declares the ACP capabilities already modeled in Phase 1. Even
though later phases will implement the full run surface, the manifest keeps room
for that intended ACP support direction.

This is acceptable because the Phase 2 manifest is discovery-oriented and the
capability set reflects the ACP-facing design trajectory established in the
plan.

### Documentation links

The manifest includes external links for:

- documentation
- repository or homepage

These links allow ACP consumers to discover project-level reference material
outside the endpoint payload itself.

## Server State Design

The ACP server uses a small shared state structure:

- generated manifests
- effective path strategy

This state is intentionally read-only for Phase 2.

That design keeps the implementation simple:

- no persistence dependency
- no mutable run lifecycle store yet
- no locking complexity for run state
- easy in-process test setup

The state also exposes manifest lookup by agent name for the single-agent
discovery handler.

## Configuration Changes

Phase 2 adds ACP configuration to `src/config.rs` under a dedicated `acp`
section.

### Top-level ACP config

The new ACP config includes:

- `enabled`
- `host`
- `port`
- `compatibility_mode`
- `base_path`
- `default_run_mode`
- `persistence`

This satisfies the plan requirement for ACP server enablement and bind settings,
base path or compatibility mode, default run mode behavior, and optional
persistence tuning.

### Compatibility mode

Supported values:

- `versioned`
- `root_compatible`

Default:

- `versioned`

### Default bind settings

Default bind configuration:

- host: `127.0.0.1`
- port: `8765`

This makes the initial ACP server local-only by default.

### Base path

Default versioned base path:

- `/api/v1/acp`

Validation ensures that in versioned mode the base path:

- is not empty
- starts with `/`
- is not just `/`

### Default run mode

Phase 2 introduces a stable configuration location for future ACP run behavior.

Supported values:

- `sync`
- `async`
- `streaming`

Default:

- `async`

Phase 2 does not yet implement run execution through ACP, but this setting is
included now because the plan explicitly required it and later phases will need
a canonical location for default behavior.

### Persistence tuning

The ACP persistence tuning section currently includes:

- `enabled`
- `max_events_per_run`
- `max_completed_runs`

These are configuration-only in Phase 2. Persistence behavior is not yet
implemented, but validation ensures the values are structurally sensible and the
configuration surface is ready for later phases.

## Environment Variable Support

Phase 2 also adds ACP environment variable overrides.

Supported ACP environment variables include:

- `XZATOMA_ACP_ENABLED`
- `XZATOMA_ACP_HOST`
- `XZATOMA_ACP_PORT`
- `XZATOMA_ACP_COMPATIBILITY_MODE`
- `XZATOMA_ACP_BASE_PATH`
- `XZATOMA_ACP_DEFAULT_RUN_MODE`
- `XZATOMA_ACP_PERSISTENCE_ENABLED`
- `XZATOMA_ACP_MAX_EVENTS_PER_RUN`
- `XZATOMA_ACP_MAX_COMPLETED_RUNS`

This keeps ACP configuration consistent with the project’s existing config
override patterns.

## CLI and Startup Integration

### Dedicated ACP CLI command

The implementation adds a dedicated ACP CLI command rather than mixing ACP
startup into the normal run/chat modes.

The new command shape is:

- `xzatoma acp serve`

This matches the plan requirement to keep ACP concerns separate from normal CLI
task execution.

### CLI overrides

The ACP serve command supports optional runtime overrides for:

- host
- port
- base path
- root-compatible mode

This makes local experimentation and deployment easier without forcing config
file edits.

### Startup flow

The main binary now dispatches ACP server startup through a dedicated ACP
command handler module.

The ACP command handler:

1. applies CLI overrides
2. enables ACP server mode
3. validates the resulting config
4. starts the Axum server

This keeps ACP startup flow explicit and separate from the rest of the
application modes.

## Error Handling

Phase 2 keeps error handling simple and structured.

### Configuration validation

Configuration validation rejects invalid ACP settings such as:

- empty host
- invalid versioned base path
- zero persistence limits

### HTTP errors

Discovery endpoint not-found behavior uses a structured JSON error body with:

- `code`
- `message`

This avoids raw plain-text HTTP errors and gives clients a stable JSON response
shape.

### Bind address validation

The ACP bind address requires `acp.host` to parse as an IP address.

This is a conservative choice for Phase 2 that avoids hostname resolution
complexity and keeps startup validation deterministic.

## Testing

Phase 2 adds both unit coverage and integration coverage.

### Endpoint and router tests

The ACP server module includes tests covering:

- default bind address behavior
- path strategy defaults
- base path normalization
- server state manifest generation
- ping handler success
- versioned route wiring
- root-compatible route wiring
- agents list route success
- single-agent lookup success
- single-agent lookup not-found response
- required manifest metadata fields

### Integration tests

A dedicated integration test file boots the ACP router in-process and validates
REST responses for:

- `/ping` success
- `/agents` list shape
- `/agents` offset and limit handling
- `/agents/{name}` success
- `/agents/{name}` not-found behavior
- manifest JSON schema shape
- root-compatible path exposure

These tests satisfy the plan requirement to boot the HTTP server in-process and
verify REST responses.

### Configuration tests

Config tests were added for:

- ACP default values
- ACP environment variable overrides
- ACP validation failures for bad host/base path/persistence settings
- acceptance of valid root-compatible configuration

This satisfies the plan requirement for configuration loading and defaulting
coverage.

### CLI tests

CLI parsing tests were added for:

- `xzatoma acp serve`
- ACP serve override parsing

This validates the dedicated server entrypoint required by the phase.

## Deliverables Mapping

The plan listed the following deliverables for Phase 2.

### HTTP server dependency and ACP server bootstrap

Completed.

- Added Axum-based ACP server support
- Added ACP server bootstrap logic
- Added bind address resolution
- Added route construction logic

### Discovery handlers for ping and agent manifests

Completed.

- Implemented `GET /ping`
- Implemented `GET /agents`
- Implemented `GET /agents/{name}`

### ACP config entries in `src/config.rs`

Completed.

- Added ACP config section
- Added defaults
- Added validation
- Added environment variable overrides

### Initial CLI/server entrypoint for ACP mode

Completed.

- Added `acp serve` CLI command
- Added ACP command handler
- Wired startup through `main.rs`

### Integration tests for discovery endpoints

Completed.

- Added in-process ACP router integration tests
- Covered success and error cases
- Covered manifest shape and path strategies

## Success Criteria Mapping

The plan defined four success criteria.

### XZatoma can start an ACP server process

Satisfied.

A dedicated ACP server mode now exists through the CLI and server bootstrap
layer.

### ACP discovery endpoints return valid JSON responses

Satisfied.

The implemented discovery endpoints return structured JSON and are test-covered.

### At least one XZatoma agent is discoverable via ACP manifest output

Satisfied.

The primary `xzatoma` ACP manifest is discoverable through the agents endpoints.

### Configuration and startup behavior are documented and test-covered

Satisfied.

ACP config defaults, overrides, validation, CLI parsing, and route behavior are
covered by tests and documented here.

## Design Notes

### Why the implementation stays small

Phase 2 deliberately avoids building a generalized ACP application server
framework. The implementation is intentionally small because the project rules
favor simple responsibility-based modules and warn against over-engineering.

That is why the Phase 2 server design uses:

- a small shared state object
- direct handlers
- direct router construction
- simple JSON response models
- minimal transport wrappers

### Why list responses wrap manifests

The `/agents` endpoint returns a wrapper object rather than a raw JSON array.

This was chosen because it:

- supports total count reporting
- supports offset/limit echoing
- leaves room for future pagination fields
- makes testing list shape explicit

### Why one manifest is enough for now

The implementation plan explicitly said to start with one primary XZatoma agent
manifest while leaving room for future multi-agent exposure.

The Phase 2 implementation follows that guidance exactly:

- one primary manifest today
- manifest list storage for future expansion

## Limitations and Deferred Work

Phase 2 intentionally leaves several ACP features for later phases:

- no run submission yet
- no async run lifecycle handling yet
- no stream responses yet
- no ACP await/resume semantics yet
- no persistence-backed event history yet
- no cancellation execution yet
- no OpenAPI or hardening work yet

These are not omissions from the plan. They are phase boundaries defined by the
implementation roadmap.

## Conclusion

Phase 2 establishes XZatoma’s ACP discovery surface with a focused HTTP server,
read-only discovery endpoints, configurable route strategy, dedicated ACP server
startup mode, and test coverage across configuration, CLI parsing, and
in-process REST behavior.

The result is a clean bridge between the transport-independent ACP domain model
from Phase 1 and the richer ACP run lifecycle and persistence work planned for
later phases.
