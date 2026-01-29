# Watcher Environment Variables Reference

This document lists environment variables that affect the XZatoma watcher. Use
environment variables to provide secrets (for example, Kafka SASL passwords)
and to override configuration values at runtime. When both a configuration
file and environment variables are present, environment variables take
precedence.

Note: Kafka-specific environment variables used by the XZepr consumer are
prefixed with `XZEPR_`. Watcher-specific runtime overrides use the
`XZATOMA_WATCHER_` prefix.

----

## Kafka Configuration

These environment variables control the Kafka connection used by the watcher.
They are consumed primarily by the XZepr consumer code (`XzeprConsumer`) and
can be used instead of or in addition to the `watcher.kafka` block in the
YAML configuration.

- `XZEPR_KAFKA_BROKERS`
  Kafka broker addresses (comma-separated).
  Default: `localhost:9092`
  Example:
  ```bash
  export XZEPR_KAFKA_BROKERS="kafka-1.prod:9093,kafka-2.prod:9093"
  ```

- `XZEPR_KAFKA_TOPIC`
  Topic to consume from.
  Default: `xzepr.dev.events` (or `watcher.kafka.topic` if set in config)
  Example:
  ```bash
  export XZEPR_KAFKA_TOPIC="xzepr.production.events"
  ```

- `XZEPR_KAFKA_GROUP_ID`
  Consumer group ID (default used when not provided).
  Default: `xzatoma-watcher` (when set in watcher config)
  Example:
  ```bash
  export XZEPR_KAFKA_GROUP_ID="xzatoma-watcher-prod"
  ```

- `XZEPR_KAFKA_SECURITY_PROTOCOL`
  Security protocol. Valid values: `PLAINTEXT`, `SSL`, `SASL_PLAINTEXT`, `SASL_SSL`.
  Example:
  ```bash
  export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
  ```

- `XZEPR_KAFKA_SASL_MECHANISM`
  SASL mechanism. Valid values: `PLAIN`, `SCRAM-SHA-256`, `SCRAM-SHA-512`.
  Example:
  ```bash
  export XZEPR_KAFKA_SASL_MECHANISM="SCRAM-SHA-256"
  ```

- `XZEPR_KAFKA_SASL_USERNAME`
  SASL username (required if using SASL).
  Example:
  ```bash
  export XZEPR_KAFKA_SASL_USERNAME="xzatoma-consumer"
  ```

- `XZEPR_KAFKA_SASL_PASSWORD`
  SASL password (sensitive). Prefer using a secret manager or process-managed
  environment variables. The watcher also checks `KAFKA_SASL_PASSWORD` as a
  fallback when applying runtime security config.
  Example:
  ```bash
  export XZEPR_KAFKA_SASL_PASSWORD="supersecret"
  ```

- `XZEPR_KAFKA_SSL_CA_LOCATION`, `XZEPR_KAFKA_SSL_CERT_LOCATION`, `XZEPR_KAFKA_SSL_KEY_LOCATION`
  Paths for TLS/SSL CA, client certificate, and client key (optional when using SSL/SASL_SSL).

Security note: Do NOT store sensitive values (passwords, private keys) in
committed configuration files. Use environment variables, secret managers,
or platform-specific secret storage (e.g., Kubernetes Secrets).

----

## Filter Configuration

Watcher filtering behavior can be overridden at runtime with these variables.
Some values are comma-separated lists.

- `XZATOMA_WATCHER_EVENT_TYPES`
  Comma-separated list of event types to process (e.g. `deployment.success,ci.pipeline.completed`).
  Example:
  ```bash
  export XZATOMA_WATCHER_EVENT_TYPES="deployment.success,ci.pipeline.completed"
  ```

- `XZATOMA_WATCHER_SOURCE_PATTERN`
  Regex pattern to filter the `source` field of incoming CloudEvents.
  Example:
  ```bash
  export XZATOMA_WATCHER_SOURCE_PATTERN="^xzepr\\.receiver\\.prod\\."
  ```

- `XZATOMA_WATCHER_PLATFORM_ID`
  Filter events by platform identifier (string).
  Example:
  ```bash
  export XZATOMA_WATCHER_PLATFORM_ID="kubernetes"
  ```

- `XZATOMA_WATCHER_PACKAGE`
  Filter events by package name (string).
  Example:
  ```bash
  export XZATOMA_WATCHER_PACKAGE="my-service-package"
  ```

- `XZATOMA_WATCHER_API_VERSION`
  Filter by `api_version` string.
  Example:
  ```bash
  export XZATOMA_WATCHER_API_VERSION="v1beta"
  ```

