# Generic Watcher Implementation Plan

## Overview

Atoma's watcher module is currently purpose-built for Polaris pipeline events.
This plan adds a second, fully generic watcher mode that consumes a plan
serialised as a JSON event from a Kafka/Redpanda topic, executes it via the
existing agent engine, and publishes the result to a (possibly identical) output
topic. As part of the work, all Polaris-specific watcher components are
relocated into a `polaris/` subdirectory of the `watcher` module, and a new
`WatcherType` discriminant in config selects which mode is active. The result is
that operators choose Polaris-integrated or standalone Kafka-driven execution
through configuration alone, with no code changes required at the call sites.

### User-Facing Documentation

The following documents are maintained alongside this implementation plan and
must be kept up to date as each phase lands:

| Document             | Location                                       | Purpose                                                                                                                                                      |
| -------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| How-to guide         | `docs/how-to/use_generic_watcher.md`           | End-to-end setup, configuration, producing plans, consuming results, topic design patterns, troubleshooting                                                  |
| Data types reference | `docs/reference/generic_watcher_data_types.md` | Complete schemas for `Plan`, `Task`, `PlanResultEvent`, `GenericEventMatcher`, `GenericWatcherConfig`, `KafkaConfig`; JSON Schema; version constraint syntax |

Service developers who need to produce plan messages to the Kafka input topic or
consume `PlanResultEvent` messages from the output topic should start with
`docs/how-to/use_generic_watcher.md`. The data types reference at
`docs/reference/generic_watcher_data_types.md` provides the authoritative field
tables and JSON Schema definitions for use in client code generation, validation
libraries, and integration tests.

## Current State Analysis

### Existing Infrastructure

The `watcher` module (`src/watcher/`) contains eight files, all of which have a
hard dependency on `polaris_core::events::PolarisEvent` or the surrounding
Polaris crate family:

| File                 | Role                                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------------- |
| `mod.rs`             | Public facade; owns `BackOffPolicy`, `WatcherTask`, `extract_metadata`                         |
| `circuit_breaker.rs` | Generic exponential-decay circuit breaker; no Polaris deps                                     |
| `matcher.rs`         | `EventMatcher` - rich predicate matching against `PolarisEvent` fields                         |
| `event_handler.rs`   | Translates `PolarisEvent` into `WatcherTask` via `PlanResolver`                                |
| `plan_resolver.rs`   | Maps gate name + version to a plan file path on disk                                           |
| `receipt_builder.rs` | Builds `polaris_core::models::Receipt` from `RunResult`                                        |
| `gatekeeper.rs`      | Wraps `polaris_client::GatekeeperClient`                                                       |
| `event_producer.rs`  | Kafka producer for `ReceiptCreated` status events                                              |
| `fake_consumer.rs`   | `EventConsumerTrait` + `RealEventConsumer` + `FakeEventConsumer` (all typed to `PolarisEvent`) |

`WatcherConfig` in `src/config.rs` conflates shared Kafka settings with
Polaris-only settings (`janus`, `receipt`, topic list, `publish_events`,
`matcher`). The struct is flat; there is no `WatcherType` discriminant and no
`polaris` or `generic` sub-section.

`commands/watch.rs` owns `WatcherLoopContext` and `run_watcher_loop`, both
tightly coupled to the Polaris flow: receipt building, Janus posting, and
`ReceiptCreated` publishing are unconditional.

The `Plan` struct in `src/commands/plan_parser.rs` has no `action` or `version`
fields, which are required by the generic matcher.

`rdkafka` is only a transitive dependency through `polaris-events`; the generic
consumer needs it directly to consume raw JSON messages.

### Demo Files

The demo directories `demo/watcher/polaris/` and `demo/watcher/generic/` already
exist in the repository and contain functional scripts and config files that
reflect the target design. Their config files (`config.yaml`,
`config_ollama_granite.yaml`) already use the `watcher_type: generic` and
`watcher.generic.*` keys that Phase 4 will implement in `src/config.rs`. These
files do not need to be created; they need to be verified and updated where the
final implementation diverges from their current content.

### Identified Issues

- All watcher code assumes Polaris semantics; adding a second watcher type
  requires significant untangling before new code can be written cleanly.
- `WatcherTask` carries `event: PolarisEvent` at the top level of `watcher/`,
  leaking Polaris types into the shared namespace.
- `WatcherConfig` mixes concerns; a `GenericWatcherConfig` sub-section cannot be
  added without restructuring.
- There is no `action` or `version` field on `Plan`, blocking the generic
  matcher from operating on plan events.
- `rdkafka` is only a transitive dependency through `polaris-events`; the
  generic consumer needs it directly.
- `version_matches` is a private function inside `watcher/matcher.rs`; the
  generic matcher needs the same logic and must share it from a common location.
- `max_concurrent_plans` exists in `WatcherConfig` but neither
  `run_watcher_loop` nor the planned `run_generic_watcher_loop` enforces it.
  Both phases use single-event-at-a-time processing; the config value is
  reserved for future use and must be validated as `> 0` without being wired to
  a real concurrency mechanism in this plan.

---

## Implementation Phases

### Phase 1: Plan Schema Extension

**Depends on:** Nothing. This phase has no prerequisites.

Extends the `Plan` and `Task` structs with the fields required by the generic
watcher matcher and restores explicit task dependency support as a required part
of the plan schema. Task identity and task dependencies are mandatory for all
plans after this phase.

#### 1.1 Add `id` and `dependencies` to `Task`

**File:** `src/commands/plan_parser.rs`

Locate the `Task` struct. Restore explicit task identity and dependency support
by adding two fields after `images`:

```rust
/// Stable identifier used for cross-task references and execution ordering.
/// Required for every task in every plan.
pub id: String,

/// IDs of tasks that must complete before this task may execute.
/// Empty means the task has no prerequisites.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub dependencies: Vec<String>,
```

These fields are required semantics for the live schema:

- Every task must have a non-empty `id`.
- `dependencies` may be empty, but the field must be supported for every task.
- Demo plans and user-facing examples must be updated to include task IDs.

Validation rules added in `Plan::validate()`:

- Every task ID must be non-empty after trimming.
- Duplicate task IDs are rejected.
- Every dependency name must match the `id` of some task in the same plan.
- A task may not depend on itself.
- Cycles in the dependency graph are rejected.
- The dependency graph must be executable in a deterministic topological order.
- Plans that omit `id` must fail validation; there is no backwards-compatibility
  path in this implementation.

Execution semantics restored by this phase:

- Tasks execute in topological order derived from `dependencies`.
- When multiple tasks are ready at the same time, the executor preserves the
  original task-list order as the deterministic tiebreaker.
- A task may execute only after all dependency tasks have completed
  successfully.
- If a prerequisite task fails, every downstream dependent task is skipped and
  reported as blocked by dependency failure rather than executed speculatively.
- Plans with no dependency edges still execute in listed order, which is the
  degenerate case of the topological scheduler.

This phase intentionally tightens the schema: task IDs are mandatory,
dependency references are validated as first-class workflow structure, and task
execution order is defined by the dependency graph rather than by prompt text
alone.

#### 1.2 Add `action` and `version` to `Plan`

**File:** `src/commands/plan_parser.rs`

Locate the `Plan` struct. Add two new fields after `allow_dangerous`:

```rust
/// Optional action label used by the generic watcher matcher.
/// Ignored by the Polaris watcher and the run/chat commands.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub action: Option<String>,

/// Optional semantic version string used by the generic watcher matcher.
/// Expected format: a valid semver string such as `"1.0.0"` or `"2.3.1"`.
/// Ignored by the Polaris watcher and the run/chat commands.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub version: Option<String>,
```

Both fields are `Option<String>` with `#[serde(default)]`. Neither field needs
validation beyond the existing `Plan::validate()` checks. Do NOT add length or
format validation for `action` or `version` in `Plan::validate()`.

#### 1.3 Surface `action`, `version`, and dependencies in `Plan::to_instruction`

**File:** `src/commands/plan_parser.rs`

In `Plan::to_instruction()`, after the `description` block and before the
`context` block, insert:

```rust
if self.action.is_some() || self.version.is_some() {
    if let Some(ref a) = self.action {
        instruction.push_str(&format!("Action: {}\n", a));
    }
    if let Some(ref v) = self.version {
        instruction.push_str(&format!("Version: {}\n", v));
    }
}
```

Also update the task-rendering loop so task identity and dependency information
are always visible to the agent. Keep the existing task numbering, priority,
optional context, and optional image-count annotations. Every rendered task must
include its required task ID, and tasks with dependencies must include a compact
dependency annotation, for example:

```rust
instruction.push_str(&format!(
    "{}. [{}] {} (task id: {})",
    index + 1,
    task.priority,
    task.description,
    task.id,
));

if !task.dependencies.is_empty() {
    instruction.push_str(&format!(" (depends on: {})", task.dependencies.join(", ")));
}
```

This is required so downstream prompts, report-writing tasks, and humans can
correlate dependency references with the rendered instruction.

#### 1.4 Testing Requirements

All new tests for this phase are added as functions inside the existing
`mod tests` block at the bottom of `src/commands/plan_parser.rs`. Do not create
a new test file for this phase.

Required test functions:

- `test_task_id_round_trips_yaml` - serialise a `Plan` with a task
  `id: "collect".to_string()` to YAML and deserialise it back; assert the field
  is preserved.
- `test_task_dependencies_round_trip_yaml` - serialise a `Plan` with
  `dependencies: vec!["collect".to_string()]`; assert the list round-trips.
- `test_task_id_and_dependencies_round_trip_json` - both fields round-trip
  through JSON.
- `test_plan_validation_passes_with_valid_dependencies` - parse a plan whose
  later task depends on an earlier task; assert `validate()` succeeds.
- `test_plan_validation_rejects_missing_task_id` - parse a YAML string with a
  task that omits `id`; assert validation fails.
- `test_plan_validation_rejects_empty_task_id` - parse a YAML string with
  `id: ""`; assert validation fails.
- `test_plan_validation_rejects_unknown_dependency` - dependency references a
  missing task ID; assert `validate()` fails with a dependency-focused error.
- `test_plan_validation_rejects_duplicate_task_ids` - two tasks share an ID;
  assert `validate()` fails.
- `test_plan_validation_rejects_self_dependency` - a task depends on its own ID;
  assert `validate()` fails.
- `test_plan_validation_rejects_dependency_cycle` - create a small cycle such as
  `a -> b -> a`; assert `validate()` fails.
- `test_plan_to_instruction_includes_required_task_id` - construct a `Plan`
  with valid task IDs; call `to_instruction()`; assert each rendered task
  includes `(task id: ...)`.
