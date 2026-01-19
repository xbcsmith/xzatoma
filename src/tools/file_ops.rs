//! File operations tool for XZatoma
//!
//! This module provides file operation tools including read, write, list, search, and diff.
//! Phase 5 implementation delivered here.

use crate::config::ToolsConfig;
use crate::error::{Result, XzatomaError};
use async_trait::async_trait;
use regex::Regex;
use similar::{ChangeTag, TextDiff};
use std::path::{Component, Path, PathBuf};
use std::{fs as stdfs, io};
use tokio::fs;
use walkdir::WalkDir;

use super::{Tool, ToolExecutor, ToolResult};

/// File operations tool
///
/// Implements list/read/write/delete/diff as a `ToolExecutor` to be registered in ToolRegistry.
pub struct FileOpsTool {
    working_dir: PathBuf,
    config: ToolsConfig,
}

impl FileOpsTool {
    /// Create a new file ops tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Working directory where operations are confined
    /// * `config` - Tools configuration (max file read size etc.)
    pub fn new(working_dir: PathBuf, config: ToolsConfig) -> Self {
        Self {
            working_dir,
            config,
        }
    }

    /// Validate a path is within the configured working directory
    ///
    /// Prevents absolute paths, home directory (`~`) and directory traversal patterns (`..`).
    /// If a target exists, it canonicalizes and verifies it is under the working dir.
    fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        // Absolute paths not allowed for tool-level operations
        if path.is_absolute() {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory("Absolute paths not allowed".to_string()),
            ));
        }

        // Normalize path token check - no home (~) or parent traversal
        let path_str = path.to_string_lossy();
        if path_str.starts_with('~') {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory(
                    "Home directory paths are not allowed".to_string(),
                ),
            ));
        }

        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory(
                    "Directory traversal is not allowed".to_string(),
                ),
            ));
        }

        // Compose candidate full path (relative to working_dir)
        let full_path = self.working_dir.join(path);

        // If the file/directory exists, canonicalize to follow symlinks and ensure confinement
        let canonical_working = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        if full_path.exists() {
            let canonical_target = full_path.canonicalize().map_err(|e| {
                anyhow::Error::from(XzatomaError::Tool(format!(
                    "Failed to canonicalize path: {}",
                    e
                )))
            })?;
            if !canonical_target.starts_with(&canonical_working) {
                return Err(anyhow::Error::from(
                    XzatomaError::PathOutsideWorkingDirectory(format!(
                        "Path escapes working directory: {:?}",
                        path
                    )),
                ));
            }
            return Ok(canonical_target);
        }

        // For non-existent target (creating new file), ensure parent is within working_dir
        if let Some(parent) = full_path.parent() {
            let parent_canonical = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if !parent_canonical.starts_with(&canonical_working) {
                return Err(anyhow::Error::from(
                    XzatomaError::PathOutsideWorkingDirectory(format!(
                        "Target parent escapes working directory: {:?}",
                        parent
                    )),
                ));
            }
        } else {
            return Err(anyhow::Error::from(XzatomaError::Tool(
                "Invalid path".to_string(),
            )));
        }

        Ok(full_path)
    }

    /// List files under the configured working directory.
    ///
    /// - If pattern is provided, it is treated as a regex and matches against the relative path.
    /// - If `recursive` is false, only files directly under the directory are returned.
    pub async fn list_files(&self, pattern: Option<String>, recursive: bool) -> Result<ToolResult> {
        let mut results = Vec::new();

        let depth = if recursive { usize::MAX } else { 1 };
        for entry in WalkDir::new(&self.working_dir)
            .max_depth(depth)
            .follow_links(false)
        {
            let entry =
                entry.map_err(|e| anyhow::Error::from(XzatomaError::Tool(e.to_string())))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(&self.working_dir)
                    .unwrap_or(entry.path());
                let rel_str = rel.to_string_lossy().to_string();

                if let Some(ref pat) = pattern {
                    // Try regex first, if invalid, fall back to substring
                    let matches = match Regex::new(pat) {
                        Ok(re) => re.is_match(&rel_str),
                        Err(_) => rel_str.contains(pat),
                    };
                    if !matches {
                        continue;
                    }
                }

                results.push(rel_str);
            }
        }

        results.sort();
        Ok(ToolResult::success(results.join("\n")))
    }

    /// Read a file from the configured working directory
    ///
    /// Enforces `config.max_file_read_size`.
    pub async fn read_file(&self, path: &str) -> Result<ToolResult> {
        let path = Path::new(path);
        let full_path = self.validate_path(path)?;

        let metadata = stdfs::metadata(&full_path).map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!("Failed to stat file: {}", e)))
        })?;

        if metadata.len() > self.config.max_file_read_size as u64 {
            return Ok(ToolResult::error(format!(
                "File too large: {} bytes (max: {})",
                metadata.len(),
                self.config.max_file_read_size
            )));
        }

        let content = fs::read_to_string(&full_path).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!("Failed to read file: {}", e)))
        })?;

        Ok(ToolResult::success(content))
    }

    /// Write content to a file under the configured working directory
    ///
    /// Creates parent directories if necessary.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<ToolResult> {
        let path = Path::new(path);

        // Validate by checking parent directory against working_dir (we allow creation)
        let full_path = self.validate_path(path)?;

        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    anyhow::Error::from(XzatomaError::Tool(format!(
                        "Failed to create directories: {}",
                        e
                    )))
                })?;
            }
        }

        fs::write(&full_path, content).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!("Failed to write file: {}", e)))
        })?;

        Ok(ToolResult::success(format!(
            "File written: {}",
            full_path.display()
        )))
    }

    /// Delete a file under the configured working directory
    ///
    /// The caller should confirm intention before allowing deletion.
    pub async fn delete_file(&self, path: &str) -> Result<ToolResult> {
        let path = Path::new(path);
        let full_path = self.validate_path(path)?;

        if !full_path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                full_path.display()
            )));
        }

        fs::remove_file(&full_path).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!("Failed to delete file: {}", e)))
        })?;

        Ok(ToolResult::success(format!(
            "File deleted: {}",
            full_path.display()
        )))
    }

    /// Produce a line-based diff between two files
    pub async fn file_diff(&self, path1: &str, path2: &str) -> Result<ToolResult> {
        let p1 = Path::new(path1);
        let p2 = Path::new(path2);

        let full1 = self.validate_path(p1)?;
        let full2 = self.validate_path(p2)?;

        let content1 = fs::read_to_string(&full1).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!(
                "Failed to read {}: {}",
                path1, e
            )))
        })?;

        let content2 = fs::read_to_string(&full2).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!(
                "Failed to read {}: {}",
                path2, e
            )))
        })?;

        let diff = TextDiff::from_lines(&content1, &content2);
        let mut output = Vec::new();

        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
                ChangeTag::Equal => ' ',
            };
            output.push(format!("{}{}", sign, change));
        }

        Ok(ToolResult::success(output.join("")))
    }
}

