# Intelligent Editing and Buffer Management Implementation (Enhanced)

## Overview

This document describes the enhanced Phase 4 of the File Tools Modularization plan:
intelligent editing and buffer management with additional safety mechanisms to prevent
destructive file operations.

**Critical Problem Addressed**: The original implementation allowed agents to accidentally
wipe entire files when attempting to add content (e.g., adding new Makefile targets resulted
in the entire Makefile being replaced with only the new targets).

Key enhancements over the original implementation:
- Strict mode enforcement: `edit` mode REQUIRES `old_text` parameter
- Added `append` mode for safely adding content to end of files
- File change validation: blocks suspiciously large changes
- Agent instruction guidelines for proper tool usage
- Enhanced error messages that guide agents toward correct usage
- Diff preview before all destructive operations


## Components to Deliver

### 1. Enhanced `src/tools/edit_file.rs`

**Changes from original implementation:**

- `EditMode` enum expanded to include `Append` mode
- Strict validation: `edit` mode errors if `old_text` is missing
- Change magnitude validation: warns/errors on dramatic file size reductions
- Enhanced error messages that explain correct usage
- New tests for append mode and safety validations

**New EditMode enum:**

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    /// Perform targeted replacement of unique old_text snippet.
    /// REQUIRES old_text parameter. Use this for modifications.
    Edit,

    /// Create a new file. Fails if file already exists.
    /// Use this only for brand new files.
    Create,

    /// Replace entire file contents. Requires file to exist.
    /// DANGEROUS: Only use when explicitly replacing entire file.
    Overwrite,

    /// Append content to end of existing file.
    /// Safe for adding new content without replacement risk.
    Append,
}
```

**Enhanced validation logic:**

```rust
EditMode::Edit => {
    // STRICT: old_text is REQUIRED for edit mode
    let old_text = params.old_text.as_ref().ok_or_else(|| {
        XzatomaError::ToolError(
            "edit mode requires old_text parameter. \
             To append to file, use 'append' mode. \
             To replace entire file, use 'overwrite' mode explicitly.".to_string()
        )
    })?;

    // File must exist
    if !full_path.exists() {
        return Ok(ToolResult::error(format!(
            "Can't edit file: file not found ({})",
            params.path
        )));
    }

    let old = fs::read_to_string(&full_path).await.map_err(XzatomaError::Io)?;

    // Find and validate old_text occurrence
    let occurrences = old.matches(old_text.as_str()).count();

    if occurrences == 0 {
        return Ok(ToolResult::error(format!(
            "The specified old_text was not found in the file.\n\
             Searched for: {}\n\
             Hint: Use read_file to view current contents and find a unique anchor point.",
            old_text
        )));
    }

    if occurrences > 1 {
        return Ok(ToolResult::error(format!(
            "The specified old_text matches {} locations (must be unique).\n\
             Searched for: {}\n\
             Hint: Include more surrounding context to make the match unique.",
            occurrences, old_text
        )));
    }

    // Perform replacement
    let new_content = old.replacen(old_text.as_str(), &params.content, 1);

    // SAFETY CHECK: Detect dramatic file reduction
    let old_line_count = old.lines().count();
    let new_line_count = new_content.lines().count();

    if old_line_count >= 20 && new_line_count < (old_line_count / 3) {
        return Ok(ToolResult::error(format!(
            "Safety check failed: This edit would reduce file from {} lines to {} lines.\n\
             This suggests the edit may be replacing too much content.\n\
             If you intend to replace the entire file, use 'overwrite' mode explicitly.\n\
             Otherwise, review your old_text parameter to ensure it's specific enough.",
            old_line_count, new_line_count
        )));
    }

    // Size check
    if new_content.len() as u64 > self.max_file_size {
        return Ok(ToolResult::error(format!(
            "Resulting content size {} bytes exceeds maximum {} bytes",
            new_content.len(), self.max_file_size
        )));
    }

    // Write and generate diff
    fs::write(&full_path, &new_content).await.map_err(XzatomaError::Io)?;
    let diff = crate::tools::generate_diff(&old, &new_content)?;

    Ok(ToolResult::success(format!(
        "Edited {} (replaced 1 occurrence):\n\n{}",
        params.path, diff
    )))
}

