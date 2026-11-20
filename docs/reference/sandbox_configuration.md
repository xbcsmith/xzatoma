# Sandbox Configuration Reference

## Overview

The sandbox layer controls what files, network hosts, and system commands the
agent may access during a session. Configuration lives under the `sandbox:` key
in the YAML configuration file. All fields are optional; when the `sandbox:`
section is omitted entirely, the defaults described below apply.

## Configuration Fields

| Field                   | Type                    | Default                         | Environment Variable                                      | Description                                                                                                                                                                  |
| ----------------------- | ----------------------- | ------------------------------- | --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `enabled`               | `bool`                  | `true`                          | `XZATOMA_SANDBOX_ENABLED`                                 | Master switch. When `false`, all sandbox enforcement is disabled. The `--no-sandbox` CLI flag also sets this to `false`.                                                     |
| `sandboxed_tools`       | `HashMap<String, bool>` | all tools sandboxed             | none                                                      | Per-tool sandbox opt-out. Valid keys: `terminal`, `fetch`, `file_ops`. Set a key to `false` to disable sandbox enforcement for that tool only.                               |
| `filesystem_root`       | `Option<String>`        | `None` (uses working directory) | `XZATOMA_SANDBOX_FILESYSTEM_ROOT`                         | Absolute path that the agent treats as its filesystem root. The `--sandbox-root <path>` CLI flag also sets this value.                                                       |
| `extra_read_paths`      | `Vec<String>`           | `[]`                            | none                                                      | Additional absolute paths the agent may read. Applied on top of `filesystem_root`.                                                                                           |
| `extra_write_paths`     | `Vec<String>`           | `[]`                            | none                                                      | Additional absolute paths the agent may write. Applied on top of `filesystem_root`.                                                                                          |
| `protect_git`           | `bool`                  | `true`                          | `XZATOMA_SANDBOX_PROTECT_GIT`                             | When `true`, prevents the agent from writing to `.git` directories anywhere within the filesystem root.                                                                      |
| `network_enabled`       | `bool`                  | `false`                         | `XZATOMA_SANDBOX_NETWORK_ENABLED`                         | When `true`, the sandbox HTTP CONNECT proxy starts and all outbound network traffic from terminal commands is routed through it.                                             |
| `network_allowed_hosts` | `Vec<String>`           | `[]`                            | `XZATOMA_SANDBOX_NETWORK_ALLOWED_HOSTS` (comma-separated) | Hostnames the proxy and `fetch` tool are permitted to contact. Hosts absent from this list are blocked with a 407 response. Requires `network_enabled: true` to have effect. |
| `proxy_port`            | `u16`                   | `8877`                          | `XZATOMA_SANDBOX_PROXY_PORT`                              | Port the sandbox HTTP CONNECT proxy binds to. Must be >= 1024.                                                                                                               |

## Valid `sandboxed_tools` Keys

| Key        | Controls                                                                            | Default when absent |
| ---------- | ----------------------------------------------------------------------------------- | ------------------- |
| `terminal` | `CommandValidator` network blocking and kernel sandbox policy for terminal commands | sandboxed (`true`)  |
| `fetch`    | `SsrfValidator` host allowlist and proxy routing for HTTP fetch requests            | sandboxed (`true`)  |
| `file_ops` | `SandboxGuard` path checks applied to all file operation tools                      | sandboxed (`true`)  |

Omitting a key from `sandboxed_tools` has the same effect as setting it to
`true`. To disable sandbox enforcement for a specific tool, set the key
explicitly to `false`:

```yaml
sandbox:
  sandboxed_tools:
    fetch: false
```

## Environment Variable Overrides

Environment variables take precedence over values in the configuration file.
They are applied after the config file is parsed.

| Variable                                | Type                      | Config Field                    |
| --------------------------------------- | ------------------------- | ------------------------------- |
| `XZATOMA_SANDBOX_ENABLED`               | `true` / `false`          | `sandbox.enabled`               |
| `XZATOMA_SANDBOX_FILESYSTEM_ROOT`       | absolute path string      | `sandbox.filesystem_root`       |
| `XZATOMA_SANDBOX_PROTECT_GIT`           | `true` / `false`          | `sandbox.protect_git`           |
| `XZATOMA_SANDBOX_NETWORK_ENABLED`       | `true` / `false`          | `sandbox.network_enabled`       |
| `XZATOMA_SANDBOX_NETWORK_ALLOWED_HOSTS` | comma-separated hostnames | `sandbox.network_allowed_hosts` |
| `XZATOMA_SANDBOX_PROXY_PORT`            | integer 1024-65535        | `sandbox.proxy_port`            |

## CLI Flags

| Flag                    | Equivalent Config                    | Description                                       |
| ----------------------- | ------------------------------------ | ------------------------------------------------- |
| `--no-sandbox`          | `sandbox.enabled = false`            | Disable all sandbox enforcement for a single run. |
| `--sandbox-root <path>` | `sandbox.filesystem_root = "<path>"` | Set the filesystem root for a single run.         |

## Validation Rules

The following rules are enforced by `Config::validate_sandbox_config` at
startup:

- `proxy_port` must be >= 1024.
- Each entry in `network_allowed_hosts` must be non-empty and must not contain a
  protocol prefix (for example, `https://`). Provide bare hostnames only, such
  as `api.example.com`.
- All entries in `extra_read_paths` and `extra_write_paths` must be non-empty
  strings.
- When `filesystem_root` is set, it must not be an empty string.
- Keys in `sandboxed_tools` must be one of `terminal`, `fetch`, or `file_ops`.
- Setting `network_allowed_hosts` to a non-empty list while `network_enabled` is
  `false` produces a non-fatal warning; the allowlist has no effect without the
  proxy.

## Platform Requirements

Sandbox enforcement operates at two independent layers:

- Application-layer guards (`SandboxGuard`, `CommandValidator`,
  `SsrfValidator`): implemented in Rust and active on all platforms.
- Kernel-layer enforcement: platform-specific and subject to the requirements
  listed below.

| Platform | Mechanism                 | Requirement                  |
| -------- | ------------------------- | ---------------------------- |
| Linux    | Landlock (ABI v1)         | kernel 5.13 or newer         |
| macOS    | Seatbelt (`sandbox_init`) | all supported macOS versions |
| Windows  | not supported             | intentional build failure    |

When the kernel version check fails on Linux, `apply_sandbox` returns a
`SandboxError::KernelVersionTooOld` error. Application-layer guards remain
active; only kernel-level enforcement is unavailable.

## Example Configuration

```yaml
sandbox:
  enabled: true
  sandboxed_tools:
    fetch: false
  filesystem_root: "/home/user/project"
  extra_read_paths:
    - "/usr/share/doc"
  extra_write_paths:
    - "/tmp/agent-output"
  protect_git: true
  network_enabled: true
  network_allowed_hosts:
    - "api.openai.com"
    - "api.github.com"
  proxy_port: 8877
```

## Related Documentation

- How-to guide for common sandboxing tasks: `../how-to/configure_sandboxing.md`
- Tutorial for first-time sandbox setup: `../tutorials/sandboxing_quickstart.md`
- Full configuration reference (all sections): `../reference/configuration.md`
- Sandboxing implementation plan:
  `../explanation/sandboxing_implementation_plan.md`
