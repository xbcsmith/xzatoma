# Phase 2: Extend Plan Format with `action` Field — Implementation Summary

## Overview

Phase 2 extends the XZatoma plan schema with a top-level `action` field and
introduces the generic watcher message types (`GenericPlanEvent` and
`GenericPlanResult`) that form the JSON contract between Kafka producers and the
generic watcher backend.

This document describes every change made, the rationale behind each decision,
and the verification steps performed.

---

## Tasks Completed

### Task 2.1 — Add `action` to the Existing Plan Schema

**File modified**: `src/tools/plan.rs`

An optional `action` field was added to the `Plan` struct between `description`
and `steps`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub action: Option<String>,
```

Design decisions:

- The field is `Option<String>` so all existing plan files continue to parse
  without modification (full backward compatibility).
- `skip_serializing_if = "Option::is_none"` omits the field from serialized
  output when it is absent, keeping the wire format compact.
- The field sits at the plan level (not the step level) because it is a dispatch
  label for the generic watcher, not a step instruction. `PlanStep` already has
  its own `action: String` field which describes what the step does.
- `Plan::new()` was updated to initialize `action: None`.
- The three struct literal constructions in `test_validate_errors` were updated
  to include `action: None` to keep the compiler satisfied.
- The `PlanParser::validate` function required no changes — the `action` field
  is optional and carries no validation invariant in the base parser.

### Task 2.2 — Define `GenericPlanEvent` and `GenericPlanResult`

**File created**: `src/watcher/generic/message.rs`

Two public structs were defined, both deriving `Debug`, `Clone`, `Serialize`,
`Deserialize`, and `PartialEq`.

#### `GenericPlanEvent`

The trigger message schema. A Kafka producer publishes one of these to the
watcher's input topic to request plan execution.

Fields:

| Field        | Type                        | Serde behavior      |
| ------------ | --------------------------- | ------------------- |
| `id`         | `String`                    | always present      |
| `event_type` | `String`                    | always present      |
| `name`       | `Option<String>`            | omitted when `None` |
| `version`    | `Option<String>`            | omitted when `None` |
| `action`     | `Option<String>`            | omitted when `None` |
| `plan`       | `serde_json::Value`         | always present      |
| `timestamp`  | `Option<DateTime<Utc>>`     | omitted when `None` |
| `metadata`   | `Option<serde_json::Value>` | omitted when `None` |

Two methods were added:

- `GenericPlanEvent::new(id, plan)` — constructs a minimal trigger event with
  `event_type = "plan"`, a caller-supplied `id`, the embedded plan value, the
  current UTC timestamp, and all optional matching fields set to `None`.

- `GenericPlanEvent::is_plan_event()` — returns `true` only when
  `event_type == "plan"`. This is the primary loop-break predicate: the generic
  matcher (Phase 3) calls this method before evaluating any other criteria.

#### `GenericPlanResult`

The result message schema. The watcher publishes one of these to the output
topic after plan execution completes.

Fields:

| Field              | Type                        | Serde behavior      |
| ------------------ | --------------------------- | ------------------- |
| `id`               | `String`                    | always present      |
| `event_type`       | `String`                    | always present      |
| `trigger_event_id` | `String`                    | always present      |
| `success`          | `bool`                      | always present      |
| `summary`          | `String`                    | always present      |
| `timestamp`        | `DateTime<Utc>`             | always present      |
| `plan_output`      | `Option<serde_json::Value>` | omitted when `None` |

One method was added:

- `GenericPlanResult::new(trigger_event_id, success, summary)` — constructs a
  result event with a freshly generated ULID as `id`, `event_type` hardcoded to
  `"result"`, the current UTC timestamp, and `plan_output = None`.

#### Loop-break guarantee

`event_type` on `GenericPlanResult` is hardcoded to `"result"` and cannot be
changed through the public constructor. This ensures that if the output topic
and input topic are configured to be the same topic, the watcher will consume
its own result message, call `is_plan_event()`, receive `false`, and discard the
message — without executing any plan. No infinite re-trigger loop is possible.

### Task 2.3 — Update Plan Examples and Format Documentation

**File modified**: `examples/quickstart_plan.yaml`

A top-level `action: quickstart` field was added. The file now serves as a
reference example for generic watcher producers, illustrating how the plan-level
`action` label annotates a plan for automated dispatch.

**File modified**: `docs/reference/workflow_format.md`

Changes made:

- Updated the Plan data model table to include the new `action` field with a
  description of its purpose and its relationship to `GenericMatchConfig`.
- Updated the YAML and JSON examples to include `action` fields so authors see
  the field in context.
- Added a new top-level section "Generic watcher trigger format" documenting:
  - `GenericPlanEvent` field table with types, required/optional status, and
    purpose for each field.
  - The loop-break guarantee (`event_type` discriminator behavior).
  - A minimal `GenericPlanEvent` JSON example (only required fields).
  - A full `GenericPlanEvent` JSON example (all optional fields populated).
  - A `GenericPlanResult` field table.
  - A reference to `src/watcher/generic/message.rs` for the Rust types.
- Updated the implementation notes section to reflect the addition of the
  plan-level `action` field and clarify that it is ignored by the `run` command.

### Task 2.4 — Testing

#### Tests added to `src/tools/plan.rs`

Two new tests:

- `test_quickstart_plan_roundtrip_with_action` — reads the updated
  `examples/quickstart_plan.yaml` through `PlanParser::from_file`, confirms
  `plan.action == Some("quickstart")`, and verifies that the pre-existing `name`
  and `description` fields are unchanged.

- `test_plan_action_field_optional_roundtrip` — confirms that:
  - A plan YAML without `action` parses successfully and produces
    `action = None` (backward compatibility).
  - A plan YAML with `action: deploy` deserializes to `action = Some("deploy")`.
  - The plan round-trips through JSON serialization/deserialization without
    loss.

#### Tests added to `src/watcher/generic/message.rs`

All tests are in a `#[cfg(test)]` module. Coverage is grouped by type:

**`GenericPlanEvent` tests (14 tests)**:

- `test_generic_plan_event_new_sets_event_type_to_plan` — constructor sets
  `event_type = "plan"`.
- `test_generic_plan_event_new_sets_id` — constructor preserves caller-supplied
  `id`.
- `test_generic_plan_event_new_optional_fields_are_none` — constructor leaves
  all optional matching fields as `None`.
- `test_generic_plan_event_new_sets_timestamp` — constructor sets a non-None
  timestamp.
- `test_generic_plan_event_is_plan_event_returns_true_for_plan_type` —
  loop-break predicate returns `true` for `event_type = "plan"`.
- `test_generic_plan_event_is_plan_event_returns_false_for_result_type` —
  predicate returns `false` for `event_type = "result"`.
- `test_generic_plan_event_is_plan_event_returns_false_for_unknown_type` —
  predicate returns `false` for `event_type = "unknown"`.
- `test_generic_plan_event_is_plan_event_returns_false_for_empty_type` —
  predicate returns `false` for empty string.
- `test_generic_plan_event_roundtrip_all_fields` — full round-trip with every
  optional field populated; verifies field equality after JSON cycle.
- `test_generic_plan_event_roundtrip_only_action` — confirms `name`, `version`,
  `timestamp`, and `metadata` are absent from the serialized JSON when `None`,
  and the round-trip preserves the `action` field.
- `test_generic_plan_event_roundtrip_only_name_and_version` — round-trip with
  only `name` and `version` present.
- `test_generic_plan_event_roundtrip_minimal` — round-trip with only the three
  required fields.
- `test_generic_plan_event_non_plan_event_type_roundtrips_correctly` — a
  `"result"` event type round-trips without data loss but `is_plan_event()`
  returns `false`.
- `test_generic_plan_event_plan_field_accepts_string_value` — `plan` accepts a
  JSON string.
- `test_generic_plan_event_plan_field_accepts_object_value` — `plan` accepts a
  JSON object.
- `test_generic_plan_event_plan_field_accepts_array_value` — `plan` accepts a
  JSON array.

**`GenericPlanResult` tests (8 tests)**:

- `test_generic_plan_result_new_sets_event_type_to_result` — constructor
  hardcodes `event_type = "result"`.
- `test_generic_plan_result_new_sets_trigger_event_id` — `trigger_event_id` is
  preserved.
- `test_generic_plan_result_new_generates_unique_ids` — two calls to `new`
  produce different ULIDs.
- `test_generic_plan_result_new_plan_output_defaults_to_none` — `plan_output`
  starts as `None`.
- `test_generic_plan_result_new_sets_success_flag` — `success` field is
  correctly set for both `true` and `false`.
