# Zed Session Mode Phase 2 Implementation

## Overview

Phase 2 of the Zed Session Mode plan implements Zed-forwarded MCP server
integration. When a user opens a new XZatoma chat session from Zed, the editor
sends `NewSessionRequest.mcp_servers` containing MCP servers the user has
configured in their Zed project settings. Prior to this change, XZatoma ignored
that field entirely. These are per-project servers distinct from the servers in
XZatoma's own `config.yaml`; they arrive over the ACP protocol for the lifetime
of a single session.

## Problem Statement

Zed project settings support a `context_servers` block that maps human-readable
MCP server definitions to the ACP `NewSessionRequest.mcp_servers` field. Because
XZatoma did not read or connect those servers, users had no way to expose
project-specific MCP tools (filesystem helpers, language servers, custom
integrations) to the agent without also adding them to XZatoma's global
configuration. This created friction and prevented per-project tool isolation.

## Changes

### Change 1: New imports

**File**: `src/acp/stdio.rs`

Two imports were added after the existing
`use crate::mcp::manager::McpClientManager;` line:

```rust
use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};
use crate::mcp::tool_bridge::register_mcp_tools;
```

### Change 2: `sanitize_mcp_server_id` helper

**File**: `src/acp/stdio.rs`

**Location**: Private free function added after `initial_mode_id_from_config`.

Lowercases the input string and replaces every character that does not match
`[a-z0-9_-]` with an underscore. Truncates the result to 64 characters. Returns
`XzatomaError::Config` when the sanitized result is empty so that servers with
names consisting entirely of unsupported characters (or with an empty name) are
rejected before any connection attempt.

The regex constraint `^[a-z0-9_-]{1,64}$` is enforced downstream by
`McpServerConfig::validate`; `sanitize_mcp_server_id` produces a string
guaranteed to pass that rule (except for the emptiness case, which is caught
explicitly).

### Change 3: `convert_acp_mcp_server` helper

**File**: `src/acp/stdio.rs`

**Location**: Private free function added after `sanitize_mcp_server_id`.

Converts a single `acp::McpServer` variant to an `McpServerConfig`. The three
ACP variants are handled as follows:

- `acp::McpServer::Stdio(s)`: Maps directly to
  `McpServerTransportConfig::Stdio`. The executable is taken from
  `s.command.to_string_lossy()`, args from `s.args`, and environment variables
  from the `Vec<EnvVariable>` using `name` and `value` fields.

- `acp::McpServer::Http(h)`: `h.url` is parsed to a `url::Url`. The server is
  mapped to `McpServerTransportConfig::Http` with no OAuth configuration.
  Headers are converted from `Vec<HttpHeader>` to `HashMap<String, String>`.

- `acp::McpServer::Sse(s)`: Mapped identically to `Http` using `s.url` and
  `s.headers`. XZatoma's internal transport layer handles both SSE and plain
  HTTP through the same `McpServerTransportConfig::Http` path.

- Wildcard arm: The `acp::McpServer` enum is `#[non_exhaustive]`, so a wildcard
  arm returning `XzatomaError::Config` handles any future protocol variants
  gracefully without a compilation error.

All capability flags other than `tools_enabled` (`resources_enabled`,
`prompts_enabled`, `sampling_enabled`, `elicitation_enabled`) are set to
`false`. Zed-forwarded servers are connected for tool access only; resource and
prompt capabilities require explicit user configuration. `cfg.validate()` is
called at the end of each match arm before returning.

### Change 4: Zed MCP connection block in `create_session`

**File**: `src/acp/stdio.rs`

**Location**: `async fn create_session`, immediately after
`let mut tools = env.tool_registry;` and before provider creation.

The block performs the following steps in order:

1. Short-circuits when `request.mcp_servers` is empty to avoid unnecessary lock
   overhead.

2. Short-circuits when `env.mcp_manager` is `None` (MCP auto-connect is disabled
   in `config.yaml` or no servers were configured). Zed-forwarded servers are
   only connected when a live manager is available; attempting to build a fresh
   manager here would bypass global MCP configuration policy.