EditMode::Append => {
    // File must exist
    if !full_path.exists() {
        return Ok(ToolResult::error(format!(
            "Can't append to file: file not found ({})\n\
             Hint: Use 'create' mode for new files.",
            params.path
        )));
    }

    let old = fs::read_to_string(&full_path).await.map_err(XzatomaError::Io)?;

    // Append content (add newline separator if original doesn't end with one)
    let separator = if old.ends_with('\n') { "" } else { "\n" };
    let new_content = format!("{}{}{}", old, separator, params.content);

    // Size check
    if new_content.len() as u64 > self.max_file_size {
        return Ok(ToolResult::error(format!(
            "Resulting content size {} bytes exceeds maximum {} bytes",
            new_content.len(), self.max_file_size
        )));
    }

    fs::write(&full_path, &new_content).await.map_err(XzatomaError::Io)?;
    let diff = crate::tools::generate_diff(&old, &new_content)?;

    Ok(ToolResult::success(format!(
        "Appended to {}:\n\n{}",
        params.path, diff
    )))
}
```


### 2. Agent System Instructions (`src/agent/prompts.rs` or similar)

**New module or addition to existing prompt configuration:**

```rust
/// Instructions for file editing tools to prevent destructive operations
pub const EDIT_FILE_USAGE_GUIDELINES: &str = r#"
## File Editing Guidelines

When modifying files, follow these rules strictly:

### Rule 1: Choose the Correct Mode

- **create**: ONLY for brand new files that don't exist yet
  - Will fail if file already exists (safety feature)

- **edit**: For targeted modifications to existing files
  - REQUIRES old_text parameter with unique snippet from the file
  - Use read_file first to find a good anchor point
  - Make old_text specific enough to match only once
  - NEVER use edit mode without old_text

- **append**: For adding content to the end of existing files
  - Safe for adding new sections, functions, or targets
  - Automatically handles newline separation
  - Perfect for Makefiles, config files, documentation

- **overwrite**: DANGEROUS - replaces entire file
  - ONLY use when user explicitly asks to replace entire file
  - NEVER use as default or fallback
  - Confirm with user before using this mode

### Rule 2: Workflow for Modifying Existing Files

1. Use read_file to view current contents
2. Identify a unique anchor point near where you want to make changes
3. Use edit mode with old_text set to that anchor
4. Review the diff in the response to verify changes

### Rule 3: Example - Adding to a Makefile

WRONG (will wipe file):
{
  "path": "Makefile",
  "mode": "overwrite",
  "content": "new-target:\n\t@echo hello"
}

CORRECT (targeted addition):
{
  "path": "Makefile",
  "mode": "edit",
  "old_text": ".PHONY: all build run",
  "content": ".PHONY: all build run new-target"
}

ALSO CORRECT (append to end):
{
  "path": "Makefile",
  "mode": "append",
  "content": "\nnew-target: ; $(info $(M) running new target...) @ ## New target\n\t$Q echo hello\n"
}

### Rule 4: Safety Checks Will Block You

The tool will ERROR if:
- You use edit mode without old_text (use append or overwrite instead)
- Your old_text matches multiple locations (make it more specific)
- Your old_text doesn't exist in file (read the file first)
- Your edit would reduce file size dramatically (probably wrong)

### Rule 5: When Errors Occur

