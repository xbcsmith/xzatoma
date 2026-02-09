/*!
edit_file tool for XZatoma

Provides an intelligent editing tool that supports four modes:
- `create`: create a new file (creates parent directories as needed)
- `edit`: make a targeted edit (replace a single matching snippet). STRICT MODE: REQUIRES `old_text` to be provided (no fallback — edits without `old_text` will be rejected).
- `overwrite`: replace the entire contents of an existing file (DANGEROUS)
- `append`: append content to the end of an existing file (safe for additions)

Generates a unified, line-based diff of changes using the existing `generate_diff`
helper (based on `similar::TextDiff`) and returns it as the tool output.

This file follows the project's conventions for path validation and file safety by
delegating to `file_utils::PathValidator`.
*/

use crate::error::{Result, XzatomaError};
use crate::tools::{file_utils, ToolExecutor, ToolResult};
use async_trait::async_trait;
use metrics::increment_counter;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;

/// Mode of operation for the edit_file tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    /// Perform a targeted replacement of a unique `old_text` snippet.
    /// This mode is intended for small, precise changes and REQUIRES `old_text`.
    Edit,
    /// Create a new file with the given contents (parent directories will be created).
    Create,
    /// Overwrite an existing file's contents completely.
    /// DANGEROUS: only use when you intend to replace the entire file.
    Overwrite,
    /// Append content to the end of an existing file.
    /// Safe for adding new sections without replacing existing content.
    Append,
}

/// Parameters for the edit_file tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditFileParams {
    /// Path to the file to edit (relative to working directory)
    path: String,
    /// Mode of operation: edit, create, overwrite, append
    mode: EditMode,
    /// Content to write / replace with
    content: String,
    /// Optional: the snippet of old text to find and replace (REQUIRED for edit mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    old_text: Option<String>,
}

/// Tool for intelligent editing of files
///
/// Uses path validation utilities to ensure safety, creates parent directories
/// for creation, and produces a unified diff showing the changes.
pub struct EditFileTool {
    path_validator: file_utils::PathValidator,
    max_file_size: u64,
}

impl EditFileTool {
    /// Create a new `EditFileTool`
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Base working directory for path validation and operations
    /// * `max_file_size` - Maximum allowed file size in bytes for resulting file contents
    pub fn new(working_dir: std::path::PathBuf, max_file_size: u64) -> Self {
        Self {
            path_validator: file_utils::PathValidator::new(working_dir),
            max_file_size,
        }
    }

    /// Replace the first occurrence of `old` in `haystack` with `replacement`
    ///
    /// Uses `replacen` to guarantee single replacement.
    fn replace_first(haystack: &str, old: &str, replacement: &str) -> String {
        haystack.replacen(old, replacement, 1)
    }
}

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
pub static TEST_EDIT_MISSING_OLDTEXT: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub static TEST_OLD_TEXT_NOT_FOUND: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub static TEST_OLD_TEXT_AMBIGUOUS: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub static TEST_SAFETY_BLOCK: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub static TEST_APPEND_NO_FILE: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
pub static TEST_OVERWRITE_NO_FILE: AtomicU64 = AtomicU64::new(0);

