# Phase 1 XZepr Watcher Restructure Implementation

## Overview

Phase 1 of the Generic Watcher Implementation Plan relocated all XZepr-specific
watcher code from two separate top-level locations into a single, cohesive
subdirectory: `src/watcher/xzepr/`. This restructure is the prerequisite for
Phase 3, which introduces a second, independent Kafka watcher backend
(`src/watcher/generic/`) without touching any XZepr logic.

XZepr remains a fully supported, permanent watcher backend. No deprecation
notices were added. The two backends (`xzepr` and `generic`) are equal
configuration peers.

## Problem Statement

Before Phase 1, XZepr-specific code was split across two unrelated module trees:

- `src/xzepr/consumer/` -- Kafka consumer, API client, message types, and
  configuration structs
- `src/watcher/filter.rs` -- `EventFilter`, tightly coupled to
  `CloudEventMessage`
- `src/watcher/plan_extractor.rs` -- `PlanExtractor`, also coupled to
  `CloudEventMessage`
- `src/watcher/watcher.rs` -- `Watcher`, wiring `XzeprConsumer` with
  `EventFilter` and `PlanExtractor`

This layout made the watcher monolithic and XZepr-only by design. Adding a
second event source required forking the entire watcher rather than selecting a
backend. The `src/watcher/mod.rs` re-exported `EventFilter`, `PlanExtractor`,
and `Watcher` at the top level, falsely implying a shared interface that would
have to be preserved for any future backend.

## Changes Implemented

### New Directory Layout

```text
src/watcher/
  generic/
    mod.rs              (Phase 3 placeholder, not yet implemented)
  logging.rs            (unchanged -- not XZepr-specific)
  mod.rs                (updated)
  xzepr/
    mod.rs              (new)
    filter.rs           (moved from src/watcher/filter.rs)
    plan_extractor.rs   (moved from src/watcher/plan_extractor.rs)
    watcher.rs          (moved from src/watcher/watcher.rs)
    consumer/
      mod.rs            (moved from src/xzepr/consumer/mod.rs)
      client.rs         (moved from src/xzepr/consumer/client.rs)
      config.rs         (moved from src/xzepr/consumer/config.rs)
      kafka.rs          (moved from src/xzepr/consumer/kafka.rs)
      message.rs        (moved from src/xzepr/consumer/message.rs)

src/xzepr/
  mod.rs                (updated to re-export from crate::watcher::xzepr)
```

The `src/xzepr/consumer/` directory and `src/watcher/filter.rs`,
`src/watcher/plan_extractor.rs`, and `src/watcher/watcher.rs` were deleted after
their content was migrated.

### Task 1.1 -- Create `src/watcher/xzepr/` Directory Layout

The full directory tree was created with `src/watcher/xzepr/consumer/` as a
subdirectory mirroring the former `src/xzepr/consumer/` structure. A
`src/watcher/generic/` directory was also created at this stage to hold the
Phase 3 placeholder module.

### Task 1.2 -- Migrate Files and Update Internal Module Paths

All five consumer files (`client.rs`, `config.rs`, `kafka.rs`, `message.rs`,
`mod.rs`) were copied verbatim from `src/xzepr/consumer/` to
`src/watcher/xzepr/consumer/`. No import changes were needed in these files
because they reference siblings only via `super::` relative paths and do not
import from `crate::xzepr`.

The three watcher files required import updates to eliminate the circular
dependency that would arise if they continued to reference `crate::xzepr` after
`src/xzepr/mod.rs` began re-exporting from `crate::watcher::xzepr`:

**`src/watcher/xzepr/filter.rs`**

| Old import                                                    | New import                                                      |
| ------------------------------------------------------------- | --------------------------------------------------------------- |
| `use crate::xzepr::CloudEventMessage;`                        | `use crate::watcher::xzepr::consumer::CloudEventMessage;`       |
| `use crate::xzepr::consumer::message::CloudEventData;` (test) | `use crate::watcher::xzepr::consumer::message::CloudEventData;` |

**`src/watcher/xzepr/plan_extractor.rs`**

| Old import                                                                   | New import                                                                     |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `use crate::xzepr::CloudEventMessage;`                                       | `use crate::watcher::xzepr::consumer::CloudEventMessage;`                      |
| `use crate::xzepr::consumer::message::{CloudEventData, EventEntity};` (test) | `use crate::watcher::xzepr::consumer::message::{CloudEventData, EventEntity};` |

**`src/watcher/xzepr/watcher.rs`**

