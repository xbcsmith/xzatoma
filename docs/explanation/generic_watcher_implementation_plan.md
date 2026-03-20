# Generic Watcher Implementation Plan

## Overview

Add a generic Kafka/Redpanda watcher mode to XZatoma that consumes plan-formatted
JSON events from a configurable input topic, matches events using flexible regex
criteria (`action`, `name + version`, `name + action`, or `name + version + action`),
executes the embedded plan, and publishes results to a configurable output topic
(which may be the same as the input topic). Every event carries a required
`event_type` field — the generic watcher processes only events where
`event_type == "plan"` and silently discards everything else, including result events
(`event_type == "result"`), making same-topic input/output loops impossible by design.

As part of this work, all XZepr-specific code is relocated from `src/xzepr/` and the
XZepr-specific portions of `src/watcher/` into a new `src/watcher/xzepr/`
subdirectory. XZepr remains a fully supported, permanent watcher backend — the two
backends (`xzepr` and `generic`) are equal configuration peers. The watcher type is
selectable via CLI flag or configuration file, defaulting to `xzepr` for full
backward compatibility.

## Current State Analysis

### Existing Infrastructure

- **`src/xzepr/`** — Top-level XZepr integration module containing a `consumer/`
  subdirectory with: `client.rs` (XZepr HTTP API client), `config.rs`
  (`KafkaConsumerConfig`, `SecurityProtocol`, `SaslMechanism`), `kafka.rs`
  (`XzeprConsumer`, `MessageHandler` trait, `ConsumerError`), and `message.rs`
  (`CloudEventMessage`, `CloudEventData`, `EventEntity`, and related types).

- **`src/watcher/`** — Watcher service module containing: `filter.rs` (`EventFilter`,
  tightly coupled to `CloudEventMessage`), `plan_extractor.rs` (`PlanExtractor`, also
  tightly coupled to `CloudEventMessage`), `watcher.rs` (`Watcher` struct that wires
  `XzeprConsumer` with `EventFilter` and `PlanExtractor`), and `logging.rs` (generic
  structured logging helpers that are not XZepr-specific).

- **`src/config.rs`** — `WatcherConfig` with `KafkaWatcherConfig` (brokers, topic,
  group_id, security), `EventFilterConfig` (event_types, source_pattern, platform_id,
  package, api_version, success_only), `WatcherLoggingConfig`, and
  `WatcherExecutionConfig`. All filter fields are oriented around the XZepr
  `CloudEventMessage` schema.

- **`src/cli.rs`** — `Commands::Watch` subcommand with flags: `--topic`,
  `--event-types`, `--filter-config`, `--log-file`, `--json-logs`, `--dry-run`.

- **`src/commands/mod.rs`** — `watch::run_watch` entry point with
  `apply_cli_overrides` helper that bridges CLI flags into `WatcherConfig`.

- **`src/lib.rs`** — Exports both `pub mod watcher` and `pub mod xzepr` as
  independent top-level modules.

### Identified Issues

- XZepr-specific code is split across two separate top-level modules (`src/xzepr/`
  and parts of `src/watcher/`), making the watcher monolithic and XZepr-only by
  design.

- No generic Kafka watcher exists; adding a new event source type requires forking
  the entire watcher rather than selecting a backend.

- The plan file format (`examples/quickstart_plan.yaml`) lacks an `action` field,
  preventing action-based event matching in a generic context.

- There is no output/result topic support; the watcher has no mechanism to publish
  execution results back to Kafka after processing a plan.

- `WatcherConfig` has no `watcher_type` discriminator, so switching backends requires
  code-level changes with no runtime configurability.

- No `GenericMatcher` exists to support the four flexible matching combinations
  (`action`, `name + version`, `name + action`, `name + version + action`) described
  in the requirements.

- No event type discriminator exists to distinguish trigger events (`"plan"`) from
  result events (`"result"`), making same-topic input/output configurations
  susceptible to infinite re-trigger loops.

---

## Implementation Phases

### Phase 1: Restructure — Move XZepr Code into `watcher/xzepr/`

#### Task 1.1 Create the `src/watcher/xzepr/` Directory Layout

Create the directory `src/watcher/xzepr/` with a `consumer/` subdirectory that
mirrors the current `src/xzepr/consumer/` structure. The target layout after
migration is:

```
src/watcher/xzepr/
  mod.rs
  filter.rs          (moved from src/watcher/filter.rs)
  plan_extractor.rs  (moved from src/watcher/plan_extractor.rs)
  watcher.rs         (moved from src/watcher/watcher.rs)
  consumer/
    mod.rs           (moved from src/xzepr/consumer/mod.rs)
    client.rs        (moved from src/xzepr/consumer/client.rs)
    config.rs        (moved from src/xzepr/consumer/config.rs)
    kafka.rs         (moved from src/xzepr/consumer/kafka.rs)
    message.rs       (moved from src/xzepr/consumer/message.rs)
```

#### Task 1.2 Migrate Files and Update Internal Module Paths

Move all files into the new layout. Update every `use crate::xzepr::...` import and
every `use crate::watcher::{filter, plan_extractor, watcher}` import throughout
`src/` to reference the new canonical paths under
`crate::watcher::xzepr::...`. The `WatcherMessageHandler` struct in the current
`watcher.rs` and the `Watcher` struct's use of `XzeprConsumer`, `KafkaConsumerConfig`,
and `CloudEventMessage` all require path updates.

#### Task 1.3 Update `src/watcher/mod.rs` and `src/lib.rs`

- In `src/watcher/mod.rs`: add `pub mod generic;` (placeholder for Phase 3) and
  `pub mod xzepr;`. Remove the now-relocated direct module declarations (`pub mod
filter`, `pub mod plan_extractor`, `pub mod watcher`). Do **not** re-export
  `EventFilter`, `PlanExtractor`, or any other XZepr-specific type at the top-level
  `watcher` module. Those types belong exclusively to the XZepr backend and are only
  accessible via `crate::watcher::xzepr::*`. Hoisting them to `watcher::*` would
  falsely imply a shared interface with the generic backend. The only top-level
  re-export needed is `pub use xzepr::watcher::Watcher as XzeprWatcher;` for the
  dispatch path in Phase 5.

- In `src/xzepr/mod.rs`: update to re-export everything from `crate::watcher::xzepr`
  so the top-level `xzepr` path continues to resolve. XZepr is a permanent,
  first-class watcher backend — a supported configuration choice alongside the
  generic watcher. Do not add deprecation notices. Keep `pub mod xzepr` in
  `src/lib.rs` unchanged.

#### Task 1.4 Testing Requirements

- All pre-existing tests in `src/watcher/` and `src/xzepr/` must pass without
  modification to their test logic (only import path updates if any test file uses
  a now-relocated path).
- Run `cargo test --workspace` to verify zero regressions.
- Confirm `examples/downstream_consumer.rs` still compiles cleanly with updated
  import paths.

#### Task 1.5 Deliverables

- `src/watcher/xzepr/` directory containing all migrated files.
- Updated `src/watcher/mod.rs` with `pub mod xzepr` and re-exports.
- Updated `src/xzepr/mod.rs` re-exporting from `crate::watcher::xzepr`.
- Clean `cargo build --workspace` and `cargo test --workspace`.

#### Task 1.6 Success Criteria

- `cargo build --workspace` produces zero errors and zero new warnings.
- All pre-existing tests pass.
- No test logic changes are required — only import path updates if necessary.

---

### Phase 2: Extend Plan Format with `action` Field

#### Task 2.1 Add `action` to the Existing Plan Schema

Locate the plan deserialization struct used by `src/commands/mod.rs run::run_plan`
(the struct that parses `quickstart_plan.yaml`). Add an optional `action` field:

```rust
pub action: Option<String>,
```

The field must be optional to preserve full backward compatibility with all existing
plan files. Update the plan struct's `serde` derivation to handle the field being
absent gracefully (default to `None`).

#### Task 2.2 Define `GenericPlanEvent` — The Generic Trigger Message Schema

Create `src/watcher/generic/message.rs` with the `GenericPlanEvent` struct. This is
the JSON message format that a producer must publish to the input topic to trigger
the generic watcher:

```rust
pub struct GenericPlanEvent {
    pub id: String,                             // ULID preferred
    pub event_type: String,                     // Must be "plan" — matcher rejects all other values
    pub name: Option<String>,                   // For name-based matching
    pub version: Option<String>,                // For version-based matching
    pub action: Option<String>,                 // For action-based matching
    pub plan: serde_json::Value,                // Embedded plan (string, object, or array)
    pub timestamp: Option<DateTime<Utc>>,       // RFC-3339 event timestamp
    pub metadata: Option<serde_json::Value>,    // Arbitrary extra fields for extensibility
}
```

The `event_type` field is the primary loop-breaker: the `GenericMatcher` rejects
any event where `event_type != "plan"` before evaluating any other criteria.
Producers MUST set this field to `"plan"`. Any other value (including `"result"`)
is silently skipped.

Also define `GenericPlanResult` in the same file — the schema for the result event
published to the output topic after plan execution:

```rust
pub struct GenericPlanResult {
    pub id: String,                             // ULID of this result event
    pub event_type: String,                     // Always "result" — prevents re-triggering
    pub trigger_event_id: String,               // id from the triggering GenericPlanEvent
    pub success: bool,
    pub summary: String,
    pub timestamp: DateTime<Utc>,               // RFC-3339
    pub plan_output: Option<serde_json::Value>, // Structured execution output
}
```

`GenericPlanResult::event_type` is hardcoded to `"result"` at construction time so
that if the output topic and input topic are the same, the watcher will consume the
result message, check `event_type`, and immediately discard it — no plan execution
loop is possible.

#### Task 2.3 Update Plan Examples and Format Documentation

- Update `examples/quickstart_plan.yaml` to include the new `action` field with a
  representative value (e.g., `action: quickstart`) so it doubles as a reference
  example for generic watcher producers.
- Update `docs/reference/workflow_format.md` (or the closest equivalent plan format
  reference) to document the `action` field, its purpose, and how it is used in
  generic watcher matching.
- Add a minimal example `GenericPlanEvent` JSON snippet to the updated workflow
  format documentation.

#### Task 2.4 Testing Requirements

- Unit tests for `GenericPlanEvent` serialization and deserialization covering: all
  fields present, only `action` present, only `name` + `version` present, and the
  minimal case (only `id`, `event_type`, and `plan`).
- A test that a `GenericPlanEvent` with `event_type` other than `"plan"` (e.g.,
  `"result"`, `"unknown"`, empty string) round-trips correctly and that the matcher
  will reject it (tested further in Phase 3).
- Unit tests for `GenericPlanResult` round-trip serialization, confirming
  `event_type` serializes as `"result"`.
- A test that parses the updated `examples/quickstart_plan.yaml` and confirms the
  `action` field is populated and that existing fields remain unchanged.

#### Task 2.5 Deliverables

- Updated plan struct with optional `action` field.
- `src/watcher/generic/message.rs` with `GenericPlanEvent` and `GenericPlanResult`.
- Updated `examples/quickstart_plan.yaml` with `action` field.
- Updated plan format documentation.

#### Task 2.6 Success Criteria

- Existing `examples/quickstart_plan.yaml` round-trips through the parser without
  errors before and after the `action` field is added.
- `GenericPlanEvent` correctly deserializes all matching-relevant field combinations.
- `cargo test` on the new `message.rs` module passes with 100% coverage of new code.

---

### Phase 3: Generic Kafka Watcher Core

#### Task 3.1 Create the `src/watcher/generic/` Directory

Create the following module structure (the `message.rs` file is introduced in
Phase 2; the rest are new in this phase):

```
src/watcher/generic/
  mod.rs
  message.rs       (GenericPlanEvent, GenericPlanResult — from Phase 2)
  matcher.rs       (GenericMatcher, GenericMatchConfig)
  producer.rs      (GenericResultProducer)
  watcher.rs       (GenericWatcher)
```

#### Task 3.2 Implement `GenericMatcher`

Create `src/watcher/generic/matcher.rs`. The matcher evaluates a `GenericPlanEvent`
against a `GenericMatchConfig` and returns `true` if the event should be processed.

**`GenericMatcher` and `EventFilter` are completely separate implementations with no
shared trait, no shared interface, and no shared logic.** They operate on entirely
different message types:

- `EventFilter` (in `src/watcher/xzepr/filter.rs`) operates exclusively on
  `CloudEventMessage` — the XZepr CloudEvents 1.0.1 wire format. Its filter fields
  (`event_types`, `source_pattern`, `platform_id`, `package`, `api_version`,
  `success_only`) are all XZepr domain concepts. It is owned entirely by the XZepr
  watcher and is never referenced by the generic watcher.

- `GenericMatcher` (in `src/watcher/generic/matcher.rs`) operates exclusively on
  `GenericPlanEvent` — the generic plan-event wire format. Its match fields
  (`action`, `name`, `version`) and its `event_type` gate are generic watcher
  concepts. It is owned entirely by the generic watcher and is never referenced by
  the XZepr watcher.

Do not introduce a shared `Matcher` trait. The superficial similarity of both types
having a `should_process` method and a `summary` method is intentional naming
consistency, not an abstraction boundary. If a shared trait is ever warranted it
should be introduced as its own deliberate design decision, not as a side-effect of
this work.

**Type gate — checked first, always, unconditionally:**
`should_process` MUST return `false` for any event where `event.event_type != "plan"`.
This is the loop-breaker that prevents result messages published to the same topic
from re-triggering plan execution. No configuration can override this check.

**Regex matching:**
All three match fields (`action`, `name`, `version`) are treated as regular
expressions, not literal strings. Patterns are compiled at construction time in
`GenericMatcher::new` and stored as `Arc<Regex>` to avoid recompilation per message.
The function(s) responsible for compiling these patterns MUST carry the comment:

```
// Now we have 2 problems.
```

Matching is case-insensitive by default (patterns are compiled with the `(?i)` flag
prepended unless the pattern already contains inline flags). A config value of
`"deploy"` therefore matches `"deploy"`, `"Deploy"`, `"DEPLOY"`, etc. A config value
of `"deploy.*"` matches any action beginning with `"deploy"`.

The four supported matching modes, derived from which config fields are `Some`, are:

| Active config fields          | Match logic                                                           |
| ----------------------------- | --------------------------------------------------------------------- |
| `action` only                 | event.action matches action regex                                     |
| `name` + `version`            | event.name matches name regex AND event.version matches version regex |
| `name` + `action`             | event.name matches name regex AND event.action matches action regex   |
| `name` + `version` + `action` | all three fields match their respective regexes                       |
| none configured               | accept all events with `event_type == "plan"` (pass-through)          |

When a required event field is `None` (e.g., the event has no `action`) and the
config pattern requires it, the event does not match — a missing field is never
treated as a wildcard match.

`GenericMatcher::new(config)` compiles all configured patterns eagerly and returns
`Result<Self>` so invalid regex is surfaced at startup, not mid-run.

The `GenericMatcher` exposes a `summary() -> String` method (analogous to
`EventFilter::summary()`) for structured logging at startup, including the compiled
pattern strings and the active matching mode.

#### Task 3.3 Implement `GenericResultProducer`

Create `src/watcher/generic/producer.rs` with a `GenericResultProducer` struct that:

- Accepts the same `KafkaWatcherConfig` used by the consumer so no additional
  connection configuration is required.
- Resolves the effective output topic: if `KafkaWatcherConfig::output_topic` is
  `Some`, use it; otherwise fall back to `KafkaWatcherConfig::topic` (same topic
  as input).
- Exposes `pub async fn publish(&self, result: &GenericPlanResult) -> Result<()>`.
- Follows the stub-first pattern already established by `XzeprConsumer` — define the
  full interface and Kafka config assembly (`get_kafka_config() -> Vec<(String,
String)>`) without requiring `rdkafka` at compile time. Log the serialized result
  at `info` level so the stub is testable and visible in dry-run mode.

#### Task 3.4 Implement `GenericWatcher`

Create `src/watcher/generic/watcher.rs` with `GenericWatcher`:

- Constructor `GenericWatcher::new(config: Config, dry_run: bool) -> Result<Self>`
  validates Kafka config presence, builds `GenericMatcher` from
  `config.watcher.generic_match`, builds `GenericResultProducer` from Kafka config,
  initializes the execution semaphore from `config.watcher.execution`.
- `pub async fn start(&mut self) -> Result<()>` — the main event loop. Consumes raw
  JSON from the input topic, deserializes each payload as `GenericPlanEvent`,
  passes it through `GenericMatcher::should_process`, extracts the embedded `plan`
  value, and (unless `dry_run`) executes the plan via the existing agent path used
  by the XZepr watcher. After execution (or in dry-run), builds a `GenericPlanResult`
  and calls `GenericResultProducer::publish`.
