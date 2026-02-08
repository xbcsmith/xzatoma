//! delete_path tool for deleting files and directories
//!
//! Provides a tool to delete files and directories with optional recursive deletion.

use crate::error::{Result, XzatomaError};
use crate::tools::{file_utils, ToolExecutor, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// Parameters for the delete_path tool
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeletePathParams {
    /// Path to the file or directory to delete
    path: String,
    /// Whether to recursively delete directories
    #[serde(default)]
    recursive: bool,
}

/// Tool for deleting files and directories
///
/// Deletes files or directories with optional recursive deletion for directories.
/// Ensures path safety through validation before deletion.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::delete_path::DeletePathTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// let tool = DeletePathTool::new(PathBuf::from("/project"));
/// // Execute tool with JSON parameters
/// # use serde_json::json;
/// # tokio_test::block_on(async {
/// # let result = tool.execute(json!({"path": "temp.txt", "recursive": false})).await;
/// # });
/// ```
pub struct DeletePathTool {
    path_validator: file_utils::PathValidator,
}

impl DeletePathTool {
    /// Creates a new DeletePathTool instance
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Base directory for path validation
    ///
    /// # Returns
    ///
    /// Returns a new DeletePathTool
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::delete_path::DeletePathTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = DeletePathTool::new(PathBuf::from("/project"));
    /// ```
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: file_utils::PathValidator::new(working_dir),
        }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for DeletePathTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "delete_path",
            "description": "Delete a file or directory. For directories, use recursive=true to delete contents.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file or directory to delete (relative to project root)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to recursively delete directories and their contents (default: false)"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: DeletePathParams = serde_json::from_value(args)?;

        // Validate path
        let path = self.path_validator.validate(&params.path)?;

        // Check if path exists
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "Path not found: {}",
                params.path
            )));
        }

        // Handle directory deletion
        if path.is_dir() {
            if !params.recursive {
                return Ok(ToolResult::error(format!(
                    "Path is a directory. Use recursive=true to delete: {}",
                    params.path
                )));
            }

            // Recursively remove directory
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(XzatomaError::Io)?;

            return Ok(ToolResult::success(format!(
                "Directory deleted successfully: {}",
                params.path
            )));
        }

        // Delete file
        tokio::fs::remove_file(&path)
            .await
            .map_err(XzatomaError::Io)?;

        Ok(ToolResult::success(format!(
            "File deleted successfully: {}",
            params.path
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_existing_file_deletes_successfully() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "content").unwrap();

        let tool = DeletePathTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": "test.txt", "recursive": false}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!test_file.exists());
    }

    #[tokio::test]
    async fn test_execute_with_directory_and_recursive_false_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir(&test_dir).unwrap();

        let tool = DeletePathTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": "test_dir", "recursive": false}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(test_dir.exists());
    }

    #[tokio::test]
    async fn test_execute_with_directory_and_recursive_true_deletes_tree() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test_dir");
        fs::create_dir(&test_dir).unwrap();
        fs::write(test_dir.join("file.txt"), "content").unwrap();

        let tool = DeletePathTool::new(temp_dir.path().to_path_buf());
        let result = tool
            .execute(json!({"path": "test_dir", "recursive": true}))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!test_dir.exists());
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_path_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DeletePathTool::new(temp_dir.path().to_path_buf());

        let result = tool
            .execute(json!({"path": "nonexistent.txt", "recursive": false}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
