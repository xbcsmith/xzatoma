# CLI Flag Placement and Logging Refinement Implementation Plan

## Overview

XZatoma has two categories of problems to solve. The first is architectural:
`--config`, `--verbose`, and `--storage-path` are declared `global = true` on
the top-level `Cli` struct, which permits but does not require the pattern
`xzatoma --flag subcommand`. The desired UX is the strict
`xzatoma subcommand --flag` form. Achieving this requires replacing the three
`global = true` fields on `Cli` with a `CommonArgs` struct flattened into every
`Commands` variant, and updating all call sites and tests accordingly.

The second category is four compounding logging problems: `init_tracing()` runs
before `Cli::parse_args()` so `--verbose` has never had any effect; there are no
named `--debug` or `--trace` flags; the `json` feature of `tracing-subscriber`
is compiled in but never wired to a flag or config field; and tool call
arguments are not emitted as structured `trace!` fields in `agent/core.rs`.

Phase 1 (the CLI architecture refactor) is the prerequisite for all logging
phases. Phases 2 through 5 address logging. Phase 6 covers documentation.

## Current State Analysis

### Existing Infrastructure

- `Cli` struct in [`src/cli.rs`](../../src/cli.rs) (L22-38): three fields with
  `global = true` (`config`, `verbose`, `storage_path`) plus
  `command: Commands`. The global flag mechanism lets clap accept flags in any
  position, but the target pattern requires flags after the subcommand only.
- `Commands` enum in `src/cli.rs` (L42-251): eleven top-level variants (`Chat`,
  `Run`, `Agent`, `Watch`, `Auth`, `Models`, `History`, `Replay`, `Mcp`, `Acp`,
  `Skills`). None carry shared flags today.
- `impl Default for Cli` in `src/cli.rs` (L458-469): constructs `Cli` with
  top-level `config`, `verbose`, and `storage_path` fields directly. Must be
  updated when those fields move into `CommonArgs`.
- `Config::load(path: &str, cli: &crate::cli::Cli)` in
  [`src/config.rs`](../../src/config.rs) (L1377): calls
  `apply_cli_overrides(cli)` passing the full `Cli` reference.
- `apply_cli_overrides(&mut self, cli: &crate::cli::Cli)` in `src/config.rs`
  (L2289-2293): reads `cli.verbose`. Both the signature and body must change
  once `verbose` moves to `CommonArgs`.
- `init_tracing()` in [`src/main.rs`](../../src/main.rs) (L266-273): accepts no
  parameters; called before `Cli::parse_args()`. `--verbose` silently has no
  effect today.
- `tracing-subscriber` in [`Cargo.toml`](../../Cargo.toml) (L36): `env-filter`,
  `fmt`, and `json` features already enabled. No new dependencies are required
  for any phase.
- `AgentObserver` events in [`src/agent/core.rs`](../../src/agent/core.rs):
  `ToolCallStarted` and `ToolCallCompleted` emit structured data to observers
  but `execute_tool_call()` (L1175-1212) does not emit corresponding `trace!`
  macro calls with structured fields.
- `Provider` trait in
  [`src/providers/trait_mod.rs`](../../src/providers/trait_mod.rs): exposes the
  synchronous `get_current_model() -> Option<String>` on every provider, usable
  for trace-level metadata logging without async API calls.
- Mod tests in `src/cli.rs` (L472-1653): approximately 100 tests. Two tests
  already use the post-subcommand pattern
  (`test_cli_parse_watch_with_config_- after_subcommand` at L1184 and
  `test_cli_parse_with_verbose_after_subcommand` at L1193), but both assert on
  `cli.config` and `cli.verbose` directly, which will not compile after the
  fields move.

### Identified Issues

1. **`global = true` permits but does not enforce post-subcommand placement.**
   `xzatoma --config x.yaml watch` and `xzatoma watch --config x.yaml` are both
   accepted today. The target is that only the second form is valid.
2. **Broken verbose flag.** `init_tracing()` runs before `Cli::parse_args()`, so
   `cli.verbose` is never read. Passing `--verbose` has zero effect.
