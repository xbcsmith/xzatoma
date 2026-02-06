//! read_file tool for reading file contents
//!
//! Provides a tool to read file contents with optional line range support.
//! Handles image files by returning outline information instead of raw content.

use crate::error::{Result, XzatomaError};
use crate::tools::{file_metadata, file_utils, ToolExecutor, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Parameters for the read_file tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReadFileParams {
    /// Path to the file to read
    path: String,
    /// Optional starting line number (1-based index)
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    /// Optional ending line number (1-based index, inclusive)
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<u32>,
}

/// Tool for reading file contents
///
/// Reads files with support for line range specification.
/// For large files, generates an outline instead of loading full content.
/// For image files, returns file information instead of raw content.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::read_file::ReadFileTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// let tool = ReadFileTool::new(PathBuf::from("/project"), 1024 * 1024, 1000);
/// // Execute tool with JSON parameters
/// # use serde_json::json;
/// # tokio_test::block_on(async {
/// # let result = tool.execute(json!({"path": "src/main.rs"})).await;
/// # });
/// ```
pub struct ReadFileTool {
    path_validator: file_utils::PathValidator,
    max_file_size: u64,
    max_outline_lines: u32,
}

impl ReadFileTool {
    /// Creates a new ReadFileTool instance
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Base directory for path validation
    /// * `max_file_size` - Maximum file size to read in bytes
    /// * `max_outline_lines` - Maximum lines before showing outline instead
    ///
    /// # Returns
    ///
    /// Returns a new ReadFileTool
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::read_file::ReadFileTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = ReadFileTool::new(
    ///     PathBuf::from("/project"),
    ///     10 * 1024 * 1024,  // 10 MB
    ///     1000                // outline for files > 1000 lines
    /// );
    /// ```
    pub fn new(working_dir: PathBuf, max_file_size: u64, max_outline_lines: u32) -> Self {
        Self {
            path_validator: file_utils::PathValidator::new(working_dir),
            max_file_size,
            max_outline_lines,
        }
    }

    /// Generates an outline for a file showing structure
    ///
    /// Returns a summary of the file structure with symbols and line numbers
    /// for large files that exceed the line threshold.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    /// * `content` - File contents
    ///
    /// # Returns
    ///
    /// Returns outline string or error
    async fn generate_outline(&self, path: &Path, content: &str) -> Result<String> {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as u32;

        // If file is small enough, return as outline format
        if total_lines <= self.max_outline_lines {
            return Ok(format!(
                "# File outline for {}\n\n{}",
                path.display(),
                content
            ));
        }

        // For large files, show first 50 and last 50 lines with indication
        let mut outline = format!("# File outline for {}\n\n", path.display());
        outline.push_str("Showing first 50 and last 50 lines (file has ");
        outline.push_str(&total_lines.to_string());
        outline.push_str(" total lines)\n\n");

        for (i, line) in lines.iter().take(50).enumerate() {
            outline.push_str(&format!("{} {}\n", i + 1, line));
        }

        outline.push_str("\n... (middle section omitted) ...\n\n");
        outline.push_str("Last 50 lines:\n\n");

        let start_idx = if total_lines > 50 {
            (total_lines - 50) as usize
        } else {
            0
        };
        for (i, line) in lines.iter().skip(start_idx).enumerate() {
            outline.push_str(&format!("{} {}\n", start_idx as u32 + i as u32 + 1, line));
        }

        Ok(outline)
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ReadFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "read_file",
            "description": "Read the contents of a file. For large files, shows outline with first and last 50 lines. For image files, shows file information instead of raw content. Supports optional line range specification.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read (relative to project root)"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional starting line number (1-based index)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional ending line number (1-based index, inclusive)"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: ReadFileParams = serde_json::from_value(args)?;

        // Validate path
        let path = self.path_validator.validate(&params.path)?;

        // Check if file exists
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                params.path
            )));
        }

        // Check if it's a directory
        if path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path is a directory, not a file: {}",
                params.path
            )));
        }

        // Check file size
        file_utils::check_file_size(&path, self.max_file_size).await?;

        // Check if it's an image
        if file_metadata::is_image_file(&path) {
            // For images, return file info instead of content
            match file_metadata::get_file_info(&path).await {
                Ok(info) => {
                    return Ok(ToolResult::success(format!(
                        "Image file: {}\nSize: {} bytes\nModified: {:?}\nRead-only: {}",
                        params.path, info.size, info.modified, info.readonly
                    )));
                }
                Err(e) => {
                    return Ok(ToolResult::error(format!(
                        "Failed to read image metadata: {}",
                        e
                    )));
                }
            }
        }

        // Read file content
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(XzatomaError::Io)?;

        // Handle line range if specified
        if let (Some(start_line), Some(end_line)) = (params.start_line, params.end_line) {
            if start_line == 0 || end_line == 0 {
                return Ok(ToolResult::error(
                    "Line numbers must be greater than 0 (1-based index)".to_string(),
                ));
            }
            if start_line > end_line {
                return Ok(ToolResult::error(
                    "start_line must be less than or equal to end_line".to_string(),
                ));
            }

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len() as u32;

            if start_line > total_lines {
                return Ok(ToolResult::error(format!(
                    "start_line {} exceeds file length of {} lines",
                    start_line, total_lines
                )));
            }

            let actual_end = std::cmp::min(end_line, total_lines);
            let selected_lines = lines
                .iter()
                .skip((start_line - 1) as usize)
                .take((actual_end - start_line + 1) as usize)
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join("\n");

            return Ok(ToolResult::success(selected_lines));
        }

        // Check if file is too large for full content
        if (content.lines().count() as u32) > self.max_outline_lines {
            let outline = self.generate_outline(&path, &content).await?;
            return Ok(ToolResult::success(outline)
                .with_metadata("outline".to_string(), "true".to_string()));
        }

        Ok(ToolResult::success(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_valid_text_file_returns_content() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let test_content = "line1\nline2\nline3";
        fs::write(&test_file, test_content).unwrap();

        let tool = ReadFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024, 100);
        let result = tool.execute(json!({"path": "test.txt"})).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, test_content);
    }

    #[tokio::test]
    async fn test_execute_with_line_range_returns_specified_lines() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let test_content = "line1\nline2\nline3\nline4\nline5";
        fs::write(&test_file, test_content).unwrap();

        let tool = ReadFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024, 100);
        let result = tool
            .execute(json!({"path": "test.txt", "start_line": 2, "end_line": 4}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "line2\nline3\nline4");
    }

    #[tokio::test]
    async fn test_execute_with_invalid_line_range_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").unwrap();

        let tool = ReadFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024, 100);
        let result = tool
            .execute(json!({"path": "test.txt", "start_line": 5, "end_line": 10}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_execute_with_missing_file_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ReadFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024, 100);
        let result = tool
            .execute(json!({"path": "nonexistent.txt"}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_execute_with_traversal_attempt_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ReadFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024, 100);
        let result = tool.execute(json!({"path": "../etc/passwd"})).await;

        assert!(result.is_err());
    }
}
