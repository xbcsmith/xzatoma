# Phase 4 Task 4.4: Simplify Tool Registry and Test Mocks

## Overview

Task 4.4 improves test ergonomics in two areas:

- **4.4a**: Named tool group helper functions in `registry_builder.rs` remove
  magic strings from registry tests and make the relationship between mode and
  registered tools explicit.
- **4.4b**: A `TestProviderBuilder` and `TestProvider` in `test_utils.rs` give
  unit tests a zero-boilerplate, configurable `Provider` implementation that
  makes no real HTTP calls.

---

## 4.4a: Named Tool Group Helpers

### Problem

Registry tests previously relied on bare string literals and `len()` counts to
assert which tools were registered. This made tests brittle: adding or removing
a tool from a mode could silently break tests in distant files with no clear
connection between the expected count and the actual tool set.

### Solution

Three standalone public functions were added to `src/tools/registry_builder.rs`,
placed before the `ToolRegistryBuilder` struct definition so they are
immediately visible at the module level:

| Function                     | Returns                   | Represents                |
| ---------------------------- | ------------------------- | ------------------------- |
| `read_only_tool_names()`     | `&'static [&'static str]` | Planning-mode tools       |
| `file_mutation_tool_names()` | `&'static [&'static str]` | Write-mode mutation tools |
| `terminal_tool_names()`      | `&'static [&'static str]` | Terminal execution tools  |

These functions return `&'static [&'static str]` because the lists are
compile-time constants. No heap allocation occurs on each call.

### Canonical Tool Sets

Planning mode (read-only):

- `read_file`
- `list_directory`
- `find_path`

Write mode adds file mutation tools:

- `write_file`
- `edit_file`
- `delete_path`
- `copy_path`
- `move_path`
- `create_directory`

Write mode also adds terminal execution:

- `terminal`

### Registry Verification Pattern

The helpers enable loop-based registry verification instead of repeated
individual `assert!` calls:

```xzatoma/src/tools/registry_builder.rs#L1-1
// iterate all three groups to verify write-mode coverage
for name in read_only_tool_names()
    .iter()
    .chain(file_mutation_tool_names())
    .chain(terminal_tool_names())
{
    assert!(registry.get(name).is_some(), "missing tool: {}", name);
}
```

Iteration uses `.iter().chain()` because all three functions return `&[&str]`.
`.iter()` produces `&&str` items; `.chain()` accepts any `IntoIterator` with a
compatible item type. Rust's auto-deref coercion converts `&&str` to `&str` at
the `registry.get(name)` call site.

### Tests Added

Four tests were added to `registry_builder.rs`:

- `test_read_only_tool_names_contains_expected_tools`
- `test_file_mutation_tool_names_contains_expected_tools`
- `test_terminal_tool_names_contains_terminal`
- `test_planning_registry_matches_read_only_names`
- `test_write_registry_contains_all_tool_groups`

---

## 4.4b: Configurable Test Provider Builder

### Problem

Provider-dependent unit tests had to either mock at the trait-object level
(requiring `mockall` setup) or skip testing provider interactions entirely.
There was no lightweight in-process `Provider` implementation that could be
configured per-test.

### Solution

Two types were added to `src/test_utils.rs`:

- `TestProviderBuilder`: builder that configures a `TestProvider` without
  boilerplate.
- `TestProvider`: `Provider` implementation backed by in-memory state.

Both types are gated behind `#[cfg(test)]` via the module declaration in
`lib.rs`, so they never appear in production builds.

### TestProviderBuilder

```xzatoma/src/test_utils.rs#L1-1
let provider = TestProviderBuilder::new()
    .with_model("my-model")
    .with_authenticated(true)
    .with_completion("Hello!")
    .build();
```

Builder defaults:

| Field             | Default           |
| ----------------- | ----------------- |
| `model`           | `"test-model"`    |
| `authenticated`   | `true`            |
| `completion_text` | `"test response"` |

### TestProvider Provider Trait Implementation

`TestProvider` implements all required `Provider` methods:

| Method                      | Behavior                                                                        |
| --------------------------- | ------------------------------------------------------------------------------- |
| `is_authenticated`          | Returns the configured `authenticated` field                                    |
| `current_model`             | Returns `Some(&self.model)`                                                     |
| `set_model`                 | Updates `self.model` in memory                                                  |
| `fetch_models`              | Returns a single `ModelInfo` constructed from `self.model`                      |
| `complete`                  | Returns a `CompletionResponse` with `Message::assistant(&self.completion_text)` |
| `get_current_model`         | Returns `self.model.clone()` (overrides the default to avoid `"none"`)          |
| `get_provider_capabilities` | Returns `ProviderCapabilities::default()`                                       |

`#[async_trait]` is applied to the `impl` block because the `Provider` trait
uses `async_trait` for its async methods.

### Import Resolution

Bringing `crate::error::Result` into scope alongside `XzatomaError` would have
shadowed `std::result::Result` and broken the existing `assert_error_contains`
function signature. The fix qualifies the pre-existing signature and test
annotations as `std::result::Result<T, XzatomaError>` while the new
`TestProvider` methods use the unqualified `Result<T>` alias from
`crate::error`.

### Tests Added

Six tests were added to `test_utils.rs`:

- `test_test_provider_builder_default_authenticated`
- `test_test_provider_builder_with_model`
- `test_test_provider_builder_with_unauthenticated`
- `test_test_provider_complete_returns_configured_text` (async)
- `test_test_provider_fetch_models_returns_configured_model` (async)
- `test_test_provider_builder_default`

---

## Files Modified

| File                            | Change                                                                                                     |
| ------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `src/tools/registry_builder.rs` | Added `read_only_tool_names`, `file_mutation_tool_names`, `terminal_tool_names` functions and five tests   |
| `src/test_utils.rs`             | Added `TestProviderBuilder`, `TestProvider`, six tests, and corrected `std::result::Result` qualifications |

No other files were modified.