#[async_trait]
impl ToolExecutor for FileOpsTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "file_ops",
            "description": "File operations: list, read, write, delete, diff (confined to working_dir)",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["list","read","write","delete","diff"]
                    },
                    "path": {
                        "type": "string",
                        "description": "File path relative to working dir (or absolute if allowed by environment)"
                    },
                    "path2": {
                        "type": "string",
                        "description": "Second path for diff"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content for write"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Pattern filter (regex)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Recursive for list"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "Confirm destructive operations like delete"
                    }
                },
                "required": ["operation"]
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let operation = params["operation"].as_str().ok_or_else(|| {
            anyhow::Error::from(XzatomaError::Tool("Missing 'operation' param".to_string()))
        })?;

        match operation {
            "list" => {
                let pattern = params["pattern"].as_str().map(String::from);
                let recursive = params["recursive"].as_bool().unwrap_or(false);
                self.list_files(pattern, recursive).await
            }
            "read" => {
                let path = params["path"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path' param".to_string()))
                })?;
                self.read_file(path).await
            }
            "write" => {
                let path = params["path"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path' param".to_string()))
                })?;
                let content = params["content"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'content' param".to_string()))
                })?;
                self.write_file(path, content).await
            }
            "delete" => {
                let path = params["path"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path' param".to_string()))
                })?;
                let confirm = params["confirm"].as_bool().unwrap_or(false);
                if !confirm {
                    return Ok(ToolResult::error(
                        "Delete operation requires confirm=true".to_string(),
                    ));
                }
                self.delete_file(path).await
            }
            "diff" => {
                let path1 = params["path"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path' param".to_string()))
                })?;
                let path2 = params["path2"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path2' param".to_string()))
                })?;
                self.file_diff(path1, path2).await
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}

