# Generic Watcher Modernization Implementation Plan

## Overview

This plan modernizes the generic Kafka watcher across five sequential phases.
The work addresses a monolithic message file, late-binding plan parsing that
delays error detection, a version predicate that uses regex instead of semver
semantics, a concrete producer with no abstraction layer that prevents unit
testing, and a Kafka consumer loop that treats all errors as fatal. Each phase
is self-contained and leaves the codebase in a compilable, tested state before
the next phase begins.

## Completion Status

| Phase | Title                                | Status   |
| ----- | ------------------------------------ | -------- |
| 1     | Module Organization                  | Complete |
| 2     | Early Plan Parsing and Event Handler | Complete |
| 3     | Matcher Version Predicate            | Complete |
| 4     | Producer Abstraction and Reliability | Complete |
| 5     | Consumer Abstraction                 | Complete |

All five phases are complete. The codebase has 210 watcher tests passing with 17
broker-dependent tests marked `#[ignore]`. All four quality gates pass:
`cargo fmt`, `cargo check`, `cargo clippy -- -D warnings`, `cargo test`.

## Current State Analysis

### Existing Infrastructure

The generic watcher is implemented across five files under
`src/watcher/generic/`:

| File          | Lines | Role                                                              |
| ------------- | ----- | ----------------------------------------------------------------- |
| `message.rs`  | ~635  | Both inbound event and outbound result types in one file          |
| `watcher.rs`  | ~915  | Full consume loop, plan extraction, execution, and result publish |
| `matcher.rs`  | ~545  | Regex-based matcher with typed `MatchMode` enum                   |
| `producer.rs` | ~555  | Concrete `GenericResultProducer` with no trait abstraction        |
| `mod.rs`      | ~65   | Module declarations, re-exports, and architecture documentation   |

The watcher integrates with the agent execution path via
`crate::commands::run::run_plan_with_options` and publishes results using
`rdkafka::producer::FutureProducer` directly through `GenericResultProducer`.
Concurrency is bounded by an `Arc<Semaphore>` keyed to
`watcher.execution.max_concurrent_executions` from the global config.

### Identified Issues

The following issues motivate this plan, listed in the order they must be
resolved:

1. `message.rs` combines `GenericPlanEvent` (inbound) and `GenericPlanResult`
   (outbound) in a single file alongside all their `impl` blocks and tests.
   These are distinct pipeline stages and the combined file obscures the data
   flow. There is no `consumer.rs` or `event_handler.rs`, so all pipeline steps
   are collapsed into `watcher.rs::process_event`, making each step untestable
   in isolation.

2. `GenericPlanEvent` stores `plan: serde_json::Value` and defers plan
   extraction to `watcher.rs::extract_plan_text`. This means a malformed plan
   payload is not detected until the executor attempts to run it, far from the
   point of ingestion. There is no plan directory resolution, so operators
   cannot supply richer on-disk plans to supplement lean inbound payloads.

3. The `version` predicate in `GenericMatcher` is compiled as a plain regex
   pattern. Regex matching does not understand version ordering, so constraints
   like `">=1.0.0"`, `"^2"`, or `"~1.2"` cannot be expressed. Operators
   intending to match a semver range must instead craft a regex that
   approximates the semantics, which is fragile and unintuitive.

4. `GenericResultProducer` is a concrete struct held as
   `Arc<GenericResultProducer>` in `GenericWatcher`. There is no trait, no fake
   implementation, and no `flush` method. Tests that exercise `process_event`
   must either construct a real Kafka producer or rely entirely on dry-run mode.
   The producer is also constructed without idempotent delivery settings,
   leaving result events vulnerable to silent loss on transient broker errors.

5. `GenericWatcher::start` builds an `rdkafka::StreamConsumer` directly and
   treats every `Err` from the message stream as an immediate fatal error that
   stops the watcher. Transient Kafka errors such as broker transport failures
   and network exceptions cause an unnecessary restart. There is no consumer
   abstraction, so the message-handling loop cannot be driven by test fixtures.

## Implementation Phases

---

### Phase 1: Module Organization

Split the monolithic `message.rs` into focused single-responsibility files and
prepare the directory structure for the event handler and consumer modules
introduced in later phases. This phase makes zero behavior changes; it only
moves types and their tests to new homes.

#### Task 1.1 Create event.rs

