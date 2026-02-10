# Add Safety Features (Phase 1) Implementation

## Overview

Phase 1 introduces non-breaking safety enhancements to the file-editing tool (`edit_file`) to prevent accidental destructive operations (e.g. wiping an entire Makefile). The primary goals are:

- Add a safe `append` mode for adding content to the end of files.
- Make `edit` mode safer by requiring a unique `old_text` anchor and adding heuristics that detect unusually large reductions (possible accidental replacements).
- Improve error messages so agents (and users) understand how to correct the request.
- Update agent prompts to explicitly instruct the model about safe editing patterns.

This phase is intentionally non-breaking for the tool API surface (the JSON schema was extended, not removed), but it enforces safer behavior for `edit` requests.

## Components Delivered

- Code
  - `src/tools/edit_file.rs` — Adds `Append` to `EditMode`, implements append behavior, and tightens `Edit` validation and safety checks.
  - `src/prompts/write_prompt.rs` — Adds `EDIT_FILE_USAGE_GUIDELINES` and integrates the guidance into the write-mode system prompt.
- Tests
  - Several unit tests added/updated in `src/tools/edit_file.rs` to cover append behavior and safety checks.
  - Tests for prompt guidance presence in `src/prompts/write_prompt.rs`.
- Documentation
  - `docs/explanation/add_safety_features_phase1_implementation.md` (this file)

## Implementation Details

### 1) `EditMode` expanded

A new `Append` variant was added to the `EditMode` enum so agents can explicitly request safe appends:

```xzatoma/src/tools/edit_file.rs#L25-40
/// Mode of operation for the edit_file tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    /// Perform a targeted replacement of a unique `old_text` snippet.
    /// This mode is intended for small, precise changes and REQUIRES `old_text`.
    Edit,
    /// Create a new file with the given contents
    Create,
    /// Replace a file's contents completely (DANGEROUS).
    Overwrite,
    /// Append content to the end of an existing file.
    /// Safe for adding new sections without replacing existing content.
    Append,
}
```

### 2) Append mode

- Requires the destination file to exist.
- Adds a newline separator if the original file does not end with a trailing newline.
- Performs size checks to avoid creating overly large files.
- Returns a unified diff of the appended result.

Example match arm (summary):

```xzatoma/src/tools/edit_file.rs#L130-180
EditMode::Append => {
    // Verify file exists & is not a directory
    // Read file contents, compute separator (\"\\n\" if needed)
    // Write new contents and return diff
}
```

### 3) Safer Edit mode

- `EditMode::Edit` now requires `old_text` (makes a targeted, single replacement).
- If `old_text` is missing the tool returns a helpful error:
  - "edit mode requires old_text parameter. To append to file, use 'append' mode. To replace entire file, use 'overwrite' mode explicitly."
- If `old_text` is not found: returns a helpful error including the search text and a hint to use `read_file`:
  - "The specified old_text was not found in the file. Searched for: <old_text>. Hint: Use read_file to view current contents and find a unique anchor point."
- If `old_text` occurs multiple times: error instructs the user to provide more context.
- Safety heuristic: if the file has at least 20 lines and the resulting replacement reduces the file to less than one third of the original lines, the change is blocked with a safety message (this detects likely accidental large deletions).
- Size guard on the resulting content remains in place.

Key portion (summary):

```xzatoma/src/tools/edit_file.rs#L180-260
EditMode::Edit => {
    // Require old_text
    // Validate file exists and not a directory
    // Check occurrences: none -> helpful hint, >1 -> ambiguity hint
    // Perform single replacement, then run safety heuristics:
    // - dramatic reduction detection (old_line_count >= 20 && new_line_count < old_line_count/3)
    // - resulting size guard
    // Write and return diff
}
```

### 4) Tool Definition Schema

`tool_definition()` was updated to include `"append"` in the `mode` enum and to clarify that `old_text` is required for `edit` mode:

```xzatoma/src/tools/edit_file.rs#L90-120
"mode": {
  "type": "string",
  "enum": ["edit", "create", "overwrite", "append"],
  "description": "Mode of operation"
},
"old_text": {
  "type": "string",
  "description": "Optional snippet of old text to be replaced (required for edit mode)"
}
```

This keeps the API backward-compatible while adding the new mode and clearer documentation.

