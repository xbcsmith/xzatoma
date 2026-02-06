//! list_directory tool for listing directory contents
//!
//! Provides a tool to list directory contents with optional recursive listing and pattern filtering.

use crate::error::{Result, XzatomaError};
use crate::tools::{file_metadata, file_utils, ToolExecutor, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Simple glob pattern matching
///
/// Supports * (match any number of characters) and ? (match single character)
fn glob_match(text: &str, pattern: &str) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();
    glob_match_recursive(&text_chars, 0, &pattern_chars, 0)
}

/// Recursive helper for glob matching
#[allow(clippy::only_used_in_recursion)]
fn glob_match_recursive(
    text: &[char],
    text_idx: usize,
    pattern: &[char],
    pattern_idx: usize,
) -> bool {
    // Both exhausted - match
    if text_idx == text.len() && pattern_idx == pattern.len() {
        return true;
    }

    // Pattern exhausted but text remains - no match
    if pattern_idx == pattern.len() {
        return text_idx == text.len();
    }

    let current_pattern = pattern[pattern_idx];

    match current_pattern {
        '*' => {
            // * can match zero characters
            if glob_match_recursive(text, text_idx, pattern, pattern_idx + 1) {
                return true;
            }
            // Or match one character and recurse
            if text_idx < text.len() {
                return glob_match_recursive(text, text_idx + 1, pattern, pattern_idx);
            }
            false
        }
        '?' => {
            // ? matches exactly one character
            if text_idx < text.len() {
                glob_match_recursive(text, text_idx + 1, pattern, pattern_idx + 1)
            } else {
                false
            }
        }
        c => {
            // Exact character match
            if text_idx < text.len() && text[text_idx] == c {
                glob_match_recursive(text, text_idx + 1, pattern, pattern_idx + 1)
            } else {
                false
            }
        }
    }
}

/// Parameters for the list_directory tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ListDirectoryParams {
    /// Path to the directory to list
    path: String,
    /// Whether to recursively list subdirectories
    #[serde(default)]
    recursive: bool,
    /// Optional glob pattern to filter results
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
}

/// Tool for listing directory contents
///
/// Lists files and directories with optional recursive traversal and pattern filtering.
/// Provides file metadata for each entry (size, type, modification time).
///
/// # Examples
///
/// ```
/// use xzatoma::tools::list_directory::ListDirectoryTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// let tool = ListDirectoryTool::new(PathBuf::from("/project"));
/// // Execute tool with JSON parameters
/// # use serde_json::json;
/// # tokio_test::block_on(async {
/// # let result = tool.execute(json!({"path": "src", "recursive": false})).await;
/// # });
/// ```
pub struct ListDirectoryTool {
    path_validator: file_utils::PathValidator,
}

impl ListDirectoryTool {
    /// Creates a new ListDirectoryTool instance
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Base directory for path validation
    ///
    /// # Returns
    ///
    /// Returns a new ListDirectoryTool
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::list_directory::ListDirectoryTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = ListDirectoryTool::new(PathBuf::from("/project"));
    /// ```
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: file_utils::PathValidator::new(working_dir),
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ListDirectoryTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "list_directory",
            "description": "List contents of a directory with optional recursive listing and pattern filtering. Shows file sizes, types, and modification times.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory to list (relative to project root)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to recursively list subdirectories (default: false)"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Optional glob pattern to filter results (e.g., '*.rs' for Rust files)"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: ListDirectoryParams = serde_json::from_value(args)?;

        // Validate path
        let path = self.path_validator.validate(&params.path)?;

        // Check if path exists
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "Directory not found: {}",
                params.path
            )));
        }

        // Check if it's a directory
        if !path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path is not a directory: {}",
                params.path
            )));
        }

        // Build listing
        let mut entries = Vec::new();
        let max_depth = if params.recursive { usize::MAX } else { 1 };

        let walker = WalkDir::new(&path)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            // Skip the root directory itself
            if entry.path() == path {
                continue;
            }

            // Apply pattern filter if provided
            if let Some(ref pattern) = params.pattern {
                let file_name = entry.file_name().to_string_lossy();
                if !glob_match(&file_name, pattern) {
                    continue;
                }
            }

            // Get file metadata
            let metadata = match tokio::fs::metadata(entry.path()).await {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_type = if entry.path().is_dir() {
                "dir"
            } else if entry.path().is_symlink() {
                "symlink"
            } else {
                "file"
            };

            let size = metadata.len();
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                })
                .unwrap_or(0);

            // Construct relative path
            let rel_path = entry
                .path()
                .strip_prefix(&path)
                .unwrap_or(entry.path())
                .to_string_lossy();

            entries.push(format!("{} {} {} {}", rel_path, file_type, size, modified));
        }

        if entries.is_empty() {
            return Ok(ToolResult::success(format!(
                "Directory is empty: {}",
                params.path
            )));
        }

        let output = entries.join("\n");
        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_directory_lists_contents() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();

        let tool = ListDirectoryTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": ".", "recursive": false}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
    }

    #[tokio::test]
    async fn test_execute_with_recursive_lists_nested_files() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir/file2.txt"), "content").unwrap();

        let tool = ListDirectoryTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": ".", "recursive": true}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(result.output.contains("subdir"));
    }

    #[tokio::test]
    async fn test_execute_with_pattern_filters_results() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        fs::write(temp_dir.path().join("file2.rs"), "content").unwrap();
        fs::write(temp_dir.path().join("file3.txt"), "content").unwrap();

        let tool = ListDirectoryTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": ".", "recursive": false, "pattern": "*.txt"}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file3.txt"));
        assert!(!result.output.contains("file2.rs"));
    }

    #[tokio::test]
    async fn test_execute_with_empty_directory_returns_empty_message() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ListDirectoryTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": ".", "recursive": false}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("Directory is empty"));
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_directory_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ListDirectoryTool::new(temp_dir.path().to_path_buf());

        let result = tool
            .execute(json!({"path": "nonexistent", "recursive": false}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
