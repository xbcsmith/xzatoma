# Phase 2: Watcher CLI and Configuration Completeness

## Overview

Phase 2 closes four gaps left open after Phase 1. All four are low-risk,
additive changes that improve the watcher's operational ergonomics without
altering its core message-handling or retry logic.

| Gap | Description                     | Priority |
| --- | ------------------------------- | -------- |
| 2a  | `watch --once` flag             | Medium   |
| 2b  | `watch --allow-dangerous` flag  | Low      |
| 2e  | Kafka `auto_offset_reset` field | Low      |
| 2f  | Kafka `ssl_ca_location` field   | Medium   |

---

## `--once` Flag

### What it does

Passing `--once` to the `watch` subcommand causes the watcher to consume exactly
one message from the Kafka topic, process it, and then exit with code `0`. The
watcher stops after the first message regardless of whether that message matched
the configured filter criteria. This is intended for CI pipelines and smoke
tests that inject a single known event and expect a clean exit.

### How it propagates

The flag is declared in `src/cli.rs` on `Commands::Watch`:

```rust
/// Process exactly one event from the Kafka topic then exit.
#[arg(long)]
once: bool,
```

`main.rs` reads the parsed value and stores it in `WatchCliOverrides.once`
(defined in `src/commands/mod.rs`). `run_watch` then calls `.with_once(true)` on
whichever backend is active before entering the consume loop:

```rust
// XZepr backend
let mut watcher = crate::watcher::XzeprWatcher::new(config, overrides.dry_run)?
    .with_once(overrides.once);

// Generic backend
let mut watcher =
    crate::watcher::generic::GenericWatcher::new(config, overrides.dry_run)?
        .with_once(overrides.once);
```

Both backends implement `with_once` as a builder method that sets an internal
`once` field. The consume loop checks this field after each successfully
dispatched message and breaks immediately when it is `true`.

---

## `--allow-dangerous` Flag

### What it does

Passing `--allow-dangerous` to the `watch` subcommand overrides
`watcher.execution.allow_dangerous` to `true` for that invocation. This permits
dangerous terminal commands to run inside plans executed by the watcher without
requiring a permanent change to the configuration file. The flag is intended for
controlled, single-run scenarios such as a staging deployment pipeline where
broader terminal access is acceptable.

### Which config field it overrides

`apply_cli_overrides` in `src/commands/mod.rs` sets the field directly:

```rust
if overrides.allow_dangerous {
    config.watcher.execution.allow_dangerous = true;
}
```

`WatcherExecutionConfig.allow_dangerous` (in `src/config.rs`) defaults to
`false`. The flag has no effect when omitted.

---

## `auto_offset_reset` Config Field

### Location

`auto_offset_reset: Option<String>` is a field on `KafkaWatcherConfig` in
`src/config.rs`. It is decorated with `#[serde(default)]`, so it deserializes as
`None` when absent from a configuration file.

### What it does

When `Some(value)`, the string is applied as the rdkafka consumer configuration
key `auto.offset.reset` during consumer construction. Accepted rdkafka values
are `"earliest"`, `"latest"`, and `"error"`.

When `None`, the field is not passed to rdkafka. Each backend then falls back to
its own internal default:

- The **generic watcher** receives the rdkafka library default, which is
  `"latest"`.
- The **XZepr watcher** has historically hardcoded `"earliest"` during consumer
  construction and continues to do so when the field is absent.

Set this field explicitly to eliminate the per-backend difference when a
consistent offset policy is required across both backends.

### Configuration example

```yaml
watcher:
  kafka:
    brokers: "localhost:9092"
    topic: "events"
    auto_offset_reset: "earliest"
```

---

## `ssl_ca_location` Config Field

### Location

`ssl_ca_location: Option<String>` is a field on `KafkaSecurityConfig` in
`src/config.rs`. It is decorated with `#[serde(default)]`, so it deserializes as
`None` when absent.

### When it applies

`apply_security_config` in `src/watcher/kafka_security.rs` constructs an
`SslConfig` only when the resolved protocol is `SSL` or `SASL_SSL`. The
`ca_location` member of that struct is populated from `ssl_ca_location`:

```rust
let ssl_config = if matches!(
    security_protocol,
    SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
) {
    Some(SslConfig {
        ca_location: security.ssl_ca_location.clone(),
        ..
    })
} else {
    None
};
```

For `PLAINTEXT` and `SASL_PLAINTEXT` protocols the `ssl_config` branch is not
entered and `ssl_ca_location` is silently ignored. This means existing
plain-text configurations are unaffected by the presence of the field in the
YAML.

### Configuration example

```yaml
watcher:
  kafka:
    security:
      protocol: "SSL"
      ssl_ca_location: "/etc/ssl/certs/ca-bundle.crt"
```

---

## Files Changed

| File                            | Change type | Description                                                        |
| ------------------------------- | ----------- | ------------------------------------------------------------------ |
| `src/cli.rs`                    | Modified    | Added `once` and `allow_dangerous` flags to `Commands::Watch`      |
| `src/commands/mod.rs`           | Modified    | Added both flags to `WatchCliOverrides`; dispatched in `run_watch` |
| `src/config.rs`                 | Modified    | Added `auto_offset_reset` to `KafkaWatcherConfig`                  |
| `src/config.rs`                 | Modified    | Added `ssl_ca_location` to `KafkaSecurityConfig`                   |
| `src/watcher/kafka_security.rs` | Modified    | `apply_security_config` populates `SslConfig.ca_location`          |

---

## Testing

### CLI flag parsing (`src/cli.rs`)

- `test_cli_parse_watch_once_flag` -- `--once` parses to `true`.
- `test_cli_parse_watch_once_defaults_false` -- omitting `--once` gives `false`.
- `test_cli_parse_watch_allow_dangerous_flag` -- `--allow-dangerous` parses to
  `true`.
- `test_cli_parse_watch_allow_dangerous_defaults_false` -- omitting
  `--allow-dangerous` gives `false`.
- `test_cli_parse_watch_once_and_allow_dangerous_together` -- both flags can be
  supplied together.

### Override application (`src/commands/mod.rs`)

- `test_apply_cli_overrides_allow_dangerous` -- verifies that passing
  `allow_dangerous: true` in `WatchCliOverrides` sets
  `config.watcher.execution.allow_dangerous` to `true`.
- `test_apply_cli_overrides_allow_dangerous_false_does_not_override` -- verifies
  that `allow_dangerous: false` leaves the config field at its existing value.

### Config field serialization (`src/config.rs`)

- `test_kafka_watcher_config_auto_offset_reset_defaults_none` -- a default
  `KafkaWatcherConfig` has `auto_offset_reset` as `None`.
- `test_kafka_watcher_config_auto_offset_reset_roundtrip` -- YAML with
  `auto_offset_reset: "earliest"` deserializes to `Some("earliest")`.
- `test_kafka_watcher_config_auto_offset_reset_absent_gives_none` -- YAML
  without the key deserializes to `None`.
- `test_kafka_security_config_ssl_ca_location_defaults_none` -- a
  `KafkaSecurityConfig` with only `protocol` set has `ssl_ca_location` as
  `None`.
- `test_kafka_security_config_ssl_ca_location_roundtrip` -- YAML with
  `ssl_ca_location: "/etc/ssl/certs/ca.pem"` deserializes correctly.
