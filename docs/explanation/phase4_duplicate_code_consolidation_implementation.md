# Phase 4: Duplicate Code Consolidation - Implementation Summary

## Overview

Phase 4 eliminated four categories of duplicated code across the tool and
provider layers. Every shared helper is now defined once and imported by all
callers, making future changes easier and reducing the surface area for
behavioural drift.

---

## Task 4.1: Consolidate File Operation Helpers

### Problem

`copy_path.rs` and `move_path.rs` each contained an independent recursive
directory-copy implementation (`copy_directory` and `copy_directory_recursive`
respectively). The two functions were structurally identical: walk the source
tree with `WalkDir`, recreate the directory skeleton, then copy each file. Both
tools also contained identical inline parent-directory creation and
post-creation revalidation blocks.

### Solution

Two shared helpers were added to `src/tools/file_utils.rs`:

**`copy_directory_recursive(source, destination) -> Result<usize, FileUtilsError>`**

The single authoritative implementation of recursive directory copying. Returns
the count of files copied so callers can report meaningful progress messages
without duplicating the counting logic.

**`ensure_parent_dirs(path) -> Result<(), FileUtilsError>`**

This function already existed. Both tools now call it instead of the repeated
inline `if let Some(parent) = destination.parent() { create_dir_all(...) }`
blocks.

### Files Changed

| File                      | Change                                                         |
| ------------------------- | -------------------------------------------------------------- |
| `src/tools/file_utils.rs` | Added `copy_directory_recursive` public async function         |
| `src/tools/copy_path.rs`  | Removed private `copy_directory`; uses shared helper           |
| `src/tools/move_path.rs`  | Removed private `copy_directory_recursive`; uses shared helper |

### Behaviour Preserved

Tool-facing user messages (e.g. `"Failed to copy directory: {}"` vs.
`"Failed to move directory: {}"`) remain in each individual tool so the output
is still contextually meaningful. Only the filesystem mechanics are shared.

---

## Task 4.2: Single Glob Matching Implementation

### Problem

Three tool modules each contained independent glob-matching code:

- `list_directory.rs` - custom `glob_match` free function plus
  `glob_match_recursive` helper
- `grep.rs` - `glob_match` and `glob_match_inner` methods on `GrepTool` plus a
  `glob_match_recursive` free function
- `find_path.rs` - already used the `glob-match` crate directly

The custom implementations used a simple `*`-matches-everything model that did
not distinguish path separators, which diverges from the `**` wildcard semantics
supported by the `glob-match` crate.

### Solution

A single public wrapper was added to `src/tools/file_utils.rs`:

**`glob_match_pattern(text: &str, pattern: &str) -> bool`**

Delegates to `glob_match::glob_match` with the correct argument order
(`pattern, text`). Supports `*` (any non-separator characters), `?` (single
character), and `**` (any path segment including separators).

Both `list_directory.rs` and `grep.rs` now call `file_utils::glob_match_pattern`
instead of their private implementations. The `grep.rs` include-pattern check
uses the workspace-relative path to ensure `*` patterns behave correctly
(single-level match only).

### Files Changed

| File                          | Change                                                                               |
| ----------------------------- | ------------------------------------------------------------------------------------ |
| `src/tools/file_utils.rs`     | Added `glob_match_pattern` public function                                           |
| `src/tools/list_directory.rs` | Removed custom `glob_match` + `glob_match_recursive`; uses shared helper             |
| `src/tools/grep.rs`           | Removed `glob_match`, `glob_match_inner`, `glob_match_recursive`; uses shared helper |

### Behaviour Notes

The `glob-match` crate treats `*` as a single path-level wildcard (does not
cross `/`). This is the correct and consistent behaviour. The old custom
implementations allowed `*.txt` to match `dir/file.txt`, which was technically
incorrect for path-based patterns. Tests that verified the old incorrect
behaviour were updated to document the correct semantics (`**/*.txt` is the
right pattern to match across directories).

---

## Task 4.3: Shared Provider Cache Helpers

### Problem

`openai.rs` and `ollama.rs` both defined an identical local type alias:

```xzatoma/src/providers/openai.rs#L46
type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>;
```

`ollama.rs` also defined a local `is_cache_valid(Instant) -> bool` associated
function. `copilot.rs` defined a `MODEL_CACHE_DURATION` constant with the same
300-second value. All three providers used the same five-minute TTL but
expressed it independently, meaning a TTL change would require editing three
files.

### Solution

A new `src/providers/cache.rs` module provides:

| Item                      | Type           | Description                                      |
| ------------------------- | -------------- | ------------------------------------------------ |
| `ModelCache`              | type alias     | `Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` |
| `MODEL_CACHE_TTL_SECS`    | `u64` constant | `300` (five minutes)                             |
| `new_model_cache()`       | function       | Returns an empty `ModelCache`                    |
| `is_cache_valid(Instant)` | function       | Returns `true` while within TTL                  |

All four items are re-exported from `crate::providers` so callers can import
them without reaching into the `cache` submodule directly.

### Files Changed

| File                       | Change                                                                      |
| -------------------------- | --------------------------------------------------------------------------- |
| `src/providers/cache.rs`   | New file - single source of truth for cache types and TTL                   |
| `src/providers/mod.rs`     | Added `pub mod cache` and re-exports                                        |
| `src/providers/openai.rs`  | Removed local `ModelCache`; uses `new_model_cache()` and `is_cache_valid()` |
| `src/providers/ollama.rs`  | Removed local `ModelCache` and `is_cache_valid`; uses cache module          |
| `src/providers/copilot.rs` | `MODEL_CACHE_DURATION` now derived from `MODEL_CACHE_TTL_SECS`              |

### Design Decision

The `CopilotCache` struct in `copilot.rs` is intentionally kept as-is because it
caches multiple heterogeneous values (`Vec<ModelInfo>`, `Vec<CopilotModelData>`,
and a single `Instant`) in a single struct, which is architecturally different
from the `(Vec<ModelInfo>, Instant)` tuple used by OpenAI and Ollama. The TTL
value is now shared; the cache struct itself is provider-specific.

---

## Task 4.4: Tool Registry Helpers and Test Provider Builder

### Problem

**Registry helpers**: There was no programmatic way to enumerate which tools
belonged to which logical group (read-only, file mutation, terminal). Tests used
hard-coded magic numbers for registry sizes.

**Test mocks**: Every test module that needed a `Provider` instance duplicated a
minimal struct with identical boilerplate: `is_authenticated`, `current_model`,
`set_model`, `fetch_models`, and `complete`. There was no single configurable
builder.

### Solution

**Three named-group helpers** were added to `src/tools/registry_builder.rs` as
standalone public functions before the `ToolRegistryBuilder` struct:

| Function                     | Returns                   | Tools                                                                                  |
| ---------------------------- | ------------------------- | -------------------------------------------------------------------------------------- |
| `read_only_tool_names()`     | `&'static [&'static str]` | `read_file`, `list_directory`, `find_path`                                             |
| `file_mutation_tool_names()` | `&'static [&'static str]` | `write_file`, `edit_file`, `delete_path`, `copy_path`, `move_path`, `create_directory` |
| `terminal_tool_names()`      | `&'static [&'static str]` | `terminal`                                                                             |

These are zero-cost compile-time constants. Tests can now assert that a built
registry contains exactly the expected tool names by iterating these slices
instead of checking raw counts.

**`TestProviderBuilder` and `TestProvider`** were added to `src/test_utils.rs`.
The builder lets tests configure only the fields they care about:

- `.with_model(name)` - sets the model name returned by `get_current_model`
- `.with_authenticated(bool)` - controls `is_authenticated` return value
- `.with_completion(text)` - sets the assistant message returned by `complete`

Defaults are `"test-model"`, `true`, and `"test response"` respectively, so a
`TestProviderBuilder::new().build()` works out of the box.

### Files Changed

| File                            | Change                                                         |
| ------------------------------- | -------------------------------------------------------------- |
| `src/tools/registry_builder.rs` | Added three tool-group name helpers and five new tests         |
| `src/test_utils.rs`             | Added `TestProviderBuilder`, `TestProvider`, and six new tests |

---

## Quality Gate Results

All quality gates pass after all changes:

| Gate                                                       | Result               |
| ---------------------------------------------------------- | -------------------- |
| `cargo fmt --all`                                          | Clean                |
| `cargo check --all-targets --all-features`                 | No errors            |
| `cargo clippy --all-targets --all-features -- -D warnings` | No warnings          |
| `cargo test --all-features`                                | 764 passed, 0 failed |

---

## Deliverables

- [x] Shared recursive directory copy helper in `file_utils.rs`
- [x] Single glob matching implementation (`glob_match_pattern`) for list, grep,
      and find-path tools
- [x] Shared provider cache types with single TTL source of truth
- [x] `new_model_cache()` constructor used by OpenAI and Ollama providers
- [x] Tool registry grouping helpers (`read_only_tool_names`,
      `file_mutation_tool_names`, `terminal_tool_names`)
- [x] Reusable `TestProviderBuilder` and `TestProvider` in `test_utils.rs`
