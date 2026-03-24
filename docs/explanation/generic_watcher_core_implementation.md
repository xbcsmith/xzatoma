# Generic watcher core implementation

## Overview

This document explains the implementation of Phase 3: Generic Kafka Watcher
Core.

The goal of this phase is to add a new generic watcher backend that can:

- consume generic plan events from Kafka-compatible topics
- evaluate each event with flexible regex-based matching
- prevent same-topic input/output loops with an unconditional event type gate
- publish structured result messages through a stub-first producer interface
- support dry-run processing with unit-testable behavior

This implementation keeps the generic watcher independent from the XZepr
watcher. Although both backends have conceptually similar responsibilities, they
operate on different wire formats and have separate matching logic by design.

## Implemented modules

The generic watcher core is implemented under:

```text
src/watcher/generic/
├── mod.rs
├── message.rs
├── matcher.rs
├── producer.rs
└── watcher.rs
```

### `message.rs`

This file defines the Phase 2 message schemas used by the generic watcher:

- `GenericPlanEvent`
- `GenericPlanResult`

`GenericPlanEvent` is the incoming trigger message. It contains:

- `id`
- `event_type`
- optional `name`
- optional `version`
- optional `action`
- `plan`
- optional `timestamp`
- optional `metadata`

`GenericPlanResult` is the outgoing result message. It contains:

- `id`
- `event_type`
- `trigger_event_id`
- `success`
- `summary`
- `timestamp`
- optional `plan_output`

The key loop-prevention guarantee is already encoded in these types:

- trigger messages must use `event_type = "plan"`
- result messages always use `event_type = "result"`

That means if input and output are the same topic, the watcher can safely read
its own result messages and discard them.

### `matcher.rs`

This file adds `GenericMatcher`, which evaluates a `GenericPlanEvent` against
`GenericMatchConfig`.

Important characteristics of this implementation:

- it is completely separate from the XZepr `EventFilter`
- it does not introduce a shared matcher trait
- it compiles configured regex patterns eagerly at construction time
- it stores compiled regex values in `Arc<Regex>`
- it applies case-insensitive matching by default
- it rejects any event where `event_type != "plan"` before doing anything else

The matcher supports the planned modes:

- action only
- name + version
- name + action
- name + version + action
- accept-all when no fields are configured

It also handles missing required event fields correctly. For example, if the
config requires `action` but the event has `action = None`, the event does not
match.

Regex compilation uses a helper that prepends `(?i)` unless the pattern already
starts with inline flags. The required comment is included in the pattern
compilation path:

```text
// Now we have 2 problems.
```

The matcher also exposes `summary() -> String` so startup logging can describe
the active mode and configured patterns.

### `producer.rs`

This file adds `GenericResultProducer`.

The producer follows the same stub-first style used elsewhere in the watcher
code:

- it exposes a full producer interface now
- it assembles Kafka configuration into key/value settings
- it does not require a concrete Kafka client dependency
- it logs serialized output so behavior is visible in dry-run mode and tests

The producer resolves the output topic this way:

1. use `KafkaWatcherConfig::output_topic` if configured
2. otherwise fall back to `KafkaWatcherConfig::topic`

It exposes:

- `new(&KafkaWatcherConfig) -> Result<Self>`
- `output_topic() -> &str`
- `input_topic() -> &str`
- `get_kafka_config() -> Vec<(String, String)>`
- `publish(&self, result: &GenericPlanResult) -> Result<()>`

Security configuration is translated into Kafka settings using the existing
Kafka security model already present in the project. Invalid protocols or SASL
settings fail during construction rather than later at publish time.

### `watcher.rs`

This file adds `GenericWatcher`, the core service type for generic event
handling.

The constructor:

- validates that Kafka watcher config exists
- builds `GenericMatcher` from `config.watcher.generic_match`
- builds `GenericResultProducer` from watcher Kafka config
- creates an execution semaphore from watcher execution settings

The watcher exposes:

- `new(config: Config, dry_run: bool) -> Result<Self>`
- `start(&mut self) -> Result<()>`
- `process_payload(&self, payload: &str) -> Result<MessageDisposition>`
- `process_event(&self, event: GenericPlanEvent) -> Result<MessageDisposition>`
- `matcher_summary(&self) -> String`
- `output_topic(&self) -> &str`
- `published_results(&self) -> Vec<GenericPlanResult>`
- `get_kafka_config(&self) -> Vec<(String, String)>`
- `extract_plan_text(plan: &serde_json::Value) -> Result<String>`

The `start()` method matches the expected watcher interface shape, but currently
operates in stub mode rather than introducing a real Kafka consumer dependency.

The actual unit-testable processing path is `process_payload()` and
`process_event()`.

## Message handling flow

The generic watcher processing flow is:

1. deserialize raw JSON into `GenericPlanEvent`
2. enforce event type gate
3. evaluate configured matcher rules
4. normalize the `plan` payload into text
5. in dry-run mode, skip execution but still create a result
6. in non-dry-run mode, build a synthetic execution result
7. publish the result through the stub producer
8. record published results for unit testing

### Event type gate

The unconditional event type gate is the most important behavioral rule in this
phase.

Any event where:

- `event_type == "result"`
- `event_type == ""`
- `event_type == "unknown"`
- `event_type == "PLAN"`

is rejected before normal matching logic is applied.

