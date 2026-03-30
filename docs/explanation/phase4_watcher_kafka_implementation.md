# Phase 4: Watcher Kafka Implementation

## Overview

Phase 4 replaces all Kafka I/O stubs in the watcher subsystem with real
`rdkafka` implementations. Before this phase, the watcher had fully implemented
business logic (matching, filtering, plan extraction, message types, config
assembly) but used log-only stubs for actual Kafka communication. After this
phase, both watcher backends (XZepr and Generic) can connect to a real Kafka
broker, consume messages, execute plans, and publish results.

## Scope

The following components were converted from stubs to real implementations:

| Component        | File                                  | Change                                                   |
| ---------------- | ------------------------------------- | -------------------------------------------------------- |
| Topic admin      | `src/watcher/topic_admin.rs`          | `AdminClient` for idempotent topic creation              |
| XZepr consumer   | `src/watcher/xzepr/consumer/kafka.rs` | `StreamConsumer` for message consumption                 |
| Generic watcher  | `src/watcher/generic/watcher.rs`      | `StreamConsumer` consume loop and real plan execution    |
| Generic producer | `src/watcher/generic/producer.rs`     | `FutureProducer` for result publishing                   |
| CLI flags        | `src/cli.rs`, `src/commands/mod.rs`   | `--brokers`, `--match-version`, `--filter-config` wiring |
| Example config   | `config/watcher.yaml`                 | Complete Kafka, matcher, and security sections           |

## Dependency Addition

`rdkafka` was added to `Cargo.toml` with the `cmake-build` and `ssl` features:

```text
rdkafka = { version = "0.36", features = ["cmake-build", "ssl"] }
```

The `cmake-build` feature compiles `librdkafka` from source so there is no
system-library dependency. Building from source requires `cmake` to be
installed. For environments with `librdkafka` already installed, a
`dynamic-linking` feature could be added in the future to reduce build times.

## Configuration Changes

Two new fields were added to `KafkaWatcherConfig`:

- `num_partitions` (default: `1`) -- number of partitions for auto-created
  topics
- `replication_factor` (default: `1`) -- replication factor for auto-created
  topics; set to `3` for production deployments with multiple brokers

Both fields use `#[serde(default)]` so existing configuration files continue to
work without modification.

## Task 4.1: rdkafka Dependency

Added `rdkafka = { version = "0.36", features = ["cmake-build", "ssl"] }` to
`Cargo.toml`. The `cmake-build` feature compiles `librdkafka` from C source,
removing any system library dependency.

## Task 4.2: WatcherTopicAdmin::ensure_topics()

### Before

The `ensure_topics()` method logged topic names at `info` level and returned
`Ok(())` without contacting a broker.

### After

The method now:

1. Validates all topic names before contacting the broker.
2. Builds an `AdminClient<DefaultClientContext>` from `get_kafka_config()`
   key-value pairs.
3. For each `TopicEnsureRequest`, calls `admin_client.create_topics()` with
   `NewTopic::new(name, num_partitions, TopicReplication::Fixed(replication_factor))`.
4. Treats `RDKafkaErrorCode::TopicAlreadyExists` as success (idempotent
   creation).
5. Retains `info!` logging so topic creation remains observable.

The `WatcherTopicAdmin` struct gained `num_partitions` and `replication_factor`
fields, populated from `KafkaWatcherConfig` in the constructor.

### Topic Admin Deduplication

Before Phase 4, topic administration ran in multiple places:

- `run_watch` in `commands/mod.rs` called topic admin before constructing the
  watcher.
- `XzeprWatcher::start()` called topic admin again internally.
- `GenericWatcher::start()` called topic admin again internally.
- `XzeprWatcher::new()` created a `_topic_admin` that was never used.

After Phase 4, topic administration runs exactly once during startup in
`run_watch`. The duplicate calls were removed from both watcher `start()`
methods and the unused construction was removed from `XzeprWatcher::new()`.
Callers using watcher types directly (without `run_watch`) should ensure topics
exist before calling `start()`.

## Task 4.3: XzeprConsumer::run() with StreamConsumer

### Before

The `run()` method entered a sleep loop that polled `self.running` every second,
producing no real Kafka messages.

### After

The `run()` method now:

1. Builds `rdkafka::ClientConfig` from `get_kafka_config()` key-value pairs.
2. Creates a `StreamConsumer` and subscribes to `&[&self.config.topic]`.
3. Uses `consumer.stream()` with `futures::StreamExt` to iterate messages.
4. For each message, extracts `payload_view::<str>()` and calls
   `Self::process_message(payload, &*handler)`.
5. Respects `self.running: Arc<AtomicBool>` for graceful shutdown using
   `tokio::select!` with a periodic timeout check.
6. Logs consumer group assignment and rebalance events.

The `run_with_channel()` method follows the same pattern but sends deserialized
`CloudEventMessage` values through the `mpsc::Sender<CloudEventMessage>` instead
of calling a handler.

Two private helper methods were extracted:

- `build_client_config()` -- constructs `rdkafka::ClientConfig` from settings
- `create_subscribed_consumer()` -- creates and subscribes a `StreamConsumer`

## Task 4.4: GenericWatcher::start() with StreamConsumer

### Before

The `start()` method logged startup details and returned immediately.

### After

The `start()` method now:

1. Builds `rdkafka::ClientConfig` from `get_kafka_config()` key-value pairs.
2. Creates a `StreamConsumer` and subscribes to `&[&self.kafka_config.topic]`.
3. Sets `self.running` to `true`.
4. Enters a message-stream loop dispatching payloads through
   `self.process_payload(payload)`.
5. Handles message processing errors gracefully (log and continue).
6. Checks `self.running` via `tokio::select!` for graceful shutdown.

New fields and methods added to `GenericWatcher`:

- `running: Arc<AtomicBool>` -- shutdown flag
- `pub fn stop(&self)` -- sets running to false
- `pub fn is_running(&self) -> bool` -- accessor

## Task 4.5: GenericResultProducer::publish() with FutureProducer

### Before

The `publish()` method serialized the result to JSON, logged it, and returned
`Ok(())`.

### After

The struct gained a `producer: FutureProducer` field, created in `new()` from
`get_kafka_config()` settings. The `publish()` method now:

1. Serializes `result` to JSON.
2. Sends via
   `self.producer.send(FutureRecord::to(&self.output_topic).payload(&payload).key(&result.trigger_event_id), self.request_timeout)`.
3. Maps delivery errors to `XzatomaError::Watcher(...)`.
4. Retains `tracing::info!` logging for observability.

Since `FutureProducer` does not implement `Clone` or `Debug`, the struct lost
its `#[derive(Debug, Clone)]` annotation and gained a manual `fmt::Debug`
implementation. The producer is wrapped in `Arc<GenericResultProducer>` in
`GenericWatcher`, so `Clone` is not needed.

## Task 4.6: GenericWatcher::execute_plan() with Real Execution

### Before

The `execute_plan()` method produced a hardcoded successful `GenericPlanResult`
with the plan text embedded in `plan_output`.

### After

The method now:

1. Calls `crate::commands::run::run_plan_with_options()` -- the same path the
   XZepr watcher's `WatcherMessageHandler` already uses.
2. Captures the execution outcome (success or failure with error message).
3. Constructs a `GenericPlanResult` reflecting the actual result.
4. Respects `execution_semaphore` (already enforced in `process_event`).
5. In dry-run mode, the existing behavior (synthetic success without execution)
   is preserved.

The `#[allow(dead_code)]` annotation was removed from `GenericWatcherError`
since all variants are now actively used.

## Task 4.7: CLI Gaps

### New flags added to the Watch command

| Flag              | Type             | Purpose                                |
| ----------------- | ---------------- | -------------------------------------- |
| `--brokers`       | `Option<String>` | Override `kafka.brokers`               |
| `--match-version` | `Option<String>` | Override `generic_match.version` regex |

The long flag `--match-version` avoids conflict with clap's built-in `--version`
flag.

### Filter config wiring

The `--filter-config` flag existed in the CLI struct but was silently ignored in
`apply_cli_overrides`. It is now wired:

1. Reads the YAML file at the provided path.
2. Deserializes it as `EventFilterConfig`.
3. Replaces `config.watcher.filters` with the parsed config.
4. Maps read and parse errors to `XzatomaError::Config(...)`.

### WatchCliOverrides

Two new fields were added:

- `brokers: Option<String>`
- `match_version: Option<String>`

Both are wired in `apply_cli_overrides` and the `Commands::Watch` destructuring
in `main.rs`.

## Task 4.8: Example Configuration

