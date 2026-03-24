# Watcher Environment Variables Reference

This document describes the environment variables that affect XZatoma watcher
behavior.

XZatoma supports two watcher backends:

- `xzepr` for XZepr CloudEvents-style messages
- `generic` for generic JSON plan-event messages

Environment variables can override watcher configuration at runtime. When both a
configuration file and environment variables are present, environment variables
take precedence over file values.

Kafka connection variables continue to use the `XZEPR_KAFKA_` prefix for
backward compatibility. Watcher-specific runtime overrides use the
`XZATOMA_WATCHER_` prefix.

## Kafka Configuration

These variables control the Kafka connection used by the watcher.

### `XZEPR_KAFKA_BROKERS`

Kafka broker addresses as a comma-separated list.

- Maps to: `watcher.kafka.brokers`
- Default: `localhost:9092` when watcher Kafka config is populated only from
  environment variables

Example:

```bash
export XZEPR_KAFKA_BROKERS="kafka-1.prod:9093,kafka-2.prod:9093"
```

### `XZEPR_KAFKA_TOPIC`

Topic to consume from.

- Maps to: `watcher.kafka.topic`
- Default: `xzepr.dev.events` when watcher Kafka config is populated only from
  environment variables

Example:

```bash
export XZEPR_KAFKA_TOPIC="xzepr.production.events"
```

### `XZEPR_KAFKA_GROUP_ID`

Kafka consumer group identifier.

- Maps to: `watcher.kafka.group_id`
- Default: `xzatoma-watcher`

Example:

```bash
export XZEPR_KAFKA_GROUP_ID="xzatoma-watcher-prod"
```

### `XZEPR_KAFKA_SECURITY_PROTOCOL`

Kafka security protocol.

- Maps to: `watcher.kafka.security.protocol`
- Valid values:
  - `PLAINTEXT`
  - `SSL`
  - `SASL_PLAINTEXT`
  - `SASL_SSL`

Example:

```bash
export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
```

### `XZEPR_KAFKA_SASL_MECHANISM`

Kafka SASL mechanism.

- Maps to: `watcher.kafka.security.sasl_mechanism`
- Valid values:
  - `PLAIN`
  - `SCRAM-SHA-256`
  - `SCRAM-SHA-512`

Example:

```bash
export XZEPR_KAFKA_SASL_MECHANISM="SCRAM-SHA-256"
```

### `XZEPR_KAFKA_SASL_USERNAME`

Kafka SASL username.

- Maps to: `watcher.kafka.security.sasl_username`

Example:

```bash
export XZEPR_KAFKA_SASL_USERNAME="xzatoma-consumer"
```

### `XZEPR_KAFKA_SASL_PASSWORD`

Kafka SASL password.

- Maps to: `watcher.kafka.security.sasl_password`
- Sensitive: prefer secret injection or a secret manager
- Related fallback used by some watcher code paths:
  - `KAFKA_SASL_PASSWORD`

Example:

```bash
export XZEPR_KAFKA_SASL_PASSWORD="supersecret"
```

## Generic Watcher Configuration

These variables configure generic-watcher-specific behavior.

### `XZATOMA_WATCHER_TYPE`

Selects the active watcher backend.

- Maps to: `watcher.watcher_type`
- Accepted values:
  - `xzepr`
  - `generic`
- Default: `xzepr`

Examples:

```bash
export XZATOMA_WATCHER_TYPE="xzepr"
```

```bash
export XZATOMA_WATCHER_TYPE="generic"
```

Use `xzepr` when consuming XZepr CloudEvents and `generic` when consuming
generic plan-event JSON messages.

### `XZATOMA_WATCHER_OUTPUT_TOPIC`

Configures the output topic for generic watcher result events.

- Maps to: `watcher.kafka.output_topic`
- Used by: generic watcher
- Default behavior when unset:
  - generic watcher publishes results back to `watcher.kafka.topic`

Example:

```bash
export XZATOMA_WATCHER_OUTPUT_TOPIC="plans.results"
```

If this variable is omitted and the generic watcher is active, result events are
published to the same topic as the input topic. This is safe because the generic
watcher only processes events where `event_type == "plan"` and generic watcher
result events always use `event_type == "result"`.

### `XZATOMA_WATCHER_MATCH_ACTION`

Regex pattern for matching the generic watcher event `action` field.

- Maps to: `watcher.generic_match.action`
- Used by: generic watcher
- Matching behavior:
  - treated as a regular expression
  - case-insensitive by default

Example:

```bash
export XZATOMA_WATCHER_MATCH_ACTION="deploy.*"
```

