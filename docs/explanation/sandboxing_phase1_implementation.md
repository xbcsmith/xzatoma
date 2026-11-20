# Sandboxing Phase 1 Implementation

## Overview

Phase 1 of the sandboxing feature establishes the scaffolding that all later
phases depend on: error types, a configuration schema, module registration,
environment variable overrides, CLI flags, and configuration validation. No
behavioral change is introduced in this phase. The sandbox module exists but
applies no OS-level restrictions; that is implemented in Phase 2.

---

## Deliverables

### `SandboxError` enum (`src/error.rs`)

A new `SandboxError` enum was added before `XzatomaError`. It carries five
variants:

| Variant               | Meaning                                                    |
| --------------------- | ---------------------------------------------------------- |
| `KernelVersionTooOld` | Landlock ABI version is below the required minimum         |
| `SeatbeltApplyFailed` | macOS `sandbox_init` returned a non-zero status            |
| `PolicyPathError`     | A path referenced in the policy could not be canonicalized |
| `PathDenied`          | A file-operation tool attempted a disallowed access        |
| `Engine`              | Generic sandbox engine failure                             |

A `Sandbox(String)` variant was added to `XzatomaError`, and a
`From<SandboxError>` conversion was implemented so any sandboxing call site can
propagate errors with `?`.

### `SandboxConfig` struct (`src/config.rs`)

`SandboxConfig` is the top-level configuration object for the sandbox layer. It
is deserialized under the `sandbox:` key in the YAML configuration file.

Key fields and their defaults:

| Field                   | Type                    | Default           | Purpose                              |
| ----------------------- | ----------------------- | ----------------- | ------------------------------------ |
| `enabled`               | `bool`                  | `true`            | Master switch for all sandboxing     |
| `sandboxed_tools`       | `HashMap<String, bool>` | empty             | Per-tool opt-out                     |
| `filesystem_root`       | `Option<String>`        | `None` (uses CWD) | Agent filesystem root                |
| `extra_read_paths`      | `Vec<String>`           | empty             | Additional read-only paths           |
| `extra_write_paths`     | `Vec<String>`           | empty             | Additional writable paths            |
| `protect_git`           | `bool`                  | `true`            | Prevent writes to `.git` directories |
| `network_enabled`       | `bool`                  | `false`           | Allow outbound network from terminal |
| `network_allowed_hosts` | `Vec<String>`           | empty             | Permitted hostnames                  |
| `proxy_port`            | `u16`                   | `8877`            | Local HTTP CONNECT proxy port        |

The struct carries `#[serde(default)]` so the entire `sandbox:` section may be
omitted from the config file; all fields then use the `Default` impl.

### Module registration (`src/lib.rs`)

`pub mod sandbox;` was added to `src/lib.rs`. The module hierarchy is:

```text
src/sandbox/
    mod.rs     -- re-exports and module declarations
    engine.rs  -- apply_sandbox stub (Phase 2)
    guard.rs   -- SandboxGuard stub (Phase 5)
    policy.rs  -- SandboxPolicy, SandboxContext stubs (Phase 2)
    proxy.rs   -- SandboxProxy stub (Phase 4)
```

All stub files contain the minimal public types required so the re-exports in
`mod.rs` compile successfully.

### Environment variable overrides

Six new environment variables were added to `apply_env_vars`:

| Variable                                | Field                                             |
| --------------------------------------- | ------------------------------------------------- |
| `XZATOMA_SANDBOX_ENABLED`               | `sandbox.enabled`                                 |
| `XZATOMA_SANDBOX_FILESYSTEM_ROOT`       | `sandbox.filesystem_root`                         |
| `XZATOMA_SANDBOX_PROTECT_GIT`           | `sandbox.protect_git`                             |
| `XZATOMA_SANDBOX_NETWORK_ENABLED`       | `sandbox.network_enabled`                         |
| `XZATOMA_SANDBOX_NETWORK_ALLOWED_HOSTS` | `sandbox.network_allowed_hosts` (comma-separated) |
| `XZATOMA_SANDBOX_PROXY_PORT`            | `sandbox.proxy_port`                              |

### CLI flags

Two flags were added to `CommonArgs`:

- `--no-sandbox`: equivalent to `sandbox.enabled = false` in config.
- `--sandbox-root <PATH>`: overrides `sandbox.filesystem_root`.

### Validation (`validate_sandbox_config`)

`Config::validate_sandbox_config` is called from `Config::validate` and checks:

- `proxy_port` must be `>= 1024`.
- Each entry in `network_allowed_hosts` must be non-empty and must not contain a
  protocol prefix (`://`).
- All `extra_read_paths` and `extra_write_paths` entries must be non-empty.
- `filesystem_root`, when set, must not be an empty string.
- Keys in `sandboxed_tools` must be one of `"terminal"`, `"fetch"`,
  `"file_ops"`.
- A non-fatal warning is emitted when `network_allowed_hosts` is non-empty but
  `network_enabled` is `false`.

---

## Breaking Change

`sandbox.enabled` defaults to `true`. YAML configs that previously relied on no
sandbox section being present will now have sandboxing enabled by default once
Phase 2 enforcement is active. See `CHANGELOG.md` for escape hatches.

---

## Test Coverage

Twelve unit tests were added to the `mod tests` block in `src/config.rs`:

- `test_sandbox_config_defaults` -- verifies Default impl values
- `test_sandbox_config_deserialize_all_fields` -- full YAML round-trip
- `test_sandbox_config_deserialize_omitted_fields_use_defaults` -- partial YAML
- `test_sandbox_config_validation_rejects_proxy_port_below_1024`
- `test_sandbox_config_validation_rejects_protocol_prefix_in_allowed_host`
- `test_sandbox_config_validation_rejects_empty_allowed_host`
- `test_sandbox_config_validation_rejects_invalid_tool_key`
- `test_sandbox_config_validation_warns_when_allowed_hosts_set_but_network_disabled`
- `test_apply_env_vars_overrides_sandbox_enabled`
- `test_apply_env_vars_overrides_sandbox_filesystem_root`
- `test_apply_env_vars_overrides_sandbox_proxy_port`
- `test_apply_env_vars_overrides_sandbox_network_allowed_hosts`

Six unit tests were added to `src/error.rs`:

- `test_sandbox_error_kernel_version_too_old_display`
- `test_sandbox_error_seatbelt_apply_failed_display`
- `test_sandbox_error_policy_path_error_display`
- `test_sandbox_error_path_denied_display`
- `test_sandbox_error_engine_display`
- `test_sandbox_error_converts_to_xzatoma_error`

---

## Next Steps

Phase 2 will replace the stub `apply_sandbox` function in
`src/sandbox/engine.rs` with real Landlock (Linux) and Seatbelt (macOS)
implementations, and will implement `SandboxPolicy` and `SandboxContext` in
`src/sandbox/policy.rs`.
