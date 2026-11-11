# Sandboxing Phase 4: Network Sandbox and Fetch Tool Hardening

## Overview

Phase 4 completes the network layer of the XZatoma sandbox. It introduces a
host-filtering HTTP CONNECT proxy (`SandboxProxy`), extends `SsrfValidator` and
`FetchTool` with an explicit hostname allowlist, threads the proxy address
through `ToolRegistryBuilder`, and stores the running proxy in
`AgentEnvironment` so it is shut down automatically when the environment is
dropped.

No existing behavior changes for configurations that do not set
`sandbox.enabled = true` and `sandbox.network_enabled = true`.

---

## Changes

### Task 4.1 -- `src/sandbox/proxy.rs`

#### `SandboxProxy` struct

```rust
pub struct SandboxProxy {
    addr: std::net::SocketAddr,
    cancel: tokio_util::sync::CancellationToken,
}
```

#### Public API

| Method                                                                   | Description                          |
| ------------------------------------------------------------------------ | ------------------------------------ |
| `SandboxProxy::start(cfg: &SandboxConfig) -> Result<Self, SandboxError>` | Binds listener, spawns accept loop   |
| `SandboxProxy::addr() -> SocketAddr`                                     | Returns the bound local address      |
| `SandboxProxy::shutdown()`                                               | Cancels the accept loop (idempotent) |

`Drop` is implemented on `SandboxProxy`; it calls `self.cancel.cancel()` so the
accept loop is stopped even if `shutdown` is never called explicitly.

#### Accept loop and connection handler

The accept loop runs in a background Tokio task until the `CancellationToken` is
triggered. Each accepted `TcpStream` is dispatched to its own `tokio::spawn`
task via the private `handle_connection` async function.

`handle_connection` processes HTTP CONNECT tunnels:

| Step                               | On failure           | Response                          |
| ---------------------------------- | -------------------- | --------------------------------- |
| Read up to 4 096 bytes             | I/O error or EOF     | silent return                     |
| Validate UTF-8                     | invalid bytes        | 400 Bad Request                   |
| Parse `CONNECT host:port HTTP/1.x` | wrong method         | 400 Bad Request                   |
| Parse port as `u16`                | non-numeric          | 400 Bad Request                   |
| Check allowlist                    | host not listed      | 407 Proxy Authentication Required |
| `SsrfValidator::validate`          | private/loopback     | 407 Proxy Authentication Required |
| `TcpStream::connect`               | upstream unreachable | 502 Bad Gateway                   |
| `copy_bidirectional` relay         | reset                | error ignored                     |

#### `CancellationToken` note

`tokio_util::sync::CancellationToken` is available in tokio-util 0.7.x without
any additional Cargo feature flag. The `sync` module is compiled
unconditionally. No `Cargo.toml` changes are needed beyond the existing
`tokio-util` dependency.

#### Tests

Three tests are included in the module:

| Test                                          | Hosts config      | CONNECT target      | Expected |
| --------------------------------------------- | ----------------- | ------------------- | -------- |
| `test_proxy_blocks_unlisted_host`             | `[]`              | `evil.internal:443` | 407      |
| `test_proxy_blocks_private_ip_even_if_listed` | `["localhost"]`   | `localhost:80`      | 407      |
| `test_proxy_allows_listed_host` (ignored)     | `["example.com"]` | `example.com:80`    | 200      |

The network test is gated with `#[ignore = "requires network access"]`.

---

### Task 4.2 -- `src/tools/fetch.rs` and `src/mention_parser.rs`

#### `SsrfValidator` -- new `allowed_hosts` field

```rust
pub struct SsrfValidator {
    allow_private_ips: bool,
    // ...
    allowed_hosts: Option<Vec<String>>,
}
```

New builder method:

```rust
pub fn with_allowed_hosts(mut self, hosts: Vec<String>) -> Self
```

When `allowed_hosts` is `Some`, `validate` checks the requested hostname against
the list **before** the SSRF host check. A host absent from the list is rejected
with `XzatomaError::SsrfAttempt`.

Order of checks in `SsrfValidator::validate`:

