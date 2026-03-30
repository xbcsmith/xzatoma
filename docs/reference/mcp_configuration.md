# MCP configuration reference

This reference documents the full MCP client configuration surface in XZatoma.

## Overview

MCP (Model Context Protocol) client behavior is configured under the top-level
`mcp:` YAML key in `config/config.yaml`. MCP allows XZatoma to connect to
external tool servers that expose tools, resources, and prompts over a
standardized protocol.

Configuration is resolved in the standard XZatoma precedence order:

1. built-in defaults
2. YAML configuration file values
3. environment variable overrides

All fields have sensible defaults. Existing configuration files that omit the
`mcp:` key continue to work without changes.

## Global fields

### `mcp.auto_connect`

- **Type:** `boolean`
- **Default:** `true`

Automatically connect to all enabled servers on startup.

When `false`, servers must be connected explicitly via the `mcp connect`
command. Overridable at runtime via the `XZATOMA_MCP_AUTO_CONNECT` environment
variable.

### `mcp.request_timeout_seconds`

- **Type:** `u64`
- **Default:** `30`

Default timeout in seconds for individual MCP requests. Can be overridden
per-server via the server-level `timeout_seconds` field. Overridable at runtime
via the `XZATOMA_MCP_REQUEST_TIMEOUT` environment variable.

### `mcp.expose_resources_tool`

- **Type:** `boolean`
- **Default:** `true`

Expose a synthetic `mcp_resources` tool that lists and reads resources from all
connected servers. When enabled, the agent can discover and access server
resources through a unified tool interface.

### `mcp.expose_prompts_tool`

- **Type:** `boolean`
- **Default:** `true`

Expose a synthetic `mcp_prompts` tool that lists and retrieves prompts from all
connected servers. When enabled, the agent can discover and use server-provided
prompt templates through a unified tool interface.

## Server definitions

The `mcp.servers` field holds a list of server entries. Each entry describes a
single MCP server connection with its transport mechanism, capability flags, and
optional authentication configuration.

### `mcp.servers[].id`

- **Type:** `string`
- **Required:** yes

Unique identifier for this server entry. Used as a key in the server registry
and as the keyring service name prefix for OAuth tokens.

Must match `^[a-z0-9_-]{1,64}$` (lowercase letters, digits, hyphens, and
underscores; 1 to 64 characters).

### `mcp.servers[].transport`

- **Type:** `object`
- **Required:** yes