### 5) Agent Prompt Guidance

A new guidance block helps agents pick the correct mode and workflow:

```xzatoma/src/prompts/write_prompt.rs#L1-40
const EDIT_FILE_USAGE_GUIDELINES: &str = r#"
FILE EDITING GUIDELINES

When modifying files, follow these rules strictly:

- create: ONLY for brand new files that don't exist yet
- edit: For targeted modifications. REQUIRES `old_text` (unique match)
- append: For safely adding content to the end of an existing file
- overwrite: DANGEROUS - replace entire file (explicit only)
...
"#;
```

This guideline is appended to the write-mode system prompt so the model receives direct, actionable instructions when operating in write mode.

## Tests

New and modified tests were added to enforce the behavior described above (all tests live under `src/tools/edit_file.rs` and `src/prompts/write_prompt.rs`):

- `test_edit_without_old_text_returns_error` — edit mode without `old_text` returns an error (and helpful guidance).
- `test_edit_dramatic_reduction_blocked` — large reductions are blocked by the safety heuristic.
- `test_append_mode_success` — append mode appends content as expected.
- `test_append_adds_separator_when_needed` — append mode inserts a newline when the file doesn't end with one.
- `test_helpful_error_when_old_text_not_found` — error message includes search text and hint to `read_file`.
- `test_tool_definition_includes_append_and_old_text_description` — tool definition JSON includes the `append` mode and `old_text` description.

Test status and validation:

- [x] `cargo fmt --all` (no changes required after formatting)
- [x] `cargo check --all-targets --all-features` (compiles cleanly)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` (no warnings)
- [x] `cargo test --all-features` (all tests pass; test count increased after adding the new tests)

(If you run these commands locally you should observe the same green-check results.)

## Usage Examples

### Before (Dangerous)
Agent issues a naive `edit` without `old_text`:

```/dev/null/before_example.json#L1-6
{
  "path": "Makefile",
  "mode": "edit",
  "content": "release-linux:\n\t@cargo build --release\n"
}
```

Result: previously this could replace the entire file (unsafe).

### After (Safe) — Recommended Workflows

Option 1 — Append (preferred for adding content):

```/dev/null/after_append.json#L1-6
{
  "path": "Makefile",
  "mode": "append",
  "content": "\nrelease-linux:\n\t@cargo build --release\n"
}
```

Option 2 — Edit with anchor (targeted replacement):

```/dev/null/after_targeted_edit.json#L1-8
{
  "path": "Makefile",
  "mode": "edit",
  "old_text": ".PHONY: all build run",
  "content": ".PHONY: all build run release-linux\n\nrelease-linux: ..."
}
```

Option 3 — If `old_text` is omitted but the agent intends to replace the whole file, use `overwrite` explicitly.

## Migration Notes & Future Phases

- Phase 1 is non-breaking: it adds `append` and safety checks and gives clear errors. Agents and integrations that relied on `edit` implicitly behaving like `overwrite` should update to explicitly use `overwrite` when that behavior is desired.
- Phase 2 (planned): move to "strict mode" where `edit` without `old_text` will be disallowed by default and any legacy fallback will be removed. Consider adding `confirm_overwrite` or `force` flags to explicitly allow risky operations when required.
- Phase 3 (production rollout): monitor agent usage metrics and error counts to iterate heuristics (e.g., tuning the dramatic reduction threshold).

## Risk Mitigation

- Clear error messages with actionable next steps to reduce agent confusion.
- Non-breaking schema changes; existing calls still valid (except behaviorally safer).
- Heuristic (line-count-based) reclamation protects against false positives by only applying when a file is sufficiently large (e.g., >= 20 lines).

## References

- Code changes:
  - `src/tools/edit_file.rs` — edit/append/overwrite logic and tests
  - `src/prompts/write_prompt.rs` — usage guidelines embedded in system prompt
- Design doc: `docs/explanation/intelligent_editing_and_buffer_management_implementation_enhanced.md`

---

If you want, I can:
- Open a staging branch with these changes (if you'd like me to prepare commits locally).
- Add additional tests or a small integration test that simulates agent behavior in a sample repository.
- Tweak the dramatic-reduction threshold if you'd prefer a different heuristic (for example: 50% reduction instead of the current 66% threshold).

Which next step would you like me to take?