Create `src/watcher/generic/event.rs` and move `GenericPlanEvent`, its `impl`
block, and all associated tests from `src/watcher/generic/message.rs` into it.
The struct fields, public API, and serialization behavior must remain identical.

#### Task 1.2 Create result_event.rs

Create `src/watcher/generic/result_event.rs` and move `GenericPlanResult`, its
`impl` block, and all associated tests from `src/watcher/generic/message.rs`
into it. The struct fields, public API, and serialization behavior must remain
identical.

#### Task 1.3 Reduce message.rs to a Compatibility Shim

After Tasks 1.1 and 1.2 are complete, reduce `src/watcher/generic/message.rs` to
a re-export shim that re-exports `GenericPlanEvent` from `event` and
`GenericPlanResult` from `result_event`. Annotate both re-exports with a
`#[deprecated]` doc note pointing to the new module paths. This allows all
existing call sites in `watcher.rs` and `mod.rs` to continue compiling without
change during this phase.

#### Task 1.4 Update mod.rs

In `src/watcher/generic/mod.rs`, declare the two new submodules (`event` and
`result_event`) and add direct re-exports of `GenericPlanEvent` and
`GenericPlanResult` from their new homes. Keep the deprecated re-exports from
`message` in place until Phase 2 removes the shim.

#### Task 1.5 Testing Requirements

No new tests are written in this phase. Every test that existed in `message.rs`
travels verbatim into `event.rs` or `result_event.rs` with its owning type. The
phase is complete only when `cargo test --all-features` passes with the same
test count as before.

#### Task 1.6 Deliverables

- `src/watcher/generic/event.rs` containing `GenericPlanEvent` and its tests
- `src/watcher/generic/result_event.rs` containing `GenericPlanResult` and its
  tests
- `src/watcher/generic/message.rs` reduced to a re-export shim
- `src/watcher/generic/mod.rs` updated with new module declarations

#### Task 1.7 Success Criteria

- `cargo check --all-targets --all-features` passes with zero errors
- `cargo test --all-features` passes with the same count as before the phase
- No public symbol previously exported from `src/watcher/generic/mod.rs` is
  removed or renamed

---

### Phase 2: Early Plan Parsing and Event Handler

Parse and validate the inbound plan payload at the point of receipt rather than
during execution. Introduce `GenericEventHandler` as the central pipeline step
that converts a raw Kafka message into an executable `GenericTask`, and add plan
directory resolution so operators can supply curated on-disk plans.

#### Task 2.1 Redesign GenericPlanEvent

In `src/watcher/generic/event.rs`, change the `plan` field from
`serde_json::Value` to a parsed and validated `Plan` struct from
`crate::commands::plan_parser`. Update `GenericPlanEvent::new` to accept a raw
payload string, a `source_topic` string, and an optional `key` string. The
constructor must call `PlanParser::parse_string` on the payload and return `Err`
if parsing or plan validation fails. Replace the `timestamp` field with
`received_at: DateTime<Utc>` set to `Utc::now()` at construction time. Remove
the `id`, `event_type`, and `metadata` fields from `GenericPlanEvent` — these
concerns belong to the wire format defined in `result_event.rs` and the loop
break described below.

The `is_plan_event` method is no longer needed on `GenericPlanEvent` once the
plan is parsed eagerly; remove it.

#### Task 2.2 Add RawKafkaMessage to event.rs

Add a `RawKafkaMessage` struct to `src/watcher/generic/event.rs` with fields
`payload: String`, `topic: String`, and `key: Option<String>`. This struct is
the bridge between the raw byte stream and the parsed `GenericPlanEvent` and
will be used by the consumer abstraction introduced in Phase 5.

#### Task 2.3 Create event_handler.rs

Create `src/watcher/generic/event_handler.rs`. Define `GenericTask` with fields
`plan: Plan`, `instruction: String`, `correlation_key: Option<String>`, and
`received_at: DateTime<Utc>`. Define `GenericEventHandler` with fields
`matcher: Option<GenericEventMatcher>` and
`plan_directory: Option<std::path::PathBuf>`.

Implement `GenericEventHandler::new(matcher, plan_directory)` and
`GenericEventHandler::handle(msg: RawKafkaMessage) -> Result<Option<GenericTask>>`.
The `handle` method must perform the following steps in order:

1. Construct `GenericPlanEvent::new` from the message payload, topic, and key;
   propagate `Err` on parse or validation failure.
2. If a `GenericEventMatcher` is configured and `matcher.matches(&event)`
   returns `false`, return `Ok(None)`.
3. If `plan_directory` is `Some(dir)`, attempt to load a plan file from disk.
   Priority order: `{dir}/{plan.name}-{plan.version}.yaml` when version is
   `Some`, then `{dir}/{plan.name}.yaml`. If a file is found, parse it with
   `PlanParser::parse_file` and use it as the resolved plan; if no file is
   found, use the event-payload plan. Propagate `Err` if a file exists but
   cannot be parsed.
4. Call `resolved_plan.validate()` and propagate `Err` on failure.
5. Derive `instruction` from `resolved_plan.to_instruction()` and return
   `Ok(Some(GenericTask { ... }))`.

Log each significant step at `info` or `debug` level using structured `tracing`
fields.

#### Task 2.4 Update watcher.rs

In `src/watcher/generic/watcher.rs`, replace the inline plan extraction and
processing logic in `process_event` with a call to
`GenericEventHandler::handle`. Remove `extract_plan_text` and the
`PlanExtraction` variant from `GenericWatcherError`. Update `GenericWatcher` to
hold a `GenericEventHandler` rather than a raw `Arc<GenericMatcher>`. Update
`GenericWatcher::new` to construct the handler from config. Remove all imports
of the deprecated `message.rs` paths and switch to the direct `event` and
`result_event` module paths.

#### Task 2.5 Update mod.rs

Declare `event_handler` as a public submodule in `src/watcher/generic/mod.rs`.
Re-export `GenericEventHandler`, `GenericTask`, and `RawKafkaMessage`. Remove
the deprecated `message` re-exports. Delete `src/watcher/generic/message.rs`.

#### Task 2.6 Testing Requirements

Add unit tests to `src/watcher/generic/event.rs`:

- `test_new_valid_yaml_payload` — valid YAML sets `plan.name`, `source_topic`,
  and `key` correctly
- `test_new_valid_json_payload` — valid JSON payload parses successfully
- `test_new_invalid_payload_returns_err` — malformed payload returns `Err`
- `test_new_missing_tasks_returns_err` — plan with no tasks returns `Err`
- `test_new_received_at_is_recent` — `received_at` falls between `before` and
  `after` timestamps bracketing the call
- `test_clone_produces_independent_copy` — cloning the event does not alias the
  plan

Add unit tests to `src/watcher/generic/event_handler.rs`:

- `test_handle_valid_plan_no_matcher_no_directory` — returns `Some(task)` with
  correct plan name and non-empty instruction
- `test_handle_propagates_correlation_key` — `task.correlation_key` matches the
  message key
- `test_handle_invalid_payload_returns_err` — parse failure propagates as `Err`
- `test_handle_matcher_passes` — matcher that accepts the event returns
  `Some(task)`
- `test_handle_matcher_filters_out` — matcher that rejects returns `Ok(None)`
- `test_handle_with_plan_directory_name_only_fallback` — loads the name-only
  file when versioned file does not exist
- `test_handle_with_plan_directory_versioned_plan_takes_priority` — versioned
  file is preferred over name-only file
- `test_handle_with_plan_directory_no_file_uses_payload_plan` — falls back to
  payload plan when no disk file is found
- `test_handle_received_at_is_set` — `task.received_at` is set

#### Task 2.7 Deliverables

- Updated `src/watcher/generic/event.rs` with `RawKafkaMessage` and redesigned
  `GenericPlanEvent`
- New `src/watcher/generic/event_handler.rs` with `GenericTask` and
  `GenericEventHandler`
- Updated `src/watcher/generic/watcher.rs` using `GenericEventHandler`
- Updated `src/watcher/generic/mod.rs` declaring and re-exporting new symbols
- `src/watcher/generic/message.rs` deleted
- Tests described in Task 2.6

#### Task 2.8 Success Criteria

- All four quality gates pass: `cargo fmt --all`,
  `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-features`
- `extract_plan_text` and the `PlanExtraction` error variant no longer exist
- A payload containing a YAML plan with a missing `tasks` field causes
  `process_event` to return `Err` rather than reaching the executor

---

### Phase 3: Matcher Version Predicate Improvements