Transport configuration for reaching this server. See the
[Transport options](#transport-options) section below.

### `mcp.servers[].enabled`

- **Type:** `boolean`
- **Default:** `true`

Whether this server is active. When `false`, the server is skipped during
auto-connect and does not appear in the active server registry.

### `mcp.servers[].timeout_seconds`

- **Type:** `u64`
- **Default:** `30`

Maximum seconds to wait for a single MCP request to this server. Overrides the
global `mcp.request_timeout_seconds` value for this server only.

### `mcp.servers[].tools_enabled`

- **Type:** `boolean`
- **Default:** `true`

Expose this server's tools through the agent's tool registry. When enabled, any
tools advertised by the server become available to the agent during task
execution.

### `mcp.servers[].resources_enabled`

- **Type:** `boolean`
- **Default:** `false`

Enable resource access for this server. When enabled, the agent can list and
read resources exposed by this server.

### `mcp.servers[].prompts_enabled`

- **Type:** `boolean`
- **Default:** `false`

Enable prompt access for this server. When enabled, the agent can list and
retrieve prompt templates exposed by this server.

### `mcp.servers[].sampling_enabled`

- **Type:** `boolean`
- **Default:** `false`

Allow the server to request LLM sampling from the client. See
[Sampling and elicitation limitations](#sampling-and-elicitation-limitations)
for current implementation status.

### `mcp.servers[].elicitation_enabled`

- **Type:** `boolean`
- **Default:** `true`

Allow the server to request structured user input via elicitation. See
[Sampling and elicitation limitations](#sampling-and-elicitation-limitations)
for current implementation status.

## Transport options

The `transport` field uses a tagged union with a `type` discriminator. Two
transport types are supported: `stdio` and `http`.

### Stdio transport

Launch a local subprocess and communicate over its stdin/stdout pipes.

#### Fields

- `type` -- must be `stdio`
- `executable` -- path or name of the MCP server executable (required,
  non-empty)
- `args` -- command-line arguments passed to the executable (default: `[]`)
- `env` -- environment variables injected into the child process (default: `{}`)
- `working_dir` -- optional working directory for the child process

The child process environment is cleared before `env` values are applied.

#### Example

```/dev/null/stdio_transport.yaml#L1-7
transport:
  type: stdio
  executable: "/usr/local/bin/my-mcp-server"
  args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
  env:
    MY_VAR: "value"
  working_dir: /path/to/dir
```

### HTTP transport

Connect to a remote server via Streamable HTTP/SSE.

#### Fields

- `type` -- must be `http`
- `endpoint` -- full URL of the MCP endpoint (required, must use `http` or
  `https` scheme)
- `headers` -- extra HTTP headers added to every request (default: `{}`)
- `timeout_seconds` -- per-request timeout in seconds, overrides the
  server-level `timeout_seconds` for HTTP operations (optional)
- `oauth` -- OAuth 2.1 configuration for this endpoint (optional)

After OAuth token acquisition, the `Authorization` header is injected
automatically. Entries in `headers` supplement but do not replace the OAuth
header.

#### Example

```/dev/null/http_transport.yaml#L1-7
transport:
  type: http
  endpoint: "https://api.example.com/mcp"
  headers:
    Authorization: "Bearer token"
  timeout_seconds: 60
  oauth:
    client_id: "my-client"
    client_secret: "secret"
    redirect_port: 8080
    metadata_url: "https://auth.example.com/.well-known/openid-configuration"
```

## OAuth 2.1 configuration

The `oauth` block within an HTTP transport entry configures OAuth 2.1
authentication for that server endpoint. When omitted, the endpoint is assumed
to be publicly accessible or protected by a static API key supplied via
`headers`.

### `oauth.client_id`

- **Type:** `string | null`
- **Default:** `null`

Static OAuth client ID. When provided, dynamic client registration is skipped
and this value is sent directly to the token endpoint.

### `oauth.client_secret`

- **Type:** `string | null`
- **Default:** `null`

Static OAuth client secret for confidential clients only. Public clients using
PKCE should leave this unset.

### `oauth.redirect_port`

- **Type:** `u16 | null`
- **Default:** `null` (OS-assigned)

Local TCP port for the OAuth redirect callback listener. When omitted or `null`,
the operating system assigns an available port automatically.

### `oauth.metadata_url`

- **Type:** `string | null`
- **Default:** `null`

Override URL for the authorization server's `.well-known` discovery document.
When `null`, the standard discovery paths derived from the resource endpoint are
tried automatically.

### OAuth security guidance

- Avoid storing `client_secret` directly in committed configuration files.
- Prefer environment variable injection or a secret manager for OAuth
  credentials in production.
- Public clients (browser-based or CLI tools) should use PKCE without a client
  secret.
- Use `https` endpoints in production to protect token exchanges.

## Environment variable overrides

The following environment variables override MCP configuration fields at
runtime:

| Environment variable          | Field                         |
| ----------------------------- | ----------------------------- |
| `XZATOMA_MCP_AUTO_CONNECT`    | `mcp.auto_connect`            |
| `XZATOMA_MCP_REQUEST_TIMEOUT` | `mcp.request_timeout_seconds` |

Boolean environment values accept common truthy and falsy values:

- true values: `1`, `true`, `yes`, `on`
- false values: `0`, `false`, `no`, `off`

## Validation rules

MCP configuration is validated at startup via `McpConfig::validate` (called
automatically by `Config::validate`). The following rules are enforced:

### Server ID rules

- Server IDs must be unique across the entire `mcp.servers` list
- Server IDs must match `^[a-z0-9_-]{1,64}$`

### Stdio transport rules

- `executable` must be non-empty

### HTTP transport rules

- `endpoint` must use the `http` or `https` URL scheme

### Validation behavior

Validation runs eagerly at startup. If any rule fails, XZatoma exits with a
configuration error before connecting to any servers.

## Sampling and elicitation limitations

### Sampling

The sampling handler is not yet implemented. Servers that require sampling
capability will fail at runtime. Set `sampling_enabled: false` (the default) for
servers that do not require sampling.

### Elicitation

The elicitation handler is not yet fully implemented. All elicitation requests
currently receive a `Cancel` response. Servers that depend on successful
elicitation may not function as expected. The `elicitation_enabled` field
defaults to `true` to maintain forward compatibility.

## Complete example

```/dev/null/complete_mcp_config.yaml#L1-24
mcp:
  auto_connect: true
  request_timeout_seconds: 30
  expose_resources_tool: true
  expose_prompts_tool: true
  servers:
    - id: "filesystem"
      transport:
        type: stdio
        executable: "npx"
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tools_enabled: true
      resources_enabled: true
    - id: "remote-api"
      transport:
        type: http
        endpoint: "https://mcp.example.com/api"
        headers:
          X-Custom-Header: "custom-value"
        oauth:
          client_id: "xzatoma-client"
          redirect_port: 8765
      tools_enabled: true
      prompts_enabled: true
```

## Related documentation

- Configuration overview: `docs/reference/configuration.md`
- Architecture reference: `docs/reference/architecture.md`
- CLI reference: `docs/reference/cli.md`
