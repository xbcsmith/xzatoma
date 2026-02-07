//! File utilities module for path validation and file operations
//!
//! This module provides core utilities for safe file operations:
//! - Path validation against security constraints
//! - Parent directory creation
//! - File size checking

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Error type for file utilities operations
///
/// Provides detailed error information for path validation and file operation failures.
#[derive(Error, Debug)]
pub enum FileUtilsError {
    /// Path traversal attempt detected (contains .. or ~ patterns)
    #[error("Path traversal attempt detected: {0}")]
    PathTraversal(String),

    /// Absolute path not allowed
    #[error("Absolute path not allowed: {0}")]
    AbsolutePath(String),

    /// Path outside working directory
    #[error("Path outside working directory: {0}")]
    OutsideWorkingDir(String),

    /// File size exceeds maximum allowed
    #[error("File size {0} bytes exceeds maximum {1} bytes")]
    FileTooLarge(u64, u64),

    /// Parent directory creation failed
    #[error("Parent directory creation failed: {0}")]
    ParentDirCreation(String),

    /// IO error occurred
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Validates file paths against security constraints
///
/// Ensures paths are relative, within working directory, and do not
/// contain traversal sequences that could escape the working directory.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::PathValidator;
/// use std::path::PathBuf;
///
/// let validator = PathValidator::new(PathBuf::from("/project"));
/// let result = validator.validate("src/main.rs");
/// assert!(result.is_ok());
/// ```
pub struct PathValidator {
    working_dir: PathBuf,
}

impl PathValidator {
    /// Creates a new path validator
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The base working directory for validation
    ///
    /// # Returns
    ///
    /// Returns a new PathValidator instance
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Returns a reference to the working directory
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::file_utils::PathValidator;
    /// use std::path::PathBuf;
    ///
    /// let validator = PathValidator::new(PathBuf::from("/project"));
    /// assert_eq!(validator.working_dir().to_str().unwrap(), "/project");
    /// ```
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Validates a path against security constraints
    ///
    /// # Arguments
    ///
    /// * `target` - The target path to validate (relative path string)
    ///
    /// # Returns
    ///
    /// Returns the canonicalized absolute PathBuf if valid
    ///
    /// # Errors
    ///
    /// Returns `FileUtilsError` if:
    /// - Path is absolute
    /// - Path contains traversal sequences (.., ~)
    /// - Path resolves outside working_dir
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::file_utils::PathValidator;
    /// use std::path::PathBuf;
    ///
    /// let validator = PathValidator::new(PathBuf::from("/project"));
    ///
    /// // Valid path
    /// let result = validator.validate("src/main.rs");
    /// assert!(result.is_ok());
    ///
    /// // Invalid path (traversal)
    /// let result = validator.validate("../etc/passwd");
    /// assert!(result.is_err());
    /// ```
    pub fn validate(&self, target: &str) -> Result<PathBuf, FileUtilsError> {
        let path = Path::new(target);

        // Check for absolute path
        if path.is_absolute() {
            return Err(FileUtilsError::AbsolutePath(target.to_string()));
        }

        // Check for home directory expansion
        if target.starts_with('~') {
            return Err(FileUtilsError::PathTraversal(
                "Home directory paths not allowed".to_string(),
            ));
        }

        // Check for ".." traversal sequences
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(FileUtilsError::PathTraversal(format!(
                "Parent directory traversal not allowed: {}",
                target
            )));
        }

        // Compose candidate full path (relative to working_dir)
        let full_path = self.working_dir.join(path);

        // Get canonical working directory
        let canonical_working = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        // If the file/directory exists, canonicalize to follow symlinks
        if full_path.exists() {
            let canonical_target = full_path.canonicalize().map_err(FileUtilsError::Io)?;

            // Verify resolved path is within working_dir
            if !canonical_target.starts_with(&canonical_working) {
                return Err(FileUtilsError::OutsideWorkingDir(format!(
                    "Path escapes working directory: {:?}",
                    target
                )));
            }
            return Ok(canonical_target);
        }

