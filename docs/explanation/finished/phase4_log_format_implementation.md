# Phase 4: Configurable Log Format and Global File Sink

## Overview

Phase 4 adds per-sink format control and a global `--logfile` flag to the CLI.
Every command now accepts `--log-format plain|compact|json` and
`--logfile <path>` through `CommonArgs`. The existing `Watch --log-file` flag is
unchanged and continues to control per-event watcher output independently.

## Changes

### `src/config.rs`

A new `LogFormat` enum is added:

```rust
pub enum LogFormat {
    Plain,    // human-readable multi-line (default for stderr)
    Compact,  // single-line without ANSI
    Json,     // newline-delimited JSON (default for file sinks)
}
```

`LogConfig` is extended with three new fields:

| Field           | Type              | Default | Description                                |
| --------------- | ----------------- | ------- | ------------------------------------------ |
| `stderr_format` | `LogFormat`       | `Plain` | Output format for stderr                   |
| `file_format`   | `LogFormat`       | `Json`  | Output format for the file sink            |
| `file_path`     | `Option<PathBuf>` | `None`  | Optional path to write a second log stream |

New environment variables wired in `apply_env_vars()`:

| Variable                    | Description                              |
| --------------------------- | ---------------------------------------- |
| `XZATOMA_LOG_FORMAT`        | Override `stderr_format`                 |
| `XZATOMA_LOG_STDERR_FORMAT` | Override `stderr_format` (more specific) |
| `XZATOMA_LOG_FILE_FORMAT`   | Override `file_format`                   |
| `XZATOMA_LOG_FILE`          | Override `file_path`                     |

`apply_cli_overrides()` stores `--log-format` and `--logfile` values into
`config.log` so library consumers see the effective configuration.

### `src/cli.rs`

Two new fields are added to `CommonArgs`:

- `log_format: Option<LogFormat>` — `--log-format plain|compact|json`, env
  `XZATOMA_LOG_FORMAT`. Uses clap `value_enum` so invalid values are rejected at
  parse time.
- `log_file: Option<PathBuf>` — `--logfile <path>`, env `XZATOMA_LOG_FILE`. Uses
  argument ID `global-logfile` to avoid collision with `Watch`'s `--log-file`
  flag.

### `src/main.rs`

`init_tracing()` signature updated to:

```rust
fn init_tracing(
    debug: bool,
    trace: bool,
    stderr_format: LogFormat,
    file_format: LogFormat,
    log_file: Option<&Path>,
)
```

The stderr and file layers are now built with a `match` on the format enum, each
arm producing a `Box<dyn Layer>` via `.boxed()`. This removes the duplicated
if/else registry construction from Phase 3.

The Watch-specific log-format extraction that previously existed in `main()` is
removed. Watch's `--log-file` and `--json-logs` still propagate to `run_watch()`
via `WatchCliOverrides` for watcher-level output.

### `config/config.yaml`

A documented `log:` section is added in commented-out form after the `skills:`
block. Operators can uncomment and configure `stderr_format`, `file_format`, and
`file_path`.

## Usage Examples

```bash
# Emit NDJSON to stderr
xzatoma run --log-format json --prompt "hello"

# Write JSON to a file, plain text to stderr
xzatoma run --logfile /var/log/xzatoma.log --prompt "hello"

# Combined: compact stderr + JSON file
xzatoma chat --log-format compact --logfile /tmp/xzatoma.log
```

## Design Decisions

- **Argument ID disambiguation**: `CommonArgs.log_file` uses clap argument ID
  `global-logfile` to prevent collision with `Watch.log_file` (ID `log_file`)
  when `CommonArgs` is flattened into `Watch`.
- **File format defaults to JSON**: Machine-readable format is appropriate for
  file sinks consumed by log aggregators.
- **`clap::ValueEnum` on `LogFormat`**: Derives from `config.rs` so the enum
  does not need to be duplicated. Clap rejects unknown values at parse time with
  a friendly error.
- **No subscriber reload**: The config-file value in `config.log.stderr_format`
  is available after `Config::load()` but the subscriber is initialized before
  that. The CLI flag therefore takes precedence for the full process lifetime.
