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

    /// Existing path component is a symbolic link.
    #[error("Symbolic link components are not allowed: {0}")]
    SymlinkComponent(String),

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
///
/// let validator = PathValidator::new(std::env::temp_dir());
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
    ///
    /// let working_dir = std::env::temp_dir();
    /// let validator = PathValidator::new(working_dir.clone());
    /// assert_eq!(validator.working_dir(), &working_dir);
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
    ///
    /// let validator = PathValidator::new(std::env::temp_dir());
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

        // Get canonical working directory. A missing working directory is a
        // configuration error for all file tools because containment cannot be
        // verified safely without a real root.
        let canonical_working = self
            .working_dir
            .canonicalize()
            .map_err(FileUtilsError::Io)?;

        self.reject_existing_symlink_components(path, target)?;

        // If the file/directory exists, canonicalize to follow symlinks and
        // verify the final target is still contained in the canonical workspace.
        if full_path.exists() {
            let canonical_target = full_path.canonicalize().map_err(FileUtilsError::Io)?;

            if !canonical_target.starts_with(&canonical_working) {
                return Err(FileUtilsError::OutsideWorkingDir(format!(
                    "Path escapes working directory: {:?}",
                    target
                )));
            }
            return Ok(canonical_target);
        }

        // For non-existent targets, canonicalize the nearest existing ancestor
        // instead of only the immediate parent. This prevents paths such as
        // `link/new/file` from passing validation when `link` is a symlink and
        // `link/new` does not yet exist.
        let nearest_existing = Self::nearest_existing_ancestor(&full_path).ok_or_else(|| {
            FileUtilsError::OutsideWorkingDir(format!(
                "No existing ancestor under working directory for {:?}",
                target
            ))
        })?;
        let ancestor_canonical = nearest_existing
            .canonicalize()
            .map_err(FileUtilsError::Io)?;
        if !ancestor_canonical.starts_with(&canonical_working) {
            return Err(FileUtilsError::OutsideWorkingDir(format!(
                "Nearest existing ancestor outside working directory: {:?}",
                target
            )));
        }

        Ok(full_path)
    }

    fn reject_existing_symlink_components(
        &self,
        relative_path: &Path,
        target: &str,
    ) -> Result<(), FileUtilsError> {
        let mut current = self.working_dir.clone();
        for component in relative_path.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => {
                    current.push(part);
                    if let Ok(metadata) = std::fs::symlink_metadata(&current) {
                        if metadata.file_type().is_symlink() {
                            return Err(FileUtilsError::SymlinkComponent(format!(
                                "Path contains symbolic link component while validating {}: {:?}",
                                target, current
                            )));
                        }
                    }
                }
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }
        Ok(())
    }

    fn nearest_existing_ancestor(path: &Path) -> Option<PathBuf> {
        let mut current = if path.exists() {
            path.to_path_buf()
        } else {
            path.parent()?.to_path_buf()
        };

        loop {
            if current.exists() {
                return Some(current);
            }
            current = current.parent()?.to_path_buf();
        }
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

/// Recursively copies a directory from source to destination.
///
/// Walks the source directory tree using `WalkDir` and copies every file and
/// directory entry to the corresponding path under `destination`. Parent
/// directories are created automatically.
///
/// # Arguments
///
/// * `source` - Absolute path to the source directory
/// * `destination` - Absolute path to the destination directory
///
/// # Returns
///
/// Returns the count of files copied
///
/// # Errors
///
/// Returns `FileUtilsError::Io` if directory creation or file copy fails
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::file_utils::copy_directory_recursive;
/// use std::path::Path;
///
/// # tokio_test::block_on(async {
/// let count = copy_directory_recursive(
///     Path::new("/source/dir"),
///     Path::new("/dest/dir"),
/// ).await.unwrap();
/// assert!(count > 0);
/// # });
/// ```
pub async fn copy_directory_recursive(
    source: &Path,
    destination: &Path,
) -> Result<usize, FileUtilsError> {
    use walkdir::WalkDir;

    let mut count = 0;

    for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
        let rel_path = entry.path().strip_prefix(source).map_err(|e| {
            FileUtilsError::ParentDirCreation(format!(
                "Failed to compute relative path while copying directory: {}",
                e
            ))
        })?;
        let dest_path = destination.join(rel_path);

        if entry.file_type().is_dir() {
            tokio::fs::create_dir_all(&dest_path)
                .await
                .map_err(FileUtilsError::Io)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(FileUtilsError::Io)?;
            }
            tokio::fs::copy(entry.path(), &dest_path)
                .await
                .map_err(FileUtilsError::Io)?;
            count += 1;
        }
    }

    Ok(count)
}

