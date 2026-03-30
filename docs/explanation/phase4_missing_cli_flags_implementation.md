# Phase 4 Task 4.7: Missing CLI Flags Implementation

## Summary

This task adds two missing CLI flags to the `watch` command (`--brokers` and
`--match-version`) and wires the previously inert `--filter-config` flag so that
it actually loads and applies an external filter configuration file at startup.

## Problem

The `watch` command was missing CLI overrides for two important configuration
values:

1. **Kafka brokers** (`kafka.brokers`) -- the most fundamental connection
   parameter had no CLI override, forcing users to edit the config file or set
   environment variables for every broker change.
2. **Generic matcher version** (`generic_match.version`) -- the config file
   supported a `version` regex pattern, but there was no corresponding CLI flag,
   unlike `--action` and `--name` which already existed.

Additionally, the `--filter-config` flag was defined in `cli.rs` and carried
through `WatchCliOverrides`, but `apply_cli_overrides` silently ignored it. Any
user passing `--filter-config filters.yaml` would see no effect.

## Changes

### `src/cli.rs`

Added two new fields to the `Watch` variant of the `Commands` enum:

- `brokers: Option<String>` with `#[arg(long)]` -- accepts a comma-separated
  broker list.
- `match_version: Option<String>` with `#[arg(long = "match-version")]` -- uses
  the long form `--match-version` to avoid conflicting with the built-in
  `--version` flag that clap auto-generates.

Updated the existing `test_cli_parse_watch_defaults` test to assert both new
fields default to `None`.

Added two new CLI parsing tests:

- `test_cli_parse_watch_with_brokers_flag`
- `test_cli_parse_watch_with_match_version_flag`

### `src/commands/mod.rs` -- `WatchCliOverrides`

Added two new fields to the struct:

- `pub brokers: Option<String>`
- `pub match_version: Option<String>`

Both fields have doc comments and participate in the `Default` derive.

### `src/commands/mod.rs` -- `apply_cli_overrides`

Added three new override blocks at the end of the function:

1. **Brokers override** -- if `overrides.brokers` is `Some`, sets
   `kafka.brokers` on the mutable Kafka config and logs via `tracing::debug`.
2. **Match version override** -- if `overrides.match_version` is `Some`, sets
   `config.watcher.generic_match.version` and logs.
3. **Filter config override** -- if `overrides.filter_config` is `Some(path)`:
   - Reads the file at the given path with `std::fs::read_to_string`.
   - Deserializes it as `crate::config::EventFilterConfig` via `serde_yaml`.
   - Replaces `config.watcher.filters` with the parsed value.
   - Maps I/O and parse errors to `XzatomaError::Config(...)`.

Added five new tests:

- `test_apply_cli_overrides_brokers` -- verifies broker string is applied.
- `test_apply_cli_overrides_match_version` -- verifies version regex is applied.
- `test_apply_cli_overrides_filter_config_from_file` -- writes a temp YAML file,
  passes it as `filter_config`, and asserts that the parsed `EventFilterConfig`
  is applied (checks `event_types` and `success_only`).
- `test_apply_cli_overrides_filter_config_missing_file` -- passes a nonexistent
  path and asserts the error message contains
  `"Failed to read filter config file"`.

### `src/main.rs`

Updated the `Commands::Watch { ... }` destructuring and the
`WatchCliOverrides { ... }` construction to include the two new fields:
`brokers` and `match_version`.

## Testing

All new and existing tests pass:

| Test                                                  | Scope                                   |
| ----------------------------------------------------- | --------------------------------------- |
| `test_cli_parse_watch_defaults`                       | Updated to assert new fields are `None` |
| `test_cli_parse_watch_with_brokers_flag`              | New: parses `--brokers`                 |
| `test_cli_parse_watch_with_match_version_flag`        | New: parses `--match-version`           |
| `test_apply_cli_overrides_brokers`                    | New: wires brokers override             |
| `test_apply_cli_overrides_match_version`              | New: wires version override             |
| `test_apply_cli_overrides_filter_config_from_file`    | New: loads YAML file                    |
| `test_apply_cli_overrides_filter_config_missing_file` | New: error on bad path                  |

## Design Decisions

- **`--match-version` instead of `--version`**: clap reserves `--version` for
  the auto-generated version flag (`#[command(version)]`). Using
  `--match-version` avoids the conflict while staying consistent with the
  `generic_match` config section. The field is named `match_version` in Rust.
- **Filter config replaces rather than merges**: when `--filter-config` is
  provided, the entire `filters` section is replaced with the file contents.
  This matches the mental model of "use this file as the filter config" and
  avoids complex merge semantics.
- **Error mapping to `XzatomaError::Config`**: file read and YAML parse errors
  are wrapped in `XzatomaError::Config` because they represent configuration
  problems the user needs to fix, keeping error categorization consistent with
  the rest of the override logic.