- Plan extraction from `GenericPlanEvent::plan` is direct (the `plan` field is the
  plan); no multi-strategy fallback is needed as the schema is controlled.
- `start()` returns `Ok(())` on graceful shutdown (signal) and `Err` on fatal errors,
  matching the interface of `XzeprWatcher` for uniform dispatch in Phase 5.
- Concurrency is controlled by the same `Arc<Semaphore>` pattern as `XzeprWatcher`.

#### Task 3.5 Wire `src/watcher/generic/mod.rs`

Re-export the public API:
`GenericWatcher`, `GenericMatcher`, `GenericMatchConfig`, `GenericPlanEvent`,
`GenericPlanResult`, `GenericResultProducer`.

Add `pub mod generic;` in `src/watcher/mod.rs` (the placeholder added in Phase 1).

#### Task 3.6 Testing Requirements

- Unit tests for `GenericMatcher` covering:
  - All four match modes with literal patterns.
  - Accept-all mode (no config) accepts events with `event_type == "plan"`.
  - **Type gate**: events with `event_type == "result"` are rejected regardless of
    matching mode, including accept-all mode.
  - **Type gate**: events with any `event_type` other than `"plan"` (empty string,
    `"unknown"`, `"PLAN"`) are rejected.
  - Regex pattern matching: a pattern of `"deploy.*"` matches `"deploy-prod"` and
    `"deployment"` but not `"rollback"`.
  - Case-insensitive matching: a pattern of `"deploy"` matches `"Deploy"` and
    `"DEPLOY"`.
  - Invalid regex in config returns `Err` from `GenericMatcher::new` before any
    events are processed.
  - A missing event field (`action` is `None`) does not match a config that requires
    `action`.
- Unit tests for `GenericPlanEvent` plan extraction: string plan, JSON object plan,
  JSON array plan, missing `plan` field returns error.
- Unit tests for `GenericResultProducer` stub: verify `get_kafka_config()` output for
  input-topic fallback and explicit `output_topic` override.
- A `GenericWatcher` dry-run unit test that:
  - Synthesizes a matching `GenericPlanEvent` (`event_type = "plan"`) and asserts it
    is processed.
  - Synthesizes a `GenericPlanResult` (`event_type = "result"`) on the same topic and
    asserts it is silently discarded (no plan execution, no error).
  - Synthesizes a `GenericPlanEvent` that does not match the configured criteria and
    asserts it is skipped.

#### Task 3.7 Deliverables

- Complete `src/watcher/generic/` module with all four files.
- `GenericWatcher`, `GenericMatcher`, and `GenericResultProducer` with unit tests.
- Updated `src/watcher/mod.rs` exporting the `generic` module.

#### Task 3.8 Success Criteria

- `GenericMatcher` correctly classifies events across all four matching modes.
- Dry-run mode processes a synthetic matching event without triggering plan execution.
- `cargo test watcher::generic` passes with ≥ 80% line coverage on the new module.

---

### Phase 4: Configuration and CLI Updates

#### Task 4.1 Add `WatcherType` Enum to `src/config.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WatcherType {
    #[default]
    XZepr,    // Existing XZepr-style CloudEvents watcher (default for backward compat)
    Generic,  // New generic Kafka plan-event watcher
}
```

Add `pub watcher_type: WatcherType` to `WatcherConfig`, with `#[serde(default)]` so
existing config files that omit the field continue to work unchanged.

#### Task 4.2 Add `output_topic` to `KafkaWatcherConfig`

```rust
pub struct KafkaWatcherConfig {
    // ... existing fields ...
    /// Output topic for publishing plan execution results (Generic watcher only).
    /// If None, results are published back to the input `topic`.
    #[serde(default)]
    pub output_topic: Option<String>,
}
```

When `output_topic` is `None` and `watcher_type` is `Generic`, the result is
published to the same topic as the input. This is documented clearly in both the
struct doc comment and the configuration reference.

#### Task 4.3 Add `GenericMatchConfig` to `src/config.rs`

