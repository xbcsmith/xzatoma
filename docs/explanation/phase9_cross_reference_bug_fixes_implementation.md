# Phase 9: Cross-Reference Bug Fixes

## Overview

Phase 9 audits XZatoma against a set of bugs that were found and fixed in the
Atoma agent codebase. Three bugs were present in both codebases and are fixed
here. Four were not present in XZatoma at all due to prior design decisions or
pre-existing implementation choices.

## Issue Inventory

| Bug                                                                          | Atoma Status | XZatoma Status                                                                      |
| ---------------------------------------------------------------------------- | ------------ | ----------------------------------------------------------------------------------- |
| Initial UsageUpdate size uses config default instead of model context window | Fixed        | Fixed in Phase 9                                                                    |
| ToolCallCompleted emits empty output for non-zero exit tools                 | Fixed        | Fixed in Phase 9                                                                    |
| OpenAI provider reports 0 context window for all models                      | Fixed        | Fixed in Phase 9                                                                    |
| unstable_session_usage feature not in default                                | Fixed        | Not applicable: XZatoma enables the feature directly on the dependency line         |
| NewSessionRequest resumes prior conversations                                | Fixed        | Design difference: XZatoma intentionally resumes by workspace                       |
| Ollama context-window extraction misses versioned architecture names         | Fixed        | Not applicable: XZatoma already uses a three-tier approach                          |
| UTF-8 panic in streamed thinking parser                                      | Fixed        | Not applicable: XZatoma uses String::from_utf8_lossy and no raw byte-offset slicing |
| ACP diff viewer support for file-writing tools                               | Added        | Not yet implemented (future phase)                                                  |

## Task 9.1: Initial UsageUpdate Size Uses Model Context Window

### Problem

In `create_session`, the initial `UsageUpdate` used
`agent.conversation().max_tokens()` as the `size` field. This value comes from
the configuration file and defaults to 100,000. For providers that report the
actual model context window, the Zed context bar denominator would show the
config default instead of the true capacity.

### Fix

A `model_context_window_from_state` helper function in `src/acp/stdio.rs`
extracts the `contextWindow` key from the `meta` map of the matching model
inside the already-fetched `SessionModelState`. The initial `UsageUpdate` uses
this value. It falls back to `agent.conversation().max_tokens()` when:

- the model listing failed and `SessionModelState` is unavailable
- the provider reports `0` for the context window
- the current model is not found in the available models list

No additional network requests are made. The `SessionModelState` is fetched once
as part of session creation and reused here.

### Files Changed

| File               | Change                                                                             |
| ------------------ | ---------------------------------------------------------------------------------- |
| `src/acp/stdio.rs` | `model_context_window_from_state` helper + updated initial `UsageUpdate` + 5 tests |

### Tests

| Test name                                              | What it verifies                                                            |
| ------------------------------------------------------ | --------------------------------------------------------------------------- |
| `test_model_context_window_from_meta`                  | Returns `contextWindow` from the `meta` map when the current model matches  |
| `test_model_context_window_fallback_when_no_meta`      | Falls back to the config value when the model has no `meta` map             |
| `test_model_context_window_fallback_when_zero`         | Falls back to the config value when `contextWindow` is `0`                  |
| `test_model_context_window_fallback_for_unknown_model` | Falls back to the config value when the current model ID is not in the list |
| `test_model_context_window_ignores_non_current_models` | Does not use the context window of a model that is not the current one      |

## Task 9.2: ToolCallCompleted Emits Full Output for Non-Zero Exit Tools

### Problem

Both `execute_with_observer` and `execute_provider_messages_with_observer` in
`src/agent/core.rs` emitted `ToolCallCompleted` with
`output: tool_result.output.clone()`. For terminal commands that exit non-zero,
the terminal tool stores the captured output in the `error` field and leaves
`output` as an empty string. Zed's tool call card displayed an empty body for
every failed command, making it impossible to see why the command failed.

### Fix

Changed `output: tool_result.output.clone()` to
`output: tool_result.to_message()` in both execution paths.

- For success, `to_message()` returns the same value as `output`, so successful
  tool calls are unaffected.
- For failure, `to_message()` returns a formatted string of the form
  `"Error: Exit code N: <full captured output>"`, which is what Zed displays in
  the tool call card.

### Files Changed

| File                | Change                                                                    |
| ------------------- | ------------------------------------------------------------------------- |
| `src/agent/core.rs` | `ToolCallCompleted` uses `to_message()` in both execution paths + 2 tests |