/// Read-only file operations tool for Planning mode
///
/// This tool provides only safe read operations:
/// - `read_file(path: string) -> string` - Read file contents
/// - `list_files(directory: string, recursive: boolean) -> array` - List files
/// - `search_files(pattern: string, directory: string) -> array` - Search files
///
/// No write, delete, or modification operations are available.
/// This tool is automatically used in Planning mode to prevent accidental
/// modifications while still allowing the agent to explore the codebase.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::file_ops::FileOpsReadOnlyTool;
/// use xzatoma::config::ToolsConfig;
/// use std::path::PathBuf;
///
/// let tool = FileOpsReadOnlyTool::new(
///     PathBuf::from("."),
///     ToolsConfig::default(),
/// );
///
/// // Only read-only operations are available
/// ```
#[derive(Debug, Clone)]
pub struct FileOpsReadOnlyTool {
    working_dir: PathBuf,
    config: ToolsConfig,
}

impl FileOpsReadOnlyTool {
    /// Create a new read-only file operations tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Working directory where operations are confined
    /// * `config` - Tools configuration (max file read size etc.)
    ///
    /// # Returns
    ///
    /// Returns a new `FileOpsReadOnlyTool` instance
    pub fn new(working_dir: PathBuf, config: ToolsConfig) -> Self {
        Self {
            working_dir,
            config,
        }
    }

