# Configure Sandboxing

## Overview

This guide covers common sandboxing configuration tasks. Each task is
self-contained and shows the minimum YAML and CLI changes needed to accomplish
it.

For a full description of every configuration field, see
`../reference/sandbox_configuration.md`.

## Task 1: Restrict the Agent to a Single Project Directory

Restrict the agent so it can only read and write files inside a specific
directory.

Using the config file:

```yaml
sandbox:
  enabled: true
  filesystem_root: "/home/user/project"
```

Using the CLI flag for a single run:

```bash
xzatoma chat --sandbox-root /home/user/project
```

The agent will refuse any file read or write that falls outside
`filesystem_root`. Paths in `extra_read_paths` and `extra_write_paths` extend
this root without replacing it. Git protection (`protect_git: true`) applies
within the root by default.

## Task 2: Permit Network Access to Specific Hosts

Allow the agent to make outbound network requests to a defined set of hostnames.

```yaml
sandbox:
  enabled: true
  network_enabled: true
  network_allowed_hosts:
    - "api.openai.com"
    - "api.github.com"
```

When `network_enabled` is `true`, the sandbox HTTP CONNECT proxy starts
automatically on the port specified by `proxy_port` (default: 8877). Terminal
commands executed by the agent have `HTTP_PROXY` and `HTTPS_PROXY` set to this
proxy address. Any connection attempt to a host not listed in
`network_allowed_hosts` is rejected with a 407 response.

The `fetch` tool applies the same allowlist in-process, without routing through
the proxy.

Hosts must be bare hostnames without a protocol prefix. The following entry is
invalid and will be rejected at startup:

```yaml
# invalid -- do not include the protocol prefix
network_allowed_hosts:
  - "https://api.openai.com"
```

The correct form is:

```yaml
network_allowed_hosts:
  - "api.openai.com"
```

## Task 3: Disable Sandboxing for a Specific Tool

To allow one tool to bypass sandbox enforcement while other tools remain
protected, use the `sandboxed_tools` map.

```yaml
sandbox:
  enabled: true
  sandboxed_tools:
    fetch: false
```

Valid keys are `terminal`, `fetch`, and `file_ops`. Setting a key to `false`
removes sandbox enforcement for that tool only. Omitting a key has the same
effect as setting it to `true`.

| Key        | What it controls                                      |
| ---------- | ----------------------------------------------------- |
| `terminal` | Network blocking and kernel policy for shell commands |
| `fetch`    | Host allowlist and proxy routing for HTTP requests    |
| `file_ops` | Path checks for all file read and write operations    |

## Task 4: Disable Sandboxing for a Single Run

To disable all sandbox enforcement without editing the config file, use the
environment variable or the CLI flag.

Using the environment variable:

```bash
XZATOMA_SANDBOX_ENABLED=false xzatoma chat
```

Using the CLI flag:

```bash
xzatoma chat --no-sandbox
```

Both options disable sandbox enforcement for that invocation only. The config
file is not modified.

## Task 5: Verify That the Sandbox is Active

Use the following checks to confirm that sandbox enforcement is running.

### Check the logs for proxy startup

When `network_enabled: true`, the sandbox proxy logs a message at the `INFO`
level when it starts:

```text
sandbox proxy started on 127.0.0.1:8877
```

If this line does not appear and `network_enabled: true` is set, check that
`proxy_port` is not already in use by another process.

### Enable debug logging to see policy application

```bash
RUST_LOG=debug xzatoma chat
```

With debug logging enabled, the sandbox engine logs each path checked against
the policy and each command validated by `CommandValidator`.

### Confirm that writes outside the root are denied

Attempt to write a file outside the configured `filesystem_root`. The operation
will return an error similar to:

```text
Sandbox denied access: path "/tmp/escape" is outside the allowed root
```

If the write succeeds instead, verify that `sandbox.enabled` is `true` in the
effective config (`xzatoma config show`) and that `file_ops` is not set to
`false` in `sandboxed_tools`.

## Related Documentation

- Reference: full field descriptions: `../reference/sandbox_configuration.md`
- Tutorial: sandboxing quickstart: `../tutorials/sandboxing_quickstart.md`
