//! Create directory tool implementation
//!
//! Creates directories with automatic parent directory creation.

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_CREATE_DIRECTORY};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Create directory tool for creating directories
///
/// Creates a directory at the specified path. Automatically creates
/// parent directories as needed.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::create_directory::CreateDirectoryTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let tool = CreateDirectoryTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "path": "src/new_module"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct CreateDirectoryTool {
    path_validator: PathValidator,
}

impl CreateDirectoryTool {
    /// Creates a new create directory tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The working directory for path validation
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::create_directory::CreateDirectoryTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = CreateDirectoryTool::new(PathBuf::from("/project"));
    /// assert_eq!(tool.working_dir().to_str().unwrap(), "/project");
    /// ```
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
        }
    }

    /// Returns the working directory
    pub fn working_dir(&self) -> &Path {
        self.path_validator.working_dir()
    }
}

#[derive(Debug, Deserialize)]
struct CreateDirectoryParams {
    path: String,
}

#[async_trait]
impl ToolExecutor for CreateDirectoryTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_CREATE_DIRECTORY,
            "description": "Creates a directory at the specified path. Creates parent directories automatically if they don't exist.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the directory to create"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: CreateDirectoryParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let dir_path = match self.path_validator.validate(&params.path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid path: {}", e))),
        };

        if dir_path.exists() {
            if dir_path.is_dir() {
                return Ok(ToolResult::success(format!(
                    "Directory already exists: {}",
                    params.path
                )));
            } else {
                return Ok(ToolResult::error(format!(
                    "Path exists but is not a directory: {}",
                    params.path
                )));
            }
        }

        match tokio::fs::create_dir_all(&dir_path).await {
            Ok(_) => Ok(ToolResult::success(format!(
                "Created directory: {}",
                params.path
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to create directory: {}",
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_creates_single_directory() {
        let temp = TempDir::new().unwrap();

        let tool = CreateDirectoryTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": "newdir"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp.path().join("newdir").is_dir());
    }

    #[tokio::test]
    async fn test_execute_creates_nested_directories() {
        let temp = TempDir::new().unwrap();

        let tool = CreateDirectoryTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": "a/b/c/d"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp.path().join("a/b/c/d").is_dir());
    }

    #[tokio::test]
    async fn test_execute_with_existing_directory_returns_success() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir(temp.path().join("existing"))
            .await
            .unwrap();

        let tool = CreateDirectoryTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": "existing"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("already exists"));
    }

    #[tokio::test]
    async fn test_execute_with_file_path_returns_error() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file.txt"), "content")
            .await
            .unwrap();

        let tool = CreateDirectoryTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": "file.txt"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not a directory"));
    }

    #[tokio::test]
    async fn test_execute_with_deep_nesting() {
        let temp = TempDir::new().unwrap();

        let tool = CreateDirectoryTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "path": "very/deep/nested/directory/structure/here"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp
            .path()
            .join("very/deep/nested/directory/structure/here")
            .is_dir());
    }
}
