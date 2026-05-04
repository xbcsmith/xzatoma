# ACP configuration reference

This reference documents the ACP server configuration surface in XZatoma for
operator use.

## Overview

ACP support is configured under the top-level `acp` section in
`config/config.yaml`. The effective configuration is derived in this order:

1. file-based configuration
2. environment variable overrides
3. CLI subcommand overrides for `xzatoma acp serve`

The ACP server is disabled by default and only starts when you explicitly invoke
the ACP server command.

## ACP modes

XZatoma supports two distinct ACP integration modes. They use different
transports and serve different client types.

### HTTP server mode

Started with `xzatoma acp serve`. Exposes an HTTP server that ACP-compatible
REST clients can call directly. Configuration lives under `acp:` in
`config/config.yaml`.

This is the mode documented in the remainder of this file.

### Stdio agent mode

Started with `xzatoma agent`. Zed and other ACP-compatible IDEs launch XZatoma
as a subprocess and communicate over stdin/stdout using newline-delimited
JSON-RPC. Configuration lives under `acp.stdio:` in `config/config.yaml`.

Stdout is reserved exclusively for JSON-RPC protocol traffic. All diagnostic
output and tracing goes to stderr. Do not add logging, banners, or debug output
to stdout in agent mode.

See `docs/how-to/zed_acp_agent_setup.md` for Zed configuration instructions. See
`docs/reference/acp_configuration.md` (the `acp.stdio` section below) for the
full stdio field reference.

## Configuration schema

```/dev/null/config.yaml#L1-16
acp:
  enabled: false
  host: "127.0.0.1"
  port: 8765
  compatibility_mode: versioned
  base_path: "/api/v1/acp"
  default_run_mode: async
  persistence:
    enabled: false
    max_events_per_run: 1000
    max_completed_runs: 1000
```

## Field reference

### `acp.enabled`

- Type: boolean
- Default: `false`

Controls whether ACP server mode is considered enabled in the effective config.
In practice, `xzatoma acp serve` enables ACP server operation for the current
process even if the config file value is `false`.

### `acp.host`

- Type: string
- Default: `"127.0.0.1"`

The bind host for the ACP HTTP server. The current implementation requires a
literal IP address. Hostnames such as `localhost` are not accepted by config
validation for server binding.

Examples:

- `127.0.0.1`
- `0.0.0.0`

### `acp.port`

- Type: integer
- Default: `8765`

The TCP port used by the ACP HTTP server.

Validation rules:

- must be greater than `0`

### `acp.compatibility_mode`

- Type: enum
- Default: `versioned`

Controls which route layout the ACP server exposes.

Supported values:

- `versioned`
- `root_compatible`

#### `versioned`

ACP endpoints are nested under `acp.base_path`.

Example base URLs:

- `/api/v1/acp/ping`
- `/api/v1/acp/agents`
- `/api/v1/acp/runs`

This is the default mode because it reduces collision risk with unrelated root
paths.

#### `root_compatible`

ACP endpoints are exposed directly at ACP-style root paths.

Examples:

- `/ping`
- `/agents`
- `/runs`
- `/sessions/{session_id}`

This mode is useful when you want stricter ACP-style path compatibility.

### `acp.base_path`

- Type: string
- Default: `"/api/v1/acp"`

The base path used when `acp.compatibility_mode = versioned`.

Validation rules:

- must not be empty
- must start with `/`
- must not be exactly `/` in `versioned` mode

In `root_compatible` mode, this field is still validated as a non-empty absolute
path for configuration consistency, but it is not used for route mounting.

### `acp.default_run_mode`

- Type: enum
- Default: `async`

Controls the default ACP run execution mode when a request does not explicitly
set one.

Supported values:

- `sync`
- `async`
- `streaming`

Notes:

- ACP HTTP payloads currently use the runtime execution modes exposed by the
  server lifecycle implementation.
- `async` is the default advertised operator mode.
- streaming behavior is implemented through server-sent events.

### `acp.persistence.enabled`

- Type: boolean
- Default: `false`

Enables persistence-oriented ACP retention settings in configuration.

Current behavior:

- ACP run, session, event, await, and cancellation state are integrated with the
  SQLite-backed storage layer used by the project
- this flag currently acts as an operator-facing persistence tuning switch and
  future compatibility anchor
- persistence and recovery behavior should still be treated as best-effort
  operational support, not a formal durability SLA

### `acp.persistence.max_events_per_run`

- Type: integer
- Default: `1000`

Maximum retained ACP events per run.

Validation rules:

- must be greater than `0`

### `acp.persistence.max_completed_runs`

- Type: integer
- Default: `1000`

Maximum retained completed ACP runs.

