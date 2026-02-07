//! Move path tool implementation
//!
//! Moves or renames files and directories with cross-filesystem fallback.

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_MOVE_PATH};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Move path tool for moving or renaming files and directories
///
/// Moves or renames a file or directory. If moving across filesystems,
/// uses copy + delete as fallback. Creates parent directories automatically.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::move_path::MovePathTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// # tokio::spawn(async {
/// let tool = MovePathTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "source_path": "old.rs",
///     "destination_path": "new.rs"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct MovePathTool {
    path_validator: PathValidator,
}

impl MovePathTool {
    /// Creates a new move path tool
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
struct MovePathParams {
    source_path: String,
    destination_path: String,
}

#[async_trait]
impl ToolExecutor for MovePathTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_MOVE_PATH,
            "description": "Moves or renames a file or directory. Creates parent directories automatically. Falls back to copy+delete for cross-filesystem moves.",
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
                    }
                },
                "required": ["source_path", "destination_path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: MovePathParams = serde_json::from_value(args)
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

        if destination.exists() {
            return Ok(ToolResult::error(format!(
                "Destination already exists: {}",
                params.destination_path
            )));
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

        // Try direct rename first
        if tokio::fs::rename(&source, &destination).await.is_ok() {
            return Ok(ToolResult::success(format!(
                "Moved {} to {}",
                params.source_path, params.destination_path
            )));
        }

        // Fallback: copy + delete for cross-filesystem moves
        if source.is_file() {
            match tokio::fs::copy(&source, &destination).await {
                Ok(bytes) => {
                    if let Err(e) = tokio::fs::remove_file(&source).await {
                        return Ok(ToolResult::error(format!(
                            "Copied file but failed to remove source: {}",
                            e
                        )));
                    }
                    Ok(ToolResult::success(format!(
                        "Moved {} bytes from {} to {} (via copy+delete)",
                        bytes, params.source_path, params.destination_path
                    )))
                }
                Err(e) => Ok(ToolResult::error(format!("Failed to move file: {}", e))),
            }
        } else if source.is_dir() {
            match self.copy_directory_recursive(&source, &destination).await {
                Ok(count) => {
                    if let Err(e) = tokio::fs::remove_dir_all(&source).await {
                        return Ok(ToolResult::error(format!(
                            "Copied directory but failed to remove source: {}",
                            e
                        )));
                    }
                    Ok(ToolResult::success(format!(
                        "Moved directory with {} items from {} to {} (via copy+delete)",
                        count, params.source_path, params.destination_path
                    )))
                }
                Err(e) => Ok(ToolResult::error(format!(
                    "Failed to move directory: {}",
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

impl MovePathTool {
    /// Recursively copies a directory for cross-filesystem moves
    async fn copy_directory_recursive(&self, source: &Path, destination: &Path) -> Result<usize> {
        use walkdir::WalkDir;

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
    async fn test_execute_with_file_moves_successfully() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "content")
            .await
            .unwrap();

        let tool = MovePathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "dest.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!temp.path().join("source.txt").exists());
        assert!(temp.path().join("dest.txt").exists());

        let content = tokio::fs::read_to_string(temp.path().join("dest.txt"))
            .await
            .unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_execute_with_directory_moves_recursively() {
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

        let tool = MovePathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source",
                "destination_path": "dest"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!temp.path().join("source").exists());
        assert!(temp.path().join("dest/file.txt").exists());
        assert!(temp.path().join("dest/subdir/nested.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_nested_destination_creates_parent_directories() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "content")
            .await
            .unwrap();

        let tool = MovePathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "deep/nested/path/dest.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(!temp.path().join("source.txt").exists());
        assert!(temp.path().join("deep/nested/path/dest.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_missing_source_returns_error() {
        let temp = TempDir::new().unwrap();

        let tool = MovePathTool::new(temp.path().to_path_buf());
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

    #[tokio::test]
    async fn test_execute_with_existing_destination_returns_error() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "source")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("dest.txt"), "dest")
            .await
            .unwrap();

        let tool = MovePathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "source_path": "source.txt",
                "destination_path": "dest.txt"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("already exists"));
    }
}