1. URL parse
2. Allowlist check (new -- only when `allowed_hosts` is `Some`)
3. `validate_host` (private IP / loopback guard)
4. Port check

#### `FetchTool::new` -- new `sandbox_cfg` parameter

```rust
pub fn new(
    timeout: Duration,
    max_size_bytes: usize,
    sandbox_cfg: Option<&SandboxConfig>,
) -> Self
```

When `sandbox_cfg` is `Some` and `sandbox.enabled = true`, the constructor calls
`validator.with_allowed_hosts(sandbox_cfg.network_allowed_hosts.clone())` to
enforce the same allowlist used by the proxy for in-process fetch calls.

All existing callers were updated to pass `None` as the third argument,
retaining prior behavior.

---

### Task 4.3 -- `src/tools/registry_builder.rs`

#### New fields on `ToolRegistryBuilder`

```rust
config: Option<Arc<Config>>,
proxy_addr: Option<std::net::SocketAddr>,
```

Both are initialized to `None` in `ToolRegistryBuilder::new`.

#### New builder methods

```rust
pub fn with_config(mut self, cfg: Arc<Config>) -> Self
pub fn with_proxy_addr(mut self, addr: std::net::SocketAddr) -> Self
```

`build_for_write` uses the config when present:

- Calls `TerminalTool::from_config(cfg, working_dir)?` instead of
  `TerminalTool::new(working_dir)` when `config` is `Some`.
- Calls `terminal.with_proxy_addr(addr)` when `proxy_addr` is `Some`.

#### `build_tools_for_mode` in `src/commands/mod.rs`

The sync helper used during hot-reload (`handle_config_reload`) now chains
`.with_config(Arc::new(config.clone()))` so sandbox policy is respected on mode
switches.

---

### Task 4.3 -- `src/commands/environment.rs`

#### `AgentEnvironment::sandbox_proxy` field

```rust
pub sandbox_proxy: Option<SandboxProxy>,
```

The field is `None` when `sandbox.enabled = false` or
`sandbox.network_enabled = false` (the defaults). When both are `true`,
`build_agent_environment` starts the proxy and stores it in this field.

#### Proxy lifecycle in `build_agent_environment`

Step 4 of the build sequence:

```rust
let mut sandbox_proxy: Option<SandboxProxy> = None;
let mut proxy_addr: Option<std::net::SocketAddr> = None;
if config.sandbox.enabled && config.sandbox.network_enabled {
    match SandboxProxy::start(&config.sandbox).await {
        Ok(proxy) => {
            proxy_addr = Some(proxy.addr());
            sandbox_proxy = Some(proxy);
        }
        Err(e) => {
            tracing::warn!("sandbox proxy failed to start: {}", e);
        }
    }
}
```

Proxy startup failure is treated as a recoverable warning rather than a fatal
error; the agent continues without network filtering in that case.

The `proxy_addr` is passed to `ToolRegistryBuilder::with_proxy_addr` so that
`TerminalTool` injects `HTTP_PROXY` / `HTTPS_PROXY` variables into every child
process.

The `sandbox_proxy` is included in the returned `AgentEnvironment`. When the
environment is eventually dropped (session end, reload, error unwind), the
`Drop` impl on `SandboxProxy` cancels the accept loop without any explicit
teardown call.

---

## Interaction with Default Config

The default `SandboxConfig` has:

```yaml
sandbox:
  enabled: true
  network_enabled: false
```

Because `network_enabled` defaults to `false`, the proxy is **not** started in
any default configuration. `AgentEnvironment::sandbox_proxy` will be `None` for
all users who have not explicitly opted into network sandboxing.

---

## Tests Added (Task 4.4)

All tests continue to use `--skip providers::copilot --skip mcp::auth` as
required by the keyring isolation rule.

The existing
`test_build_agent_environment_headless_true_no_mcp_manager_by_default` test
passes without modification: the default config has `network_enabled: false` so
`sandbox_proxy` is `None` and no network calls are made during the test.

---

## Documentation Note

A draft document `docs/explanation/phase4_sandbox_proxy_implementation.md` was
created by a subagent during development. Its content has been incorporated
here. That file should be removed once this document is finalized.
