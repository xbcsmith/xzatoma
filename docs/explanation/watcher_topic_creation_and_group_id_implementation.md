# Watcher topic creation and group id implementation

## Overview

This document describes the follow-up implementation for two operational watcher
features:

- automatic Kafka topic creation during watcher startup
- explicit Kafka consumer group override support through the watch CLI

These features were added after the original generic watcher phases were
completed. They improve the operator experience for both watcher backends:

- `xzepr`
- `generic`

The implementation follows the same design principles already used in the
watcher codebase:

- keep watcher backends as explicit peers
- avoid unnecessary abstraction
- use a stub-first Kafka integration style
- keep startup behavior observable through logs and tests

## Why this follow-up was needed

The original watcher implementation already supported:

- backend selection through `watcher_type`
- Kafka topic configuration through `watcher.kafka.topic`
- consumer group configuration through `watcher.kafka.group_id`
- generic watcher result routing through `output_topic`

However, two practical operational gaps remained:

1. starting watcher mode did not ensure that required Kafka topics existed
2. the watch command did not allow overriding the Kafka consumer group ID from
   the CLI

This follow-up closes those gaps while staying aligned with the repository's
current Kafka architecture.

## Scope of the change

The implementation adds support for:

- automatic topic ensure/create behavior at watcher startup
- config-file support for auto-topic creation
- environment variable override for watcher group ID
- watch CLI support for:
  - `--group-id`
  - `--create-topics`

These changes affect both watcher backends in a backend-appropriate way:

- XZepr watcher ensures its input topic exists
- generic watcher ensures its input topic exists and also ensures its output
  topic exists when distinct from the input topic

## Design approach

## Stub-first Kafka administration

The current watcher codebase uses stub-first Kafka integration rather than a
concrete broker client dependency for all runtime behavior.

That pattern already existed in:

- the XZepr Kafka consumer
- the generic watcher result producer

This follow-up continues that same design approach by adding a shared watcher
topic administration module that:

- resolves which topics should exist
- validates topic names
- assembles Kafka admin-style configuration
- logs ensure/create operations
- remains testable without requiring a broker client dependency at compile time

This means startup behavior is explicit and testable now, while still leaving
room for a future concrete Kafka admin client integration.

## Shared startup concern, backend-specific topic sets

Topic administration is a startup concern that applies to both watcher backends,
so it belongs in shared watcher infrastructure rather than inside one backend's
implementation details.

At the same time, topic requirements differ by backend:

- the XZepr watcher needs its input topic
- the generic watcher needs its input topic
- the generic watcher may also need a distinct output topic

To keep that split clean, the shared topic-admin module resolves topic sets for
each backend separately instead of trying to collapse the backends into one
unified watcher abstraction.

## Files affected

This follow-up touches:

- `src/config.rs`
- `src/cli.rs`
- `src/commands/mod.rs`
- `src/watcher/mod.rs`
- `src/watcher/topic_admin.rs`
- `src/watcher/xzepr/watcher.rs`
- `src/watcher/generic/watcher.rs`

It also implies updates to operator-facing documentation elsewhere in the
project, but this document focuses on the implementation details.

## Configuration changes

## `KafkaWatcherConfig::auto_create_topics`

A new field is added to `KafkaWatcherConfig`:

- `auto_create_topics: bool`

This field controls whether watcher startup should ensure required Kafka topics
exist before entering the consume loop.

### Default behavior

The default is:

- `true`

That means watcher mode is permissive and operationally friendly by default.

### Why a boolean flag is enough for now

At this stage, the repository does not yet support a fully broker-backed topic
provisioning system with explicit partition counts, replication factors, or
advanced retention policies.

Because of that, a single boolean startup toggle is the right level of
complexity:

- simple to understand
- easy to validate
- compatible with the existing stub-first Kafka approach
- easy to extend later if richer topic provisioning settings are added

## `group_id` support status

The watcher already had config-file support for consumer groups through:

- `watcher.kafka.group_id`

So this follow-up did not invent consumer group support from scratch. Instead,
it completed the runtime override story by making group selection available
through additional entry points.

## Environment variable changes

## `XZATOMA_WATCHER_GROUP_ID`

A new watcher-specific environment variable is supported:

- `XZATOMA_WATCHER_GROUP_ID`

It maps to:

- `watcher.kafka.group_id`

