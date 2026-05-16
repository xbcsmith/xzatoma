# Phase 4 Tasks 4.1 and 4.2: File Utilities Consolidation Implementation

## Overview

Tasks 4.1 and 4.2 eliminate duplicated logic from the tools layer by promoting
two shared helpers into `src/tools/file_utils.rs`. Every tool module now calls a
single canonical implementation instead of maintaining its own copy.

---

## Task 4.1: Consolidate File Operation Helpers

### Problem

`copy_path.rs` and `move_path.rs` each contained a private
`copy_directory_recursive` method with identical logic: walk a source directory
with `WalkDir`, recreate the subtree under the destination, and count copied
files. Any bug fix or behavioral change had to be applied twice.

### Solution

A new public async function `copy_directory_recursive` was added to
`file_utils.rs`:

- Takes `&Path` for both source and destination (no struct state required).
- Returns `Result<usize, FileUtilsError>` (count of files copied).
- Uses `use walkdir::WalkDir;` as a local import so the top-level import list in
  `file_utils.rs` is not cluttered.
- Maps the `strip_prefix` error to `FileUtilsError::ParentDirCreation` with a
  descriptive message, and maps `tokio::fs` errors to `FileUtilsError::Io`.

Both `copy_path.rs` and `move_path.rs` were updated to:

1. Import `file_utils` at the module level alongside `PathValidator`.
2. Delete their private `copy_directory_recursive` / `copy_directory` impl
   blocks.
3. Call `file_utils::copy_directory_recursive(...)` in `execute`.
4. Replace the inline parent-directory creation guard with a call to the
   existing `file_utils::ensure_parent_dirs` helper (also in `file_utils.rs`).

The `use walkdir::WalkDir;` top-level import in `copy_path.rs` was removed
because `WalkDir` is no longer referenced in that file.

---

## Task 4.2: Canonical Glob Matching

### Problem

Three modules maintained independent recursive glob implementations:

- `list_directory.rs`: `glob_match` + `glob_match_recursive` free functions.
- `grep.rs`: `GrepTool::glob_match` + `GrepTool::glob_match_inner` methods plus
  a free `glob_match_recursive` function.

Each used a hand-rolled backtracking algorithm that treated `*` as matching any
character including the path separator `/`. This produced subtly different
behaviour from the `glob-match` crate already used in `find_path.rs`.

### Solution

A new public function `glob_match_pattern` was added to `file_utils.rs`:

```xzatoma/src/tools/file_utils.rs#L1-2
pub fn glob_match_pattern(text: &str, pattern: &str) -> bool {
    glob_match::glob_match(pattern, text)
}
```

The argument order note is important: the `glob-match` crate takes
`(pattern, text)` while the XZatoma convention is `(text, pattern)`, matching
the feel of `str::contains`.

#### list_directory.rs

The two free functions (`glob_match`, `glob_match_recursive`) and the
`#[allow(clippy::only_used_in_recursion)]` attribute were removed. The single
call site in `execute` was updated to
`file_utils::glob_match_pattern(&file_name, pattern)`.

#### grep.rs

The three helpers (`glob_match`, `glob_match_inner`, `glob_match_recursive`)
were removed. Two call sites were updated:

- `should_exclude`: checks both the bare filename and the full path string
  against each excluded pattern, preserving the original behaviour.
- `search` include_pattern check: updated to compute a
  working-directory-relative path string first, then falls back to checking just
  the filename. This is necessary because the `glob-match` crate's `*` wildcard
  does not cross path separators, so an absolute path like `/tmp/xyz/file.txt`
  would not match `*.txt` directly. Using a relative path (`file.txt`) makes
  `*.txt` work as expected, and `**/*.rs` works for nested paths.

### Semantic Difference from the Old Custom Implementation

| Pattern                     | Old behaviour               | New behaviour                                                           |
| --------------------------- | --------------------------- | ----------------------------------------------------------------------- |
| `*.txt` on `dir/file.txt`   | match (old `*` crossed `/`) | no match via rel-path check; falls back to filename check which matches |
| `*.txt` on `file.txt`       | match                       | match                                                                   |
| `**/*.rs` on `src/main.rs`  | match (via `*`)             | match (via `**`)                                                        |
| `*.rs` on `path/to/file.rs` | match                       | no match unless filename check applies                                  |

The tests in `grep.rs` that previously called the removed private
`tool.glob_match` method were updated to call
`crate::tools::file_utils::glob_match_pattern` directly. The
`test_glob_match_complex` assertion for `"path/to/file.rs"` with `"*.rs"` was
updated to document that single `*` no longer crosses `/`, and `"**/*.rs"` is
the correct pattern for nested paths.

---

## Files Changed

| File                          | Change                                                                  |
| ----------------------------- | ----------------------------------------------------------------------- |
| `src/tools/file_utils.rs`     | Added `copy_directory_recursive`, `glob_match_pattern`, and their tests |
| `src/tools/copy_path.rs`      | Removed private `copy_directory`; use shared helpers                    |
| `src/tools/move_path.rs`      | Removed private `copy_directory_recursive`; use shared helpers          |
| `src/tools/list_directory.rs` | Removed local glob functions; use `file_utils::glob_match_pattern`      |
| `src/tools/grep.rs`           | Removed three glob helpers; use `file_utils::glob_match_pattern`        |

---

## Quality Gates

All gates passed after the change:

- `cargo fmt --all` - no formatting changes required
- `cargo check` - zero errors in the library crate
- `cargo clippy --lib -- -D warnings` - zero warnings
- `cargo test --lib -- tools::` - 290 tests, 0 failures
