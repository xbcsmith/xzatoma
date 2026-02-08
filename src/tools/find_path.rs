//! Find path tool implementation
//!
//! Searches for files matching glob patterns with pagination support.

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_FIND_PATH};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const DEFAULT_PAGE_SIZE: usize = 50;

/// Find path tool for searching files
///
/// Searches for files matching glob patterns. Supports pagination
/// with configurable page size and offset.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::find_path::FindPathTool;
/// use xzatoma::tools::ToolExecutor;
/// use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let tool = FindPathTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "glob": "**/*.rs",
///     "offset": 0
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct FindPathTool {
    path_validator: PathValidator,
}

impl FindPathTool {
    /// Creates a new find path tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The working directory for path validation
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::find_path::FindPathTool;
    /// use std::path::PathBuf;
    ///
    /// let tool = FindPathTool::new(PathBuf::from("/project"));
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
struct FindPathParams {
    glob: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_page_size")]
    limit: usize,
}

fn default_page_size() -> usize {
    DEFAULT_PAGE_SIZE
}

#[async_trait]
impl ToolExecutor for FindPathTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_FIND_PATH,
            "description": "Searches for files matching a glob pattern. Supports pagination with offset and limit parameters.",
            "parameters": {
                "type": "object",
                "properties": {
                    "glob": {
                        "type": "string",
                        "description": "Glob pattern to match (e.g., '**/*.rs', 'src/**/*.txt'). Supports * (any chars in one dir level) and ? (single char) wildcards."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Starting position for pagination (0-based, default: 0)",
                        "default": 0
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 50)",
                        "default": 50
                    }
                },
                "required": ["glob"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: FindPathParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // Validate limit to prevent excessive memory usage
        let limit = if params.limit > 1000 {
            1000
        } else {
            params.limit
        };
        let limit = if limit == 0 { DEFAULT_PAGE_SIZE } else { limit };

        // Validate glob pattern is not empty
        if params.glob.is_empty() {
            return Ok(ToolResult::error(
                "Glob pattern cannot be empty".to_string(),
            ));
        }

        // Collect all matching paths using glob-match
        let mut matches: Vec<String> = Vec::new();

        // Walk the working directory
        for entry in WalkDir::new(self.path_validator.working_dir())
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let rel_path = match entry.path().strip_prefix(self.path_validator.working_dir()) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let path_str = match rel_path.to_str() {
                Some(s) => s,
                None => continue,
            };

            // Normalize path separators for cross-platform compatibility
            let normalized = path_str.replace('\\', "/");

            // Use glob-match for pattern matching
            if glob_match::glob_match(&params.glob, &normalized) {
                matches.push(normalized);
            }
        }

        // Sort matches alphabetically
        matches.sort();

        // Apply pagination
        let total = matches.len();
        let start = params.offset.min(total);
        let end = (start + limit).min(total);

        let page_results = &matches[start..end];

        // Format output
        let output = if page_results.is_empty() {
            if total == 0 {
                format!("No matches found for pattern: {}", params.glob)
            } else {
                format!(
                    "Offset {} is beyond available results (total: {})",
                    params.offset, total
                )
            }
        } else {
            let results_str = page_results.join("\n");
            format!(
                "Found {} matches (showing {}-{} of {}):\n{}",
                total,
                start + 1,
                end,
                total,
                results_str
            )
        };

        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_simple_pattern_returns_matches() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file1.txt"), "content")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("file2.txt"), "content")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("file.rs"), "code")
            .await
            .unwrap();

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "*.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(!result.output.contains("file.rs"));
    }

    #[tokio::test]
    async fn test_execute_with_recursive_pattern_returns_nested_matches() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir_all(temp.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("src/main.rs"), "code")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("main.rs"), "code")
            .await
            .unwrap();

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "**/*.rs"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("main.rs"));
        assert!(result.output.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_execute_with_question_mark_pattern() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file1.txt"), "content")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("file2.txt"), "content")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("file12.txt"), "content")
            .await
            .unwrap();

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "file?.txt"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(!result.output.contains("file12.txt"));
    }

    #[tokio::test]
    async fn test_execute_with_pagination_limits_results() {
        let temp = TempDir::new().unwrap();
        for i in 0..100 {
            let filename = format!("file{:03}.txt", i);
            tokio::fs::write(temp.path().join(&filename), "content")
                .await
                .unwrap();
        }

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "*.txt",
                "limit": 10,
                "offset": 0
            }))
            .await
            .unwrap();

        assert!(result.success);
        // Count newlines in output (each match is on a line)
        let match_count = result.output.lines().filter(|l| l.contains("file")).count();
        assert_eq!(match_count, 10);
    }

    #[tokio::test]
    async fn test_execute_with_offset_pagination() {
        let temp = TempDir::new().unwrap();
        for i in 0..20 {
            let filename = format!("file{:02}.txt", i);
            tokio::fs::write(temp.path().join(&filename), "content")
                .await
                .unwrap();
        }

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "*.txt",
                "limit": 10,
                "offset": 10
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("showing 11-20"));
    }

    #[tokio::test]
    async fn test_execute_with_no_matches_returns_message() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file.txt"), "content")
            .await
            .unwrap();

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "*.rs"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_execute_with_specific_directory_pattern() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir_all(temp.path().join("src/tools"))
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("src/tools/read.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("src/tools/write.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(temp.path().join("src/main.rs"), "")
            .await
            .unwrap();

        let tool = FindPathTool::new(temp.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "glob": "src/tools/*.rs"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("src/tools/read.rs"));
        assert!(result.output.contains("src/tools/write.rs"));
        assert!(!result.output.contains("src/main.rs"));
    }
}