3. **Inverted initialization order.** `init_tracing()` must precede
   `Config::load()` to capture load-time events, but it can follow
   `Cli::parse_args()` since that is a pure synchronous parse with no I/O.
4. **No named log-level flags.** Operators must use `RUST_LOG=debug` or
   `RUST_LOG=trace`; there are no `--debug` or `--trace` flags.
5. **No log format control.** A single stderr pretty formatter is always used.
   The `json` feature is compiled in but never routed to any flag or config
   field, and there is no global `--logfile` option.
6. **Incomplete trace transcript.** `execute_tool_call()` (L1177) emits
   `debug!("Executing tool: {}", tool_name)` but tool arguments and result
   content are not emitted as structured `trace!` fields.

## Implementation Phases

### Phase 1: CLI Architecture - `CommonArgs` and Per-Subcommand Flags

Replace the three `global = true` fields on `Cli` with a `CommonArgs` struct
flattened into every `Commands` variant. Add a `Commands::common_args()` helper
for extracting shared state in `main()`. Update `Config::load()`,
`apply_cli_overrides()`, `Default for Cli`, and all ~100 CLI tests. This phase
is the prerequisite for all subsequent phases.

#### Task 1.1 Define `CommonArgs` in `src/cli.rs`

Add a new `#[derive(Args, Debug, Clone)]` struct `CommonArgs` immediately after
the imports, carrying the three fields removed from `Cli`:

- `config: Option<String>`: short `-c`, long `--config`,
  `default_value = "config/config.yaml"`, env `XZATOMA_CONFIG`.
- `verbose: bool`: short `-v`, long `--verbose`. Doc comment: "Deprecated: use
  `--debug` or `--trace` instead." (The deprecation is noted now so the field is
  not re-documented when it is formally deprecated in Phase 3.)
- `storage_path: Option<String>`: long `--storage-path`, env
  `XZATOMA_HISTORY_DB`.

The logging flags added in Phases 3 and 4 (`debug`, `trace`, `log_format`,
`log_file`) are not in scope for Phase 1 so that each phase remains
independently shippable.

Add `impl Default for CommonArgs` with `config = Some("config/config.yaml")`,
`verbose = false`, and `storage_path = None`.

#### Task 1.2 Flatten `CommonArgs` into Every `Commands` Variant

For each of the eleven variants in `Commands` (L42-251), add:

```rust
#[command(flatten)]
common: CommonArgs,
```

as the first named field. Variants that currently hold only a nested
`#[command(subcommand)]` field (`Models`, `History`, `Mcp`, `Acp`, `Skills`)
accept both `#[command(flatten)]` and `#[command(subcommand)]` in the same
variant under clap 4. The common flags (`--config`, `--verbose`,
`--storage-path`) appear between the outer subcommand token and the inner
subcommand token:

```text
xzatoma models --config x.yaml list --json
xzatoma history --verbose show --id abc
```

#### Task 1.3 Simplify the `Cli` Struct

Remove `config`, `verbose`, and `storage_path` from `Cli`. The struct becomes:

```rust
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
```

Remove the corresponding module-level doc comment lines that describe these as
global options. Update `impl Cli` (L447-456): `parse_args()` is unchanged.

Update `impl Default for Cli` (L458-469) to construct `CommonArgs::default()`
inside the `Auth` variant rather than populating top-level fields:

```rust
impl Default for Cli {
    fn default() -> Self {
        Self {
            command: Commands::Auth {
                common: CommonArgs::default(),
                provider: Some("copilot".to_string()),
            },
        }
    }
}
```

#### Task 1.4 Add `Commands::common_args()` Helper

Add a method on `Commands` returning `&CommonArgs`:

```rust
impl Commands {
    pub fn common_args(&self) -> &CommonArgs {
        match self {
            Commands::Chat    { common, .. } => common,
            Commands::Run     { common, .. } => common,
            Commands::Agent   { common, .. } => common,
            Commands::Watch   { common, .. } => common,
            Commands::Auth    { common, .. } => common,
            Commands::Models  { common, .. } => common,
            Commands::History { common, .. } => common,
            Commands::Replay  { common, .. } => common,
            Commands::Mcp     { common, .. } => common,
            Commands::Acp     { common, .. } => common,
            Commands::Skills  { common, .. } => common,
        }
    }
}
```

This is the sole extraction point used by `main()` and any call site that needs
shared flags before dispatching to a command handler.

#### Task 1.5 Update `src/main.rs`

Extract common args immediately after parsing, before any other use:

```rust
let cli = Cli::parse_args();
let common = cli.command.common_args();
```

Replace every reference to `cli.config`, `cli.verbose`, and `cli.storage_path`
with `common.config`, `common.verbose`, and `common.storage_path`.

The config-path derivation becomes:

```rust
let config_path = common.config.as_deref().unwrap_or("config/config.yaml");
let config = Config::load(config_path, common)?;
```

#### Task 1.6 Update `Config::load()` and `apply_cli_overrides()` in `src/config.rs`

Change both signatures to accept `&crate::cli::CommonArgs` instead of
`&crate::cli::Cli`:

- `pub fn load(path: &str, common: &crate::cli::CommonArgs) -> Result<Self>`
- `fn apply_cli_overrides(&mut self, common: &crate::cli::CommonArgs)`

Inside `apply_cli_overrides()`, update the field reference from `cli.verbose` to
`common.verbose`. Update the single `apply_cli_overrides` call site inside
`load()` accordingly.

#### Task 1.7 Update All Tests in `src/cli.rs`

All ~100 tests in `mod tests` (L472-1653) must move shared flags from before the
subcommand token to after it, and update assertions from `cli.config`,
`cli.verbose`, and `cli.storage_path` to access via `cli.command.common_args()`.

The two tests that already name the post-subcommand pattern need assertion fixes
only:

- `test_cli_parse_watch_with_config_after_subcommand` (L1184): keep the argument
  order, change `assert_eq!(cli.config, ...)` to
  `assert_eq!(cli.command.common_args().config, ...)`.
- `test_cli_parse_with_verbose_after_subcommand` (L1193): keep argument order,
  change `assert!(cli.verbose)` to `assert!(cli.command.common_args().verbose)`.

All other tests that pass a shared flag before the subcommand token must move
that token after the subcommand. Representative examples:

| Before                                                 | After                                                  |
| ------------------------------------------------------ | ------------------------------------------------------ |
| `["xzatoma", "--verbose", "auth", ...]`                | `["xzatoma", "auth", "--verbose", ...]`                |
| `["xzatoma", "--config", "x.yaml", "watch", ...]`      | `["xzatoma", "watch", "--config", "x.yaml", ...]`      |
| `["xzatoma", "--storage-path", "/db", "history", ...]` | `["xzatoma", "history", "--storage-path", "/db", ...]` |

Assertions on `cli.verbose` and `cli.config` change to
`cli.command.common_args().verbose` and `cli.command.common_args().config`.

#### Task 1.8 Testing Requirements

- All ~100 existing tests must pass with the updated argument order and
  assertion paths.
- Add `test_common_args_config_default`: `["xzatoma", "chat"]` yields
  `common_args().config == Some("config/config.yaml".to_string())`.
- Add `test_common_args_storage_path_env`: set env `XZATOMA_HISTORY_DB=/tmp/x`,
  parse `["xzatoma", "chat"]`, assert
  `common_args().storage_path == Some("/tmp/x".to_string())`.
- Add `test_flag_before_subcommand_is_rejected`: verify
  `["xzatoma", "--config", "x.yaml", "chat"]` fails to parse, confirming the old
  pattern is no longer accepted.
- Add `test_nested_subcommand_with_common_flags`: verify
  `["xzatoma", "models", "--config", "x.yaml", "list"]` parses correctly and
  `common_args().config == Some("x.yaml".to_string())`.

#### Task 1.9 Deliverables