A value of `deploy.*` matches actions such as:

- `deploy`
- `deploy-prod`
- `deployment`

### `XZATOMA_WATCHER_MATCH_NAME`

Regex pattern for matching the generic watcher event `name` field.

- Maps to: `watcher.generic_match.name`
- Used by: generic watcher
- Matching behavior:
  - treated as a regular expression
  - case-insensitive by default

Example:

```bash
export XZATOMA_WATCHER_MATCH_NAME="service-a"
```

### `XZATOMA_WATCHER_MATCH_VERSION`

Regex pattern for matching the generic watcher event `version` field.

- Maps to: `watcher.generic_match.version`
- Used by: generic watcher
- Matching behavior:
  - treated as a regular expression
  - case-insensitive by default

Example:

```bash
export XZATOMA_WATCHER_MATCH_VERSION="^v1\\.[0-9]+$"
```

## XZepr Watcher Filter Configuration

These variables override XZepr CloudEvent filter behavior.

### `XZATOMA_WATCHER_EVENT_TYPES`

Comma-separated list of XZepr event types to process.

- Maps to: `watcher.filters.event_types`
- Used by: XZepr watcher

Example:

```bash
export XZATOMA_WATCHER_EVENT_TYPES="deployment.success,ci.pipeline.completed"
```

### `XZATOMA_WATCHER_SOURCE_PATTERN`

Regex filter for the XZepr CloudEvent `source` field.

- Maps to: `watcher.filters.source_pattern`
- Used by: XZepr watcher

Example:

```bash
export XZATOMA_WATCHER_SOURCE_PATTERN="^xzepr\\.receiver\\.prod\\."
```

### `XZATOMA_WATCHER_PLATFORM_ID`

Filter for XZepr `platform_id`.

- Maps to: `watcher.filters.platform_id`
- Used by: XZepr watcher

Example:

```bash
export XZATOMA_WATCHER_PLATFORM_ID="kubernetes"
```

### `XZATOMA_WATCHER_PACKAGE`

Filter for XZepr `package`.

- Maps to: `watcher.filters.package`
- Used by: XZepr watcher

Example:

```bash
export XZATOMA_WATCHER_PACKAGE="my-service-package"
```

### `XZATOMA_WATCHER_API_VERSION`

Filter for XZepr `api_version`.

- Maps to: `watcher.filters.api_version`
- Used by: XZepr watcher

Example:

```bash
export XZATOMA_WATCHER_API_VERSION="v1beta"
```

### `XZATOMA_WATCHER_SUCCESS_ONLY`

Controls whether only successful XZepr events are processed.

- Maps to: `watcher.filters.success_only`
- Used by: XZepr watcher
- Accepted values:
  - `true`
  - `false`

Example:

```bash
export XZATOMA_WATCHER_SUCCESS_ONLY="true"
```

## Logging Configuration

These variables control watcher logging.

### `XZATOMA_WATCHER_LOG_LEVEL`

Watcher log level.

- Maps to: `watcher.logging.level`
- Accepted values:
  - `trace`
  - `debug`
  - `info`
  - `warn`
  - `error`

Example:

```bash
export XZATOMA_WATCHER_LOG_LEVEL="debug"
```

### `XZATOMA_WATCHER_LOG_FILE`

Path to the watcher log file.

- Maps to: `watcher.logging.file_path`

Example:

```bash
export XZATOMA_WATCHER_LOG_FILE="/var/log/xzatoma/watcher.log"
```

### `XZATOMA_WATCHER_JSON_LOGS`

Enable JSON log output.

- Maps to: `watcher.logging.json_format`
- Accepted values:
  - `true`
  - `false`

Example:

```bash
export XZATOMA_WATCHER_JSON_LOGS="true"
```

### `XZATOMA_WATCHER_INCLUDE_PAYLOAD`

Include full event payloads in logs.

- Maps to: `watcher.logging.include_payload`
- Accepted values:
  - `true`
  - `false`

Example:

```bash
export XZATOMA_WATCHER_INCLUDE_PAYLOAD="true"
```

Be careful when enabling payload logging in environments where event contents
may contain sensitive data.

## Execution Configuration

These variables control execution behavior for watcher-triggered plans.

### `XZATOMA_WATCHER_ALLOW_DANGEROUS`

Allow potentially dangerous operations during plan execution.

- Maps to: `watcher.execution.allow_dangerous`
- Accepted values:
  - `true`
  - `false`

Example:

```bash
export XZATOMA_WATCHER_ALLOW_DANGEROUS="false"
```