/// Match a text string against a glob pattern.
///
/// Delegates to the `glob-match` crate which supports `*` (any non-separator
/// characters), `?` (single character), and `**` (any path segment including
/// separators).
///
/// This is the single canonical glob implementation for all tool modules.
/// Use this function instead of adding custom recursive glob matchers.
///
/// # Arguments
///
/// * `text` - Text to match against the pattern
/// * `pattern` - Glob pattern (supports `*`, `?`, `**`)
///
/// # Returns
///
/// Returns `true` if `text` matches `pattern`
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::glob_match_pattern;
///
/// assert!(glob_match_pattern("file.txt", "*.txt"));
/// assert!(glob_match_pattern("src/main.rs", "**/*.rs"));
/// assert!(!glob_match_pattern("file.txt", "*.rs"));
/// ```
pub fn glob_match_pattern(text: &str, pattern: &str) -> bool {
    glob_match::glob_match(pattern, text)
}

/// Generate a unified diff between two text strings
///
/// Creates a line-based unified diff showing changes between old and new content.
/// Uses the `similar` crate's TextDiff for accurate diff generation.
///
/// # Arguments
///
/// * `old_text` - The original text content
/// * `new_text` - The modified text content
///
/// # Returns
///
/// Returns a string containing the unified diff, or an error if generation fails
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::generate_diff;
///
/// let old = "line 1\nline 2\n";
/// let new = "line 1\nline 2 modified\n";
/// let diff = generate_diff(old, new).unwrap();
/// assert!(diff.contains("line 2 modified"));
/// ```
pub fn generate_diff(old_text: &str, new_text: &str) -> crate::error::Result<String> {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old_text, new_text);
    let mut result = String::new();

    for change in diff.iter_all_changes() {
        let line = change.value();
        match change.tag() {
            ChangeTag::Delete => {
                result.push_str(&format!("- {}", line));
                if !line.ends_with('\n') {
                    result.push('\n');
                }
            }
            ChangeTag::Insert => {
                result.push_str(&format!("+ {}", line));
                if !line.ends_with('\n') {
                    result.push('\n');
                }
            }
            ChangeTag::Equal => {
                result.push_str(&format!("  {}", line));
                if !line.ends_with('\n') {
                    result.push('\n');
                }
            }
        }
    }

    if result.is_empty() {
        result.push_str("(no changes)\n");
    }

    Ok(result)
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

    #[cfg(unix)]
    #[test]
    fn test_validate_with_symlink_ancestor_for_new_path_returns_error() {
        use std::os::unix::fs::symlink;

        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        symlink(outside.path(), workspace.path().join("link")).unwrap();

        let validator = PathValidator::new(workspace.path().to_path_buf());
        let result = validator.validate("link/new/file.txt");

        assert!(matches!(result, Err(FileUtilsError::SymlinkComponent(_))));
    }

    #[cfg(unix)]
    #[test]
    fn test_validate_with_existing_symlink_target_returns_error() {
        use std::os::unix::fs::symlink;

        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "secret").unwrap();
        symlink(&outside_file, workspace.path().join("secret_link.txt")).unwrap();

        let validator = PathValidator::new(workspace.path().to_path_buf());
        let result = validator.validate("secret_link.txt");

        assert!(matches!(result, Err(FileUtilsError::SymlinkComponent(_))));
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

    #[tokio::test]
    async fn test_copy_directory_recursive_copies_files_and_dirs() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");
        tokio::fs::create_dir(&src).await.unwrap();
        tokio::fs::create_dir(src.join("sub")).await.unwrap();
        tokio::fs::write(src.join("a.txt"), "alpha").await.unwrap();
        tokio::fs::write(src.join("sub/b.txt"), "beta")
            .await
            .unwrap();

        let count = copy_directory_recursive(&src, &dst).await.unwrap();
        assert_eq!(count, 2);
        assert!(dst.join("a.txt").exists());
        assert!(dst.join("sub/b.txt").exists());
    }

    #[tokio::test]
    async fn test_copy_directory_recursive_empty_directory_returns_zero() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");
        tokio::fs::create_dir(&src).await.unwrap();

        let count = copy_directory_recursive(&src, &dst).await.unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_glob_match_pattern_simple_extension() {
        assert!(glob_match_pattern("file.txt", "*.txt"));
        assert!(!glob_match_pattern("file.rs", "*.txt"));
    }

    #[test]
    fn test_glob_match_pattern_double_star_matches_path() {
        assert!(glob_match_pattern("src/main.rs", "**/*.rs"));
        assert!(glob_match_pattern("a/b/c.rs", "**/*.rs"));
    }

    #[test]
    fn test_glob_match_pattern_question_mark() {
        assert!(glob_match_pattern("file1.txt", "file?.txt"));
        assert!(!glob_match_pattern("file12.txt", "file?.txt"));
    }
}
