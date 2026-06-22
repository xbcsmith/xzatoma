# Phase 2: Context Window Display Implementation

## Overview

Phase 2 achieves accurate token usage reporting in Zed's context window bar by
populating `PromptResponse.usage` and sending `UsageUpdate` notifications after
every `EndTurn`. Before this work, the context window bar remained empty after
prompts completed because the agent never reported token counts back to the
client.

## Problem Statement

Before Phase 2, the context window bar in Zed showed no token usage after
prompts completed because:

- `execute_queued_prompt` returned `PromptResponse` with `usage: None`
- No `UsageUpdate` session notification was sent after prompt turns

## Solution

### Task 2.1: Populate PromptResponse.usage

In `execute_queued_prompt` (in `src/acp/stdio.rs`), before returning `Ok(...)`,
the implementation calls
`agent.get_context_info(agent.conversation().max_tokens())` to get current token
usage. It then constructs `acp::Usage::new(used_tokens, used_tokens, 0)` where:

- `total_tokens` = `used_tokens` (tokens currently in context)
- `input_tokens` = `used_tokens` (best approximation; no per-turn provider
  split)
- `output_tokens` = 0 (unknown without provider response metadata)

This block is gated by `#[cfg(feature = "unstable_session_usage")]` to match the
SDK gating.

### Task 2.2: UsageUpdate Notification After Every EndTurn

After each successful `EndTurn` in `execute_queued_prompt`, a
`SessionUpdate::UsageUpdate` notification is sent via the connection. This
updates the context window bar even when the client does not inspect
`PromptResponse.usage`.

### Task 2.3: Debug Logging for Initial UsageUpdate

A `tracing::debug!` log was added in the `create_session` initial `UsageUpdate`
block so the event is visible when running with `RUST_LOG=xzatoma::acp=debug`.

## Token Counting Approach

XZatoma uses a two-tier approach for token counting:

1. **Provider-reported usage**: When providers return token usage in responses
   (e.g., OpenAI `usage` field), `Agent::get_context_info` uses that data.
2. **Heuristic counting**: When providers do not return usage,
   `Conversation.token_count` provides an approximate count based on message
   content length.

The `total_tokens` value in `PromptResponse.usage` uses whichever source is
available.

## Design Decisions

### Why send UsageUpdate AND populate PromptResponse.usage?

Two notification paths exist because different ACP clients may use different
mechanisms to update the context bar. `PromptResponse.usage` is returned
synchronously with the prompt response; `UsageUpdate` is a push notification.
Populating both maximizes compatibility across client implementations.

### Why approximate input_tokens as used_tokens?

XZatoma does not split per-turn input vs. output tokens without provider-level
token accounting. The `total_tokens` value is the most important field for the
context window bar. A follow-up improvement can wire accurate per-turn counts
when providers expose them.

### Why gate PromptResponse.usage behind #[cfg(feature = "unstable_session_usage")]?

The `acp::Usage` struct is defined under this feature flag in
`agent-client-protocol-schema`. The project enables this feature unconditionally
(see `Cargo.toml`), so the code always compiles. The `#[cfg]` gate ensures the
code stays in sync with the SDK's own gating as the protocol stabilizes.

### Why not send UsageUpdate on Cancelled?

Cancelled turns may not have a meaningful token count since execution stopped
mid-turn. Sending a `UsageUpdate` on `Cancelled` could produce misleading token
counts. The guard `stop_reason == EndTurn` prevents this.

## Files Changed

- `src/acp/stdio.rs` - `execute_queued_prompt`: populates
  `PromptResponse.usage`, sends `UsageUpdate` after every `EndTurn`.
- `src/acp/stdio.rs` - `create_session`: added `tracing::debug!` log for the
  initial `UsageUpdate`.

## Testing

Three new unit tests verify Phase 2:

- `test_execute_queued_prompt_response_includes_usage`: verifies
  `PromptResponse.usage` is `Some` with `total_tokens > 0` after a successful
  prompt turn.
- `test_execute_queued_prompt_sends_usage_update_on_end_turn`: verifies the
  `UsageUpdate` construction logic and the `EndTurn` guard condition.
- `test_execute_queued_prompt_no_usage_update_on_cancelled`: verifies that
  `UsageUpdate` is not sent on `Cancelled` (guard condition check).

## Success Criteria

- `PromptResponse.usage` is non-null for every completed (non-cancelled) prompt
  turn.
- A `UsageUpdate` notification is sent after every `EndTurn`.
- An initial `UsageUpdate` is sent at session creation (pre-existing, debug log
  added).
- `cargo test --all-features` passes with all new tests green.
