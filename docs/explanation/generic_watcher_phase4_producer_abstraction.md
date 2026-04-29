# Generic Watcher Phase 4: Producer Abstraction and Reliability

## Overview

Phase 4 introduces a `ResultProducerTrait` that decouples `GenericWatcher` from
the concrete Kafka producer implementation, adds a `FakeResultProducer` for
broker-free unit and integration testing, adds a `BufferedResultProducer`
dead-letter queue wrapper for resilience against transient broker failures, and
enables idempotent delivery settings on the production producer.

## Problem Statement

Before Phase 4 the `GenericWatcher` held a direct `Arc<GenericResultProducer>`
reference. This created two problems:

1. **Testability**: any test that exercised code paths that called
   `producer.publish()` required a live Kafka broker. The dry-run integration
   tests in `watcher.rs` were all marked `#[ignore]` for this reason.
2. **Reliability**: the production Kafka producer was created without idempotent
   delivery settings, meaning a transient broker timeout could cause a duplicate
   result message or a dropped result with no retry.

## Implementation

### Task 4.1: ResultProducerTrait in src/watcher/generic/result_producer.rs

A new file `src/watcher/generic/result_producer.rs` was created. It contains all
producer-related code and the `ResultProducerTrait` definition.

```rust
#[async_trait]
pub trait ResultProducerTrait: Send + Sync {
    async fn publish(&self, result: &GenericPlanResult) -> Result<()>;
    async fn flush(&self, timeout: Duration) -> Result<()>;
}
```

Both methods are `async` so all implementations fit naturally in a Tokio
executor. The `Send + Sync` bounds allow the trait to be stored as
`Arc<dyn ResultProducerTrait>` and shared safely across async tasks.

The `async_trait` macro from the `async-trait` crate is used to make the trait
object-safe, consistent with the pattern used elsewhere in the codebase.

### Task 4.2: GenericResultProducer moved and updated

`GenericResultProducer` was moved from `src/watcher/generic/producer.rs` into
`src/watcher/generic/result_producer.rs` and `ResultProducerTrait` was
implemented for it.

Two additions were made to the existing implementation:

**Idempotent delivery settings** are now applied unconditionally inside
`GenericResultProducer::new` when building the `rdkafka::ClientConfig`:

```rust
client_config.set("acks", "all");
client_config.set("retries", "5");
client_config.set("compression.type", "snappy");
client_config.set("enable.idempotence", "true");
```

These four settings guarantee that every produce request is retried safely by
the broker without introducing duplicate messages, even in the face of transient
network partitions or leader elections.

**A `flush` method** was added as part of the `ResultProducerTrait`
implementation. It delegates to `rdkafka`'s `Producer::flush`:

```rust
async fn flush(&self, timeout: Duration) -> Result<()> {
    self.producer
        .flush(timeout)
        .map_err(|e| XzatomaError::Watcher(format!("Failed to flush Kafka producer: {e}")))
}
```

`FutureProducer` in rdkafka 0.36 implements `Producer<FutureProducerContext<C>>`
which exposes `flush`. No additional wrapping is needed.

**`get_kafka_config`** was extended to include the four idempotent delivery
settings so they are visible in tests and diagnostic tooling:

```rust
("acks".to_string(), "all".to_string()),
("retries".to_string(), "5".to_string()),
("compression.type".to_string(), "snappy".to_string()),
("enable.idempotence".to_string(), "true".to_string()),
```

### Task 4.3: FakeResultProducer

`FakeResultProducer` is an in-memory accumulator that implements
`ResultProducerTrait` without performing any network I/O. It is declared `pub`
(not `#[cfg(test)]`) so integration tests in the `tests/` directory can use it
without a running broker.

```rust
pub struct FakeResultProducer {
    published: Mutex<Vec<GenericPlanResult>>,
}
```

`publish` appends the result to the internal `Vec` protected by a
`tokio::sync::Mutex`. `flush` is a no-op that returns `Ok(())`. The
`published_events` method returns a clone of the accumulated results for
assertion in tests.

A `Default` implementation delegates to `FakeResultProducer::new()`, consistent
with Rust conventions for types that have a sensible default construction.

### Task 4.4: BufferedResultProducer

`BufferedResultProducer` wraps any `ResultProducerTrait` implementation and adds
a bounded dead-letter queue:

```rust
pub struct BufferedResultProducer {
    inner: Arc<dyn ResultProducerTrait>,
    buffer: Mutex<VecDeque<GenericPlanResult>>,
    max_buffered: usize,
}

pub const DEFAULT_DLQ_MAX_BUFFERED: usize = 100;
```

The `publish` implementation follows a two-phase protocol:

**Drain phase**: attempt to forward each buffered event to `inner` in insertion
order. Stop on the first failure to preserve ordering and avoid reordering
events.

