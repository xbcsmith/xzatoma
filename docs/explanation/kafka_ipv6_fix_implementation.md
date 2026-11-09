# Kafka IPv6 Connection Failure Fix

## Problem

When xzatoma connects to a local Kafka broker using `localhost` as the
bootstrap-servers address, rdkafka resolves `localhost` to its IPv6 loopback
address `::1` on systems where IPv6 is preferred. The broker is only listening
on the IPv4 loopback address `127.0.0.1`, so every connection attempt fails
with:

```text
ERROR rdkafka::client: librdkafka: Global error: BrokerTransportFailure
    localhost:19092/0: Connect to ipv6#[::1]:19092 failed: Connection refused
```

## Root Cause

`KafkaWatcherConfig` already carried a `broker_address_family` field (defaulting
to `"v4"`) that rdkafka maps to its `broker.address.family` property. This
property tells rdkafka which address family to prefer when resolving bootstrap
broker host names.

The field was wired correctly in two places but was silently dropped in two
others:

| Location                                                    | Status      |
| ----------------------------------------------------------- | ----------- |
| `xzepr/consumer/kafka.rs` `get_kafka_config()`              | Correct     |
| `generic/watcher.rs` `get_kafka_config()`                   | Correct     |
| `generic/result_producer.rs` `GenericResultProducer::new()` | **Missing** |
| `topic_admin.rs` `WatcherTopicAdmin`                        | **Missing** |

### Bug 1 — `GenericResultProducer::new()`

`GenericResultProducer::new()` builds the rdkafka `ClientConfig` manually, key
by key. `ResolvedProducerSettings` already populated
`settings.broker_address_family` correctly from the config, and
`ResolvedProducerSettings::kafka_config()` (the informational method) emitted
the key. However, the manual build block that produces the real `FutureProducer`
never called:

```rust
client_config.set("broker.address.family", &settings.broker_address_family);
```

So the produced `FutureProducer` used rdkafka's default address-family
resolution, which on dual-stack hosts prefers IPv6.

### Bug 2 — `WatcherTopicAdmin`

`WatcherTopicAdmin` had no `broker_address_family` field. Its `new()`
constructor read `num_partitions` and `replication_factor` from the config but
not `broker_address_family`. Consequently `get_kafka_config()` never emitted the
`broker.address.family` key, so the admin client used for topic creation
connected via IPv6 on affected hosts.

## Fix

### `src/watcher/generic/result_producer.rs`

Added one line inside the manual `ClientConfig` builder in
`GenericResultProducer::new()`, immediately after the idempotent delivery
settings:

```rust
// Force IPv4 (or v6/any) so rdkafka does not try IPv6 when the
// broker is reachable only on IPv4 (e.g. localhost:19092 on macOS).
client_config.set("broker.address.family", &settings.broker_address_family);
```

### `src/watcher/topic_admin.rs`

Three changes:

1. Added `broker_address_family: String` to the `WatcherTopicAdmin` struct after
   `replication_factor`.

2. Populated the field in `new()`:

   ```rust
   broker_address_family: config.broker_address_family.clone(),
   ```

3. Emitted the key from `get_kafka_config()`:

   ```rust
   (
       "broker.address.family".to_string(),
       self.broker_address_family.clone(),
   ),
   ```

## Default Configuration

`KafkaWatcherConfig::broker_address_family` already defaults to `"v4"` via
`#[serde(default = "default_broker_address_family")]`. No config-file or
default-value changes are required. Users who need IPv6 can set
`broker_address_family: "v6"` or `broker_address_family: "any"` in their YAML
config.

## Tests Added

| Test                                                                            | File                 | Description                                         |
| ------------------------------------------------------------------------------- | -------------------- | --------------------------------------------------- |
| `test_resolved_producer_settings_includes_broker_address_family`                | `result_producer.rs` | Default config emits `broker.address.family = "v4"` |
| `test_resolved_producer_settings_broker_address_family_v6`                      | `result_producer.rs` | `"v6"` is forwarded through to the config map       |
| Updated `test_generic_result_producer_get_kafka_config_includes_basic_settings` | `result_producer.rs` | Added assertion for `broker.address.family = "v4"`  |
| Updated `test_watcher_topic_admin_get_kafka_config_includes_basic_settings`     | `topic_admin.rs`     | Added assertion for `broker.address.family = "v4"`  |
| `test_watcher_topic_admin_get_kafka_config_respects_broker_address_family`      | `topic_admin.rs`     | `"any"` is stored and returned correctly            |

## Files Changed

- `src/watcher/generic/result_producer.rs`
- `src/watcher/topic_admin.rs`