3. Iterates `request.mcp_servers`. For each server:

   a. Calls `convert_acp_mcp_server`. On `Err`, logs `warn!` with the workspace
   path and the error, then `continue`. The failure is not fatal to session
   creation.

   b. Reads `McpClientManager::connected_servers()` under a read lock. If any
   existing entry has the same `id` as the converted config, logs `debug!` and
   `continue` to avoid duplicate registrations across reconnects.

   c. Calls `McpClientManager::connect(cfg).await` under a write lock. On `Err`,
   logs `warn!` and `continue`.

   d. On success, logs `info!` with `server_id` and workspace.

4. After the loop, calls `register_mcp_tools` once with the full manager. This
   registers tools from all newly connected servers (and any previously
   connected servers, since `register_mcp_tools` calls `get_tools_for_registry`
   which returns all connected entries). Failures are logged as `warn!` and do
   not abort session creation.

The design ensures that a single unreachable or misconfigured Zed-forwarded
server never prevents the session from being created or the remaining servers
from being connected.

## Test Coverage

Six new unit tests were added to `mod tests` in `src/acp/stdio.rs`. All existing
tests continue to pass unchanged.

| Test name                                                        | What it verifies                                                                                                       |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `test_convert_acp_mcp_server_stdio_produces_valid_config`        | A `Stdio` ACP server produces a config with the correct `id`, `executable`, `args`, and `enabled` fields.              |
| `test_convert_acp_mcp_server_http_produces_valid_config`         | An `Http` ACP server produces a config with a correctly parsed `url::Url` endpoint.                                    |
| `test_convert_acp_mcp_server_sanitizes_name_with_spaces`         | `"My MCP Server!"` sanitizes to `"my_mcp_server_"` (spaces and punctuation replaced with underscores, lowercased).     |
| `test_convert_acp_mcp_server_rejects_empty_name`                 | An empty server name returns `Err` rather than producing an invalid config.                                            |
| `test_convert_acp_mcp_server_rejects_invalid_http_url`           | An `Http` server with `url: "not a url"` returns `Err` at the URL parse step.                                          |
| `test_create_session_with_empty_mcp_servers_list_does_not_error` | A `NewSessionRequest` with no `mcp_servers` (the default) creates a session successfully over the full protocol stack. |

## Behavioral Invariants Preserved

- Sessions with no `mcp_servers` in the request behave identically to before.
- Sessions where `env.mcp_manager` is `None` (MCP disabled) silently skip the
  Zed-forwarded server block; no error is surfaced to the client.
- All existing MCP servers from `config.yaml` that were registered during
  `build_agent_environment` remain registered. `register_mcp_tools` is
  idempotent with respect to names that already exist in the registry; a `warn!`
  is emitted for overwrites, matching pre-existing behavior.
- Tool registration errors from `register_mcp_tools` are logged but do not abort
  session creation, consistent with the tolerant error policy applied
  throughout.

## Design Rationale

The guard on `env.mcp_manager.is_some()` is intentional. Building a fresh
`McpClientManager` solely for Zed-forwarded servers would bypass the
`mcp.auto_connect` configuration flag and the global server list, creating a
split-brain scenario where some tools are governed by `config.yaml` policy and
others are not. Using the existing manager ensures all MCP connections share the
same HTTP client, token store, and task manager.

The per-server `continue` on every error path (conversion, deduplication,
connection, registration) is the correct default for a session-startup side
effect. Missing a single Zed-forwarded tool is acceptable; failing the entire
session creation for a typo in Zed's `context_servers` setting is not.

Mapping `acp::McpServer::Sse` to `McpServerTransportConfig::Http` is valid
because XZatoma's HTTP transport (`src/mcp/transport/http.rs`) already handles
the SSE event stream natively. A separate `Sse` transport variant would
duplicate that logic without adding value.