#[async_trait]
impl ToolExecutor for EditFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "edit_file",
            "description": "Edit a file with four modes: create, edit, overwrite, append. \
                            IMPORTANT: Strict mode is enabled — 'edit' now REQUIRES 'old_text' and \
                            the tool will reject edits without it (fallback behavior removed). \
                            Use 'append' to add content to the end of a file safely. \
                            Returns a unified diff of the applied changes.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file relative to the working directory"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["edit", "create", "overwrite", "append"],
                        "description": "Mode of operation"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write or the replacement text"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Snippet of old text to be replaced. REQUIRED when mode == 'edit' (strict mode; no fallback)"
                    }
                },
                "required": ["path", "mode", "content"],
                "allOf": [
                    {
                        "if": { "properties": { "mode": { "const": "edit" } } },
                        "then": { "required": ["old_text"] }
                    }
                ]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: EditFileParams = serde_json::from_value(args)?;

        // Quick guard: content size limit
        if params.content.len() as u64 > self.max_file_size {
            return Ok(ToolResult::error(format!(
                "Content size {} bytes exceeds maximum {} bytes",
                params.content.len(),
                self.max_file_size
            )));
        }

        // Validate path (will return error on traversal/absolute paths)
        let full_path = self.path_validator.validate(&params.path)?;

        match params.mode {
            EditMode::Create => {
                // Must not already exist
                if full_path.exists() {
                    return Ok(ToolResult::error(format!(
                        "Can't create file: file already exists ({})",
                        params.path
                    )));
                }

                // Ensure parents exist (creates as necessary)
                file_utils::ensure_parent_dirs(&full_path).await?;

                // Write new file
                fs::write(&full_path, &params.content)
                    .await
                    .map_err(XzatomaError::Io)?;

                // Generate diff against empty original
                let diff = crate::tools::generate_diff("", &params.content)?;

                Ok(ToolResult::success(format!(
                    "File created: {}\n\n{}",
                    params.path, diff
                )))
            }

            EditMode::Overwrite => {
                // Must exist and be a file
                if !full_path.exists() {
                    // Metrics & logging: overwrite attempted on missing file
                    let file_label = std::path::Path::new(&params.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    increment_counter!("overwrite_mode_no_file_total", "file" => file_label.clone());
                    tracing::warn!(path = %params.path, "overwrite failed: file not found");

                    #[cfg(test)]
                    {
                        TEST_OVERWRITE_NO_FILE.fetch_add(1, Ordering::SeqCst);
                    }

                    return Ok(ToolResult::error(format!(
                        "Can't overwrite file: file not found ({})",
                        params.path
                    )));
                }
                if full_path.is_dir() {
                    return Ok(ToolResult::error(format!(
                        "Can't overwrite file: path is a directory ({})",
                        params.path
                    )));
                }

                // Read current contents
                let old = fs::read_to_string(&full_path)
                    .await
                    .map_err(XzatomaError::Io)?;

                // Ensure resulting size is within bounds
                if params.content.len() as u64 > self.max_file_size {
                    return Ok(ToolResult::error(format!(
                        "Resulting content size {} bytes exceeds maximum {} bytes",
                        params.content.len(),
                        self.max_file_size
                    )));
                }

                fs::write(&full_path, &params.content)
                    .await
                    .map_err(XzatomaError::Io)?;

                let diff = crate::tools::generate_diff(&old, &params.content)?;
                Ok(ToolResult::success(format!(
                    "Overwrote {}:\n\n{}",
                    params.path, diff
                )))
            }

            EditMode::Append => {
                // Append mode: add content to end of existing file (safe operation)
                if !full_path.exists() {
                    // Metrics & logging: append attempted on missing file
                    let file_label = std::path::Path::new(&params.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    increment_counter!("append_mode_no_file_total", "file" => file_label.clone());
                    tracing::warn!(path = %params.path, "append failed: file not found; suggest 'create'");

                    #[cfg(test)]
                    {
                        TEST_APPEND_NO_FILE.fetch_add(1, Ordering::SeqCst);
                    }

                    return Ok(ToolResult::error(format!(
                        "Can't append to file: file not found ({})\n\
                         Hint: Use 'create' mode for new files.",
                        params.path
                    )));
                }
                if full_path.is_dir() {
                    return Ok(ToolResult::error(format!(
                        "Can't append to file: path is a directory ({})",
                        params.path
                    )));
                }

                let old = fs::read_to_string(&full_path)
                    .await
                    .map_err(XzatomaError::Io)?;

                // Add newline separator if original does not end with one
                let separator = if old.ends_with('\n') { "" } else { "\n" };
                let new_content = format!("{}{}{}", old, separator, params.content);

                // Size check
                if new_content.len() as u64 > self.max_file_size {
                    return Ok(ToolResult::error(format!(
                        "Resulting content size {} bytes exceeds maximum {} bytes",
                        new_content.len(),
                        self.max_file_size
                    )));
                }

                fs::write(&full_path, &new_content)
                    .await
                    .map_err(XzatomaError::Io)?;

                let diff = crate::tools::generate_diff(&old, &new_content)?;
                Ok(ToolResult::success(format!(
                    "Appended to {}:\n\n{}",
                    params.path, diff
                )))
            }

            EditMode::Edit => {
                // Must exist and be a file
                if !full_path.exists() {
                    return Ok(ToolResult::error(format!(
                        "Can't edit file: file not found ({})",
                        params.path
                    )));
                }
                if full_path.is_dir() {
                    return Ok(ToolResult::error(format!(
                        "Can't edit file: path is a directory ({})",
                        params.path
                    )));
                }

                // Strict: edit mode requires an explicit `old_text` anchor to avoid accidental whole-file replacements
                let old_text = match params.old_text.as_ref() {
                    Some(t) => t,
                    None => {
                        // Metrics & logging: count missing old_text events (use filename to avoid high cardinality)
                        let file_label = std::path::Path::new(&params.path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        increment_counter!("edit_mode_missing_oldtext_total", "file" => file_label.clone());
                        tracing::warn!(path = %params.path, "edit rejected: missing old_text; suggest 'append' or 'overwrite'");

                        #[cfg(test)]
                        {
                            TEST_EDIT_MISSING_OLDTEXT.fetch_add(1, Ordering::SeqCst);
                        }

                        return Ok(ToolResult::error(
                            "edit mode requires old_text parameter.\n\
                             To append to file, use 'append' mode.\n\
                             To replace entire file, use 'overwrite' mode explicitly.\n\
                             NOTE: Strict mode is enabled; edit will not fall back to overwriting the file."
                                .to_string(),
                        ));
                    }
                };

                let old = fs::read_to_string(&full_path)
                    .await
                    .map_err(XzatomaError::Io)?;

                // Count occurrences (non-overlapping)
                let occurrences = old.matches(old_text).count();

                if occurrences == 0 {
                    // Metrics & logging: record not-found occurrences
                    let file_label = std::path::Path::new(&params.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    increment_counter!("edit_mode_oldtext_not_found_total", "file" => file_label.clone());

                    let old_text_snippet: String = old_text.chars().take(128).collect();
                    tracing::warn!(path = %params.path, old_text = %old_text_snippet, "edit failed: old_text not found");

                    #[cfg(test)]
                    {
                        TEST_OLD_TEXT_NOT_FOUND.fetch_add(1, Ordering::SeqCst);
                    }

                    return Ok(ToolResult::error(format!(
                        "The specified old_text was not found in the file.\n\
                         Searched for: {}\n\
                         Hint: Use read_file to view current contents and find a unique anchor point.",
                        old_text
                    )));
                }
                if occurrences > 1 {
                    // Metrics & logging: ambiguous match
                    let file_label = std::path::Path::new(&params.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    increment_counter!("edit_mode_oldtext_ambiguous_total", "file" => file_label.clone(), "occurrences" => occurrences.to_string());
                    tracing::warn!(path = %params.path, occurrences = occurrences, "edit failed: old_text ambiguous");

                    #[cfg(test)]
                    {
                        TEST_OLD_TEXT_AMBIGUOUS.fetch_add(1, Ordering::SeqCst);
                    }

                    return Ok(ToolResult::error(format!(
                        "The specified old_text matches {} locations (must be unique).\n\
                         Searched for: {}\n\
                         Hint: Include more surrounding context to make the match unique.",
                        occurrences, old_text
                    )));
                }

                // Perform replacement (single occurrence)
                let new_content = Self::replace_first(&old, old_text, &params.content);

                // SAFETY CHECK: Detect dramatic file reduction (e.g., >66% reduction for larger files)
                let old_line_count = old.lines().count();
                let new_line_count = new_content.lines().count();
                if old_line_count >= 20 && new_line_count < (old_line_count / 3) {
                    // Metrics & logging: safety block due to dramatic reduction
                    let file_label = std::path::Path::new(&params.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    increment_counter!("edit_mode_safety_block_total", "file" => file_label.clone(), "old_lines" => old_line_count.to_string(), "new_lines" => new_line_count.to_string());
                    tracing::warn!(
                        "safety check blocked edit on file={}; old_lines={}, new_lines={}",
                        params.path,
                        old_line_count,
                        new_line_count
                    );

                    #[cfg(test)]
                    {
                        TEST_SAFETY_BLOCK.fetch_add(1, Ordering::SeqCst);
                    }

                    return Ok(ToolResult::error(format!(
                        "Safety check failed: This edit would reduce file from {} lines to {} lines.\n\
                         This suggests the edit may be replacing too much content.\n\
                         If you intend to replace the entire file, use 'overwrite' mode explicitly.\n\
                         Otherwise, review your old_text parameter to ensure it's specific enough.",
                        old_line_count, new_line_count
                    )));
                }

                // Size guard on resulting content
                if new_content.len() as u64 > self.max_file_size {
                    return Ok(ToolResult::error(format!(
                        "Resulting content size {} bytes exceeds maximum {} bytes",
                        new_content.len(),
                        self.max_file_size
                    )));
                }

                // If no changes, report accordingly
                if new_content == old {
                    return Ok(ToolResult::success(
                        "No changes made (contents identical)".to_string(),
                    ));
                }

                // Write new contents
                fs::write(&full_path, &new_content)
                    .await
                    .map_err(XzatomaError::Io)?;

                let diff = crate::tools::generate_diff(&old, &new_content)?;
                Ok(ToolResult::success(format!(
                    "Edited {} (replaced 1 occurrence):\n\n{}",
                    params.path, diff
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "prometheus")]
    use metrics_exporter_prometheus::PrometheusBuilder;
    use serde_json::json;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_file_success() {
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);

        let result = tool
            .execute(json!({
                "path": "new.txt",
                "mode": "create",
                "content": "hello\nworld\n"
            }))
            .await
            .unwrap();

        assert!(result.success);
        let file_path = td.path().join("new.txt");
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello\nworld\n");
        // Standard diff format has space after prefix: "+ hello"
        assert!(result.output.contains("+ hello") || result.output.contains("+hello"));
    }

    #[tokio::test]
    async fn test_create_existing_file_returns_error() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("exists.txt");
        fs::write(&file_path, "old").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "exists.txt",
                "mode": "create",
                "content": "new"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_overwrite_success() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("file.txt");
        fs::write(&file_path, "old\nline\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "file.txt",
                "mode": "overwrite",
                "content": "new\ncontent\n"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "new\ncontent\n");
        // Standard diff format: "- old" or "+ old" depending on spacing
        assert!(result.output.contains("- old") || result.output.contains("-old"));
        assert!(result.output.contains("+ new") || result.output.contains("+new"));
    }

    #[tokio::test]
    async fn test_overwrite_nonexistent_returns_error() {
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);

        let result = tool
            .execute(json!({
                "path": "nope.txt",
                "mode": "overwrite",
                "content": "hi"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_edit_replace_unique_old_text_success() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("edit.txt");
        fs::write(&file_path, "first\nTARGET_LINE\nlast\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "edit.txt",
                "mode": "edit",
                "old_text": "TARGET_LINE",
                "content": "REPLACED_LINE"
            }))
            .await
            .unwrap();

        assert!(result.success);
        let final_contents = fs::read_to_string(&file_path).unwrap();
        assert!(final_contents.contains("REPLACED_LINE"));
        // Standard diff format with space: "- TARGET_LINE"
        assert!(result.output.contains("- TARGET_LINE") || result.output.contains("-TARGET_LINE"));
        assert!(
            result.output.contains("+ REPLACED_LINE") || result.output.contains("+REPLACED_LINE")
        );
    }

    #[tokio::test]
    async fn test_edit_ambiguous_old_text_returns_error() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("amb.txt");
        fs::write(&file_path, "dup\nmatch\nmatch\nend\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "amb.txt",
                "mode": "edit",
                "old_text": "match",
                "content": "repl"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_edit_without_old_text_returns_error() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("full.txt");
        fs::write(&file_path, "original content\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "full.txt",
                "mode": "edit",
                "content": "completely new\n"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("edit mode requires old_text"));
        assert!(err.contains("append") || err.contains("overwrite"));
        assert!(err.to_lowercase().contains("strict mode"));
    }

    #[tokio::test]
    async fn test_edit_dramatic_reduction_blocked() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("big.txt");
        // Construct a file with a distinct block that will be replaced by a tiny snippet
        let mut content = String::from("header\n");
        content.push_str("BLOCKSTART\n");
        for i in 0..28 {
            content.push_str(&format!("line{}\n", i));
        }
        content.push_str("BLOCKEND\n");
        content.push_str("footer\n");
        fs::write(&file_path, &content).unwrap();

        // Build the exact old_text block we want to replace
        let mut old_text = String::from("BLOCKSTART\n");
        for i in 0..28 {
            old_text.push_str(&format!("line{}\n", i));
        }
        old_text.push_str("BLOCKEND\n");

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "big.txt",
                "mode": "edit",
                "old_text": old_text,
                "content": "tiny\n"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("Safety check failed"));
        assert!(err.contains("reduce file"));
    }

    #[tokio::test]
    async fn test_append_mode_success() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("append.txt");
        fs::write(&file_path, "start\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "append.txt",
                "mode": "append",
                "content": "end\n"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "start\nend\n");
        assert!(result.output.contains("+ end") || result.output.contains("+end"));
    }

    #[tokio::test]
    async fn test_append_adds_separator_when_needed() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("append2.txt");
        fs::write(&file_path, "start").unwrap(); // no trailing newline

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "append2.txt",
                "mode": "append",
                "content": "end\n"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "start\nend\n");
    }

    #[tokio::test]
    async fn test_helpful_error_when_old_text_not_found() {
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("hf.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({
                "path": "hf.txt",
                "mode": "edit",
                "old_text": "MISSING_SNIPPET",
                "content": "new"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("was not found"));
        assert!(err.contains("Searched for:"));
        assert!(err.contains("read_file"));
    }

    #[tokio::test]
    async fn test_invalid_path_returns_error() {
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);

        // Path traversal should produce an Err (path validation)
        let result = tool
            .execute(json!({
                "path": "../etc/passwd",
                "mode": "create",
                "content": "x"
            }))
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_tool_definition_includes_append_and_old_text_description() {
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let def = tool.tool_definition();

        // Verify 'append' is present in the mode enum
        let mode_enum = def
            .get("parameters")
            .and_then(|p| p.get("properties"))
            .and_then(|m| m.get("mode"))
            .and_then(|e| e.get("enum"))
            .and_then(|e| e.as_array())
            .expect("mode enum must be an array");
        assert!(mode_enum.iter().any(|v| v.as_str() == Some("append")));

        // Verify 'old_text' description mentions required for edit mode
        let old_text_desc = def
            .get("parameters")
            .and_then(|p| p.get("properties"))
            .and_then(|ot| ot.get("old_text"))
            .and_then(|o| o.get("description"))
            .and_then(|d| d.as_str())
            .expect("old_text description must be present");
        let old_text_desc_lower = old_text_desc.to_lowercase();
        assert!(old_text_desc_lower.contains("required"));
        assert!(old_text_desc_lower.contains("edit"));

        // Verify conditional schema enforces old_text when mode == edit
        let all_of = def
            .get("parameters")
            .and_then(|p| p.get("allOf"))
            .and_then(|a| a.as_array());
        assert!(
            all_of.is_some(),
            "parameters.allOf must be present and be an array"
        );

        let mut found = false;
        if let Some(arr) = all_of {
            for item in arr {
                if let Some(if_obj) = item.get("if") {
                    if let Some(mode_obj) = if_obj.get("properties").and_then(|pr| pr.get("mode")) {
                        if mode_obj.get("const").and_then(|c| c.as_str()) == Some("edit") {
                            if let Some(required_arr) = item
                                .get("then")
                                .and_then(|t| t.get("required"))
                                .and_then(|r| r.as_array())
                            {
                                if required_arr.iter().any(|v| v.as_str() == Some("old_text")) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(
            found,
            "Conditional schema requiring 'old_text' for edit mode not found"
        );
    }

    // ----- Metrics verification (test-only atomic counters) -----
    fn reset_test_counters() {
        TEST_EDIT_MISSING_OLDTEXT.store(0, Ordering::SeqCst);
        TEST_OLD_TEXT_NOT_FOUND.store(0, Ordering::SeqCst);
        TEST_OLD_TEXT_AMBIGUOUS.store(0, Ordering::SeqCst);
        TEST_SAFETY_BLOCK.store(0, Ordering::SeqCst);
        TEST_APPEND_NO_FILE.store(0, Ordering::SeqCst);
        TEST_OVERWRITE_NO_FILE.store(0, Ordering::SeqCst);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_edit_missing_old_text() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("full.txt");
        fs::write(&file_path, "original content\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "full.txt",
                "mode": "edit",
                "content": "completely new\n"
            }))
            .await
            .unwrap();

        assert!(TEST_EDIT_MISSING_OLDTEXT.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_old_text_not_found() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("hf.txt");
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "hf.txt",
                "mode": "edit",
                "old_text": "MISSING_SNIPPET",
                "content": "new"
            }))
            .await
            .unwrap();

        assert!(TEST_OLD_TEXT_NOT_FOUND.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_old_text_ambiguous() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("amb.txt");
        fs::write(&file_path, "dup\nmatch\nmatch\nend\n").unwrap();

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "amb.txt",
                "mode": "edit",
                "old_text": "match",
                "content": "repl"
            }))
            .await
            .unwrap();

        assert!(TEST_OLD_TEXT_AMBIGUOUS.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_edit_safety_block() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let file_path = td.path().join("big.txt");
        // Construct a file with a distinct block that will be replaced by a tiny snippet
        let mut content = String::from("header\n");
        content.push_str("BLOCKSTART\n");
        for i in 0..28 {
            content.push_str(&format!("line{}\n", i));
        }
        content.push_str("BLOCKEND\n");
        content.push_str("footer\n");
        fs::write(&file_path, &content).unwrap();

        // Build the exact old_text block we want to replace
        let mut old_text = String::from("BLOCKSTART\n");
        for i in 0..28 {
            old_text.push_str(&format!("line{}\n", i));
        }
        old_text.push_str("BLOCKEND\n");

        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "big.txt",
                "mode": "edit",
                "old_text": old_text,
                "content": "tiny\n"
            }))
            .await
            .unwrap();

        assert!(TEST_SAFETY_BLOCK.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_append_missing_file() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "nope.txt",
                "mode": "append",
                "content": "hi"
            }))
            .await
            .unwrap();

        assert!(TEST_APPEND_NO_FILE.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_metrics_atomic_increment_on_overwrite_missing_file() {
        reset_test_counters();
        let td = TempDir::new().unwrap();
        let tool = EditFileTool::new(td.path().to_path_buf(), 1024 * 1024);
        let _ = tool
            .execute(json!({
                "path": "nope2.txt",
                "mode": "overwrite",
                "content": "hi"
            }))
            .await
            .unwrap();

        assert!(TEST_OVERWRITE_NO_FILE.load(Ordering::SeqCst) >= 1);
    }
}
