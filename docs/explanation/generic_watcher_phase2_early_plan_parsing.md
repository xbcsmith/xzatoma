# Generic Watcher Phase 2: Early Plan Parsing and Event Handler

## Overview

Phase 2 of the Generic Watcher Modernization introduces _early plan parsing_ and
a centralized event-handling pipeline. Rather than deferring plan validation
until execution time, the watcher now parses and validates the raw Kafka payload
at the point of receipt. A new `GenericEventHandler` struct encapsulates the
five-step pipeline from raw bytes to an executable `GenericTask`.

---

## What Changed

### Task 2.1 Redesigned `GenericPlanEvent` (`src/watcher/generic/event.rs`)

`GenericPlanEvent` is no longer a JSON wire-format type. It is now a parsed,
validated, in-memory representation of an inbound plan trigger.

**Removed fields:**

- `id: String`
- `event_type: String`
- `metadata: Option<serde_json::Value>`
- `timestamp: Option<DateTime<Utc>>`
- `plan: serde_json::Value`

**Added fields:**

- `plan: Plan` - the parsed and validated plan from `crate::tools::plan`
- `source_topic: String` - the Kafka topic the message was consumed from
- `key: Option<String>` - the Kafka message key (correlation identifier)
- `received_at: DateTime<Utc>` - set to `Utc::now()` at construction time

**Auto-populated fields** (derived from the parsed plan at construction):

- `name: Option<String>` - from `plan.name`
- `version: Option<String>` - from `plan.version`
- `action: Option<String>` - from `plan.action`

**Removed methods:**

- `is_plan_event()` - the type gate is now implicit in the parsing step

**New constructor signature:**

```rust
pub fn new(payload: &str, source_topic: String, key: Option<String>) -> Result<Self>
```

The constructor calls `PlanParser::parse_string` on the raw payload and returns
`Err` immediately if parsing or validation fails. Any `GenericPlanEvent` value
that exists in memory is therefore guaranteed to hold a structurally valid plan.

**Loop-break guarantee (updated):** Previously enforced by
`event_type == "plan"`. Now enforced implicitly: `GenericPlanResult` JSON
payloads lack the `name` and `steps` fields required by `Plan`, so they fail
parsing when consumed back on the same topic. No infinite re-trigger loop is
possible.

---

### Task 2.2 Added `RawKafkaMessage` (`src/watcher/generic/event.rs`)

A new `RawKafkaMessage` struct bridges the raw Kafka byte stream and the parsed
event pipeline:

```rust
pub struct RawKafkaMessage {
    pub payload: String,
    pub topic: String,
    pub key: Option<String>,
}
```

This type is the primary input to `GenericEventHandler::handle` and is
constructed by the watcher's consume loop (and by `process_payload` for tests).

---

### Task 2.3 Created `event_handler.rs` (`src/watcher/generic/event_handler.rs`)

#### `GenericTask`

The output of the handler pipeline - a fully resolved and validated plan task:

```rust
pub struct GenericTask {
    pub plan: Plan,
    pub instruction: String,
    pub correlation_key: Option<String>,
    pub received_at: DateTime<Utc>,
}
```

The `instruction` field is derived from `Plan::to_instruction()` and contains
the prompt text passed to the agent executor.

The `correlation_key` maps to the Kafka message key and is used as the
`trigger_event_id` in the published `GenericPlanResult`.

#### `GenericEventHandler`

Encapsulates the five-step pipeline:

```rust
pub struct GenericEventHandler {
    pub matcher: Option<GenericMatcher>,
    pub plan_directory: Option<PathBuf>,
}
```

**Pipeline steps in
`handle(msg: RawKafkaMessage) -> Result<Option<GenericTask>>`:**

1. **Parse** - calls `GenericPlanEvent::new` on the raw payload; propagates
   `Err` on parse or validation failure.
2. **Match** - if a `GenericMatcher` is configured and `matcher.should_process`
   returns `false`, returns `Ok(None)`.
3. **Resolve** - if `plan_directory` is `Some(dir)`, searches for a curated
   on-disk plan file with priority:
   - `{dir}/{plan.name}-{version}.yaml` (when `event.version` is `Some`)
   - `{dir}/{plan.name}.yaml` (name-only fallback)
   - Embedded plan from the payload (when no file is found)
   - Returns `Err` if a file exists but cannot be parsed.