Read the error message carefully - it tells you what to do:
- "requires old_text parameter" → add old_text or use different mode
- "not found in the file" → use read_file to see actual contents
- "matches N locations" → make old_text more specific with more context
- "reduce file from X to Y lines" → you're probably replacing too much
"#;
```

**Integration into agent conversation loop:**

```rust
// In agent initialization or system prompt building
let system_prompt = format!(
    "{}\n\n{}\n\n{}",
    base_system_prompt,
    EDIT_FILE_USAGE_GUIDELINES,
    tool_descriptions
);
```


### 3. Enhanced Tool Definition JSON Schema

**Update to `tool_definition()` method:**

```rust
fn tool_definition(&self) -> serde_json::Value {
    json!({
        "name": "edit_file",
        "description": "Create, edit, append to, or overwrite files with safety checks and diff preview. \
                        IMPORTANT: Use 'edit' mode with old_text for modifications, 'append' for adding \
                        content to end of file, 'create' for new files only. NEVER use 'overwrite' \
                        unless explicitly replacing entire file.",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path to the file (no absolute paths or traversal)"
                },
                "mode": {
                    "type": "string",
                    "enum": ["edit", "create", "overwrite", "append"],
                    "description": "Operation mode:\n\
                                   - 'edit': Targeted replacement (REQUIRES old_text)\n\
                                   - 'create': New file only (fails if exists)\n\
                                   - 'append': Add to end of file (safe for adding content)\n\
                                   - 'overwrite': Replace entire file (DANGEROUS - use sparingly)"
                },
                "content": {
                    "type": "string",
                    "description": "New content to write, replace with, or append"
                },
                "old_text": {
                    "type": "string",
                    "description": "REQUIRED for edit mode: Unique text snippet to find and replace. \
                                   Must match exactly once in the file. Include enough context to be unique."
                }
            },
            "required": ["path", "mode", "content"]
        }
    })
}
```


### 4. New Test Cases

**Add to `src/tools/edit_file.rs` tests module:**

```rust
#[tokio::test]
async fn test_edit_without_old_text_returns_error() {
    let temp = TempDir::new().unwrap();
    let tool = EditFileTool::new(temp.path().to_path_buf(), 10_485_760);

    // Create initial file
    let file_path = temp.path().join("test.txt");
    fs::write(&file_path, "original content").await.unwrap();

    // Try to edit without old_text - should fail
    let result = tool.execute(json!({
        "path": "test.txt",
        "mode": "edit",
        "content": "new content"
    })).await.unwrap();

    assert!(!result.success);
    assert!(result.output.contains("requires old_text parameter"));
    assert!(result.output.contains("append"));
    assert!(result.output.contains("overwrite"));
}

#[tokio::test]
async fn test_edit_dramatic_reduction_blocked() {
    let temp = TempDir::new().unwrap();
    let tool = EditFileTool::new(temp.path().to_path_buf(), 10_485_760);

    // Create file with 50 lines
    let mut content = String::new();
    for i in 1..=50 {
        content.push_str(&format!("Line {}\n", i));
    }
    let file_path = temp.path().join("test.txt");
    fs::write(&file_path, &content).await.unwrap();

    // Try to replace most of it with small content - should fail safety check
    let result = tool.execute(json!({
        "path": "test.txt",
        "mode": "edit",
        "old_text": &content, // Entire file as old_text
        "content": "Just 3 lines\nof new\ncontent"
    })).await.unwrap();

    assert!(!result.success);
    assert!(result.output.contains("Safety check failed"));
    assert!(result.output.contains("reduce file from"));
}

#[tokio::test]
async fn test_append_mode_success() {
    let temp = TempDir::new().unwrap();
    let tool = EditFileTool::new(temp.path().to_path_buf(), 10_485_760);

    // Create initial file
    let file_path = temp.path().join("Makefile");
    fs::write(&file_path, "build:\n\t@cargo build\n").await.unwrap();

    // Append new target
    let result = tool.execute(json!({
        "path": "Makefile",
        "mode": "append",
        "content": "test:\n\t@cargo test\n"
    })).await.unwrap();

    assert!(result.success);

    // Verify both targets exist
    let final_content = fs::read_to_string(&file_path).await.unwrap();
    assert!(final_content.contains("build:"));
    assert!(final_content.contains("test:"));
    assert!(result.output.contains("Appended to"));
}

#[tokio::test]
async fn test_append_adds_separator_when_needed() {
    let temp = TempDir::new().unwrap();
    let tool = EditFileTool::new(temp.path().to_path_buf(), 10_485_760);

    // Create file WITHOUT trailing newline
    let file_path = temp.path().join("test.txt");
    fs::write(&file_path, "line 1").await.unwrap();

    // Append should add separator
    let result = tool.execute(json!({
        "path": "test.txt",
        "mode": "append",
        "content": "line 2"
    })).await.unwrap();

    assert!(result.success);

    let final_content = fs::read_to_string(&file_path).await.unwrap();
    assert_eq!(final_content, "line 1\nline 2");
}

