# Network Kafka Test Disabling Implementation

## Overview

This change disables tests that are unsafe for the default CI path because they
touch external integration boundaries or shared runtime resources. The goal is
to keep normal CI deterministic while preserving the tests for intentional local
or integration runs with `cargo test -- --ignored`.

The affected test categories are:

- ACP runtime tests that must exercise persistence through shared storage.
- ACP server tests that initialize runtime-backed state and can hang in CI.
- ACP runtime, server unit, and ACP HTTP integration tests that can avoid
  persistence by using in-memory runtime isolation.
- Wiremock-backed OAuth discovery, OAuth flow, MCP HTTP transport, and OpenAI
  provider tests that bind local network sockets.
- Kafka-backed watcher tests that require a broker or instantiate clients that
  may attempt broker communication.
- Generic watcher dry-run tests that should avoid constructing real Kafka
  producers.

## Rationale

CI should not depend on services or resources that may be unavailable, slow, or
shared between jobs. Tests that bind sockets, connect to Kafka, exercise
provider-like HTTP behavior, or use default persistent runtime storage can fail
for reasons unrelated to the code under test.

The hanging tests observed in CI were ACP runtime and ACP server tests. They
exercise behavior that can touch durable runtime storage during run creation,
manifest generation, server state construction, or endpoint handling. In a CI
environment, this shared persistence path can block long enough to stall the
test suite.

ACP unit tests that do not explicitly validate durable persistence should use
the in-memory runtime constructor. This keeps lifecycle, executor, and route
tests deterministic by preventing accidental access to the shared application
database while still preserving dedicated persistence coverage for tests that
opt into storage.

Disabling these tests by default makes the standard test suite focused on unit
and deterministic behavior. The tests remain available for explicit integration
validation.

## Implementation Details

The affected tests were either isolated with an in-memory ACP runtime or
annotated with `#[ignore = "..."]` rather than removed. This keeps coverage
available while preventing accidental execution of integration-sensitive tests
in default CI.

Ignored ACP runtime tests include:

- `test_runtime_set_awaiting_persists_await_state`
- `test_runtime_resume_run_transitions_awaiting_to_running`

Ignored ACP server tests include:

- `test_acp_server_state_generates_primary_manifest`
- `test_agents_endpoint_returns_list_shape`
- `test_agent_by_name_endpoint_returns_success`
- `test_agent_by_name_endpoint_returns_not_found`
- `test_handle_create_run_async_returns_accepted`
- `test_handle_create_run_stream_returns_sse_response`
- `test_handle_create_run_rejects_unknown_agent`
- `test_handle_create_run_rejects_unsupported_artifact_input`

Wiremock-backed network tests were also marked ignored because they bind local
network sockets and use HTTP clients against local mock servers. These include
tests in:

- `tests/mcp_auth_discovery_test.rs`
- `tests/mcp_auth_flow_test.rs`
- `tests/mcp_http_transport_test.rs`
- `src/providers/openai.rs`

For ACP tests that only need runtime lifecycle behavior, the test setup now uses
`AcpRuntime::new_in_memory`. This constructor disables SQLite-backed storage and
restoration so unit tests do not touch shared user or CI state.

ACP HTTP integration tests that exercise in-process Axum routers also construct
server state from in-memory runtimes and mock executors. This keeps lifecycle
and discovery coverage active while avoiding shared SQLite restoration paths
that can stall CI jobs.

Generic watcher dry-run construction now uses an in-memory fake result producer
instead of `GenericResultProducer`. This prevents dry-run unit tests from
creating an `rdkafka` `FutureProducer`, which can attempt broker communication
even when the watcher is not intended to publish to Kafka.

Kafka-sensitive tests were already being handled with `#[ignore]` where they
require a broker or instantiate Kafka producer/client types that can attempt
broker communication. Those tests should remain ignored by default.

## Running Ignored Tests Locally

Developers can still run the ignored integration-style tests explicitly:

```text
cargo test -- --ignored
```

For a narrower run, filter by test name:

```text
cargo test test_runtime_set_awaiting_persists_await_state -- --ignored
```

Some ignored tests may require additional local setup, such as:

- A running Kafka broker at the configured address.
- Local network socket binding permission.
- System keyring access for Copilot-related tests.
- Isolated runtime storage paths when testing persistence behavior.

## CI Behavior

Default CI should continue using the normal test command without `--ignored`.
That keeps network, Kafka, and shared-storage-sensitive tests out of the primary
quality gate while preserving them for opt-in validation.

The intended default command remains:

```text
cargo test --all-features
```

## Future Improvements

The ignored tests can be re-enabled by default if they are refactored to avoid
non-deterministic integration boundaries. Good candidates include:

- Moving ACP persistence tests to explicit temporary database paths.
- Avoiding shared application storage in server state tests and ACP HTTP
  integration tests.
- Replacing local socket tests with lower-level deterministic unit tests where
  possible.
- Gating Kafka integration tests behind a feature or environment variable.
- Keeping dry-run watcher paths fully isolated from real Kafka producer and
  consumer construction.
- Running integration tests in a separate CI job with required services and
  longer timeouts.
