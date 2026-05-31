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

Choose `generic` when your upstream producer emits standard **CloudEvents 1.0**
envelopes containing a plan in the `data` field, and you want to match on:

- `action`
- `name`
- `version`

The generic watcher requires CloudEvents 1.0 format (see Producer Example
below). It has no XZepr-specific fields and works with any CloudEvents-capable
producer.

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

This is safe because result events published by the watcher are plain JSON (not
CloudEvents envelopes). When consumed back, they fail to parse at the
CloudEvents boundary and are silently discarded as `InvalidPayload` without
triggering a new execution.

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

If you omit all generic match fields, every valid CloudEvents message whose
`data` parses as a Plan is accepted:

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

## Generic Watcher Producer Example

The generic watcher requires **standard CloudEvents 1.0** format. A producer
publishes a JSON object with these attributes and the plan in the `data` field:

```json
{
  "id": "01JTEST0000000000000000001",
  "specversion": "1.0",
  "type": "xzatoma.plan.execute",
  "source": "my-ci-system",
  "time": "2026-01-24T12:00:00Z",
  "datacontenttype": "application/json",
  "data": {
    "name": "service-a",
    "version": "v1.2.3",
    "action": "deploy",
    "tasks": [
      {
        "id": "apply-manifests",
        "description": "Apply Kubernetes manifests: kubectl apply -f manifests/"
      }
    ]
  }
}
```

Required CloudEvents attributes: `id`, `specversion` (`"1.0"`), `type`,
`source`, `data`.

The `data` field must contain a valid Plan. Plans support two formats:

**Task-based** (preferred for the generic watcher):

```json
{
  "name": "deploy",
  "action": "deploy",
  "tasks": [
    { "id": "step-1", "description": "Full agent instruction for this step." }
  ]
}
```

**Step-based** (legacy format, backward compatible):

```json
{
  "name": "deploy",
  "steps": [{ "name": "apply", "action": "kubectl apply -f manifests/" }]
}
```

The `action`, `name`, and `version` fields inside `data` are used by
`generic_match` for filtering. They are optional unless required by your matcher
configuration.

> **Contrast with XZepr**: The XZepr watcher uses a different CloudEvents schema
> with extensions (`success`, `api_version`, `platform_id`, `release`,
> `package`) that are XZepr-specific and absent from the generic format.

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

## Plan Execution Model

XZatoma supports two execution modes for task-based plans. Configure the mode in
the `execution:` block of your watcher config.

### per_task (default)

Each task in `plan.tasks` is sent to the agent as a separate prompt within a
shared session. The agent retains conversation history between tasks, so later
tasks can reference outputs produced by earlier ones. Task execution order
respects the `dependencies` field via a topological sort.

```yaml
watcher:
  execution:
    execution_mode: per_task
```

Use `per_task` when:

- The plan has multiple tasks with dependencies.
- Later tasks need context from earlier task outputs.
- You want structured per-task outcome data in the result event.

### single_shot (legacy)

The full plan is collapsed into a single numbered prompt and sent to the agent
once. This is the pre-Phase-1 behaviour. Use it when your plan has no structured
tasks, only free-form `steps`, or when backward-compatible single-shot execution
is required by your operator policy.

```yaml
watcher:
  execution:
    execution_mode: single_shot
```

### Runtime override

Override the configured mode at runtime with the
`XZATOMA_WATCHER_EXECUTION_MODE` environment variable:

```bash
export XZATOMA_WATCHER_EXECUTION_MODE=single_shot
xzatoma watch --config config.yaml
```

Accepted values: `per_task`, `single_shot`.

### Per-task outcome data

When `execution_mode` is `per_task` and the plan has tasks, the result event
includes a `task_outcomes` array. Each entry records the task `id`, whether it
succeeded, a summary of the agent response, and the number of LLM iterations:

```json
{
  "task_outcomes": [
    { "id": "setup", "success": true, "summary": "...", "iterations": 2 },
    { "id": "build", "success": true, "summary": "...", "iterations": 3 }
  ]
}
```

## Troubleshooting

### Kafka configuration is required

If you see a startup error about missing Kafka configuration:

- make sure `watcher.kafka` exists in your YAML file
- or provide `XZEPR_KAFKA_*` environment variables

### Generic watcher does not process events

Check the following:

- `watcher_type` is set to `generic`
- the incoming message is a valid **CloudEvents 1.0** JSON object with `id`,
  `specversion`, `type`, `source`, and `data` fields
- `data` contains a valid Plan (has a `name` field and at least one `task` or
  `step`)
- your `action`, `name`, or `version` values in the Plan `data` actually match
  the configured regex

### Generic watcher appears to ignore result messages

That is expected behavior. Result events published by the watcher are not
CloudEvents envelopes — they fail parsing at the CloudEvents boundary and are
silently discarded as `InvalidPayload` to prevent same-topic re-trigger loops.

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

### Agent completes in 1 iteration without using tools

If you see agent execution log lines indicating only 1 iteration and no tool
calls, check two things:

1. **Execution mode**: confirm `execution_mode: per_task` is set in
   `watcher.execution`. The default is `per_task` but an explicit `single_shot`
   setting collapses the plan into one prompt where the LLM may describe tasks
   rather than executing them.

2. **Watcher system prompt**: the watcher uses `ChatMode::Watcher`, which
   injects an autonomous system prompt telling the LLM to act immediately
   without confirmation. If you are running a custom provider or model that
   ignores system prompts, the LLM may default to chat-assistant behaviour and
   respond with a description instead of tool calls.

To diagnose, enable debug logging and inspect the first system message:

```bash
export XZATOMA_WATCHER_LOG_LEVEL=debug
xzatoma watch --config config.yaml --dry-run
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
