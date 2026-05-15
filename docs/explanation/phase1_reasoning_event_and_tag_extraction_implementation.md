# Phase 1: Reasoning Event and Tag Extraction Utilities

## Overview

This document describes the implementation of Phase 1 of the thinking mode
feature for XZatoma. Phase 1 introduces the foundational infrastructure that all
subsequent phases build on: a new `AgentExecutionEvent::ReasoningEmitted`
variant and a reusable tag-stripping module that normalises inline thinking
blocks from provider responses.

## Deliverables

| Deliverable                                             | File                    | Status   |
| ------------------------------------------------------- | ----------------------- | -------- |
| `AgentExecutionEvent::ReasoningEmitted { text: String}` | `src/agent/events.rs`   | Complete |
| `extract_thinking` function                             | `src/agent/thinking.rs` | Complete |
| `pub(crate) mod thinking` declaration                   | `src/agent/mod.rs`      | Complete |
| `pub use thinking::extract_thinking` re-export          | `src/agent/mod.rs`      | Complete |
| All required tests passing                              | Both files              | Complete |

## Design Decisions

### ReasoningEmitted Variant Placement

The `ReasoningEmitted` variant is placed between `AssistantTextEmitted` and
`ToolCallStarted` in the enum body. This ordering reflects the natural execution
sequence: reasoning is emitted alongside or just before the visible assistant
text, before tool invocations begin.

### Tag Format Support

Three vendor-specific inline thinking formats are handled by a single
left-to-right scan:

| Format   | Open tag         | Close tag         | Vendor            |
| -------- | ---------------- | ----------------- | ----------------- |
| Standard | `<think>`        | `</think>`        | DeepSeek, QwQ     |
| XZatoma  | `<\|thinking\|>` | `<\|/thinking\|>` | Internal format   |
| Block    | `<\|channel>`    | `<channel\|>`     | Copilot reasoning |

All comparisons are ASCII case-insensitive so that models that emit `<THINK>` or
`<Think>` are handled correctly without extra normalisation.

### ASCII Case-Insensitive Search

Rather than calling `to_lowercase()` on the entire input (which can change byte
offsets for multi-byte Unicode characters and causes an allocation even on the
fast path), the implementation uses a private `find_ascii_case_insensitive`
helper. This function scans the haystack byte-by-byte with
`u8::eq_ignore_ascii_case`, returning a byte offset directly into the original
string slice. This keeps offsets correct for UTF-8 text containing non-ASCII
characters while still handling the all-ASCII tag patterns.

### Nesting Behaviour

The function does not track nesting depth. When an opening tag is found, the
scan immediately looks for the first matching closing tag. Nested identical tags
(for example `<think>outer<think>inner</think>`) terminate at the innermost
close, leaving `outer<think>inner` as the extracted content. Any stray closing
tag that appears in the output text will be treated as literal text because no
unmatched opening tag precedes it.

### Unclosed Tag Fallback

If an opening tag has no corresponding closing tag anywhere in the remaining
text, the opening tag itself is emitted as literal text and scanning continues
from the character after the opening tag. This prevents silent data loss when
providers emit partial or malformed responses.

### Fast Path

Two checks guard against unnecessary work when no tags are present:

1. A `str::contains('<')` scan - zero allocations, returns early for text with
   no angle brackets at all.
2. A linear scan for any of the three open tags - also zero allocations, returns
   `(input.to_string(), None)` with a single clone if no open tag is found.

Only when at least one open tag is detected does the function allocate a `clean`
buffer and `reasoning_parts` vector.

### Public Re-export

The `thinking` module is declared `pub(crate)` in `src/agent/mod.rs` to keep the
module hierarchy internal. The `extract_thinking` function is re-exported at
`pub use thinking::extract_thinking` so that the path
`xzatoma::agent::extract_thinking` is available for doc tests compiled as
external crates. This satisfies both the plan requirement
(`pub(crate) mod thinking`) and the AGENTS.md requirement for runnable doc
examples.

## Test Coverage

### `src/agent/events.rs`

| Test name                                             | Behaviour verified                                  |
| ----------------------------------------------------- | --------------------------------------------------- |
| `test_no_op_observer_accepts_all_events`              | Updated to include `ReasoningEmitted` event         |
| `test_no_op_observer_accepts_reasoning_emitted_event` | `NoOpObserver` discards `ReasoningEmitted` silently |
| `test_agent_execution_event_is_debug_clone`           | Enum derives `Debug` and `Clone` correctly          |
| `test_custom_observer_receives_events`                | Custom observer counts events correctly             |

### `src/agent/thinking.rs`

| Test name                                                             | Behaviour verified                                     |
| --------------------------------------------------------------------- | ------------------------------------------------------ |
| `test_extract_thinking_with_no_tags_returns_original_unchanged`       | Fast path returns original text, no reasoning          |
| `test_extract_thinking_strips_standard_think_tags`                    | `<think>` / `</think>` block is stripped               |
| `test_extract_thinking_strips_xzatoma_thinking_tags`                  | `<\|thinking\|>` block is stripped                     |
| `test_extract_thinking_strips_channel_block_tags`                     | `<\|channel>` block is stripped                        |
| `test_extract_thinking_strips_multiple_blocks_and_concatenates`       | Multiple blocks joined with newline                    |
| `test_extract_thinking_is_case_insensitive_for_open_tags`             | `<THINK>` and `</THINK>` handled correctly             |
| `test_extract_thinking_strips_tags_preserves_surrounding_text`        | Text surrounding the block is preserved verbatim       |
| `test_extract_thinking_empty_tag_content_yields_none_reasoning`       | Empty block yields `None` for reasoning                |
| `test_extract_thinking_all_three_formats_in_single_string`            | All three formats co-exist in a single input string    |
| `test_extract_thinking_unclosed_open_tag_treated_as_literal_text`     | Unclosed open tag becomes literal text in clean output |
| `test_extract_thinking_whitespace_only_content_yields_none_reasoning` | Whitespace-only block yields `None` for reasoning      |
| `test_find_ascii_case_insensitive_finds_exact_match`                  | Exact-case match returns correct byte offset           |
| `test_find_ascii_case_insensitive_finds_uppercase_needle`             | Case-folded match returns correct byte offset          |
| `test_find_ascii_case_insensitive_returns_none_when_absent`           | Absent needle returns `None`                           |
| `test_find_ascii_case_insensitive_empty_needle_returns_zero`          | Empty needle matches at offset 0                       |

Two doc tests in `src/agent/thinking.rs` are compiled and executed by
`cargo test`:

- Module-level example demonstrating `extract_thinking` with a `<think>` block.
- Function-level examples covering the no-tags path, the single-block path, and
  the multiple-block concatenation path.

## Quality Gate Results

All four mandatory quality gates pass on the final commit:

```text
cargo fmt --all                                    OK
cargo check --all-targets --all-features           OK
cargo clippy --all-targets --all-features -D warnings  OK
cargo test --all-features --lib                    OK  (2043 tests pass)
```

The targeted module tests pass individually:

```text
cargo test --all-features -- agent::thinking       15 passed
cargo test --all-features -- agent::events          4 passed
Doc-tests xzatoma (thinking module)                 2 passed
```