`GenericMatchConfig` and `EventFilterConfig` are the configuration counterparts of
their respective matchers — they are completely separate structs used by completely
separate backends. `EventFilterConfig` is read only by the XZepr watcher when
`watcher_type = xzepr`. `GenericMatchConfig` is read only by the generic watcher when
`watcher_type = generic`. Neither struct is shared, and neither watcher reads the
other's config section. Both live in `WatcherConfig` for convenience of a single
config file, but their separation must be maintained in code — the XZepr watcher
constructor must not accept or inspect `GenericMatchConfig`, and the generic watcher
constructor must not accept or inspect `EventFilterConfig`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenericMatchConfig {
    /// Regex pattern matched against the event action field (case-insensitive by default)
    pub action: Option<String>,
    /// Regex pattern matched against the event name field (case-insensitive by default)
    pub name: Option<String>,
    /// Regex pattern matched against the event version field (case-insensitive by default)
    pub version: Option<String>,
}
```

Add `pub generic_match: GenericMatchConfig` to `WatcherConfig` with
`#[serde(default)]`.

#### Task 4.4 Add New Environment Variable Support in `Config::apply_env_vars`

Add handling for the following new variables in the `apply_env_vars` method in
`src/config.rs`:

| Environment Variable            | Maps to                         | Notes                          |
| ------------------------------- | ------------------------------- | ------------------------------ |
| `XZATOMA_WATCHER_TYPE`          | `watcher.watcher_type`          | `xzepr` or `generic`           |
| `XZATOMA_WATCHER_OUTPUT_TOPIC`  | `watcher.kafka.output_topic`    | Generic watcher result topic   |
| `XZATOMA_WATCHER_MATCH_ACTION`  | `watcher.generic_match.action`  | Generic matcher: action field  |
| `XZATOMA_WATCHER_MATCH_NAME`    | `watcher.generic_match.name`    | Generic matcher: name field    |
| `XZATOMA_WATCHER_MATCH_VERSION` | `watcher.generic_match.version` | Generic matcher: version field |

#### Task 4.5 Update the `Watch` Subcommand in `src/cli.rs`

Add the following arguments to `Commands::Watch`:

```rust
/// Watcher backend type: "xzepr" (default) or "generic"
#[arg(long, default_value = "xzepr")]
watcher_type: Option<String>,

/// Output topic for publishing results (generic watcher only; defaults to input topic)
#[arg(long)]
output_topic: Option<String>,

/// Generic matcher: regex pattern for the action field (case-insensitive)
#[arg(long)]
action: Option<String>,

/// Generic matcher: regex pattern for the name field (case-insensitive)
#[arg(long)]
name: Option<String>,
```

Update `commands::watch::apply_cli_overrides` to apply these four new overrides to
`config.watcher`.

#### Task 4.6 Update `Config::validate`

Add the following validation rules:

- If `watcher_type` is `Generic` and all three `generic_match` fields are `None`,
  emit a `tracing::warn!` (not an error) indicating accept-all mode is active — this
  is valid but likely unintentional in production.
- If `watcher_type` is `Generic` and any `generic_match` field is `Some`, attempt to
  compile it as a regex and return a `ValidationError` immediately if any pattern is
  invalid. Surfacing bad regex at startup is far preferable to silently skipping all
  events at runtime.
- If `watcher_type` is `XZepr` and `generic_match` fields are populated, emit a
  `tracing::debug!` noting the generic match config is unused.
- If `watcher_type` is `Generic` and `kafka` is `None`, return a validation error
  (same rule that already exists for XZepr).

#### Task 4.7 Testing Requirements

- YAML config round-trip tests for the new `WatcherType` field (both values, default
  omitted).
- YAML round-trip tests for `GenericMatchConfig` with all combinations of
  `Some`/`None` fields, including regex special characters in patterns.
- YAML round-trip test for `output_topic` in `KafkaWatcherConfig`.
- `apply_env_vars` tests for all five new environment variables.
- `apply_cli_overrides` tests for `--watcher-type`, `--output-topic`, `--action`,
  and `--name`.
- `Config::validate` tests:
  - Generic + no match config → warns, no error.
  - Generic + valid regex patterns → no error.
  - Generic + invalid regex pattern (e.g., `"[broken"`) → returns `ValidationError`.
  - Generic + missing kafka → returns error.