Validation rules:

- must be greater than `0`

## Environment variable overrides

The following environment variables override ACP configuration fields:

| Environment variable              | Field                                |
| --------------------------------- | ------------------------------------ |
| `XZATOMA_ACP_ENABLED`             | `acp.enabled`                        |
| `XZATOMA_ACP_HOST`                | `acp.host`                           |
| `XZATOMA_ACP_PORT`                | `acp.port`                           |
| `XZATOMA_ACP_COMPATIBILITY_MODE`  | `acp.compatibility_mode`             |
| `XZATOMA_ACP_BASE_PATH`           | `acp.base_path`                      |
| `XZATOMA_ACP_DEFAULT_RUN_MODE`    | `acp.default_run_mode`               |
| `XZATOMA_ACP_PERSISTENCE_ENABLED` | `acp.persistence.enabled`            |
| `XZATOMA_ACP_MAX_EVENTS_PER_RUN`  | `acp.persistence.max_events_per_run` |
| `XZATOMA_ACP_MAX_COMPLETED_RUNS`  | `acp.persistence.max_completed_runs` |

### Boolean environment values

Boolean ACP environment variables accept these values:

- true values: `1`, `true`, `yes`, `on`
- false values: `0`, `false`, `no`, `off`

### Compatibility mode environment values

`XZATOMA_ACP_COMPATIBILITY_MODE` supports:

- `versioned`
- `root_compatible`

### Default run mode environment values

`XZATOMA_ACP_DEFAULT_RUN_MODE` supports:

- `sync`
- `async`
- `streaming`

## CLI overrides

The `xzatoma acp serve` command supports temporary runtime overrides for:

- `--host`
- `--port`
- `--base-path`
- `--root-compatible`

These affect the current process only and take precedence over file and
environment configuration.

Example:

```/dev/null/shell.sh#L1-1
xzatoma acp serve --host 0.0.0.0 --port 9000 --base-path /acp
```

## Example configurations

### Default versioned ACP server

```/dev/null/config.yaml#L1-9
acp:
  enabled: false
  host: "127.0.0.1"
  port: 8765
  compatibility_mode: versioned
  base_path: "/api/v1/acp"
  default_run_mode: async
  persistence:
    enabled: false
```

### Root-compatible ACP deployment

```/dev/null/config.yaml#L1-9
acp:
  enabled: false
  host: "0.0.0.0"
  port: 8765
  compatibility_mode: root_compatible
  base_path: "/api/v1/acp"
  default_run_mode: async
  persistence:
    enabled: true
```

### Tuned persistence retention

```/dev/null/config.yaml#L1-11
acp:
  enabled: false
  host: "127.0.0.1"
  port: 8765
  compatibility_mode: versioned
  base_path: "/api/v1/acp"
  default_run_mode: async
  persistence:
    enabled: true
    max_events_per_run: 2000
    max_completed_runs: 5000
```

## Validation behavior

ACP configuration validation currently enforces:

- `acp.host` must not be empty
- `acp.port` must be greater than `0`
- `acp.base_path` must not be empty
- `acp.base_path` must start with `/`
- `acp.base_path` must not be `/` in `versioned` mode
- `acp.persistence.max_events_per_run` must be greater than `0`
- `acp.persistence.max_completed_runs` must be greater than `0`

## Operational notes

### Path compatibility caveat

XZatoma supports both a project-friendly versioned path layout and a
root-compatible ACP path layout. If you need ACP-style root paths, set
`compatibility_mode` to `root_compatible`.

### Persistence and recovery caveat

ACP persistence is integrated with the existing SQLite storage layer. Session,
run, and event data can survive process restart, but this should be treated as
practical recovery support rather than a formal guarantee of zero-loss recovery
under every failure mode.

### Authentication caveat

ACP server authentication is not yet fully implemented as a hardened production
feature. If you expose the ACP server beyond localhost, you should place it
behind an authenticated reverse proxy or other trusted network boundary.

### Multimodal caveat

The current ACP implementation is primarily text-oriented. Multimodal and
artifact-heavy request handling has compatibility gaps and should be considered
partial.

### Await and resume caveat

Await and resume semantics are implemented, but partial compatibility caveats
may still exist between ACP client expectations and the underlying XZatoma
execution model.

## Stdio ACP configuration (`acp.stdio`)

These fields control the behavior of `xzatoma agent`, the stdio ACP subprocess.
All fields live under `acp.stdio:` in `config/config.yaml`.

### `acp.stdio.persist_sessions`

- Type: boolean
- Default: `true`

When true, ACP stdio session mappings are written to the local SQLite database.
This enables workspace-based conversation resume across subprocess restarts.

