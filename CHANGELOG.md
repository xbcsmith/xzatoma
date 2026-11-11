# Changelog

All notable changes to XZatoma are documented here.

---

## Unreleased

### Breaking Changes

#### Sandbox enabled by default

The `sandbox.enabled` field now defaults to `true`. The sandbox module is
scaffolded in this release (Phase 1); the kernel enforcement engine lands in
Phase 2. Until Phase 2 is merged, the config field is parsed and validated but
no OS-level restriction is applied.

Workflows that are expected to work outside the project directory, issue network
calls from terminal commands, or write into `.git` will fail once Phase 2
enforcement is active. Escape hatches:

- Set `sandbox.enabled: false` in the config file to disable all sandboxing.
- Set the environment variable `XZATOMA_SANDBOX_ENABLED=false`.
- Add required paths to `sandbox.extra_write_paths` for targeted access.

### Added

- `SandboxError` enum in `src/error.rs` with variants for Landlock ABI version
  errors, Seatbelt failures, policy path errors, path-denied errors, and a
  generic engine error.
- `XzatomaError::Sandbox(String)` variant and `From<SandboxError>` conversion.
- `SandboxConfig` struct in `src/config.rs` with fields: `enabled`,
  `sandboxed_tools`, `filesystem_root`, `extra_read_paths`, `extra_write_paths`,
  `protect_git`, `network_enabled`, `network_allowed_hosts`, and `proxy_port`.
- `sandbox: SandboxConfig` field added to the top-level `Config` struct.
- Environment variable overrides: `XZATOMA_SANDBOX_ENABLED`,
  `XZATOMA_SANDBOX_FILESYSTEM_ROOT`, `XZATOMA_SANDBOX_PROTECT_GIT`,
  `XZATOMA_SANDBOX_NETWORK_ENABLED`, `XZATOMA_SANDBOX_NETWORK_ALLOWED_HOSTS`,
  and `XZATOMA_SANDBOX_PROXY_PORT`.
- CLI flags `--no-sandbox` and `--sandbox-root` in `CommonArgs`.
- `Config::validate_sandbox_config` validates proxy port range, allowed-host
  format, path entries, and tool key names.
- `src/sandbox/` module registered in `src/lib.rs` with stub files for `engine`,
  `guard`, `policy`, and `proxy` submodules (implementations in Phases 2-5).
- `SandboxPolicy` builder struct in `src/sandbox/policy.rs` with `allow_read`,
  `allow_write`, `deny_write`, `allow_execute`, and `with_network` builder
  methods, plus `SandboxPolicy::from_config` for deriving a policy from
  `SandboxConfig`.
- `SandboxContext` zero-sized proof type returned by a successful
  `apply_sandbox` call.
- `apply_sandbox` in `src/sandbox/engine.rs`: applies Landlock (Linux) or
  Seatbelt (macOS) restrictions to the current process via the `nono` crate.
- `nono = "0.81"` dependency added to `Cargo.toml`.
- `network_blocked_programs: Vec<String>` field on `CommandValidator`; populated
  by `CommandValidator::from_config` when sandbox is enabled and network is
  disabled.
- Two new unconditional denylist patterns in `CommandValidator::new`: `nc -l`
  (netcat listen/execute) and `socat EXEC` (execute relay).
- Network-blocking check added as the first step in
  `CommandValidator::validate`; returns `DangerousCommand` with an actionable
  message when a network tool is invoked with `sandbox.network_enabled = false`.
- `CommandValidator::from_config` constructor.
- `.git` directory path guard in `CommandValidator::validate_paths`; rejects any
  argument whose resolved path contains a `.git` component.
- `sandbox_policy` and `proxy_addr` fields on `TerminalTool`.
- `TerminalTool::with_sandbox_policy` and `with_proxy_addr` builder methods.
- `TerminalTool::from_config` constructor.
- `execute_command` convenience function updated with
  `sandbox_config: Option<&SandboxConfig>` parameter.
- Combined `setpgid` and `apply_sandbox` in a single `pre_exec` closure; proxy
  environment variables (`HTTP_PROXY`, `HTTPS_PROXY`, etc.) injected into child
  processes when `proxy_addr` is set.
- `SandboxProxy` struct in `src/sandbox/proxy.rs` with `start`, `addr`, and
  `shutdown` methods; binds a local `TcpListener` and runs an accept loop that
  enforces `network_allowed_hosts` plus SSRF double-check before relaying HTTP
  CONNECT tunnels.
- `Drop` implementation on `SandboxProxy` cancels the accept loop automatically
  on drop so the background task does not outlive its owner.
- `SsrfValidator::allowed_hosts` field and `with_allowed_hosts` builder method;
  allowlist check inserted in `SsrfValidator::validate` before the private-IP
  guard.
- `FetchTool::new` updated with `sandbox_cfg: Option<&SandboxConfig>` third
  parameter; passes `network_allowed_hosts` into `SsrfValidator` when sandbox is
  enabled.
- `ToolRegistryBuilder::with_config(Arc<Config>)` and
  `ToolRegistryBuilder::with_proxy_addr(SocketAddr)` builder methods.
- `AgentEnvironment::sandbox_proxy: Option<SandboxProxy>` field; proxy started
  in `build_agent_environment` when
  `sandbox.enabled && sandbox.network_enabled`.
- `build_tools_for_mode` in `src/commands/mod.rs` chains `.with_config` so
  sandbox policy is respected during hot-reload mode switches.
