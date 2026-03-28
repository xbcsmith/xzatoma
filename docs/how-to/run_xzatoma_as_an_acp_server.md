# Run XZatoma as an ACP server

This guide shows you how to start XZatoma in ACP server mode, inspect the
effective ACP configuration, validate ACP setup, and confirm that the service is
responding on its discovery and run endpoints.

## Before you begin

Make sure you have:

- a working Rust toolchain
- the XZatoma repository checked out locally
- a valid `config/config.yaml` file, or another config file you want to use
- a provider configuration that XZatoma can use for execution

For local development, you can usually start with the repository default config
and override ACP settings from the command line or environment.

## ACP server modes

XZatoma supports two ACP route layouts:

- `versioned` mode, which serves ACP under a base path such as `/api/v1/acp`
- `root_compatible` mode, which serves ACP endpoints directly at paths such as
  `/ping`, `/agents`, `/runs`, and `/sessions`

Use `versioned` mode when you want to avoid path collisions with other HTTP
surfaces. Use `root_compatible` mode when you want ACP-style root paths.

## Step 1: Review the effective ACP configuration

Print the resolved ACP configuration after file and environment overrides:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp config
```

If you want to use a specific config file:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- --config config/config.yaml acp config
```

This is useful for verifying:

- whether ACP is enabled
- which host and port the server will bind to
- which compatibility mode is active
- which base path is configured
- whether persistence settings are enabled

## Step 2: Optionally override ACP settings with environment variables

XZatoma supports ACP-related environment variable overrides.

Common variables include:

- `XZATOMA_ACP_ENABLED`
- `XZATOMA_ACP_HOST`
- `XZATOMA_ACP_PORT`
- `XZATOMA_ACP_COMPATIBILITY_MODE`
- `XZATOMA_ACP_BASE_PATH`
- `XZATOMA_ACP_DEFAULT_RUN_MODE`
- `XZATOMA_ACP_PERSISTENCE_ENABLED`
- `XZATOMA_ACP_MAX_EVENTS_PER_RUN`
- `XZATOMA_ACP_MAX_COMPLETED_RUNS`

Example:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-4
export XZATOMA_ACP_ENABLED=true
export XZATOMA_ACP_HOST=127.0.0.1
export XZATOMA_ACP_PORT=8765
export XZATOMA_ACP_COMPATIBILITY_MODE=versioned
```

Then confirm the effective configuration:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp config
```

## Step 3: Start the ACP server

Start the ACP server with configuration defaults:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp serve
```

Start the ACP server with explicit CLI overrides:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp serve --host 127.0.0.1 --port 8765 --base-path /api/v1/acp
```

Start the ACP server in root-compatible mode:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp serve --host 127.0.0.1 --port 8765 --root-compatible
```

Notes:

- `--root-compatible` switches routing to root ACP paths
- `--base-path` applies to versioned mode
- the configured host must be a valid IP address
- the server runs until you stop it

## Step 4: Check discovery endpoints

If you are using the default versioned layout, verify the ping endpoint:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/ping
```

List available agents:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/agents
```

Fetch the XZatoma ACP manifest:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/agents/xzatoma
```

If you enabled root-compatible mode, use:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-3
curl http://127.0.0.1:8765/ping
curl http://127.0.0.1:8765/agents
curl http://127.0.0.1:8765/agents/xzatoma
```

## Step 5: Create a run

Create a synchronous run:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-17
curl -X POST http://127.0.0.1:8765/api/v1/acp/runs \
  -H 'Content-Type: application/json' \
  -d '{
    "mode": "sync",
    "agentName": "xzatoma",
    "input": [
      {
        "role": "user",
        "parts": [
          {
            "type": "text",
            "data": {
              "text": "Summarize the purpose of this repository in one sentence."
            }
          }
        ]
      }
    ]
  }'
```