- [ ] `src/cli.rs`: `CommonArgs` struct defined with `Default`; all three
      `global = true` fields removed from `Cli`; `CommonArgs` flattened into all
      eleven `Commands` variants; `Commands::common_args()` method added;
      `Default for Cli` updated.
- [ ] `src/config.rs`: `Config::load()` and `apply_cli_overrides()` accept
      `&CommonArgs`.
- [ ] `src/main.rs`: all top-level field accesses replaced with
      `cli.command.common_args()`.
- [ ] All ~100 existing CLI tests updated; four new tests added.

#### Task 1.10 Success Criteria

- `xzatoma watch --config config/config.yaml` parses and the config path is read
  correctly.
- `xzatoma --config config/config.yaml watch` fails to parse.
- `xzatoma models --verbose list` parses and `common_args().verbose == true`.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All tests pass.

---

### Phase 2: Fix Initialization Order and Wire `--verbose`

With `CommonArgs` in place, the order fix and verbose wiring become a small,
focused change to `main()` and the `init_tracing()` signature. No new flags and
no config changes.

#### Task 2.1 Reorder Initialization in `src/main.rs`

Change the execution sequence so that `Cli::parse_args()` and common arg
extraction precede `init_tracing()`:

1. `let cli = Cli::parse_args();`
2. `let common = cli.command.common_args();`
3. `init_tracing(common.verbose);`
4. Mirror `common.storage_path` into `XZATOMA_HISTORY_DB` if set.
5. `Config::load(config_path, common)?;`
6. `config.validate()?;`
7. Dispatch to the matched command.

`Cli::parse_args()` is a pure synchronous parse with no I/O, so moving it before
the subscriber initialisation has no side effects.

#### Task 2.2 Update `init_tracing()` in `src/main.rs`

Change the signature from `fn init_tracing()` to
`fn init_tracing(verbose: bool)`. Derive the filter level:

```rust
let level = if verbose { "debug" } else { "xzatoma=info" };
let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(level));
```

`RUST_LOG` still overrides when set explicitly, preserving existing operator
workflows.

#### Task 2.3 Testing Requirements

- `test_cli_parse_with_verbose_after_subcommand` updated in Phase 1 now also
  confirms that `common_args().verbose` is accessible before `init_tracing()`.
- Promote `init_tracing()` to `pub(crate)` and add:
  - `test_init_tracing_verbose_false_defaults_to_info`
  - `test_init_tracing_verbose_true_uses_debug`

#### Task 2.4 Deliverables

- [ ] `src/main.rs`: `init_tracing(verbose: bool)` replaces `init_tracing()`;
      call order corrected.
- [ ] `--verbose` after any subcommand now activates `DEBUG` level output.

#### Task 2.5 Success Criteria

- `xzatoma run --verbose --prompt "hello"` produces `DEBUG` log lines.
- `xzatoma run --prompt "hello"` produces only `INFO` lines.
- `RUST_LOG=trace xzatoma run --prompt "hello"` overrides to `TRACE`.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All tests pass.

---

### Phase 3: Named `--debug` and `--trace` Flags

Adds explicit named log-level flags to `CommonArgs` and propagates them through
the config system so they can also be set via environment variable or config
file. Backward compatibility for `--verbose` is maintained.

#### Task 3.1 Extend `CommonArgs` in `src/cli.rs`

Add two new fields to `CommonArgs`:

- `debug: bool`: long `--debug`, env `XZATOMA_DEBUG`. Help: "Enable debug-level
  logging. Equivalent to `RUST_LOG=debug`. Takes precedence over `--verbose`."
- `trace: bool`: long `--trace`, env `XZATOMA_TRACE`. Help: "Enable trace-level
  logging. Equivalent to `RUST_LOG=trace`. Implies `--debug`. Use for LLM
  transcript capture and deep protocol inspection."

Precedence rule (document in both help texts): `--trace` > `--debug` >
`--verbose` > `RUST_LOG` env > info default.

#### Task 3.2 Add `LogConfig` to `src/config.rs`