- `test_plan_action_field_round_trips_yaml` - serialise a `Plan` with
  `action: Some("deploy")` to YAML and deserialise it back; assert the field is
  preserved.
- `test_plan_version_field_round_trips_yaml` - same as above for
  `version: Some("1.2.3")`.
- `test_plan_action_version_round_trips_json` - both fields round-trip through
  JSON.
- `test_plan_to_instruction_includes_action_and_version_when_set` - construct a
  `Plan` with both fields set; call `to_instruction()`; assert the output
  contains `"Action: deploy"` and `"Version: 1.2.3"`.
- `test_plan_to_instruction_includes_task_id_and_dependencies_when_set` -
  construct a `Plan` with task IDs and dependencies; call `to_instruction()`;
  assert the rendered instruction includes both annotations.
- `test_plan_to_instruction_omits_action_version_block_when_absent` - construct
  a `Plan` with both fields `None`; call `to_instruction()`; assert the output
  does not contain the string `"Action:"` or `"Version:"`.
- `test_plan_validation_passes_when_action_and_version_absent` - a minimal valid
  `Plan` with no `action` or `version` passes `validate()`.
- `test_plan_action_and_version_default_to_none_when_absent` - parse a YAML
  string that omits both fields; assert both are `None`.

#### 1.5 Deliverables

| File                                  | Change                                                                                                                                                        |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/commands/plan_parser.rs`         | Add required `Task.id` and validated `Task.dependencies`; add `Plan.action` and `Plan.version`; update `validate()` and `to_instruction()`; add Phase 1 tests |
| `docs/explanation/implementations.md` | Prepend a new entry following Rule 8 format                                                                                                                   |

#### 1.6 Success Criteria

- `cargo test` passes with no failures.
- `cargo clippy --all-targets --all-features -- -D warnings` emits no warnings.
- All demo plans and user-facing plan examples are updated to include required
  task IDs and parse successfully.
- Plans that omit task IDs fail validation.
- Plans that use `dependencies` fail fast on missing IDs, duplicate IDs,
  self-dependencies, and cycles.
- Tasks execute in deterministic topological order, using original task-list
  order as the tiebreaker when multiple tasks are simultaneously ready.
- Downstream tasks are skipped and reported clearly when an upstream dependency
  fails.
- New `action` and `version` fields appear in the instruction string when set
  and are absent when not set.
- Required task IDs always appear in the instruction string.
- Dependency annotations appear in the instruction string when present.
- `grep -r "dependencies\|action\|version" src/commands/plan_parser.rs --include="*.rs"` shows all restored schema fields and the corresponding validation and instruction rendering logic.

---

### Phase 2: Polaris Watcher Migration

**Depends on:** Phase 1 must be complete and the build must be green.

Relocates every Polaris-specific component into `src/watcher/polaris/`, leaving
the `watcher/` top level as a shared namespace containing only
`circuit_breaker`, the shared `version_matches` helper, `BackOffPolicy`, and the
module facade.

#### 2.1 Extract `version_matches` to `watcher/mod.rs`

**File:** `src/watcher/matcher.rs` (source), `src/watcher/mod.rs` (destination)

The private function `version_matches` in `src/watcher/matcher.rs` (currently
near L428) implements semver constraint matching. Both the Polaris
`EventMatcher` and the new `GenericEventMatcher` need this logic.

**Exact steps:**

1. Copy the complete body of `version_matches` and its immediate helper
   `release_matches` / `parse_release_expr` / `ReleaseOp` (all private items in
   `matcher.rs` that `version_matches` calls) to `src/watcher/mod.rs`.
2. Change the visibility of `version_matches` in `watcher/mod.rs` to
   `pub(crate)`.
3. In `src/watcher/matcher.rs`, replace the function body with a delegation:

   ```rust
   fn version_matches(version: &str, constraint: &str) -> bool {
       crate::watcher::version_matches(version, constraint)
   }
   ```

   Keep `release_matches`, `parse_release_expr`, and `ReleaseOp` in `mod.rs`
   only (not duplicated in `matcher.rs`).

4. Update all `version_matches` call sites inside `matcher.rs` to call the local
   delegating wrapper; no external call-site changes are required at this step.

After Phase 3, `GenericEventMatcher` will call `crate::watcher::version_matches`
directly.

#### 2.2 Create the `polaris` Submodule

Create directory `src/watcher/polaris/` and file `src/watcher/polaris/mod.rs`
with the following content:

```rust
pub mod consumer;
pub mod event_handler;
pub mod event_producer;
pub mod gatekeeper;
pub mod matcher;
pub mod plan_resolver;
pub mod receipt_builder;

pub use consumer::{EventConsumerTrait, FakeEventConsumer, RealEventConsumer};
pub use event_handler::EventHandler;
pub use event_producer::{StatusProducer, build_receipt_created_event};
pub use gatekeeper::GatekeeperService;
pub use matcher::EventMatcher;
pub use plan_resolver::PlanResolver;
pub use receipt_builder::ReceiptBuilder;
```

#### 2.3 Move Polaris-Specific Files

Move files using the following mapping. After moving, update every `use` path
inside each moved file to reflect the new module hierarchy
(`crate::watcher::polaris::...` where previously `crate::watcher::...`).

| Old path                         | New path                                 |
| -------------------------------- | ---------------------------------------- |
| `src/watcher/event_handler.rs`   | `src/watcher/polaris/event_handler.rs`   |
| `src/watcher/plan_resolver.rs`   | `src/watcher/polaris/plan_resolver.rs`   |
| `src/watcher/receipt_builder.rs` | `src/watcher/polaris/receipt_builder.rs` |
| `src/watcher/gatekeeper.rs`      | `src/watcher/polaris/gatekeeper.rs`      |
| `src/watcher/event_producer.rs`  | `src/watcher/polaris/event_producer.rs`  |
| `src/watcher/fake_consumer.rs`   | `src/watcher/polaris/consumer.rs`        |
| `src/watcher/matcher.rs`         | `src/watcher/polaris/matcher.rs`         |

`extract_metadata` in `watcher/mod.rs` accepts a `&PolarisEvent`; move it to
`src/watcher/polaris/mod.rs` (append after the `pub use` block) and delete it
from `watcher/mod.rs`.

`WatcherTask` in `watcher/mod.rs` holds `event: PolarisEvent`. Move the entire
struct and its `impl` blocks to `src/watcher/polaris/mod.rs` and add
`pub use polaris::WatcherTask;` in `watcher/mod.rs` so that `commands/watch.rs`
import paths are unaffected in this phase.

#### 2.4 Update `watcher/mod.rs`

After migration, `src/watcher/mod.rs` declares only the following. Delete all
existing content and replace with:

```rust
//! Watcher module: event-driven plan execution.
//!
//! This module provides two watcher modes:
//!
//! - [`polaris`] - Polaris pipeline event watcher; consumes `PolarisEvent`s,
//!   posts receipts to the Janus Gatekeeper API.
//! - [`generic`] - Standalone Kafka watcher; consumes raw JSON plan payloads
//!   and publishes `PlanResultEvent` results.
//!
//! Shared infrastructure:
//! - [`circuit_breaker`] - Exponential-decay circuit breaker used by both loops
//! - [`BackOffPolicy`] - Exponential back-off for consecutive consumer errors
//! - [`version_matches`] - Semver constraint helper used by both matchers

pub mod circuit_breaker;
pub mod generic;
pub mod polaris;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use polaris::WatcherTask;

// BackOffPolicy definition remains here (unchanged from original watcher/mod.rs).
// Copy the full struct, impl block, and doc comments verbatim.
pub struct BackOffPolicy { ... }

// version_matches and its helpers (moved from polaris/matcher.rs in step 2.1)
pub(crate) fn version_matches(version: &str, constraint: &str) -> bool { ... }
// release_matches, parse_release_expr, ReleaseOp go here too (all pub(crate) or private)
```

The `generic` module declaration is added now so the file compiles once Phase 3
creates `src/watcher/generic/mod.rs`. Until Phase 3 is complete, adding
`pub mod generic;` will cause a compile error; therefore create an empty
`src/watcher/generic/mod.rs` as a placeholder at the END of Phase 2.

#### 2.5 Update `commands/watch.rs` Import Paths

Replace all `use crate::watcher::{...}` imports that reference Polaris-specific
types with `use crate::watcher::polaris::{...}`. Specifically:

- `use crate::watcher::event_handler::EventHandler;` becomes
  `use crate::watcher::polaris::EventHandler;`
- `use crate::watcher::event_producer::StatusProducer;` becomes
  `use crate::watcher::polaris::StatusProducer;`
- `use crate::watcher::fake_consumer::{EventConsumerTrait, RealEventConsumer};`
  becomes
  `use crate::watcher::polaris::{EventConsumerTrait, RealEventConsumer};`
- `use crate::watcher::gatekeeper::GatekeeperService;` becomes
  `use crate::watcher::polaris::GatekeeperService;`
- `use crate::watcher::matcher::EventMatcher;` becomes
  `use crate::watcher::polaris::EventMatcher;`
- `use crate::watcher::receipt_builder::ReceiptBuilder;` becomes
  `use crate::watcher::polaris::ReceiptBuilder;`

The import `use crate::watcher::BackOffPolicy;` and
`use crate::watcher::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};`
remain unchanged (those types stay at the top level).

No logic changes are required in `commands/watch.rs` during this phase; the goal
is a green build after the file moves.

#### 2.6 Update `config.rs` References to Polaris Matcher Type

In `src/config.rs`, the `WatcherConfig.matcher` field currently references
`crate::watcher::matcher::EventMatcher`. Update this to
`crate::watcher::polaris::EventMatcher` (or the re-exported path
`crate::watcher::polaris::matcher::EventMatcher`). The type name and behaviour
are unchanged.

#### 2.7 Testing Requirements

- `cargo check --all-targets --all-features` must pass with zero errors after
  all moves.
- All existing tests in the moved files must pass. Because moving files changes
  `use` paths inside tests, update any `use super::*` or absolute paths within
  the moved test modules to use the new paths.
- Run `cargo clippy --all-targets --all-features -- -D warnings` and fix all
  path-related warnings.

#### 2.8 Deliverables

| File / Directory                      | Change                                            |
| ------------------------------------- | ------------------------------------------------- |
| `src/watcher/polaris/` (new dir)      | Create with `mod.rs` and all seven migrated files |
| `src/watcher/generic/mod.rs` (new)    | Empty placeholder file                            |
| `src/watcher/mod.rs`                  | Rewritten facade; `version_matches` moved here    |
| `src/commands/watch.rs`               | Import paths updated to `watcher::polaris::*`     |
| `src/config.rs`                       | `WatcherConfig.matcher` type path updated         |
| `docs/explanation/implementations.md` | New entry following Rule 8 format                 |

#### 2.9 Success Criteria

- `cargo test` passes with no regressions.
- No Polaris type is imported at `crate::watcher::` scope (verify with
  `rg "polaris_core|polaris_events" src/watcher/mod.rs src/watcher/circuit_breaker.rs`
  returning zero results).
- `src/watcher/` root contains only: `mod.rs`, `circuit_breaker.rs`, `polaris/`,
  and `generic/` (placeholder).
- The following command returns zero results:

  ```bash
  rg "use crate::watcher::event_handler\|use crate::watcher::fake_consumer\|use crate::watcher::matcher\|use crate::watcher::receipt_builder\|use crate::watcher::gatekeeper\|use crate::watcher::event_producer" src/
  ```

---

### Phase 3: Generic Watcher Core

**Depends on:** Phase 2 must be complete and the build must be green.

Implements the new generic Kafka/Redpanda watcher components in
`src/watcher/generic/`, replacing the empty placeholder created at the end of
Phase 2.

#### 3.1 Replace the `generic` Placeholder with the Full Submodule

Replace the empty `src/watcher/generic/mod.rs` with:

```rust
pub mod consumer;
pub mod event;
pub mod event_handler;
pub mod matcher;
pub mod result_event;
pub mod result_producer;

