# Configuration Reference

## Overview

This document describes XZatoma's configuration model, how configuration is
loaded, the major configuration sections available in YAML, supported runtime
overrides, and the validation rules that apply at startup.

Configuration is resolved in this order:

1. defaults built into the application
2. values loaded from the YAML config file
3. environment variable overrides
4. CLI overrides where supported

This means a value passed through the command line has the highest precedence,
followed by environment variables, then the configuration file, then defaults.

## Default Config Path and Loading

By default, the CLI loads configuration from:

- `config/config.yaml`

You can override that path with:

```bash
xzatoma --config /path/to/config.yaml chat
```

If the file does not exist, XZatoma falls back to built-in defaults and then
applies any environment-variable or CLI overrides.

## Top-Level Configuration Structure

A typical configuration file includes these top-level sections:

- `provider`
- `agent`
- `watcher`
- `mcp`

Example:

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 50
  timeout_seconds: 300

watcher:
  watcher_type: xzepr

mcp:
  auto_connect: true
```

## Provider Configuration

The `provider` section controls which AI backend XZatoma uses.

### Fields

- `type`

  - Type: string
  - Accepted values:
    - `copilot`
    - `ollama`

- `copilot`

  - Copilot-specific configuration

- `ollama`
  - Ollama-specific configuration

### Example

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5-mini
```

Or:

```yaml
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest
```

### Copilot Configuration

#### Fields

- `model`

  - Type: string
  - Default: `gpt-5-mini`

- `api_base`

  - Type: string or null
  - Optional custom API base URL

- `enable_streaming`

  - Type: boolean
  - Default: `true`

- `enable_endpoint_fallback`

  - Type: boolean
  - Default: `true`

- `reasoning_effort`

  - Type: string or null
  - Optional values typically include:
    - `low`
    - `medium`
    - `high`

- `include_reasoning`
  - Type: boolean
  - Default: `false`

### Ollama Configuration

#### Fields

- `host`

  - Type: string
  - Default: `http://localhost:11434`

- `model`
  - Type: string
  - Default: `llama3.2:latest`

## Agent Configuration

The `agent` section controls execution behavior, conversation management, tool
limits, terminal behavior, chat defaults, and subagent settings.

### Common Fields

- `max_turns`

  - Type: integer
  - Default: `50`

- `timeout_seconds`

  - Type: integer
  - Default: `300`

- `conversation`

  - Conversation window and summarization settings

- `tools`

  - Tool-related size and limit settings

- `terminal`

  - Terminal execution settings

- `chat`

  - Chat mode defaults

- `subagent`
  - Subagent delegation settings

### Example

```yaml
agent:
  max_turns: 50
  timeout_seconds: 300
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8
    warning_threshold: 0.85
    auto_summary_threshold: 0.9
  terminal:
    timeout_seconds: 30
```

## Conversation Configuration

The `agent.conversation` section controls context window usage and
summarization.

### Fields

- `max_tokens`

  - Type: integer
  - Default: `100000`

- `min_retain_turns`

  - Type: integer
  - Default: `5`

- `prune_threshold`

  - Type: float
  - Default: `0.8`

- `warning_threshold`

  - Type: float
  - Default: `0.85`

- `auto_summary_threshold`

  - Type: float
  - Default: `0.90`

- `summary_model`
  - Type: string or null
  - Optional override used for summaries

### Example

```yaml
agent:
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8
    warning_threshold: 0.85
    auto_summary_threshold: 0.9
    summary_model: gpt-5-mini
```

## Watcher Configuration

The `watcher` section configures Kafka-backed event monitoring and plan
execution.

XZatoma supports two watcher backends:

- `xzepr`
- `generic`

These backends are selected through `watcher_type`.

### WatcherConfig Fields

- `watcher_type`
- `kafka`
- `generic_match`
- `filters`
- `logging`
- `execution`

### Example

```yaml
watcher:
  watcher_type: xzepr
  kafka:
    brokers: localhost:9092
    topic: xzepr.events
    group_id: xzatoma-watcher
```

## `watcher_type`

Selects which watcher backend is active.

### Fields

- Type: string
- Accepted values:
  - `xzepr`
  - `generic`
- Default: `xzepr`

### Behavior

- If omitted, XZatoma defaults to `xzepr`.
- Existing watcher configs that do not specify `watcher_type` continue to work
  unchanged.
- `xzepr` uses `watcher.filters`.
- `generic` uses `watcher.generic_match`.

### Example

```yaml
watcher:
  watcher_type: xzepr
```

Or:

```yaml
watcher:
  watcher_type: generic
```

## Kafka Watcher Configuration

The `watcher.kafka` section configures the Kafka connection used by watcher
backends.

### Fields

- `brokers`

  - Type: string
  - Comma-separated broker addresses

- `topic`

  - Type: string
  - Input topic to consume from

- `output_topic`

  - Type: string or null
  - Generic watcher result topic
  - If omitted, results are published back to `topic`

- `group_id`

  - Type: string
  - Consumer group ID
  - Default: `xzatoma-watcher`

- `security`
  - Optional Kafka security configuration

### Example

```yaml
watcher:
  kafka:
    brokers: localhost:9092
    topic: plans.events
    output_topic: plans.results
    group_id: xzatoma-generic-watcher
```

## `kafka.output_topic`

The `output_topic` field is used by the generic watcher when publishing
`GenericPlanResult` messages after plan execution.

### Default Behavior

If `output_topic` is omitted:

- the generic watcher publishes results back to the input `topic`

This is safe because:

- generic trigger events must use `event_type: "plan"`
- generic result events always use `event_type: "result"`
- the generic watcher rejects every event where `event_type != "plan"`

### Example Using a Separate Output Topic

```yaml
watcher:
  watcher_type: generic
  kafka:
    brokers: localhost:9092
    topic: plans.events
    output_topic: plans.results
    group_id: xzatoma-generic-watcher
```

### Example Using the Same Topic

```yaml
watcher:
  watcher_type: generic
  kafka:
    brokers: localhost:9092
    topic: plans.events
    group_id: xzatoma-generic-watcher
```

## Kafka Security Configuration

The `watcher.kafka.security` section controls connection security.

### Fields

- `protocol`

  - Type: string
  - Accepted values:
    - `PLAINTEXT`
    - `SSL`
    - `SASL_PLAINTEXT`
    - `SASL_SSL`

- `sasl_mechanism`

  - Type: string or null
  - Accepted values:
    - `PLAIN`
    - `SCRAM-SHA-256`
    - `SCRAM-SHA-512`

- `sasl_username`

  - Type: string or null

- `sasl_password`
  - Type: string or null

### Example

```yaml
watcher:
  kafka:
    brokers: kafka-1.prod:9093,kafka-2.prod:9093
    topic: plans.production.input
    output_topic: plans.production.output
    group_id: xzatoma-generic-watcher-prod
    security:
      protocol: SASL_SSL
      sasl_mechanism: SCRAM-SHA-256
      sasl_username: xzatoma-consumer
      sasl_password: set-through-env-in-production
```

## XZepr Watcher Filters

The `watcher.filters` section applies only when:

- `watcher_type: xzepr`

These filters are specific to XZepr CloudEvents.

### Fields

- `event_types`

  - Type: array of strings
  - Default: empty list

- `source_pattern`

  - Type: string or null

- `platform_id`

  - Type: string or null

- `package`

  - Type: string or null

- `api_version`

  - Type: string or null

- `success_only`
  - Type: boolean
  - Default: `true`

### Example

```yaml
watcher:
  watcher_type: xzepr
  filters:
    event_types:
      - deployment.success
      - ci.pipeline.completed
    source_pattern: "^xzepr\\.receiver\\."
    platform_id: kubernetes
    package: my-service
    api_version: v1
    success_only: true
```

## `generic_match`

The `watcher.generic_match` section applies only when:

- `watcher_type: generic`

It controls which generic plan events are processed by the generic watcher.

### Fields

- `action`

  - Type: string or null
  - Regex matched against the event `action` field
  - Case-insensitive by default

- `name`

  - Type: string or null
  - Regex matched against the event `name` field
  - Case-insensitive by default

- `version`
  - Type: string or null
  - Regex matched against the event `version` field
  - Case-insensitive by default

### Example

```yaml
watcher:
  watcher_type: generic
  generic_match:
    action: deploy
```

## Generic Match Modes

The generic watcher supports these matching modes depending on which fields are
set.

### Action only

```yaml
watcher:
  watcher_type: generic
  generic_match:
    action: deploy
```

Runtime behavior:

- event `action` must match `deploy`

### Name and version

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: service-a
    version: "^v1\\.[0-9]+$"
```

Runtime behavior:

- event `name` must match `service-a`
- event `version` must match `^v1\.[0-9]+$`

### Name and action

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: service-a
    action: deploy.*
```

Runtime behavior:

- event `name` must match `service-a`
- event `action` must match `deploy.*`

### Name, version, and action