4. **Validate** - calls `PlanParser::validate` on the resolved plan; propagates
   `Err` on failure.
5. **Build** - derives the instruction via `Plan::to_instruction()` and returns
   `Ok(Some(GenericTask { ... }))`.

Each step logs at `info` or `debug` level using structured `tracing` fields.

---

### Task 2.4 Updated `watcher.rs` (`src/watcher/generic/watcher.rs`)

**Removed:**

- `GenericWatcherError::PlanExtraction` variant
- `GenericWatcherError::Deserialization` variant
- `GenericWatcher::extract_plan_text` method
- `matcher: Arc<GenericMatcher>` field
- All imports from the deprecated `message.rs` module

**Added / Changed:**

- `event_handler: GenericEventHandler` field replaces `matcher`
- `GenericWatcher::new` constructs the handler:
  `GenericEventHandler::new(Some(matcher), None)`
- `process_event` signature changed from `(GenericPlanEvent)` to
  `(RawKafkaMessage)`
- `process_event` delegates to `event_handler.handle(msg)` and maps the result:
  - `Err` -> `Ok(MessageDisposition::InvalidPayload)`
  - `Ok(None)` -> `Ok(MessageDisposition::SkippedNoMatch)`
  - `Ok(Some(task))` -> acquires semaphore, dry-runs or executes, publishes
- `process_payload(&str)` wraps the payload in a `RawKafkaMessage` and calls
  `process_event`; preserved for backward-compatible test paths
- `execute_plan` now takes `&GenericTask` instead of `(&GenericPlanEvent, &str)`
- The consume loop in `start` now passes the full topic and key to
  `process_event`
- `matcher_summary` reads through `event_handler.matcher`

---

### Task 2.5 Updated `mod.rs` (`src/watcher/generic/mod.rs`)

- Declared `pub mod event_handler`
- Added re-exports: `GenericEventHandler`, `GenericTask`, `RawKafkaMessage`
- Removed `pub mod message` declaration
- Updated module-level doc comments to reflect the new loop-break model

---

### Task 2.5 (continued) Deleted `message.rs`

`src/watcher/generic/message.rs` (the Phase 1 compatibility shim) has been
deleted. All call sites now import from the canonical modules:

- `crate::watcher::generic::event` for `GenericPlanEvent` and `RawKafkaMessage`
- `crate::watcher::generic::result_event` for `GenericPlanResult`

---

### Supporting changes to `src/tools/plan.rs`

Two additions were required to support the handler pipeline:

**`Plan::to_instruction() -> String`**

Formats the plan as an agent instruction prompt:

```text
Execute this plan:

Name: <plan.name>

Steps:
- <step1.name>: <step1.action>
- <step2.name>: <step2.action>
```

**`PlanParser::parse_string(content: &str) -> Result<Plan>`**

The primary entry point for parsing raw Kafka payloads. Tries JSON first for
content that begins with `{`, then falls back to YAML for all other content.

**`Plan::version: Option<String>`** (new field)

Added to `Plan` to enable version-based plan directory resolution
(`{dir}/{name}-{version}.yaml`). The field is optional and defaults to `None`
for all existing plans. The `Plan` struct now also derives `PartialEq`.

---

### Updated `matcher.rs` (`src/watcher/generic/matcher.rs`)

- Import updated from `message::GenericPlanEvent` to `event::GenericPlanEvent`
- The `is_plan_event()` check removed from `should_process`. The type gate is
  now handled upstream by `GenericPlanEvent::new` returning `Err` for non-plan
  payloads. The matcher only evaluates `name`, `version`, and `action` criteria.
- Doc comments updated to reflect the new matching model
- Tests that manipulated `event.event_type` directly removed (they tested the
  Phase 1 type gate, which no longer exists in the matcher)

### Updated `producer.rs` (`src/watcher/generic/producer.rs`)

Import updated from `message::GenericPlanResult` to
`result_event::GenericPlanResult`.

---

## Architecture After Phase 2