Replace the regex-based `version` predicate in `GenericMatcher` with a proper
semver constraint evaluator so operators can write human-readable version
constraints instead of regex approximations. Keep regex matching for `action`
and `name` predicates, which benefit from pattern flexibility.

#### Task 3.1 Add version_matches to watcher::mod

In `src/watcher/mod.rs`, add a public function
`version_matches(plan_version: &str, constraint: &str) -> bool`. The function
must:

1. Attempt to parse `plan_version` as a `semver::Version`; return `false` on
   parse failure.
2. Attempt to parse `constraint` as a `semver::VersionReq`; if `constraint`
   fails semver parsing, fall back to case-insensitive exact string equality
   between `plan_version` and `constraint` so that non-semver version strings
   continue to work.
3. Return `req.matches(&version)` when both parse successfully.

The `semver` crate is already available in the workspace via `Cargo.toml`; no
new dependency is needed.

#### Task 3.2 Update GenericMatcher

In `src/watcher/generic/matcher.rs`, remove
`compiled_version: Option<Arc<Regex>>` from `GenericMatcher`. Add
`version_constraint: Option<String>` in its place, storing the raw constraint
string. Remove the call to `compile_optional_pattern` for the version field in
`GenericMatcher::new`. Update `matches_version` to call
`crate::watcher::version_matches(plan_version, constraint)` instead of
performing a regex match.

Update `GenericMatcher::summary` to format the version field as
`version=<constraint>` rather than `version=/<pattern>/` to distinguish it
visually from regex predicates.

#### Task 3.3 Document Accept-All Semantics

In `src/watcher/generic/matcher.rs`, add a `has_predicates` method to
`GenericMatcher` that returns `true` when at least one of `compiled_action`,
`compiled_name`, or `version_constraint` is `Some`. Update the top-level module
doc comment in `src/watcher/generic/mod.rs` to explicitly document that a
matcher with no fields configured accepts all `"plan"` events, so operators
understand the implications of an empty match config.

#### Task 3.4 Testing Requirements

Add unit tests to `src/watcher/generic/matcher.rs`:

- `test_version_constraint_gte_matches` — `">=1.0.0"` accepts `"1.2.0"`
- `test_version_constraint_gte_rejects` — `">=1.0.0"` rejects `"0.9.0"`
- `test_version_constraint_caret_matches` — `"^2"` accepts `"2.5.1"`
- `test_version_constraint_caret_rejects` — `"^2"` rejects `"3.0.0"`
- `test_version_exact_string_fallback` — non-semver constraint falls back to
  exact string equality
- `test_version_required_but_plan_version_is_none` — returns `false`
- `test_has_predicates_empty` — returns `false` for default config
- `test_has_predicates_version_only` — returns `true` when only version is set

Add unit tests to `src/watcher/mod.rs` (or a dedicated test module):

- `test_version_matches_exact` — exact semver string
- `test_version_matches_gte_range` — `>=` constraint
- `test_version_matches_caret_range` — `^` constraint
- `test_version_matches_invalid_version_returns_false` — malformed plan version
- `test_version_matches_invalid_constraint_falls_back_to_string_equality`

#### Task 3.5 Deliverables

- Updated `src/watcher/generic/matcher.rs` with semver version matching and
  `has_predicates`
- New `version_matches` function in `src/watcher/mod.rs`
- Updated `src/watcher/generic/mod.rs` with accept-all semantics documented
- Tests described in Task 3.4

#### Task 3.6 Success Criteria

- All four quality gates pass
- A `GenericMatcher` configured with `version: Some(">=2.0.0".to_string())`
  correctly rejects a plan event with `version = "1.9.0"` and accepts one with
  `version = "2.1.0"`
- The `regex` crate is no longer used for the version predicate anywhere in
  `matcher.rs`

---

### Phase 4: Producer Abstraction and Reliability

Introduce a `ResultProducerTrait` to decouple `GenericWatcher` from the concrete
Kafka producer, add a fake implementation for unit testing, add a buffered
dead-letter queue wrapper for resilience against transient broker failures, and
enable idempotent delivery settings on the production producer.

#### Task 4.1 Create result_producer.rs

Create `src/watcher/generic/result_producer.rs`. Define `ResultProducerTrait` as
a `Send + Sync` trait with two methods:

- `async fn publish(&self, result: &GenericPlanResult) -> Result<()>`
- `async fn flush(&self, timeout: std::time::Duration) -> Result<()>`

Both methods must be `async`. Document each with `///` doc comments including
`# Errors` and `# Examples` sections.

#### Task 4.2 Move GenericResultProducer

Move `GenericResultProducer` from `src/watcher/generic/producer.rs` into
`src/watcher/generic/result_producer.rs` and implement `ResultProducerTrait` for
it. Add a `flush` method that calls `self.producer.flush(timeout)`. Enable
idempotent delivery by adding `acks=all`, `retries=5`,
`compression.type=snappy`, and `enable.idempotence=true` to the
`rdkafka::ClientConfig` inside `GenericResultProducer::new`.

Reduce `src/watcher/generic/producer.rs` to a re-export shim for backward
compatibility during this phase. It will be deleted in Task 4.6.

#### Task 4.3 Add FakeResultProducer

In `src/watcher/generic/result_producer.rs`, define `FakeResultProducer` with a
`published: tokio::sync::Mutex<Vec<GenericPlanResult>>` field. Implement
`ResultProducerTrait` for it so that `publish` appends the event to the internal
vector and `flush` is a no-op. Add a `published_events` method that returns a
clone of the accumulated vector. This type must be declared `pub` (not
`#[cfg(test)]`) so integration tests in `tests/` can use it. Derive or manually
implement `Debug`.

#### Task 4.4 Add BufferedResultProducer

In `src/watcher/generic/result_producer.rs`, define `BufferedResultProducer`
with fields `inner: Arc<dyn ResultProducerTrait>`,
`buffer: tokio::sync::Mutex<std::collections::VecDeque<GenericPlanResult>>`, and
`max_buffered: usize`. Add a public constant `DEFAULT_DLQ_MAX_BUFFERED: usize`
set to 100.

Implement `ResultProducerTrait` for `BufferedResultProducer`:

- `publish`: first attempt to drain any buffered events by trying to publish
  them through `inner`; if draining succeeds, publish the new event through
  `inner` normally; if `inner.publish` fails for the new event, push it to the
  buffer; if the buffer is at capacity, drop the oldest entry and emit a `warn!`
  log.
- `flush`: delegate to `inner.flush` after attempting to drain the buffer.

Add a `pending_count` method that returns the current buffer length.

#### Task 4.5 Update GenericWatcher

In `src/watcher/generic/watcher.rs`, change the `producer` field from
`Arc<GenericResultProducer>` to `Arc<dyn ResultProducerTrait>`. Update
`GenericWatcher::new` to construct a `GenericResultProducer`, wrap it in `Arc`,
and assign it. All calls to `self.producer.publish` and any future calls to
`self.producer.flush` automatically go through the trait.

#### Task 4.6 Update mod.rs and Delete producer.rs

In `src/watcher/generic/mod.rs`, replace the `producer` submodule declaration
with `result_producer`. Re-export `ResultProducerTrait`, `FakeResultProducer`,
`BufferedResultProducer`, and `DEFAULT_DLQ_MAX_BUFFERED` from `result_producer`.
Delete `src/watcher/generic/producer.rs`.

#### Task 4.7 Testing Requirements

Add unit tests to `src/watcher/generic/result_producer.rs`:

- `test_fake_producer_new_is_empty` — `published_events()` is empty on
  construction
- `test_fake_producer_records_single_event` — one publish results in one
  recorded event
- `test_fake_producer_records_multiple_events_in_order` — events recorded in
  insertion order
- `test_fake_producer_flush_is_noop` — `flush` does not panic and returns `Ok`
- `test_trait_object_dispatch` — a `&dyn ResultProducerTrait` reference to a
  `FakeResultProducer` calls `publish` correctly
- `test_buffered_publish_success_leaves_buffer_empty` — successful publish keeps
  buffer empty
- `test_buffered_publish_buffers_on_broker_failure` — failed publish adds to
  buffer
- `test_buffered_multiple_failures_accumulate` — consecutive failures grow the
  buffer
- `test_buffered_drains_buffer_when_broker_recovers` — next successful publish
  drains buffered events in order
- `test_buffered_drops_oldest_when_buffer_full` — when buffer is at
  `max_buffered`, the oldest entry is dropped on the next failure

#### Task 4.8 Deliverables