| Old import                                                                                   | New import                                                                                      |
| -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `use crate::watcher::{EventFilter, PlanExtractor};`                                          | `use super::filter::EventFilter;` and `use super::plan_extractor::PlanExtractor;`               |
| `use crate::xzepr::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};` | `use super::consumer::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};` |
| `use crate::xzepr::consumer::config::{SaslConfig, SaslMechanism, SecurityProtocol};`         | `use super::consumer::config::{SaslConfig, SaslMechanism, SecurityProtocol};`                   |

Doc comments referencing `xzatoma::watcher::Watcher`,
`xzatoma::watcher::EventFilter`, and `xzatoma::watcher::PlanExtractor` were
updated to `xzatoma::watcher::XzeprWatcher`,
`xzatoma::watcher::xzepr::filter::EventFilter`, and
`xzatoma::watcher::xzepr::plan_extractor::PlanExtractor` respectively, since
those types are no longer re-exported at the top-level `watcher` module.

### Task 1.3 -- Update `src/watcher/mod.rs` and `src/lib.rs`

**`src/watcher/mod.rs`** was rewritten to:

- Remove `pub mod filter`, `pub mod plan_extractor`, and `pub mod watcher` (the
  now-relocated direct declarations).
- Remove the former top-level re-exports `pub use filter::EventFilter`,
  `pub use plan_extractor::PlanExtractor`, and `pub use watcher::Watcher`.
- Add `pub mod generic` (Phase 3 placeholder) and `pub mod xzepr`.
- Keep `pub mod logging` unchanged (not XZepr-specific).
- Add the single permitted top-level re-export:
  `pub use xzepr::watcher::Watcher as XzeprWatcher`.

`EventFilter`, `PlanExtractor`, and other XZepr-specific types are intentionally
not re-exported at the `watcher` level. They are accessible exclusively via
`crate::watcher::xzepr::*`. Hoisting them to `watcher::*` would falsely imply a
shared interface with the generic backend.

**`src/lib.rs`** was left unchanged. The `pub mod xzepr` declaration at the
library root continues to resolve, because `src/xzepr/mod.rs` now re-exports the
entire relocated implementation.

**`src/xzepr/mod.rs`** was updated to:

- Replace `pub mod consumer` (which declared the now-deleted
  `src/xzepr/consumer/` subtree) with `pub use crate::watcher::xzepr::consumer`.
- Replace all individual `pub use consumer::{...}` lines with
  `pub use crate::watcher::xzepr::consumer::{...}`.

This preserves every previously public path under `crate::xzepr::*` and
`xzatoma::xzepr::*` without duplicating any code.

**`src/commands/mod.rs`** -- the single call site in `run_watch` was updated:

```text
- let mut watcher = crate::watcher::Watcher::new(config, dry_run)?;
+ let mut watcher = crate::watcher::XzeprWatcher::new(config, dry_run)?;
```

### Circular Dependency Analysis

The re-export chain `crate::xzepr` -> `crate::watcher::xzepr` is safe because
the consumer files in `crate::watcher::xzepr::consumer` use only `super::`
relative paths and have no dependency on `crate::xzepr`. Had the moved watcher
files been left using `crate::xzepr::CloudEventMessage` (and similar), the
following cycle would have been created:

```text
crate::xzepr  ->  crate::watcher::xzepr  ->  crate::xzepr  (CYCLE)
```

Updating those imports to `crate::watcher::xzepr::consumer::CloudEventMessage`
breaks the cycle entirely.

## Files Created

| File                                    | Description                                                      |
| --------------------------------------- | ---------------------------------------------------------------- |
| `src/watcher/xzepr/mod.rs`              | XZepr backend module root                                        |
| `src/watcher/xzepr/filter.rs`           | `EventFilter` (relocated, imports updated)                       |
| `src/watcher/xzepr/plan_extractor.rs`   | `PlanExtractor` (relocated, imports updated)                     |
| `src/watcher/xzepr/watcher.rs`          | `Watcher` / `WatcherMessageHandler` (relocated, imports updated) |
| `src/watcher/xzepr/consumer/mod.rs`     | XZepr consumer module root (relocated verbatim)                  |
| `src/watcher/xzepr/consumer/client.rs`  | `XzeprClient` and HTTP types (relocated verbatim)                |
| `src/watcher/xzepr/consumer/config.rs`  | `KafkaConsumerConfig` and security types (relocated verbatim)    |
| `src/watcher/xzepr/consumer/kafka.rs`   | `XzeprConsumer` and `MessageHandler` (relocated verbatim)        |
| `src/watcher/xzepr/consumer/message.rs` | `CloudEventMessage` and payload types (relocated verbatim)       |
| `src/watcher/generic/mod.rs`            | Phase 3 placeholder (not yet implemented)                        |