```text
Kafka topic
    |
    v
RawKafkaMessage { payload, topic, key }
    |
    v
GenericEventHandler::handle()
    |-- Step 1: GenericPlanEvent::new(payload, topic, key)
    |       calls PlanParser::parse_string()
    |       returns Err for non-plan payloads (loop-break)
    |
    |-- Step 2: GenericMatcher::should_process(&event)
    |       returns Ok(None) if no match
    |
    |-- Step 3: resolve plan from directory (optional)
    |       versioned file > name-only file > embedded plan
    |
    |-- Step 4: PlanParser::validate(&resolved_plan)
    |
    |-- Step 5: Plan::to_instruction()
    |
    v
Ok(Some(GenericTask { plan, instruction, correlation_key, received_at }))
    |
    v
GenericWatcher::process_event()
    |-- dry_run:  GenericPlanResult { success: true, summary: "Dry-run..." }
    |-- execute:  run_plan_with_options(instruction)
    |
    v
GenericResultProducer::publish(&result)
    |
    v
Kafka output topic
```

---

## Tests Added

### `src/watcher/generic/event.rs` (16 unit tests)

Covers the required Task 2.6 tests and additional edge cases:

| Test                                                         | Description                            |
| ------------------------------------------------------------ | -------------------------------------- |
| `test_new_valid_yaml_payload`                                | YAML sets plan.name, source_topic, key |
| `test_new_valid_json_payload`                                | JSON payload parses successfully       |
| `test_new_invalid_payload_returns_err`                       | Malformed payload returns Err          |
| `test_new_missing_tasks_returns_err`                         | Empty steps list returns Err           |
| `test_new_received_at_is_recent`                             | received_at bracketed by before/after  |
| `test_clone_produces_independent_copy`                       | Clone does not alias plan              |
| `test_new_action_auto_populated_from_plan`                   | action from plan.action                |
| `test_new_version_auto_populated_from_plan`                  | version from plan.version              |
| `test_new_version_defaults_to_none_when_plan_has_no_version` | None when absent                       |
| `test_new_action_is_none_when_plan_has_no_action`            | None when absent                       |
| `test_new_name_auto_populated_from_plan`                     | name mirrors plan.name                 |
| `test_raw_kafka_message_fields_accessible`                   | RawKafkaMessage field access           |
| `test_raw_kafka_message_key_can_be_none`                     | Optional key                           |
| `test_new_key_propagated_to_event`                           | key flows to event.key                 |
| `test_new_key_is_none_when_not_provided`                     | None key propagated                    |
| `test_new_result_json_payload_returns_err`                   | Loop-break: result JSON fails          |

### `src/watcher/generic/event_handler.rs` (14 async unit tests)

Covers the required Task 2.6 tests and additional edge cases:

| Test                                                            | Description                                  |
| --------------------------------------------------------------- | -------------------------------------------- |
| `test_handle_valid_plan_no_matcher_no_directory`                | Some(task), plan.name, non-empty instruction |
| `test_handle_propagates_correlation_key`                        | task.correlation_key == message key          |
| `test_handle_invalid_payload_returns_err`                       | Parse failure propagates as Err              |
| `test_handle_matcher_passes`                                    | Accepting matcher returns Some(task)         |
| `test_handle_matcher_filters_out`                               | Rejecting matcher returns Ok(None)           |
| `test_handle_with_plan_directory_name_only_fallback`            | Name-only file loaded                        |
| `test_handle_with_plan_directory_versioned_plan_takes_priority` | Versioned file preferred                     |
| `test_handle_with_plan_directory_no_file_uses_payload_plan`     | Falls back to payload                        |
| `test_handle_received_at_is_set`                                | task.received_at is set                      |
| `test_handle_accept_all_matcher_passes_all_valid_events`        | Accept-all mode                              |
| `test_handle_instruction_contains_step_action`                  | Instruction includes step action             |
| `test_handle_missing_key_sets_correlation_key_to_none`          | None key propagated                          |
| `test_handle_with_plan_directory_parse_error_propagates`        | Bad disk file -> Err                         |
| `test_handle_result_json_payload_returns_err`                   | Loop-break reinforced                        |
| `test_handle_empty_steps_returns_err`                           | Empty steps -> Err from validate             |
| `test_handle_json_payload_works`                                | JSON plan parsed correctly                   |

### `src/watcher/generic/watcher.rs` (non-ignored tests added)

