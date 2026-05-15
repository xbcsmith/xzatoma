# Phase 1: Enable `unstable_session_usage` Feature Flag

## Summary

Phase 1 of the context window support plan enables the `unstable_session_usage`
feature flag for the `agent-client-protocol` crate in `Cargo.toml`. This is a
single-line mechanical change that unlocks `acp::UsageUpdate` and the
`acp::SessionUpdate::UsageUpdate(UsageUpdate)` variant in all downstream
modules. No application logic is changed in this phase.

## Motivation

The `agent-client-protocol` crate gates certain unstable types behind Cargo
feature flags. Before this change, only `unstable_session_model` was enabled,
making `acp::UsageUpdate` and `acp::SessionUpdate::UsageUpdate` invisible to the
compiler. Every subsequent phase (2, 3, and 4) that sends token usage
notifications to Zed depends on these types being resolvable at compile time.
Enabling the flag is therefore the mandatory first step.

## Change Made

**File:** `Cargo.toml`

Before:

```xzatoma/Cargo.toml#L94
agent-client-protocol = { version = "0.11.1", features = ["unstable_session_model"] }
```

After:

```xzatoma/Cargo.toml#L94
agent-client-protocol = { version = "0.11.1", features = ["unstable_session_model", "unstable_session_usage"] }
```

## Types Unlocked

| Type                                           | Description                                                             |
| ---------------------------------------------- | ----------------------------------------------------------------------- |
| `acp::UsageUpdate`                             | Carries `used_tokens: u64` and `max_tokens: u64` to Zed                 |
| `acp::SessionUpdate::UsageUpdate(UsageUpdate)` | The `SessionUpdate` enum variant that wraps `UsageUpdate` for transport |

## Quality Gates

All four mandatory gates pass with zero errors and zero warnings:

```/dev/null/shell.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

`cargo check --all-targets --all-features` exits with code 0 and produces no
`error[E...]` lines. The grep probe
`cargo check ... 2>&1 | grep -E "error|warning.*unused"` exits with code 1 (no
matches), confirming zero errors and zero unused-type warnings.

## Deliverables

- [x] `Cargo.toml` updated: `unstable_session_usage` added to
      `agent-client-protocol` features
- [x] `cargo check --all-targets --all-features` passes with zero errors
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes with
      zero warnings
- [x] `cargo fmt --all` produces no diff
- [x] Test suite compiles and runs (2135 tests, no compilation errors)

## Success Criteria Verification

The success criterion from the plan is:

> `cargo check --all-targets --all-features` must complete without `error[E...]`
> output. `acp::UsageUpdate::new(0u64, 0u64)` must resolve without error when
> referenced in downstream modules.

Both conditions are satisfied. The crate compiles cleanly with the new flag, and
`acp::UsageUpdate` is now available for use in phases 2, 3, and 4.

## Next Steps

Phase 2 adds the `ContextWindowUpdated` event variant to `AgentExecutionEvent`
and emits it from both execution loops (`execute_with_observer` and
`execute_provider_messages_with_observer`) immediately after provider usage is
stored. Phase 2 depends on the types unlocked in this phase.

## Reference

- Implementation plan: `docs/explanation/context_window_implementation_plan.md`,
  Phase 1 (L93-147)
- `agent-client-protocol` crate version: `0.11.1`
