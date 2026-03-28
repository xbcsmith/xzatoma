# ACP API Reference

## Overview

This document describes XZatoma's ACP-compatible HTTP API surface for server
mode operation.

The current ACP implementation supports:

- discovery endpoints
- run creation
- synchronous execution
- asynchronous execution
- streaming execution over Server-Sent Events
- session lookup
- await and resume lifecycle operations
- cancellation
- persisted run and event history

The current implementation does not yet provide:

- authentication
- multimodal artifact execution beyond current text-first behavior
- strict ACP root-path routing by default
- hard guarantees for distributed recovery beyond local persisted state

XZatoma defaults to a versioned ACP base path:

- `/api/v1/acp`

When configured for root-compatible routing, the same endpoints are exposed at
ACP-style root paths such as `/ping`, `/agents`, and `/runs`.

## Compatibility Notes

### Path compatibility mode

XZatoma supports two route layouts:

- `versioned`: serves ACP routes under a configured base path such as
  `/api/v1/acp`
- `root_compatible`: serves ACP routes at ACP-style root paths such as `/ping`

Default behavior uses `versioned` mode to avoid collisions with other CLI or
application HTTP paths.

### Await and resume semantics

Await and resume are supported for persisted runs, but compatibility should be
treated as practical rather than exhaustive. The current implementation supports
resuming runs that entered an awaiting state and persisted their await payload.

### Persistence and recovery guarantees

Runs, sessions, event history, await state, and cancellation state are persisted
to local SQLite-backed storage. This supports restart recovery on the same host
when the storage database remains available.

This is not a distributed durability guarantee.

### Authentication

Authentication is not currently enforced at the ACP HTTP layer. If you deploy
this server beyond local development or a trusted network, place it behind a
trusted reverse proxy or other network controls.

### Multimodal limitations

Current ACP execution is text-first. Artifact-shaped inputs that imply broader
multimodal execution may be rejected when not yet supported by the runtime.

## Base URL

In default configuration, example requests assume:

- `http://127.0.0.1:8765/api/v1/acp`

If root-compatible mode is enabled, remove the `/api/v1/acp` prefix from all
paths below.

## OpenAPI Summary

The following OpenAPI 3.1 document reflects the operator-facing ACP API exposed
by XZatoma.

