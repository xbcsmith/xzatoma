# XZatoma Architecture

## System Overview

XZatoma is an autonomous AI agent CLI written in Rust. It executes tasks through
conversation with AI providers and uses a small set of generic tools such as
file operations, search, terminal execution, and web fetching.

The system is intentionally simple:

- CLI and config drive execution
- providers generate responses and tool calls
- the agent executes tools and continues the conversation loop
- optional watcher backends consume Kafka-compatible events and trigger plans

Two watcher backends are supported as first-class peers:

- `xzepr` for XZepr CloudEvents-style messages
- `generic` for generic JSON plan-event messages

The active watcher backend is selected through `watcher.watcher_type`.

## Top-Level Module Structure

```text
src/
├── main.rs
├── lib.rs
├── cli.rs
├── config.rs
├── error.rs
├── security.rs          # Path and input hardening helpers
├── test_utils.rs        # Shared test helpers (test builds)
├── chat_mode.rs
├── mention_parser.rs
├── prompts/
├── providers/
├── agent/
├── tools/
├── commands/
├── mcp/
├── storage/
├── skills/              # Extensible agent capabilities via discoverable skill definitions
├── acp/                 # Agent Client Protocol (HTTP server + stdio transport)
└── watcher/
    ├── mod.rs
    ├── kafka_security.rs
    ├── lifecycle.rs
    ├── logging.rs
    ├── plan_executor.rs
    ├── topic_admin.rs
    ├── generic/
    └── xzepr/
```

### Key Architectural Areas

- `cli.rs`

  - command-line parsing and user-facing flags

- `config.rs`

  - configuration loading, environment-variable overrides, validation

- `providers/`

  - AI provider abstraction and implementations

- `agent/`

  - conversation loop, tool execution, persistence, quotas, metrics

- `tools/`

  - generic file, grep, fetch, and terminal capabilities

- `commands/`

  - top-level command handlers such as `chat`, `run`, and `watch`

- `watcher/`
  - Kafka-backed watcher infrastructure, including backend dispatch targets

## High-Level Runtime Architecture

```text
User / CLI
   |
   v
main.rs
   |
   v
commands/
   |
   +--> commands::chat
   +--> commands::run
   +--> commands::watch
                 |
                 v
             watcher backend dispatch
                 |
        +--------+--------+
        |                 |
        v                 v
 watcher::xzepr      watcher::generic
```

## Core Execution Architecture

### CLI Layer

The CLI layer is responsible for:

- parsing command-line arguments
- selecting the command path
- loading config
- applying CLI overrides
- invoking the command handler

Important files:

- `src/main.rs`
- `src/cli.rs`
- `src/commands/mod.rs`

### Configuration Layer

The configuration layer is responsible for:

- parsing YAML configuration
- applying environment-variable overrides
- validating the final effective configuration
- selecting watcher backend behavior through `WatcherType`

Important file:

- `src/config.rs`

### Provider Layer

The provider layer abstracts AI backends behind a common interface.

Implemented providers:

- GitHub Copilot
- Ollama
- OpenAI-compatible providers

Important files:

- `src/providers/trait_mod.rs`
- `src/providers/types.rs`
- `src/providers/copilot.rs`
- `src/providers/ollama.rs`
- `src/providers/openai.rs`

### Agent Layer

The agent layer runs the autonomous execution loop.

Responsibilities:

- maintain conversation history
- send messages to the provider
- receive tool calls
- execute tools
- append tool results
- stop on completion or limits

Important files:

- `src/agent/core.rs`
- `src/agent/conversation.rs`
- `src/agent/persistence.rs`

Tool and plan execution for ACP sessions lives in `src/acp/executor.rs`.

### Tools Layer

The tools layer exposes generic capabilities rather than task-specific logic.

Representative capabilities:

- file reads and edits
- search and grep
- terminal execution
- remote content fetch
- planning helpers

Important files:

- `src/tools/mod.rs`
- `src/tools/read_file.rs`
- `src/tools/edit_file.rs`
- `src/tools/grep.rs`
- `src/tools/terminal.rs`
- `src/tools/fetch.rs`
- `src/tools/plan.rs`

## Watcher Architecture

## Watcher Overview

The watcher system connects Kafka-compatible topics to XZatoma plan execution.

Watcher responsibilities include:

- consuming messages from configured topics
- validating and filtering or matching messages
- extracting or reading plans from messages
- executing plans or running in dry-run mode
- publishing results for the generic watcher
- supporting graceful shutdown from the `watch` command

The watcher system is split into:

- shared watcher infrastructure
- XZepr-specific backend
- generic plan-event backend

## Watcher Module Structure

