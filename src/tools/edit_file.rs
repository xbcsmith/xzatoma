/*!
edit_file tool for XZatoma

Provides an intelligent editing tool that supports three modes:
- `create`: create a new file (creates parent directories as needed)
- `edit`: make a targeted edit (replace a single matching snippet) or replace whole file if `old_text` omitted
- `overwrite`: replace the entire contents of an existing file

Generates a unified, line-based diff of changes using the existing `generate_diff`
helper (based on `similar::TextDiff`) and returns it as the tool output.

This file follows the project's conventions for path validation and file safety by
delegating to `file_utils::PathValidator`.
*/

use crate::error::{Result, XzatomaError};
use crate::tools::{file_utils, ToolExecutor, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use tokio::fs;

/// Mode of operation for the edit_file tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EditMode {
    /// Make a targeted or whole-file edit to an existing file.
    Edit,
    /// Create a new file with the given contents (parent directories will be created).
    Create,
    /// Overwrite an existing file's contents completely.
    Overwrite,
}

/// Parameters for the edit_file tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditFileParams {
    /// Path to the file to edit (relative to working directory)
    path: String,
    /// Mode of operation: edit, create, overwrite
    mode: EditMode,
    /// Content to write / replace with
    content: String,
    /// Optional: the snippet of old text to find and replace (used for targeted edits)
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

#[async_trait]
impl ToolExecutor for EditFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "edit_file",
            "description": "Edit a file with three modes: create, edit, overwrite. \
                            For 'edit' mode provide an optional 'old_text' to replace a specific snippet. \
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
                        "enum": ["edit", "create", "overwrite"],
                        "description": "Mode of operation"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write or the replacement text"
                    },
                    "old_text": {
                        "type": "string",
                        "description": "Optional snippet of old text to be replaced (required for targeted edits)"
                    }
                },
                "required": ["path", "mode", "content"]
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

                let old = fs::read_to_string(&full_path)
                    .await
                    .map_err(XzatomaError::Io)?;

                let new_content = if let Some(ref old_text) = params.old_text {
                    // Targeted snippet replacement

                    // Count occurrences (non-overlapping)
                    let occurrences = old.matches(old_text).count();

                    if occurrences == 0 {
                        return Ok(ToolResult::error(
                            "The specified old_text was not found in the file.".to_string(),
                        ));
                    }
                    if occurrences > 1 {
                        return Ok(ToolResult::error(format!(
                            "The specified old_text matches more than one location ({} matches). \
                             Make it more specific so that only a single replacement is possible.",
                            occurrences
                        )));
                    }

                    let replaced = Self::replace_first(&old, old_text, &params.content);

                    // Size guard on resulting content
                    if replaced.len() as u64 > self.max_file_size {
                        return Ok(ToolResult::error(format!(
                            "Resulting content size {} bytes exceeds maximum {} bytes",
                            replaced.len(),
                            self.max_file_size
                        )));
                    }

                    replaced
                } else {
                    // No old_text provided -> fall back to whole-file replace (overwrite-like)
                    if params.content.len() as u64 > self.max_file_size {
                        return Ok(ToolResult::error(format!(
                            "Resulting content size {} bytes exceeds maximum {} bytes",
                            params.content.len(),
                            self.max_file_size
                        )));
                    }
                    params.content.clone()
                };

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
                    "Edited {}:\n\n{}",
                    params.path, diff
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
    async fn test_edit_without_old_text_behaves_like_overwrite() {
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

        assert!(result.success);
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "completely new\n");
        // Standard diff format with space: "- original"
        assert!(result.output.contains("- original") || result.output.contains("-original"));
        assert!(
            result.output.contains("+ completely new") || result.output.contains("+completely new")
        );
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
}
