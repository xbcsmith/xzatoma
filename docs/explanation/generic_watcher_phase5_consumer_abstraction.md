# Generic Watcher Phase 5: Consumer Abstraction

## Overview

Phase 5 introduces `GenericConsumerTrait`, decoupling the watcher message loop
from `rdkafka` so the loop can be driven by any conforming consumer
implementation. Two implementations are provided:

- `RealGenericConsumer`: wraps `rdkafka::consumer::StreamConsumer` for
  production use
- `FakeGenericConsumer`: an in-memory queue consumer for offline testing

The `RawKafkaMessage` boundary type was relocated from `event.rs` to the new
`consumer.rs` module where it logically belongs as the bridge between the raw
Kafka byte stream and the event-handling pipeline.

## Deliverables

### New file: `src/watcher/generic/consumer.rs`

Contains:

- `RawKafkaMessage` — UTF-8 payload, source topic, optional message key
- `GenericConsumerTrait` — async trait with `next` and `commit` methods
- `RealGenericConsumer` — wraps `rdkafka::consumer::StreamConsumer`; handles
  transient errors internally; performs explicit offset commits via
  `TopicPartitionList`
- `FakeGenericConsumer` — `VecDeque`-backed in-memory consumer; exposes
  `commits_recorded()` and `commit_counter()` for test assertions
- `is_transient_kafka_recv_error` — private helper classifying Kafka errors by
  substring (`BrokerTransportFailure`, `AllBrokersDown`, `NetworkException`)

### Updated: `src/watcher/generic/watcher.rs`

- `start` signature changed from `start(&mut self)` to
  `start(&mut self, consumer_override: Option<Box<dyn GenericConsumerTrait>>)`.
  Passing `None` builds a `RealGenericConsumer` via `build_consumer()`. Passing
  `Some(consumer)` injects any `GenericConsumerTrait` implementation.
- `build_consumer()` private method: delegates to
  `RealGenericConsumer::from_config` with the watcher's Kafka settings and
  `enable.auto.commit=false`.
- `with_producer(producer)` builder: replaces the internal producer, enabling
  `FakeResultProducer` injection for offline tests.
- `GenericWatcher::start` no longer imports or constructs any `rdkafka` type
  directly; all Kafka interaction is behind `GenericConsumerTrait`.
- Five new integration tests run without a live Kafka connection.

### Updated: `src/watcher/generic/event.rs`

- `RawKafkaMessage` struct definition removed; the module doc updated to
  redirect readers to `consumer.rs`.
- Test module imports `RawKafkaMessage` from
  `crate::watcher::generic::consumer`.

### Updated: `src/watcher/generic/event_handler.rs`

- Import changed from `event::{GenericPlanEvent, RawKafkaMessage}` to separate
  imports: `consumer::RawKafkaMessage` and `event::GenericPlanEvent`.
- Doc example paths updated from `event::RawKafkaMessage` to
  `watcher::generic::RawKafkaMessage` (via the `mod.rs` re-export).

### Updated: `src/watcher/generic/mod.rs`

- Added `pub mod consumer;` declaration.
- `RawKafkaMessage` re-exported from `consumer` instead of `event`.
- New re-exports: `GenericConsumerTrait`, `RealGenericConsumer`,
  `FakeGenericConsumer`.

### Updated: `src/commands/mod.rs`

- `watcher.start()` in the `WatcherType::Generic` arm updated to
  `watcher.start(None)`.

## Design Decisions

### Transient error handling in the consumer, not the loop

`RealGenericConsumer::next` classifies `KafkaError` values using
`is_transient_kafka_recv_error` and converts transient errors into a dummy-skip
`Some(Ok(...))`. This keeps the watcher loop simple: it sees only
`Some(Ok(msg))` for valid or skip messages, `Some(Err(...))` for fatal errors,
and `None` when the stream ends.

### Arc<AtomicUsize> for FakeGenericConsumer commit counter

`FakeGenericConsumer` stores its commit counter behind an `Arc<AtomicUsize>`
rather than a plain `usize`. This allows a handle to the counter to be cloned
via `commit_counter()` before the consumer is moved into `start()`, enabling the
caller to verify commit counts after the loop terminates.

### RealGenericConsumer::from_config and the build_consumer indirection

`RealGenericConsumer::from_config` keeps all `rdkafka::ClientConfig`
construction inside `consumer.rs`, so `watcher.rs` imports no rdkafka types.
`build_consumer()` in `watcher.rs` is a thin wrapper that passes
`get_kafka_config()` output to `from_config`.

### Manual offset commit

`from_config` sets `enable.auto.commit=false`. Offsets are advanced only by an
explicit `commit()` call, giving the watcher loop fine-grained control: it
commits after each successfully received message regardless of the resulting
`MessageDisposition`.

## Test Coverage

### `consumer.rs` tests (13 tests, no broker required)

- Empty consumer helpers: `test_new_empty_consumer_is_empty`,
  `test_new_consumer_with_messages_reports_len`
- FIFO ordering: `test_next_returns_messages_in_order`
- Exhaustion: `test_next_exhausted_returns_none`
- Error passthrough: `test_next_returns_error_items`
- Commit counter: `test_commit_increments_counter`,
  `test_commit_increments_each_call`
- Dynamic dispatch: `test_trait_object_dispatch`
- Transient classifier: `test_is_transient_broker_transport_failure`,
  `test_is_transient_all_brokers_down`, `test_is_transient_network_exception`,
  `test_is_not_transient_unknown_topic`,
  `test_is_not_transient_non_message_error`

### `watcher.rs` integration tests (5 tests, no broker required)

- `test_watcher_processes_matching_event_via_fake_consumer`: one matching event
  in `FakeGenericConsumer` produces one `GenericPlanResult` via
  `FakeResultProducer`.
- `test_watcher_discards_non_plan_event`: a result-JSON payload that fails plan
  parsing produces `MessageDisposition::InvalidPayload` and publishes no result.
- `test_watcher_skips_non_matching_event`: a valid plan that does not satisfy
  the matcher produces `MessageDisposition::SkippedNoMatch`.
- `test_watcher_invalid_json_produces_invalid_payload`: malformed input returns
  `MessageDisposition::InvalidPayload`.
- `test_watcher_commit_called_after_each_message`: three messages (matching,
  non-matching, invalid) each receive a commit, verified via
  `FakeGenericConsumer::commit_counter()`.

## Success Criteria Verification

- All four quality gates pass: `cargo fmt`, `cargo check`, `cargo clippy`,
  `cargo test`.
- `GenericWatcher::start` imports no rdkafka types directly.
- All five watcher loop tests pass without a live Kafka connection.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with no
  suppression attributes added.
- 210 watcher tests pass; 17 ignored (broker-dependent only).
