//! write_file tool for writing content to files
//!
//! Provides a tool to write or overwrite file contents with automatic parent directory creation.

use crate::error::{Result, XzatomaError};
use crate::tools::{file_metadata, file_utils, ToolExecutor, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Parameters for the write_file tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteFileParams {
    /// Path to the file to write
    path: String,
    /// Content to write to the file
    content: String,
}

/// Tool for writing content to files
///
/// Writes or overwrites file contents with automatic parent directory creation.
/// Ensures path safety through validation before writing.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::write_file::WriteFileTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// let tool = WriteFileTool::new(PathBuf::from("/project"), 10 * 1024 * 1024);
/// // Execute tool with JSON parameters
/// # use serde_json::json;
/// # tokio_test::block_on(async {
/// # let result = tool.execute(json!({"path": "output.txt", "content": "hello"})).await;
/// # });
/// ```
pub struct WriteFileTool {
    path_validator: file_utils::PathValidator,
    max_file_size: u64,
}

impl WriteFileTool {
    /// Creates a new WriteFileTool instance
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Base directory for path validation
    /// * `max_file_size` - Maximum file size allowed in bytes
    ///
    /// # Returns
    ///
    /// Returns a new WriteFileTool
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::write_file::WriteFileTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = WriteFileTool::new(
    ///     PathBuf::from("/project"),
    ///     10 * 1024 * 1024  // 10 MB
    /// );
    /// ```
    pub fn new(working_dir: PathBuf, max_file_size: u64) -> Self {
        Self {
            path_validator: file_utils::PathValidator::new(working_dir),
            max_file_size,
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for WriteFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "write_file",
            "description": "Write content to a file. Creates parent directories as needed. Overwrites existing files.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to write (relative to project root)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: WriteFileParams = serde_json::from_value(args)?;

        // Check content size before validation
        if params.content.len() as u64 > self.max_file_size {
            return Ok(ToolResult::error(format!(
                "Content size {} bytes exceeds maximum {} bytes",
                params.content.len(),
                self.max_file_size
            )));
        }

        // Validate path
        let path = self.path_validator.validate(&params.path)?;

        // Check if path exists and is a directory
        if path.exists() && path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path is a directory, not a file: {}",
                params.path
            )));
        }

        // Ensure parent directories exist
        file_utils::ensure_parent_dirs(&path).await?;

        // Write content to file
        tokio::fs::write(&path, &params.content)
            .await
            .map_err(XzatomaError::Io)?;

        Ok(ToolResult::success(format!(
            "File written successfully: {} ({} bytes)",
            params.path,
            params.content.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_new_file_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WriteFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024);

        let result = tool
            .execute(json!({"path": "new_file.txt", "content": "test content"}))
            .await
            .unwrap();

        assert!(result.success);
        let file_path = temp_dir.path().join("new_file.txt");
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "test content");
    }

    #[tokio::test]
    async fn test_execute_with_existing_file_overwrites() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "old content").unwrap();

        let tool = WriteFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024);
        let result = tool
            .execute(json!({"path": "test.txt", "content": "new content"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(fs::read_to_string(&test_file).unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_execute_with_nested_path_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WriteFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024);

        let result = tool
            .execute(json!({"path": "nested/deep/file.txt", "content": "content"}))
            .await
            .unwrap();

        assert!(result.success);
        let file_path = temp_dir.path().join("nested/deep/file.txt");
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "content");
    }

    #[tokio::test]
    async fn test_execute_with_invalid_path_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let tool = WriteFileTool::new(temp_dir.path().to_path_buf(), 1024 * 1024);

        let result = tool
            .execute(json!({"path": "../etc/passwd", "content": "content"}))
            .await;

        assert!(result.is_err());
    }
}