This variable complements the pre-existing Kafka environment variable:

- `XZEPR_KAFKA_GROUP_ID`

### Why add a watcher-specific variable

The existing `XZEPR_KAFKA_GROUP_ID` variable is retained for compatibility with
the earlier XZepr-oriented watcher path.

The new `XZATOMA_WATCHER_GROUP_ID` variable makes the override intent clearer
for the expanded watcher system, which now supports both watcher backends.

This gives the configuration model a more coherent watcher-specific surface
without breaking earlier behavior.

## Validation changes

Configuration validation now explicitly checks that:

- `watcher.kafka.group_id` is not empty when Kafka config is present

This protects against invalid consumer configurations introduced through:

- YAML config
- environment-variable overrides
- CLI overrides

The validation remains strict and early, which is consistent with the overall
configuration design in the project.

## CLI changes

## New `watch` flags

The watch command now supports two new flags:

- `--group-id`
- `--create-topics`

### `--group-id`

This flag overrides:

- `watcher.kafka.group_id`

It gives operators a direct runtime way to change the consumer group without
editing config files.

This is especially useful for:

- local testing
- temporary environment-specific overrides
- multi-instance testing with isolated consumer groups
- CI or ephemeral runtime environments

### `--create-topics`

This flag enables auto-topic creation behavior at startup.

It maps conceptually to:

- `watcher.kafka.auto_create_topics = true`

This gives operators an explicit CLI control for startup provisioning behavior.

## `WatchCliOverrides` changes

The watch command already used a grouped overrides struct to keep the command
layer readable and clippy-compliant.

This follow-up extends that struct with fields for:

- `group_id`
- `create_topics`

That preserves the existing clean override pattern without regressing into large
positional-argument functions.

## CLI override application

The watch override logic now applies:

- watcher type
- topic
- output topic
- group ID
- topic auto-creation
- generic matcher action
- generic matcher name
- XZepr event-type filter overrides
- logging overrides
- dry-run signaling

This keeps all watch-mode runtime overrides centralized in one place.

## Shared topic administration module

## New module: `src/watcher/topic_admin.rs`

A new shared module was added:

- `src/watcher/topic_admin.rs`

This module contains the watcher-facing Kafka topic ensure/create abstraction.

## Core type: `WatcherTopicAdmin`

The main type is:

- `WatcherTopicAdmin`

Its responsibilities are:

- store watcher Kafka connection settings relevant to topic administration
- expose Kafka admin-style configuration through `get_kafka_config()`
- resolve required topics for each watcher backend
- build topic ensure requests
- perform the stub ensure/create flow

## Topic resolution behavior

### XZepr watcher

For the XZepr watcher, the topic admin resolves:

- the input topic only

That is the only topic required by the XZepr watcher startup path.

### Generic watcher

For the generic watcher, the topic admin resolves:

- the input topic
- the output topic if configured and distinct from the input topic

If the output topic equals the input topic, the module de-duplicates the topic
set so the same topic is not ensured twice.

This matches the generic watcher's same-topic loop-prevention design while
keeping the startup logic clean.

## Topic ensure requests

The topic admin module also exposes explicit ensure-request structures so
startup behavior is testable and future concrete Kafka integration has a stable
internal shape to build on.

Each request includes:

- the topic name
- a human-readable purpose string

Examples include:

- `xzepr watcher input topic`
- `generic watcher input topic`
- `generic watcher output topic`

These purpose labels improve test clarity and make startup logs easier to read.

## Stub ensure behavior

The current ensure implementation does not contact Kafka directly.

Instead, it:

1. validates topic names
2. logs an ensure/create action for each required topic
3. returns success

This is intentional and matches the repository's current Kafka strategy.

### Why logging is useful here

Even without a real admin client, the stub still adds real value because it:

- makes startup behavior visible
- makes the code path testable
- documents intended operational semantics
- establishes a stable place to integrate a real Kafka admin client later

## Watcher integration

## `src/watcher/mod.rs`

The top-level watcher module now exports:

- `pub mod topic_admin;`

This makes topic administration a shared watcher concern available to both
backends.

## XZepr watcher startup integration

The XZepr watcher startup flow now has a natural place to ensure its input topic
exists before entering the consume loop.

The conceptual flow is:

1. build watcher from config
2. construct topic admin from watcher Kafka settings
3. if `auto_create_topics` is enabled, ensure the XZepr input topic exists
4. continue into normal consumer startup

This keeps topic creation a startup concern rather than a runtime message-loop
concern.

## Generic watcher startup integration

The generic watcher startup flow now has a natural place to ensure its required
topics exist before entering its consume loop.

The conceptual flow is:

1. build watcher from config
2. construct topic admin from watcher Kafka settings
3. if `auto_create_topics` is enabled:
   - ensure input topic exists
   - ensure output topic exists when distinct
4. continue into normal startup

This is especially important for the generic watcher because the output topic is
part of the end-to-end event flow, not just the consume side.

## Test coverage

This follow-up adds or extends tests in several areas.

## Configuration tests

Coverage includes:

- round-trip of `auto_create_topics`
- validation failure for empty `group_id`
- environment-variable override behavior for `XZATOMA_WATCHER_GROUP_ID`

These tests make sure the new config and env-surface area is reliable.

## CLI tests

Coverage includes parsing of:

- `--group-id`
- `--create-topics`

This ensures the watch CLI exposes the new operational controls correctly.

## CLI override tests

Coverage includes applying:

- `group_id`
- topic auto-create behavior

This verifies that command-line overrides are actually reflected in the final
runtime config passed into watcher startup.

## Topic admin tests

The new topic admin module includes unit coverage for:

- basic initialization
- Kafka config assembly
- XZepr topic resolution
- generic watcher topic resolution
- generic watcher de-duplication when input and output topics are the same
- ensure-request purpose labels
- invalid security protocol handling
- stub ensure success paths
- invalid topic-name error handling

This gives the new startup-provisioning path strong coverage without needing a
real Kafka broker in the unit test layer.

## Why this is enough for now

Because topic creation remains stub-first, the most important behaviors to test
today are:

- config correctness
- override correctness
- topic resolution correctness
- startup path observability
- invalid input handling

Those are the things this test coverage targets directly.

## Operational behavior

## Consumer groups

After this follow-up, watcher consumer groups can be set through:

- YAML config:
  - `watcher.kafka.group_id`
- environment:
  - `XZEPR_KAFKA_GROUP_ID`
  - `XZATOMA_WATCHER_GROUP_ID`
- CLI:
  - `xzatoma watch --group-id <value>`

This makes consumer-group selection flexible enough for real operational use.

## Topic provisioning

After this follow-up, watcher startup can be configured to ensure topics exist
through:

- YAML config:
  - `watcher.kafka.auto_create_topics`
- CLI:
  - `xzatoma watch --create-topics`

The current behavior is still stub-first, but the startup semantics are now
defined and exercised.

## Backward compatibility

This implementation preserves backward compatibility in several ways:

- existing watcher configs that omit `auto_create_topics` use the default
- existing group ID config remains valid
- `XZEPR_KAFKA_GROUP_ID` still works
- the watch command remains compatible for users who do not pass the new flags
- watcher backends remain selected through the existing `watcher_type` flow

This means the new behavior is additive rather than disruptive.

## Limitations

The biggest current limitation is that topic creation remains a stub-first
implementation.

That means:

- no broker-side topic existence check is performed yet
- no actual topic creation request is sent to Kafka yet
- no partition-count or replication-factor controls exist yet

This is consistent with the rest of the current Kafka architecture in the repo,
but it is still important to call out explicitly.

## Future extension path

This implementation creates a clean future path for richer Kafka provisioning.

Natural next steps would be:

- replace stub ensure logic with a real Kafka admin client
- add partition and replication settings to topic provisioning config
- allow output-topic-specific provisioning controls
- surface create-topic failures with richer diagnostics
- possibly add watcher startup metrics for provisioning behavior

Because the topic administration logic is now isolated in its own shared module,
those future enhancements can be added without entangling backend-specific
watcher code.

## Summary

This follow-up adds two important watcher operational capabilities:

- topic auto-creation semantics at watcher startup
- explicit consumer-group override support through the watch CLI

The implementation fits the current architecture by:

- using a shared watcher startup helper
- preserving backend separation
- following the repository's stub-first Kafka pattern
- keeping configuration, CLI, and startup behavior aligned

As a result, the watcher system is now more practical for real-world operation
without introducing unnecessary complexity or breaking the existing design.