```text
src/watcher/
├── mod.rs
├── kafka_security.rs
├── lifecycle.rs
├── logging.rs
├── plan_executor.rs
├── topic_admin.rs
├── generic/
│   ├── mod.rs
│   ├── consumer.rs
│   ├── event.rs
│   ├── event_handler.rs
│   ├── matcher.rs
│   ├── message.rs
│   ├── result_event.rs
│   ├── result_producer.rs
│   └── watcher.rs
└── xzepr/
    ├── mod.rs
    ├── filter.rs
    ├── plan_extractor.rs
    ├── watcher.rs
    └── consumer/
        ├── mod.rs
        ├── client.rs
        ├── config.rs
        ├── kafka.rs
        └── message.rs
```

## Top-Level Watcher Module

`src/watcher/mod.rs` is the shared entry point for watcher-related code.

It contains:

- `pub mod generic`
- `pub mod kafka_security`
- `pub mod lifecycle`
- `pub mod logging`
- `pub mod plan_executor`
- `pub mod topic_admin`
- `pub mod xzepr`

It also re-exports:

- `XzeprWatcher`

The generic watcher is accessed through:

- `crate::watcher::generic::*`

The XZepr watcher is accessed through:

- `crate::watcher::xzepr::*`

## Canonical XZepr Watcher Path

XZepr implementation code lives under:

- `src/watcher/xzepr/`

Use `crate::watcher::xzepr::*` as the canonical location for XZepr watcher
imports.

## Watcher Backend Selection

Watcher backend selection is configured through:

- `watcher.watcher_type`

The supported values are:

- `xzepr`
- `generic`

Default behavior:

- if omitted, `watcher_type` defaults to `xzepr`

This preserves backward compatibility for existing watcher configuration files.

## Watch Command Dispatch

The `watch` command is the single user-facing entry point for watcher execution.

The dispatch path is:

```text
xzatoma watch
   |
   v
commands::watch::run_watch
   |
   v
match config.watcher.watcher_type
   |
   +--> WatcherType::XZepr   -> watcher::XzeprWatcher::new(...).start().await
   |
   +--> WatcherType::Generic -> watcher::generic::GenericWatcher::new(...).start().await
```

This design keeps the external command surface simple while allowing backend
behavior to differ internally.

## Watcher Dispatch Diagram

```text
commands::watch::run_watch
    |
    +--> apply CLI overrides
    |
    +--> init watcher logging
    |
    +--> inspect config.watcher.watcher_type
            |
            +--> xzepr
            |      |
            |      +--> XzeprWatcher::new(config, dry_run)
            |      +--> watcher.start().await
            |
            +--> generic
                   |
                   +--> GenericWatcher::new(config, dry_run)
                   +--> watcher.start().await
```

Both watcher backends expose the same outer shape:

- constructor taking `Config` and `dry_run`
- `start() -> Result<()>`

This shared surface is deliberate, but there is no shared watcher trait yet.

## XZepr Watcher Backend

## Purpose

The XZepr watcher backend consumes XZepr CloudEvents-style messages from Kafka
and processes them using XZepr-specific filtering and extraction logic.

## XZepr Components

### `watcher/xzepr/consumer/`

This subtree contains XZepr-specific Kafka and API integration code.

Important files:

- `consumer/config.rs`
  - Kafka consumer configuration types
- `consumer/kafka.rs`
  - real `rdkafka StreamConsumer` Kafka consumer behavior and message processing
- `consumer/message.rs`
  - XZepr CloudEvents wire-format message types
- `consumer/client.rs`
  - XZepr API client support

### `watcher/xzepr/filter.rs`

Contains `EventFilter`.

Responsibilities:

- evaluate XZepr CloudEvents against XZepr-specific filter configuration
- apply criteria such as:
  - `event_types`
  - `source_pattern`
  - `platform_id`
  - `package`
  - `api_version`
  - `success_only`

This filter is owned exclusively by the XZepr backend.

### `watcher/xzepr/plan_extractor.rs`

Contains `PlanExtractor`.

Responsibilities:

- extract plan content from XZepr event payloads
- support the XZepr payload shapes expected by downstream execution

This extractor is XZepr-specific and is not shared with the generic watcher.

### `watcher/xzepr/watcher.rs`

Contains the main XZepr watcher service.

Responsibilities:

- validate XZepr watcher configuration
- build the XZepr consumer
- build the XZepr event filter
- build the XZepr plan extractor
- manage execution concurrency
- consume messages and trigger plan execution flow

## XZepr Backend Data Flow

```text
Kafka topic
   |
   v
XzeprConsumer
   |
   v
CloudEventMessage
   |
   v
EventFilter::should_process
   |
   v
PlanExtractor::extract
   |
   v
plan execution path
```

