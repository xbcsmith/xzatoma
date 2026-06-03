# Logging Reference

XZatoma uses the [`tracing`](https://docs.rs/tracing) crate for structured
logging. This document describes every knob available for controlling what the
process logs, at what level, and where it goes.

## Log Levels

| Level | Flag        | Env var                 | Description                                                                              |
| ----- | ----------- | ----------------------- | ---------------------------------------------------------------------------------------- |
| INFO  | _(default)_ | `RUST_LOG=xzatoma=info` | Normal operational messages.                                                             |
| DEBUG | `--debug`   | `XZATOMA_DEBUG=true`    | Provider round-trips, iteration counts, tool execution.                                  |
| TRACE | `--trace`   | `XZATOMA_TRACE=true`    | Full conversation transcript per message, tool arguments and results, provider metadata. |

Precedence (highest wins): `--trace` > `--debug` > `--verbose` > `RUST_LOG`
env > INFO default.

`--verbose` is a deprecated alias for `--debug`. Use `--debug` in all new
scripts and documentation.

## Flag Placement

All logging flags are part of `CommonArgs` and must appear **after** the
subcommand token:

```bash
# Correct
xzatoma run --debug --prompt "hello"
xzatoma chat --trace
xzatoma watch --debug

# Wrong (flags before subcommand are rejected)
xzatoma --debug run --prompt "hello"
```

## Output Formats

The `--log-format` flag controls how log lines are emitted to stderr. Three
values are accepted:

| Value     | Description                                                  |
| --------- | ------------------------------------------------------------ |
| `plain`   | Human-readable multi-line format with ANSI colour (default). |
| `compact` | Single-line text without ANSI colour.                        |
| `json`    | Newline-delimited JSON (NDJSON).                             |

```bash
xzatoma run --log-format json --prompt "hello"
xzatoma run --log-format compact --prompt "hello"
```

The format can also be set in `config.yaml`:

```yaml
log:
  stderr_format: plain # plain | compact | json
```

## File Sink

Use `--logfile` to write a second log stream to a file. The file is created or
appended to. File output defaults to JSON format regardless of `--log-format`.

```bash
# Plain text to stderr, JSON to file
xzatoma run --logfile /var/log/xzatoma.log --prompt "hello"

# JSON to both stderr and file
xzatoma run --log-format json --logfile /var/log/xzatoma.log --prompt "hello"
```

Configure the file sink in `config.yaml`:

```yaml
log:
  file_format: json # format for the file sink
  file_path: /var/log/xzatoma.log
```

`--logfile` on the command line overrides `log.file_path` from the config file.
The file sink is entirely separate from the `Watch` command's `--log-file` flag,
which controls per-event watcher output.

## Environment Variables

All logging knobs are available as environment variables so they can be set in
process supervisors, container environments, and Zed extension env blocks.

| Variable                    | Type                   | Description                                                       |
| --------------------------- | ---------------------- | ----------------------------------------------------------------- |
| `XZATOMA_DEBUG`             | `true`/`false`         | Enable debug-level logging. Equivalent to `--debug`.              |
| `XZATOMA_TRACE`             | `true`/`false`         | Enable trace-level logging. Equivalent to `--trace`.              |
| `XZATOMA_LOG_FORMAT`        | `plain\|compact\|json` | Override stderr log format.                                       |
| `XZATOMA_LOG_STDERR_FORMAT` | `plain\|compact\|json` | Override stderr format (more specific than `XZATOMA_LOG_FORMAT`). |
| `XZATOMA_LOG_FILE_FORMAT`   | `plain\|compact\|json` | Override file sink format.                                        |
| `XZATOMA_LOG_FILE`          | path                   | Write an additional log stream to this file.                      |
| `RUST_LOG`                  | filter string          | Full tracing filter expression. Overrides all flags when set.     |

Boolean env vars accept: `1`, `true`, `yes`, `on` (enable) or `0`, `false`,
`no`, `off` (disable).

## RUST_LOG Override

`RUST_LOG` is a standard tracing env var that accepts the full
[`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
syntax. When set, it takes precedence over `--debug`, `--trace`, and
`XZATOMA_DEBUG`/`XZATOMA_TRACE`.

```bash
# Module-level targeting: only ACP subsystem at debug, everything else at warn
RUST_LOG=warn,xzatoma::acp=debug xzatoma acp serve

# Full trace for the agent module only
RUST_LOG=xzatoma::agent=trace xzatoma run --prompt "hello"

# Equivalent to --debug
RUST_LOG=xzatoma=debug xzatoma chat
```

`RUST_LOG` is the only way to set per-module log levels. The CLI flags
(`--debug`, `--trace`) apply a single level to the entire `xzatoma` target.

## Global Subscriber vs Watcher Logging

XZatoma has two independent logging systems:

**Global tracing subscriber** — controlled by `--debug`, `--trace`,
`--log-format`, `--logfile`, and the env vars above. This records all structured
events from every module: agent turns, tool calls, provider requests, and so on.

**Watcher logging config** — controlled by the `watcher.logging` block in
`config.yaml` and the `Watch` command's `--log-file` / `--json-logs` flags. This
writes a separate per-event file sink specifically for Kafka watcher activity.
It operates independently of the global subscriber and is unaffected by
`--log-format` or `--logfile`.

```yaml
watcher:
  logging:
    level: info
    json_format: true
    file_path: /var/log/xzatoma-watcher.log
```

Use the global subscriber flags for general debugging. Use `watcher.logging`
when you need a dedicated structured event log for the Kafka watch loop.

## TRACE-Level Transcript

When `--trace` is active, the agent core emits one structured event per
conversation message sent to the provider:

```text
TRACE xzatoma::agent::core msg_index=0 msg_role=system msg_char_count=412 msg_content="You are..."
TRACE xzatoma::agent::core msg_index=1 msg_role=user msg_char_count=18 msg_content="hello"
```

Tool calls are also recorded:

```text
TRACE xzatoma::agent::core tool_name=read_file tool_call_id=call_abc tool_args_json={"path":"src/main.rs"}
TRACE xzatoma::agent::core tool_name=read_file tool_call_id=call_abc tool_result_bytes=1234 tool_result_preview="//! XZatoma..."
```

Provider metadata is emitted once per execution entry point:

```text
TRACE xzatoma::agent::core provider_model=gpt-4o provider_type="dyn xzatoma::providers::Provider"
```

Combine `--trace` with `--logfile` and `--log-format json` to capture the full
transcript as NDJSON for post-hoc analysis:

```bash
xzatoma run --trace --log-format json --logfile /tmp/trace.ndjson --prompt "hello"
```