- `XZATOMA_WATCHER_SUCCESS_ONLY`
  Whether to process only successful events. Accepts `true` or `false`.
  Example:
  ```bash
  export XZATOMA_WATCHER_SUCCESS_ONLY="true"
  ```

----

## Logging Configuration

These variables control the watcher's logging behavior.

- `XZATOMA_WATCHER_LOG_LEVEL`
  Log level: `trace`, `debug`, `info`, `warn`, `error`.
  Example:
  ```bash
  export XZATOMA_WATCHER_LOG_LEVEL="debug"
  ```

- `XZATOMA_WATCHER_LOG_FILE`
  Path to write log file. If omitted, logs are written to STDOUT.
  Example:
  ```bash
  export XZATOMA_WATCHER_LOG_FILE="/var/log/xzatoma/watcher.log"
  ```

- `XZATOMA_WATCHER_JSON_LOGS`
  Enable JSON formatted logs (`true`/`false`).
  Example:
  ```bash
  export XZATOMA_WATCHER_JSON_LOGS="true"
  ```

- `XZATOMA_WATCHER_INCLUDE_PAYLOAD`
  Include full CloudEvent payload in logs (`true`/`false`). Useful for debugging but may increase log volume.
  Example:
  ```bash
  export XZATOMA_WATCHER_INCLUDE_PAYLOAD="true"
  ```

----

## Execution Configuration

Control runtime execution limits and safety settings.

- `XZATOMA_WATCHER_ALLOW_DANGEROUS`
  Allow potentially dangerous operations in executed plans. Boolean: `true`/`false`.
  Example:
  ```bash
  export XZATOMA_WATCHER_ALLOW_DANGEROUS="false"
  ```

- `XZATOMA_WATCHER_MAX_CONCURRENT`
  Maximum number of concurrent plan executions (integer).
  Example:
  ```bash
  export XZATOMA_WATCHER_MAX_CONCURRENT="5"
  ```

- `XZATOMA_WATCHER_EXECUTION_TIMEOUT`
  Execution timeout in seconds (integer).
  Example:
  ```bash
  export XZATOMA_WATCHER_EXECUTION_TIMEOUT="600"
  ```

----

## Examples

Set a production Kafka connection with SASL and enable a small number
of concurrent executions:

```bash
export XZEPR_KAFKA_BROKERS="kafka-1.prod:9093,kafka-2.prod:9093"
export XZEPR_KAFKA_TOPIC="xzepr.production.events"
export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
export XZEPR_KAFKA_SASL_MECHANISM="SCRAM-SHA-256"
export XZEPR_KAFKA_SASL_USERNAME="xzatoma-consumer"
export XZEPR_KAFKA_SASL_PASSWORD="(secret from secret manager)"
export XZATOMA_WATCHER_MAX_CONCURRENT="10"
export XZATOMA_WATCHER_LOG_LEVEL="warn"
```

Start the watcher in dry-run mode (parse plans but don't execute them):

```bash
xzatoma watch --config config/watcher.yaml --dry-run
```

Systemd service snippet example (keep secrets out of committed unit files):

```ini
[Service]
Environment="XZEPR_KAFKA_BROKERS=kafka-1.prod:9093"
Environment="XZEPR_KAFKA_TOPIC=xzepr.production.events"
EnvironmentFile=/etc/xzatoma/secret.env  # file must be owned and readable only by privileged user
ExecStart=/usr/bin/xzatoma watch --config /etc/xzatoma/config.yaml
```

----

## Troubleshooting & Best Practices

- If watcher fails to start with a missing Kafka configuration error, ensure
  either `watcher.kafka` is configured in your YAML file or the required
  `XZEPR_KAFKA_*` environment variables are set.

- Do not commit secrets (passwords, private keys) to version control. Use a
  secret manager, environment injection via orchestration, or a protected
  file with strict permissions.

- If using SASL, both `XZEPR_KAFKA_SASL_USERNAME` and `XZEPR_KAFKA_SASL_PASSWORD`
  must be available at runtime. Missing credentials will cause connection
  failure.

- For TLS/SSL, verify that `XZEPR_KAFKA_SSL_CA_LOCATION` (and cert/key paths
  if client certs are required) are correct and accessible by the process.

- For high throughput, tune `XZATOMA_WATCHER_MAX_CONCURRENT` in combination
  with `XZATOMA_WATCHER_EXECUTION_TIMEOUT` to avoid resource exhaustion.

----

## References

- Example configuration files: `config/watcher.yaml`
- How-to guide: `docs/how-to/setup_watcher.md`
- Watcher implementation details: `docs/explanation/phase3_configuration_and_documentation.md`
