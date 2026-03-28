# ACP Phase 4 Implementation

## Summary

This document records the implementation of Phase 4 from the ACP support plan:
session persistence, await/resume, cancellation, and event history.

Phase 4 extends the earlier ACP work by adding durable backing for ACP sessions,
runs, events, await state, and cancellation audit data using the existing
SQLite-backed storage layer. It also expands the ACP HTTP surface with stateful
session and lifecycle endpoints so ACP usage can survive process restarts and
support session continuity across multiple runs.

The implementation follows the project rule to prefer extending the existing
SQLite path rather than introducing a separate datastore. ACP persistence is now
stored alongside the existing conversation history database, while still keeping
ACP data in its own tables and storage types.

## Scope Completed

Phase 4 covers five major areas:

1. durable ACP storage schema in the existing SQLite backend
2. runtime restoration of persisted sessions, runs, and event history
3. await/resume support with persisted resume state
4. cancellation handling with durable audit state
5. session lookup and stateful run history retrieval

This phase intentionally does not introduce:

- distributed execution workers
- full continuation checkpoints for arbitrary mid-tool execution
- a separate ACP-only datastore
- speculative background recovery orchestration beyond runtime restoration
- OpenAPI output or hardening work

Those remain outside the scope of Phase 4.

## Storage Strategy

### Reusing the existing SQLite backend

The ACP implementation plan explicitly recommended extending the existing SQLite
path instead of creating an unrelated second datastore.

Phase 4 follows that guidance exactly.

The existing `SqliteStorage` type in `src/storage/mod.rs` now handles both:

- existing conversation history persistence
- ACP persistence data

This keeps the system simple and avoids divergence between general
conversation/session storage and ACP state storage.

### Why this design was chosen

This approach works well because it:

- preserves one authoritative persistence backend
- reuses existing storage initialization and path logic
- avoids adding a second embedded database or key-value store
- keeps ACP state colocated with existing conversation history
- supports future mapping between conversation IDs and ACP sessions/runs

This aligns with `AGENTS.md` guidance to avoid unnecessary abstraction and avoid
introducing unrelated systems when the current backend can be extended safely.

## SQLite Schema Extensions

Phase 4 adds durable storage tables for ACP data.

### New ACP tables

The implementation adds:

- `acp_sessions`
- `acp_runs`
- `acp_run_events`
- `acp_await_states`
- `acp_cancellations`

These tables are created during normal storage initialization.

### `acp_sessions`

This table stores durable ACP session summaries and state linkage.

Representative fields include:

- `session_id`
- `conversation_id`
- `title`
- `created_at`
- `updated_at`
- `metadata_json`
- `last_run_id`

This table is the durable anchor for session-level ACP state.

### `acp_runs`

This table stores persisted ACP run state.

Representative fields include:

- `run_id`
- `session_id`
- `conversation_id`
- `mode`
- `state`
- `created_at`
- `updated_at`
- `completed_at`
- `failure_reason`
- `cancellation_reason`
- `await_kind`
- `await_detail`
- `input_json`
- `output_json`
- `metadata_json`

This makes completed, failed, cancelled, and awaiting runs queryable after
restart.

### `acp_run_events`

This table stores replayable ordered event history for each run.

Representative fields include:

- `run_id`
- `sequence`
- `kind`
- `created_at`
- `payload_json`
- `terminal`

The `(run_id, sequence)` primary key preserves monotonic event ordering and
supports deterministic replay.

### `acp_await_states`

This table stores durable pending await state for awaiting runs.

Representative fields include:

- `run_id`
- `session_id`
- `kind`
- `detail`
- `created_at`
- `updated_at`
- `resumed_at`
- `resume_payload_json`

This is the durable contract for the minimal Phase 4 await/resume model.

### `acp_cancellations`

This table stores cancellation request and audit information.

Representative fields include:

- `run_id`
- `requested_at`
- `acknowledged_at`
- `completed_at`
- `reason`
- `acknowledged`

This allows cancellation state to be queried and audited after restart.

## New Storage Types

Phase 4 adds dedicated ACP storage types in `src/storage/types.rs`.

### Added persisted type models

The implementation adds:

- `StoredAcpSession`
- `StoredAcpRun`
- `StoredAcpRunEvent`
- `StoredAcpAwaitState`
- `StoredAcpCancellation`

These types represent the durable SQLite-facing data model for ACP state.

### Why dedicated ACP storage types exist

The canonical ACP protocol types remain transport-facing and runtime-facing.
Their storage representations are intentionally separate because persistence
needs fields and serialization choices that do not exactly match the live
runtime model.

This separation keeps the code clear:

- canonical ACP types remain protocol-oriented
- storage types remain database-oriented
- runtime logic can convert between the two explicitly

This is simpler and safer than trying to force the canonical runtime types to be
stored directly without translation.

## Storage API Extensions

Phase 4 extends `SqliteStorage` with ACP-specific persistence APIs.

### ACP session storage methods

Added methods include:

- save ACP session metadata
- load ACP session metadata

This allows session lookup and restart-safe session continuity.

### ACP run storage methods

Added methods include:

- save ACP run summaries
- load ACP runs
- list ACP runs for a session
- count ACP runs for a session

These methods are used to support session-linked stateful behavior and runtime
restoration.

### ACP event storage methods

Added methods include:

- save ACP run events
- load ACP run events
- restore runtime events from persisted rows

These methods preserve replayability after restart.

### Await state storage methods

Added methods include:

- save await state
- load await state

These allow awaiting runs to survive process restarts.

### Cancellation storage methods

Added methods include:

- save cancellation state
- load cancellation state

These support durable cancellation audits and terminal-state restoration.

### Canonical run persistence helpers

The storage layer also adds helpers for:

- persisting canonical ACP runs from runtime state
- restoring canonical ACP runs back into runtime form

This makes it possible to reconstruct the in-memory runtime from SQLite after a
restart.

## Runtime Persistence Integration

### Persistent runtime initialization

Phase 3 used an in-memory runtime registry as the primary source of ACP state.
Phase 4 extends that runtime so it can initialize with optional durable storage.

At startup, the runtime now attempts to initialize the shared SQLite-backed
storage. When storage is available, the runtime can restore persisted ACP runs
and events.

### Runtime restoration

The runtime now supports restoring persisted runs and event history back into
its in-memory registry.

This means a run can be:

1. created and updated
2. persisted to SQLite
3. reloaded after restart
4. queried again through the normal runtime and HTTP paths

This satisfies the plan requirement that runs and events can be queried after
completion and after process restart.

### Durable persistence during lifecycle transitions

Phase 4 integrates persistence into runtime lifecycle behavior.

When important run lifecycle operations happen, the runtime persists the updated
state:

- run creation
- state transitions
- output accumulation
- await transitions
- resume transitions
- cancellation transitions
- terminal completion

This means the durable storage stays aligned with the current runtime view.

## Session Model and Continuity

### Session-linked runs

The implementation plan required session-linked runs so repeated calls with the
same session ID can retrieve prior history and present stateful behavior.

Phase 4 implements this by:

- accepting and persisting session IDs
- linking runs to sessions durably
- allowing session lookup through the runtime and server
- returning all associated runs for a session

### Session lookup behavior

The runtime can now retrieve a session by ACP session ID.

The server exposes this through the session endpoint so ACP clients can query
stateful session history.

### Continuity across runs

The runtime now supports listing all known runs for a given session. That list
combines:

- persisted runs restored from SQLite
- active in-memory runs already loaded into the runtime

This gives a single session-oriented view of ACP history.

### Mapping to conversation storage

The implementation plan asked for a single authoritative mapping between:

- stored conversation IDs
- ACP session IDs
- ACP run IDs
- persisted output/event history

Phase 4 introduces the required mapping fields in the storage schema:

- `conversation_id` on ACP sessions
- `conversation_id` on ACP runs

The current implementation uses the same SQLite backend and provides a clear ACP
storage namespace while leaving room to deepen the conversation-to-session
mapping later without changing backends.

This satisfies the plan’s guidance to reuse the same SQLite backend and shared
utilities while allowing a dedicated ACP namespace where necessary.

## Await/Resume Contract

### Why a constrained contract is necessary

The implementation plan correctly called out that ACP awaiting semantics are
richer than the project’s current execution model.

