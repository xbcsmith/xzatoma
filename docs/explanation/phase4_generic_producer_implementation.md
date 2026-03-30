# Phase 4.5: GenericResultProducer FutureProducer Implementation

## Summary

Replaced the log-only stub `publish()` method in `GenericResultProducer` with a
real `rdkafka::producer::FutureProducer` implementation. The producer now
serializes `GenericPlanResult` messages to JSON and sends them to the configured
Kafka output topic.

## Changes

### File Modified

- `src/watcher/generic/producer.rs`

### Structural Changes

1. **Added `producer: FutureProducer` field** to `GenericResultProducer`. The
   producer is created eagerly in `new()` from the fully assembled Kafka
   configuration.

2. **Removed `#[derive(Debug, Clone)]`** from the struct. `FutureProducer` does
   not implement `Clone` or `Debug`. Since `GenericResultProducer` is wrapped in
   `Arc<GenericResultProducer>` inside `GenericWatcher`, removing `Clone` has no
   impact. A manual `fmt::Debug` implementation was added that renders the
   producer field as `<FutureProducer>`.

3. **Refactored `new()` constructor** to avoid a two-phase initialization
   pattern. Security configuration (protocol, SASL, SSL) is parsed into local
   variables first, then the `FutureProducer` is built from a `ClientConfig`
   populated with the full set of key-value pairs, and finally `Self` is
   constructed in a single expression. The former `apply_security_config()`
   private method was inlined into `new()` since it was the only call site and
   the local-variable approach is cleaner.

4. **Replaced stub `publish()` with real Kafka send**:
   - Serializes `result` to JSON (unchanged from stub).
   - Constructs a `FutureRecord` with the output topic, JSON payload, and
     `trigger_event_id` as the message key for deterministic partitioning.
   - Calls `self.producer.send(record, self.request_timeout).await`.
   - Maps delivery errors to `XzatomaError::Watcher(...)`.
   - Retains the existing `tracing::info!` log after successful publish so
     results remain observable. Removed "stub mode" from the log message.

### New Imports

- `rdkafka::producer::{FutureProducer, FutureRecord}`
- `rdkafka::ClientConfig`
- `std::fmt`

### Removed Imports

- `crate::config::KafkaSecurityConfig` moved from module-level to `#[cfg(test)]`
  scope (only used by tests).

## Configuration Assembly

The `FutureProducer` is created from the same key-value pairs that
`get_kafka_config()` returns. The `new()` constructor builds the `ClientConfig`
inline using the local variables rather than calling `get_kafka_config()` on
`&self`, because the struct is not yet constructed at that point. The
`get_kafka_config()` public method remains available for introspection and
testing.

Key-value pairs set on the producer:

| Key                        | Source                         |
| -------------------------- | ------------------------------ |
| `bootstrap.servers`        | `config.brokers`               |
| `client.id`                | Hardcoded identifier           |
| `security.protocol`        | Parsed from security config    |
| `message.timeout.ms`       | `request_timeout` (30 seconds) |
| `sasl.mechanism`           | Optional SASL config           |
| `sasl.username`            | Optional SASL config           |
| `sasl.password`            | Optional SASL config           |
| `ssl.ca.location`          | Optional SSL config            |
| `ssl.certificate.location` | Optional SSL config            |
| `ssl.key.location`         | Optional SSL config            |

## Error Handling

- `new()` returns `XzatomaError::Watcher` if `ClientConfig::create()` fails (for
  example, due to invalid security settings that `rdkafka` rejects at the native
  library level).
- `publish()` returns `XzatomaError::Watcher` if JSON serialization fails or if
  the `FutureProducer::send()` call returns a Kafka error (broker unreachable,
  topic does not exist, message too large, timeout, and so on).

## Test Impact

### Producer Tests (in `producer.rs`)

All 8 non-publish unit tests continue to pass without modification:

- `test_generic_result_producer_uses_input_topic_when_output_topic_not_set`
- `test_generic_result_producer_uses_explicit_output_topic_when_configured`
- `test_generic_result_producer_get_kafka_config_includes_basic_settings`
- `test_generic_result_producer_get_kafka_config_includes_sasl_settings`
- `test_generic_result_producer_new_returns_error_for_invalid_protocol`
- `test_generic_result_producer_new_returns_error_for_missing_sasl_username`
- `test_generic_result_producer_new_returns_error_for_missing_sasl_password`
- `test_generic_result_producer_debug_impl` (new test for manual `Debug` impl)

The publish integration test was marked `#[ignore]` because it requires a
running Kafka broker:

- `test_generic_result_producer_publish_succeeds`

All 3 doc-tests pass (struct example, `new()` example, `get_kafka_config()`
example). Creating a `FutureProducer` via `ClientConfig::create()` does not
require a live broker; the connection is established lazily when `send()` is
called.

### Watcher Tests (in `watcher.rs`) -- Follow-Up Required

Two tests in `watcher.rs` route through `process_event()` which calls
`publish()`. These now attempt a real Kafka send and will fail (after the
30-second `message.timeout.ms`) when no broker is available:

- `test_generic_watcher_dry_run_processes_matching_event`
- `test_generic_watcher_process_event_matching_action_is_processed`

These tests were not modified per the constraint that only `producer.rs` should
be changed. They should be marked `#[ignore]` in a follow-up change, or the
watcher should be refactored to accept a trait-based producer so tests can
inject a mock.

All other watcher tests (those that test non-matching events, invalid payloads,
config assembly, and topic resolution) continue to pass because they never reach
the `publish()` call path.

## Quality Gates

```text
cargo fmt --all                                              -- passed
cargo check --all-targets --all-features                     -- passed
cargo clippy --all-targets --all-features -- -D warnings     -- passed
cargo test --all-features -- watcher::generic::producer      -- 8 passed, 1 ignored
```
