# Generic watcher phase 5 integration testing documentation implementation

## Overview

This document explains the completed implementation of Phase 5: Integration,
Testing, and Documentation for the generic watcher work.

Earlier phases established the structural and functional pieces required for the
generic watcher feature:

- Phase 1 moved the XZepr watcher code into its canonical module layout
- Phase 2 introduced the generic plan-event and result-event message formats
- Phase 3 implemented the generic watcher core
- Phase 4 added watcher backend selection, configuration, environment-variable
  support, and CLI flags

Phase 5 completes the feature by integrating the watcher backends behind the
main `watch` command, adding higher-level tests, shipping a generic watcher
example configuration, and updating the documentation so operators can actually
use the system end-to-end.

The completed work in this phase includes:

- dispatch on `watcher_type` in `commands::watch::run_watch`
- a generic watcher example configuration file
- integration-oriented tests for watch-command startup behavior
- additional generic watcher `process_event` tests
- updated setup, environment-variable, configuration, and architecture
  documentation
- this explanation document

## Goals of Phase 5

Phase 5 had four practical goals:

1. make `watcher_type` affect real runtime behavior
2. provide an example configuration that users can start from immediately
3. add integration-level confidence around dispatch and dry-run processing
4. document both watcher backends as first-class supported options

This phase is where the generic watcher becomes an operator-facing feature
rather than just an internal code path.

## Watch command dispatch

## Problem before Phase 5

Before this phase, the codebase already had:

- `WatcherType`
- generic watcher config
- `GenericWatcher`
- generic matcher and producer logic
- CLI flags for selecting the watcher backend

But `commands::watch::run_watch` still always constructed `XzeprWatcher`. That
meant the configuration and CLI surface existed without the watch command
actually honoring the chosen watcher backend.

## Implemented dispatch behavior

The watch command now dispatches on `config.watcher.watcher_type`:

- `WatcherType::XZepr` constructs `crate::watcher::XzeprWatcher`
- `WatcherType::Generic` constructs `crate::watcher::generic::GenericWatcher`

Both backends already expose the same external startup shape:

- constructor taking `Config` and `dry_run`
- async `start() -> Result<()>`

That common outer shape allows the watch command to stay simple while still
keeping the backend implementations separate and explicit.

The dispatch logic lives in `commands::watch::run_watch`, which now:

1. applies CLI overrides
2. initializes watcher logging
3. logs watcher startup context, including the active watcher type
4. matches on `config.watcher.watcher_type`
5. constructs the appropriate watcher
6. enters the backend-specific `start()` path under shared shutdown handling

## Why this approach fits the architecture

This dispatch design aligns with the project constraints:

- no unnecessary abstraction layer
- no premature shared watcher trait
- clear technical ownership boundaries
- simple and readable command-level control flow

The generic and XZepr watcher backends remain equal peers. The command layer
selects between them, but it does not try to collapse them into a forced common
internal abstraction.

## Constructor validation timing

Phase 5 also improved the structure of `run_watch` so watcher construction
happens before entering the signal-handling wait path.

That matters because constructor-time validation errors, such as missing Kafka
configuration, now surface immediately in the normal command path rather than
only after entering the asynchronous startup/select logic.

This was especially important for the new dispatch tests covering missing Kafka
configuration for both backends.

## Example configuration file

## Added file

Phase 5 ships a new example configuration file:

- `config/generic_watcher.yaml`

This file is intended to be the primary operator reference for generic watcher
setup.

## Scenarios covered in the example file

The configuration file includes three documented scenarios.

### Minimal example

The minimal example shows:

- `watcher_type: generic`
- Kafka brokers and input topic
- a simple `action` match
- logging configuration
- conservative execution settings

This is the shortest practical path to trying the generic watcher.

### Production example

The production example shows:

- a secured Kafka setup using `SASL_SSL`
- a dedicated `output_topic`
- all three generic match fields
- higher concurrency
- a longer execution timeout

This gives operators a realistic starting point for a more serious deployment.

### Accept-all example

The accept-all example intentionally leaves all generic match fields unset.

That means every event with `event_type == "plan"` on the configured topic is
accepted by the generic watcher.

This scenario is valid and supported, but it is called out explicitly because it
is powerful and easy to enable unintentionally.

## Active example section

The file also includes an active, ready-to-use example at the bottom, using:

- `provider.type: copilot`
- `copilot.model: gpt-5-mini`
- `watcher_type: generic`
- `topic: plans.events`
- `output_topic: plans.results`
- `generic_match.action: deploy`

This means a user can point the watch command at the file directly instead of
manually copying one of the commented examples first.

## Integration and higher-level testing

## Dispatch behavior tests

Phase 5 adds integration-style tests around `commands::watch::run_watch` for the
backend selection path.

### XZepr backend with missing Kafka config

A test now confirms that when:

- `watcher_type == WatcherType::XZepr`
- `watcher.kafka == None`

then `run_watch(...)` returns an error.

This preserves the existing startup requirement for the XZepr watcher path.

### Generic backend with missing Kafka config

A parallel test now confirms that when:

- `watcher_type == WatcherType::Generic`
- `watcher.kafka == None`

then `run_watch(...)` also returns an error.

This ensures consistent startup behavior across both watcher backends.

## Why these tests matter

These tests validate two important properties:

1. the watch command is now actually dispatching by watcher type
2. constructor validation remains enforced for both backends

Because the watcher Kafka integration is still stub-first in this codebase, this
level of integration testing is the right one for the current architecture. It
tests real control flow without pretending there is already a fully
broker-backed end-to-end environment in the unit test layer.

## Generic watcher `process_event` integration tests

Phase 5 also expands the generic watcher test coverage with direct
`process_event` tests.

These tests exercise the higher-level watcher processing path rather than only
the lower-level matcher behavior.

### Matching event is processed

A test now creates a `GenericWatcher` configured with:

- `GenericMatchConfig { action: Some("deploy") }`

It then builds a matching `GenericPlanEvent` and calls `process_event(...)`.

The assertions verify that:

- the event is classified as `MessageDisposition::Processed`
- a `GenericPlanResult` is published
- the published result has `event_type == "result"`
- the published result is marked successful
- the trigger event ID is preserved

### Non-matching event is skipped

A companion test uses the same watcher configuration but sends an event with:

- `action = "rollback"`

The assertions verify that:

- the event is classified as `MessageDisposition::SkippedNoMatch`
- no results are published

## Relationship to earlier tests

These Phase 5 tests complement earlier phases rather than replacing them:

- Phase 3 matcher tests verified field-level matching behavior
- Phase 3 dry-run watcher tests verified payload handling and loop prevention
- Phase 4 tests verified config parsing, environment overrides, CLI parsing, and
  validation

Phase 5 adds the next layer up:

- command dispatch behavior
- backend selection behavior
- watcher-level `process_event` integration behavior

## Setup documentation

## `docs/how-to/setup_watcher.md`

The watcher setup guide was rewritten to support both watcher backends rather
than only the XZepr flow.

The updated guide now covers:

- backend selection
- minimal XZepr watcher setup
- generic watcher setup
- generic watcher CLI examples
- generic watcher output-topic behavior
- generic match modes
- `GenericPlanEvent` producer payload example
- environment-variable usage
- secure Kafka configuration guidance
- dry-run testing
- troubleshooting for both backends

## New generic watcher section

A dedicated section now explains when to choose:

- `xzepr`
- `generic`

This is important because the backends are selected by configuration but are
designed for different upstream message formats.

## Minimal generic CLI example

The guide now includes the CLI example required by the implementation plan:

- `xzatoma watch --watcher-type generic --topic plans.events --action deploy`

It explains what the command means and how it maps onto generic watcher
behavior.

## Output topic guidance

The setup guide explicitly documents both generic watcher output modes:

- same topic when `output_topic` is omitted
- separate topic when `output_topic` is configured

It also explains why same-topic operation is safe:

- generic trigger messages use `event_type: "plan"`
- generic result messages use `event_type: "result"`
- generic watcher rejects any event whose `event_type != "plan"`

## Producer payload example

The guide includes a concrete JSON `GenericPlanEvent` example showing:

- `id`
- `event_type`
- `name`
- `version`
- `action`
- `plan`
- `timestamp`
- `metadata`

This gives upstream producers a clear message shape to target.

## Environment-variable reference updates

## `docs/reference/watcher_environment_variables.md`

The environment-variable reference was expanded so it now documents both watcher
backends clearly.

The updated document includes:

- Kafka connection variables
- generic watcher configuration variables
- XZepr filter variables
- watcher logging variables
- watcher execution variables
- end-to-end usage examples

## Generic watcher variables documented

The generic watcher section now documents:

- `XZATOMA_WATCHER_TYPE`
- `XZATOMA_WATCHER_OUTPUT_TOPIC`
- `XZATOMA_WATCHER_MATCH_ACTION`
- `XZATOMA_WATCHER_MATCH_NAME`
- `XZATOMA_WATCHER_MATCH_VERSION`

For each variable, the documentation explains:

- what config field it maps to
- accepted values or semantics
- default behavior
- example usage

## Output topic behavior documented

The environment-variable reference also explains the behavior of
`XZATOMA_WATCHER_OUTPUT_TOPIC` when it is omitted:

- the generic watcher publishes results back to the input topic

The same-topic loop-prevention mechanism is documented there as well.

## Configuration reference updates

## `docs/reference/configuration.md`

The configuration reference was rewritten to include the watcher-specific fields
that were added in earlier phases and integrated in Phase 5.

The watcher documentation now covers:

- `watcher_type`
- `watcher.kafka`
- `watcher.kafka.output_topic`
- `watcher.filters`
- `watcher.generic_match`
- `watcher.logging`
- `watcher.execution`

## `watcher_type`

The configuration reference now documents:

- accepted values:
  - `xzepr`
  - `generic`
- default:
  - `xzepr`
- omitted-field behavior
- example YAML

This is the most important backend-selection field in the watcher config model.

## `kafka.output_topic`

The config reference now documents:

- that `output_topic` is used by the generic watcher result path
- that it falls back to `topic` when omitted
- that same-topic operation is safe due to `event_type` loop prevention
- YAML examples for both same-topic and separate-topic usage

## `generic_match`

The config reference now documents all supported generic matching modes with
YAML examples:

- action only
- name + version
- name + action
- name + version + action
- accept-all mode

This makes the relationship between config shape and runtime matching behavior
clear and concrete.

## Architecture documentation updates

## `docs/reference/architecture.md`

The architecture reference was rewritten to reflect the completed watcher module
structure and dispatch model.

The updated architecture document now explains:

- the top-level module layout
- the watcher module structure
- the watcher backend split
- the XZepr compatibility shim
- the watch command dispatch path
- the responsibilities of the generic and XZepr watcher subtrees

## XZepr subtree documentation

The architecture reference now describes the canonical XZepr watcher location:

- `src/watcher/xzepr/`

It also explains the role of:

- `src/xzepr/mod.rs`

as a compatibility shim for older import paths.

## Generic subtree documentation

The architecture reference now documents the generic watcher subtree and its
components:

- `src/watcher/generic/mod.rs`
- `message.rs`
- `matcher.rs`
- `producer.rs`
- `watcher.rs`

Each component’s responsibility is described so the code organization is obvious
to maintainers.

## Dispatch model documented

The architecture reference now explicitly describes the watch command dispatch
model and shows that both backends sit behind the same outer command path and
share the same external startup shape.

This is one of the most important architectural ideas of the feature:

- both watcher backends are first-class runtime options
- neither backend is a temporary special case
- the command layer selects the backend, but backend logic remains separate

## Generic watcher explanation document

## `docs/explanation/generic_watcher_phase5_integration_testing_documentation_implementation.md`

This explanation document exists to describe the completed Phase 5 work in a
single place.

Its role is to capture:

- what changed
- why the phase was needed
- how the code and docs now fit together
- what was actually implemented versus what remains future work

This follows the project rule that each feature or task should include a
corresponding explanation document in `docs/explanation/`.

## Relationship among phases

Phase 5 depends directly on the work done in the earlier phases.

## Dependency on Phase 1

The XZepr watcher had to be moved into `src/watcher/xzepr/` so dispatch could
refer to a clear canonical backend path.

## Dependency on Phase 2

The generic watcher message model had to exist so the setup guide and
configuration examples could describe real payloads instead of hypothetical
ones.

## Dependency on Phase 3

The generic watcher core had to exist so the watch command could dispatch to it
and so higher-level tests could exercise `process_event(...)`.

## Dependency on Phase 4

The watch command had to parse and validate:

- `watcher_type`
- generic match flags
- generic output topic configuration

before Phase 5 could integrate them into the real runtime path.

## Backward compatibility

Phase 5 preserves backward compatibility in several important ways.

## Default backend remains XZepr

If a config omits `watcher_type`, it still defaults to `xzepr`.

That means existing configurations continue working without modification.

## XZepr behavior remains structurally the same

Although `run_watch` now dispatches on watcher type, the XZepr path still:

- applies watcher CLI overrides
- initializes watcher logging
- constructs `XzeprWatcher`
- runs the watcher with the same `dry_run` behavior

The Phase 5 dispatch change does not alter the intended semantics of the XZepr
watch command path.

## Documentation is additive

The updated docs add generic watcher guidance without removing the XZepr watcher
path. The result is a broader, clearer watcher story without breaking existing
operator expectations.

## Quality and validation outcomes

Phase 5 work was designed to satisfy the project quality gates through:

- backend dispatch tests
- generic watcher process-event tests
- updated operator-facing docs
- example configuration coverage
- architecture documentation alignment

This phase is primarily an integration and documentation milestone, so the
critical outcome is that the system now behaves consistently from:

- config
- CLI
- runtime dispatch
- tests
- documentation

## Limitations and future work

Phase 5 completes the integration arc described by the generic watcher plan, but
it does not add new operational features beyond the approved scope.

Two natural follow-on improvements remain outside the scope of this completed
phase:

- automatic Kafka topic creation when watcher mode starts
- Kafka consumer-group override through the watch CLI

Those are operational enhancements that fit naturally into a subsequent phase
rather than being mixed into the Phase 5 integration and documentation work.

## Deliverables completed in Phase 5

The completed deliverables for this phase are:

- updated `commands::watch::run_watch` with `WatcherType` dispatch
- `config/generic_watcher.yaml`
- dispatch-oriented tests for both watcher-type startup paths
- higher-level generic watcher `process_event` integration tests
- updated setup documentation
- updated watcher environment-variable reference
- updated configuration reference
- updated architecture reference
- this explanation document

## Summary

Phase 5 completes the generic watcher feature as an integrated user-facing part
of XZatoma.

After this phase:

- `xzatoma watch` honors `watcher_type`
- both watcher backends are documented as supported runtime options
- a generic watcher example configuration is available
- dispatch behavior is tested
- generic watcher event processing is exercised at a higher level
- the architecture docs match the implemented watcher structure

This phase turns the generic watcher from a configured internal capability into
a documented, selectable, test-backed feature of the system.
