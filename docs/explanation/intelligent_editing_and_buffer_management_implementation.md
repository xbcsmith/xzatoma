# Intelligent Editing and Buffer Management Implementation

## Overview

This document describes Phase 4 of the File Tools Modularization plan:
intelligent editing and buffer management. The goal of this phase was to
implement an `edit_file` tool that supports targeted edits, file creation,
and whole-file overwrite, and to provide a clear, diff-based result that
the agent can present to the user.

Key goals achieved:
- Implemented a robust `edit_file` tool supporting three modes: `create`,
  `edit`, and `overwrite`.
- Produced unified diffs for feedback using `similar::TextDiff`.
- Integrated the tool into the write-mode tool registry.
- Added unit tests covering successful and failure scenarios.
- Created this documentation file in `docs/explanation/`.


## Components Delivered

- `src/tools/edit_file.rs` (new, ~440 lines)
  - `EditFileTool` type with `EditMode` enum
  - `ToolExecutor` implementation with `tool_definition` and `execute`
  - Unit tests covering create/edit/overwrite and relevant error cases

- `src/tools/mod.rs` (minor edit)
  - Exported the module: `pub mod edit_file;`

- `src/tools/registry_builder.rs` (minor edit)
  - Registered `edit_file` tool in `build_for_write()` (Write mode)
  - Updated write-mode tool registry tests to include `edit_file`

- `docs/explanation/intelligent_editing_and_buffer_management_implementation.md` (this file)


## Implementation Details

Design highlights:
- Modes:
  - `create`: creates a new file. Fails if the target already exists.
  - `overwrite`: replaces entire file contents. Requires the file to exist.
  - `edit`: performs a targeted replacement of a unique `old_text` snippet
    in the file. If `old_text` is absent, behaves like a full-file replace.
- Path validation and safety:
  - All paths are validated with `file_utils::PathValidator::validate`.
  - Parent directories are created with `file_utils::ensure_parent_dirs`.
  - PathValidator enforces no absolute paths, disallows `~` and `..` traversal,
    and ensures the target is (or would be) inside the configured working dir.
- Diff generation:
  - The tool returns a unified, line-based diff describing the change.
  - Diff generation uses the existing helper `crate::tools::generate_diff` which
    is based on `similar::TextDiff` and emits `+ / - / ` markers per line.
- Error handling:
  - The tool returns `ToolResult::error(...)` when operations are invalid,
    such as ambiguous `old_text` matches, missing file on overwrite/edit, or
    content size exceeding the configured limit.
  - Uses `Result<T, XzatomaError>` and maps filesystem IO errors through
    `XzatomaError::Io` following project patterns.

Key implementation points (examples):

- The public types (`EditMode`, `EditFileTool`) include doc comments and are
  designed to be straightforward to test.

```src/tools/edit_file.rs#L1-220
/// Example snippet (illustrative - see file for complete implementation):
///
/// pub enum EditMode { Edit, Create, Overwrite }
///
/// pub struct EditFileTool { path_validator: PathValidator, max_file_size: u64 }
///
/// impl ToolExecutor for EditFileTool {
///     fn tool_definition(&self) -> Value { /* JSON schema */ }
///     async fn execute(&self, args: Value) -> Result<ToolResult> { /* ... */ }
/// }
```

- The tool produces a diff using the project's `generate_diff` helper:

```src/tools/file_ops.rs#L240-280
// generate_diff uses similar::TextDiff to create a line-by-line diff:
// let diff = TextDiff::from_lines(original, modified);
```

Integration:
- `ToolRegistryBuilder::build_for_write()` now registers `edit_file` (only in Write mode),
  so the tool is available for agents that are executing edits.

```src/tools/registry_builder.rs#L160-200
// Register edit_file tool for targeted edits and diffs
let edit_tool = EditFileTool::new(self.working_dir.clone(), self.tools_config.max_file_read_size as u64);
registry.register("edit_file", Arc::new(edit_tool));
```


