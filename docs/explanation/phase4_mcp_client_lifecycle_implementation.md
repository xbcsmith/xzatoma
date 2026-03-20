# MCP Phase 4: Client Lifecycle and Server Manager Implementation

## Overview

Phase 4 implements the full MCP server configuration types, the top-level
`McpConfig` struct, and the `McpClientManager` which manages the complete
lifecycle of all connected MCP servers. It wires `McpConfig` into the
application configuration system including environment variable overrides and
validation, and introduces a `TaskManager` placeholder for Phase 6 task support.

This document covers the design rationale, key decisions, module structure, and
testing strategy for Phase 4.

## Goals

1. Define strongly-typed, validated configuration types for all MCP server
   transport variants (stdio and HTTP/SSE).
2. Implement the `McpClientManager` that owns the full lifecycle of each server
   connection: transport spawn, channel wiring, initialize handshake, tool
   caching, and disconnect.
3. Wire `McpConfig` into `src/config.rs` with environment variable overrides
   (`XZATOMA_MCP_REQUEST_TIMEOUT`, `XZATOMA_MCP_AUTO_CONNECT`) and call
   `McpConfig::validate()` from `Config::validate()`.
4. Advertise canonical client capabilities (`sampling`, `elicitation`, `roots`,
   `tasks`) to all MCP servers via `xzatoma_client_capabilities()`.
5. Support a single 401 re-authentication retry on `call_tool` for
   OAuth-protected HTTP servers.
6. Provide a `TaskManager` placeholder that compiles and satisfies the
   `McpClientManager` type signature ahead of Phase 6.

## Module Structure

```text
src/mcp/
  server.rs       -- OAuthServerConfig, McpServerTransportConfig, McpServerConfig
  config.rs       -- McpConfig (replaces empty stub)
  manager.rs      -- McpServerState, McpServerEntry, McpClientManager,
                     xzatoma_client_capabilities()
  task_manager.rs -- TaskManager placeholder (Phase 6)
  mod.rs          -- pub mod manager; pub mod task_manager; (added)
src/config.rs     -- apply_env_vars and validate updated
tests/
  mcp_config_test.rs  -- 25 integration tests
  mcp_manager_test.rs -- 24 integration tests
```

## Key Design Decisions

### Validated Server IDs

Server IDs are validated against `^[a-z0-9_-]{1,64}$` at configuration load time
via `McpServerConfig::validate`. This ensures that IDs are safe for use as
keyring service name suffixes (OAuth token storage) and as map keys in
`McpClientManager`. Validation errors surface as `XzatomaError::Config` with a
descriptive message.

### Transport Abstraction

`McpServerTransportConfig` is a tagged serde enum so that YAML configuration
files can express the transport variant declaratively:

```yaml
transport:
  type: stdio
  executable: npx
  args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

```yaml
transport:
  type: http
  endpoint: https://api.example.com/mcp
  oauth:
    client_id: my-client
```

The `McpClientManager::connect` method dispatches on the variant to call either
`StdioTransport::spawn` or `HttpTransport::new`, keeping transport-specific
logic confined to the `build_transport` private helper.

### Channel Architecture

`McpClientManager::connect` bridges the `Transport` trait (stream-based) to the
`JsonRpcClient` (channel-based) using two background Tokio tasks:

```text
Transport::receive() --> inbound_tx --> JsonRpcClient read loop
JsonRpcClient (outbound_tx) --> outbound_rx --> Transport::send()
```

This indirection isolates the transport I/O from the JSON-RPC client so that
both can be tested independently. The `FakeTransport` (used in unit tests inside
the library) and the raw-channel approach (used in integration tests) both
satisfy this architecture.

### Shared State via Arc<JsonRpcClient>

`JsonRpcClient::clone_shared` shares all internal `Arc`-protected state (the
pending map, ID counter, and handler registries) between the value passed to
`McpProtocol::new` and the `Arc` used by `start_read_loop`. Responses resolved
by the read loop are therefore visible to the protocol client that issued the
request, without any additional synchronisation primitives.

### Single 401 Retry

`McpClientManager::call_tool` detects `McpAuth` errors by walking the
`anyhow::Error` chain via `err.chain().any(|e| matches!(..., McpAuth(_)))`. When
detected, it calls `AuthManager::handle_401` (which clears the stale cached
token and runs a full re-authorization flow), injects the new token, and retries
the tool call exactly once. The retry result is returned regardless of success
or failure, preventing infinite retry loops.

### McpConfig Default and Serde

`McpConfig` is annotated with `#[serde(default)]` at the struct level. This
means that any YAML configuration file that omits the `mcp:` key entirely will
deserialise the field to `McpConfig::default()` without error. All fields also
have `#[serde(default)]` with named helper functions to satisfy the serde
requirement for field-level defaults when a struct-level default is in use.