## Generic Watcher Backend

## Generic Backend Purpose

The generic watcher backend consumes generic JSON plan-event messages from
Kafka-compatible topics.

It is intended for producers that are not coupled to XZepr CloudEvents and want
a simpler message format centered on direct plan execution.

## Generic Components

### `watcher/generic/message.rs`

Defines the generic watcher message types:

- `GenericPlanEvent`
- `GenericPlanResult`

`GenericPlanEvent` is the trigger message shape.

Key fields include:

- `id`
- `event_type`
- `name`
- `version`
- `action`
- `plan`
- `timestamp`
- `metadata`

`GenericPlanResult` is the result message shape.

Key fields include:

- `id`
- `event_type`
- `trigger_event_id`
- `success`
- `summary`
- `timestamp`
- `plan_output`

Loop prevention is built into the message model:

- trigger events use `event_type = "plan"`
- result events use `event_type = "result"`

### `watcher/generic/matcher.rs`

Contains `GenericMatcher`.

Responsibilities:

- evaluate `GenericPlanEvent` against `GenericMatchConfig`
- enforce the unconditional type gate:
  - only process events where `event_type == "plan"`
- perform regex-based matching on:
  - `action`
  - `name`
  - `version`

Supported matching modes:

- action only
- name + version
- name + action
- name + version + action
- accept-all when no match fields are configured

This matcher is intentionally separate from the XZepr `EventFilter`.

### `watcher/generic/result_producer.rs`

Contains `GenericResultProducer`.

Responsibilities:

- resolve the effective output topic
- serialize `GenericPlanResult`
- expose Kafka configuration assembly for future client integration
- publish results through the real `rdkafka FutureProducer` path

Output topic behavior:

- if `kafka.output_topic` is set, publish there
- otherwise publish back to `kafka.topic`

### `watcher/generic/watcher.rs`

Contains `GenericWatcher`.

Responsibilities:

- validate generic watcher config
- construct `GenericMatcher`
- construct `GenericResultProducer`
- control execution concurrency
- process generic plan events
- publish generic result events
- support dry-run processing

It also exposes a test-friendly internal processing path through methods such
as:

- `process_payload`
- `process_event`

## Generic Backend Data Flow

```text
Kafka topic
   |
   v
raw JSON payload
   |
   v
GenericPlanEvent
   |
   v
GenericMatcher::should_process
   |
   v
direct plan extraction from event.plan
   |
   v
plan execution or dry-run
   |
   v
GenericPlanResult
   |
   v
GenericResultProducer::publish
```

## Separation Between Watcher Backends

The watcher backends are intentionally separated by message format and logic.

### XZepr backend uses

- `CloudEventMessage`
- `EventFilter`
- `PlanExtractor`

### Generic backend uses

- `GenericPlanEvent`
- `GenericMatcher`
- direct plan extraction from `plan`
- `GenericResultProducer`

This separation is deliberate.

There is currently:

- no shared matcher trait
- no shared event abstraction
- no shared extraction interface

The code uses consistent naming such as `should_process` and `summary`, but that
does not imply a shared abstraction boundary.

## Watcher Logging

`src/watcher/logging.rs` contains watcher-specific logging setup and helpers.

Responsibilities:

- initialize watcher log formatting
- support structured JSON logging
- support watcher log file configuration
- keep watcher startup and processing output consistent across backends

This module is shared by both watcher backends.

## Configuration Architecture for Watchers

Watcher behavior is configured through the `watcher` section of the main config.

Important watcher-related config fields include:

- `watcher.watcher_type`
- `watcher.kafka`
- `watcher.filters`
- `watcher.generic_match`
- `watcher.logging`
- `watcher.execution`

### Backend-specific config ownership

`xzepr` backend reads:

- `watcher.filters`

`generic` backend reads:

- `watcher.generic_match`

Both may read:

- `watcher.kafka`
- `watcher.logging`
- `watcher.execution`

This keeps a single config file practical while preserving backend separation.

## Concurrency Model

Both watcher backends use the same broad concurrency strategy:

- the watcher holds a semaphore
- each message-processing path acquires a permit before execution
- `max_concurrent_executions` controls concurrency

This keeps execution bounded and avoids unregulated parallel plan execution.

## Dry-Run Model

Both watcher backends support dry-run mode.

Dry-run means:

- messages are still parsed
- filters or matchers still run
- plans are still extracted or normalized
- execution is skipped
- logging remains observable

For the generic watcher, dry-run still produces a result event through the real
producer path, which makes behavior easy to test and reason about.

## Error Handling Strategy

Watcher components use recoverable error handling throughout.

Typical categories include:

- configuration errors
- deserialization errors
- filter or matcher errors
- extraction errors
- execution errors
- producer errors

Startup errors fail early.

Per-message errors are generally logged and classified rather than causing
silent state corruption.

## Architectural Principles Applied

The watcher system follows the same general principles as the rest of XZatoma:

- keep modules organized by technical responsibility
- avoid over-abstraction
- prefer simple composition over premature shared traits
- preserve backward compatibility where possible
- keep runtime behavior observable through logging and tests

This is why watcher dispatch happens in the command layer rather than through a
more elaborate plugin system, and why the backends remain explicit peers.

## Summary

The watcher architecture now supports two first-class Kafka-backed execution
paths:

- `xzepr`
- `generic`

The architecture is centered on:

- one CLI command: `xzatoma watch`
- one dispatch point: `commands::watch::run_watch`
- two backend implementations under `src/watcher/`

This structure provides:

- backward compatibility for existing XZepr configuration files
- a new generic plan-event watcher for non-XZepr producers
- clear code ownership boundaries
- a straightforward place to extend watcher functionality

## Skills Architecture

The skills system provides extensible agent capabilities through discoverable
skill definitions.

### Module Structure

```text
src/skills/
├── mod.rs           # Module root and re-exports
├── types.rs         # Skill metadata and record types
├── discovery.rs     # Filesystem-based skill discovery
├── parser.rs        # Skill file parsing (YAML frontmatter + Markdown body)
├── catalog.rs       # In-memory skill catalog
├── activation.rs    # Runtime skill activation
├── trust.rs         # Trust store management
├── validation.rs    # Skill validation rules
└── disclosure.rs    # Skill catalog disclosure to agent
```

### Discovery Paths

Skill files are discovered from the following locations, in order:

- Project-level: `./.xzatoma/skills/`, `./.agents/skills/`
- User-level: `~/.xzatoma/skills/`, `~/.agents/skills/`
- Additional paths from configuration

### Skills Data Flow

```text
filesystem skill files
   |
   v
discovery.rs (walk configured paths)
   |
   v
parser.rs (parse YAML frontmatter + Markdown body)
   |
   v
validation.rs (validate skill definitions)
   |
   v
catalog.rs (register in-memory catalog)
   |
   v
trust.rs (verify trust status)
   |
   v
activation.rs (activate for runtime use)
   |
   v
disclosure.rs (expose catalog to agent)
```

## ACP Architecture

The ACP (Agent Client Protocol) module exposes XZatoma's agent capabilities over
two transports that share a single, transport-independent domain model defined
in `types.rs`:

- an HTTP server (`server.rs`) exposing discovery, run-lifecycle, and session
  endpoints
- a stdio transport (`stdio.rs`) that speaks newline-delimited JSON-RPC for
  Zed-compatible subprocess integration (`xzatoma agent`)

### Module Structure

```text
src/acp/
├── mod.rs                 # Module root and unified ACP surface
├── types.rs               # Canonical protocol and lifecycle model
├── server.rs              # HTTP server bootstrap and inline router
├── stdio.rs               # JSON-RPC stdio transport for `xzatoma agent`
├── runtime.rs             # Agent runtime management
├── executor.rs            # Plan and tool execution
├── session.rs             # Session management
├── session_config.rs      # Per-session configuration
├── session_mode.rs        # Session mode handling
├── streaming.rs           # Streaming support
├── available_commands.rs  # Slash-command advertisement
├── prompt_input.rs        # Prompt input conversion and validation
├── tool_notifications.rs  # Tool-call notification mapping
├── ide_bridge.rs          # IDE bridge integration
├── manifest.rs            # Agent manifest types
└── error.rs               # ACP-specific errors
```

### ACP Request Flow

```text
HTTP request                         stdio JSON-RPC request
   |                                     |
   v                                     v
server.rs (build_router,             stdio.rs (run_stdio_agent,
accept + route inline)               initialization + dispatch)
   |                                     |
   +-------------------+-----------------+
                       |
                       v
              runtime.rs (agent lifecycle)
                       |
                       +--> executor.rs (plan and tool execution)
                       +--> session.rs (session state)
                       |
                       v
              streaming.rs (incremental output)
                       |
                       v
              response (HTTP body or JSON-RPC reply)
```

### Key Components

- `server.rs` builds the HTTP router and handles routes inline (there are no
  separate `routes.rs`/`handlers.rs` modules)
- `stdio.rs` owns the stdio transport, initialization handshake, and in-memory
  session registry for `xzatoma agent`
- `AcpAgentManifest` describes agent capabilities to ACP clients
- Sessions track stateful interactions across multiple requests
- Streaming allows clients to receive incremental agent output