    /// Validate a path is within the configured working directory
    ///
    /// Prevents absolute paths, home directory (`~`) and directory traversal patterns (`..`).
    /// If a target exists, it canonicalizes and verifies it is under the working dir.
    fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        // Absolute paths not allowed for tool-level operations
        if path.is_absolute() {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory("Absolute paths not allowed".to_string()),
            ));
        }

        // Normalize path token check - no home (~) or parent traversal
        let path_str = path.to_string_lossy();
        if path_str.starts_with('~') {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory(
                    "Home directory paths are not allowed".to_string(),
                ),
            ));
        }

        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(anyhow::Error::from(
                XzatomaError::PathOutsideWorkingDirectory(
                    "Directory traversal is not allowed".to_string(),
                ),
            ));
        }

        // Compose candidate full path (relative to working_dir)
        let full_path = self.working_dir.join(path);

        // If the file/directory exists, canonicalize to follow symlinks and ensure confinement
        let canonical_working = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        if full_path.exists() {
            let canonical_target = full_path.canonicalize().map_err(|e| {
                anyhow::Error::from(XzatomaError::Tool(format!(
                    "Failed to canonicalize path: {}",
                    e
                )))
            })?;
            if !canonical_target.starts_with(&canonical_working) {
                return Err(anyhow::Error::from(
                    XzatomaError::PathOutsideWorkingDirectory(format!(
                        "Path escapes working directory: {:?}",
                        path
                    )),
                ));
            }
            return Ok(canonical_target);
        }

        // For non-existent target (creating new file), ensure parent is within working_dir
        if let Some(parent) = full_path.parent() {
            let parent_canonical = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if !parent_canonical.starts_with(&canonical_working) {
                return Err(anyhow::Error::from(
                    XzatomaError::PathOutsideWorkingDirectory(format!(
                        "Target parent escapes working directory: {:?}",
                        parent
                    )),
                ));
            }
        } else {
            return Err(anyhow::Error::from(XzatomaError::Tool(
                "Invalid path".to_string(),
            )));
        }

        Ok(full_path)
    }

    /// Read file contents
    ///
    /// # Arguments
    ///
    /// * `path` - File path relative to working directory
    ///
    /// # Returns
    ///
    /// Returns a ToolResult with file contents or error
    async fn read_file(&self, path: &str) -> Result<ToolResult> {
        let validated = self.validate_path(Path::new(path))?;
        let content = fs::read_to_string(&validated).await.map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!(
                "Failed to read file '{}': {}",
                path, e
            )))
        })?;

        let max_size = self.config.max_file_read_size;
        let result = ToolResult::success(content).truncate_if_needed(max_size);
        Ok(result)
    }

    /// List files in a directory
    ///
    /// - If pattern is provided, it is treated as a regex and matches against the relative path.
    /// - If `recursive` is false, only files directly under the directory are returned.
    async fn list_files(&self, pattern: Option<String>, recursive: bool) -> Result<ToolResult> {
        let mut results = Vec::new();

        let depth = if recursive { usize::MAX } else { 1 };
        for entry in WalkDir::new(&self.working_dir)
            .max_depth(depth)
            .follow_links(false)
        {
            let entry =
                entry.map_err(|e| anyhow::Error::from(XzatomaError::Tool(e.to_string())))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(&self.working_dir)
                    .unwrap_or(entry.path());
                let rel_str = rel.to_string_lossy().to_string();

                if let Some(ref pat) = pattern {
                    // Try regex first, if invalid, fall back to substring
                    let matches = match Regex::new(pat) {
                        Ok(re) => re.is_match(&rel_str),
                        Err(_) => rel_str.contains(pat),
                    };
                    if !matches {
                        continue;
                    }
                }

                results.push(rel_str);
            }
        }

        results.sort();
        let output = results.join("\n");
        Ok(ToolResult::success(output))
    }

    /// Search for files matching a pattern
    ///
    /// # Arguments
    ///
    /// * `pattern` - Regex pattern or substring to match against file paths
    /// * `recursive` - Whether to search recursively
    async fn search_files(&self, pattern: String, _recursive: bool) -> Result<ToolResult> {
        let mut results = Vec::new();

        let depth = usize::MAX;
        for entry in WalkDir::new(&self.working_dir)
            .max_depth(depth)
            .follow_links(false)
        {
            let entry =
                entry.map_err(|e| anyhow::Error::from(XzatomaError::Tool(e.to_string())))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(&self.working_dir)
                    .unwrap_or(entry.path());
                let rel_str = rel.to_string_lossy().to_string();

                // Try regex first, if invalid, fall back to substring
                let matches = match Regex::new(&pattern) {
                    Ok(re) => re.is_match(&rel_str),
                    Err(_) => rel_str.contains(&pattern),
                };

                if matches {
                    results.push(rel_str);
                }
            }
        }

        results.sort();
        let output = results.join("\n");
        Ok(ToolResult::success(output))
    }
}

#[async_trait]
impl ToolExecutor for FileOpsReadOnlyTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "file_ops",
            "description": "Read-only file operations: list, read, search (Planning mode - no modifications)",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["list","read","search"]
                    },
                    "path": {
                        "type": "string",
                        "description": "File path relative to working dir"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Pattern filter (regex)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Recursive for list/search"
                    }
                },
                "required": ["operation"]
            }
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let operation = params["operation"].as_str().ok_or_else(|| {
            anyhow::Error::from(XzatomaError::Tool("Missing 'operation' param".to_string()))
        })?;

        match operation {
            "read" => {
                let path = params["path"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'path' param".to_string()))
                })?;
                self.read_file(path).await
            }
            "list" => {
                let pattern = params["pattern"].as_str().map(String::from);
                let recursive = params["recursive"].as_bool().unwrap_or(false);
                self.list_files(pattern, recursive).await
            }
            "search" => {
                let pattern = params["pattern"].as_str().ok_or_else(|| {
                    anyhow::Error::from(XzatomaError::Tool("Missing 'pattern' param".to_string()))
                })?;
                let recursive = params["recursive"].as_bool().unwrap_or(true);
                self.search_files(pattern.to_string(), recursive).await
            }
            _ => Ok(ToolResult::error(format!(
                "Operation '{}' not available in read-only mode. Available operations: list, read, search",
                operation
            ))),
        }
    }
}