pub use consumer::{FakeGenericConsumer, GenericConsumerTrait, RealGenericConsumer};
pub use event::GenericPlanEvent;
pub use event_handler::{GenericEventHandler, GenericTask};
pub use matcher::GenericEventMatcher;
pub use result_event::PlanResultEvent;
pub use result_producer::{FakeResultProducer, ResultProducer, ResultProducerTrait};
```

Create the seven files described in sections 3.2 through 3.8 below.

#### 3.2 Create `generic/event.rs` - `GenericPlanEvent`

`GenericPlanEvent` is the in-process representation of a message consumed from
the input topic. The Kafka message payload is a `Plan` serialised as JSON.

```rust
use crate::commands::plan_parser::{Plan, PlanParser};
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct GenericPlanEvent {
    /// The plan deserialised from the Kafka message payload.
    pub plan: Plan,
    /// Name of the Kafka topic the message arrived on.
    pub source_topic: String,
    /// Optional Kafka message key used as correlation ID when present.
    pub key: Option<String>,
    /// RFC-3339 timestamp of when the message was received.
    pub received_at: chrono::DateTime<chrono::Utc>,
}

impl GenericPlanEvent {
    /// Deserialise a plan from a raw JSON string and construct the event.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `payload` cannot be parsed as a valid `Plan`.
    pub fn new(payload: &str, topic: String, key: Option<String>) -> Result<Self> {
        let plan = PlanParser::default().parse_string(payload)?;
        Ok(Self {
            plan,
            source_topic: topic,
            key,
            received_at: chrono::Utc::now(),
        })
    }
}
```

#### 3.3 Create `generic/matcher.rs` - `GenericEventMatcher`

```rust
use serde::{Deserialize, Serialize};
use crate::watcher::generic::event::GenericPlanEvent;

/// Predicate matcher for generic plan events.
///
/// An instance with no fields set never matches (identical behaviour to
/// `polaris::EventMatcher`). When one or more predicates are set, ALL
/// non-`None` predicates must pass simultaneously (logical AND).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenericEventMatcher {
    /// Match `plan.action` exactly (case-insensitive string equality).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    /// Match `plan.name` exactly (case-insensitive string equality).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Match `plan.version` using semver constraint syntax (e.g. `">=1.0.0"`).
    /// Uses the same `version_matches` helper shared with `polaris::EventMatcher`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
```

Implement `GenericEventMatcher::has_predicates(&self) -> bool` returning `true`
when any field is `Some`.

Implement
`GenericEventMatcher::matches(&self, event: &GenericPlanEvent) -> bool` with the
following rules:

1. If `!self.has_predicates()`, return `false`.
2. If `self.action` is `Some(a)` and `event.plan.action` is `None` or does not
   equal `a` (case-insensitive), return `false`.
3. If `self.name` is `Some(n)` and `event.plan.name` does not equal `n`
   (case-insensitive), return `false`.
4. If `self.version` is `Some(v)` and either `event.plan.version` is `None` or
   `crate::watcher::version_matches(plan_version, v)` returns `false`, return
   `false`.
5. Return `true`.

Do NOT call into `polaris::matcher` for any logic. Use
`crate::watcher::version_matches` directly (extracted in Phase 2.1).

#### 3.4 Create `generic/consumer.rs` - `GenericConsumerTrait`

Define a trait for generic message consumption and two implementations:

```rust
use async_trait::async_trait;
use crate::error::Result;
use std::collections::VecDeque;

/// A raw Kafka message with its topic and optional key.
#[derive(Debug, Clone)]
pub struct RawKafkaMessage {
    /// UTF-8 payload string.
    pub payload: String,
    /// Source topic name.
    pub topic: String,
    /// Optional message key used as a correlation ID.
    pub key: Option<String>,
}

/// Trait abstracting generic Kafka message consumption.
#[async_trait]
pub trait GenericConsumerTrait: Send {
    /// Block until the next message is available.
    /// Returns `None` to signal graceful shutdown.
    async fn next(&mut self) -> Option<Result<RawKafkaMessage>>;
    /// Commit the offset of the most-recently-returned message.
    async fn commit(&mut self) -> Result<()>;
}
```

`RealGenericConsumer` wraps `rdkafka::consumer::StreamConsumer`. Add `rdkafka`
as a direct dependency using:

```bash
cargo add rdkafka --no-default-features --features "cmake-build,tokio"
```

If the build environment does not support CMake, substitute
`--features "dynamic-linking,tokio"` and document the choice in a comment. The
feature set must be consistent with whatever `polaris-events` uses for `rdkafka`
to avoid duplicate compiled copies; verify after adding by running
`cargo tree -d` and confirming `rdkafka` appears once.

Build a constructor with the following signature:

```rust
fn new(kafka: &crate::config::KafkaConfig, input_topic: &str) -> Result<Self>
```

It constructs an `rdkafka::ClientConfig` from the `KafkaConfig` fields:
`bootstrap.servers`, `group.id`, `security.protocol`, and the optional SASL and
SSL fields. Subscribe to `[input_topic]`.

`FakeGenericConsumer` stores `VecDeque<Result<RawKafkaMessage>>` and is used in
tests. It must be declared `pub` (not `#[cfg(test)]`) because integration tests
outside `src/` need it. Mirror the visibility of `FakeEventConsumer` in
`watcher/polaris/consumer.rs`.

```rust
pub struct FakeGenericConsumer {
    queue: VecDeque<Result<RawKafkaMessage>>,
    commits: usize,
}

impl FakeGenericConsumer {
    pub fn new(messages: Vec<Result<RawKafkaMessage>>) -> Self { ... }
    pub fn commits_recorded(&self) -> usize { self.commits }
}
```

#### 3.5 Create `generic/event_handler.rs` - `GenericEventHandler` and `GenericTask`

```rust
use std::path::PathBuf;
use crate::commands::plan_parser::{Plan, PlanParser};
use crate::error::Result;
use crate::watcher::generic::consumer::RawKafkaMessage;
use crate::watcher::generic::event::GenericPlanEvent;
use crate::watcher::generic::matcher::GenericEventMatcher;

pub struct GenericEventHandler {
    matcher: Option<GenericEventMatcher>,
    /// When `Some`, the plan from the event payload is validated against a
    /// matching file on disk located at `{plan_directory}/{plan.name}.yaml` or
    /// `{plan_directory}/{plan.name}-{plan.version}.yaml`. If found, the
    /// on-disk plan REPLACES the event payload plan. If not found, the event
    /// payload plan is used as-is. When `None`, the event payload plan is
    /// always used directly.
    plan_directory: Option<PathBuf>,
}

/// An executable task produced from a consumed plan event.
#[derive(Debug, Clone)]
pub struct GenericTask {
    pub plan: Plan,
    pub instruction: String,
    /// Correlation key from the originating Kafka message.
    pub correlation_key: Option<String>,
    pub received_at: chrono::DateTime<chrono::Utc>,
}
```

Implement
`GenericEventHandler::handle(&self, msg: RawKafkaMessage) -> Result<Option<GenericTask>>`:

1. Construct
   `GenericPlanEvent::new(&msg.payload, msg.topic.clone(), msg.key.clone())`.
   Return `Err` on parse failure.
2. If `self.matcher` is `Some(m)` and `!m.matches(&event)`, return `Ok(None)`
   (filtered; caller must commit and continue).
3. Resolve the plan:
   - If `self.plan_directory` is `Some(dir)`, attempt to load the plan from disk
     in priority order: a. `{dir}/{plan.name}-{plan.version}.yaml` (only if
     `plan.version` is `Some`) b. `{dir}/{plan.name}.yaml` If a file is found,
     parse it with `PlanParser::default().parse_file(path)?` and use the
     resulting plan. If no file is found, use `event.plan` as-is.
   - If `self.plan_directory` is `None`, use `event.plan` as-is.
4. Call `resolved_plan.validate()?`.
5. Build `instruction = resolved_plan.to_instruction()`.
6. Return the following value:

   ```rust
   Ok(Some(GenericTask {
       plan: resolved_plan,
       instruction,
       correlation_key: msg.key,
       received_at: event.received_at,
   }))
   ```

Add a constructor:

```rust
impl GenericEventHandler {
    pub fn new(
        matcher: Option<GenericEventMatcher>,
        plan_directory: Option<PathBuf>,
    ) -> Self { ... }
}
```

#### 3.6 Create `generic/result_event.rs` - `PlanResultEvent`

The output message published to the results topic after each plan execution:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanResultEvent {
    /// ULID for this result event. Generate with `ulid::Ulid::new().to_string()`.
    pub id: String,
    /// Name from the originating plan.
    pub name: String,
    /// Action from the originating plan, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Version from the originating plan, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Correlation key from the originating Kafka message, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_key: Option<String>,
    /// `"success"` when `RunResult.success` is `true`, otherwise `"failure"`.
    pub status: String,
    /// Final summary output from the agent run (`RunResult.output`).
    pub summary: String,
    /// Number of agent iterations consumed (`RunResult.iterations`).
    pub iterations: usize,
    /// RFC-3339 completion timestamp. Use `chrono::Utc::now().to_rfc3339()`.
    pub completed_at: String,
}
```

Implement the following associated function:

```rust
fn from_task_and_result(
    task: &GenericTask,
    result: &crate::commands::run::RunResult,
) -> Self
```

Set fields as follows:

- `id = ulid::Ulid::new().to_string()`
- `name = task.plan.name.clone()`
- `action = task.plan.action.clone()`
- `version = task.plan.version.clone()`
- `correlation_key = task.correlation_key.clone()`
- `status = if result.success { "success" } else { "failure" }`
- `summary = result.output.clone()`
- `iterations = result.iterations`
- `completed_at = chrono::Utc::now().to_rfc3339()`

#### 3.7 Create `generic/result_producer.rs` - `ResultProducerTrait`, `ResultProducer`, `FakeResultProducer`

Define a trait so the loop can be tested without a real Kafka broker:

```rust
use async_trait::async_trait;
use crate::error::Result;
use crate::watcher::generic::result_event::PlanResultEvent;
use std::time::Duration;
use std::sync::Mutex;

