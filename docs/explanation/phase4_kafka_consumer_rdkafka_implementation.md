# Phase 4 Task 4.3: Kafka Consumer rdkafka Implementation

## Summary

Replaced the sleep-loop stubs in `XzeprConsumer::run()` and
`XzeprConsumer::run_with_channel()` with a real
`rdkafka::consumer::StreamConsumer` implementation. The consumer now connects to
a Kafka broker, subscribes to the configured topic, and streams messages for
processing.

## Changes

### File Modified

- `src/watcher/xzepr/consumer/kafka.rs`

### New Imports

| Import                                          | Purpose                                             |
| ----------------------------------------------- | --------------------------------------------------- |
| `rdkafka::consumer::{Consumer, StreamConsumer}` | Kafka consumer trait and async stream consumer      |
| `rdkafka::ClientConfig`                         | Builder for Kafka client configuration              |
| `rdkafka::Message`                              | Trait for accessing message payload                 |
| `futures::StreamExt`                            | Async stream iteration via `.next()`                |
| `tracing::warn`                                 | Warning-level log for unexpected stream termination |

### New Private Helper Methods

#### `build_client_config(&self) -> ClientConfig`

Constructs an `rdkafka::ClientConfig` from the key-value pairs returned by
`get_kafka_config()`. This isolates the rdkafka configuration assembly into a
single reusable method.

#### `create_subscribed_consumer(&self) -> Result<StreamConsumer, ConsumerError>`

Calls `build_client_config()`, creates a `StreamConsumer`, subscribes it to the
configured topic, and logs the consumer group assignment. Returns a
`ConsumerError::Kafka` on failure. Both `run()` and `run_with_channel()`
delegate to this method to avoid duplicating consumer setup logic.

### Updated Public Methods

#### `run(handler)` -- Handler-Based Consumption

Previous behavior: sleep-loop stub that never consumed messages.

New behavior:

1. Sets the `running` flag to `true`.
2. Calls `create_subscribed_consumer()` to build and subscribe the consumer.
3. Enters a loop that checks `self.running` on each iteration.
4. Uses `tokio::select!` with a biased branch: the primary branch awaits
   `stream.next()` for the next Kafka message; a secondary branch sleeps for one
   second so the shutdown flag is checked periodically even when no messages are
   arriving.
5. For each message, extracts the payload via `payload_view::<str>()` and
   delegates to `Self::process_message(payload, handler)`.
6. On fatal Kafka errors (`Some(Err(e))`), stores `false` in `running` and
   returns `ConsumerError::Kafka`.
7. On stream termination (`None`), logs a warning and breaks.
8. On exit, stores `false` in `running`.

#### `run_with_channel(sender)` -- Channel-Based Consumption

Previous behavior: sleep-loop stub that never consumed messages.

New behavior: identical `StreamConsumer` setup and message loop, but instead of
calling a `MessageHandler`, deserializes the payload into a `CloudEventMessage`
and sends it through the `mpsc::Sender<CloudEventMessage>`. If the receiver side
of the channel has been dropped (`sender.send()` returns `Err`), the consumer
logs a message and breaks out of the loop gracefully.

### Doc Comment Updates

- Removed all "stub mode", "mock/stub", and "no actual Kafka connection"
  language from struct and method documentation.
- Updated `XzeprConsumer` struct doc to describe the real `StreamConsumer`
  backing.
- Updated `run()` and `run_with_channel()` doc comments with accurate
  descriptions of behavior, error conditions, and shutdown semantics.
- Added `# Errors` section to `process_message()` doc comment.

## Error Handling

All `rdkafka` errors are mapped to `ConsumerError::Kafka(String)`:

- Consumer creation failure from `ClientConfig::create()`.
- Subscription failure from `consumer.subscribe()`.
- Per-message Kafka errors during streaming cause the consumer to stop and
  return the error.
- UTF-8 decoding errors on message payloads are logged but do not stop the
  consumer.
- JSON deserialization errors are logged but do not stop the consumer (in
  `run()` they propagate from `process_message`; in `run_with_channel()` they
  are caught and logged inline).

## Graceful Shutdown

The `Arc<AtomicBool>` running flag is checked at the top of every loop
iteration. The `tokio::select!` with a one-second sleep timeout ensures the flag
is evaluated even when no Kafka messages are arriving. Calling `consumer.stop()`
from any thread or task sets the flag to `false`, causing the loop to exit
within at most one second.

## Testing

### Existing Tests Preserved

All existing unit tests continue to pass without modification:

| Test                                | Status |
| ----------------------------------- | ------ |
| `test_consumer_new`                 | Pass   |
| `test_consumer_kafka_config`        | Pass   |
| `test_consumer_stop`                | Pass   |
| `test_process_message_valid`        | Pass   |
| `test_process_message_invalid_json` | Pass   |
| `test_consumer_with_ssl_config`     | Pass   |

### Modified Tests

| Test                | Change                                                         |
| ------------------- | -------------------------------------------------------------- |
| `test_run_and_stop` | Marked `#[ignore]` because it now requires a real Kafka broker |

### New Unit Tests

| Test                                                  | Purpose                                                               |
| ----------------------------------------------------- | --------------------------------------------------------------------- |
| `test_build_client_config_contains_all_settings`      | Verifies config key-value assembly including SASL credentials         |
| `test_consumer_not_running_initially`                 | Confirms `is_running()` is `false` after construction                 |
| `test_stop_when_already_stopped`                      | Confirms calling `stop()` when not running does not panic             |
| `test_process_message_handler_error_still_returns_ok` | Confirms handler errors are logged but `process_message` returns `Ok` |

### New Integration Tests (Ignored)

These tests require a running Kafka broker at `localhost:9092` with a
`test-topic` topic available. Run them with `cargo test -- --ignored`.

| Test                                             | Purpose                                                          |
| ------------------------------------------------ | ---------------------------------------------------------------- |
| `test_run_with_channel_integration`              | Validates channel-based consumption lifecycle with a real broker |
| `test_run_handler_receives_messages_integration` | Validates handler-based consumption with a real broker           |

## Architecture Compliance

- No changes to any file outside `src/watcher/xzepr/consumer/kafka.rs`.
- `tools/` module boundary not crossed; `providers/` not imported.
- All public items have `///` doc comments.
- No emojis in code, comments, or documentation.
- No `unwrap()` or `expect()` without justification.
- All errors use `Result<T, ConsumerError>` with `?` propagation.
