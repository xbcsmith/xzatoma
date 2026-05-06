# Phase 2: Zed MCP Session Mode Implementation

## Overview

Phase 2 connects Zed-forwarded MCP servers to XZatoma sessions. When Zed
launches XZatoma as a subprocess agent, it sends a `NewSessionRequest` that
includes an `mcp_servers` list populated from the user's Zed project settings.
Before this phase, `create_session` ignored that list entirely. After this
phase, each server in the list is converted, validated, connected, and its tools
are registered into the session's tool registry.

## Problem Statement

Zed users configure MCP servers in their project settings (`.zed/settings.json`
or workspace-level config). Zed forwards those servers to the agent via the ACP
protocol field `NewSessionRequest.mcp_servers`. XZatoma has its own MCP
connection infrastructure (`McpClientManager`, `McpServerConfig`), but there was
no bridge from the ACP schema types to XZatoma's internal types, and no code in
`create_session` that acted on the forwarded list.

Without this bridge, tools provided by the user's Zed-configured MCP servers
were unavailable to the agent during a session, even though Zed had already told
the agent about them.

## Design Decisions

### Conversion Layer in `stdio.rs` Only

All new code lives in `src/acp/stdio.rs`. The conversion functions
`convert_acp_mcp_server` and `sanitize_mcp_server_id` are private to that
module. This keeps the ACP-to-internal translation concern isolated at the
transport boundary and avoids coupling the `mcp` module to ACP schema types.

### Soft Failure Model

Individual server connection failures are logged at `WARN` level and skipped.
They do not abort session creation. This matches the real-world expectation that
a misconfigured or temporarily unavailable MCP server in the user's Zed settings
should not prevent the agent session from starting. The session proceeds with
whatever servers connect successfully.

### Duplicate Detection

Before attempting a connection, the manager's `connected_servers()` list is
checked for an entry whose `id` matches the sanitized name of the incoming
server. This prevents double-connection when Zed sends a `NewSessionRequest` for
a workspace that already has a running session with the same servers, or when a
server is also present in XZatoma's own `config.yaml`.

### SSE Maps to Http Transport

XZatoma's internal transport model has two variants: `Stdio` and `Http`. The ACP
schema has three: `Stdio`, `Http`, and `Sse`. SSE (Server-Sent Events) uses the
same HTTP-based connection path inside XZatoma, so both `acp::McpServer::Http`
and `acp::McpServer::Sse` are mapped to `McpServerTransportConfig::Http`. The
URL and headers from both variants are forwarded unchanged.

### Non-Exhaustive Match Safety

`acp::McpServer` is marked `#[non_exhaustive]` in the ACP schema crate, meaning
future protocol versions may add new variants. The match in
`convert_acp_mcp_server` includes a wildcard arm that returns
`XzatomaError::Config` rather than panicking. This is surfaced as a `WARN` log
and the server is skipped, preserving the soft-failure model.

### Name Sanitization

ACP server names are free-form strings chosen by users in their Zed settings.
XZatoma's `McpServerConfig::validate()` requires IDs to match
`[a-z0-9_-]{1,64}`. The `sanitize_mcp_server_id` function performs a
deterministic, lossy transformation:

1. Lowercases the entire string with `to_ascii_lowercase`.
2. Replaces every character outside `[a-z0-9_-]` with `_`.
3. Truncates to 64 characters via `Iterator::take`.
4. Rejects the empty string after transformation.

The resulting ID is stable across repeated sessions for the same server name,
which is important for duplicate detection.

### Tool Registration After All Connections

After the loop that connects each forwarded server, `register_mcp_tools` is
called once on the full `McpClientManager`. This call is idempotent with respect
to already-registered tools from XZatoma's own config because `ToolRegistry`
uses the namespaced key `{server_id}__{tool_name}` and only registers tools from
servers that are currently connected. Servers that failed to connect have no
entries in the manager and contribute no tools.

## Code Walkthrough

### Imports Added

```xzatoma/src/acp/stdio.rs#L56-57
use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};
use crate::mcp::tool_bridge::register_mcp_tools;
```

These are the only two new module-level imports. `McpClientManager` was already
imported.

### `sanitize_mcp_server_id`

A small pure function. It operates on `&str`, returns `Result<String>`, and has
no side effects. It is the single source of truth for how a Zed server name
becomes an XZatoma server ID.

### `convert_acp_mcp_server`

Matches on all known ACP variants plus a wildcard. For each known variant it:

1. Calls `sanitize_mcp_server_id` on the name field.
2. Constructs a `McpServerConfig` with `enabled: true`, `timeout_seconds: 30`,
   `tools_enabled: true`, and all other capability flags set to `false`.
3. Calls `cfg.validate()` to enforce XZatoma's own schema constraints.
4. Returns the validated config.

Capability flags default conservatively. Only tool calls are enabled because
that is the primary use case for Zed-forwarded servers. Resources, prompts,
sampling, and elicitation can be enabled in a future phase if needed.

### `create_session` MCP Block

The block is inserted immediately after `let mut tools = env.tool_registry;` and
before `let provider_box = create_provider_with_override(...)`. Placement at
this point ensures:

- The tool registry exists and is mutable.
- The provider has not yet been constructed, keeping the MCP connection work
  logically separate from provider initialization.
- The IDE bridge registration (which happens later in the same function) is not
  affected.

The block is guarded by `if !request.mcp_servers.is_empty()` to avoid any lock
acquisition overhead for the common case where Zed sends no servers.

## Testing

Six new unit tests cover the conversion and session path:

| Test                                                             | What it verifies                                                                         |
| ---------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `test_convert_acp_mcp_server_stdio_produces_valid_config`        | Stdio variant converts correctly; executable and args are preserved.                     |
| `test_convert_acp_mcp_server_http_produces_valid_config`         | Http variant converts correctly; endpoint URL is preserved.                              |
| `test_convert_acp_mcp_server_sanitizes_name_with_spaces`         | Spaces and punctuation are replaced with underscores; lowercase applied.                 |
| `test_convert_acp_mcp_server_rejects_empty_name`                 | Empty name string produces `Err`.                                                        |
| `test_convert_acp_mcp_server_rejects_invalid_http_url`           | Malformed URL produces `Err`.                                                            |
| `test_create_session_with_empty_mcp_servers_list_does_not_error` | Session creation with an empty `mcp_servers` list succeeds end-to-end over the protocol. |

The SSE-to-Http mapping is exercised indirectly through `convert_acp_mcp_server`
since the SSE and Http code paths are structurally identical after the name
sanitization step.

## Files Changed

Only one file was modified:

- `src/acp/stdio.rs` -- two imports, two private functions, one inline block in
  `create_session`, three test imports, six test functions.

No other files were modified. The `mcp` module, the ACP schema crate, and all
other modules are unchanged.