#[tokio::test]
async fn test_helpful_error_when_old_text_not_found() {
    let temp = TempDir::new().unwrap();
    let tool = EditFileTool::new(temp.path().to_path_buf(), 10_485_760);

    let file_path = temp.path().join("test.txt");
    fs::write(&file_path, "actual content").await.unwrap();

    let result = tool.execute(json!({
        "path": "test.txt",
        "mode": "edit",
        "old_text": "nonexistent text",
        "content": "replacement"
    })).await.unwrap();

    assert!(!result.success);
    assert!(result.output.contains("not found in the file"));
    assert!(result.output.contains("Searched for:"));
    assert!(result.output.contains("read_file"));
}
```


### 5. Documentation Updates

**Update to README.md or user documentation:**

```markdown
## File Editing Safety

XZatoma includes intelligent file editing with multiple safety mechanisms:

### Editing Modes

- **create**: Create new files (fails if file exists)
- **edit**: Targeted replacement using unique anchor text (SAFEST for modifications)
- **append**: Add content to end of file (SAFEST for additions)
- **overwrite**: Replace entire file (USE WITH CAUTION)

### Safety Features

1. **Required anchor text**: Edit mode requires `old_text` parameter
2. **Uniqueness validation**: Anchor text must match exactly once
3. **Change magnitude detection**: Blocks edits that dramatically reduce file size
4. **Diff preview**: All changes shown before being applied
5. **Clear error messages**: Guidance on correct usage when errors occur

### Example: Safely Adding to a Makefile

Instead of risking file replacement, use append mode:

```bash
xzatoma --write "Add a release-linux target to the Makefile that runs cargo build --release for x86_64-unknown-linux-gnu"
```

The agent will use:
```json
{
  "path": "Makefile",
  "mode": "append",
  "content": "\nrelease-linux:\n\t@cargo build --release --target x86_64-unknown-linux-gnu\n"
}
```

This safely adds to the file without touching existing content.
```


## Implementation Checklist

### Code Changes

- [ ] Update `EditMode` enum to include `Append` variant
- [ ] Add strict validation for `edit` mode requiring `old_text`
- [ ] Implement `Append` mode logic with separator handling
- [ ] Add file change magnitude validation (safety check)
- [ ] Enhance error messages with helpful guidance
- [ ] Update tool definition JSON schema with better descriptions
- [ ] Remove fallback behavior from edit mode (no more silent overwrite)

### Testing

- [ ] Add test: `test_edit_without_old_text_returns_error`
- [ ] Add test: `test_edit_dramatic_reduction_blocked`
- [ ] Add test: `test_append_mode_success`
- [ ] Add test: `test_append_adds_separator_when_needed`
- [ ] Add test: `test_helpful_error_when_old_text_not_found`
- [ ] Update existing test: remove or rename `test_edit_without_old_text_behaves_like_overwrite`
- [ ] Ensure all tests pass: `cargo test --all-features`
- [ ] Verify >80% test coverage maintained

### Agent Instructions

- [ ] Create or update `EDIT_FILE_USAGE_GUIDELINES` constant
- [ ] Integrate guidelines into agent system prompt
- [ ] Test with real agent: verify it uses append mode for Makefile additions
- [ ] Test with real agent: verify it errors when trying edit without old_text
- [ ] Test with real agent: verify helpful error messages guide it to correct usage

### Documentation

- [ ] Update README.md with safety features explanation
- [ ] Add examples showing correct usage patterns
- [ ] Document the four modes and when to use each
- [ ] Create troubleshooting guide for common errors
- [ ] Update this implementation doc with final validation results

### Validation

- [ ] Run `cargo fmt --all`
- [ ] Run `cargo check --all-targets --all-features` (0 errors)
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings` (0 warnings)
- [ ] Run `cargo test --all-features` (all pass)
- [ ] Manual test: try to reproduce original Makefile bug (should be prevented)
- [ ] Manual test: verify append mode works correctly
- [ ] Manual test: verify edit mode requires old_text