This provides the same-topic loop breaker required by the plan. Even if a result
message otherwise resembles a plan event, it is still discarded because the gate
runs first and cannot be overridden by configuration.

### Plan extraction

The generic watcher does not use multi-strategy extraction. The plan is read
directly from `GenericPlanEvent::plan`.

Normalization rules are:

- string plan: returned as-is
- object plan: serialized as pretty JSON text
- array plan: serialized as pretty JSON text
- null plan: error
- bool/number plan: serialized as pretty JSON text

This keeps the generic schema simple and deterministic.

## Configuration additions required for Phase 3

Phase 3 depends on config fields that were described in a later phase of the
plan but are necessary for this implementation to compile and function
correctly.

The following schema additions were introduced as prerequisites:

### `GenericMatchConfig`

Added to `src/config.rs`:

- `action: Option<String>`
- `name: Option<String>`
- `version: Option<String>`

This type is used by `GenericMatcher`.

### `WatcherConfig::generic_match`

Added to `WatcherConfig` so the generic watcher can obtain match rules from the
main configuration model.

### `KafkaWatcherConfig::output_topic`

Added as:

- `output_topic: Option<String>`

This allows the result producer to resolve an explicit output topic or fall back
to the input topic.

### Validation updates

Validation was extended to ensure:

- `watcher.kafka.brokers` is not empty
- `watcher.kafka.topic` is not empty
- `watcher.kafka.group_id` is not empty
- `watcher.kafka.output_topic` is not empty when present

The environment-based watcher Kafka population path was also updated so newly
constructed `KafkaWatcherConfig` values include `output_topic: None`.

## Dry-run behavior

Dry-run mode is intentionally first-class in this phase.

When `dry_run = true`:

- matching plan events are still parsed and matched
- the plan is normalized
- no execution is performed
- a successful `GenericPlanResult` is still produced
- the result is published through the stub producer
- the result summary explicitly indicates dry-run handling

This makes the generic watcher observable and testable before a concrete runtime
execution path is shared between watcher backends.

## Testing coverage

Unit tests were added for the major behaviors required by the plan.

### `GenericMatcher` tests

Coverage includes:

- action-only mode
- name + version mode
- name + action mode
- name + version + action mode
- accept-all mode
- unconditional rejection of `event_type = "result"`
- unconditional rejection of non-`"plan"` event types
- regex matching such as `deploy.*`
- case-insensitive matching
- invalid regex rejection during construction
- rejection when a required event field is missing
- summary output content

### `GenericPlanEvent` and plan extraction tests

Coverage includes:

- string plan handling
- object plan handling
- array plan handling
- error on null/missing plan payload

The message module already included broad serialization and round-trip coverage,
and the watcher module adds direct plan extraction tests.

### `GenericResultProducer` tests

Coverage includes:

- fallback to input topic when `output_topic` is not configured
- explicit `output_topic` override
- Kafka config generation
- SASL config propagation
- invalid protocol handling
- missing SASL credential handling
- successful stub publish

### `GenericWatcher` dry-run tests

Coverage includes:

- matching `GenericPlanEvent` is processed in dry-run mode
- `event_type = "result"` is silently discarded
- non-matching event is skipped
- invalid JSON payload is classified as invalid
- watcher Kafka config assembly
- missing Kafka config error
- output topic resolution

## Separation from XZepr watcher

A key architectural requirement was to keep the generic watcher completely
separate from the XZepr watcher.

That separation is preserved in several ways:

- `GenericMatcher` is independent from `EventFilter`
- generic message types are distinct from XZepr CloudEvents types
- no shared matcher abstraction was introduced
- the generic producer is separate from XZepr consumer/client code
- generic watcher behavior is implemented under `src/watcher/generic/`

This keeps both backends as equal peers rather than forcing an artificial shared
abstraction too early.

## Current limitations

This phase intentionally uses a stub-first implementation model.

That means:

- no real Kafka producer client has been introduced yet
- `start()` is a stub startup path rather than a long-running real consumer loop
- non-dry-run execution currently records a synthetic success result instead of
  invoking a shared watcher execution entry point

This was done to stay aligned with the repository’s existing stub-first watcher
patterns and to avoid inventing a new execution abstraction that does not yet
exist in the codebase.

The control-flow contract, matcher behavior, loop prevention, output topic
resolution, and unit-testable processing path are all implemented now, and the
runtime integration pieces can be extended later without changing the public
shape of the generic watcher core.

## Deliverables completed in this phase

This phase adds or completes:

- `src/watcher/generic/matcher.rs`
- `src/watcher/generic/producer.rs`
- `src/watcher/generic/watcher.rs`
- `src/watcher/generic/mod.rs` exports
- prerequisite config schema updates needed by Phase 3
- unit tests for matcher, producer, watcher, and plan extraction behavior
- this implementation explanation document

## Summary

Phase 3 establishes the generic watcher backend’s core structure and behavior.

The result is a generic watcher core that:

- accepts generic plan-event messages
- applies regex-based matching with eager validation
- rejects all non-`"plan"` events unconditionally
- normalizes embedded plan payloads directly
- publishes structured result messages to a resolved output topic
- supports dry-run processing with observable output
- remains separate from the XZepr watcher architecture

This provides a stable foundation for the later configuration, CLI, dispatch,
and documentation work described in the remaining phases of the generic watcher
plan.
