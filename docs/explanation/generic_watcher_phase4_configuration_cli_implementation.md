# Generic watcher phase 4 configuration cli implementation

## Overview

This document explains the implementation of Phase 4: Configuration and CLI
Updates for the generic watcher work.

The goal of this phase is to make the generic watcher selectable and
configurable through the main configuration model, environment variables, and
the `watch` command-line interface while preserving full backward compatibility
for the existing XZepr watcher.

Phase 4 introduces:

- a watcher backend selector via `WatcherType`
- generic watcher configuration fields under `WatcherConfig`
- generic watcher environment variable overrides
- new `watch` command flags for backend selection and generic matching
- startup validation rules for generic watcher configuration
- tests covering YAML parsing, environment overrides, CLI overrides, and
  validation behavior

## Design goals

The implementation follows several key design rules from the watcher plan.

### Backward compatibility

Existing watcher configurations that omit `watcher_type` must continue to work
unchanged. The default backend remains `xzepr`.

### Strict backend separation

Although both watcher backends live under the same top-level watcher
configuration block, they remain separate in code and intent:

- `WatcherType::XZepr` uses `EventFilterConfig`
- `WatcherType::Generic` uses `GenericMatchConfig`

The XZepr watcher must not read generic watcher match settings, and the generic
watcher must not read XZepr event filter settings.

### Early failure for invalid regex

When the generic watcher is selected, invalid regex values in
`watcher.generic_match` must fail configuration validation immediately instead
of surfacing later during runtime event handling.

### Non-breaking accept-all mode

A generic watcher with no configured match fields is valid. It runs in
accept-all mode, but startup emits a warning because this is likely risky in
production.

## Configuration changes

## `WatcherType`

A new enum is added to `src/config.rs`:

- `WatcherType::XZepr`
- `WatcherType::Generic`

The enum derives:

- `Debug`
- `Clone`
- `Serialize`
- `Deserialize`
- `Default`
- `PartialEq`
- `Eq`

It uses `#[serde(rename_all = "lowercase")]` so the YAML values are:

- `xzepr`
- `generic`

`WatcherType::XZepr` is the default to preserve backward compatibility.

Helper methods were added to support configuration and CLI parsing:

- `as_str() -> &'static str`
- `from_str_name(&str) -> Option<WatcherType>`

The parser is case-insensitive and trims whitespace so CLI and environment input
are handled robustly.

## `WatcherConfig`

`WatcherConfig` now contains:

- `watcher_type: WatcherType`
- `kafka: Option<KafkaWatcherConfig>`
- `generic_match: GenericMatchConfig`
- `filters: EventFilterConfig`
- `logging: WatcherLoggingConfig`
- `execution: WatcherExecutionConfig`

The new fields use `#[serde(default)]` so older YAML configurations still parse
without requiring changes.

## `KafkaWatcherConfig::output_topic`

`KafkaWatcherConfig` now contains:

- `brokers`
- `topic`
- `output_topic`
- `group_id`
- `security`

`output_topic` is documented as a generic-watcher-only result publishing target.
If it is `None`, the generic watcher publishes results back to the input topic.

That behavior is intentionally explicit in both the struct doc comments and the
validation/documentation work so same-topic operation is a first-class supported
case.

## `GenericMatchConfig`

A new `GenericMatchConfig` struct is defined in `src/config.rs` with:

- `action: Option<String>`
- `name: Option<String>`
- `version: Option<String>`

Each field represents a regex pattern for the corresponding field in
`GenericPlanEvent`. These patterns are compiled later by the generic matcher,
but Phase 4 also validates them eagerly when the generic watcher is selected.

The struct is intentionally separate from `EventFilterConfig`. They are not
interchangeable, and they map to different watcher backends and different wire
formats.

## Environment variable support

`Config::apply_env_vars` was extended with the new generic watcher overrides.

### `XZATOMA_WATCHER_TYPE`

Maps to:

- `watcher.watcher_type`

Accepted values:

- `xzepr`
- `generic`

Invalid values are rejected with a warning and do not overwrite the current
setting.

### `XZATOMA_WATCHER_OUTPUT_TOPIC`

Maps to:

- `watcher.kafka.output_topic`

If Kafka config already exists, the output topic is updated in place. If Kafka
config does not exist yet, a minimal watcher Kafka config is created so the
output topic can still be represented in the loaded configuration.

### `XZATOMA_WATCHER_MATCH_ACTION`

Maps to:

- `watcher.generic_match.action`

### `XZATOMA_WATCHER_MATCH_NAME`

Maps to:

- `watcher.generic_match.name`

### `XZATOMA_WATCHER_MATCH_VERSION`

Maps to:

- `watcher.generic_match.version`

These three environment variables allow generic matcher rules to be injected
without editing YAML configuration files.

## CLI updates

The `Watch` subcommand in `src/cli.rs` was extended with four new Phase 4 flags
plus the backend selector.

The added fields are:

- `watcher_type: Option<String>`
- `output_topic: Option<String>`
- `action: Option<String>`
- `name: Option<String>`

The command already had `dry_run`, `topic`, `event_types`, `filter_config`,
`log_file`, and `json_logs`, so Phase 4 extends the existing command instead of
introducing a new one.

### New flags

#### `--watcher-type`

Selects the backend:

- `xzepr`
- `generic`

The CLI default is `xzepr`.

#### `--output-topic`

Overrides `watcher.kafka.output_topic`.

This is primarily relevant for the generic watcher.

#### `--action`

Overrides `watcher.generic_match.action`.

#### `--name`

Overrides `watcher.generic_match.name`.

The Phase 4 plan explicitly called for these four CLI additions, and the
implementation applies them as configuration overrides before watcher startup.