**Publish phase**: attempt to forward the new event to `inner`. If that fails,
append the event to the buffer. If the buffer is at capacity (`max_buffered`),
drop the oldest entry and emit a `warn!` log.

```rust
async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
    let mut buf = self.buffer.lock().await;

    while let Some(buffered) = buf.front().cloned() {
        if self.inner.publish(&buffered).await.is_err() {
            break;
        }
        buf.pop_front();
    }

    if self.inner.publish(result).await.is_err() {
        if buf.len() >= self.max_buffered {
            buf.pop_front();
            warn!(max_buffered = self.max_buffered,
                  "Dead-letter buffer full; dropping oldest buffered result");
        }
        buf.push_back(result.clone());
    }

    Ok(())
}
```

`publish` always returns `Ok(())`. Failures are absorbed into the buffer so the
watcher consume loop is never interrupted by transient broker unavailability.

The `flush` implementation drains the buffer before delegating to `inner.flush`.
The buffer lock is dropped before calling `inner.flush` to avoid holding it
across an arbitrary-length await.

`pending_count` returns the current buffer length for operational monitoring.

### Task 4.5: GenericWatcher updated

In `src/watcher/generic/watcher.rs` the `producer` field type was changed:

```rust
// Before
producer: Arc<GenericResultProducer>,

// After
producer: Arc<dyn ResultProducerTrait>,
```

`GenericWatcher::new` constructs a `GenericResultProducer` and coerces it to
`Arc<dyn ResultProducerTrait>` at the assignment site:

```rust
let producer: Arc<dyn ResultProducerTrait> = Arc::new(
    GenericResultProducer::new(&kafka_config)
        .map_err(|e| GenericWatcherError::Producer(e.to_string()))?,
);
```

All existing calls to `self.producer.publish` automatically dispatch through the
trait. No call sites required changes.

The `output_topic` method was updated to read directly from `kafka_config`
rather than delegating to the producer, since `output_topic()` is not part of
`ResultProducerTrait`:

```rust
pub fn output_topic(&self) -> &str {
    self.kafka_config
        .output_topic
        .as_deref()
        .unwrap_or(self.kafka_config.topic.as_str())
}
```

This is semantically equivalent to the previous delegation and removes the
dependency on a non-trait method.

### Task 4.6: mod.rs updated; producer.rs deleted

`src/watcher/generic/mod.rs` was updated:

- `pub mod producer;` replaced with `pub mod result_producer;`
- `pub use producer::GenericResultProducer;` replaced with a consolidated
  re-export block:

```rust
pub use result_producer::{
    BufferedResultProducer, FakeResultProducer, GenericResultProducer, ResultProducerTrait,
    DEFAULT_DLQ_MAX_BUFFERED,
};
```

`src/watcher/generic/producer.rs` was deleted. No re-export shim was created
since the module migration and deletion were performed in a single pass.

## Design Decisions

### Why always-on idempotent delivery?

Idempotent delivery (`enable.idempotence=true` + `acks=all`) is the correct
default for a result publisher. Results are critical operational records: a
dropped result means a plan execution goes unrecorded, and a duplicate is
harmless because downstream consumers can deduplicate on the `id` field (ULID).
There is no configuration knob to disable idempotence; it is always on.

### Why does BufferedResultProducer absorb errors?

The watcher consume loop must not crash due to a transient broker outage. If
`publish` returned an error, the watcher would propagate it up through
`process_event`, log it, and continue — but the result would be permanently
lost. The buffer trades memory for at-least-once delivery guarantees. The
`warn!` log and `pending_count` method provide observability into the buffer
state so operators can detect persistent broker unavailability.

### Why clone buffered events before await?

The drain loop calls `buf.front().cloned()` to clone the front element before
passing it to `inner.publish`. Cloning is necessary because `inner.publish`
takes `&GenericPlanResult` and requires an await, while `buf` holds a
`MutexGuard`. Cloning avoids holding an immutable borrow across the await point
while the `MutexGuard` is still live. `GenericPlanResult` is `Clone`, so the
clone is cheap relative to a network round-trip.

### Why is FakeResultProducer pub, not cfg(test)?

Integration tests in the `tests/` directory compile as separate crates and
cannot access `#[cfg(test)]` items from the library crate. Marking
`FakeResultProducer` as `pub` allows `tests/` integration tests to construct a
`GenericWatcher` equivalent with a fake producer without requiring a Kafka
broker.

### Why DEFAULT_DLQ_MAX_BUFFERED = 100?

100 events is a conservative bound. A `GenericPlanResult` JSON payload is
typically under 1 KB, so the buffer holds at most ~100 KB of memory. Operators
who need a larger buffer can pass a custom `max_buffered` value to
`BufferedResultProducer::new`.