#### Task 4.8 Deliverables

- `WatcherType` enum in `src/config.rs`.
- `GenericMatchConfig` struct in `src/config.rs`.
- `output_topic` field in `KafkaWatcherConfig`.
- Updated `Commands::Watch` in `src/cli.rs` with four new flags.
- Updated `apply_env_vars` and `apply_cli_overrides`.
- Updated `Config::validate`.

#### Task 4.9 Success Criteria

- YAML configs for both watcher types parse correctly; existing configs without
  `watcher_type` default to `WatcherType::XZepr`.
- All five new environment variables correctly override their corresponding config
  fields.
- All four new CLI flags override config values.
- `cargo test config` passes with all new test cases.

---

### Phase 5: Integration, Testing, and Documentation

#### Task 5.1 Dispatch on `watcher_type` in `commands::watch::run_watch`

Update `commands::watch::run_watch` to branch on `config.watcher.watcher_type`:

```rust
match config.watcher.watcher_type {
    WatcherType::XZepr   => crate::watcher::XzeprWatcher::new(config, dry_run)?.start().await,
    WatcherType::Generic => crate::watcher::generic::GenericWatcher::new(config, dry_run)?.start().await,
}
```

Both watcher types expose the same `start() -> Result<()>` async interface, enabling
uniform dispatch with no further branching. Pass `dry_run` through to both paths.

#### Task 5.2 Add Example Configuration Files

Create `config/generic_watcher.yaml` with a fully annotated example configuration
for the generic watcher, covering:

- A minimal example (brokers, topic, one match criterion).
- A production example (SASL_SSL security, separate output topic, all three match
  criteria, tuned concurrency and timeout).
- An accept-all example (no match criteria — processes every event on the topic).

Update the existing `config/watcher.yaml` (if present) to add a commented-out
`watcher_type: xzepr` line so operators can see the new field without it changing
any defaults.

#### Task 5.3 Add Integration Tests

Add tests that exercise the full dispatch path in `commands::watch`:

- A test that calls `run_watch` with `watcher_type: xzepr` and a missing Kafka config
  returns an error (existing behavior preserved).
- A test that calls `run_watch` with `watcher_type: generic` and a missing Kafka
  config returns an error (consistent behavior).
- A `GenericWatcher` dry-run integration test (in `src/watcher/generic/watcher.rs`
  mod tests) that:
  1. Creates a `GenericWatcher` with a `GenericMatchConfig` requiring `action =
"deploy"`.
  2. Calls `process_event` (an internal helper extracted for testability) with a
     matching `GenericPlanEvent` and asserts the event is processed.
  3. Calls `process_event` with a non-matching event (`action = "rollback"`) and
     asserts it is skipped.

#### Task 5.4 Update `docs/how-to/setup_watcher.md`

Add a new section **"Using the Generic Watcher"** covering:

- When to choose `generic` vs `xzepr`.
- A minimal CLI example: `xzatoma watch --watcher-type generic --topic plans.events
--action deploy`.
- A config-file example using `config/generic_watcher.yaml`.
- How to configure the output topic (same vs separate).
- A `GenericPlanEvent` JSON payload example for producers.

#### Task 5.5 Update `docs/reference/watcher_environment_variables.md`

Add a new **"Generic Watcher Configuration"** section documenting all five new
`XZATOMA_WATCHER_*` variables introduced in Phase 4, including accepted values,
defaults, and examples. Update the existing **"Kafka Configuration"** section to
document `XZATOMA_WATCHER_OUTPUT_TOPIC`.

#### Task 5.6 Update `docs/reference/configuration.md`

Document the three new fields added to `WatcherConfig`:

- `watcher_type` — accepted values, default, YAML example.
- `kafka.output_topic` — purpose, default behavior (falls back to input topic), YAML
  example.
- `generic_match` — the four matching modes, YAML examples for each combination.

#### Task 5.7 Update Architecture Documentation

Update `docs/reference/architecture.md` to reflect:

- The new `src/watcher/xzepr/` subtree and its relationship to the legacy
  `src/xzepr/` compatibility shim.
- The new `src/watcher/generic/` subtree and its components (`GenericMatcher`,
  `GenericResultProducer`, `GenericWatcher`).