The `config/watcher.yaml` file was updated to show:

1. `watcher_type: xzepr` with a comment listing valid values.
2. Complete `kafka` section including `brokers`, `topic`, `output_topic`,
   `group_id`, `auto_create_topics`, `num_partitions`, and `replication_factor`.
3. A `generic_match` section with `action`, `name`, and `version` patterns.
4. A `security` section with `protocol`, `sasl_mechanism`, `sasl_username`, and
   `sasl_password` (referencing environment variables for secrets).
5. Both development (PLAINTEXT) and production (SASL_SSL) examples.

## Task 4.9: Testing

### Unit tests

All existing unit tests continue to pass (1618 passed, 0 failed). Tests that
exercise configuration assembly, topic resolution, matcher logic, message
serialization, and plan extraction work without a Kafka broker.

### Integration tests

Tests requiring a real Kafka broker are marked `#[ignore]`. These cover:

- `WatcherTopicAdmin::ensure_topics()` idempotent topic creation
- `XzeprConsumer::run()` handler-based message processing
- `XzeprConsumer::run_with_channel()` channel-based message processing
- `GenericWatcher::start()` end-to-end consume loop
- `GenericResultProducer::publish()` delivery to a topic

### New unit tests added

- `test_watcher_topic_admin_new_stores_num_partitions`
- `test_watcher_topic_admin_new_stores_replication_factor`
- `test_validate_topic_name_rejects_empty`
- `test_validate_topic_name_rejects_whitespace_only`
- `test_validate_topic_name_accepts_valid_name`
- `test_build_client_config_contains_all_settings`
- `test_consumer_not_running_initially`
- `test_stop_when_already_stopped`
- `test_process_message_handler_error_still_returns_ok`
- `test_generic_result_producer_debug_impl`
- `test_cli_parse_watch_with_brokers_flag`
- `test_cli_parse_watch_with_match_version_flag`
- `test_apply_cli_overrides_brokers`
- `test_apply_cli_overrides_match_version`
- `test_apply_cli_overrides_filter_config_from_file`
- `test_apply_cli_overrides_filter_config_missing_file`

## Success Criteria Verification

| Criterion                                                  | Status                                                         |
| ---------------------------------------------------------- | -------------------------------------------------------------- |
| `grep -r "stub" src/watcher/` returns zero results         | Verified                                                       |
| Both backends can connect to a real Kafka broker           | Implemented (StreamConsumer + FutureProducer)                  |
| XZepr backend consumes messages and executes plans         | Implemented via real StreamConsumer + WatcherMessageHandler    |
| Generic backend consumes, matches, executes, and publishes | Implemented via StreamConsumer + execute_plan + FutureProducer |
| `--dry-run` works for both backends                        | Preserved (dry-run logic unchanged)                            |
| All existing unit tests pass                               | 1618 passed, 0 failed                                          |
| New integration tests added                                | 19 ignored (need real broker)                                  |
| `--brokers` CLI flag added                                 | Implemented                                                    |
| `--match-version` CLI flag added                           | Implemented                                                    |
| `--filter-config` wired or removed                         | Wired (loads YAML and applies to config)                       |
| `config/watcher.yaml` updated                              | Complete with all sections                                     |

## Files Modified

- `Cargo.toml` -- added `rdkafka` dependency
- `src/config.rs` -- added `num_partitions`, `replication_factor` fields
- `src/cli.rs` -- added `--brokers`, `--match-version` flags
- `src/main.rs` -- wired new CLI fields to `WatchCliOverrides`
- `src/commands/mod.rs` -- new override fields, filter-config wiring,
  deduplication of topic admin
- `src/watcher/mod.rs` -- no changes
- `src/watcher/generic/mod.rs` -- removed "stub" from doc comment
- `src/watcher/generic/producer.rs` -- real `FutureProducer` implementation
- `src/watcher/generic/watcher.rs` -- real `StreamConsumer` loop, real
  `execute_plan`, `running`/`stop()` support
- `src/watcher/topic_admin.rs` -- real `AdminClient` implementation
- `src/watcher/xzepr/consumer/kafka.rs` -- real `StreamConsumer` in `run()` and
  `run_with_channel()`
- `src/watcher/xzepr/watcher.rs` -- removed duplicate topic admin calls
- `config/watcher.yaml` -- comprehensive example configuration
