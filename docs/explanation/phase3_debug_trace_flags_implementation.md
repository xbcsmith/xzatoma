# Phase 3: Named `--debug` and `--trace` Flags

## Overview

Phase 3 adds explicit `--debug` and `--trace` CLI flags to `CommonArgs`,
propagates them through the config system via a new `LogConfig` struct, and
updates `init_tracing` to use a three-level precedence hierarchy. Backward
compatibility with `--verbose` is preserved.

## Changes

### `src/cli.rs` — `CommonArgs`

Two new fields added after `verbose`:

| Field         | Flag      | Env var         | Description                                            |
| ------------- | --------- | --------------- | ------------------------------------------------------ |
| `debug: bool` | `--debug` | `XZATOMA_DEBUG` | Enable DEBUG level. Takes precedence over `--verbose`. |
| `trace: bool` | `--trace` | `XZATOMA_TRACE` | Enable TRACE level. Implies debug.                     |

Precedence rule (documented in help text): `--trace` > `--debug` > `--verbose` >
`RUST_LOG` env > `xzatoma=info` default.

### `src/config.rs` — `LogConfig`

New public struct that carries the same two flags for library consumers and
config-file users:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogConfig {
    pub debug: bool,  // Env: XZATOMA_DEBUG
    pub trace: bool,  // Env: XZATOMA_TRACE
}
```

`Config` gains a `#[serde(default)] pub log: LogConfig` field.
`apply_env_vars()` wires `XZATOMA_DEBUG` and `XZATOMA_TRACE` using the existing
`parse_env_bool()` helper. `apply_cli_overrides()` logs when debug/trace mode is
active.

The module-level doc comment now includes a logging env var table.

### `src/main.rs` — `log_level_str` and `init_tracing`

`log_level_str(debug: bool, trace: bool) -> &'static str` replaces the
single-argument form from Phase 2. Logic:

```rust
if trace { "trace" } else if debug { "debug" } else { "xzatoma=info" }
```

`init_tracing` signature becomes
`init_tracing(debug, trace, json_format, log_file)`.

Call-site backward compat:

```rust
let debug = common.debug || common.verbose;
let trace = common.trace;
init_tracing(debug, trace, log_json, watch_log_file.as_deref());
```

## Tests Added

### `src/cli.rs`

| Test                               | Asserts                                                |
| ---------------------------------- | ------------------------------------------------------ |
| `test_debug_flag_after_subcommand` | `--debug` parses to `common_args().debug == true`      |
| `test_trace_flag_after_subcommand` | `--trace` parses to `common_args().trace == true`      |
| `test_debug_and_trace_independent` | `--debug` alone leaves `trace == false`                |
| `test_xzatoma_debug_env_sets_flag` | `XZATOMA_DEBUG=true` sets `debug == true` via clap env |

### `src/config.rs`

| Test                                        | Asserts                                      |
| ------------------------------------------- | -------------------------------------------- |
| `test_log_config_debug_field_default_false` | `LogConfig::default().debug == false`        |
| `test_log_config_trace_field_default_false` | `LogConfig::default().trace == false`        |
| `test_apply_env_vars_overrides_log_debug`   | `XZATOMA_DEBUG=true` sets `config.log.debug` |
| `test_apply_env_vars_overrides_log_trace`   | `XZATOMA_TRACE=true` sets `config.log.trace` |

### `src/main.rs`

Updated `test_init_tracing_verbose_false_defaults_to_info` and
`test_init_tracing_verbose_true_uses_debug` to pass `(bool, bool)`. Added
`test_init_tracing_debug_flag_uses_debug`,
`test_init_tracing_trace_flag_uses_trace`, and
`test_init_tracing_trace_overrides_debug`.

## Success Criteria

- `xzatoma run --debug --prompt "hello"` produces DEBUG and above.
- `xzatoma chat --trace` produces TRACE lines.
- `xzatoma run --verbose --prompt "hello"` continues to activate DEBUG.
- `XZATOMA_DEBUG=true xzatoma chat` enables debug without the CLI flag.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All existing and new tests pass.