- New `src/watcher/generic/result_producer.rs` with `ResultProducerTrait`,
  `GenericResultProducer` (with idempotent delivery settings),
  `FakeResultProducer`, and `BufferedResultProducer`
- Updated `src/watcher/generic/watcher.rs` using `Arc<dyn ResultProducerTrait>`
- Updated `src/watcher/generic/mod.rs`
- `src/watcher/generic/producer.rs` deleted
- Tests described in Task 4.7

#### Task 4.9 Success Criteria

- All four quality gates pass
- `GenericResultProducer` is constructed with `acks=all` and
  `enable.idempotence=true` (verifiable via `get_kafka_config`)
- `test_buffered_drains_buffer_when_broker_recovers` passes without a live Kafka
  connection by using a `ControlledProducer` test double
- `GenericWatcher` holds `Arc<dyn ResultProducerTrait>`, not
  `Arc<GenericResultProducer>`

---

### Phase 5: Consumer Abstraction

Introduce a `GenericConsumerTrait` to decouple the Kafka message loop from the
`rdkafka` implementation, add a `FakeGenericConsumer` for driving the loop in
tests, and teach the consumer loop to distinguish transient Kafka errors from
fatal ones so the watcher survives brief broker disruptions.

#### Task 5.1 Create consumer.rs

Create `src/watcher/generic/consumer.rs`. Define `GenericConsumerTrait` as a
`Send + Sync` trait with two methods:

- `async fn next(&mut self) -> Option<Result<RawKafkaMessage>>` — returns the
  next available message, `None` when the stream is exhausted, and
  `Some(Err(...))` for consumer-level errors
- `async fn commit(&mut self) -> Result<()>` — commits the most recently
  returned offset

Both methods must be `async` and have `///` doc comments with `# Errors` and
`# Examples` sections.

#### Task 5.2 Add is_transient_kafka_recv_error

In `src/watcher/generic/consumer.rs`, add a private function
`is_transient_kafka_recv_error(err: &rdkafka::error::KafkaError) -> bool`. The
function must return `true` for errors whose string representation contains any
of the substrings: `"BrokerTransportFailure"`, `"AllBrokersDown"`, or
`"NetworkException"`. Return `false` for all other error strings. This function
must be tested in isolation.

#### Task 5.3 Add RealGenericConsumer

In `src/watcher/generic/consumer.rs`, define `RealGenericConsumer` wrapping
`rdkafka::consumer::StreamConsumer` with fields `inner: StreamConsumer` and
`pending_commit: Option<RawKafkaMessage>`. Implement `GenericConsumerTrait`:

- `next`: call `self.inner.stream().next()` using the message stream. On
  `Some(Ok(msg))`, extract the UTF-8 payload, construct a `RawKafkaMessage`,
  store it in `pending_commit`, and return `Some(Ok(...))`. On `Some(Err(e))`,
  check `is_transient_kafka_recv_error(&e)`: if transient, log a `warn!` and
  return `Some(Ok(/* dummy skip */))` so the loop continues; if fatal, return
  `Some(Err(...))`. On `None`, return `None`.
- `commit`: if `pending_commit` is `Some`, call `self.inner.commit_message(...)`
  and clear `pending_commit`.

Implement `std::fmt::Debug` for `RealGenericConsumer` manually (the inner
`StreamConsumer` does not implement `Debug`).

#### Task 5.4 Add FakeGenericConsumer

In `src/watcher/generic/consumer.rs`, define `FakeGenericConsumer` with fields
`queue: std::collections::VecDeque<Result<RawKafkaMessage>>` and
`commits: usize`. Implement `GenericConsumerTrait`:

- `next`: pop the front item; return `None` when the queue is empty.
- `commit`: increment `self.commits`.

Add a `new(items: Vec<Result<RawKafkaMessage>>) -> Self` constructor. Add
`commits_recorded(&self) -> usize`, `is_empty(&self) -> bool`, and
`len(&self) -> usize` helper methods. This type must be declared `pub` (not
`#[cfg(test)]`).

#### Task 5.5 Update GenericWatcher

In `src/watcher/generic/watcher.rs`, replace the inline `rdkafka::ClientConfig`
construction and `StreamConsumer` creation in `start` with a helper method
`build_consumer() -> Result<impl GenericConsumerTrait>`. Change the `start`
signature to accept an optional pre-built consumer
(`Option<Box<dyn GenericConsumerTrait>>`), defaulting to `build_consumer()` when
`None` is passed. This allows test code to inject a `FakeGenericConsumer`
without spawning a real Kafka connection.