/// Convenience: read a file by path (works with absolute or relative path)
///
/// Returns content or an error on failure.
pub async fn read_file(path: &str) -> Result<String> {
    let p = Path::new(path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()?.join(p)
    };

    let s = fs::read_to_string(full).await.map_err(|e| {
        anyhow::Error::from(XzatomaError::Tool(format!("Failed to read file: {}", e)))
    })?;
    Ok(s)
}

/// Convenience: write a file (absolute or relative path)
pub async fn write_file(path: &str, content: &str) -> Result<()> {
    let p = Path::new(path);
    let full = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()?.join(p)
    };

    if let Some(parent) = full.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await.map_err(|e| {
                anyhow::Error::from(XzatomaError::Tool(format!("Failed to create dir: {}", e)))
            })?;
        }
    }

    fs::write(full, content).await.map_err(|e| {
        anyhow::Error::from(XzatomaError::Tool(format!("Failed to write file: {}", e)))
    })?;

    Ok(())
}

/// Convenience: list files in a directory
///
/// - If `recursive` is true, performs recursive walk.
pub async fn list_files(path: &str, recursive: bool) -> Result<Vec<String>> {
    let root = Path::new(path);
    let start = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };

    let mut files = Vec::new();
    let depth = if recursive { usize::MAX } else { 1 };
    for entry in WalkDir::new(&start).max_depth(depth).follow_links(false) {
        let entry = entry.map_err(|e| anyhow::Error::from(XzatomaError::Tool(e.to_string())))?;
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(&start).unwrap_or(entry.path());
            files.push(rel.to_string_lossy().to_string());
        }
    }
    files.sort();
    Ok(files)
}

/// Search files matching a pattern (regex or substring)
pub async fn search_files(pattern: &str, path: &str) -> Result<Vec<String>> {
    let root = Path::new(path);
    let start = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };

    let re = Regex::new(pattern);
    let mut results = Vec::new();
    for entry in WalkDir::new(&start).follow_links(false) {
        let entry = entry.map_err(|e| anyhow::Error::from(XzatomaError::Tool(e.to_string())))?;
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(&start).unwrap_or(entry.path());
            let rel_str = rel.to_string_lossy().to_string();

            let matched = match &re {
                Ok(r) => r.is_match(&rel_str),
                Err(_) => rel_str.contains(pattern),
            };

            if matched {
                results.push(rel_str);
            }
        }
    }
    results.sort();
    Ok(results)
}