Add a new top-level `LogConfig` struct (distinct from the existing
`WatcherLoggingConfig` in the watcher subsystem):

```rust
/// Global log subscriber configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Enable debug-level logging. Env: XZATOMA_DEBUG.
    #[serde(default)]
    pub debug: bool,
    /// Enable trace-level logging. Env: XZATOMA_TRACE.
    #[serde(default)]
    pub trace: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { debug: false, trace: false }
    }
}
```

Add `pub log: LogConfig` to `Config` and `Config::default()`. Wire both fields
in `apply_env_vars()` (L1414) using the existing `parse_env_bool()` helper
(L2751). Document `XZATOMA_DEBUG` and `XZATOMA_TRACE` in the `Config`
module-level doc comment alongside the existing env var table.

#### Task 3.3 Update `init_tracing()` in `src/main.rs`

Change the signature to `fn init_tracing(debug: bool, trace: bool)`. Precedence
logic:

```rust
let level = if trace { "trace" } else if debug { "debug" } else { "xzatoma=info" };
let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(level));
```

Update the call site with backward-compatibility fallback:

```rust
let debug = common.debug || common.verbose;
let trace = common.trace;
init_tracing(debug, trace);
```

#### Task 3.4 Testing Requirements

In `src/cli.rs` mod tests add:

- `test_debug_flag_after_subcommand`: `["xzatoma", "chat", "--debug"]` yields
  `common_args().debug == true`.
- `test_trace_flag_after_subcommand`:
  `["xzatoma", "run", "--trace", "--prompt", "x"]` yields
  `common_args().trace == true`.
- `test_debug_and_trace_independent`: `["xzatoma", "chat", "--debug"]` yields
  `common_args().trace == false`.
- `test_xzatoma_debug_env_sets_flag`: set `XZATOMA_DEBUG=true`, parse
  `["xzatoma", "chat"]`, assert `common_args().debug == true`.

In `src/config.rs` mod tests add:

- `test_log_config_debug_field_default_false`
- `test_log_config_trace_field_default_false`
- `test_apply_env_vars_overrides_log_debug`
- `test_apply_env_vars_overrides_log_trace`

#### Task 3.5 Deliverables

- [ ] `src/cli.rs`: `debug: bool` and `trace: bool` in `CommonArgs`.
- [ ] `src/config.rs`: `LogConfig` struct; `log: LogConfig` on `Config`; env var
      wiring in `apply_env_vars()`.
- [ ] `src/main.rs`: `init_tracing(debug, trace)` with backward compat for
      `--verbose`.
- [ ] New CLI tests for `--debug` and `--trace`.
- [ ] New config tests for `LogConfig` defaults and env var overrides.

#### Task 3.6 Success Criteria

- `xzatoma run --debug --prompt "hello"` produces only `DEBUG` and above.
- `xzatoma chat --trace` produces `TRACE` lines.
- `xzatoma run --verbose --prompt "hello"` continues to activate `DEBUG`.
- `XZATOMA_DEBUG=true xzatoma chat` enables debug without the CLI flag.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All existing and new tests pass.

---

### Phase 4: Configurable Log Format and Global File Sink

Adds per-sink format control and a top-level `--logfile` flag to `CommonArgs`.
The `json` feature of `tracing-subscriber` is already compiled in. The new
global `LogConfig` is complementary to (not a replacement for) the existing
`WatcherLoggingConfig`, which controls per-event watcher file output
independently.

#### Task 4.1 Extend `CommonArgs` in `src/cli.rs`

Add two new fields to `CommonArgs`:

- `log_format: Option<LogFormat>`: long `--log-format`, accepts
  `plain | compact | json`, env `XZATOMA_LOG_FORMAT`. When set, overrides
  `config.log.stderr_format`.
- `log_file: Option<PathBuf>`: long `--logfile`, env `XZATOMA_LOG_FILE`. Writes
  a second log stream to the given path. This is a new top-level flag distinct
  from the `log_file` field that already exists on the `Watch` variant (L128),
  which remains in place for watcher-specific output.

