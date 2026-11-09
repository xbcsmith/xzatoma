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