/// Generate a line-by-line diff between two inputs using `similar::TextDiff`
pub fn generate_diff(original: &str, modified: &str) -> Result<String> {
    let diff = TextDiff::from_lines(original, modified);
    let mut output = Vec::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push(format!("{}{}", sign, change));
    }

    Ok(output.join(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ToolsConfig;
    use std::fs as stdfs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_file_success() {
        let dir = tempdir().unwrap();
        let file_path = {
            let p = dir.path().join("read_test.txt");
            stdfs::write(&p, "hello read").unwrap();
            p
        };
        let content = read_file(file_path.to_str().unwrap()).await.unwrap();
        assert_eq!(content, "hello read");
    }

    #[tokio::test]
    async fn test_write_file_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir/write_test.txt");
        let path_str = path.to_str().unwrap();
        write_file(path_str, "write content").await.unwrap();
        let content = stdfs::read_to_string(path).unwrap();
        assert_eq!(content, "write content");
    }

    #[tokio::test]
    async fn test_list_files_recursive_and_non_recursive() {
        let dir = tempdir().unwrap();
        let _ = stdfs::write(dir.path().join("a.txt"), "a");
        let sub_path = dir.path().join("sub");
        stdfs::create_dir_all(&sub_path).unwrap();
        let _ = stdfs::write(sub_path.join("b.txt"), "b");
        let files_non_recursive = list_files(dir.path().to_str().unwrap(), false)
            .await
            .unwrap();
        assert!(files_non_recursive.contains(&"a.txt".to_string()));
        assert!(!files_non_recursive.iter().any(|f| f.contains("sub")));
        let files_recursive = list_files(dir.path().to_str().unwrap(), true)
            .await
            .unwrap();
        assert!(files_recursive.iter().any(|f| f.contains("sub")));
    }

    #[tokio::test]
    async fn test_search_files_regex_and_substring() {
        let dir = tempdir().unwrap();
        let _ = stdfs::write(dir.path().join("x.rs"), "x");
        let _ = stdfs::write(dir.path().join("y.txt"), "y");
        let matches_regex = search_files(r"\.rs$", dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert!(matches_regex.iter().any(|m| m.ends_with("x.rs")));
        let matches_sub = search_files(".txt", dir.path().to_str().unwrap())
            .await
            .unwrap();
        assert!(matches_sub.iter().any(|m| m.ends_with("y.txt")));
    }

    #[test]
    fn test_generate_diff_basic() {
        let a = "line1\nline2\n";
        let b = "line1\nline2 modified\n";
        let diff = generate_diff(a, b).unwrap();
        assert!(diff.contains("+line2 modified"));
        assert!(diff.contains("-line2"));
    }

    #[tokio::test]
    async fn test_fileops_tool_read_write_delete_and_diff() {
        let dir = tempdir().unwrap();
        let config = ToolsConfig::default();
        let tool = FileOpsTool::new(dir.path().to_path_buf(), config);

        // Write a file
        let write_res = tool
            .write_file("tool_write.txt", "tool content")
            .await
            .unwrap();
        assert!(write_res.success);
        assert!(write_res.output.contains("File written"));

        // Read the file
        let read_res = tool.read_file("tool_write.txt").await.unwrap();
        assert!(read_res.success);
        assert_eq!(read_res.output, "tool content");

        // Diff with a second file
        let _ = stdfs::write(dir.path().join("other.txt"), "other content");
        let diff_res = tool.file_diff("tool_write.txt", "other.txt").await.unwrap();
        assert!(diff_res.success);
        assert!(!diff_res.output.is_empty());

        // Delete the written file
        let delete_res = tool.delete_file("tool_write.txt").await.unwrap();
        assert!(delete_res.success);
        assert!(!dir.path().join("tool_write.txt").exists());
    }

    #[tokio::test]
    async fn test_fileops_tool_list_and_pattern() {
        let dir = tempdir().unwrap();
        let config = ToolsConfig::default();
        let tool = FileOpsTool::new(dir.path().to_path_buf(), config);

        let _ = stdfs::write(dir.path().join("match.rs"), "x");
        let _ = stdfs::write(dir.path().join("nomatch.txt"), "y");
        let res = tool
            .list_files(Some(r"\.rs$".to_string()), false)
            .await
            .unwrap();
        assert!(res.success);
        assert!(res.output.contains("match.rs"));
        assert!(!res.output.contains("nomatch.txt"));
    }

    #[tokio::test]
    async fn test_validate_path_outside() {
        let dir = tempdir().unwrap();
        let config = ToolsConfig::default();
        let tool = FileOpsTool::new(dir.path().to_path_buf(), config);

        // Attempt to read outside the working dir should fail
        let read_err = tool.read_file("../outside.txt").await;
        assert!(read_err.is_err());
    }
}