### `XZATOMA_WATCHER_MAX_CONCURRENT`

Maximum number of concurrent watcher-triggered plan executions.

- Maps to: `watcher.execution.max_concurrent_executions`

Example:

```bash
export XZATOMA_WATCHER_MAX_CONCURRENT="5"
```

### `XZATOMA_WATCHER_EXECUTION_TIMEOUT`

Execution timeout in seconds for watcher-triggered plans.

- Maps to: `watcher.execution.execution_timeout_secs`

Example:

```bash
export XZATOMA_WATCHER_EXECUTION_TIMEOUT="600"
```

## Generic Watcher Examples

### Minimal generic watcher environment setup

```bash
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.events"
export XZEPR_KAFKA_GROUP_ID="xzatoma-generic-watcher"
export XZATOMA_WATCHER_MATCH_ACTION="deploy"
```

### Generic watcher with separate output topic

```bash
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.events"
export XZEPR_KAFKA_GROUP_ID="xzatoma-generic-watcher"
export XZATOMA_WATCHER_OUTPUT_TOPIC="plans.results"
export XZATOMA_WATCHER_MATCH_ACTION="deploy.*"
export XZATOMA_WATCHER_MATCH_NAME="service-a"
export XZATOMA_WATCHER_MATCH_VERSION="^v1\\.[0-9]+$"
```

### Generic watcher accept-all mode

```bash
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.catch_all"
export XZEPR_KAFKA_GROUP_ID="xzatoma-generic-catch-all"
```

If no generic match variables are set, the generic watcher accepts every event
where `event_type == "plan"`.

## XZepr Watcher Example

```bash
export XZATOMA_WATCHER_TYPE="xzepr"
export XZEPR_KAFKA_BROKERS="kafka-1.prod:9093,kafka-2.prod:9093"
export XZEPR_KAFKA_TOPIC="xzepr.production.events"
export XZEPR_KAFKA_GROUP_ID="xzatoma-watcher-prod"
export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
export XZEPR_KAFKA_SASL_MECHANISM="SCRAM-SHA-256"
export XZEPR_KAFKA_SASL_USERNAME="xzatoma-consumer"
export XZEPR_KAFKA_SASL_PASSWORD="secret-from-manager"
export XZATOMA_WATCHER_EVENT_TYPES="deployment.success,ci.pipeline.completed"
export XZATOMA_WATCHER_LOG_LEVEL="warn"
```

## Running the Watcher

After setting environment variables, start the watcher like this:

```bash
xzatoma watch --config config/config.yaml --dry-run
```

For a generic watcher launched entirely from environment variables:

```bash
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.events"
export XZATOMA_WATCHER_MATCH_ACTION="deploy"
xzatoma watch --config config/config.yaml --dry-run
```

## Troubleshooting

### The watcher fails with a Kafka configuration error

Make sure you have either:

- a populated `watcher.kafka` section in your YAML config
- or the required `XZEPR_KAFKA_*` environment variables set

### The generic watcher does not process any events

Check that:

- `XZATOMA_WATCHER_TYPE` is set to `generic`
- incoming messages use `event_type: "plan"`
- your regex patterns actually match the incoming `action`, `name`, or `version`
  values

### The generic watcher appears to ignore result messages

That is expected. Generic watcher result events use `event_type: "result"` and
are intentionally skipped to prevent same-topic input/output loops.

### The XZepr watcher skips expected events

Check your XZepr filter variables:

- `XZATOMA_WATCHER_EVENT_TYPES`
- `XZATOMA_WATCHER_SOURCE_PATTERN`
- `XZATOMA_WATCHER_PLATFORM_ID`
- `XZATOMA_WATCHER_PACKAGE`
- `XZATOMA_WATCHER_API_VERSION`
- `XZATOMA_WATCHER_SUCCESS_ONLY`

### Logging is not detailed enough

Increase verbosity:

```bash
export XZATOMA_WATCHER_LOG_LEVEL="debug"
```

## Security Notes

- Do not commit SASL passwords or other secrets to version control.
- Prefer environment injection, a secret manager, or platform-managed secret
  storage.
- Use `SASL_SSL` in production where possible.
- Be careful with `XZATOMA_WATCHER_INCLUDE_PAYLOAD` in environments where
  payload contents are sensitive.

## Related Documentation

- how-to guide: `docs/how-to/setup_watcher.md`
- configuration reference: `docs/reference/configuration.md`
- architecture reference: `docs/reference/architecture.md`
- generic watcher example config: `config/generic_watcher.yaml`
