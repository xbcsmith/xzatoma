# ACP configuration reference

This reference documents the ACP server configuration surface in XZatoma for
Phase 5 operator use.

## Overview

ACP support is configured under the top-level `acp` section in
`config/config.yaml`. The effective configuration is derived in this order:

1. file-based configuration
2. environment variable overrides
3. CLI subcommand overrides for `xzatoma acp serve`

The ACP server is disabled by default and only starts when you explicitly invoke
the ACP server command.

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

## Related documentation

- `docs/how-to/run_xzatoma_as_an_acp_server.md`
- `docs/reference/acp_api.md`
- `docs/explanation/acp_implementation.md`