Update the message loop body to call `consumer.next().await` instead of
iterating the `rdkafka` stream directly. On a transient `Err` from
`consumer.next()`, log at `warn` level and continue without stopping. On a fatal
`Err`, set `self.running` to `false` and return `Err`. Call
`consumer.commit().await` after each successfully processed message.

#### Task 5.6 Update mod.rs

In `src/watcher/generic/mod.rs`, declare the `consumer` submodule as public.
Re-export `GenericConsumerTrait`, `RealGenericConsumer`, `FakeGenericConsumer`,
and `RawKafkaMessage` from `consumer`. Remove the re-export of `RawKafkaMessage`
from `event.rs` if it was placed there in Phase 2 (it belongs in `consumer.rs`
as the boundary type between the raw stream and the parsed event).

#### Task 5.7 Testing Requirements

Add unit tests to `src/watcher/generic/consumer.rs`:

- `test_new_empty_consumer_is_empty` — empty queue reports `is_empty() == true`
- `test_new_consumer_with_messages_reports_len` — correct length reported
- `test_next_returns_messages_in_order` — messages returned in insertion order
- `test_next_exhausted_returns_none` — returns `None` after all items consumed
- `test_next_returns_error_items` — `Err` items are returned as `Some(Err(...))`
- `test_commit_increments_counter` — one commit increments `commits_recorded`
- `test_commit_increments_each_call` — three commits gives count of three
- `test_trait_object_dispatch` — works through `&mut dyn GenericConsumerTrait`
- `test_is_transient_broker_transport_failure` — returns `true`
- `test_is_transient_all_brokers_down` — returns `true`
- `test_is_transient_network_exception` — returns `true`
- `test_is_not_transient_unknown_topic` — returns `false`
- `test_is_not_transient_non_message_error` — returns `false`

Add integration-style unit tests to `src/watcher/generic/watcher.rs` using
`FakeGenericConsumer` and `FakeResultProducer`:

- `test_watcher_processes_matching_event_via_fake_consumer` — a single matching
  event results in one published `GenericPlanResult`
- `test_watcher_discards_non_plan_event` — an event with `event_type != "plan"`
  is skipped and produces `MessageDisposition::SkippedNonPlanEvent`
- `test_watcher_skips_non_matching_event` — an event that does not satisfy the
  matcher produces `MessageDisposition::SkippedNoMatch`
- `test_watcher_invalid_json_produces_invalid_payload` — malformed JSON returns
  `MessageDisposition::InvalidPayload`
- `test_watcher_commit_called_after_each_message` — `commits_recorded` equals
  the number of messages processed

#### Task 5.8 Deliverables

- New `src/watcher/generic/consumer.rs` with `GenericConsumerTrait`,
  `RealGenericConsumer`, `FakeGenericConsumer`, and
  `is_transient_kafka_recv_error`
- Updated `src/watcher/generic/watcher.rs` using `GenericConsumerTrait`
- Updated `src/watcher/generic/mod.rs`
- Updated `docs/explanation/generic_watcher_modernization_implementation.md`
  with completion notes for all five phases
- Tests described in Task 5.7

#### Task 5.9 Success Criteria

- All four quality gates pass
- `GenericWatcher::start` no longer directly imports or constructs any `rdkafka`
  type; all Kafka interaction is behind `GenericConsumerTrait`
- The five watcher loop tests in Task 5.7 pass without a live Kafka connection
- `cargo clippy --all-targets --all-features -- -D warnings` passes with no
  suppression attributes added to suppress legitimate warnings

### Phase 5 Completion Notes

Completed. New module `src/watcher/generic/consumer.rs` contains
`RawKafkaMessage` (relocated from `event.rs`), `GenericConsumerTrait`,
`RealGenericConsumer`, `FakeGenericConsumer`, and
`is_transient_kafka_recv_error`. `GenericWatcher::start` now accepts
`Option<Box<dyn GenericConsumerTrait>>` and no longer constructs any rdkafka
type directly. All five Task 5.7 watcher loop tests pass without a live Kafka
connection. Implementation summary:
`docs/explanation/generic_watcher_phase5_consumer_abstraction.md`.