```yaml
watcher:
  watcher_type: generic
  generic_match:
    name: service-a
    version: "^v1\\.[0-9]+$"
    action: deploy.*
```

Runtime behavior:

- event `name` must match
- event `version` must match
- event `action` must match

### Accept-all mode

If all generic match fields are omitted or null, the generic watcher accepts
every event where:

- `event_type == "plan"`

Example:

```yaml
watcher:
  watcher_type: generic
  generic_match:
    action:
    name:
    version:
```

This is valid, but XZatoma emits a warning because accept-all mode may be
unintentional in production.

## Watcher Logging Configuration

The `watcher.logging` section controls watcher-specific logging behavior.

### Fields

- `level`

  - Type: string
  - Default: `info`

- `json_format`

  - Type: boolean
  - Default: `true`

- `file_path`

  - Type: string or null

- `include_payload`
  - Type: boolean
  - Default: `false`

### Example

```yaml
watcher:
  logging:
    level: debug
    json_format: true
    file_path: /var/log/xzatoma/watcher.log
    include_payload: false
```

## Watcher Execution Configuration

The `watcher.execution` section controls execution behavior for
watcher-triggered plans.

### Fields

- `allow_dangerous`

  - Type: boolean
  - Default: `false`

- `max_concurrent_executions`

  - Type: integer
  - Default: `1`

- `execution_timeout_secs`
  - Type: integer
  - Default: `300`

### Example

```yaml
watcher:
  execution:
    allow_dangerous: false
    max_concurrent_executions: 5
    execution_timeout_secs: 1800
```

## MCP Configuration

The `mcp` section controls MCP client behavior.

### Common Fields

- `auto_connect`
- `request_timeout_seconds`
- server definitions

### Example

```yaml
mcp:
  auto_connect: true
  request_timeout_seconds: 30
```

## Environment Variable Overrides

Environment variables can override many configuration values at runtime.

Examples:

```bash
export XZATOMA_PROVIDER="copilot"
export XZATOMA_COPILOT_MODEL="gpt-5-mini"
export XZATOMA_WATCHER_TYPE="generic"
export XZEPR_KAFKA_BROKERS="localhost:9092"
export XZEPR_KAFKA_TOPIC="plans.events"
export XZATOMA_WATCHER_OUTPUT_TOPIC="plans.results"
export XZATOMA_WATCHER_MATCH_ACTION="deploy"
```

See the full environment variable reference in:

- `docs/reference/watcher_environment_variables.md`

## Validation Rules

XZatoma validates configuration at startup.

### General Rules

- provider type must be valid
- numeric limits must be positive where required
- conversation thresholds must be within valid ranges
- Kafka config fields cannot be empty when provided

### Generic Watcher Rules

When `watcher_type: generic`:

- `watcher.kafka` must be present
- configured `generic_match` fields must be valid regex patterns
- if all generic match fields are unset, validation succeeds but logs a warning

### XZepr Watcher Rules

When `watcher_type: xzepr`:

- `generic_match` is ignored
- if generic match fields are set anyway, XZatoma logs a debug message noting
  they are unused

## Example Complete Generic Watcher Configuration

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 50
  timeout_seconds: 300

watcher:
  watcher_type: generic
  kafka:
    brokers: localhost:9092
    topic: plans.events
    output_topic: plans.results
    group_id: xzatoma-generic-watcher
  generic_match:
    action: deploy
    name:
    version:
  logging:
    level: info
    json_format: true
    include_payload: false
  execution:
    allow_dangerous: false
    max_concurrent_executions: 1
    execution_timeout_secs: 300
```

## Example Complete XZepr Watcher Configuration

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 50
  timeout_seconds: 300

watcher:
  watcher_type: xzepr
  kafka:
    brokers: localhost:9092
    topic: xzepr.events
    group_id: xzatoma-watcher
  filters:
    event_types:
      - deployment.success
    success_only: true
  logging:
    level: info
    json_format: true
  execution:
    allow_dangerous: false
    max_concurrent_executions: 1
    execution_timeout_secs: 300
```

## Security Guidance

- Avoid storing sensitive passwords directly in committed config files.
- Prefer environment-variable injection or a secret manager for Kafka SASL
  credentials.
- Use secure Kafka protocols such as `SASL_SSL` in production.
- Be cautious with payload logging in sensitive environments.

## Related Documentation

- how-to setup guide: `docs/how-to/setup_watcher.md`
- watcher environment variables:
  `docs/reference/watcher_environment_variables.md`
- architecture reference: `docs/reference/architecture.md`
- generic watcher example config: `config/generic_watcher.yaml`
