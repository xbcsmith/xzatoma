# Ollama Streaming Connection Fix

## Problem

When running xzatoma as a Zed ACP stdio agent, the first user prompt
consistently failed with:

```text
Internal error: "prompt execution failed: Provider HTTP request failed:
provider=ollama, endpoint=api/chat:stream: error sending request for url
(http://localhost:11434/api/chat)"
```

The interactive `chat` command worked because it uses a `NoOpObserver`, which
bypasses the streaming path. The ACP agent always uses `AcpSessionObserver` (a
real observer), which triggers `complete_with_callbacks` which in turn calls
`complete_streaming_with_callbacks` for every prompt.

## Root Cause

Two compounding factors caused the failure.

### 1. DNS dual-stack lookup for `localhost`

When the Ollama host is configured as `http://localhost:11434`, the OS resolver
returns both the IPv6 address `::1` and the IPv4 address `127.0.0.1`. Ollama
only binds to the IPv4 loopback on macOS (and most Linux systems), so:

- hyper-util's `HttpConnector` attempts `[::1]:11434` first.
- The connection is immediately refused (ECONNREFUSED).
- hyper falls back to `127.0.0.1:11434`, which succeeds.

The fallback works for the initial connection. However, `reqwest` and hyper 1.x
do not retry failed pool connections for non-idempotent methods (POST). If the
pool entry that was established via the IPv4 fallback is later reused for a POST
request and that entry has gone stale, the request fails with "error sending
request for url" and no underlying cause is surfaced.

Debug logging confirmed the dual-stack behavior:

```text
hyper_util::client::legacy::connect::http: connecting to [::1]:11434
hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:11434
hyper_util::client::legacy::connect::http: connected to 127.0.0.1:11434
```

### 2. Stale connection pool entry for POST requests

During ACP session creation, the provider factory calls `list_models()` and
`fetch_model_details()` for each installed model. This makes five or more HTTP
requests to Ollama, pooling connections after each one. The connections are
keyed by `("http", localhost:11434)` and point to `127.0.0.1`.

When the user eventually sends the first prompt in Zed (which may be seconds to
minutes after the session was created), the streaming POST to `/api/chat` reuses
one of those pooled connections. If Ollama has since closed its end of the
socket (e.g., due to OS-level TCP teardown or an idle-connection timeout from a
downstream proxy), hyper detects the stale connection but does NOT retry because
POST is not idempotent. The result is "error sending request for url".

## Fix

Three changes were made.

### Change 1: Default host changed to `127.0.0.1`

`default_ollama_host()` in `src/config.rs` now returns
`"http://127.0.0.1:11434"` instead of `"http://localhost:11434"`. Using a
literal IPv4 address skips the dual-stack DNS lookup entirely, eliminating the
IPv6 attempt and reducing connection setup to a single direct socket.

### Change 2: reqwest client hardened in `OllamaProvider::new()`

Three new options were added to the `Client::builder()` call in
`src/providers/ollama.rs`:

| Option              | Value | Purpose                                                  |
| ------------------- | ----- | -------------------------------------------------------- |
| `connect_timeout`   | 10 s  | Fail fast on refused addresses; limits IPv6 delay        |
| `pool_idle_timeout` | 90 s  | Evict idle pool entries before they go stale             |
| `tcp_keepalive`     | 30 s  | OS sends keepalive probes; dead connections are detected |

`pool_idle_timeout(90s)` ensures that a connection idle for more than 90 seconds
is dropped from the pool. A new connection is then established on the next
request, which avoids the "stale pool entry for POST" failure mode entirely.

`tcp_keepalive(30s)` causes the OS to begin sending TCP keepalive probes after
the connection has been idle for 30 seconds. If Ollama has already closed the
server side, the probe will receive a RST and the connection is evicted before
it can be mistakenly reused.

`connect_timeout(10s)` bounds the time spent waiting for a connection to be
established, allowing failed IPv6 or slow-connection attempts to be abandoned
quickly.

### Change 3: Config files updated

All project-provided YAML configuration files (in `config/` and `demos/`) and
the user config files in `~/.config/xzatoma/` were updated to use
`http://127.0.0.1:11434` in place of `http://localhost:11434`.

## Files Changed

| File                                   | Change                                                                           |
| -------------------------------------- | -------------------------------------------------------------------------------- |
| `src/config.rs`                        | `default_ollama_host()` returns `127.0.0.1`                                      |
| `src/providers/ollama.rs`              | `Client::builder()` adds `connect_timeout`, `pool_idle_timeout`, `tcp_keepalive` |
| `config/config.yaml`                   | Host updated                                                                     |
| `config/openai_config.yaml`            | Host updated                                                                     |
| `demos/*/config.yaml`                  | Host updated in all demo configs                                                 |
| `~/.config/xzatoma/config.yaml`        | Host updated                                                                     |
| `~/.config/xzatoma/config_ollama.yaml` | Host updated                                                                     |

## Validation

All quality gates pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

2442 tests pass, 0 failures.