### Tests

| Test name                                              | What it verifies                                                                                                |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `test_tool_call_completed_uses_to_message_for_success` | A successful `ToolResult` produces the same string from `to_message()` as from `output`                         |
| `test_tool_call_completed_uses_to_message_for_failure` | A failed `ToolResult` produces a non-empty error string from `to_message()` instead of the empty `output` field |

## Task 9.3: OpenAI Provider Reports Actual Context Windows

### Problem

`src/providers/openai.rs` always passed `0` as the context window when
constructing `ModelInfo` in both `list_models` and `get_model_info`. A zero
context window causes `model_context_window_from_state` (Task 9.1) to fall
through to the config fallback. It also shows `0` in Zed's model selector, which
is misleading.

### Fix

Added `pub fn context_window_for_model_id(id: &str) -> usize` that
pattern-matches on model ID prefix to return a known context window:

| Pattern             | Context window |
| ------------------- | -------------- |
| `gpt-4.1*`          | 1,047,576      |
| `o1*`, `o3*`, `o4*` | 200,000        |
| `gpt-4-32k*`        | 32,768         |
| `gpt-3.5*`          | 16,385         |
| all others          | 128,000        |

All three call sites that previously passed `0` now call
`context_window_for_model_id(&entry.id)`.

### Files Changed

| File                      | Change                                                                  |
| ------------------------- | ----------------------------------------------------------------------- |
| `src/providers/openai.rs` | `context_window_for_model_id` function + updated 3 call sites + 5 tests |

### Tests

| Test name                                    | What it verifies                                     |
| -------------------------------------------- | ---------------------------------------------------- |
| `test_context_window_gpt4_1_returns_million` | `gpt-4.1` and `gpt-4.1-mini` both return 1,047,576   |
| `test_context_window_o_series_returns_200k`  | `o1`, `o3-mini`, and `o4` all return 200,000         |
| `test_context_window_gpt35_returns_16k`      | `gpt-3.5-turbo` returns 16,385                       |
| `test_context_window_gpt4_returns_128k`      | `gpt-4` and `gpt-4-turbo` return 128,000             |
| `test_context_window_unknown_returns_128k`   | An unrecognized model ID returns the 128,000 default |

## Task 9.4: Bugs Not Present in XZatoma

### unstable_session_usage Feature Flag

Atoma required an explicit Cargo feature flag to be added to the workspace
`default` features list before the session-usage API was activated. XZatoma
enables `unstable_session_usage` directly on the dependency line in `Cargo.toml`
using the `features = [...]` key, so the flag is unconditionally active and
there is no separate feature-gate step to forget.

### Ollama Context-Window Extraction

Atoma's Ollama provider failed to recognize versioned architecture names such as
`llama3.2` because its extraction logic only checked for exact architecture
strings. XZatoma already uses a three-tier extraction chain: first, a direct
lookup of the dynamic key reported by the model; second, a bare architecture
name fallback; and third, a linear scan of all known model families. This
approach handles versioned suffixes without an additional code change.

### UTF-8 Panic in Streamed Thinking Parser

Atoma used raw byte-offset slicing on streamed content, which panicked when a
multi-byte UTF-8 character was split across two chunks. XZatoma uses
`String::from_utf8_lossy` throughout its streaming pipeline and never performs
byte-offset slicing on raw buffers, so this class of panic cannot occur.

### NewSessionRequest Resuming Prior Conversations

Atoma treated `NewSessionRequest` as a strictly fresh start and was later fixed
to not resume prior conversations when a new session is requested. XZatoma
intentionally resumes by workspace: when a session for the same workspace
already exists, it is continued rather than discarded. XZatoma does not
advertise the `load_session` capability to Zed, so Zed never sends an explicit
load request; the resume behavior is an internal implementation detail. This is
a deliberate design difference rather than a bug.

## Files Changed

| File                      | Change                                                                             |
| ------------------------- | ---------------------------------------------------------------------------------- |
| `src/acp/stdio.rs`        | `model_context_window_from_state` helper + updated initial `UsageUpdate` + 5 tests |
| `src/agent/core.rs`       | `ToolCallCompleted` uses `to_message()` in both execution paths + 2 tests          |
| `src/providers/openai.rs` | `context_window_for_model_id` function + updated 3 call sites + 5 tests            |
