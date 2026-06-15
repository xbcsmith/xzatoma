# Phase 1: CLI Architecture Implementation

## Overview

This document summarizes the implementation of Phase 1 from the CLI Flag
Placement and Logging Refinement plan. The goal was to replace three
`global = true` fields on `Cli` with a `CommonArgs` struct flattened into every
`Commands` variant, enforcing the `xzatoma subcommand --flag` placement pattern.

## Changes

### `src/cli.rs`

- Added `CommonArgs` struct with `#[derive(Args, Debug, Clone)]` carrying
  `config`, `verbose`, and `storage_path`. Added `impl Default for CommonArgs`.
- Removed `config`, `verbose`, and `storage_path` from `Cli`. The struct now
  only carries `#[command(subcommand)] pub command: Commands`.
- Added `#[command(flatten)] common: CommonArgs` as the first named field in all
  eleven `Commands` variants (`Chat`, `Run`, `Agent`, `Watch`, `Auth`, `Models`,
  `History`, `Replay`, `Mcp`, `Acp`, `Skills`).
- Added `impl Commands { pub fn common_args(&self) -> &CommonArgs }` as the
  single extraction point for shared flags.
- Updated `impl Default for Cli` to construct `CommonArgs::default()` inside the
  `Auth` variant.
- Updated all ~100 existing unit tests: moved shared flags to post-subcommand
  position, updated assertions from `cli.config` / `cli.verbose` /
  `cli.storage_path` to `cli.command.common_args().*`.
- Added four new tests required by Task 1.8: `test_common_args_config_default`,
  `test_common_args_storage_path_env`,
  `test_flag_before_subcommand_is_rejected`,
  `test_nested_subcommand_with_common_flags`.

### `src/config.rs`

- Changed `Config::load(path, cli: &crate::cli::Cli)` signature to
  `Config::load(path, common: &crate::cli::CommonArgs)`.
- Changed `apply_cli_overrides(&mut self, cli: &crate::cli::Cli)` to accept
  `&crate::cli::CommonArgs`; updated the `cli.verbose` field reference.
- Updated `test_load_nonexistent_file_uses_defaults` to construct `CommonArgs`
  directly instead of `Cli`.

### `src/main.rs`

- Extracts common args immediately after parsing:
  `let common = cli.command.common_args().clone()`.
- Passes `common.verbose` to `init_tracing`, reads `common.storage_path` for the
  `XZATOMA_HISTORY_DB` override, and derives `config_path` from `common.config`.
- Passes `&common` to `Config::load`.
- Added `..` to all eleven `match cli.command` arms so `common` is silently
  ignored during command dispatch.

### Integration and eval tests

Updated all integration test files that invoked the binary with `--config` or
`--storage-path` before the subcommand token:

- `tests/skills_config.rs` - updated helper functions and `Config::load` call
  sites.
- `tests/mcp_config_test.rs` - replaced `make_cli()` helper with
  `make_common()`.
- `tests/acp_cli.rs` - moved `--config` to after `acp` in all nine tests.
- `tests/agent_cli.rs` - moved `--config` to after `agent` in all call sites.
- `tests/skills_cli.rs` - moved `--config` to after `skills` in all call sites.
- `tests/integration_subagent.rs` - removed unused `--config` from `--version`
  calls; moved `--config` after `run` in validation-failure tests.
- `tests/subagent_configuration_integration.rs` - same pattern; applied
  `cargo fix` to prefix now-unused `_config_path` variables.
- `tests/eval_cli_commands.rs` - updated `run_scenario` harness to emit the
  subcommand token before injecting `--config <path>`.
- `src/commands/history.rs` - moved `--storage-path` to after `history` in two
  binary-invocation tests.

## Success Criteria Verification

- `xzatoma watch --config config/config.yaml` parses and the config path is read
  correctly.
- `xzatoma --config config/config.yaml watch` fails to parse (enforced by the
  new four test `test_flag_before_subcommand_is_rejected`).
- `xzatoma models --verbose list` parses and `common_args().verbose == true`
  (covered by `test_nested_subcommand_with_common_flags`).
- `cargo fmt --all` passes.
- `cargo check --all-targets --all-features` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- All tests pass (2246 unit + ~100 integration tests across all test binaries).