#[async_trait]
pub trait ResultProducerTrait: Send + Sync {
    async fn publish(&self, event: &PlanResultEvent) -> Result<()>;
    async fn flush(&self, timeout: Duration) -> Result<()>;
}
```

`ResultProducer` wraps `rdkafka::producer::FutureProducer`:

```rust
pub struct ResultProducer {
    producer: rdkafka::producer::FutureProducer,
    output_topic: String,
}
```

Implement:

- `ResultProducer::new(kafka: &KafkaConfig, output_topic: String) -> Result<Self>`
  - build an `rdkafka::ClientConfig` the same way as `RealGenericConsumer::new`.
- `publish` - serialise `event` with `serde_json::to_string(event)?` and produce
  with a `None` key. Use `AtomaError::Kafka` to wrap rdkafka errors.
- `flush` - call `self.producer.flush(timeout)` and map errors to
  `AtomaError::Kafka`.

`FakeResultProducer` records published events for use in tests:

```rust
pub struct FakeResultProducer {
    published: Mutex<Vec<PlanResultEvent>>,
}

impl FakeResultProducer {
    pub fn new() -> Self { ... }
    pub fn published_events(&self) -> Vec<PlanResultEvent> { ... }
}
```

`FakeResultProducer` must be `pub` (not `#[cfg(test)]`) because integration
tests outside `src/` need it.

#### 3.8 `rdkafka` Dependency

Add `rdkafka` as a direct `[dependencies]` entry using `cargo add` (not manual
`Cargo.toml` editing, per Rule 4):

```bash
cargo add rdkafka --no-default-features --features "cmake-build,tokio"
```

After adding, run `cargo tree -d | grep rdkafka` and confirm only one version
appears. If two versions appear, align the feature set with what
`polaris-events` uses or pin to the same version.

#### 3.9 Testing Requirements

All tests for this phase are placed in `tests/unit/watcher_generic.rs`. Create
this file if it does not exist. Add `mod watcher_generic;` to
`tests/unit/mod.rs` (or equivalent test module entry point).

Required test functions:

- `test_generic_event_matcher_no_predicates_never_matches` - empty matcher
  returns `false`.
- `test_generic_event_matcher_action_only` - action matches case-insensitively;
  wrong action returns `false`.
- `test_generic_event_matcher_name_only` - name matches case-insensitively;
  wrong name returns `false`.
- `test_generic_event_matcher_version_constraint` - `">=1.0.0"` matches
  `"1.2.0"`; `"<1.0.0"` does not.
- `test_generic_event_matcher_name_and_version` - both must pass simultaneously.
- `test_generic_event_matcher_name_action_and_version` - all three predicates
  pass.
- `test_generic_plan_event_new_valid_json` - valid JSON payload produces a
  `GenericPlanEvent`.
- `test_generic_plan_event_new_invalid_json_errors` - malformed JSON returns
  `Err`.
- `test_generic_plan_event_new_missing_required_fields_errors` - JSON missing
  `name` or `tasks` returns `Err`.
- `test_generic_event_handler_handle_no_matcher_returns_task` - no matcher
  configured; any valid plan returns `Ok(Some(...))`.
- `test_generic_event_handler_handle_matcher_passes` - matcher matches; returns
  `Ok(Some(...))`.
- `test_generic_event_handler_handle_matcher_filters` - matcher does not match;
  returns `Ok(None)`.
- `test_generic_event_handler_handle_invalid_plan_returns_err` - invalid plan
  fails `validate()` and returns `Err`.
- `test_plan_result_event_from_task_and_result_success` -
  `result.success = true` sets `status = "success"`.
- `test_plan_result_event_from_task_and_result_failure` -
  `result.success = false` sets `status = "failure"`.
- `test_plan_result_event_includes_correlation_key` - `correlation_key` from
  task is propagated.
- `test_fake_result_producer_records_published_events` - `publish` appends to
  internal list.

All tests use `FakeGenericConsumer` and `FakeResultProducer`. No real Kafka
connections. No `std::process::Command`. Per Rule 9.

#### 3.10 Deliverables

| File                                     | Change                                                     |
| ---------------------------------------- | ---------------------------------------------------------- |
| `src/watcher/generic/mod.rs`             | Replace placeholder; declare 6 sub-modules                 |
| `src/watcher/generic/event.rs`           | New: `GenericPlanEvent`                                    |
| `src/watcher/generic/matcher.rs`         | New: `GenericEventMatcher`                                 |
| `src/watcher/generic/consumer.rs`        | New: trait + `RealGenericConsumer` + `FakeGenericConsumer` |
| `src/watcher/generic/event_handler.rs`   | New: `GenericEventHandler` + `GenericTask`                 |
| `src/watcher/generic/result_event.rs`    | New: `PlanResultEvent`                                     |
| `src/watcher/generic/result_producer.rs` | New: trait + `ResultProducer` + `FakeResultProducer`       |
| `Cargo.toml`                             | `rdkafka` added as direct dependency via `cargo add`       |
| `tests/unit/watcher_generic.rs`          | New: 17 unit tests                                         |
| `docs/explanation/implementations.md`    | New entry following Rule 8 format                          |

#### 3.11 Success Criteria

- `cargo test` passes; `FakeGenericConsumer` drives all tests; no real broker
  required.
- `rg "polaris_core|polaris_client|polaris_auth|polaris_events" src/watcher/generic/`
  returns zero results.
- `rg "rdkafka" src/watcher/polaris/` returns zero results (Polaris path
  continues to use `polaris-events`).
- `GenericEventMatcher` rejects events when no predicates are configured.
- `cargo tree -d | grep rdkafka` shows a single version.

---

### Phase 4: Configuration and CLI Updates

**Depends on:** Phase 3 must be complete. `GenericWatcherConfig` in this phase
references `crate::watcher::generic::matcher::GenericEventMatcher`, which is
defined in Phase 3. Phases 3 and 4 are NOT independent.

Restructures `WatcherConfig` to support both watcher types with type-specific
sub-sections while preserving backwards compatibility with existing config
files.

#### 4.1 Add `WatcherType` Enum

In `src/config.rs`, add before the `WatcherConfig` struct:

```rust
/// Selects which watcher mode is active.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WatcherType {
    /// Polaris pipeline event watcher (default).
    #[default]
    Polaris,
    /// Standalone generic Kafka plan watcher.
    Generic,
}
```

Implement `std::fmt::Display` returning `"polaris"` or `"generic"`. Implement
`std::str::FromStr` (case-insensitive) returning
`AtomaError::Config("unknown watcher type: ...")` on unknown values, following
the existing `ProviderType::from_str` pattern exactly.

#### 4.2 Add `PolarisWatcherConfig`

Extract all Polaris-specific fields out of `WatcherConfig` into:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolarisWatcherConfig {
    #[serde(default)]
    pub janus: JanusConfig,
    #[serde(default = "default_watcher_topics")]
    pub topics: Vec<String>,
    #[serde(default = "default_watcher_plan_directory")]
    pub plan_directory: std::path::PathBuf,
    #[serde(default)]
    pub receipt: ReceiptConfig,
    #[serde(default)]
    pub publish_events: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<crate::watcher::polaris::matcher::EventMatcher>,
}

impl Default for PolarisWatcherConfig {
    fn default() -> Self {
        Self {
            janus: JanusConfig::default(),
            topics: default_watcher_topics(),
            plan_directory: default_watcher_plan_directory(),
            receipt: ReceiptConfig::default(),
            publish_events: false,
            matcher: None,
        }
    }
}
```

The existing helper functions `default_watcher_topics`,
`default_watcher_plan_directory`, and `default_watcher_max_concurrent_plans`
remain in `config.rs` (unchanged).

#### 4.3 Add `GenericWatcherConfig`

```rust
fn default_generic_input_topic() -> String { "atoma.plans".to_string() }
fn default_generic_output_topic() -> String { "atoma.results".to_string() }
fn default_generic_publish_results() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericWatcherConfig {
    /// Kafka topic to consume plan events from.
    #[serde(default = "default_generic_input_topic")]
    pub input_topic: String,
    /// Kafka topic to publish result events to. May equal `input_topic`.
    #[serde(default = "default_generic_output_topic")]
    pub output_topic: String,
    /// When set, the event payload plan is optionally overridden by a
    /// matching file on disk. See `GenericEventHandler` for resolution order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_directory: Option<std::path::PathBuf>,
    /// When set, only events satisfying all predicates are executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<crate::watcher::generic::matcher::GenericEventMatcher>,
    /// Publish a `PlanResultEvent` to `output_topic` after each execution.
    #[serde(default = "default_generic_publish_results")]
    pub publish_results: bool,
}

impl Default for GenericWatcherConfig {
    fn default() -> Self {
        Self {
            input_topic: default_generic_input_topic(),
            output_topic: default_generic_output_topic(),
            plan_directory: None,
            matcher: None,
            publish_results: default_generic_publish_results(),
        }
    }
}
```

#### 4.4 Restructure `WatcherConfig`

Replace the existing flat `WatcherConfig` struct with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub watcher_type: WatcherType,
    #[serde(default)]
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub allow_dangerous: bool,
    #[serde(default = "default_watcher_max_concurrent_plans")]
    pub max_concurrent_plans: usize,
    #[serde(default)]
    pub polaris: PolarisWatcherConfig,
    #[serde(default)]
    pub generic: GenericWatcherConfig,
}
```

The following fields are REMOVED from `WatcherConfig` and now live in
`PolarisWatcherConfig`: `janus`, `topics`, `plan_directory`, `receipt`,
`publish_events`, `matcher`.

**Backwards compatibility requirement:** Config files that use the OLD flat
layout (where `janus`, `topics`, `plan_directory`, `receipt`, `publish_events`,
and `matcher` appear directly under `watcher:`) must continue to deserialise
correctly. Implement this by adding a Serde `#[serde(alias)]` approach: keep the
old field names as deprecated aliases on `WatcherConfig` using a custom
`Deserialize` implementation that:

1. Deserialises into an intermediate `RawWatcherConfig` struct containing BOTH
   the old flat fields AND the new `polaris`/`generic` sub-sections.