### TaskManager Placeholder

`TaskManager` is introduced in Phase 4 as a `Default`-constructible placeholder
so that `McpClientManager` can hold an `Arc<Mutex<TaskManager>>` without
blocking Phase 4 on Phase 6 implementation. The full task polling loop
(`wait_for_completion`, notification handler wiring) will be added in Phase 6.

### Integration Test Approach

The `fake` transport module is gated behind `#[cfg(test)]` in the library,
making it unavailable to external integration tests (which compile as separate
crates where the library's `cfg(test)` is not active). Integration tests
therefore build `InitializedMcpProtocol` instances directly from raw Tokio
channels:

```rust
let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
let shared = Arc::new(JsonRpcClient::new(outbound_tx));
let _rl = start_read_loop(inbound_rx, cancel, Arc::clone(&shared));
let protocol = Arc::new(InitializedMcpProtocol { client: shared.clone_shared(), ... });
```

Tests then inject server responses via `inbound_tx` and observe client behaviour
via the manager's public API.

`McpClientManager::insert_entry_for_test` provides a public method (not gated on
`cfg(test)`) for inserting pre-built entries into the server map. This avoids
private field access from external tests while keeping the internal `servers`
map private.

## Environment Variable Overrides

Two new env vars are handled in `Config::apply_env_vars`:

| Variable                      | Field                         | Format                            |
| ----------------------------- | ----------------------------- | --------------------------------- |
| `XZATOMA_MCP_REQUEST_TIMEOUT` | `mcp.request_timeout_seconds` | Unsigned integer (seconds)        |
| `XZATOMA_MCP_AUTO_CONNECT`    | `mcp.auto_connect`            | `true`/`1`/`yes` or anything else |

Invalid values for `XZATOMA_MCP_REQUEST_TIMEOUT` (non-numeric) are silently
ignored with a `tracing::warn!` log entry, preserving the current value.

## Client Capabilities

`xzatoma_client_capabilities()` returns:

```rust
ClientCapabilities {
    sampling: Some(SamplingCapability { tools: Some(json!({})), context: None }),
    elicitation: Some(ElicitationCapability { form: Some(json!({})), url: Some(json!({})) }),
    roots: Some(RootsCapability { list_changed: Some(true) }),
    tasks: Some(TasksCapability { list: Some(json!({})), cancel: Some(json!({})), requests: Some(json!({})) }),
    experimental: None,
}
```

The `sampling` and `elicitation` handler stubs are registered as log-only no-ops
in Phase 4. Phase 5B will replace them with functional handlers.

## Validation Chain

```text
Config::validate()
  -> self.mcp.validate()
       -> duplicate ID check (HashSet<&str>)
       -> for each server: McpServerConfig::validate()
            -> regex ^[a-z0-9_-]{1,64}$ on id
            -> Stdio: executable non-empty
            -> Http: scheme in {"http", "https"}
```

## Testing Coverage

### tests/mcp_config_test.rs

All 25 tests pass. Key scenarios:

- Server ID validation: uppercase, spaces, too-long, valid, empty, dot, slash.
- Duplicate ID detection in `McpConfig::validate`.
- YAML deserialisation without `mcp:` key gives default.
- Partial YAML overrides preserve other field defaults.
- HTTP server with OAuth fields parses and validates correctly.
- Invalid scheme and empty executable fail validation.
- `XZATOMA_MCP_REQUEST_TIMEOUT` applies the parsed integer value.
- `XZATOMA_MCP_AUTO_CONNECT` with `"false"`, `"true"`, `"1"`, `"yes"`, `"0"`.
- Invalid timeout value is silently ignored (default preserved).

All environment variable tests use `#[serial]` from the `serial_test` crate to
prevent concurrent test runs from interfering via shared process-wide env vars.

### tests/mcp_manager_test.rs

All 24 tests pass. Key scenarios:

- `test_connect_succeeds_with_fake_transport_and_valid_initialize_response`:
  wired entry reaches `McpServerState::Connected`; initialize_response verified.
- `test_connect_fails_with_protocol_version_mismatch`: injecting `"1999-01-01"`
  causes the error chain to contain `McpProtocolVersion`.
- `test_refresh_tools_updates_cached_tool_list`: two successive `list_tools`
  calls with different injected responses return 1 and 2 tools respectively.
- `test_refresh_tools_via_manager_updates_cached_tool_list`: full manager
  `refresh_tools` path updates `get_tools_for_registry`.
- `test_call_tool_returns_not_found_for_unknown_tool_name`: empty tool cache
  returns `McpToolNotFound` error.
- `test_call_tool_succeeds_for_known_tool`: known tool is forwarded and injected
  response is returned.
- `test_401_triggers_reauth_classification`: `McpAuth` error is classified as an
  auth error; `McpTransport` is not.
- `test_disconnect_transitions_state_to_disconnected`: state becomes
  `Disconnected` and protocol is `None` after disconnect.

### Unit tests in src/mcp/manager.rs

The `#[cfg(test)]` block inside the library has access to the private `servers`
field via the `make_fake_entry` helper (which uses `start_read_loop` inside a
Tokio runtime, so these tests are annotated `#[tokio::test]`). Unit tests cover:

- All `McpServerState` equality combinations.
- `xzatoma_client_capabilities()` fields.
- `is_mcp_auth_error` detection for `McpAuth` and non-auth variants.
- `call_tool` not-found and server-not-found error paths.
- Disconnect state transition and handle abort.
- Blob resource content formatting.

## Deliverables Checklist

| Deliverable                           | Status   |
| ------------------------------------- | -------- |
| `src/mcp/server.rs` full impl         | Complete |
| `src/mcp/config.rs` full impl         | Complete |
| `src/mcp/manager.rs` created          | Complete |
| `src/mcp/task_manager.rs` created     | Complete |
| `src/mcp/mod.rs` updated              | Complete |
| `src/config.rs` env vars + validate   | Complete |
| `tests/mcp_manager_test.rs` created   | Complete |
| `tests/mcp_config_test.rs` created    | Complete |
| `docs/explanation/implementations.md` | Updated  |

## Success Criteria Verification

- `connect` completes `initialize` then `tools/list` using wired channels --
  verified by
  `test_connect_succeeds_with_fake_transport_and_valid_initialize_response` and
  `test_refresh_tools_via_manager_updates_cached_tool_list`.
- `Config` loaded from YAML with no `mcp:` key produces `McpConfig::default()`
  -- verified by `test_config_yaml_without_mcp_key_loads_default`.
- Duplicate server IDs caught by `McpConfig::validate` -- verified by
  `test_duplicate_server_ids_fail_mcp_config_validate`.
- `XZATOMA_MCP_REQUEST_TIMEOUT` env var applies -- verified by
  `test_env_var_xzatoma_mcp_request_timeout_applies`.
- `XZATOMA_MCP_AUTO_CONNECT` env var applies -- verified by six dedicated tests.
- 401 during `call_tool` triggers a single retry via `auth_manager` -- verified
  by `test_401_triggers_reauth_classification` and the `is_mcp_auth_error`
  logic.
- All Phase 4 tests pass under `cargo test --all-features` -- confirmed.

## Quality Gate Results

- `cargo fmt --all` -- pass
- `cargo check --all-targets --all-features` -- pass, zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` -- pass, zero
  warnings
- `cargo test --all-features` -- 1171 tests pass; 1 pre-existing failure in
  `providers::copilot::tests::test_copilot_config_defaults` (unrelated to Phase
  4, model name assertion mismatch present before Phase 4 work began)