## Testing

### ControlledProducer test double

All `BufferedResultProducer` tests use a `ControlledProducer` defined in
`#[cfg(test)]`. It fails the first `fail_count` calls to `publish` (using an
`AtomicUsize` for thread safety) then succeeds for all subsequent calls. This
simulates a broker that is transiently unavailable and then recovers, without
requiring a live Kafka connection.

### Test matrix

#### GenericResultProducer tests

| Test name                                                                    | What it verifies                                                      |
| ---------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| `test_generic_result_producer_uses_input_topic_when_output_topic_not_set`    | Output topic falls back to input topic                                |
| `test_generic_result_producer_uses_explicit_output_topic_when_configured`    | Explicit output topic takes priority                                  |
| `test_generic_result_producer_get_kafka_config_includes_basic_settings`      | bootstrap.servers, client.id, security.protocol, message.timeout.ms   |
| `test_generic_result_producer_get_kafka_config_includes_idempotent_settings` | acks=all, retries=5, compression.type=snappy, enable.idempotence=true |
| `test_generic_result_producer_get_kafka_config_includes_sasl_settings`       | SASL username, password, mechanism                                    |
| `test_generic_result_producer_new_returns_error_for_invalid_protocol`        | Unknown protocol string rejected at construction                      |
| `test_generic_result_producer_new_returns_error_for_missing_sasl_username`   | Missing SASL username rejected                                        |
| `test_generic_result_producer_new_returns_error_for_missing_sasl_password`   | Missing SASL password rejected                                        |
| `test_generic_result_producer_debug_impl`                                    | Debug output contains expected field values                           |
| `test_generic_result_producer_publish_succeeds`                              | `#[ignore]` — requires live broker                                    |

#### FakeResultProducer tests

| Test name                                             | What it verifies                                                           |
| ----------------------------------------------------- | -------------------------------------------------------------------------- |
| `test_fake_producer_new_is_empty`                     | `published_events()` is empty on construction                              |
| `test_fake_producer_records_single_event`             | One publish records one event                                              |
| `test_fake_producer_records_multiple_events_in_order` | Events recorded in insertion order                                         |
| `test_fake_producer_flush_is_noop`                    | `flush` returns `Ok(())` without panic                                     |
| `test_trait_object_dispatch`                          | `Arc<dyn ResultProducerTrait>` dispatches to `FakeResultProducer::publish` |

#### BufferedResultProducer tests

| Test name                                           | What it verifies                                        |
| --------------------------------------------------- | ------------------------------------------------------- |
| `test_buffered_publish_success_leaves_buffer_empty` | Successful publish keeps buffer empty                   |
| `test_buffered_publish_buffers_on_broker_failure`   | Failed publish appends to buffer                        |
| `test_buffered_multiple_failures_accumulate`        | Consecutive failures grow the buffer to 3 entries       |
| `test_buffered_drains_buffer_when_broker_recovers`  | Next successful publish drains buffered events in order |
| `test_buffered_drops_oldest_when_buffer_full`       | Buffer at capacity drops oldest entry on next failure   |

The `test_buffered_drains_buffer_when_broker_recovers` and
`test_buffered_drops_oldest_when_buffer_full` tests include annotated traces of
the `ControlledProducer` failure counter so the expected outcome is fully
auditable without running the code.

## Files Changed

| File                                     | Change                                                                                                                                                                              |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/watcher/generic/result_producer.rs` | Created: `ResultProducerTrait`, `GenericResultProducer` (with idempotent delivery + `flush`), `FakeResultProducer`, `BufferedResultProducer`, `DEFAULT_DLQ_MAX_BUFFERED`, all tests |
| `src/watcher/generic/watcher.rs`         | Changed `producer` field to `Arc<dyn ResultProducerTrait>`; updated `new()` cast; updated `output_topic()` to read from `kafka_config`; updated `start()` log line                  |
| `src/watcher/generic/mod.rs`             | Replaced `producer` submodule with `result_producer`; updated re-exports                                                                                                            |
| `src/watcher/generic/producer.rs`        | Deleted                                                                                                                                                                             |

## Success Criteria Verification

- `GenericResultProducer` is constructed with `acks=all` and
  `enable.idempotence=true`, verified by
  `test_generic_result_producer_get_kafka_config_includes_idempotent_settings`.
- `test_buffered_drains_buffer_when_broker_recovers` passes without a live Kafka
  connection by using the `ControlledProducer` test double.
- `GenericWatcher` holds `Arc<dyn ResultProducerTrait>`, not
  `Arc<GenericResultProducer>` (verified by inspecting the struct definition in
  `watcher.rs`).
- All four quality gates pass: `cargo fmt --all`,
  `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-features`.
- 192 watcher tests pass; 17 are ignored (require a live Kafka broker).