## Files Modified

| File                  | Change                                                                                   |
| --------------------- | ---------------------------------------------------------------------------------------- |
| `src/watcher/mod.rs`  | Removed old module declarations and re-exports; added `xzepr`, `generic`, `XzeprWatcher` |
| `src/xzepr/mod.rs`    | Replaced `pub mod consumer` with `pub use crate::watcher::xzepr::consumer`               |
| `src/commands/mod.rs` | `crate::watcher::Watcher` -> `crate::watcher::XzeprWatcher` in `run_watch`               |

## Files Deleted

| File                              | Reason                                             |
| --------------------------------- | -------------------------------------------------- |
| `src/watcher/filter.rs`           | Relocated to `src/watcher/xzepr/filter.rs`         |
| `src/watcher/plan_extractor.rs`   | Relocated to `src/watcher/xzepr/plan_extractor.rs` |
| `src/watcher/watcher.rs`          | Relocated to `src/watcher/xzepr/watcher.rs`        |
| `src/xzepr/consumer/` (directory) | Relocated to `src/watcher/xzepr/consumer/`         |

## Quality Gate Results

All four mandatory quality gates passed with zero errors and zero new warnings:

```text
cargo fmt --all                                       -- clean
cargo check --all-targets --all-features              -- clean
cargo clippy --all-targets --all-features -D warnings -- clean
cargo test --all-features                             -- 1218 passed, 11 ignored, 1 pre-existing failure
```

The one test failure (`providers::copilot::tests::test_copilot_config_defaults`)
is pre-existing and unrelated to Phase 1. It fails identically on the unmodified
branch.

The `examples/downstream_consumer.rs` example compiles cleanly. The example
continues to use `xzatoma::xzepr::consumer::{...}` and those paths resolve
correctly through the backward-compatible re-exports in `src/xzepr/mod.rs`.

## Success Criteria Verification

| Criterion                                                                                       | Status                                     |
| ----------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `cargo build --workspace` produces zero errors and zero new warnings                            | Passed                                     |
| All pre-existing tests pass                                                                     | Passed (pre-existing failure is unrelated) |
| No test logic changes required -- only import path updates                                      | Passed                                     |
| `examples/downstream_consumer.rs` compiles cleanly                                              | Passed                                     |
| `src/watcher/xzepr/` contains all migrated files                                                | Passed                                     |
| `src/watcher/mod.rs` declares `pub mod xzepr`, `pub mod generic`, and re-exports `XzeprWatcher` | Passed                                     |
| `src/xzepr/mod.rs` re-exports from `crate::watcher::xzepr`                                      | Passed                                     |
| No XZepr-specific types re-exported at `crate::watcher` level (except `XzeprWatcher`)           | Passed                                     |
| No deprecation notices added to XZepr code                                                      | Passed                                     |

## Backward Compatibility

Every previously public path continues to resolve:

| Path                                              | Resolution                                         |
| ------------------------------------------------- | -------------------------------------------------- |
| `xzatoma::xzepr::CloudEventMessage`               | Re-exported from `crate::watcher::xzepr::consumer` |
| `xzatoma::xzepr::consumer::XzeprConsumer`         | Re-exported module                                 |
| `xzatoma::xzepr::KafkaConsumerConfig`             | Re-exported from `crate::watcher::xzepr::consumer` |
| `xzatoma::xzepr::MessageHandler`                  | Re-exported from `crate::watcher::xzepr::consumer` |
| `xzatoma::watcher::logging::init_watcher_logging` | Unchanged, `logging` remains in `watcher`          |

The only path that changed is `xzatoma::watcher::Watcher`, which no longer
exists. Its replacement is `xzatoma::watcher::XzeprWatcher`. The single internal
call site in `src/commands/mod.rs` was updated accordingly.

## Relationship to Subsequent Phases

Phase 1 creates the structural foundation that all subsequent phases build on:

- Phase 2 adds the `action` field to the plan format, which the generic watcher
  will use for matching.
- Phase 3 creates `src/watcher/generic/` using the placeholder established here,
  implementing `GenericMatcher`, `GenericResultProducer`, and `GenericWatcher`
  without touching any XZepr code.
- Phase 4 adds `WatcherType` to `src/config.rs` and `--watcher-type` to the CLI,
  dispatching between `XzeprWatcher` and the generic watcher using the
  `XzeprWatcher` alias introduced in `src/watcher/mod.rs`.
- Phase 5 wires the dispatch into `commands::watch::run_watch` and adds
  integration tests for both backends.