2. If any old flat field is present AND `polaris` is absent, populate `polaris`
   from the flat fields and emit a `tracing::warn!` with message
   `"Config uses deprecated flat watcher layout; migrate to watcher.polaris.* fields"`.
3. Construct `WatcherConfig` from the resolved values.

The intermediate `RawWatcherConfig` is private to the `config` module.

The `Default` implementation for `WatcherConfig` must produce the same defaults
as before for all Polaris fields so existing tests pass unchanged.

#### 4.5 Update `validate_for_watcher`

Modify `Config::validate_for_watcher()` in `src/config.rs`.

Replace the existing single-path validation with a branch on `watcher_type`:

**Shared validation (both types):**

- `watcher.enabled` must be `true` (fail fast if not).
- `watcher.kafka.bootstrap_servers` must be non-empty.
- `watcher.kafka.group_id` must be non-empty.
- `watcher.max_concurrent_plans` must be `> 0`.

**When `watcher_type == WatcherType::Polaris`:**

- Apply all existing Polaris-specific validations verbatim: `janus.url`
  non-empty, `janus.url` parses as a valid URL, `janus.api_key_env_var`
  non-empty, `topics` non-empty, `receipt.maintainer` non-empty,
  `receipt.platform_id` non-empty, `receipt.package` non-empty, `plan_directory`
  non-empty and if present on disk it must be a directory.
- Warn when `polaris.matcher` has no predicates.
- All field accesses use `self.watcher.polaris.*` paths.

**When `watcher_type == WatcherType::Generic`:**

- `generic.input_topic` must be non-empty.
- `generic.output_topic` must be non-empty.
- If `generic.matcher` is `Some(m)` and `!m.has_predicates()`, emit a
  `tracing::warn!` stating that the matcher has no predicates and all events
  will be skipped.

**Environment variable overrides** to add in `Config::apply_env_vars`:

- `ATOMA_WATCHER_TYPE` -> `config.watcher.watcher_type` (parse via
  `WatcherType::from_str`).
- `ATOMA_WATCHER_GENERIC_INPUT_TOPIC` -> `config.watcher.generic.input_topic`.
- `ATOMA_WATCHER_GENERIC_OUTPUT_TOPIC` -> `config.watcher.generic.output_topic`.
- `ATOMA_WATCHER_GENERIC_PUBLISH_RESULTS` ->
  `config.watcher.generic.publish_results` (parse `"true"` / `"false"`
  case-insensitively).

Follow the existing `apply_env_vars` pattern exactly (check `std::env::var`, use
`tracing::debug!` on application, do nothing on `Err`).

#### 4.6 Update CLI - `Watch` Command

In `src/cli.rs`, extend the `Watch` variant of `Commands`:

```rust
Watch {
    /// When true, plans may execute commands without confirmation.
    #[arg(long, default_value_t = false)]
    allow_dangerous: bool,

    /// When true, process exactly one event then exit.
    #[arg(long, default_value_t = false)]
    once: bool,

    /// Watcher type override: "polaris" or "generic".
    /// Overrides the value in the config file.
    #[arg(long, value_name = "TYPE")]
    watcher_type: Option<String>,
}
```

In `src/main.rs`, update the `Commands::Watch` arm to extract `watcher_type` and
pass it to `handle_watch`:

```rust
Commands::Watch { allow_dangerous, once, watcher_type } =>
    handle_watch(allow_dangerous, once, watcher_type, config).await,
```

In `src/commands/watch.rs`, update `handle_watch` signature to accept
`watcher_type: Option<String>` and at the START of the function body (before
`validate_for_watcher`), apply the override:

```rust
if let Some(ref wt) = watcher_type {
    config.watcher.watcher_type = wt.parse().map_err(|e| {
        AtomaError::Config(format!("Invalid --watcher-type '{}': {}", wt, e))
    })?;
}
```

#### 4.7 Testing Requirements

All new tests for this phase are added inside the existing `mod tests` block in
`src/config.rs`. Do not create a new file.

Required test functions:

- `test_watcher_type_display_polaris` -
  `WatcherType::Polaris.to_string() == "polaris"`.
- `test_watcher_type_display_generic` -
  `WatcherType::Generic.to_string() == "generic"`.
- `test_watcher_type_from_str_polaris_case_insensitive` - `"POLARIS"`,
  `"Polaris"`, `"polaris"` all parse correctly.
- `test_watcher_type_from_str_generic_case_insensitive` - `"GENERIC"`,
  `"Generic"`, `"generic"` all parse correctly.
- `test_watcher_type_from_str_unknown_returns_error` - `"unknown"` returns
  `Err`.
- `test_validate_for_watcher_generic_empty_input_topic_errors` - empty
  `input_topic` fails.
- `test_validate_for_watcher_generic_empty_output_topic_errors` - empty
  `output_topic` fails.
- `test_validate_for_watcher_generic_zero_max_concurrent_plans_errors` -
  `max_concurrent_plans = 0` fails.
- `test_validate_for_watcher_generic_valid_config_passes` - minimal valid
  generic config passes.
- `test_watcher_config_backwards_compat_flat_layout` - YAML with top-level
  `watcher.janus`, `watcher.topics`, etc. deserialises into
  `watcher.polaris.janus`, `watcher.polaris.topics` correctly.
- `test_env_var_watcher_type` - setting `ATOMA_WATCHER_TYPE=generic` overrides
  config.
- `test_env_var_watcher_generic_input_topic` - env var overrides
  `generic.input_topic`.
- `test_env_var_watcher_generic_output_topic` - env var overrides
  `generic.output_topic`.
- `test_env_var_watcher_generic_publish_results` - env var overrides
  `generic.publish_results`.
- `test_generic_watcher_config_defaults` - default instance has expected field
  values.

In `src/cli.rs`, add:

- `test_watch_command_with_watcher_type_flag` - parses
  `watch --watcher-type generic` and asserts `watcher_type == Some("generic")`.
- `test_watch_command_without_watcher_type_flag` - parses `watch` and asserts
  `watcher_type == None`.

#### 4.8 Deliverables

| File                                  | Change                                                                                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/config.rs`                       | `WatcherType`, `PolarisWatcherConfig`, `GenericWatcherConfig`, restructured `WatcherConfig`, updated `validate_for_watcher`, updated `apply_env_vars`, 15 new tests |
| `src/cli.rs`                          | `--watcher-type` flag on `Watch`; 2 new CLI tests                                                                                                                   |
| `src/main.rs`                         | `Commands::Watch` arm extracts `watcher_type`                                                                                                                       |
| `src/commands/watch.rs`               | `handle_watch` signature updated; `watcher_type` override applied                                                                                                   |
| `docs/explanation/implementations.md` | New entry following Rule 8 format                                                                                                                                   |

#### 4.9 Success Criteria

- `cargo test` passes with no regressions in `config.rs` tests.
- Old flat config YAML parses without error and emits a `WARN` level log line
  containing `"deprecated flat watcher layout"`.
- `validate_for_watcher` rejects incomplete generic configs (empty topic names).
- `validate_for_watcher` for `WatcherType::Polaris` continues to reject all
  previously-rejected invalid configs.
- All Polaris field accesses now go through `watcher.polaris.*`; the following
  command returns zero results:

  ```bash
  rg "watcher\.janus\|watcher\.topics\|watcher\.plan_directory\|watcher\.receipt\|watcher\.publish_events" src/commands/watch.rs
  ```

---

### Phase 5: Watch Command Dispatch and Integration

**Depends on:** Phases 3 and 4 must be complete and the build must be green.

Wires the generic watcher loop into `commands/watch.rs` and ensures both modes
are exercised end-to-end in tests.

#### 5.1 Update `handle_watch` to Dispatch by Type

In `src/commands/watch.rs`, after applying the `watcher_type` override and
calling `config.validate_for_watcher()?`, branch on
`config.watcher.watcher_type`:

```rust
match config.watcher.watcher_type {
    WatcherType::Polaris => handle_polaris_watch(allow_dangerous, once, config).await,
    WatcherType::Generic => handle_generic_watch(allow_dangerous, once, config).await,
}
```

Rename the existing `handle_watch` body (everything after the validation and
dispatch) to
`handle_polaris_watch(allow_dangerous: bool, once: bool, config: Config) -> Result<()>`.
This function is private (not re-exported). It calls `run_watcher_loop`
unchanged.

The public function with signature
`handle_watch(allow_dangerous, once, watcher_type: Option<String>, config)`
becomes the thin dispatcher described above.

Update `src/commands/mod.rs`:

1. The `pub use watch::handle_watch;` export is unchanged (the function
   signature changes, so update call sites in tests within `commands/watch.rs`
   that call `handle_watch` directly).
2. Add the `watch` module doc comment to list both `handle_polaris_watch` and
   `handle_generic_watch` as internal entry points.

Update the module-level doc comment at the top of `src/commands/watch.rs` to
describe dual-mode dispatch. Replace the Polaris-only description with:

```text
Implements the `atoma watch` command for event-driven plan execution.
Supports two modes selected by `WatcherType`:
- `WatcherType::Polaris` - subscribes to Polaris Kafka events, resolves plans,
  posts receipts to the Janus Gatekeeper API.
- `WatcherType::Generic` - consumes raw JSON plan payloads from a Kafka topic,
  executes plans, publishes `PlanResultEvent` results.