### `acp.stdio.resume_by_workspace`

- Type: boolean
- Default: `true`

When true, a new session for a known workspace automatically rehydrates the most
recent conversation stored for that workspace. Set to `false` to always start
fresh conversations.

### `acp.stdio.max_active_sessions`

- Type: integer
- Default: `32`

Maximum number of concurrent ACP stdio sessions in a single subprocess. Requests
that would exceed this limit are rejected with a configuration error.

### `acp.stdio.session_timeout_seconds`

- Type: integer
- Default: `3600`

Inactive session timeout in seconds. Sessions idle longer than this value may be
pruned from the in-memory registry during cleanup passes.

### `acp.stdio.prompt_queue_capacity`

- Type: integer
- Default: `8`

Maximum number of prompts that can be queued for a single session at one time.
When the queue is full, additional prompt requests are rejected immediately with
a descriptive error. Increase this if your workflow submits many prompts in
rapid succession.

### `acp.stdio.model_list_timeout_seconds`

- Type: integer
- Default: `5`

Timeout in seconds for the model advertisement request sent to the provider
during session creation. If the provider does not respond within this window,
XZatoma falls back to advertising only the current model.

### `acp.stdio.vision_enabled`

- Type: boolean
- Default: `true`

Controls whether image content blocks in ACP prompt requests are accepted. When
`false`, any prompt that includes image content is rejected with a clear error
before reaching the provider. Set to `false` when using a text-only provider to
avoid misleading capability advertisement.

Vision support also depends on the provider and model. See
`docs/how-to/zed_acp_agent_setup.md` for per-provider guidance.

### `acp.stdio.max_image_bytes`

- Type: integer
- Default: `5242880` (5 MiB)

Maximum decoded byte size for a single inline base64 image. Images that exceed
this limit are rejected. Reduce this value on memory-constrained systems.

### `acp.stdio.allowed_image_mime_types`

- Type: list of strings
- Default: `["image/png", "image/jpeg", "image/webp", "image/gif"]`

MIME types accepted from ACP prompt image content blocks. Requests containing
unsupported MIME types are rejected with a descriptive error.

### `acp.stdio.allow_image_file_references`

- Type: boolean
- Default: `true`

When true, image content blocks that reference a local file path are resolved
from the session workspace root. When `false`, only inline base64 image data is
accepted.

### `acp.stdio.allow_remote_image_urls`

- Type: boolean
- Default: `false`

When true, image content blocks that contain `http://` or `https://` URLs are
fetched and decoded. Disabled by default to prevent unintended outbound
requests.

## Stdio environment variable overrides

The following environment variables override `acp.stdio` fields:

| Environment variable                            | Field                                   |
| ----------------------------------------------- | --------------------------------------- |
| `XZATOMA_ACP_STDIO_PERSIST_SESSIONS`            | `acp.stdio.persist_sessions`            |
| `XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE`         | `acp.stdio.resume_by_workspace`         |
| `XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS`         | `acp.stdio.max_active_sessions`         |
| `XZATOMA_ACP_STDIO_SESSION_TIMEOUT`             | `acp.stdio.session_timeout_seconds`     |
| `XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY`       | `acp.stdio.prompt_queue_capacity`       |
| `XZATOMA_ACP_STDIO_VISION_ENABLED`              | `acp.stdio.vision_enabled`              |
| `XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES`             | `acp.stdio.max_image_bytes`             |
| `XZATOMA_ACP_STDIO_ALLOW_IMAGE_FILE_REFERENCES` | `acp.stdio.allow_image_file_references` |
| `XZATOMA_ACP_STDIO_ALLOW_REMOTE_IMAGE_URLS`     | `acp.stdio.allow_remote_image_urls`     |

Boolean values follow the same conventions as the HTTP ACP environment
variables: true values are `1`, `true`, `yes`, `on`; false values are `0`,
`false`, `no`, `off`.

## Stdio configuration example

```yaml
acp:
  stdio:
    persist_sessions: true
    resume_by_workspace: true
    max_active_sessions: 32
    session_timeout_seconds: 3600
    prompt_queue_capacity: 8
    model_list_timeout_seconds: 5
    vision_enabled: true
    max_image_bytes: 5242880
    allowed_image_mime_types:
      - image/png
      - image/jpeg
      - image/webp
      - image/gif
    allow_image_file_references: true
    allow_remote_image_urls: false
```

## Related documentation

- `docs/how-to/run_xzatoma_as_an_acp_server.md`
- `docs/reference/acp_api.md`
- `docs/explanation/acp_implementation.md`
- `docs/how-to/zed_acp_agent_setup.md`
- `docs/explanation/zed_acp_agent_command_implementation.md`
