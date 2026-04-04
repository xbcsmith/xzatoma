# Grep and Search Mention Execution Implementation

## Overview

This document describes the implementation that wires up `@grep:` and `@search:`
mention execution in the `mention_parser` module. Previously, these mention
types were parsed but not executed -- the processing loop only logged debug
messages. Now they invoke the `GrepTool` to perform actual file content searches
and inject the results into the augmented prompt.

## Problem Statement

The `augment_prompt_with_mentions` function already handled `@file` and `@url:`
mentions by loading their content and injecting it into the prompt. However,
`@grep:"pattern"` and `@search:"pattern"` mentions were stubbed out with debug
logging and no execution. This meant users could write these mentions but would
receive no search results in the augmented context.

## Design Decisions

### Reuse of GrepTool

Rather than implementing a separate search mechanism, the implementation reuses
`crate::tools::GrepTool` which already provides regex-based file searching with
pagination, context lines, and gitignore-aware exclusion. This avoids code
duplication and keeps the tools module as the single source of truth for search
behavior.

### Case Sensitivity Distinction

- `@grep:"pattern"` uses **case-sensitive** matching (`case_sensitive: true`),
  treating the pattern as a precise regex. This matches the behavior users
  expect from a grep-like tool.
- `@search:"pattern"` uses **case-insensitive** matching
  (`case_sensitive: false`), treating it as a broader, more forgiving search.
  This distinction gives users two complementary search modes within the mention
  syntax.

### GrepTool Configuration Defaults

Each mention creates a `GrepTool` instance with these defaults:

| Parameter              | Value            | Rationale                                 |
| ---------------------- | ---------------- | ----------------------------------------- |
| `max_results_per_page` | 20               | Enough context without overwhelming       |
| `context_lines`        | 2                | Shows surrounding code for understanding  |
| `max_file_size`        | `max_size_bytes` | Respects the caller's size limit          |
| `excluded_patterns`    | empty            | No additional exclusions beyond gitignore |

### Error Handling Strategy

Search failures (invalid regex, I/O errors) produce a `LoadError` with kind
`ParseError` and a human-readable suggestion. The error is both recorded in the
`errors` vector and injected as a placeholder into `file_contents` so the agent
can see what went wrong. This matches the pattern used by file and URL mention
error handling.

### Result Formatting

The existing `format_search_results` function formats `SearchMatch` results into
a prompt-friendly string. It was previously annotated with `#[allow(dead_code)]`
since nothing called it. That annotation has been removed now that both grep and
search mention processing invoke it.

When no matches are found, the function returns a clear "No matches found"
message rather than silently producing empty output. This lets the agent know
the search executed successfully but yielded no results.

## Changes Made

### `src/mention_parser.rs`

1. **Removed `#[allow(dead_code)]`** from `format_search_results` -- the
   function is now actively called by the mention processing loop.

2. **Replaced the stub search/grep processing block** in
   `augment_prompt_with_mentions`. The new implementation:

   - Creates a `GrepTool` instance configured with `working_dir` and
     `max_size_bytes`
   - Calls `grep_tool.search()` with case sensitivity based on mention type
   - On success: formats results via `format_search_results`, pushes into
     `file_contents`, and records a success message
   - On error: creates a `LoadError`, pushes it into `errors`, and injects an
     error placeholder into `file_contents`

3. **Updated test `test_augment_prompt_non_file_mentions_ignored`** -- renamed
   to `test_augment_prompt_search_mention_executes` since search mentions now
   produce output. Updated assertions to verify that an empty directory yields 0
   matches with appropriate success messaging.

4. **Added three new tests:**
   - `test_augment_prompt_with_grep_mention_injects_results` -- creates a temp
     file, uses `@grep:"Hello"` (case-sensitive), and verifies results appear in
     the augmented prompt with the correct search results header.
   - `test_augment_prompt_with_search_mention_injects_results` -- creates a temp
     file, uses `@search:"apple"` (case-insensitive), and verifies
     case-insensitive matching works correctly.
   - `test_augment_prompt_with_grep_no_matches` -- creates a temp file that does
     not contain the search pattern, verifies graceful handling with "No matches
     found" in the output and 0 match count in the success message.

## Module Dependency Compliance

The implementation respects the module dependency boundaries defined in
AGENTS.md:

- `mention_parser` (part of the agent pipeline) calls into `tools::GrepTool` --
  this is permitted since the agent pipeline may call tools.
- `tools::GrepTool` remains independent with no new imports from agent or
  provider modules.
- No circular dependencies were introduced.

## Testing

All quality gates pass:

- `cargo fmt --all` -- no formatting changes needed
- `cargo check --all-targets --all-features` -- compiles cleanly
- `cargo clippy --all-targets --all-features -- -D warnings` -- zero warnings
- `cargo test --all-features -- mention_parser` -- all 63 unit tests and 2
  doc-tests pass