XZatoma’s current execution flow is not a full general-purpose resumable
continuation engine for arbitrary paused tool execution or partially completed
model interactions.

Phase 4 therefore implements a minimal but correct await/resume contract.

### Await behavior

The runtime can now move a run into the `awaiting` state and attach an await
payload containing:

- `kind`
- `detail`

This await payload is persisted durably in both:

- the canonical persisted run row
- the dedicated `acp_await_states` table

The runtime also emits a `run.awaiting` event for event history continuity.

### Resume behavior

Phase 4 adds a resume handler and runtime resume path.

The resume contract is:

- the run must exist
- the run must currently be in `awaiting`
- the resume payload must be non-null JSON
- the await payload is cleared
- the run transitions back to `running`
- a `run.resumed` event is recorded
- the resume payload is stored in runtime state and can be persisted through the
  await-state storage contract

This is intentionally constrained but correct.

### Why this is acceptable

This implementation matches the plan’s stated allowance for a documented
constrained implementation when full continuation cannot be done safely in the
first pass.

The runtime resumes from the coordinator and updates persisted state rather than
pretending it can restore arbitrary low-level execution continuation.

That is a better engineering tradeoff than claiming full resumability without a
safe continuation model.

## Cancellation Semantics

### Cancellation model

Phase 4 adds cancellation semantics through both runtime behavior and durable
storage.

The implementation plan asked for cancellation semantics that move runs through
cancellation flow and produce terminal audit state.

### Current implementation behavior

The runtime now supports cancellation requests for non-terminal runs.

Current behavior is:

- the run must exist
- the run must not already be terminal
- cancellation request metadata is stored
- the run transitions to `cancelled`
- cancellation reason is recorded
- a terminal cancellation event is emitted
- the cancellation state is persisted durably

Because the current execution model is still in-process and simple, the
implementation uses a minimal acknowledgement model rather than introducing a
complex cooperative cancellation framework too early.

This is still a valid Phase 4 implementation because it provides:

- clear state transitions
- durable cancellation records
- failure-path rejection for cancelling completed runs
- persistent terminal audit data

## HTTP Surface Additions

Phase 4 adds the remaining stateful lifecycle endpoints required by the plan.

### `POST /runs/{run_id}`

This endpoint is used for the await/resume flow.

It accepts a resume payload and delegates to runtime resume behavior.

Current behavior:

- restores the run if needed
- validates the run is awaiting
- validates the payload
- resumes the run to `running`
- returns the updated run response

### `POST /runs/{run_id}/cancel`

This endpoint cancels a run.

Current behavior:

- restores the run if needed
- rejects missing runs
- rejects terminal runs
- cancels the run
- persists cancellation state
- returns the updated run response

### `GET /sessions/{session_id}`

This endpoint loads a persisted or active session and returns session-linked run
history.

Current behavior:

- loads the session
- returns not-found for missing sessions
- returns all known runs for the session
- supports stateful ACP usage through persisted session history

## Run and Event Restoration After Restart

### Restart-safe event history

The implementation plan required event history replay after restart.

Phase 4 supports this by:

- persisting ordered runtime events
- restoring them into the runtime
- preserving sequence ordering
- making them available again through the normal run event endpoint

### Restart-safe run restoration

The runtime can restore a persisted canonical ACP run from SQLite and insert it
back into the in-memory registry.

This restored run includes:

- session linkage
- current state
- failure or cancellation metadata
- output messages
- await payload when present

### Why this matters

This is the core of durable ACP behavior.

Without restart-safe restoration, the server would only support ephemeral ACP
runs. With Phase 4, stateful ACP behavior is possible even after process
restarts.

## Testing

Phase 4 required tests for:

- session creation and retrieval
- history continuity across multiple runs in one session
- persisted run restoration
- resume of awaiting runs
- cancellation behavior for in-progress runs
- event history replay after restart
- invalid resume payloads
- cancelling completed runs
- loading missing sessions

The implementation adds coverage across storage, runtime, and HTTP-facing
integration behavior.

### Storage tests

Storage tests now cover:

- ACP table creation
- ACP session save/load round trip
- ACP run persistence and restoration
- ACP event persistence and restoration
- ACP await-state persistence
- ACP cancellation persistence
- ACP run counting for sessions
- ACP session run listing order
- persisted await payload restoration
- missing run and missing session behavior

These tests validate the new SQLite schema and durable conversion logic.

### Runtime tests

Runtime tests now cover:

- session lookup for created runs
- history continuity across multiple runs in one session
- entering the awaiting state and persisting await state
- resume transitions from awaiting to running
- invalid null resume payload rejection
- cancellation of in-progress runs
- rejection of cancelling terminal runs
- restoration of completed runs and event history
- missing session behavior

These tests validate the new Phase 4 state transitions and persistence-backed
restoration logic.

### HTTP integration tests

Phase 4 integration tests cover:

- session creation and retrieval
- continuity across multiple runs in one session
- persisted run restoration from runtime
- resume of awaiting runs
- cancellation of in-progress runs
- event history replay after restart
- invalid resume payload failure
- cancelling completed runs
- loading missing sessions

These tests validate the server-facing lifecycle behavior required by the phase.

## Deliverables Mapping

The implementation plan listed the following deliverables for Phase 4.

### Extended SQLite schema for ACP persistence

Completed.

The shared SQLite backend now includes dedicated ACP tables for sessions, runs,
events, await states, and cancellations.

### Session lookup endpoint

Completed.

`GET /sessions/{session_id}` is implemented.

### Resume and cancel handlers

Completed.

The server now supports:

- `POST /runs/{run_id}`
- `POST /runs/{run_id}/cancel`

### Awaiting run support

Completed.

The runtime now supports entering `awaiting`, persisting await state, and
resuming back to `running`.

### Persistence-backed event and run history

Completed.

Runs and event history are now stored durably and can be restored after restart.

## Success Criteria Mapping

The implementation plan defined four success criteria.

### ACP sessions survive process restarts

Satisfied.

Sessions are persisted durably in SQLite and can be reloaded by session ID.

### Runs and events can be queried after completion

Satisfied.

Runs and events are stored durably and restored into the runtime when needed.

### Await/resume and cancel flows are implemented with clear state transitions

Satisfied.

The runtime and server now support:

- `awaiting -> running` via resume
- in-progress cancellation to terminal `cancelled`
- explicit failure behavior for invalid resume or cancelling terminal runs

### Stateful ACP usage is possible through persisted session history

Satisfied.

Multiple runs may now be linked to one session and retrieved together through
the session endpoint and runtime session-run listing.

## Design Constraints and Tradeoffs

### Constrained resume instead of full continuation replay

This is the main deliberate tradeoff in Phase 4.

The implementation does not pretend to restore arbitrary low-level execution
continuation. Instead, it implements a minimal, explicit, durable await/resume
contract at the coordinator level.

That is the right tradeoff for the current architecture.

### Durable storage without new datastore complexity

Phase 4 adds durable ACP behavior while still keeping one SQLite backend. This
avoids unnecessary complexity and keeps the persistence story coherent.

### Minimal cancellation semantics

The implementation records and persists cancellation correctly and transitions
runs to terminal cancellation state. It does not introduce a more complex
multi-stage cooperative cancellation protocol than the current execution model
can safely support.

That keeps the design honest and phase-appropriate.

## Limitations and Deferred Work

Phase 4 intentionally does not yet implement:

- deep continuation checkpoints for arbitrary partial execution
- more advanced cancellation acknowledgement lifecycles
- persistent stream cursor replay semantics
- OpenAPI or hardening work
- more advanced conversation-to-session synthesis beyond the current shared
  SQLite mapping fields
- full multimodal await/resume state handling

These are reasonable deferrals for later phases.

## Conclusion

Phase 4 completes the first durable stateful ACP layer for XZatoma.

The system now has:

- persistent ACP sessions
- persistent ACP runs
- persistent ACP event history
- persisted await state
- persisted cancellation audit state
- session lookup
- await/resume handling
- cancellation handling
- event replay after restart

Most importantly, ACP usage is no longer purely in-memory and ephemeral. The ACP
runtime and HTTP surface can now support stateful, restart-safe behavior through
the shared SQLite backend, which is exactly what this phase was intended to
deliver.