```/dev/null/openapi.yaml#L1-390
openapi: 3.1.0
info:
  title: XZatoma ACP API
  version: 0.2.0
  summary: ACP-compatible HTTP API for XZatoma server mode
  description: |
    XZatoma exposes an ACP-compatible HTTP API for discovery, run execution,
    event replay, session lookup, resume, and cancellation.

    Default deployments use versioned routing under `/api/v1/acp`. Operators may
    optionally enable root-compatible routing for ACP-style top-level paths.

    Authentication is not currently implemented at the ACP HTTP layer.
servers:
  - url: http://127.0.0.1:8765/api/v1/acp
    description: Default versioned ACP server
  - url: http://127.0.0.1:8765
    description: Root-compatible ACP server when enabled
tags:
  - name: Discovery
  - name: Runs
  - name: Sessions
paths:
  /ping:
    get:
      tags: [Discovery]
      summary: Health check for ACP availability
      operationId: ping
      responses:
        "200":
          description: ACP server is available
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/PingResponse"
  /agents:
    get:
      tags: [Discovery]
      summary: List discoverable ACP agents
      operationId: listAgents
      parameters:
        - in: query
          name: offset
          required: false
          schema:
            type: integer
            minimum: 0
        - in: query
          name: limit
          required: false
          schema:
            type: integer
            minimum: 1
      responses:
        "200":
          description: Paginated ACP agent list
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AgentsListResponse"
  /agents/{name}:
    get:
      tags: [Discovery]
      summary: Get a specific ACP agent manifest
      operationId: getAgentByName
      parameters:
        - in: path
          name: name
          required: true
          schema:
            type: string
      responses:
        "200":
          description: ACP agent manifest
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpAgentManifest"
        "404":
          description: Agent was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
  /runs:
    post:
      tags: [Runs]
      summary: Create and execute an ACP run
      operationId: createRun
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/CreateRunRequestBody"
      responses:
        "200":
          description: Synchronous execution completed or stream established
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/CreateRunResponseBody"
            text/event-stream:
              schema:
                type: string
                description: Server-Sent Events stream for `stream` execution mode
        "202":
          description: Asynchronous run accepted
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/CreateRunResponseBody"
        "400":
          description: Invalid ACP request
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
        "404":
          description: ACP agent was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
        "500":
          description: Internal ACP execution error
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
  /runs/{runId}:
    get:
      tags: [Runs]
      summary: Get the current snapshot for a run
      operationId: getRun
      parameters:
        - in: path
          name: runId
          required: true
          schema:
            type: string
      responses:
        "200":
          description: Current run snapshot
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/RunSnapshot"
        "404":
          description: Run was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
    post:
      tags: [Runs]
      summary: Resume a run that is awaiting input
      operationId: resumeRun
      parameters:
        - in: path
          name: runId
          required: true
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/ResumeRunRequestBody"
      responses:
        "200":
          description: Resumed run state
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/CreateRunResponseBody"
        "400":
          description: Invalid resume request
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
        "404":
          description: Run was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
  /runs/{runId}/events:
    get:
      tags: [Runs]
      summary: Replay ordered event history for a run
      operationId: getRunEvents
      parameters:
        - in: path
          name: runId
          required: true
          schema:
            type: string
      responses:
        "200":
          description: Ordered runtime events for the run
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/RunEventsResponseBody"
        "404":
          description: Run was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
  /runs/{runId}/cancel:
    post:
      tags: [Runs]
      summary: Cancel a run
      operationId: cancelRun
      parameters:
        - in: path
          name: runId
          required: true
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/CancelRunRequestBody"
      responses:
        "200":
          description: Cancelled run state
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/CreateRunResponseBody"
        "400":
          description: Invalid cancellation request
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
        "404":
          description: Run was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
  /sessions/{sessionId}:
    get:
      tags: [Sessions]
      summary: Get a persisted ACP session and its runs
      operationId: getSession
      parameters:
        - in: path
          name: sessionId
          required: true
          schema:
            type: string
      responses:
        "200":
          description: Session and associated runs
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/SessionResponseBody"
        "404":
          description: Session was not found
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/AcpHttpErrorBody"
components:
  schemas:
    PingResponse:
      type: object
      required: [status, service, timestamp]
      properties:
        status:
          type: string
          example: ok
        service:
          type: string
          example: xzatoma-acp
        timestamp:
          type: string
          format: date-time
    AgentsListResponse:
      type: object
      required: [limit, offset, total, agents]
      properties:
        limit:
          type: integer
        offset:
          type: integer
        total:
          type: integer
        agents:
          type: array
          items:
            $ref: "#/components/schemas/AcpAgentManifest"
    AcpAgentManifest:
      type: object
      required:
        [name, version, displayName, description, capabilities, metadata, links]
      properties:
        name:
          type: string
        version:
          type: string
        displayName:
          type: string
        description:
          type: string
        capabilities:
          type: array
          items:
            type: string
        metadata:
          type: object
          additionalProperties: true
        links:
          type: array
          items:
            $ref: "#/components/schemas/AcpManifestLink"
    AcpManifestLink:
      type: object
      required: [rel, href]
      properties:
        rel:
          type: string
        href:
          type: string
          format: uri
        type:
          type: string
          nullable: true
        title:
          type: string
          nullable: true
    CreateRunRequestBody:
      type: object
      required: [input]
      properties:
        mode:
          type: string
          enum: [sync, async, stream]
        agentName:
          type: string
        sessionId:
          type: string
        input:
          type: array
          items:
            $ref: "#/components/schemas/AcpMessage"
    CreateRunResponseBody:
      type: object
      required: [run, mode]
      properties:
        run:
          $ref: "#/components/schemas/AcpRun"
        mode:
          type: string
          enum: [sync, async, stream]
    ResumeRunRequestBody:
      type: object
      required: [resumePayload]
      properties:
        resumePayload:
          type: object
          additionalProperties: true
    CancelRunRequestBody:
      type: object
      properties:
        reason:
          type: string
          nullable: true
    SessionResponseBody:
      type: object
      required: [session, runs]
      properties:
        session:
          allOf:
            - $ref: "#/components/schemas/AcpRunSession"
          nullable: true
        runs:
          type: array
          items:
            $ref: "#/components/schemas/AcpRun"
    RunEventsResponseBody:
      type: object
      required: [events]
      properties:
        events:
          type: array
          items:
            $ref: "#/components/schemas/AcpRuntimeEvent"
    RunSnapshot:
      type: object
      additionalProperties: true
      description: Serialized run snapshot returned by the runtime snapshot builder
    AcpRun:
      type: object
      required: [id, request, session, status, output]
      properties:
        id:
          type: string
        request:
          $ref: "#/components/schemas/AcpRunCreateRequest"
        session:
          $ref: "#/components/schemas/AcpRunSession"
        status:
          $ref: "#/components/schemas/AcpRunStatus"
        output:
          $ref: "#/components/schemas/AcpRunOutput"
    AcpRunCreateRequest:
      type: object
      required: [sessionId, input]
      properties:
        sessionId:
          type: string
        agentName:
          type: string
          nullable: true
        input:
          type: array
          items:
            $ref: "#/components/schemas/AcpMessage"
    AcpRunSession:
      type: object
      required: [id, createdAt]
      properties:
        id:
          type: string
        createdAt:
          type: string
          format: date-time
    AcpRunStatus:
      type: object
      required: [state, createdAt, updatedAt]
      properties:
        state:
          type: string
        createdAt:
          type: string
          format: date-time
        updatedAt:
          type: string
          format: date-time
    AcpRunOutput:
      type: object
      required: [messages]
      properties:
        messages:
          type: array
          items:
            $ref: "#/components/schemas/AcpMessage"
    AcpMessage:
      type: object
      required: [role, parts]
      properties:
        role:
          type: string
          enum: [system, user, assistant, tool]
        parts:
          type: array
          items:
            $ref: "#/components/schemas/AcpMessagePart"
    AcpMessagePart:
      oneOf:
        - $ref: "#/components/schemas/AcpTextMessagePart"
        - $ref: "#/components/schemas/AcpArtifactMessagePart"
    AcpTextMessagePart:
      type: object
      required: [type, data]
      properties:
        type:
          type: string
          const: text
        data:
          type: object
          required: [text]
          properties:
            text:
              type: string
    AcpArtifactMessagePart:
      type: object
      required: [type, data]
      properties:
        type:
          type: string
          const: artifact
        data:
          type: object
          additionalProperties: true
    AcpRuntimeEvent:
      type: object
      required: [sequence, event, createdAt]
      properties:
        sequence:
          type: integer
          minimum: 0
        createdAt:
          type: string
          format: date-time
        event:
          $ref: "#/components/schemas/AcpEvent"
    AcpEvent:
      type: object
      required: [kind, payload]
      properties:
        kind:
          type: string
        runId:
          type: string
          nullable: true
        payload:
          type: object
          additionalProperties: true
    AcpHttpErrorBody:
      type: object
      required: [code, message]
      properties:
        code:
          type: string
        message:
          type: string
```