## Testing

Unit tests added to `src/tools/edit_file.rs` cover these scenarios:

- `test_create_file_success`: create file and ensure created content and diff contains the new lines.
- `test_create_existing_file_returns_error`: creating an existing file produces an error.
- `test_overwrite_success`: overwrite an existing file and ensure diff shows deletions and insertions.
- `test_overwrite_nonexistent_returns_error`: overwriting a missing file fails.
- `test_edit_replace_unique_old_text_success`: target a unique old snippet and replace it.
- `test_edit_ambiguous_old_text_returns_error`: multiple matches produce an ambiguity error.
- `test_edit_without_old_text_behaves_like_overwrite`: `edit` with no `old_text` replaces the file.
- `test_invalid_path_returns_error`: path traversal attempts are rejected.

All tests for the project (including these) have been run locally and passed.

How to run tests:

- Run all tests:
  - `cargo test --all-features`

- Run only `edit_file` tests:
  - `cargo test edit_file -- --nocapture`


## Usage Examples

Programmatic usage of the tool (example):

```src/tools/edit_file.rs#L320-380
use xzatoma::tools::edit_file::EditFileTool;
use serde_json::json;

let tool = EditFileTool::new(std::path::PathBuf::from("."), 10_485_760);

// Create a new file:
let res = tokio_test::block_on(async {
    tool.execute(json!({
        "path": "example.txt",
        "mode": "create",
        "content": "Hello\nWorld\n"
    })).await
});
assert!(res.unwrap().success);

// Targeted edit (replace a unique snippet):
let res = tokio_test::block_on(async {
    tool.execute(json!({
        "path": "example.txt",
        "mode": "edit",
        "old_text": "World",
        "content": "Universe"
    })).await
});
assert!(res.unwrap().success);
```

JSON-style tool call (used by planner/agent):
```src/tools/edit_file.rs#L320-380
{
  "path": "src/lib.rs",
  "mode": "edit",
  "old_text": "let x = 1;",
  "content": "let x = 2;"
}
```

The tool returns a `ToolResult` whose `output` contains a readable diff that
the agent can emit back to the user.

## Validation / Commands

Recommended validation workflow (these were executed locally and succeeded):

- Format
  - `cargo fmt --all` — expected: no output (formatted)
- Compile check
  - `cargo check --all-targets --all-features` — expected: finished with 0 errors
- Lint
  - `cargo clippy --all-targets --all-features -- -D warnings` — expected: finished with 0 warnings
- Tests
  - `cargo test --all-features` — expected: tests pass (no failures)

Note: Please run any coverage analysis tools your CI uses (such as `cargo tarpaulin`)
to confirm overall coverage meets your >80% policy. The test suite passed locally.


## Deliverables

- `src/tools/edit_file.rs` — new tool with tests and doc comments.
- `src/tools/mod.rs` — module export.
- `src/tools/registry_builder.rs` — registry registration and test update.
- `docs/explanation/intelligent_editing_and_buffer_management_implementation.md` — this document.


## Next Steps / Phase 5 Preparation

- Phase 5 will focus on deprecating or migrating the old `file_ops` write operations if desired,
  and updating subagent tool filters to ensure tooling consistently selects the right
  operations in different chat modes (Planning vs Write).
- Consider adding further defensive checks that compare mtime or checksums to prevent
  accidental overwrites in concurrent editing scenarios.


## References

- File tools modularization plan: `docs/explanation/file_tools_modularization_implementation_plan.md`
- Implemented tool: `src/tools/edit_file.rs`
- Registry changes: `src/tools/registry_builder.rs`
- Diff generation helper: `src/tools/file_ops.rs`

---

If you'd like, I can:
- Add more fine-grained tests (edge cases or concurrency scenarios),
- Add integration tests exercising the registry + agent tool calling flow,
- Or prepare the Phase 5 migration changes to gradually replace overlapping operations.