- The watcher dispatch diagram showing both backends behind the common `start()`
  interface.

#### Task 5.8 Testing Requirements

- `cargo test --workspace` passes with zero regressions from all previous phases.
- New code in Phases 2–5 achieves ≥ 80% line coverage.
- `examples/downstream_consumer.rs` compiles cleanly against the updated module
  structure from Phase 1.
- Both watcher configurations launch in dry-run mode without errors in a local test
  environment.

#### Task 5.9 Deliverables

- Updated `commands::watch::run_watch` with `WatcherType` dispatch.
- `config/generic_watcher.yaml` example configuration file.
- Integration tests for both watcher type dispatch paths.
- Complete documentation updates across:
  - `docs/how-to/setup_watcher.md`
  - `docs/reference/watcher_environment_variables.md`
  - `docs/reference/configuration.md`
  - `docs/reference/architecture.md`

#### Task 5.10 Success Criteria

- `xzatoma watch --watcher-type generic --topic plans.events --action deploy`
  launches, logs the matcher summary, and enters the event consumption loop without
  errors.
- `xzatoma watch --watcher-type xzepr --topic xzepr.events` behaves identically to
  the pre-implementation behavior — no regressions.
- All `cargo test --workspace` tests pass.
- Documentation covers both watcher types end-to-end with working YAML and CLI
  examples.

---

## Summary of New File Locations

| New Path                                | Source / Description                                     |
| --------------------------------------- | -------------------------------------------------------- |
| `src/watcher/xzepr/mod.rs`              | New module root for XZepr watcher                        |
| `src/watcher/xzepr/filter.rs`           | Moved from `src/watcher/filter.rs`                       |
| `src/watcher/xzepr/plan_extractor.rs`   | Moved from `src/watcher/plan_extractor.rs`               |
| `src/watcher/xzepr/watcher.rs`          | Moved from `src/watcher/watcher.rs`                      |
| `src/watcher/xzepr/consumer/mod.rs`     | Moved from `src/xzepr/consumer/mod.rs`                   |
| `src/watcher/xzepr/consumer/client.rs`  | Moved from `src/xzepr/consumer/client.rs`                |
| `src/watcher/xzepr/consumer/config.rs`  | Moved from `src/xzepr/consumer/config.rs`                |
| `src/watcher/xzepr/consumer/kafka.rs`   | Moved from `src/xzepr/consumer/kafka.rs`                 |
| `src/watcher/xzepr/consumer/message.rs` | Moved from `src/xzepr/consumer/message.rs`               |
| `src/xzepr/mod.rs`                      | Converted to backward-compatibility re-export shim       |
| `src/watcher/generic/mod.rs`            | New module root for generic watcher                      |
| `src/watcher/generic/message.rs`        | New: `GenericPlanEvent`, `GenericPlanResult`             |
| `src/watcher/generic/matcher.rs`        | New: `GenericMatcher`, `GenericMatchConfig`              |
| `src/watcher/generic/producer.rs`       | New: `GenericResultProducer`                             |
| `src/watcher/generic/watcher.rs`        | New: `GenericWatcher`                                    |
| `config/generic_watcher.yaml`           | New: annotated example configuration for generic watcher |

## Summary of Modified Files

| Modified Path                                     | Change Summary                                               |
| ------------------------------------------------- | ------------------------------------------------------------ |
| `src/watcher/mod.rs`                              | Add `pub mod xzepr`, `pub mod generic`; update re-exports    |
| `src/lib.rs`                                      | Keep `pub mod xzepr` shim; no structural change needed       |
| `src/config.rs`                                   | Add `WatcherType`, `GenericMatchConfig`, `output_topic`      |
| `src/cli.rs`                                      | Add `--watcher-type`, `--output-topic`, `--action`, `--name` |
| `src/commands/mod.rs`                             | Update `run_watch` dispatch; update `apply_cli_overrides`    |
| `examples/quickstart_plan.yaml`                   | Add `action` field                                           |
| `docs/how-to/setup_watcher.md`                    | Add Generic Watcher section                                  |
| `docs/reference/watcher_environment_variables.md` | Document five new env vars                                   |
| `docs/reference/configuration.md`                 | Document three new config fields                             |
| `docs/reference/architecture.md`                  | Update module structure diagram                              |