## Endpoint Reference

### `GET /ping`

Returns a health-style ACP availability payload.

#### Response fields

- `status`: expected to be `ok`
- `service`: expected to be `xzatoma-acp`
- `timestamp`: RFC 3339 timestamp

### `GET /agents`

Returns the discoverable ACP manifest list.

#### Query parameters

- `offset`: optional pagination offset
- `limit`: optional pagination limit

### `GET /agents/{name}`

Returns the ACP manifest for the named agent.

Current deployments typically expose a single primary agent named `xzatoma`.

### `POST /runs`

Creates a run and executes it according to the chosen mode.

#### Supported modes

- `sync`: waits for completion and returns the terminal run
- `async`: accepts the run and allows later polling
- `stream`: returns an SSE event stream

#### Request body summary

- `mode`: optional execution mode
- `agentName`: optional target agent name
- `sessionId`: optional existing ACP session identifier
- `input`: ACP message list

### `GET /runs/{runId}`

Returns a serialized run snapshot for the specified run.

This endpoint may restore persisted runs from storage when they are no longer in
active memory.

### `POST /runs/{runId}`

Resumes an awaiting run using `resumePayload`.

### `GET /runs/{runId}/events`

Returns ordered replayable runtime events for the run.

### `POST /runs/{runId}/cancel`

Cancels the specified run.

### `GET /sessions/{sessionId}`

Returns the persisted session and associated runs.

## Streaming Behavior

When `mode` is `stream`, the server returns `text/event-stream`. Event names may
include lifecycle markers such as:

- `run.created`
- `run.in-progress`
- `message.created`
- `message.part`
- `message.completed`
- `run.completed`

Clients should treat the ordered event stream as the authoritative execution log
for streaming mode.

## Error Model

Errors are returned as JSON objects with:

- `code`
- `message`

Typical error codes include:

- `invalid_request`
- `not_found`
- `internal_error`

## Deployment Notes

For containerized or managed deployments:

- use a persistent volume if you need restart recovery
- use `/ping` or the versioned equivalent for liveness-style probing
- prefer readiness checks that confirm the HTTP listener is accepting requests
- place the service behind a reverse proxy if exposed beyond localhost
- use explicit network policy because ACP HTTP authentication is not yet built
  in

## Related Documents

- `docs/how-to/run_xzatoma_as_an_acp_server.md`
- `docs/reference/acp_configuration.md`
- `docs/explanation/acp_implementation.md`
