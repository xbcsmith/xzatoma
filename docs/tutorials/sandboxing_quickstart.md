# Sandboxing Quickstart

## Introduction

XZatoma includes a sandbox layer that limits what the agent can access during a
session. The sandbox prevents the agent from reading or writing files outside a
designated project directory, blocks outbound network requests to unlisted
hosts, and applies kernel-level enforcement on supported platforms.

The sandbox operates at two layers:

- Application-layer guards: path checks (`SandboxGuard`), command validation
  (`CommandValidator`), and HTTP request filtering (`SsrfValidator`). These run
  in-process and are active on all platforms.
- Kernel-layer enforcement: Landlock on Linux (kernel 5.13+) and Seatbelt on
  macOS. These enforce policy at the OS level.

The sandbox is on by default. This tutorial walks through what that means in
practice and how to change the defaults for common scenarios. You can complete
it in about 15 minutes.

## Prerequisites

- XZatoma installed and available on your PATH. Confirm with:

  ```bash
  xzatoma --version
  ```

- A project directory to work in. This tutorial uses `/home/user/project` as the
  example path.
- A config file at `config/config.yaml`. If you do not have one yet, create it
  with a minimal provider entry:

  ```yaml
  provider:
    type: ollama
  ```

## Step 1: Check the Default Configuration

Sandbox enforcement is active immediately without any configuration changes. Run
the following command to see the effective configuration:

```bash
xzatoma config show
```

The sandbox section of the output will show the defaults:

```text
sandbox:
  enabled: true
  filesystem_root: null       # uses the current working directory
  protect_git: true
  network_enabled: false
  network_allowed_hosts: []
  proxy_port: 8877
  extra_read_paths: []
  extra_write_paths: []
  sandboxed_tools: {}         # all tools sandboxed
```

Key defaults to note:

- `enabled: true` -- sandbox enforcement is active
- `filesystem_root: null` -- the current working directory is the filesystem
  root
- `network_enabled: false` -- outbound network is blocked for terminal commands
- `protect_git: true` -- writes to `.git` directories are denied

## Step 2: Observe the Sandbox in Action

Start a chat session from inside your project directory:

```bash
cd /home/user/project
xzatoma chat
```

### Write inside the project (succeeds)

Ask the agent to write a file inside the project:

```text
> Write "hello" to notes.txt
```

The agent creates `notes.txt` in the current directory. This succeeds because
the path is within the filesystem root.

### Write outside the project (fails)

Ask the agent to write outside the project:

```text
> Write "hello" to /tmp/escape.txt
```

The agent reports a sandbox denial:

```text
Sandbox denied access: path "/tmp/escape.txt" is outside the allowed root
```

The file is not created. The agent returns control to you after reporting the
error; it does not terminate the session.

## Step 3: Configure an Extra Write Path

To allow the agent to write to a directory outside the project root (for
example, to deliver output artifacts to a shared location), add it to
`extra_write_paths`.

Edit `config/config.yaml`:

```yaml
sandbox:
  enabled: true
  extra_write_paths:
    - "/tmp/agent-output"
```

Restart the session:

```bash
xzatoma chat
```

Ask the agent to write to the extra path:

```text
> Write "hello" to /tmp/agent-output/result.txt
```

This succeeds. The original filesystem root is still enforced; only the
explicitly listed extra paths are added to the writable set.

To allow reading (but not writing) from an additional path, use
`extra_read_paths` instead.

## Step 4: Enable Network Access

By default, terminal commands spawned by the agent cannot make outbound network
requests. To allow requests to specific hosts, set `network_enabled: true` and
list the permitted hostnames.

Edit `config/config.yaml`:

```yaml
sandbox:
  enabled: true
  network_enabled: true
  network_allowed_hosts:
    - "api.example.com"
```

Restart the session. The sandbox proxy starts automatically and you will see the
following in the output:

```text
sandbox proxy started on 127.0.0.1:8877
```

### Allowed host (succeeds)

Ask the agent to run a request to the permitted host:

```text
> Run: curl https://api.example.com/ping
```

The request is routed through the proxy and reaches the target.

### Unlisted host (blocked)

Ask the agent to reach a host not in the list:

```text
> Run: curl https://evil.internal/data
```

The proxy rejects the connection:

```text
curl: (56) Received HTTP code 407 from proxy after CONNECT
```

The agent cannot contact `evil.internal` because it is absent from
`network_allowed_hosts`.

## Summary

| Task                                      | Config or command                                   |
| ----------------------------------------- | --------------------------------------------------- |
| Check default sandbox config              | `xzatoma config show`                               |
| Observe write denial outside root         | default behavior -- no config needed                |
| Add a writable path outside the root      | `extra_write_paths`                                 |
| Enable outbound network to specific hosts | `network_enabled: true` and `network_allowed_hosts` |
| Block requests to unlisted hosts          | default proxy behavior -- no extra config needed    |

The sandbox is on by default and requires no configuration to protect the host
filesystem from accidental writes outside the working directory. Network access
is off by default and must be explicitly enabled with `network_enabled: true`.

## Next Steps

- Reference: full field descriptions for all sandbox options:
  `../reference/sandbox_configuration.md`
- How-to: specific sandboxing tasks: `../how-to/configure_sandboxing.md`
- Full configuration reference (all sections): `../reference/configuration.md`
