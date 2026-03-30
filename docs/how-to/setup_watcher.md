# How to Set Up XZatoma Watcher

## Overview

This guide walks you through setting up the XZatoma watcher to consume events
from Kafka-compatible brokers, filter or match them, extract plans, and execute
them.

XZatoma supports two watcher backends:

- `xzepr` for XZepr CloudEvents-style messages
- `generic` for generic JSON plan-event messages

This guide covers:

- prerequisites
- minimal configuration
- watcher backend selection
- XZepr watcher setup
- generic watcher setup
- CLI examples
- output topic behavior
- dry-run testing
- troubleshooting

## Prerequisites

Before starting, make sure you have:

- a running Kafka or Redpanda cluster reachable from the machine running XZatoma
- the `xzatoma` binary installed and available in your `PATH`, or the source
  checked out so you can run it with `cargo run -- ...`
- access to the topic you want to consume from
- for production deployments, a safe way to provide secrets such as SASL
  passwords

## Choosing a Watcher Backend

Use `watcher_type` to choose which watcher backend to run.

### Use `xzepr` when

Choose `xzepr` when your upstream producer emits XZepr CloudEvents payloads and
you want to filter on XZepr-specific fields such as:

- `event_types`
- `source_pattern`
- `platform_id`
- `package`
- `api_version`
- `success_only`

### Use `generic` when

Choose `generic` when your upstream producer emits generic JSON plan-event
messages and you want to match on:

- `action`
- `name`
- `version`

The generic watcher expects events in the `GenericPlanEvent` format described
later in this guide.

## Basic Watcher Setup

1. Create or update a configuration file.
2. Set the watcher backend type.
3. Configure Kafka connection settings.
4. Add either XZepr filters or generic match rules.
5. Start the watcher in dry-run mode first.
6. Verify logs and payload handling before enabling live execution.

## Minimal XZepr Watcher Configuration

Use this when consuming XZepr CloudEvents.

```yaml
watcher:
  watcher_type: xzepr
  kafka:
    brokers: "localhost:9092"
    topic: "xzepr.events"
    group_id: "xzatoma-watcher"
  filters:
    event_types:
      - "deployment.success"
  logging:
    level: "info"
    json_format: true
  execution:
    allow_dangerous: false
    max_concurrent_executions: 1
    execution_timeout_secs: 300
```

Start it with:

```bash
xzatoma watch --config config/watcher.yaml
```

## Using the Generic Watcher

The generic watcher consumes plan-event JSON messages from Kafka-compatible
topics. It matches events using optional regex criteria and executes the
embedded plan when a matching message is received.

### Minimal CLI Example

This is the simplest way to launch the generic watcher from the command line:

```bash
xzatoma watch --watcher-type generic --topic plans.events --action deploy
```

This means:

- use the `generic` watcher backend
- consume from `plans.events`
- process only events whose `action` matches `deploy` case-insensitively

### Minimal Generic Watcher Configuration

```yaml
watcher:
  watcher_type: generic
  kafka:
    brokers: "localhost:9092"
    topic: "plans.events"
    group_id: "xzatoma-generic-watcher"
    output_topic: "plans.results"
  generic_match:
    action: "deploy"
  logging:
    level: "info"
    json_format: true
  execution:
    allow_dangerous: false
    max_concurrent_executions: 1
    execution_timeout_secs: 300
```

You can also use the example configuration file:

```bash
xzatoma watch --config config/generic_watcher.yaml
```

## Generic Watcher Output Topic Behavior

The generic watcher publishes `GenericPlanResult` messages after execution.

### Same input and output topic

If `watcher.kafka.output_topic` is omitted, the generic watcher publishes
results back to the input topic:

```yaml
watcher:
  watcher_type: generic
  kafka:
    brokers: "localhost:9092"
    topic: "plans.events"
    group_id: "xzatoma-generic-watcher"
```

This is safe because:

- input events must use `event_type: "plan"`
- result events always use `event_type: "result"`
- the generic watcher rejects any event where `event_type != "plan"`

That means the watcher can consume its own result messages and immediately skip
them without re-triggering execution.

### Separate output topic

If you want cleaner separation between trigger events and result events, set a
dedicated output topic:

```yaml
watcher:
  watcher_type: generic
  kafka:
    brokers: "localhost:9092"
    topic: "plans.events"
    output_topic: "plans.results"
    group_id: "xzatoma-generic-watcher"
```

## Generic Watcher Match Modes

The generic watcher supports these match configurations:

### Action only

```yaml
watcher:
  watcher_type: generic
  generic_match:
    action: "deploy"
```

### Name and version

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: "service-a"
    version: "^v1\\.[0-9]+$"
```

### Name and action

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: "service-a"
    action: "deploy.*"
```

### Name, version, and action

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: "service-a"
    version: "^v1\\.[0-9]+$"
    action: "deploy.*"
```

### Accept-all mode

If you omit all generic match fields, every event with `event_type: "plan"` is
accepted:

```yaml
watcher:
  watcher_type: generic
  generic_match:
    action:
    name:
    version:
```

This is valid, but it is usually better to configure at least one match field in
production.

## GenericPlanEvent Producer Example

A producer can trigger the generic watcher by publishing a JSON message like
this:

```json
{
  "id": "01JTEST0000000000000000001",
  "event_type": "plan",
  "name": "service-a",
  "version": "v1.2.3",
  "action": "deploy",
  "plan": {
    "name": "Deploy service-a",
    "steps": [
      {
        "name": "apply manifests",
        "action": "kubectl apply -f manifests/"
      }
    ]
  },
  "timestamp": "2026-01-24T12:00:00Z",
  "metadata": {
    "environment": "staging"
  }
}
```

Important notes:

- `event_type` must be exactly `"plan"`
- `plan` may be a string, object, or array
- `action`, `name`, and `version` are optional unless required by your watcher
  configuration

## CLI Options

The `watch` command supports runtime overrides.

### Shared watcher options

```bash
xzatoma watch --config config/watcher.yaml --topic custom.topic --dry-run
```

Useful shared options:

- `--topic` override the configured Kafka topic
- `--brokers <ADDRS>` -- override Kafka broker addresses (comma-separated).
  Takes precedence over config file.
- `--create-topics` -- automatically create missing Kafka topics at watcher
  startup. Useful for local development with Redpanda or Kafka.
- `--log-file` write logs to a file
- `--json-logs` enable JSON logging
- `--dry-run` parse and classify plans without executing them

### Generic watcher options

```bash
xzatoma watch \
  --watcher-type generic \
  --topic plans.events \
  --output-topic plans.results \
  --action deploy.* \
  --name service-a \
  --dry-run
```

Generic-specific options:

- `--watcher-type generic`
- `--output-topic <topic>`
- `--action <regex>`
- `--name <regex>`

### XZepr watcher options

```bash
xzatoma watch \
  --watcher-type xzepr \
  --topic xzepr.events \
  -e deployment.success,ci.pipeline.completed \
  --dry-run
```

XZepr-specific filter overrides include:

- `-e, --event-types <comma-separated-list>`

## Using Environment Variables

You can provide watcher settings through environment variables instead of, or in
addition to, a config file.

### Generic watcher example

```bash
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.events"
export XZATOMA_WATCHER_OUTPUT_TOPIC="plans.results"
export XZATOMA_WATCHER_MATCH_ACTION="deploy"
xzatoma watch --config config/config.yaml --dry-run
```

### XZepr watcher example

```bash
export XZATOMA_WATCHER_TYPE="xzepr"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="xzepr.events"
export XZATOMA_WATCHER_EVENT_TYPES="deployment.success"
xzatoma watch --config config/config.yaml --dry-run
```

See the full variable reference in
`docs/reference/watcher_environment_variables.md`.

## Secure Kafka Configuration with SASL or SSL

For production deployments, prefer secure Kafka settings.

```yaml
watcher:
  kafka:
    brokers: "kafka-1.prod:9093,kafka-2.prod:9093"
    topic: "plans.production.input"
    output_topic: "plans.production.output"
    group_id: "xzatoma-generic-watcher-prod"
    security:
      protocol: "SASL_SSL"
      sasl_mechanism: "SCRAM-SHA-256"
      sasl_username: "xzatoma-consumer"
      sasl_password: "set-through-environment-in-real-deployments"
```

Security guidance:

- do not commit secrets to version control
- prefer environment injection or a secret manager
- use `SASL_SSL` in production where possible

## Dry-Run Testing

Always start with dry-run mode first.

### XZepr watcher dry-run

```bash
xzatoma watch --config config/watcher.yaml --watcher-type xzepr --dry-run
```

### Generic watcher dry-run

```bash
xzatoma watch --config config/generic_watcher.yaml --dry-run
```

In dry-run mode, the watcher still:

- loads configuration
- initializes logging
- validates filters or match rules
- processes and classifies matching messages

But it does not execute the embedded plan.

## Troubleshooting

### Kafka configuration is required

If you see a startup error about missing Kafka configuration:

- make sure `watcher.kafka` exists in your YAML file
- or provide `XZEPR_KAFKA_*` environment variables

### Generic watcher does not process events

Check the following:

- `watcher_type` is set to `generic`
- incoming payload uses `event_type: "plan"`
- your `action`, `name`, or `version` values actually match the configured regex
- the event fields are present when required by your matcher

### Generic watcher appears to ignore result messages

That is expected behavior. Result events use `event_type: "result"` and are
silently discarded by the generic watcher to prevent same-topic loops.

### XZepr watcher does not process expected events

Check:

- the configured `event_types`
- `source_pattern`
- `platform_id`
- `package`
- `api_version`
- `success_only`

If any configured filter does not match, the event is skipped.

### Logging is not detailed enough

Increase logging verbosity:

```bash
export XZATOMA_WATCHER_LOG_LEVEL="debug"
xzatoma watch --config config/watcher.yaml --dry-run
```

### Payload debugging

If you need more insight into incoming XZepr messages, enable payload logging:

```bash
export XZATOMA_WATCHER_INCLUDE_PAYLOAD="true"
```

Use that carefully in environments where payloads may contain sensitive data.

## Recommended Workflow

For a new deployment:

1. start with a config file
2. choose `watcher_type`
3. configure Kafka brokers, topic, and group ID
4. add the narrowest useful filters or match rules
5. run with `--dry-run`
6. verify logs and message handling
7. remove `--dry-run` only after validation

## References

- example generic watcher config: `config/generic_watcher.yaml`
- environment variable reference:
  `docs/reference/watcher_environment_variables.md`
- configuration reference: `docs/reference/configuration.md`
- architecture reference: `docs/reference/architecture.md`

## Final Notes

The watcher system is designed so both backends are first-class options selected
by configuration. Use:

- `xzepr` for XZepr CloudEvents workflows
- `generic` for generic plan-event workflows

If you are onboarding a new producer that is not tied to XZepr, the generic
watcher is the right place to start.
