//! Copy path tool implementation
//!
//! Supports recursive directory copying with overwrite control.

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_COPY_PATH};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Copy path tool for copying files and directories
///
/// Supports recursive directory copying with overwrite control.
/// Creates destination parent directories automatically.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::copy_path::CopyPathTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let tool = CopyPathTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "source_path": "src/old.rs",
///     "destination_path": "src/new.rs"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct CopyPathTool {
    path_validator: PathValidator,
}

impl CopyPathTool {
    /// Creates a new copy path tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The working directory for path validation
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
struct CopyPathParams {
    source_path: String,
    destination_path: String,
    #[serde(default)]
    overwrite: bool,
}

#[async_trait]
impl ToolExecutor for CopyPathTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_COPY_PATH,
            "description": "Copies a file or directory (recursively). Creates destination parent directories automatically.",
            "parameters": {
                "type": "object",
                "properties": {
                    "source_path": {
                        "type": "string",
                        "description": "Relative path to the source file or directory"
                    },
                    "destination_path": {
                        "type": "string",
                        "description": "Relative path to the destination"
                    },
                    "overwrite": {
                        "type": "boolean",
                        "description": "Whether to overwrite existing destination (default: false)",
                        "default": false
                    }
                },
                "required": ["source_path", "destination_path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: CopyPathParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let source = match self.path_validator.validate(&params.source_path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid source path: {}", e))),
        };

        let destination = match self.path_validator.validate(&params.destination_path) {
            Ok(path) => path,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Invalid destination path: {}",
                    e
                )))
            }
        };

        if !source.exists() {
            return Ok(ToolResult::error(format!(
                "Source not found: {}",
                params.source_path
            )));
        }

        if destination.exists() && !params.overwrite {
            return Ok(ToolResult::error(format!(
                "Destination already exists: {}. Set overwrite=true to replace.",
                params.destination_path
            )));
        }

        // Remove existing destination if overwrite is true
        if destination.exists() && params.overwrite {
            if destination.is_file() {
                if let Err(e) = tokio::fs::remove_file(&destination).await {
                    return Ok(ToolResult::error(format!(
                        "Failed to remove existing file: {}",
                        e
                    )));
                }
            } else if destination.is_dir() {
                if let Err(e) = tokio::fs::remove_dir_all(&destination).await {
                    return Ok(ToolResult::error(format!(
                        "Failed to remove existing directory: {}",
                        e
                    )));
                }
            }
        }

        // Create destination parent directories
        if let Some(parent) = destination.parent() {
            if !parent.exists() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return Ok(ToolResult::error(format!(
                        "Failed to create parent directories: {}",
                        e
                    )));
                }
            }
        }

        if source.is_file() {
            match tokio::fs::copy(&source, &destination).await {
                Ok(bytes) => Ok(ToolResult::success(format!(
                    "Copied {} bytes from {} to {}",
                    bytes, params.source_path, params.destination_path
                ))),
                Err(e) => Ok(ToolResult::error(format!("Failed to copy file: {}", e))),
            }
        } else if source.is_dir() {
            match self.copy_directory(&source, &destination).await {
                Ok(count) => Ok(ToolResult::success(format!(
                    "Copied directory with {} items from {} to {}",
                    count, params.source_path, params.destination_path
                ))),
                Err(e) => Ok(ToolResult::error(format!(
                    "Failed to copy directory: {}",
                    e
                ))),
            }
        } else {
            Ok(ToolResult::error(format!(
                "Unsupported file type: {}",
                params.source_path
            )))
        }
    }
}

impl CopyPathTool {
    /// Recursively copies a directory from source to destination
    async fn copy_directory(&self, source: &Path, destination: &Path) -> Result<usize> {
        let mut count = 0;

        for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
            let rel_path = entry.path().strip_prefix(source)?;
            let dest_path = destination.join(rel_path);

            if entry.file_type().is_dir() {
                tokio::fs::create_dir_all(&dest_path).await?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = dest_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::copy(entry.path(), &dest_path).await?;
                count += 1;
            }
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_file_copies_successfully() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "content")
            .await
            .unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "dest.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp.path().join("dest.txt").exists());

        let content = tokio::fs::read_to_string(temp.path().join("dest.txt"))
            .await
            .unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_execute_with_directory_copies_recursively() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir(temp.path().join("source"))
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("source/file.txt"), "content")
            .await
            .unwrap();
        tokio::fs::create_dir(temp.path().join("source/subdir"))
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("source/subdir/nested.txt"), "nested")
            .await
            .unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source",
                "destination_path": "dest"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp.path().join("dest/file.txt").exists());
        assert!(temp.path().join("dest/subdir/nested.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_existing_destination_and_overwrite_false_returns_error() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "source")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("dest.txt"), "dest")
            .await
            .unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "dest.txt",
                "overwrite": false
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("overwrite=true"));
    }

    #[tokio::test]
    async fn test_execute_with_existing_destination_and_overwrite_true_succeeds() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "new")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("dest.txt"), "old")
            .await
            .unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "dest.txt",
                "overwrite": true
            }))
            .await
            .unwrap();

        assert!(result.success);

        let content = tokio::fs::read_to_string(temp.path().join("dest.txt"))
            .await
            .unwrap();
        assert_eq!(content, "new");
    }

    #[tokio::test]
    async fn test_execute_with_nested_destination_creates_parent_directories() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "content")
            .await
            .unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "deep/nested/path/dest.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(temp.path().join("deep/nested/path/dest.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_missing_source_returns_error() {
        let temp = TempDir::new().unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "missing.txt",
                "destination_path": "dest.txt"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Source not found"));
    }
}
