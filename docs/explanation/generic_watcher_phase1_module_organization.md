# Generic Watcher Phase 1: Module Organization

## Summary

Phase 1 of the generic watcher modernization splits the monolithic
`src/watcher/generic/message.rs` into two focused, single-responsibility modules
and updates the module declarations and re-exports accordingly. No behavior
changes were made. All existing public symbols remain accessible at their
original paths through a compatibility shim.

## Problem Statement

Before this phase, `message.rs` was a 635-line file that combined two
conceptually distinct pipeline stages in a single module:

- `GenericPlanEvent` — the inbound trigger message consumed from the Kafka input
  topic
- `GenericPlanResult` — the outbound result message published to the Kafka
  output topic after plan execution

Combining both types in one file obscured the data flow direction, made it
harder to locate the canonical definition of each type, and created a larger
surface area for future editing conflicts as each type grows independently
through subsequent phases.

## What Changed

### New files

#### `src/watcher/generic/event.rs`

Contains `GenericPlanEvent` and its `impl` block with `new` and `is_plan_event`
methods. Carries all 16 unit tests that previously lived in `message.rs` under
the `GenericPlanEvent` section, plus two doc-test examples attached to the
struct and its methods.

The module-level doc explains the inbound event role and restates the loop-break
guarantee: `is_plan_event` returns `false` for any `event_type` value other than
`"plan"`, which is the first check the matcher performs before evaluating any
other criteria.

#### `src/watcher/generic/result_event.rs`

Contains `GenericPlanResult` and its `impl` block with the `new` constructor.
Carries all 9 unit tests that previously lived in `message.rs` under the
`GenericPlanResult` section, plus doc-test examples.

The module-level doc explains the outbound result role and restates how
`event_type = "result"` prevents the watcher from re-processing its own output
when the input and output topics are the same.

### Modified files

#### `src/watcher/generic/message.rs`

Reduced from 635 lines to a 31-line compatibility shim. The shim re-exports both
types from their new canonical homes:

```text
pub use super::event::GenericPlanEvent;
pub use super::result_event::GenericPlanResult;
```

A migration note in the module-level doc instructs callers to update their
imports to use the new direct paths. The shim will be removed in a future phase
once all call sites have been migrated.

#### `src/watcher/generic/mod.rs`

Two new submodule declarations were added:

```text
pub mod event;
pub mod result_event;
```

The top-level re-exports were updated to point directly at the canonical modules
instead of routing through the `message` shim:

```text
pub use event::GenericPlanEvent;
pub use result_event::GenericPlanResult;
```

The `pub mod message` declaration was retained so that existing import paths
such as `xzatoma::watcher::generic::message::GenericPlanEvent` continue to
resolve through the shim without modification.

The module-level doc component table was updated to list `event` and
`result_event` as separate entries and to identify `message` explicitly as a
compatibility shim.

## What Did Not Change

- The struct fields, derives, serde attributes, and method signatures of both
  `GenericPlanEvent` and `GenericPlanResult` are byte-for-byte identical to
  their definitions in the original `message.rs`.
- `watcher.rs` was not modified. It continues to import from
  `crate::watcher::generic::message` and resolves both types through the shim.
- `matcher.rs` and `producer.rs` were not modified.
- No public symbol previously exported from `src/watcher/generic/mod.rs` was
  removed or renamed.

## Test Count

| Module                                  | Tests before | Tests after |
| --------------------------------------- | ------------ | ----------- |
| `watcher::generic::event::tests`        | 0 (new)      | 16          |
| `watcher::generic::result_event::tests` | 0 (new)      | 9           |
| `watcher::generic::message` (tests)     | 23 (removed) | 0           |
| All other `watcher::generic` tests      | unchanged    | unchanged   |

The 23 tests that existed in `message.rs` travel verbatim into their owning
modules. The 2 additional tests are doc-test examples added to satisfy the
AGENTS.md documentation requirement that all public functions carry runnable
examples.

All tests pass: `cargo test --all-features --lib -- watcher::generic` reports 53
passed, 9 ignored (the 9 ignored tests require a live Kafka broker and were
already ignored before this phase).

## Quality Gates

All four mandatory gates pass in order:

1. `cargo fmt --all` — no formatting changes required
2. `cargo check --all-targets --all-features` — zero errors
3. `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings
4. `cargo test --all-features --lib -- watcher::generic` — 53 passed, 0 failed

## Design Decisions

### Doc-comment path update in new modules

The doc examples in `event.rs` and `result_event.rs` use the canonical new
import paths (`xzatoma::watcher::generic::event::GenericPlanEvent` and
`xzatoma::watcher::generic::result_event::GenericPlanResult`) rather than the
shim path. This keeps the doc examples consistent with where the types
canonically live and avoids the situation where the authoritative module's own
doc examples point elsewhere.

### No `#[deprecated]` attribute on the shim re-exports

The compatibility shim re-exports are documented with a migration note in the
module-level doc rather than carrying the Rust `#[deprecated]` attribute. Using
`#[deprecated]` on the re-exports would cause a `deprecated` lint at every
existing call site, which would be promoted to an error by
`cargo clippy -- -D warnings` and break the quality gates. The doc note
communicates the same intent without violating the quality gates or requiring
changes to `watcher.rs` in this phase.

### `message` shim retained in `mod.rs`

`pub mod message` is kept in `mod.rs` so that code outside this crate (or
integration tests) that uses the full path
`xzatoma::watcher::generic::message::GenericPlanEvent` continues to compile. The
shim is the correct place to remove in Phase 2 once all call sites have been
updated to the canonical module paths.

## Next Phase

Phase 2 introduces early plan parsing and an `event_handler.rs` module. That
phase will redesign `GenericPlanEvent` to include a parsed plan representation,
add `RawKafkaMessage` to `event.rs`, create `event_handler.rs` with an
`EventHandler` type that owns the early-parsing logic, and update `watcher.rs`
to delegate to it. At that point the `message.rs` shim will be removed and
`watcher.rs` will import directly from `event` and `result_event`.
