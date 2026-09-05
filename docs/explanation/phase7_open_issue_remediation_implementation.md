# Phase 7: Open-Issue Remediation Implementation

## Overview

Phase 7 closes the residual partial items surfaced by a post-implementation
audit of Phases 1-6: the URL-open confirmation gate and Kafka payload size bound
left partial in Phase 1 (M1, L2), the two approximated failure-path tests from
Phase 2, the missing IDE-permission wire-level tests from Phase 1, and the
dangling documentation links left after Phase 4's archival. No new features were
added -- this phase is hardening, completeness, and test fidelity only, per the
plan's own framing.

All work in this document is complete. Every quality gate in `AGENTS.md` passes:
`cargo fmt --all`, `cargo check --all-targets --all-features`,
`cargo clippy --all-targets --all-features -- -D warnings`, and
`cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`.

## M1 - URL open confirmation gate

`src/mcp/elicitation.rs` already enforced the `https` scheme allowlist from
Phase 1, but `handle_url` printed the URL to stderr and opened it without an
approval gate. A `UrlOpenConfirmer` trait was added, with a production
`StdinUrlConfirmer` (prompts on stderr, reads a `y`/`yes` confirmation line from
stdin, fails closed on EOF, an empty answer, or any I/O error) and a testable
`handle_url_with_confirmer` entry point that takes the confirmer as an
injectable dependency. `handle_url` delegates to it using `StdinUrlConfirmer`.
The existing headless fail-closed branch is unchanged and still short-circuits
before the confirmer or browser opener is ever consulted.

Tests drive `handle_url_with_confirmer` with fake confirmers
(`RecordingApprovingConfirmer`, `RecordingDecliningConfirmer`,
`PanickingConfirmer`) and a recording browser opener, proving: the browser opens
only after an approving confirmation and receives the full URL; a declined
confirmation returns `Decline` and never opens; and headless / non-`https` paths
never consult the confirmer or opener at all (enforced by `PanickingConfirmer` /
a panicking opener).

## L2 - Kafka payload size bound

A configurable maximum payload size is now enforced before an untrusted Kafka
message reaches either watcher backend's parser:

- `src/watcher/kafka_security.rs` gained `DEFAULT_MAX_PAYLOAD_BYTES` (1 MiB) and
  `validate_payload_size(payload, max_bytes) -> Result<()>`, the single shared
  enforcement helper both backends call.
- `watcher.execution.max_payload_bytes` (`WatcherExecutionConfig`) is the new
  config field, overridable via `XZATOMA_WATCHER_MAX_PAYLOAD_BYTES` and
  validated non-zero in `Config::validate`.
- Generic backend: `GenericEventHandler` gained a `max_payload_bytes` field
  (defaulting to `DEFAULT_MAX_PAYLOAD_BYTES`) and a `with_max_payload_bytes`
  builder method. `GenericEventHandler::handle` calls `validate_payload_size` as
  step 0, before `GenericPlanEvent::new` ever parses the payload.
  `GenericWatcher::new` threads `watcher.execution.max_payload_bytes` through
  via the builder method.
- XZepr backend: `KafkaConsumerConfig` gained the same field/builder method.
  `XzeprConsumer::process_message` (the production ingestion path used by `run`)
  takes `max_payload_bytes` and rejects oversized payloads with the new
  `ConsumerError::PayloadTooLarge` variant before deserializing the
  `CloudEventMessage`. The alternate `run_with_channel` path enforces the same
  bound inline. `Watcher::new` (xzepr) threads
  `watcher.execution.max_payload_bytes` through the consumer config builder.

`GenericPlanEvent::new`'s own signature and the "any value that exists in memory
is a valid, size-bounded plan" guarantee for callers that go through
`GenericEventHandler::handle` are preserved; the bound is enforced by the
handler immediately before the parse call rather than inside `new` itself, so
`GenericPlanEvent::new`'s many existing doc examples and unit tests (which
construct events directly, bypassing the handler) were not disturbed.

Tests cover both backends: at-limit payloads parse normally, over-limit payloads
are rejected with a handled error (`GenericEventHandler::handle` returns `Err`;
`XzeprConsumer::process_message` returns `Err(ConsumerError::PayloadTooLarge)`
without invoking the message handler), and the config layer has
default/override/validation tests
(`test_apply_env_vars_overrides_max_payload_bytes`,
`test_apply_env_vars_ignores_invalid_max_payload_bytes`,
`test_config_validate_rejects_zero_max_payload_bytes`).

## Phase 2 backfill - real failure-path tests

Two previously-approximated failure-path tests were replaced with coverage of
the actual production code path:

- `src/mcp/transport/http.rs`: the `reqwest::Client::builder().build()` error
  branch in `HttpTransport::new` was factored into
  `map_client_build_error<E: Display>`, a small, directly testable helper. A
  real `reqwest::Client::build()` failure can't be triggered hermetically (it
  only fails on fatal TLS backend initialization), so the test drives the helper
  with a synthetic error string and asserts the exact
  `XzatomaError::McpTransport` message shape `new()`'s `map_err` produces.
- `src/tools/parallel_subagent.rs`: the semaphore-acquire body spawned by
  `ParallelSubagentTool::execute` was factored out into `run_task_with_permit`.
  The test now closes a real `tokio::sync::Semaphore`, calls
  `run_task_with_permit` directly with a real `TestProvider`, and asserts the
  resulting `TaskResult` is a handled failure (`success: false`, populated
  `error`, correct `label`) rather than reconstructing the expected shape by
  hand.

## Phase 1 backfill - IDE-permission wire-level tests

Two live-connection tests were added, using a real in-memory
`agent_client_protocol::Channel::duplex()` connection with a fake "Zed" peer, so
the `session/request_permission` wire shape and fail-closed behavior are
exercised end-to-end rather than only through the already-existing pure
`decision_from_outcome` / `permission_result_json` unit tests:

- `src/acp/ide_bridge.rs`
  `test_request_permission_live_connection_carries_four_options_and_title`:
  drives `IdeBridge::request_permission` against a fake client that records the
  incoming `RequestPermissionRequest` and approves it. Asserts the request
  carries exactly the four options (`allow-once`, `allow-always`, `reject-once`,
  `reject-always`) and that `tool_call.fields.title` is the full `operation`
  string, and that the mapped decision is `Approved`.
- `src/tools/ide_tools.rs`
  `test_ide_request_permission_live_connection_unsupported_client_fails_closed`:
  drives `IdeRequestPermissionTool::execute` against a fake client that
  explicitly rejects the permission request (simulating a client that does not
  support `session/request_permission`), and asserts the tool's JSON result
  fails closed to `{"approved": false, "outcome": "error"}`.

ACP's `ClientCapabilities` has no discrete capability flag for permission
requests (unlike `fs` and `terminal`), so "missing capability" is modeled as the
client-side handler explicitly returning a protocol error for the request -- the
same effect a client that doesn't implement the method would produce, and the
same error path `IdeRequestPermissionTool::execute` already handled via its
`Err` branch.

## Documentation updates

- `docs/explanation/implementations.md`: reconciled dangling
  `*_implementation.md` links (both `phaseN_`-named entries and the broader set
  of links to files that never existed in
  `docs/archive/implementation_summaries/`, which does not exist as a directory
  at all). Entries with a real target were repointed; entries with no real
  target anywhere in the tree had their dead links removed per the plan's own
  guidance.
- `docs/reference/watcher_environment_variables.md` and
  `docs/reference/configuration.md`: documented the new
  `watcher.execution.max_payload_bytes` field and
  `XZATOMA_WATCHER_MAX_PAYLOAD_BYTES` environment variable, including the three
  complete example configs in `configuration.md`.
- `docs/explanation/phase1_security_hardening_implementation.md`: reworded the
  M1 and L2 bullets (Task 1.2, Task 1.3, and the Task 1.5 deliverables
  checklist) to state plainly that only the `https` scheme allowlist (M1) and
  the plaintext-transport warning (L2) were delivered in Phase 1, with the
  confirmation gate and payload size bound completed here in Phase 7.
- All touched Markdown passes `markdownlint --fix --config .markdownlint.json`
  and `prettier --write --parser markdown --prose-wrap always`.

## Validation

The full AGENTS.md Rule 4 quality gate passed after this pass:

- `cargo fmt --all` -- clean.
- `cargo check --all-targets --all-features` -- clean.
- `cargo clippy --all-targets --all-features -- -D warnings` -- clean.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth` --
  all tests passed, 0 failed.

## Out of scope / deferred

- No new features were added; this phase is limited to finishing the five
  partial items the audit reopened.
- The pre-existing `watcher.execution.execution_mode` documentation gap in
  `configuration.md` (the field exists in code but was never documented) was
  noticed but is unrelated to Phase 7's scope and was left untouched.
- `implementations.md` also contains plain code-span path mentions of
  nonexistent files (for example
  `` `docs/explanation/phase3_streaming_infrastructure_implementation.md` ``)
  that are not markdown hyperlinks and were therefore outside the "dangling
  links" scan this task used. The plan's Task 7.5 scope and the "~19" estimate
  are both specific to markdown link syntax; a broader stale-path audit of the
  file's prose is a separate future cleanup.