`LogFormat` must be defined in `src/config.rs` (Task 4.2) and re-exported or
imported in `src/cli.rs`.

#### Task 4.2 Extend `LogConfig` in `src/config.rs`

Add `LogFormat` and extend `LogConfig` with format and file-path fields:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable multi-line format with ANSI colour. Default for stderr.
    #[default]
    Plain,
    /// Single-line compact text without ANSI colour.
    Compact,
    /// Newline-delimited JSON. Default for file sinks.
    Json,
}

pub struct LogConfig {
    pub debug: bool,
    pub trace: bool,
    #[serde(default)]
    pub stderr_format: LogFormat,
    #[serde(default = "default_log_file_format")]
    pub file_format: LogFormat,      // defaults to LogFormat::Json
    #[serde(default)]
    pub file_path: Option<PathBuf>,
}
```

Add env var parsing for `XZATOMA_LOG_FORMAT`, `XZATOMA_LOG_STDERR_FORMAT`, and
`XZATOMA_LOG_FILE_FORMAT` in `apply_env_vars()`. Document all five new env vars
(`XZATOMA_DEBUG`, `XZATOMA_TRACE`, `XZATOMA_LOG_FORMAT`,
`XZATOMA_LOG_STDERR_FORMAT`, `XZATOMA_LOG_FILE_FORMAT`) in the `Config`
module-level doc comment.

#### Task 4.3 Refactor `init_tracing()` in `src/main.rs`

Change the signature to:

```rust
fn init_tracing(
    debug: bool,
    trace: bool,
    stderr_format: LogFormat,
    file_format: LogFormat,
    log_file: Option<&Path>,
)
```

Build the stderr layer conditionally on `stderr_format`:

```rust
let stderr_layer = match stderr_format {
    LogFormat::Plain   => fmt::layer().with_writer(stderr).boxed(),
    LogFormat::Compact => fmt::layer().compact().with_writer(stderr).boxed(),
    LogFormat::Json    => fmt::layer().json().with_writer(stderr).boxed(),
};
```

Apply the same pattern for the optional file layer when `log_file` is `Some`,
defaulting to `LogFormat::Json` for files. Because `init_tracing()` runs before
`Config::load()`, derive format from `common.log_format` at the call site,
defaulting to `LogFormat::Plain` when absent. The config-file value in
`config.log.stderr_format` is available after loading and can be applied to a
second subscriber reload if needed, or the CLI override takes precedence
throughout the process lifetime.

#### Task 4.4 Update `config/config.yaml`

Add a documented `log:` section after the `skills:` block:

```yaml
# log:
#   # Format for stderr output. Options: plain | compact | json
#   # Default: plain
#   stderr_format: plain
#
#   # Format for the optional file sink. Options: plain | compact | json
#   # Default: json
#   file_format: json
#
#   # Optional path to write logs to a file in addition to stderr.
#   # file_path: /var/log/xzatoma.log
```

#### Task 4.5 Testing Requirements

- Unit tests for `LogFormat` serde round-trip: `plain`, `compact`, `json`.
- Unit test for `LogConfig::default()`: `stderr_format == Plain`,
  `file_format == Json`.
- CLI test: `["xzatoma", "chat", "--log-format", "json"]` yields
  `common_args().log_format == Some(LogFormat::Json)`.
- CLI test: `["xzatoma", "run", "--logfile", "/tmp/x.log", "--prompt", "hi"]`
  yields `common_args().log_file == Some(PathBuf::from("/tmp/x.log"))`.

#### Task 4.6 Deliverables

- [ ] `src/cli.rs`: `log_format` and `log_file` in `CommonArgs`.
- [ ] `src/config.rs`: `LogFormat` enum; `LogConfig` extended with
      `stderr_format`, `file_format`, `file_path`; env var wiring.
- [ ] `src/main.rs`: `init_tracing()` dispatches correct formatter per sink;
      optional file layer attached.
- [ ] `config/config.yaml`: `log:` section documented.
- [ ] Unit tests for `LogFormat` serde and `LogConfig` defaults.

#### Task 4.7 Success Criteria

- `xzatoma run --log-format json --prompt "hi"` emits NDJSON to stderr.
- `xzatoma run --logfile out.log --prompt "hi"` writes JSON to the file and
  plain text to stderr.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.

---

### Phase 5: Structured Trace Transcript

At `TRACE` level, the agent core emits the full provider-visible conversation so
operators can reconstruct exactly what the model saw and responded with,
including tool invocations. All expensive formatting is guarded behind
`tracing::enabled!(Level::TRACE)` so release builds at `INFO` or `DEBUG` pay no
serialisation cost.

#### Task 5.1 Conversation Message Logging (`src/agent/core.rs`)

In both `execute_with_observer()` (L556-801) and
`execute_provider_messages_with_observer()` (L898-1158), add a trace block
immediately before each `provider.complete()` call, gated by
`tracing::enabled!(Level::TRACE)`. For each message in `prompt_messages`, emit
one `trace!` event with structured fields: `msg.index` (position in the slice),
`msg.role`, `msg.char_count` (content byte length), and `msg.content` (full text
as a dedicated field, not embedded in the format string).

After the provider responds (after the `tokio::select!` block), emit a `trace!`
event with fields: `has_tool_calls`, `tool_call_count` (when
`has_tool_calls == true`), and `response_chars`.

#### Task 5.2 Tool Call and Result Logging (`src/agent/core.rs`)

In `execute_tool_call()` (L1175-1212):

- Before dispatching, emit a `trace!` event with fields: `tool.name`,
  `tool.call_id` (`tool_call.id`), `tool.args_json`
  (`tool_call.function.arguments`).
- After the tool returns and before truncation, emit a `trace!` event with
  fields: `tool.name`, `tool.call_id`, `tool.result_bytes`
  (`result.output.len()` before truncation), `tool.result_preview` (first 200
  chars of the output).

These supplement, not replace, the existing
`debug!("Executing tool: {}", tool_name)` at L1177 and the `AgentObserver`
events. The `debug!` call at L1177 is unchanged.

#### Task 5.3 Provider Metadata Logging (`src/agent/core.rs`)

Add `fn log_provider_metadata(&self)` on `Agent`. Call
`self.provider.get_current_model()` (the owned synchronous variant from the
`Provider` trait) and emit a `trace!` event with fields `provider.model` (the
returned string or `"unknown"` when `None`) and `provider.type`
(`std::any::type_name` of the concrete provider). Call this helper once at the
start of `execute_with_observer()` and
`execute_provider_messages_with_- observer()`, gated by
`tracing::enabled!(Level::TRACE)`. Because `get_current_model()` is synchronous,
no async API call is made at lower log levels.

#### Task 5.4 Testing Requirements

- All existing tests in `src/agent/core.rs` mod tests (L1439-2436) must pass
  unchanged.
- Add `test_log_provider_metadata_no_panic_when_model_is_none`: construct a
  `MockProvider` returning `None` from `current_model()`, call the helper,
  assert no panic.
- Add `test_log_provider_metadata_uses_model_name`: construct a `MockProvider`
  returning `Some("test-model")`, confirm the helper completes without error.
- Use the existing `MockProvider` in `mod tests` (L1446-1468); no real
  providers.

#### Task 5.5 Deliverables

- [ ] `src/agent/core.rs`: structured `trace!` fields for conversation messages,
      tool call arguments, tool results, and response finish condition.
- [ ] `src/agent/core.rs`: `log_provider_metadata()` helper on `Agent`.
- [ ] Unit tests for the helper with mock providers.

#### Task 5.6 Success Criteria

- `xzatoma run --trace --prompt "hello"` produces one `TRACE` event per
  conversation message, each with `msg.role` and `msg.content` as separate
  structured fields.
- `xzatoma run --debug --prompt "hello"` (no `--trace`) produces no per-message
  trace events (guard is effective).
- `cargo clippy --all-targets --all-features -- -D warnings` passes.

---

### Phase 6: Documentation Updates

All guides that show flags before the subcommand, reference `--verbose` as the
primary logging mechanism, or use bare `RUST_LOG` invocations must be updated.
New reference documentation covers the full logging configuration surface and
the distinction between the global subscriber and the per-watcher
`WatcherLoggingConfig`.

#### Task 6.1 How-to Guides

- [`docs/how-to/debug_subagents.md`](../how-to/debug_subagents.md): Replace
  `RUST_LOG=debug` examples with `xzatoma <subcommand> --debug`. Add a note that
  `RUST_LOG` still takes precedence when set explicitly.
- [`docs/how-to/configure_providers.md`](../how-to/configure_providers.md):
  Update any logging examples to use `--debug` or `--trace` in the correct
  post-subcommand position.
- [`docs/how-to/setup_watcher.md`](../how-to/setup_watcher.md): Add
  `xzatoma watch --debug` and `xzatoma watch --trace` as diagnosis examples.
  Note the distinction between the global subscriber flags and the
  `watcher.logging` config block, which controls the per-event watcher file sink
  independently.
- [`docs/how-to/run_xzatoma_as_an_acp_server.md`](../how-to/run_xzatoma_as_an_acp_server.md):
  Add `xzatoma acp --debug serve` and `xzatoma acp --trace serve` as
  alternatives to raw `RUST_LOG` invocations. Keep `RUST_LOG=xzatoma::acp=debug`
  for targeted module-level filtering.
- [`docs/how-to/zed_acp_agent_setup.md`](../how-to/zed_acp_agent_setup.md): Zed
  env blocks must continue using `RUST_LOG` since Zed passes env vars, not CLI
  flags. Add a note mapping `RUST_LOG=xzatoma=debug` to the same level as
  `--debug`.

#### Task 6.2 Reference and Config Documentation

- `config/config.yaml`: `log:` section documented (Phase 4 deliverable).
- `src/config.rs` module-level doc comment: Add `XZATOMA_DEBUG`,
  `XZATOMA_TRACE`, `XZATOMA_LOG_FORMAT`, `XZATOMA_LOG_STDERR_FORMAT`, and
  `XZATOMA_LOG_FILE_FORMAT` to the environment variable table.
- `src/cli.rs` module-level doc comment: Remove the lines that describe global
  options as placeable before or after the subcommand. Update to describe the
  `CommonArgs` pattern.
- `src/cli.rs` `CommonArgs.verbose` doc comment: "Deprecated: use `--debug` or
  `--trace` instead."
- Create [`docs/reference/logging.md`](../reference/logging.md): describe log
  levels, env var overrides, format options (`plain`, `compact`, `json`), file
  sink configuration, the `RUST_LOG` override mechanism, and the distinction
  between the global subscriber and the `watcher.logging` config block.
- Update `README.md`: replace any examples showing flags before the subcommand
  with the `xzatoma subcommand --flag` pattern.

#### Task 6.3 Deliverables

- [ ] `docs/how-to/debug_subagents.md`: updated flag order and `--debug` /
      `--trace` examples.
- [ ] `docs/how-to/configure_providers.md`: updated examples.
- [ ] `docs/how-to/setup_watcher.md`: `--debug` / `--trace` added; watcher
      logging distinction clarified.
- [ ] `docs/how-to/run_xzatoma_as_an_acp_server.md`: updated examples.
- [ ] `src/config.rs`: new env vars documented.
- [ ] `src/cli.rs`: module-level doc comment and `verbose` deprecation note
      updated.
- [ ] `docs/reference/logging.md`: new reference document created.
- [ ] `README.md`: flag-after-subcommand examples throughout.

#### Task 6.4 Success Criteria

- `markdownlint --fix --config .markdownlint.json` passes on all modified files.
- `prettier --write --parser markdown --prose-wrap always` passes on all
  modified files.
- No doc file shows a shared flag (`--config`, `--verbose`, `--debug`,
  `--trace`, `--log-format`, `--logfile`) before the subcommand token.
- No doc file references `--verbose` as the primary mechanism for enabling debug
  or trace logging.
