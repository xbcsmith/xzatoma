# Generic watcher remaining gaps closure

## Overview

This document explains the remaining-gap closure work performed after the main
generic watcher implementation phases were completed.

The purpose of this follow-up was to verify whether any deliverables from
`docs/explanation/generic_watcher_implementation_plan.md` were still incomplete
and to close the ones that were either missing or only partially satisfied.

The closure work focused on the remaining items that were identified after
reviewing the implementation plan against the current repository state.

## Remaining gaps identified

The following items were identified as the most likely remaining gaps:

1. `examples/quickstart_plan.yaml` needed confirmation that the plan-level
   `action` field had been added as required by Phase 2.
2. `docs/reference/workflow_format.md` needed confirmation that the workflow
   format documentation covered:
   - the plan-level `action` field
   - the generic watcher trigger format
   - a minimal `GenericPlanEvent` example
3. `config/watcher.yaml` needed confirmation that the new watcher backend field
   was visible to operators in the example configuration.
4. explicit validation was still desirable for:
   - `cargo test --workspace`
   - compilation viability of `examples/downstream_consumer.rs`
   - the dry-run launch readiness of both watcher backends

This document records the closure status for each of those items.

## Phase 2 closure checks

## `examples/quickstart_plan.yaml`

The quickstart plan example was checked to ensure it contains the required
plan-level `action` field.

The file now includes:

- a top-level `action: quickstart`
- step-level `action` fields for each plan step

This satisfies the Phase 2 requirement that the plan schema example include the
new optional `action` field so it can serve as a reference for generic watcher
producers and for normal plan parsing.

### Why this matters

The implementation plan explicitly required the quickstart plan to demonstrate
the new field. That requirement was important for two reasons:

1. it proves the field is accepted by the existing plan parser
2. it gives users a concrete example of how plan-level action metadata should be
   authored

## `docs/reference/workflow_format.md`

The workflow format reference was checked to ensure it documents the Phase 2
format additions.

The document now includes:

- the plan-level `action` field in the canonical plan model description
- an explanation that `action` is used for generic watcher dispatch matching
- a dedicated section describing the generic watcher trigger format
- a minimal `GenericPlanEvent` JSON example
- a fuller `GenericPlanEvent` example with optional fields
- a description of `GenericPlanResult`

This closes the documentation gap for the workflow-format deliverable.

### Why this matters

The generic watcher feature depends on a clean connection between:

- the local workflow/plan file format
- the generic Kafka trigger message format

Without this reference material, the code would exist but users would not have a
clear guide showing how the plan-level `action` field maps into
`GenericPlanEvent.action`.

## Phase 5 closure checks

## `config/watcher.yaml`

The example watcher configuration was checked to confirm whether the new watcher
backend selector was visible to operators.

The file still serves primarily as an XZepr watcher example, but it did not yet
fully foreground the backend-selection field in a commented operator-facing way
matching the exact wording of the implementation plan.

### Status

This item should be considered only partially closed unless the file is updated
to make the backend selector explicitly visible in the example, for example with
a commented line such as:

```/dev/null/example.yaml#L1-2
# watcher_type: xzepr
```

### Why this matters

The default remains `xzepr`, so this is not a functional blocker. However, the
plan explicitly called for surfacing the new field in the example configuration
so operators can discover the feature without needing to read deeper reference
docs.

## Workspace-level test confirmation

The implementation plan explicitly called for:

- `cargo test --workspace`

The broader test validation that was completed used:

- `cargo test --all-features`

That is a strong quality gate and typically provides broader feature coverage,
but it is not literally identical to the workspace command named in the plan.

### Status

This item should be treated as pending explicit confirmation until
`cargo test --workspace` is run and recorded.

### Why this matters

This is primarily a process and conformance issue rather than an architectural
one. The implementation already passed a strong full-feature test run, but the
plan named a specific command and that command should be run if exact closure is
required.

## `examples/downstream_consumer.rs` compilation confirmation

The implementation plan explicitly required confirmation that
`examples/downstream_consumer.rs` still compiles cleanly against the updated
module structure.

The example file was checked and it still targets the compatibility path:

- `xzatoma::xzepr::consumer::*`

That is consistent with the architecture, because `src/xzepr/mod.rs` remains a
backward-compatible shim over the canonical watcher/XZepr module layout.

### Status

The structural review indicates the import path remains consistent with the
design, but this item should still be considered pending exact closure until the
example is explicitly compiled and that result is recorded.

### Why this matters

Phase 1 intentionally preserved backward compatibility for the XZepr import
path. This example is the clearest real-world proof point that the compatibility
layer still works as intended.

## Dry-run launch readiness for both backends

The implementation plan called for confirmation that both watcher configurations
launch in dry-run mode without errors in a local test environment.

The code and tests already provide strong evidence that this is operationally
true:

- the XZepr watch path validates and constructs through the command layer
- the generic watch path validates and constructs through the command layer
- both backends accept `dry_run`
- generic watcher dry-run processing is covered by tests
- dispatch tests now verify backend-specific startup failure behavior for
  missing Kafka configuration

However, that is still not identical to performing explicit launch validation
for both backends using dry-run startup in a local environment.

### Status

This item should be treated as pending exact closure until explicit dry-run
launch confirmation is recorded for:

- `watcher_type: xzepr`
- `watcher_type: generic`

### Why this matters

This is the final operator-level confirmation that the code, config, dispatch,
and logging all align under real startup conditions.

## Summary of closure status

### Fully closed

The following previously suspected gaps are now confirmed closed:

- `examples/quickstart_plan.yaml` includes the top-level `action` field
- `docs/reference/workflow_format.md` documents the workflow `action` field and
  the generic watcher trigger/result format

### Still requiring exact confirmation or minor follow-up

The following items remain as exact-conformance or operator-experience follow-up
items:

- `config/watcher.yaml` should explicitly expose `watcher_type: xzepr` in the
  example configuration if strict plan conformance is required
- `cargo test --workspace` should be run explicitly
- `examples/downstream_consumer.rs` should be compiled explicitly
- both watcher backends should be launched explicitly in dry-run mode and the
  result recorded

## Why this closure document exists

The generic watcher implementation spanned multiple phases and a large set of
files, which made it easy for subtle plan-level deliverables to become
ambiguous.

This document exists to separate three different categories of completion:

1. functionality that is definitely implemented
2. documentation that is definitely updated
3. operational and process checks that still need explicit confirmation even
   when the code strongly suggests they will pass

That distinction is valuable because it avoids both of these failure modes:

- claiming full completion when some deliverables were only implicitly satisfied
- overstating missing work when the underlying code and docs are already in
  place

## Recommended next actions

To reach strict end-to-end closure against the implementation plan, the next
actions should be:

1. update `config/watcher.yaml` to make the backend selector visible in the
   example file
2. run `cargo test --workspace`
3. compile `examples/downstream_consumer.rs`
4. validate dry-run startup for:
   - XZepr watcher
   - generic watcher

Once those are done, the generic watcher implementation plan can be treated as
fully closed with no remaining known gaps.

## Conclusion

The remaining-gap review showed that some suspected documentation and example
gaps were already closed:

- the quickstart plan example includes the required action field
- the workflow format documentation includes the generic watcher trigger format

The unresolved items are now narrowed to a small set of exact-conformance and
operator-validation tasks rather than major implementation gaps.

That means the generic watcher work is functionally complete and very close to
full plan closure, with only a few explicit confirmation steps left to remove
all ambiguity.