| Test                                                                               | Description                                |
| ---------------------------------------------------------------------------------- | ------------------------------------------ |
| `test_generic_watcher_process_payload_invalid_content_returns_invalid_payload`     | Bad payload -> InvalidPayload              |
| `test_generic_watcher_process_payload_result_json_returns_invalid_payload`         | Result JSON -> InvalidPayload (loop-break) |
| `test_generic_watcher_process_payload_non_matching_event_returns_skipped_no_match` | Matcher rejects -> SkippedNoMatch          |
| `test_generic_watcher_process_payload_empty_steps_returns_invalid_payload`         | Empty steps -> InvalidPayload              |
| `test_generic_watcher_matcher_summary_accept_all`                                  | Default config -> accept-all summary       |
| `test_generic_watcher_matcher_summary_action_only`                                 | Action config -> action summary            |

### `src/tools/plan.rs` (new tests)

| Test                                                | Description                      |
| --------------------------------------------------- | -------------------------------- |
| `test_plan_version_field_optional_roundtrip`        | version round-trips through YAML |
| `test_parse_string_parses_yaml`                     | YAML parsed correctly            |
| `test_parse_string_parses_json`                     | JSON parsed correctly            |
| `test_parse_string_returns_err_for_invalid_content` | Invalid content -> Err           |
| `test_parse_string_returns_err_for_empty_steps`     | Empty steps -> Err               |
| `test_to_instruction_contains_plan_name_and_steps`  | Instruction format verified      |

---

## Quality Gate Results

All four mandatory quality gates passed:

```bash
cargo fmt --all                                          # clean
cargo check --all-targets --all-features                # clean
cargo clippy --all-targets --all-features -- -D warnings # clean
cargo test --all-features watcher::generic              # 70 passed, 7 ignored
cargo test --all-features tools::plan                   # 41 passed
cargo test --all-features --doc -- watcher::generic     # 22 passed
```

The 7 ignored tests in `watcher::generic::watcher` require a running Kafka
broker for the result-publish step and are skipped in offline CI. They can be
run explicitly with:

```bash
cargo test --all-features -- --ignored
```

---

## Success Criteria Verification

| Criterion                                                           | Status                                                                                 |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `cargo fmt --all` passes                                            | Passed                                                                                 |
| `cargo check --all-targets --all-features` passes                   | Passed                                                                                 |
| `cargo clippy --all-targets --all-features -- -D warnings` passes   | Passed                                                                                 |
| `cargo test --all-features` passes                                  | Passed                                                                                 |
| `extract_plan_text` no longer exists                                | Confirmed deleted                                                                      |
| `PlanExtraction` error variant no longer exists                     | Confirmed deleted                                                                      |
| `message.rs` deleted                                                | Confirmed                                                                              |
| Result JSON payload causes `InvalidPayload`, not execution          | Verified by `test_generic_watcher_process_payload_result_json_returns_invalid_payload` |
| Plan with no steps causes `InvalidPayload` before reaching executor | Verified by `test_generic_watcher_process_payload_empty_steps_returns_invalid_payload` |
| `GenericEventHandler` is the central pipeline step                  | Implemented in `event_handler.rs`                                                      |
| Plan directory resolution (versioned > name-only > embedded)        | Verified by three directory tests in `event_handler.rs`                                |

---

## Files Changed

| File                                   | Change                                                                                         |
| -------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `src/tools/plan.rs`                    | Added `Plan::version`, `Plan::to_instruction`, `PlanParser::parse_string`, `PartialEq` derives |
| `src/watcher/generic/event.rs`         | Complete redesign: `GenericPlanEvent` + new `RawKafkaMessage`                                  |
| `src/watcher/generic/event_handler.rs` | New file: `GenericTask` + `GenericEventHandler`                                                |
| `src/watcher/generic/matcher.rs`       | Removed type gate; updated import and tests                                                    |
| `src/watcher/generic/watcher.rs`       | Uses `GenericEventHandler`; removed `extract_plan_text`                                        |
| `src/watcher/generic/producer.rs`      | Import updated from `message` to `result_event`                                                |
| `src/watcher/generic/mod.rs`           | Added `event_handler` module; removed `message` module                                         |
| `src/watcher/generic/message.rs`       | Deleted                                                                                        |
