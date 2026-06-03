# Phase 2: Fix Initialization Order and Wire `--verbose`

## Overview

Phase 2 completes the `--verbose` wiring begun in Phase 1 by extracting a
testable level-string helper from `init_tracing`, promoting `init_tracing` to
`pub(crate)`, and adding unit tests that verify the correct log level is
selected without initialising a real subscriber.

## Changes

### `src/main.rs`

#### `log_level_str(verbose: bool) -> &'static str`

A new `pub(crate)` helper that returns the log-level directive string for the
given verbosity flag:

- `verbose = false` returns `"xzatoma=info"` (only this crate at INFO, all other
  crates suppressed).
- `verbose = true` returns `"debug"` (all crates at DEBUG).

Extracting this logic as a pure function means the level-selection behavior can
be unit-tested deterministically without initialising a `tracing` subscriber,
which may only be done once per process.

#### `init_tracing` promoted to `pub(crate)`

`init_tracing` is now `pub(crate)` so test modules in the same binary crate can
reference it by name (even if they cannot safely call it more than once). Its
body now delegates to `log_level_str` instead of inlining the conditional.

#### `RUST_LOG` override still applies

`EnvFilter::try_from_default_env()` is attempted before falling back to the
derived level string, so `RUST_LOG=trace xzatoma run ...` still overrides to
TRACE as expected.

#### Initialization order

The call order established in Phase 1 is preserved:

1. `Cli::parse_args()` - pure synchronous parse, no I/O.
2. `cli.command.common_args()` - extract shared flags.
3. `init_tracing(common.verbose, ...)` - subscriber registered with correct
   level before any I/O or logging occurs.
4. Mirror `common.storage_path` into `XZATOMA_HISTORY_DB` if set.
5. `Config::load(config_path, &common)` - file I/O happens after subscriber is
   live so config warnings are captured.
6. `config.validate()`.
7. Command dispatch.

## Tests Added

Both tests live in the `#[cfg(test)] mod tests` block at the bottom of
`src/main.rs`:

| Test                                               | Asserts                                  |
| -------------------------------------------------- | ---------------------------------------- |
| `test_init_tracing_verbose_false_defaults_to_info` | `log_level_str(false) == "xzatoma=info"` |
| `test_init_tracing_verbose_true_uses_debug`        | `log_level_str(true) == "debug"`         |

## Success Criteria

- `xzatoma run --verbose --prompt "hello"` produces `DEBUG` log lines.
- `xzatoma run --prompt "hello"` produces only `INFO` lines.
- `RUST_LOG=trace xzatoma run --prompt "hello"` overrides to `TRACE`.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All tests pass.