```

#### 5.2 Implement `handle_generic_watch`

`async fn handle_generic_watch(allow_dangerous, once, config) -> Result<()>`:

1. The config has already been validated by `handle_watch`; no second call
   needed.
2. `let allow_dangerous = allow_dangerous || config.watcher.allow_dangerous;`
3. Build
   `RealGenericConsumer::new(&config.watcher.kafka, &config.watcher.generic.input_topic)?`.
4. Build
   `GenericEventHandler::new(config.watcher.generic.matcher.clone(), config.watcher.generic.plan_directory.clone())`.
5. Optionally build `ResultProducer` when
   `config.watcher.generic.publish_results`:

   ```rust
   let result_producer: Option<Box<dyn ResultProducerTrait>> = if config.watcher.generic.publish_results {
       Some(Box::new(ResultProducer::new(&config.watcher.kafka, config.watcher.generic.output_topic.clone())?))
   } else {
       None
   };
   ```

6. Print the startup banner (see 5.4).
7. Build `CircuitBreaker::new(CircuitBreakerConfig::default())`.
8. Construct `GenericWatcherLoopContext` and call `run_generic_watcher_loop`.

#### 5.3 Define `GenericWatcherLoopContext`

Add this struct to `src/commands/watch.rs`:

```rust
/// Services and settings consumed by [`run_generic_watcher_loop`].
pub struct GenericWatcherLoopContext {
    pub consumer: Box<dyn GenericConsumerTrait>,
    pub event_handler: GenericEventHandler,
    pub result_producer: Option<Box<dyn ResultProducerTrait>>,
    pub circuit_breaker: CircuitBreaker,
    pub allow_dangerous: bool,
    pub once: bool,
}
```

Implement `std::fmt::Debug` for `GenericWatcherLoopContext` manually (matching
the existing `WatcherLoopContext` debug impl pattern): include
`allow_dangerous`, `once`, `result_producer.is_some()`, and
`circuit_breaker.state()`.

#### 5.4 Implement `run_generic_watcher_loop`

Signature:

```rust
pub async fn run_generic_watcher_loop(
    ctx: GenericWatcherLoopContext,
    config: &Config,
) -> Result<()>
```

Print the startup banner before the loop:

```rust
println!();
println!("{}", "=== Atoma Generic Watcher Mode ===".bright_yellow().bold());
println!("  Input  topic : {}", config.watcher.generic.input_topic);
println!("  Output topic : {}", config.watcher.generic.output_topic);
println!("  Publish      : {}", ctx.result_producer.is_some());
println!("  Once mode    : {}", ctx.once);
println!();
```

Loop structure mirrors `run_watcher_loop` exactly for signal handling and
back-off but omits receipt building, Gatekeeper posting, and `ReceiptCreated`
publishing:

```text
loop {
    select! {
        _ = ctrl_c / SIGTERM => break,
        msg = consumer.next() => {
            match msg {
                None => break,
                Some(Err(e)) => {
                    error!("Consumer error: {}", e);
                    back_off.warn_if_backing_off("consumer");
                    tokio::time::sleep(back_off.next_delay()).await;
                    continue;
                }
                Some(Ok(raw_msg)) => {
                    back_off.reset();
                    match event_handler.handle(raw_msg) {
                        Err(e) => { error!("Event handler error: {}", e); consumer.commit().await?; }
                        Ok(None) => { debug!("Event filtered by matcher; skipping"); consumer.commit().await?; }
                        Ok(Some(task)) => {
                            let result = execute_plan_task(
                                task.instruction.clone(),
                                vec![],       // generic watcher does not support image tasks
                                allow_dangerous,
                                config,
                            ).await;
                            match result {
                                Ok(run_result) => {
                                    if let Some(ref producer) = result_producer {
                                        let event = PlanResultEvent::from_task_and_result(&task, &run_result);
                                        if let Err(e) = producer.publish(&event).await {
                                            error!("Failed to publish result event: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Plan execution failed: {}", e);
                                    if let Some(ref producer) = result_producer {
                                        let failure_result = RunResult::failure(
                                            format!("Execution error: {}", e),
                                            0,
                                        );
                                        let event = PlanResultEvent::from_task_and_result(&task, &failure_result);
                                        if let Err(publish_err) = producer.publish(&event).await {
                                            error!("Failed to publish result event for execution error: {}", publish_err);
                                        }
                                    }
                                }
                            }
                            consumer.commit().await?;
                        }
                    }
                    if ctx.once { break; }
                }
            }
        }
    }
}
// Flush result producer on shutdown
if let Some(ref producer) = result_producer {
    producer.flush(Duration::from_secs(5)).await.ok();
}
Ok(())
```

There is NO circuit breaker protecting `execute_plan_task` in the generic loop;
the circuit breaker field in `GenericWatcherLoopContext` is reserved for future
use and must be present in the struct but need not be consulted in this
implementation. Document this with a
`// TODO: apply circuit breaker to execution` comment.

#### 5.5 Update `run_watcher_loop` Banner (Polaris Path)

The existing `run_watcher_loop` in `commands/watch.rs` displays
`config.watcher.janus.url` in its startup banner. After the config restructure
in Phase 4, this field is now at `config.watcher.polaris.janus.url`. Update
every reference to `config.watcher.janus` in `run_watcher_loop` and
`handle_polaris_watch` to `config.watcher.polaris.janus`, and every reference to
`config.watcher.topics` to `config.watcher.polaris.topics`, and every reference
to `config.watcher.plan_directory` to `config.watcher.polaris.plan_directory`,
and `config.watcher.publish_events` to `config.watcher.polaris.publish_events`,
and `config.watcher.matcher` to `config.watcher.polaris.matcher`, and
`config.watcher.receipt` to `config.watcher.polaris.receipt`.

Perform a global search before finishing:

```bash
rg "watcher\.janus\|watcher\.topics\|watcher\.plan_directory\|watcher\.receipt\|watcher\.publish_events\|watcher\.matcher" src/commands/watch.rs
```

This must return zero results.

#### 5.6 Testing Requirements

Add the following tests inside the existing `mod tests` block at the bottom of
`src/commands/watch.rs`:

- `test_handle_generic_watch_disabled_config_errors` - `watcher.enabled = false`
  returns `Err` before any Kafka call.
- `test_handle_generic_watch_missing_input_topic_errors` - empty `input_topic`
  fails `validate_for_watcher` before any Kafka call.
- `test_run_generic_watcher_loop_once_mode_single_event` - inject one valid JSON
  plan event via `FakeGenericConsumer` with `once = true`; assert loop exits
  after one event and `consumer.commits_recorded() == 1`.
- `test_run_generic_watcher_loop_filters_non_matching_event` - configure a
  `GenericEventMatcher` that does not match the injected plan; assert loop
  commits and exits (`once = true`) without calling `execute_plan_task` (use a
  flag or mock to detect execution).
- `test_run_generic_watcher_loop_publishes_result_event` - inject one valid plan
  via `FakeGenericConsumer` with `publish_results = true` and a
  `FakeResultProducer`; assert `fake_producer.published_events().len() == 1`
  after the loop exits.
- `test_run_generic_watcher_loop_no_publish_when_disabled` - same setup but
  `result_producer = None`; assert no panic and
  `consumer.commits_recorded() == 1`.
- `test_run_generic_watcher_loop_consumer_error_continues` -
  `FakeGenericConsumer` returns `Err` then `None`; assert loop exits cleanly
  without returning `Err`.

All tests use `FakeGenericConsumer` and `FakeResultProducer`. No real Kafka
connections. No OS process spawning. Per Rule 9.

To drive `execute_plan_task` without a real provider in tests that need to
verify execution occurred, use the `test-utils` feature flag and `MockProvider`
from `src/test_utils.rs`, or check `ATOMA_TEST_MODE` as done in existing watcher
tests.

#### 5.7 Deliverables

| File                                  | Change                                                                                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/commands/watch.rs`               | `handle_polaris_watch` (renamed body), `handle_generic_watch`, `run_generic_watcher_loop`, `GenericWatcherLoopContext`, banner update for Polaris path, 7 new tests |
| `src/commands/mod.rs`                 | Module-level doc comment updated to describe both entry points                                                                                                      |
| `src/main.rs`                         | `Commands::Watch` arm passes `watcher_type` to `handle_watch`                                                                                                       |
| `docs/explanation/implementations.md` | New entry following Rule 8 format                                                                                                                                   |

#### 5.8 Success Criteria

All five quality gates must pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test
```

Additional checks:

- The following command returns zero results:

  ```bash
  rg "polaris_core|polaris_client|polaris_auth|polaris_events" \
    src/watcher/generic/ src/commands/watch.rs \
    | grep -v "handle_polaris_watch\|WatcherType::Polaris"
  ```

- The following command returns zero results:

  ```bash
  rg "watcher\.janus\|watcher\.topics\|watcher\.plan_directory\|watcher\.receipt\|watcher\.publish_events\b" src/commands/watch.rs
  ```

- `atoma watch --watcher-type generic --once` runs end-to-end in a test using
  `FakeGenericConsumer` without contacting a real broker.
- `atoma watch --watcher-type polaris --once` continues to pass all existing
  watcher tests without modification.

---

### Phase 6: Topic Auto-Creation and Configurable Consumer Group

**Depends on:** Phase 5 must be complete and the build must be green.

Adds two independent but co-delivered capabilities:

1. **Automatic Kafka topic creation** - when the watcher starts up it ensures
   all required topics exist on the broker, creating missing ones rather than
   blocking or failing at subscription time.
2. **Configurable consumer group** - the Kafka consumer group ID becomes
   overrideable at the CLI (`--group-id`) in addition to the existing config
   file and `ATOMA_WATCHER_KAFKA_GROUP_ID` environment variable paths.

---

#### 6.1 Add Topic-Creation Fields to `KafkaConfig`

**File:** `src/config.rs`

Add three new fields to `KafkaConfig`. They are placed in `KafkaConfig` rather
than in `GenericWatcherConfig` or `PolarisWatcherConfig` so that both watcher
types share identical auto-creation behaviour and a single configuration section.

```rust
pub struct KafkaConfig {
    // ... existing fields unchanged ...

    /// Automatically create topics that do not exist when the watcher starts.
    ///
    /// When `true` (the default) the watcher calls the Kafka Admin API before
    /// subscribing the consumer.  Topics that already exist are silently
    /// accepted.  Set to `false` to disable creation and let the consumer fail
    /// naturally if a topic is missing.
    #[serde(default = "default_kafka_auto_create_topics")]
    pub auto_create_topics: bool,

    /// Number of partitions to use when auto-creating a topic.
    #[serde(default = "default_kafka_topic_num_partitions")]
    pub topic_num_partitions: i32,

    /// Replication factor to use when auto-creating a topic.
    ///
    /// Must not exceed the number of available brokers.  Use `1` for local
    /// development; use `3` for production clusters.
    #[serde(default = "default_kafka_topic_replication_factor")]
    pub topic_replication_factor: i32,
}
```

Add the three corresponding default-value functions immediately after the
existing `default_kafka_*` helpers:

```rust
fn default_kafka_auto_create_topics() -> bool {
    true
}

fn default_kafka_topic_num_partitions() -> i32 {
    1
}

fn default_kafka_topic_replication_factor() -> i32 {
    1
}
```

Update `impl Default for KafkaConfig` to include the three new fields:

```rust
impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            // ... existing fields ...
            auto_create_topics: default_kafka_auto_create_topics(),
            topic_num_partitions: default_kafka_topic_num_partitions(),
            topic_replication_factor: default_kafka_topic_replication_factor(),
        }
    }
}
```

---

#### 6.2 Create `src/watcher/kafka_admin.rs`

**File:** `src/watcher/kafka_admin.rs` (new)

Implements topic creation behind a trait so that production code uses the real
`rdkafka` `AdminClient` and tests use `FakeTopicAdmin` without contacting a
broker. Follow the same trait/real/fake pattern already established by
`GenericConsumerTrait` / `RealGenericConsumer` / `FakeGenericConsumer`.

##### `TopicAdminTrait`

```rust
/// Abstracts Kafka topic administration so that tests can use a fake
/// implementation without a real broker.
#[async_trait::async_trait]
pub trait TopicAdminTrait: Send + Sync {
    /// Ensure every named topic exists on the broker.
    ///
    /// Topics that already exist are silently accepted.  Any other error is
    /// propagated as [`AtomaError::Kafka`].
    async fn ensure_topics(
        &self,
        topics: &[&str],
        num_partitions: i32,
        replication_factor: i32,
    ) -> crate::error::Result<()>;
}
```

##### `KafkaTopicAdmin`

```rust
/// Production implementation that wraps `rdkafka::admin::AdminClient`.
pub struct KafkaTopicAdmin {
    admin: rdkafka::admin::AdminClient<rdkafka::client::DefaultClientContext>,
}

impl KafkaTopicAdmin {
    /// Build an admin client from the shared [`KafkaConfig`].
    ///
    /// Applies the same bootstrap servers, security protocol, and optional
    /// SASL / SSL settings as [`RealGenericConsumer::new`].
    pub fn new(kafka: &crate::config::KafkaConfig) -> crate::error::Result<Self> {
        use rdkafka::config::ClientConfig;

        let mut cfg = ClientConfig::new();
        cfg.set("bootstrap.servers", &kafka.bootstrap_servers);
        cfg.set("security.protocol", &kafka.security_protocol);

        if let Some(m) = &kafka.sasl_mechanism {
            cfg.set("sasl.mechanism", m);
        }
        if let Some(u) = &kafka.sasl_username {
            cfg.set("sasl.username", u);
        }
        if let Some(p) = &kafka.sasl_password {
            cfg.set("sasl.password", p);
        }
        if let Some(ca) = &kafka.ssl_ca_location {
            cfg.set("ssl.ca.location", ca);
        }

        let admin = cfg.create().map_err(|e| {
            crate::error::AtomaError::Kafka(format!(
                "Failed to create Kafka AdminClient: {}",
                e
            ))
        })?;

        Ok(Self { admin })
    }
}

#[async_trait::async_trait]
impl TopicAdminTrait for KafkaTopicAdmin {
    async fn ensure_topics(
        &self,
        topics: &[&str],
        num_partitions: i32,
        replication_factor: i32,
    ) -> crate::error::Result<()> {
        use rdkafka::admin::{AdminOptions, NewTopic, TopicReplication};
        use rdkafka::error::RDKafkaErrorCode;

        let new_topics: Vec<NewTopic<'_>> = topics
            .iter()
            .map(|t| NewTopic::new(t, num_partitions, TopicReplication::Fixed(replication_factor)))
            .collect();

        let results = self
            .admin
            .create_topics(&new_topics, &AdminOptions::new())
            .await
            .map_err(|e| {
                crate::error::AtomaError::Kafka(format!(
                    "Admin API error during topic creation: {}",
                    e
                ))
            })?;

        for result in results {
            match result {
                Ok(name) => {
                    tracing::info!(topic = %name, "Kafka topic created");
                }
                Err((name, RDKafkaErrorCode::TopicAlreadyExists)) => {
                    tracing::debug!(topic = %name, "Kafka topic already exists, skipping");
                }
                Err((name, code)) => {
                    return Err(crate::error::AtomaError::Kafka(format!(
                        "Failed to create Kafka topic '{}': {:?}",
                        name, code
                    )));
                }
            }
        }

        Ok(())
    }
}
```

##### `FakeTopicAdmin`

````rust
/// Test double for [`TopicAdminTrait`].
///
/// Holds a set of topics that are pre-declared as "already existing" and
/// records every topic creation call.  No network I/O is performed.
///
/// # Examples
///
/// ```rust
/// use atoma::watcher::kafka_admin::{FakeTopicAdmin, TopicAdminTrait};
///
/// # tokio_test::block_on(async {
/// let fake = FakeTopicAdmin::new(&["existing-topic"]);
/// fake.ensure_topics(&["existing-topic", "new-topic"], 1, 1)
///     .await
///     .unwrap();
/// assert_eq!(fake.created_topics(), vec!["new-topic"]);
/// # });
/// ```
pub struct FakeTopicAdmin {
    existing: std::collections::HashSet<String>,
    created: std::sync::Mutex<Vec<String>>,
}

impl FakeTopicAdmin {
    /// Create a fake admin that treats `existing` as pre-existing topics.
    pub fn new(existing: &[&str]) -> Self {
        Self {
            existing: existing.iter().map(|s| s.to_string()).collect(),
            created: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Return the names of topics that were created during the test.
    pub fn created_topics(&self) -> Vec<String> {
        self.created.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl TopicAdminTrait for FakeTopicAdmin {
    async fn ensure_topics(
        &self,
        topics: &[&str],
        _num_partitions: i32,
        _replication_factor: i32,
    ) -> crate::error::Result<()> {
        for &topic in topics {
            if !self.existing.contains(topic) {
                self.created.lock().unwrap().push(topic.to_string());
            }
        }
        Ok(())
    }
}
````

##### Module-level helper

Expose a convenience free function used by the watcher handlers. It reads the
`auto_create_topics` flag so call sites do not need to check it themselves:

```rust
/// Ensure all named topics exist, creating any that are absent.
///
/// When [`KafkaConfig::auto_create_topics`] is `false` this is a no-op.
///
/// # Errors
///
/// Returns [`AtomaError::Kafka`] if the admin client cannot be created or if
/// a topic creation fails for a reason other than the topic already existing.
pub async fn ensure_topics_exist(
    kafka: &crate::config::KafkaConfig,
    topics: &[&str],
) -> crate::error::Result<()> {
    if !kafka.auto_create_topics {
        tracing::debug!("auto_create_topics=false; skipping topic creation");
        return Ok(());
    }

    let admin = KafkaTopicAdmin::new(kafka)?;
    admin
        .ensure_topics(topics, kafka.topic_num_partitions, kafka.topic_replication_factor)
        .await
}
```

---

#### 6.3 Export `kafka_admin` from `src/watcher/mod.rs`

**File:** `src/watcher/mod.rs`

Add the following declaration and re-export alongside the existing module
declarations:

```rust
pub mod kafka_admin;

pub use kafka_admin::{FakeTopicAdmin, KafkaTopicAdmin, TopicAdminTrait, ensure_topics_exist};
```

---

#### 6.4 Wire `ensure_topics_exist` into Both Watcher Handlers

**File:** `src/commands/watch.rs`

Add the import at the top of the file alongside the existing watcher imports:

```rust
use crate::watcher::ensure_topics_exist;
```

**In `handle_generic_watch`**, immediately after the `allow_dangerous` override
line and before the `RealGenericConsumer::new` call:

```rust
// Ensure required topics exist before subscribing the consumer.
if watcher_cfg.kafka.auto_create_topics {
    let mut topics: Vec<&str> = vec![watcher_cfg.generic.input_topic.as_str()];
    if watcher_cfg.generic.publish_results {
        topics.push(watcher_cfg.generic.output_topic.as_str());
    }
    ensure_topics_exist(&watcher_cfg.kafka, &topics).await?;
}
```

**In `handle_polaris_watch`**, immediately after the `allow_dangerous` override
line and before the `build_event_config` call:

```rust
// Ensure required topics exist before subscribing the consumer.
if watcher_cfg.kafka.auto_create_topics {
    let topics: Vec<&str> = watcher_cfg.polaris.topics.iter().map(String::as_str).collect();
    ensure_topics_exist(&watcher_cfg.kafka, &topics).await?;
}
```

Both call sites rely on the `auto_create_topics` guard inside
`ensure_topics_exist` to no-op when the feature is disabled, but check the flag
explicitly here so the `topics` allocation is skipped entirely when not needed.

---

#### 6.5 Add `--group-id` CLI Flag

**File:** `src/cli.rs`

In the `Commands::Watch` variant, add a new argument after `watcher_type`:

```rust
/// Consumer group ID for the Kafka consumer.
///
/// Overrides the `watcher.kafka.group_id` value in the config file.
/// Useful when running multiple independent watcher instances that must
/// each receive every event (e.g. a staging instance alongside production).
///
/// The `ATOMA_WATCHER_KAFKA_GROUP_ID` environment variable provides the
/// same override; the CLI flag takes precedence over the env var.
#[arg(long, value_name = "GROUP_ID")]
group_id: Option<String>,
```

The precedence chain is (highest to lowest):

1. `--group-id` CLI flag
2. `ATOMA_WATCHER_KAFKA_GROUP_ID` environment variable
3. `watcher.kafka.group_id` in the config file
4. compiled default (`"atoma-watcher"`)

---

#### 6.6 Apply `--group-id` Override in `handle_watch`

**File:** `src/commands/watch.rs`

Update the `handle_watch` signature to accept the new argument:

```rust
pub async fn handle_watch(
    allow_dangerous: bool,
    once: bool,
    watcher_type: Option<String>,
    group_id: Option<String>,       // NEW
    mut config: Config,
) -> Result<()> {
```

At the top of the function body, apply the CLI override **before** the
`validate_for_watcher` call so that validation operates on the final resolved
value:

```rust
// Apply CLI overrides before validation so the resolved values are checked.
if let Some(ref wt) = watcher_type {
    config.watcher.watcher_type = wt
        .parse()
        .map_err(|e| AtomaError::Config(format!("Invalid --watcher-type '{}': {}", wt, e)))?;
}

if let Some(gid) = group_id {
    config.watcher.kafka.group_id = gid;
}
```

**File:** `src/main.rs`

Locate the `Commands::Watch { allow_dangerous, once, watcher_type }` match arm
and add `group_id` to the destructure and the `handle_watch` call:

```rust
Commands::Watch { allow_dangerous, once, watcher_type, group_id } => {
    commands::watch::handle_watch(allow_dangerous, once, watcher_type, group_id, config).await?;
}
```

---

#### 6.7 Env Vars and Validation

**File:** `src/config.rs`, `Config::apply_env_vars`

Add three entries in the watcher section, immediately after the existing
`ATOMA_WATCHER_KAFKA_SSL_CA_LOCATION` block:

```rust
if let Ok(val) = std::env::var("ATOMA_WATCHER_KAFKA_AUTO_CREATE_TOPICS") {
    self.watcher.kafka.auto_create_topics =
        matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
}

if let Ok(val) = std::env::var("ATOMA_WATCHER_KAFKA_TOPIC_NUM_PARTITIONS")
    && let Ok(parsed) = val.parse::<i32>()
{
    self.watcher.kafka.topic_num_partitions = parsed;
}

if let Ok(val) = std::env::var("ATOMA_WATCHER_KAFKA_TOPIC_REPLICATION_FACTOR")
    && let Ok(parsed) = val.parse::<i32>()
{
    self.watcher.kafka.topic_replication_factor = parsed;
}
```

**File:** `src/config.rs`, `Config::validate_for_watcher`

Add validation for the new `KafkaConfig` fields. These checks apply regardless
of watcher type and should be placed in the shared section before the
type-specific dispatch:

```rust
if config.watcher.kafka.topic_num_partitions < 1 {
    return Err(AtomaError::Config(
        "watcher.kafka.topic_num_partitions must be >= 1".to_string(),
    ));
}

if config.watcher.kafka.topic_replication_factor < 1 {
    return Err(AtomaError::Config(
        "watcher.kafka.topic_replication_factor must be >= 1".to_string(),
    ));
}
```

---

#### 6.8 Testing Requirements

**Rule 9 applies in full:** no test may make a real Kafka connection. All tests
for `kafka_admin.rs` must use `FakeTopicAdmin`.

##### Tests for `src/watcher/kafka_admin.rs`

Add a `mod tests` block at the bottom of the new file:

| Test name                                | What it verifies                                                              |
| ---------------------------------------- | ----------------------------------------------------------------------------- |
| `test_fake_admin_creates_new_topic`      | `FakeTopicAdmin::ensure_topics` records a topic that was not in `existing`.   |
| `test_fake_admin_skips_existing_topic`   | A topic listed in `existing` does not appear in `created_topics()`.           |
| `test_fake_admin_mixed_existing_and_new` | Of three topics, only the two absent from `existing` are recorded as created. |
| `test_fake_admin_empty_topic_list`       | Calling `ensure_topics` with an empty slice succeeds and records nothing.     |

All four tests must be sync (`#[test]` with `tokio_test::block_on` or
`#[tokio::test]`) and must not spawn any OS processes or open network sockets.

##### Tests for `src/config.rs`

Add to the existing `mod tests` block:

| Test name                                     | What it verifies                                                                                                |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `test_kafka_config_auto_create_defaults`      | `KafkaConfig::default()` has `auto_create_topics=true`, `topic_num_partitions=1`, `topic_replication_factor=1`. |
| `test_env_var_auto_create_topics`             | Setting `ATOMA_WATCHER_KAFKA_AUTO_CREATE_TOPICS=false` disables the flag.                                       |
| `test_env_var_topic_num_partitions`           | `ATOMA_WATCHER_KAFKA_TOPIC_NUM_PARTITIONS=4` sets `topic_num_partitions=4`.                                     |
| `test_env_var_topic_replication_factor`       | `ATOMA_WATCHER_KAFKA_TOPIC_REPLICATION_FACTOR=3` sets `topic_replication_factor=3`.                             |
| `test_validate_topic_num_partitions_zero`     | `topic_num_partitions=0` is rejected by `validate_for_watcher`.                                                 |
| `test_validate_topic_replication_factor_zero` | `topic_replication_factor=0` is rejected by `validate_for_watcher`.                                             |

##### Tests for `src/cli.rs`

| Test name                                  | What it verifies                                                                  |
| ------------------------------------------ | --------------------------------------------------------------------------------- |
| `test_watch_command_with_group_id_flag`    | Parsing `atoma watch --group-id my-group` produces `group_id = Some("my-group")`. |
| `test_watch_command_without_group_id_flag` | Parsing `atoma watch` produces `group_id = None`.                                 |

##### Tests for `src/commands/watch.rs`

| Test name                                                       | What it verifies                                                                                                                                                                                  |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test_handle_watch_group_id_override_applied_before_validation` | A `group_id` of `Some("ci-group")` overrides `config.watcher.kafka.group_id` before `validate_for_watcher` is called; use a disabled-watcher config that returns early so no broker is contacted. |

**Verification — before every commit, confirm:**

```bash
rg "AdminClient::new\|rdkafka::admin" tests/ --type rust
```

If the command returns results, move the offending code behind `FakeTopicAdmin`.

---

#### 6.9 Deliverables

| File                                           | Change                                                                                                 |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `src/watcher/kafka_admin.rs` (new)             | `TopicAdminTrait`, `KafkaTopicAdmin`, `FakeTopicAdmin`, `ensure_topics_exist`                          |
| `src/watcher/mod.rs`                           | `pub mod kafka_admin;` declaration and re-exports                                                      |
| `src/config.rs`                                | Three new `KafkaConfig` fields, three default functions, three env var handlers, two validation checks |
| `src/cli.rs`                                   | `--group-id` flag on `Commands::Watch`                                                                 |
| `src/commands/watch.rs`                        | Updated `handle_watch` signature; `group_id` override; `ensure_topics_exist` calls in both handlers    |
| `src/main.rs`                                  | `group_id` threaded through to `handle_watch`                                                          |
| `docs/explanation/implementations.md`          | New entry following Rule 8 format                                                                      |
| `docs/how-to/use_generic_watcher.md`           | Document `--group-id`, `auto_create_topics`, partition and replication fields                          |
| `docs/reference/generic_watcher_data_types.md` | Update `KafkaConfig` field table with three new fields                                                 |

---

#### 6.10 Success Criteria

All five quality gates must pass:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test
```

Additional checks:

- The following command returns zero results, confirming no test contacts a real
  broker:

  ```bash
  rg "AdminClient::new\|rdkafka::admin" tests/ --type rust
  ```

- `atoma watch --group-id ci-group` can be parsed without error:

  ```bash
  atoma watch --help | grep group-id
  ```

- With `auto_create_topics = true` and a broker available, starting the generic
  watcher against a fresh namespace creates both `atoma.plans` and
  `atoma.results` before the consumer subscribes.
- With `auto_create_topics = false`, topic creation is skipped and the consumer
  subscribes directly (existing behaviour preserved).
- `ATOMA_WATCHER_KAFKA_GROUP_ID` continues to override `group_id` via the
  env-var path; `--group-id` on the CLI takes precedence over it.

---

## Demo Files

The demo directories already exist and contain functional scripts and config
files. No new demo files need to be created. The following updates are required
during implementation:

### `demo/watcher/polaris/` - Polaris Demo Updates (Phase 5)

After the config restructure in Phase 4, the demo config files at
`demo/watcher/polaris/config.yaml` and
`demo/watcher/polaris/config_ollama_granite.yaml` must be inspected. If they use
the old flat `watcher.janus`, `watcher.topics`, etc. keys, they may continue to
work via the backwards-compat layer, but should be updated to use the new
`watcher.polaris.*` sub-section layout to avoid the deprecation warning. The
`README.md` and `seed_event.sh` require no path changes since the Polaris source
files have moved within `src/` only.

### `demo/watcher/generic/` - Generic Watcher Demo Verification (Phase 5)

After Phase 5 implementation is complete, run the generic demo end-to-end to
verify:

1. `cd demo/watcher/generic && atoma --config config_ollama_granite.yaml watch`
   starts without error.
2. `./seed_plan.sh hello` publishes a plan event.
3. Atoma executes the plan and publishes a `PlanResultEvent` to `atoma.results`.
4. `./read_results.sh` displays the result.

Verify that `demo/watcher/generic/config.yaml` matches the final
`GenericWatcherConfig` schema. The current demo configs already use
`watcher_type: generic` and a `watcher.generic.*` sub-section; if any field
names or defaults changed during implementation, update the demo config files to
match.

---

## Implementation Order Summary

| Phase | Depends On   | Primary Files                                                                                             | Key Deliverable                                                                 |
| ----- | ------------ | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| 1     | None         | `src/commands/plan_parser.rs`                                                                             | Required `Task.id`, validated `Task.dependencies`, `Plan.action`, and `version` |
| 2     | Phase 1      | `src/watcher/polaris/` (new), `src/watcher/mod.rs`                                                        | All Polaris code migrated; `version_matches` shared                             |
| 3     | Phase 2      | `src/watcher/generic/` (new), `Cargo.toml`                                                                | Generic consumer, matcher, handler, producer                                    |
| 4     | Phase 3      | `src/config.rs`, `src/cli.rs`, `src/main.rs`                                                              | `WatcherType`, restructured `WatcherConfig`                                     |
| 5     | Phases 3 + 4 | `src/commands/watch.rs`, `src/main.rs`                                                                    | Dual-mode dispatch and integration                                              |
| 6     | Phase 5      | `src/watcher/kafka_admin.rs` (new), `src/config.rs`, `src/cli.rs`, `src/commands/watch.rs`, `src/main.rs` | Topic auto-creation at startup; `--group-id` CLI flag                           |

Each phase must leave the build green before the next begins. Phases may not be
parallelised; each has a strict dependency on the previous. After each phase,
run all five quality gates before proceeding.

### Documentation Update Checklist

After each phase that changes a user-visible interface, update the following
documents before marking the phase complete:

| Changed interface                                      | Update required                                                                                                                |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Required `Plan` and `Task` fields (Phase 1)            | `docs/reference/generic_watcher_data_types.md` - Plan / Task field tables and JSON Schema                                      |
| `GenericWatcherConfig` fields (Phase 4)                | Both documents - configuration reference sections                                                                              |
| `KafkaConfig` fields (any phase)                       | Both documents - Kafka configuration sections                                                                                  |
| `KafkaConfig` auto-create fields (Phase 6)             | Both documents - add `auto_create_topics`, `topic_num_partitions`, `topic_replication_factor` to the `KafkaConfig` field table |
| `PlanResultEvent` fields (Phase 3+)                    | `docs/reference/generic_watcher_data_types.md` - PlanResultEvent table and JSON Schema                                         |
| `GenericEventMatcher` fields (Phase 3+)                | Both documents - event matching sections                                                                                       |
| CLI flags (Phase 4)                                    | `docs/how-to/use_generic_watcher.md` - Quick Start and Configuration Reference                                                 |
| `--group-id` CLI flag (Phase 6)                        | `docs/how-to/use_generic_watcher.md` - add to CLI reference table and Quick Start example                                      |
| Environment variable overrides (Phase 4)               | `docs/how-to/use_generic_watcher.md` - Environment variable overrides section                                                  |
| `ATOMA_WATCHER_KAFKA_AUTO_CREATE_TOPICS` env (Phase 6) | `docs/how-to/use_generic_watcher.md` - add to environment variable overrides table                                             |

The documents at `docs/how-to/use_generic_watcher.md` and
`docs/reference/generic_watcher_data_types.md` reflect the completed Phase 3
data types. Sections covering `watcher_type: generic`, `watcher.generic.*`
configuration keys, and the `--once` flag require Phases 4 and 5 to be complete
before those code paths are fully wired. The configuration examples in those
documents already show the target schema; they are correct for the final
implementation and should not be changed unless the Phase 4 schema deviates from
the plan.