## CLI override application

The `watch::apply_cli_overrides` logic in `src/commands/mod.rs` was extended to
apply the new Phase 4 flags.

The override flow now supports:

- watcher backend selection
- generic watcher output topic override
- generic matcher action override
- generic matcher name override

### Watcher type parsing

The watcher type string is parsed through `WatcherType::from_str_name`. Invalid
values return an immediate configuration error.

This prevents silent fallback behavior and ensures the user knows right away
when they have provided an unsupported backend name.

### Output topic override

If Kafka config is already present, `output_topic` is written into
`watcher.kafka.output_topic`.

### Generic matcher overrides

If `--action` is provided, it updates:

- `watcher.generic_match.action`

If `--name` is provided, it updates:

- `watcher.generic_match.name`

These changes are independent of the XZepr filter fields and preserve the
backend separation requirement from the implementation plan.

## Validation behavior

`Config::validate` was extended with the new watcher-type-dependent rules.

### Generic watcher with no Kafka config

If:

- `watcher.watcher_type == generic`
- `watcher.kafka == None`

validation fails immediately.

This prevents starting the generic watcher without a topic or broker
configuration.

### Generic watcher with no match fields

If:

- `watcher.watcher_type == generic`
- `action == None`
- `name == None`
- `version == None`

validation succeeds, but a warning is emitted stating that accept-all mode is
active.

This behavior is intentional. Accept-all mode is valid, but it deserves startup
visibility because it may be unintentional.

### Generic watcher with configured regex fields

If the generic watcher is selected and any of the following are populated:

- `watcher.generic_match.action`
- `watcher.generic_match.name`
- `watcher.generic_match.version`

each value is compiled as a regex during validation.

If compilation fails, validation returns an error immediately.

This guarantees that malformed patterns are caught at startup instead of causing
unexpected watcher behavior later.

### XZepr watcher with generic match fields populated

If:

- `watcher.watcher_type == xzepr`
- any generic match field is set

validation does not fail. Instead, a debug message indicates that the generic
match config is unused.

This preserves compatibility while still making it observable that the user has
set fields that are not relevant to the active backend.

## Testing strategy

Phase 4 includes tests for all of the requested areas.

## YAML round-trip tests

Tests verify:

- `WatcherType::XZepr` round-trips correctly through YAML
- `WatcherType::Generic` round-trips correctly through YAML
- omitted `watcher_type` defaults to `WatcherType::XZepr`
- `GenericMatchConfig` round-trips with:
  - all fields populated
  - only action
  - only name and version
  - all fields omitted
- `KafkaWatcherConfig` round-trips with `output_topic`

These tests confirm both serialization format and backward compatibility.

## Environment variable tests

Tests cover the new environment variable support for:

- `XZATOMA_WATCHER_TYPE`
- `XZATOMA_WATCHER_OUTPUT_TOPIC`
- `XZATOMA_WATCHER_MATCH_ACTION`
- `XZATOMA_WATCHER_MATCH_NAME`
- `XZATOMA_WATCHER_MATCH_VERSION`

Because these tests modify global process environment state, they are marked as
ignored to avoid parallel test interference, matching the existing pattern
already used in the codebase.

## CLI parsing tests

Tests in `src/cli.rs` verify:

- default `watch` parsing
- explicit parsing of:
  - `--watcher-type`
  - `--output-topic`
  - `--action`
  - `--name`
  - `--dry-run`

These tests ensure the new flags are visible and parsed correctly by clap.

## CLI override tests

Tests in `src/commands/mod.rs` verify that `apply_cli_overrides` correctly
applies:

- `--watcher-type`
- `--output-topic`
- `--action`
- `--name`

The existing override tests for topic, event types, and JSON logging remain
intact.

## Validation tests

Tests verify:

- generic watcher with no match config is valid
- generic watcher with valid regex patterns is valid
- generic watcher with invalid regex fails validation
- generic watcher with missing Kafka config fails validation

These tests directly cover the Phase 4 validation requirements from the plan.

## Backward compatibility outcomes

Phase 4 preserves compatibility in several ways:

- omitted `watcher_type` defaults to `xzepr`
- existing XZepr watcher configurations still parse
- existing watcher environment overrides still work
- generic match fields do not break XZepr validation
- `output_topic` is optional and does not affect existing configs unless used

This means existing users can upgrade without changing their watcher config
files, while new generic watcher users gain the fields they need.

## Relationship to Phase 3

Phase 3 introduced the generic watcher core and required a few configuration
primitives to exist so the code could compile and run. Phase 4 formalizes those
primitives into the full configuration and CLI surface described in the plan.

In practice, Phase 4 completes the external interface around the generic watcher
core by making the backend selectable and configurable through all three normal
entry points:

- YAML config
- environment variables
- CLI flags

## Deliverables completed

This phase completes the following deliverables:

- `WatcherType` enum in `src/config.rs`
- `GenericMatchConfig` in `src/config.rs`
- `output_topic` in `KafkaWatcherConfig`
- updated `Commands::Watch` in `src/cli.rs`
- updated environment variable handling in `Config::apply_env_vars`
- updated validation in `Config::validate`
- YAML, env, CLI, and validation tests
- this implementation explanation document

## Summary

Phase 4 makes the generic watcher configurable and selectable in a way that is
consistent with the rest of the project.

The implementation now supports:

- choosing between XZepr and generic watcher backends
- configuring generic matching rules in YAML
- overriding generic watcher settings through environment variables
- overriding generic watcher settings through CLI flags
- validating regex patterns and required Kafka config at startup
- preserving full backward compatibility for existing XZepr configurations

This creates the configuration and UX foundation needed for Phase 5 to dispatch
between watcher backends during actual watch command execution.