- `test_generic_plan_result_roundtrip_all_fields` — full round-trip with
  `plan_output` populated; verifies nested JSON structure survives the cycle.
- `test_generic_plan_result_event_type_serializes_as_result` — confirms that the
  serialized JSON contains `"event_type": "result"` (loop-break guarantee at the
  wire level).
- `test_generic_plan_result_plan_output_omitted_when_none` — confirms
  `plan_output` is absent from the JSON when `None`.
- `test_generic_plan_result_timestamp_is_recent` — verifies the constructor
  timestamp is within the interval `[before, after]` of two `Utc::now()` calls.

### Task 2.5 — Module Wiring

**File modified**: `src/watcher/generic/mod.rs`

- Replaced the Phase 3 placeholder comment with accurate module-level
  documentation describing both Phase 2 message types and the Phase 3 items
  still to come.
- Added `pub mod message;` declaration.
- Added top-level re-exports
  `pub use message::{GenericPlanEvent, GenericPlanResult}` so callers can use
  `crate::watcher::generic::GenericPlanEvent` without the extra `::message` path
  segment.

---

## File Inventory

### New Files

| Path                                                          | Purpose                                    |
| ------------------------------------------------------------- | ------------------------------------------ |
| `src/watcher/generic/message.rs`                              | `GenericPlanEvent` and `GenericPlanResult` |
| `docs/explanation/phase2_plan_action_field_implementation.md` | This document                              |

### Modified Files

| Path                                | Change summary                                          |
| ----------------------------------- | ------------------------------------------------------- |
| `src/tools/plan.rs`                 | Added `action: Option<String>` to `Plan`; new tests     |
| `src/watcher/generic/mod.rs`        | Declared `message` submodule; added re-exports          |
| `examples/quickstart_plan.yaml`     | Added `action: quickstart` at top level                 |
| `docs/reference/workflow_format.md` | Documented `action` field and `GenericPlanEvent` schema |

---

## Backward Compatibility

All existing plan files are unaffected. The `action` field is optional and
defaults to `None` when absent from a YAML or JSON document. Existing tests that
construct `Plan` struct literals directly were updated to include `action: None`
— no behavior change, only struct initialization completeness.

---

## Dependencies Used

No new crate dependencies were added. Phase 2 uses only crates already present
in `Cargo.toml`:

| Crate        | Usage                                      |
| ------------ | ------------------------------------------ |
| `serde`      | `Serialize` / `Deserialize` for both types |
| `serde_json` | `serde_json::Value` for the `plan` field   |
| `chrono`     | `DateTime<Utc>` for timestamps             |
| `ulid`       | `Ulid::new()` to generate result event IDs |

---

## Quality Gate Results

Run in the order specified by AGENTS.md:

```text
cargo fmt --all                                          -- pass
cargo check --all-targets --all-features                -- pass
cargo clippy --all-targets --all-features -- -D warnings -- pass
cargo test --all-features                               -- pass
```

Markdown linting and formatting:

```text
markdownlint --fix --config .markdownlint.json docs/reference/workflow_format.md
prettier --write --parser markdown --prose-wrap always docs/reference/workflow_format.md
markdownlint --fix --config .markdownlint.json docs/explanation/phase2_plan_action_field_implementation.md
prettier --write --parser markdown --prose-wrap always docs/explanation/phase2_plan_action_field_implementation.md
```

---

## Phase 3 Preview

Phase 3 will add the generic watcher core to `src/watcher/generic/`:

- `matcher.rs` — `GenericMatcher` evaluates `action`, `name`, `version`
  combinations from `GenericMatchConfig` against an incoming `GenericPlanEvent`.
- `producer.rs` — `GenericResultProducer` serializes a `GenericPlanResult` and
  publishes it to the configured output topic.
- `watcher.rs` — `GenericWatcher` ties together a Kafka consumer, the matcher,
  the plan executor, and the result producer into a running event loop.

The message types defined here in Phase 2 are the stable contract that Phase 3
builds on. No changes to `GenericPlanEvent` or `GenericPlanResult` are expected
in Phase 3.

---

## References

- Implementation plan: `docs/explanation/generic_watcher_implementation_plan.md`
- Phase 1 summary:
  `docs/explanation/phase1_xzepr_watcher_restructure_implementation.md`
- Workflow format reference: `docs/reference/workflow_format.md`
- Plan implementation: `src/tools/plan.rs`
- Generic message types: `src/watcher/generic/message.rs`