Create an asynchronous run:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-17
curl -X POST http://127.0.0.1:8765/api/v1/acp/runs \
  -H 'Content-Type: application/json' \
  -d '{
    "mode": "async",
    "agentName": "xzatoma",
    "input": [
      {
        "role": "user",
        "parts": [
          {
            "type": "text",
            "data": {
              "text": "Count to three."
            }
          }
        ]
      }
    ]
  }'
```

Create a streaming run:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-17
curl -N -X POST http://127.0.0.1:8765/api/v1/acp/runs \
  -H 'Content-Type: application/json' \
  -d '{
    "mode": "stream",
    "agentName": "xzatoma",
    "input": [
      {
        "role": "user",
        "parts": [
          {
            "type": "text",
            "data": {
              "text": "Stream a short greeting."
            }
          }
        ]
      }
    ]
  }'
```

If you are using root-compatible mode, replace `/api/v1/acp/runs` with `/runs`.

## Step 6: Inspect runs and sessions

Once you have a run ID, fetch its current snapshot:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/runs/<run_id>
```

Fetch its event history:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/runs/<run_id>/events
```

Fetch the session and associated runs:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
curl http://127.0.0.1:8765/api/v1/acp/sessions/<session_id>
```

XZatoma also provides CLI-side run inspection for persisted runs:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-2
cargo run -- acp runs
cargo run -- acp runs --limit 10
```

Filter by session ID:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp runs --session-id <session_id>
```

## Step 7: Validate configuration and optional manifest files

Validate ACP configuration only:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp validate
```

Validate an ACP manifest document:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp validate --manifest docs/reference/acp_manifest.json
```

Supported manifest input formats are:

- `.json`
- `.yaml`

## Deployment considerations

### Readiness and liveness

A simple readiness and liveness strategy is:

- use `GET /ping` for root-compatible deployments
- use `GET /api/v1/acp/ping` for versioned deployments

A deployment is generally ready when:

- the process has started
- the configured bind address is listening
- the ping endpoint returns a successful JSON response

### Persistence expectations

ACP run and session inspection depends on the configured storage backend and
runtime persistence behavior. Persistence is intended to support restart-aware
run and session recovery, but you should still treat local development storage
as an operational dependency.

If you want stable run history across restarts, ensure the backing database path
is durable and not ephemeral.

### Authentication caveat

If ACP server deployment is exposed beyond a trusted local environment, place it
behind an authentication and transport security layer. If authentication is not
configured at the ACP surface yet, treat the server as suitable only for trusted
network environments.

## Compatibility caveats

Current ACP support includes important caveats you should understand before
production use:

- multimodal inputs may be partially supported or unsupported depending on the
  payload shape
- await and resume behavior may be partial rather than fully general ACP
  semantics
- `root_compatible` mode changes path exposure and may conflict with other root
  HTTP paths
- persistence and recovery guarantees depend on the available stored ACP state
  and storage health
- ACP-facing authentication may still require external hardening or a reverse
  proxy strategy

## Troubleshooting

### The server fails to start

Check for:

- an invalid `acp.host` value
- a port already in use
- invalid ACP configuration values
- provider configuration problems

Print the resolved configuration first:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-1
cargo run -- acp config
```

### `curl` returns `404 Not Found`

This usually means you used the wrong path layout.

Check whether you are using:

- versioned mode, with `/api/v1/acp/...`
- root-compatible mode, with `/...`

### Run creation fails with `invalid_request`

Review the payload shape carefully. Start with a text-only user message and a
known agent name:

```/dev/null/run_xzatoma_as_an_acp_server.sh#L1-16
{
  "mode": "sync",
  "agentName": "xzatoma",
  "input": [
    {
      "role": "user",
      "parts": [
        {
          "type": "text",
          "data": {
            "text": "hello"
          }
        }
      ]
    }
  ]
}
```

## Next steps

After you can start the server and exercise discovery and run endpoints, read:

- `docs/reference/acp_api.md`
- `docs/reference/acp_configuration.md`
- `docs/explanation/acp_implementation.md`
