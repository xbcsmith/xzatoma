# Phase 2: Add `ContextWindowUpdated` Event and Emit from Execution Loops

## Summary

Phase 2 adds the `ContextWindowUpdated` event variant to `AgentExecutionEvent`
and emits it from both execution loops immediately after provider token usage is
stored. This creates the internal signal that Phase 3 will consume to send
`SessionUpdate::UsageUpdate` notifications to Zed over the ACP connection.

## Changes Made

### `src/agent/events.rs`

**New variant** inserted between `VisionInputAttached` and
`CancellationRequested`:

```xzatoma/src/agent/events.rs#L100-112
    /// Context window state was updated after a provider response.
    ///
    /// Emitted immediately after provider token usage is stored in the
    /// conversation. `used_tokens` reflects the most accurate available count:
    /// provider-reported if present, heuristic otherwise. `max_tokens` is the
    /// configured context window size from `Conversation.max_tokens()`.
    ContextWindowUpdated {
        /// Tokens currently occupying the context window.
        used_tokens: u64,
        /// Maximum tokens available in the context window.
        max_tokens: u64,
    },
```

**`test_no_op_observer_accepts_all_events`** updated to pass
`ContextWindowUpdated` through the `NoOpObserver` to keep the exhaustive
coverage valid.

**New test** `test_context_window_updated_event_is_debug_clone` added to confirm
the variant supports `Clone` and `Debug`.

### `src/agent/core.rs`

Two symmetric emit blocks added, one per execution loop.

**`execute_with_observer`** — immediately after the closing brace of the
`if let Some(usage) = completion_response.usage` block:

```xzatoma/src/agent/core.rs#L665-671
            // Emit context window state regardless of whether provider usage was returned.
            // get_context_info() prefers provider usage over the heuristic when available.
            let ctx = self.get_context_info(self.conversation.max_tokens());
            observer.on_event(AgentExecutionEvent::ContextWindowUpdated {
                used_tokens: ctx.used_tokens as u64,
                max_tokens: ctx.max_tokens as u64,
            });
```

**`execute_provider_messages_with_observer`** — identical pattern applied after
the parallel usage block in that loop.

## Design Decisions

- The emit is placed **outside** the `if let Some(usage)` block so the event
  fires unconditionally. When the provider omits usage, `get_context_info` falls
  back to the heuristic character-divided-by-4 count stored in
  `Conversation.token_count`. This ensures Zed always receives a progress update
  even from providers that do not report token counts.

- `ctx.used_tokens` is already clamped to `max_tokens` by `ContextInfo::new`, so
  the cast to `u64` is always safe.

- The variant is placed between `VisionInputAttached` and
  `CancellationRequested` to preserve emission-sequence ordering in the enum
  definition.

## Tests Added

| Test                                                                           | File        | What It Verifies                                                |
| ------------------------------------------------------------------------------ | ----------- | --------------------------------------------------------------- |
| `test_context_window_updated_event_is_debug_clone`                             | `events.rs` | Variant derives `Debug` and `Clone`                             |
| `test_execute_with_observer_emits_context_window_updated_on_provider_response` | `core.rs`   | Event fires with provider-reported usage (100+50=150 tokens)    |
| `test_execute_provider_messages_with_observer_emits_context_window_updated`    | `core.rs`   | Event fires from the second execution loop (200+100=300 tokens) |
| `test_context_window_updated_uses_heuristic_when_no_provider_usage`            | `core.rs`   | Event fires with heuristic count when provider returns no usage |

## Quality Gates

All four mandatory gates pass with zero errors and zero warnings:

```/dev/null/shell.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Targeted test run: 4 new tests pass, 171 agent module tests pass, 0 failures.

## Deliverables

- [x] `ContextWindowUpdated` variant added to `AgentExecutionEvent` with full
      doc comment
- [x] Event emitted in `execute_with_observer` after the usage block
- [x] Event emitted in `execute_provider_messages_with_observer` after the usage
      block
- [x] `test_no_op_observer_accepts_all_events` updated with new variant
- [x] All four new tests pass

## Next Steps

Phase 3 adds an arm to `AcpSessionObserver.on_event()` in `src/acp/stdio.rs`
that matches `ContextWindowUpdated` and sends an
`acp::SessionUpdate::UsageUpdate` notification to the connected Zed client.
Phase 3 depends on the event variant added in this phase.

## Reference

- Implementation plan: `docs/explanation/context_window_implementation_plan.md`,
  Phase 2 (L147-299)
- Existing infrastructure: `Agent::get_context_info` (`src/agent/core.rs`
  L1408-1416), `Conversation::max_tokens` (`src/agent/conversation.rs` L631)