        // For non-existent target (creating new file), ensure parent is within working_dir
        if let Some(parent) = full_path.parent() {
            if parent.exists() {
                let parent_canonical = parent.canonicalize().map_err(FileUtilsError::Io)?;
                if !parent_canonical.starts_with(&canonical_working) {
                    return Err(FileUtilsError::OutsideWorkingDir(format!(
                        "Parent directory outside working directory: {:?}",
                        target
                    )));
                }
            }
        }

        Ok(full_path)
    }
}

/// Ensures parent directories exist for a given path
///
/// Creates all necessary parent directories using `tokio::fs::create_dir_all`.
///
/// # Arguments
///
/// * `path` - The file path whose parent directories should be created
///
/// # Returns
///
/// Returns Ok(()) if successful
///
/// # Errors
///
/// Returns `FileUtilsError::ParentDirCreation` if directory creation fails
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::ensure_parent_dirs;
/// use std::path::Path;
///
/// # tokio_test::block_on(async {
/// let path = Path::new("/tmp/test/nested/file.txt");
/// ensure_parent_dirs(path).await.unwrap();
/// # });
/// ```
pub async fn ensure_parent_dirs(path: &Path) -> Result<(), FileUtilsError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                FileUtilsError::ParentDirCreation(format!(
                    "Failed to create parent directories for {:?}: {}",
                    path, e
                ))
            })?;
        }
    }
    Ok(())
}

/// Checks if a file's size exceeds the maximum allowed
///
/// # Arguments
///
/// * `path` - The file path to check
/// * `max_size` - Maximum allowed size in bytes
///
/// # Returns
///
/// Returns Ok(file_size) if within limit
///
/// # Errors
///
/// Returns `FileUtilsError::FileTooLarge` if file exceeds max_size
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::check_file_size;
/// use std::path::Path;
///
/// # tokio_test::block_on(async {
/// let path = Path::new("small_file.txt");
/// let result = check_file_size(path, 1024 * 1024).await;
/// # });
/// ```
pub async fn check_file_size(path: &Path, max_size: u64) -> Result<u64, FileUtilsError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(FileUtilsError::Io)?;

    let file_size = metadata.len();
    if file_size > max_size {
        return Err(FileUtilsError::FileTooLarge(file_size, max_size));
    }

    Ok(file_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_path_validator_new_creates_validator() {
        let validator = PathValidator::new(PathBuf::from("/project"));
        assert_eq!(validator.working_dir, PathBuf::from("/project"));
    }

    #[test]
    fn test_validate_with_relative_path_succeeds() {
        let temp = TempDir::new().unwrap();
        let validator = PathValidator::new(temp.path().to_path_buf());
        let result = validator.validate("src/main.rs");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_absolute_path_returns_error() {
        let temp = TempDir::new().unwrap();
        let validator = PathValidator::new(temp.path().to_path_buf());
        let result = validator.validate("/etc/passwd");
        assert!(matches!(result, Err(FileUtilsError::AbsolutePath(_))));
    }

    #[test]
    fn test_validate_with_traversal_sequence_returns_error() {
        let temp = TempDir::new().unwrap();
        let validator = PathValidator::new(temp.path().to_path_buf());
        let result = validator.validate("../etc/passwd");
        assert!(matches!(result, Err(FileUtilsError::PathTraversal(_))));
    }

    #[test]
    fn test_validate_with_home_directory_returns_error() {
        let temp = TempDir::new().unwrap();
        let validator = PathValidator::new(temp.path().to_path_buf());
        let result = validator.validate("~/.bashrc");
        assert!(matches!(result, Err(FileUtilsError::PathTraversal(_))));
    }

    #[tokio::test]
    async fn test_ensure_parent_dirs_creates_directories() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested/deep/file.txt");
        let result = ensure_parent_dirs(&path).await;
        assert!(result.is_ok());
        assert!(path.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn test_check_file_size_with_small_file_succeeds() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("small.txt");
        tokio::fs::write(&path, "small content").await.unwrap();
        let result = check_file_size(&path, 1024).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 13);
    }

    #[tokio::test]
    async fn test_check_file_size_with_large_file_returns_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("large.txt");
        tokio::fs::write(&path, "x".repeat(2000)).await.unwrap();
        let result = check_file_size(&path, 1000).await;
        assert!(matches!(
            result,
            Err(FileUtilsError::FileTooLarge(2000, 1000))
        ));
    }
}