## Expected Behavior Changes

### Before (Dangerous)

Agent asked to "add release targets to Makefile":
```json
{
  "path": "Makefile",
  "mode": "edit",
  "content": "release-linux:\n\t@cargo build --release\n"
}
```
Result: File gets replaced because no `old_text` provided, fallback to overwrite.
**ENTIRE MAKEFILE WIPED** ❌

### After (Safe)

Agent asked to "add release targets to Makefile":

Option 1 - Agent uses append mode:
```json
{
  "path": "Makefile",
  "mode": "append",
  "content": "\nrelease-linux:\n\t@cargo build --release\n"
}
```
Result: Content safely added to end of file. ✅

Option 2 - Agent tries edit without old_text:
```json
{
  "path": "Makefile",
  "mode": "edit",
  "content": "release-linux:\n\t@cargo build --release\n"
}
```
Result: ERROR with message:
```
edit mode requires old_text parameter.
To append to file, use 'append' mode.
To replace entire file, use 'overwrite' mode explicitly.
```
Agent learns and retries with correct mode. ✅

Option 3 - Agent uses edit with proper anchor:
```json
{
  "path": "Makefile",
  "mode": "edit",
  "old_text": ".PHONY: all build run",
  "content": ".PHONY: all build run release-linux\n\nrelease-linux:"
}
```
Result: Targeted edit at specific location. ✅


## Migration Path

### Phase 1: Add Safety Features (Non-Breaking)
1. Add `Append` mode
2. Add validation and safety checks
3. Keep edit mode fallback behavior temporarily
4. Deploy and monitor agent behavior

### Phase 2: Enable Strict Mode (Breaking)
1. Remove edit mode fallback behavior
2. Make old_text required for edit mode
3. Update all agent prompts with guidelines
4. Test extensively in staging

### Phase 3: Production Deployment
1. Roll out to production with monitoring
2. Watch for agent errors and adjust prompts if needed
3. Document any edge cases discovered
4. Create runbook for common issues


## Risk Mitigation

### Risk 1: Agents Don't Adapt to New Requirements

**Mitigation**:
- Clear error messages that explain exactly what to do
- System prompt includes detailed examples
- Append mode provides easy alternative for common case

### Risk 2: Breaking Existing Workflows

**Mitigation**:
- Phase 1 adds features without breaking changes
- Extensive testing before enabling strict mode
- Gradual rollout with monitoring

### Risk 3: False Positives on Safety Checks

**Mitigation**:
- Tune thresholds based on real usage patterns
- Allow override mechanism if needed (with confirmation)
- Monitor rejected operations to identify legitimate uses


## Success Metrics

- **Zero file wipeouts**: No incidents where entire file replaced unintentionally
- **High append mode adoption**: >60% of file additions use append mode
- **Low error rate**: <5% of edit_file calls result in errors after agent learns patterns
- **Fast error recovery**: Agent corrects and retries within 1-2 attempts when errors occur


## Open Questions

1. Should we add a `confirm_overwrite` parameter for overwrite mode that requires explicit true value?
2. What should the exact threshold be for dramatic reduction detection? (Currently 66% reduction)
3. Should we add a `preview_only` mode that shows diff without applying changes?
4. Do we need a `patch` mode that accepts unified diff format directly?


## References

- Original implementation: `src/tools/edit_file.rs`
- Original plan: `docs/explanation/intelligent_editing_and_buffer_management_implementation.md`
- File utils: `src/tools/file_utils.rs`
- Tool registry: `src/tools/registry_builder.rs`


## Conclusion

This enhanced implementation transforms file editing from a dangerous operation into a safe,
guided process. By requiring explicit intent (old_text for edits, mode selection for operations)
and validating changes before applying them, we prevent the entire class of "accidental file
wipeout" bugs.

The key insight: **Tools should guide agents toward correct usage through errors, not silently
do the wrong thing through fallback behaviors.**
