# Phase 2: Agent Execution Loop Reasoning Plumbing

## Overview

This document describes the implementation of Phase 2 of the thinking mode
feature for XZatoma. Phase 2 wires the reasoning infrastructure built in Phase 1
into both agent execution loops so that `ReasoningEmitted` events are emitted to
observers whenever a provider returns chain-of-thought content.

## Deliverables

| Deliverable                                              | File                | Status   |
| -------------------------------------------------------- | ------------------- | -------- |
| `combine_reasoning` private helper above `impl Agent`    | `src/agent/core.rs` | Complete |
| `execute_with_observer` strips tags and emits reasoning  | `src/agent/core.rs` | Complete |
| `execute_provider_messages_with_observer` same treatment | `src/agent/core.rs` | Complete |
| Conversation history never contains thinking tag content | `src/agent/core.rs` | Complete |
| All 9 required tests passing                             | `src/agent/core.rs` | Complete |

## Design Decisions

### combine_reasoning Placement

`combine_reasoning` is a module-level private free function placed immediately
before `impl Agent`. This lets both `execute_with_observer` and
`execute_provider_messages_with_observer` call it without any duplication. Being
a free function rather than an associated function keeps it lightweight and
testable in isolation via `super::combine_reasoning` from the `mod tests` block.

### Precedence and Combination Rules

When a completion response carries reasoning from two sources simultaneously,
the following precedence applies:

| `raw_reasoning` | `tag_reasoning` | Emitted `ReasoningEmitted.text` |
| --------------- | --------------- | ------------------------------- |
| `Some(r)`       | `Some(t)`       | `format!("{}\n{}", r, t)`       |
| `Some(r)`       | `None`          | `r`                             |
| `None`          | `Some(t)`       | `t`                             |
| `None`          | `None`          | event not emitted               |

Raw reasoning (from `CompletionResponse.reasoning`) is listed first in the
concatenated output because it is the structured field produced by the
provider's own stream accumulator and therefore more authoritative than ad-hoc
inline tags.

### Tag Stripping Before Every Add-to-History Call

The stripping logic runs on `message.content` using `Option::take` before the
message is passed to `self.conversation.add_message`. This guarantees that
thinking tags can never be written into conversation history regardless of the
execution path taken later (tool calls, final response, or auto-summarisation).

The pattern used is:

```rust
let tag_reasoning = if let Some(text) = message.content.take() {
    let (clean, tag_r) = extract_thinking(&text);
    message.content = Some(clean);
    tag_r
} else {
    None
};
```

`Option::take` moves the owned `String` out without cloning, and the cleaned
string is moved back in. No borrow-checker friction arises because `message` is
declared `mut` after the `let mut message = completion_response.message` rename.

### No Shared Helper for the Two Loops

The plan explicitly requires the same six-step transformation to appear in both
`execute_with_observer` and `execute_provider_messages_with_observer` rather
than in a shared helper method. This keeps each loop self-contained and the diff
easy to review. The only shared piece is `combine_reasoning`, which is pure and
has no access to `self`.

### import Path

`extract_thinking` is imported at the top of `core.rs` as
`use super::thinking::extract_thinking;`. Because `thinking` is `pub(crate)` in
`agent/mod.rs`, all code within the same crate can use it via `super::thinking`.

## Modified Code Sections

### execute_with_observer (src/agent/core.rs ~L613)

After `completion_response` is received from the `tokio::select!`:

1. `let raw_reasoning = completion_response.reasoning;` is captured before
   `completion_response.message` is moved.
2. `let mut message = completion_response.message;` — `mut` added.
3. Inline tag stripping via `extract_thinking` runs on `message.content`.
4. `combine_reasoning(raw_reasoning, tag_reasoning)` produces the combined text.
5. `observer.on_event(AgentExecutionEvent::ReasoningEmitted { text: combined })`
   is emitted when the result is `Some`.
6. All subsequent events (`ProviderResponseReceived`, `AssistantTextEmitted`)
   and the `add_message` call use the already-cleaned `message`.

### execute_provider_messages_with_observer (src/agent/core.rs ~L952)

Identical six-step transformation applied at the parallel location in the
provider-messages loop.

## Test Coverage

### Pure unit tests for combine_reasoning

| Test name                                       | Behaviour verified                          |
| ----------------------------------------------- | ------------------------------------------- |
| `test_combine_reasoning_both_some_concatenates` | Both sources joined with newline            |
| `test_combine_reasoning_only_raw_returns_raw`   | Only raw source preserved unchanged         |
| `test_combine_reasoning_only_tags_returns_tags` | Only tag source preserved unchanged         |
| `test_combine_reasoning_both_none_returns_none` | Returns `None` when no reasoning is present |

### execute_with_observer integration tests

| Test name                                                                   | Behaviour verified                                   |
| --------------------------------------------------------------------------- | ---------------------------------------------------- |
| `test_execute_with_observer_emits_reasoning_from_completion_response_field` | Structured `.reasoning` field emits event            |
| `test_execute_with_observer_emits_reasoning_from_think_tags_in_content`     | Tag-extracted reasoning emits event; clean text sent |
| `test_execute_with_observer_does_not_store_tags_in_conversation`            | Raw tags never reach conversation history            |
| `test_execute_with_observer_combines_raw_and_tag_reasoning`                 | Both sources merged correctly                        |

### execute_provider_messages_with_observer integration test

| Test name                                                      | Behaviour verified                       |
| -------------------------------------------------------------- | ---------------------------------------- |
| `test_execute_provider_messages_with_observer_emits_reasoning` | Reasoning emitted in the second loop too |

### MockProviderWithReasoning

A new test-only mock struct `MockProviderWithReasoning` was added to the test
module. It wraps a single `Message` and an optional `reasoning: Option<String>`,
returning a `CompletionResponse` with `set_reasoning` applied on the first call
and a bare "Done" response on any subsequent call.

## Quality Gate Results

All four mandatory quality gates pass:

```text
cargo fmt --all                                        OK
cargo check --all-targets --all-features               OK
cargo clippy --all-targets --all-features -D warnings  OK
cargo test --all-features --lib -- agent::core         32 passed, 0 failed
```

Targeted success-criteria commands from the plan:

```text
cargo test -- agent::core::tests::test_combine_reasoning          4 passed
cargo test -- agent::core::tests::test_execute_with_observer_emits_reasoning
                                                                   3 passed
cargo test -- agent::core::tests::test_execute_provider_messages_with_observer_emits_reasoning
                                                                   1 passed
```
