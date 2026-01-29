# How to Set Up XZatoma Watcher

## Overview

This guide walks you through setting up the XZatoma watcher to consume CloudEvents from Kafka, filter events, extract plans, and execute them. It covers prerequisites, basic configuration, environment-variable alternatives, common advanced scenarios (SASL/SSL, concurrency, dry-run), and troubleshooting tips.

## Prerequisites

- A running Kafka/Redpanda cluster reachable by the host running XZatoma.
- Access to the CloudEvents topic produced by XZepr (or equivalent producer).
- XZatoma binary installed and available in your PATH (or run from source with `cargo run -- ...`).
- For production: a strategy for injecting secrets (secret manager, environment injection by orchestration, etc.).

## Basic Setup

1. Create a watcher configuration file (recommended location: `config/watcher.yaml`).
2. Configure Kafka connection, event filters, logging, and execution options.
3. Start the watcher using the CLI or your process manager.

### Example configuration (minimal)

```yaml
watcher:
  kafka:
    brokers: "localhost:9092"
    topic: "xzepr.events"
    group_id: "xzatoma-watcher"
  filters:
    event_types:
      - "deployment.success"
```

Save the configuration and start the watcher:

```bash
xzatoma watch --config config/watcher.yaml
# Optional runtime flags:
#   --topic <topic>              Override configured topic
#   -e, --event-types <types>    Comma-separated event types
#   -f, --filter-config <path>   (Reserved) extra filter file
#   --log-file <path>            Write logs to file
#   --json-logs                  Use JSON formatted logs
#   --dry-run                    Parse plans but do not execute
```

You can also supply Kafka configuration through environment variables (useful in containerized or managed deployments). See the environment variables reference: `docs/reference/watcher_environment_variables.md`.

## Configure Event Filters

Filter configuration lets you process only relevant CloudEvents:

```yaml
watcher:
  filters:
    event_types:
      - "ci.pipeline.completed"
      - "deployment.failure"
    success_only: false
    source_pattern: "^xzepr\\.receiver\\."
    platform_id: "kubernetes"
```

Alternatively, set `XZATOMA_WATCHER_EVENT_TYPES` as a comma-separated environment variable to override configured filters at runtime.

## Starting the Watcher

- Local development (using config file):

```bash
xzatoma watch --config config/watcher.yaml
```

- With environment-provided Kafka settings (no watcher.kafka in config):

```bash
export XZEPR_KAFKA_BROKERS="kafka1:9092,kafka2:9092"
export XZEPR_KAFKA_TOPIC="xzepr.events"
xzatoma watch --config config/watcher.yaml
```

- Dry-run (parse plans but don't execute them):

```bash
xzatoma watch --config config/watcher.yaml --dry-run
```

- Quick override for event types:

```bash
xzatoma watch --config config/watcher.yaml -e "deployment.success,ci.pipeline.completed"
```

## Advanced Configuration

### Filter by Multiple Criteria

Combine `event_types`, `source_pattern`, `platform_id`, and `api_version` to express precise selection rules. Filters are conjunctive — an event must match all non-empty criteria to be processed.

### Secure Connection with SASL/SSL

For production Kafka clusters, prefer `SASL_SSL` with SCRAM or PLAIN:

```yaml
watcher:
  kafka:
    brokers: "kafka-1.prod:9093,kafka-2.prod:9093"
    topic: "xzepr.production.events"
    security:
      protocol: "SASL_SSL"
      sasl_mechanism: "SCRAM-SHA-256"
      sasl_username: "xzatoma-consumer"
      # sasl_password should be provided via environment:
      # export XZEPR_KAFKA_SASL_PASSWORD="(secret)"
```

Security best practices:

- Never commit passwords or private keys to version control.
- Use a secrets manager or orchestrator to inject sensitive data.
- Validate CA/cert/key paths and file permissions when using SSL.

### Concurrent Execution

Tune concurrency to match available compute and downstream system capacity:

```yaml
watcher:
  execution:
    max_concurrent_executions: 5
    execution_timeout_secs: 600
```

The watcher enforces concurrency using a semaphore. If you increase concurrency, ensure the agents and target systems have sufficient resources.

### Dry Run Mode

Use `--dry-run` to validate plan extraction and filter behavior without executing plans. This is useful during initial setup and testing.

## Validation and Testing

- Start in dry-run and monitor logs for plan extraction:

```bash
xzatoma watch --config config/watcher.yaml --dry-run --json-logs
```

- Publish a test CloudEvent to the topic (example using `kcat`):

```bash
cat <<EOF | kcat -b kafka:9092 -t xzepr.events -P -C -K:
{
  "id": "test-event-1",
  "source": "xzepr.unit.test",
  "specversion": "1.0",
  "type": "deployment.success",
  "data": {
    "plan": "steps:\n  - run: echo Hello World\n"
  }
}
EOF
```

- Inspect logs to confirm the event was filtered, a plan was extracted, and (if not dry-run) the plan executed.

## Troubleshooting

- "Kafka configuration is required" error
  Ensure either `watcher.kafka` exists in `config/watcher.yaml` or appropriate `XZEPR_KAFKA_*` environment variables are set.

- SASL/SSL authentication failure
  Check that `XZEPR_KAFKA_SASL_USERNAME` and `XZEPR_KAFKA_SASL_PASSWORD` are available at runtime and that the correct `XZEPR_KAFKA_SECURITY_PROTOCOL` is set. For SSL, ensure `XZEPR_KAFKA_SSL_CA_LOCATION` (and client certificate if used) are correct.

- No plans extracted from events
  Confirm the event payload structure matches expectation (plan in `data.plan`, or event payload shape your PlanExtractor supports). Use `XZATOMA_WATCHER_INCLUDE_PAYLOAD=true` to log payloads for investigation (be careful with sensitive content).

- Logs are not helpful
  Increase verbosity with `XZATOMA_WATCHER_LOG_LEVEL=debug` or enable JSON logs with `--json-logs` or `XZATOMA_WATCHER_JSON_LOGS=true`.

- Execution tasks fail or hang
  Adjust `watcher.execution.execution_timeout_secs` and tune `max_concurrent_executions` to manage resources.

## References

- Example configuration: `config/watcher.yaml`
- Environment variables: `docs/reference/watcher_environment_variables.md`
- Implementation summary and Phase 3 details: `docs/explanation/phase3_configuration_and_documentation.md`
- CLI help: `xzatoma watch --help`

## Final Notes

- Use configuration files for stable settings and environment variables for secrets or environment-specific overrides.
- Validate changes in a test environment with `--dry-run` before switching to production execution.
- Review watcher logs and set up appropriate monitoring/alerting for production deployments.
