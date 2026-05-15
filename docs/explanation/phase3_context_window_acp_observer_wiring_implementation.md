# Phase 3: Wire `ContextWindowUpdated` into `AcpSessionObserver`

## Summary

Phase 3 connects the `ContextWindowUpdated` event (added in Phase 2) to the Zed
client by adding a match arm to `AcpSessionObserver.on_event()` in
`src/acp/stdio.rs`. After this phase, every provider turn in an ACP session
causes a `SessionUpdate::UsageUpdate` notification to be sent over the live
connection, allowing Zed's context window progress bar to update in real time.

## Change Made

**File:** `src/acp/stdio.rs`

**Location:** `impl AgentObserver for AcpSessionObserver`, `on_event()` method,
inserted before the wildcard `_ => {}` arm.

```xzatoma/src/acp/stdio.rs#L1921-1927
            AgentExecutionEvent::ContextWindowUpdated {
                used_tokens,
                max_tokens,
            } => {
                let update = acp::UsageUpdate::new(used_tokens, max_tokens);
                self.send_update(acp::SessionUpdate::UsageUpdate(update));
            }
```

## API Mapping

The `acp::UsageUpdate::new(used: u64, size: u64)` constructor (enabled by the
`unstable_session_usage` feature flag from Phase 1) takes two positional
arguments:

| `AgentExecutionEvent` field | `UsageUpdate` field | Meaning                       |
| --------------------------- | ------------------- | ----------------------------- |
| `used_tokens`               | `used`              | Tokens currently in context   |
| `max_tokens`                | `size`              | Total context window capacity |

## Design Decisions

- The new arm is placed before `_ => {}` so any future exhaustive match refactor
  will not accidentally drop the case.

- `send_update()` already uses `let _ =` to absorb channel send errors. No
  additional error handling is needed; a disconnected or slow client simply
  misses the notification.

- The `ContextWindowUpdated` arm is symmetric with the existing
  `ExecutionFailed` arm: both consume the event but do not mutate
  `self.text_emitted`, preserving the streaming de-duplicate logic.

## Tests Added

Both tests are in `src/acp/stdio.rs` under `mod tests`.

| Test                                                                     | What It Verifies                                                           |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| `test_acp_session_observer_sends_usage_update_on_context_window_updated` | `acp::UsageUpdate::new(500, 8192)` sets `.used == 500` and `.size == 8192` |
| `test_acp_session_observer_context_window_updated_zero_values`           | Zero values are accepted without panic                                     |

The tests directly verify the field mapping performed by the new match arm.
`acp::UsageUpdate.used` maps from `used_tokens` and `acp::UsageUpdate.size` maps
from `max_tokens`. Since `send_update` uses `let _ =`, the tests do not require
a live connection to avoid panicking.

## Quality Gates

All four mandatory gates pass with zero errors and zero warnings:

```/dev/null/shell.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Targeted test run: 2 new tests pass. Broader `acp::stdio::tests` run: 53 tests
pass, 0 failures.

## Deliverables

- [x] `ContextWindowUpdated` arm added to `AcpSessionObserver.on_event()` before
      the wildcard arm
- [x] `acp::UsageUpdate::new(used_tokens, max_tokens)` constructed and sent as
      `acp::SessionUpdate::UsageUpdate`
- [x] `test_acp_session_observer_sends_usage_update_on_context_window_updated`
      passes
- [x] `test_acp_session_observer_context_window_updated_zero_values` passes

## Next Steps

Phase 4 sends the initial `UsageUpdate` at session creation (so Zed shows a
baseline reading before the first turn) and sends a `SessionInfoUpdate` with the
auto-generated session title after the first `EndTurn` response.

## Reference

- Implementation plan: `docs/explanation/context_window_implementation_plan.md`,
  Phase 3 (L299-362)
- `acp::UsageUpdate` struct: fields `used: u64` and `size: u64`, constructor
  `UsageUpdate::new(used: u64, size: u64)`
- `AcpSessionObserver.send_update()`: `src/acp/stdio.rs` L1854-1859
