# File Tools Modularization Implementation Plan

## Overview

This plan restructures the consolidated `file_ops.rs` module into individual, granular tool files following Zed's architecture pattern. Each file operation will be implemented as a standalone tool with its own `ToolExecutor` implementation, providing better separation of concerns, clearer API boundaries, and easier testing. The plan also implements missing tools from Zed's toolkit to achieve feature parity (excluding editor-specific `open_tool`).

**Key Objectives:**

- Split `file_ops.rs` into 9 individual tool files
- Implement 5 new missing tools (`copy_path`, `move_path`, `create_directory`, `find_path`, `edit_file`)
- Create shared utilities module for common functionality (path validation, file size checks, image decoding)
- Update registry builder to register individual tools
- Achieve >80% test coverage for all new tools
- Clean break from old API (no backward compatibility)
- Full compliance with AGENTS.md requirements (thiserror, doc comments, test naming)

## Pre-Implementation Requirements

Before starting Phase 1, verify ALL of the following:

**Environment Setup:**

```bash
# 1. Verify Rust toolchain version
rustc --version
# EXPECTED: rustc 1.70.0 or later

# 2. Install required components
rustup component add clippy rustfmt
# EXPECTED: component 'clippy' is up to date
#           component 'rustfmt' is up to date

# 3. Optional: Install coverage tool
cargo install cargo-tarpaulin
# EXPECTED: Installed package `cargo-tarpaulin v0.x.x`

# 4. Verify current branch
git rev-parse --abbrev-ref HEAD
# EXPECTED: pr-file-tools-modularization-<issue-number>

# 5. Verify clean working state
git status
# EXPECTED: nothing to commit, working tree clean

# 6. Run existing test suite
cargo test --all-features
# EXPECTED: test result: ok. X passed; 0 failed
```

**Checklist:**

- [ ] Rust toolchain >= 1.70
- [ ] `clippy` and `rustfmt` components installed
- [ ] `cargo-tarpaulin` installed (optional but recommended)
- [ ] Branch name: `pr-file-tools-modularization-<issue>`
- [ ] All existing tests pass
- [ ] No uncommitted changes
- [ ] Read and understood AGENTS.md requirements

## Current State Analysis

### Existing Infrastructure

**Consolidated File Operations (`src/tools/file_ops.rs` lines 1-914):**

- `FileOpsTool` - Multi-operation tool with enum-based dispatch (list, read, write, delete, diff)
- `FileOpsReadOnlyTool` - Planning mode variant (list, read, search)
- Shared path validation logic (prevent traversal, absolute paths, home directory)
- Size limits enforcement via `ToolsConfig` (defined in `src/config.rs` lines 376-398)
- 914 lines including tests

**Tool Registration Pattern (`src/tools/registry_builder.rs`):**

- `ToolRegistry` in `src/tools/mod.rs` lines 270-377 manages tool collection
- `ToolRegistryBuilder` (`src/tools/registry_builder.rs` lines 1-220) constructs mode-specific registries
- Tools implement `ToolExecutor` trait (defined in `src/tools/mod.rs` lines 235-269) with `tool_definition()` and `async execute()`
- Each tool is Arc-wrapped for thread-safe sharing
- Planning mode registration: `build_for_planning()` at line 128
- Write mode registration: `build_for_write()` at line 162

**Existing Individual Tools:**

- `fetch.rs` - HTTP content retrieval (353 lines, well-structured)
- `grep.rs` - Regex search with pagination (344 lines)
- `terminal.rs` - Command execution with validation (366 lines)
- `subagent.rs` - Recursive agent delegation (470 lines)
- `parallel_subagent.rs` - Parallel task execution (176 lines)

**ToolResult Structure (`src/tools/mod.rs` lines 94-196):**

```rust
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub truncated: bool,
    pub metadata: HashMap<String, String>,
}

// Key methods:
// - ToolResult::success(output: impl Into<String>) -> Self
// - ToolResult::error(error: impl Into<String>) -> Self
// - truncate_if_needed(max_size: usize) -> Self
// - with_metadata(key: String, value: String) -> Self
```

### Identified Issues

1. **Monolithic Design:** Single `file_ops.rs` handles 5+ operations via enum dispatch, making it hard to extend
2. **API Ambiguity:** Clients must specify `operation` parameter and remember which params apply to which operation
3. **Testing Complexity:** Single test suite must cover all operations and their interactions
4. **Missing Tools:** Lack of `copy_path`, `move_path`, `find_path`, `create_directory`, `edit_file`
5. **Limited `read_file`:** No line range support (`start_line`, `end_line`) or image file reading (PNG, JPEG, WebP, GIF, BMP, TIFF)
6. **Code Duplication:** `validate_path()` duplicated in `FileOpsTool` and `FileOpsReadOnlyTool`
7. **Registry Coupling:** `registry_builder.rs` hardcodes `file_ops` as single tool
8. **Missing Error Types:** No dedicated error types using `thiserror` pattern
9. **Incomplete Documentation:** Missing `///` doc comments with runnable examples

## Implementation Phases

### Phase 1: Shared Infrastructure

**Goal:** Extract common functionality into reusable utilities module before splitting tools.

**Dependencies:** None (foundation phase)

**Estimated Effort:** ~400 lines of code with tests

#### Task 1.1: Create Path Validation Utilities

**File to create:** `src/tools/file_utils.rs`

**Implementation Requirements:**

1. **Error Type Definition (MANDATORY - AGENTS.md compliance):**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FileUtilsError {
    #[error("Path traversal attempt detected: {0}")]
    PathTraversal(String),

    #[error("Absolute path not allowed: {0}")]
    AbsolutePath(String),

    #[error("Path outside working directory: {0}")]
    OutsideWorkingDir(String),

    #[error("File size {0} bytes exceeds maximum {1} bytes")]
    FileTooLarge(u64, u64),

    #[error("Parent directory creation failed: {0}")]
    ParentDirCreation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

2. **Path Validator Structure:**

````rust
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
        // Implementation: Extract from FileOpsTool::validate_path()
        // 1. Check for absolute path
        // 2. Check for home directory expansion
        // 3. Check for ".." sequences
        // 4. Resolve against working_dir
        // 5. Verify resolved path is within working_dir
    }
}
````

3. **Utility Functions:**

````rust
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
/// assert!(path.parent().unwrap().exists());
/// # });
/// ```
pub async fn ensure_parent_dirs(path: &Path) -> Result<(), FileUtilsError> {
    // Implementation
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
/// assert!(result.is_ok());
/// # });
/// ```
pub async fn check_file_size(path: &Path, max_size: u64) -> Result<u64, FileUtilsError> {
    // Implementation
}
````

4. **Testing Requirements (MANDATORY naming pattern):**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Pattern: test_{function}_{condition}_{expected}

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
    }

    #[tokio::test]
    async fn test_check_file_size_with_large_file_returns_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("large.txt");
        tokio::fs::write(&path, "x".repeat(2000)).await.unwrap();
        let result = check_file_size(&path, 1000).await;
        assert!(matches!(result, Err(FileUtilsError::FileTooLarge(_, _))));
    }
}
```

**Expected File Size:** ~250 lines with tests

#### Task 1.2: Create File Metadata and Image Utilities

**File to create:** `src/tools/file_metadata.rs`

**Dependencies to add to `Cargo.toml`:**

```toml
# Verify current dependencies first
# Run: grep "image = " Cargo.toml || echo "Not found - safe to add"

[dependencies]
image = "0.25"   # Image decoding for PNG, JPEG, WebP, GIF, BMP, TIFF
base64 = "0.22"  # Base64 encoding for image data
```

**Verification Commands:**

```bash
# After adding dependencies
cargo update
cargo check --all-targets --all-features
# EXPECTED: Finished `dev` profile [unoptimized + debuginfo] target(s)
```

**Implementation Requirements:**

1. **Error Type Definition:**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FileMetadataError {
    #[error("Unsupported image format: {0}")]
    UnsupportedImageFormat(String),

    #[error("Image decoding failed: {0}")]
    ImageDecodingFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
}
```

2. **Type Definitions:**

```rust
/// File type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    File,
    Directory,
    Symlink,
    Image(ImageFormat),
}

/// Supported image formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
}

/// File metadata structure
///
/// Contains information about a file including size, modification time,
/// and permissions.
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size: u64,
    pub modified: std::time::SystemTime,
    pub readonly: bool,
    pub file_type: FileType,
}

/// Image metadata structure
///
/// Contains image-specific information including dimensions and format.
#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}
```

3. **Utility Functions:**

````rust
/// Detects the file type at the given path
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns the FileType
///
/// # Errors
///
/// Returns `FileMetadataError` if file metadata cannot be read
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_metadata::get_file_type;
/// use std::path::Path;
///
/// # tokio_test::block_on(async {
/// let file_type = get_file_type(Path::new("image.png")).await.unwrap();
/// # });
/// ```
pub async fn get_file_type(path: &Path) -> Result<FileType, FileMetadataError> {
    // Implementation
}

/// Retrieves detailed file information
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns FileInfo with metadata
///
/// # Errors
///
/// Returns `FileMetadataError` if metadata cannot be read
pub async fn get_file_info(path: &Path) -> Result<FileInfo, FileMetadataError> {
    // Implementation
}

/// Checks if a file is an image based on extension and magic bytes
///
/// # Arguments
///
/// * `path` - The file path to check
///
/// # Returns
///
/// Returns true if the file is a supported image format
pub fn is_image_file(path: &Path) -> bool {
    // Check extension first (fast path)
    // Supported: .png, .jpg, .jpeg, .webp, .gif, .bmp, .tiff, .tif
}

/// Detects content type using file extension and magic bytes
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns MIME type string (e.g., "image/png", "text/plain")
pub async fn detect_content_type(path: &Path) -> Result<String, FileMetadataError> {
    // Implementation
}

/// Reads an image file and returns base64-encoded data with metadata
///
/// Decodes the image using the `image` crate, validates format,
/// and encodes as base64 for transmission.
///
/// # Arguments
///
/// * `path` - The image file path
///
/// # Returns
///
/// Returns tuple of (base64_data, ImageMetadata)
///
/// # Errors
///
/// Returns `FileMetadataError` if:
/// - File is not a supported image format
/// - Image decoding fails
/// - IO error occurs
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_metadata::read_image_as_base64;
/// use std::path::Path;
///
/// # tokio_test::block_on(async {
/// let (data, metadata) = read_image_as_base64(Path::new("test.png")).await.unwrap();
/// assert!(data.starts_with("iVBORw0KGgo")); // PNG magic bytes in base64
/// # });
/// ```
pub async fn read_image_as_base64(path: &Path) -> Result<(String, ImageMetadata), FileMetadataError> {
    // Implementation:
    // 1. Read file bytes
    // 2. Decode using image::load_from_memory()
    // 3. Extract metadata (width, height, format)
    // 4. Encode to base64
    // 5. Return (base64_string, metadata)
}
````

4. **Testing Requirements:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_get_file_type_with_regular_file_returns_file() {
        // Test implementation
    }

    #[tokio::test]
    async fn test_get_file_type_with_directory_returns_directory() {
        // Test implementation
    }

    #[test]
    fn test_is_image_file_with_png_returns_true() {
        let path = Path::new("test.png");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_text_file_returns_false() {
        let path = Path::new("test.txt");
        assert!(!is_image_file(path));
    }

    #[tokio::test]
    async fn test_read_image_as_base64_with_valid_png_succeeds() {
        // Create minimal valid PNG in test
        // Verify base64 output and metadata
    }

    #[tokio::test]
    async fn test_read_image_as_base64_with_invalid_image_returns_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("invalid.png");
        tokio::fs::write(&path, "not an image").await.unwrap();
        let result = read_image_as_base64(&path).await;
        assert!(result.is_err());
    }
}
```

**Expected File Size:** ~280 lines with tests

#### Task 1.3: Define Tool Name Constants

**File to modify:** `src/tools/mod.rs`

**Location:** Add after line 48 (after existing re-exports)

**Exact code to add:**

```rust
// Tool name constants for type-safe registration and filtering
// These constants ensure consistent tool naming across the codebase
pub const TOOL_READ_FILE: &str = "read_file";
pub const TOOL_WRITE_FILE: &str = "write_file";
pub const TOOL_DELETE_PATH: &str = "delete_path";
pub const TOOL_LIST_DIRECTORY: &str = "list_directory";
pub const TOOL_COPY_PATH: &str = "copy_path";
pub const TOOL_MOVE_PATH: &str = "move_path";
pub const TOOL_CREATE_DIRECTORY: &str = "create_directory";
pub const TOOL_FIND_PATH: &str = "find_path";
pub const TOOL_EDIT_FILE: &str = "edit_file";

// Deprecated tool names (Phase 5 migration)
#[deprecated(note = "Use individual file tools instead")]
pub const TOOL_FILE_OPS: &str = "file_ops";
```

#### Task 1.4: Update Module Exports

**File to modify:** `src/tools/mod.rs`

**Location:** Add after line 17 (after `pub mod terminal;`)

**Exact code to add:**

```rust
// Phase 1: Shared utilities
pub mod file_utils;
pub mod file_metadata;
```

**Location:** Add after tool name constants (from Task 1.3)

**Exact code to add:**

```rust
// Re-export shared utilities
pub use file_utils::{
    PathValidator, FileUtilsError, ensure_parent_dirs, check_file_size,
};
pub use file_metadata::{
    FileType, ImageFormat, FileInfo, ImageMetadata, FileMetadataError,
    get_file_type, get_file_info, is_image_file, detect_content_type,
    read_image_as_base64,
};
```

#### Task 1.5: Testing Requirements

**Test Coverage Goals:**

- `file_utils.rs`: >85% coverage (security-critical code)
- `file_metadata.rs`: >80% coverage

**Integration Tests:**

Create `tests/file_utils_integration_test.rs`:

```rust
use xzatoma::tools::file_utils::PathValidator;
use tempfile::TempDir;
use std::path::PathBuf;

#[test]
fn test_path_validator_integration_with_nested_paths() {
    let temp = TempDir::new().unwrap();
    let validator = PathValidator::new(temp.path().to_path_buf());

    // Test valid nested path
    let result = validator.validate("src/tools/file_utils.rs");
    assert!(result.is_ok());

    // Test invalid traversal
    let result = validator.validate("src/../../etc/passwd");
    assert!(result.is_err());
}
```

#### Task 1.6: Deliverables

- [ ] `src/tools/file_utils.rs` (~250 lines with tests)
- [ ] `src/tools/file_metadata.rs` (~280 lines with tests)
- [ ] Updated `src/tools/mod.rs` (added module declarations and exports)
- [ ] Updated `Cargo.toml` (added `image = "0.25"` and `base64 = "0.22"`)
- [ ] Integration test `tests/file_utils_integration_test.rs` (~50 lines)

**Total New Code:** ~580 lines

#### Task 1.7: Validation Commands

**Execute these commands in sequence. ALL must pass:**

```bash
# 1. Format code
cargo fmt --all
# EXPECTED OUTPUT: (no output = success)

# 2. Check compilation
cargo check --all-targets --all-features
# EXPECTED OUTPUT:
#   Finished `dev` profile [unoptimized + debuginfo] target(s) in X.XXs

# 3. Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings
# EXPECTED OUTPUT:
#   Finished `dev` profile [unoptimized + debuginfo] target(s) in X.XXs

# 4. Run new tests
cargo test --all-features file_utils file_metadata
# EXPECTED OUTPUT:
#   test result: ok. X passed; 0 failed; 0 ignored

# 5. Verify module exports
cargo test --all-features --doc
# EXPECTED OUTPUT: doc tests pass

# 6. Optional: Check coverage
cargo tarpaulin --out Stdout --lib --packages xzatoma -- file_utils file_metadata
# EXPECTED OUTPUT: Coverage >= 80.00%
```

#### Task 1.8: Architecture Compliance Verification

**Verify module boundaries per AGENTS.md:**

```bash
# Tools modules must NOT import from agent/ or providers/
grep -rn "use crate::agent\|use crate::providers" src/tools/file_utils.rs src/tools/file_metadata.rs
# EXPECTED OUTPUT: (no matches)

# Verify only allowed dependencies
grep -rn "use crate::" src/tools/file_utils.rs
# EXPECTED OUTPUT: Only "use crate::error::" or similar error imports

grep -rn "use crate::" src/tools/file_metadata.rs
# EXPECTED OUTPUT: Only "use crate::error::" or similar error imports
```

#### Task 1.9: Success Criteria

**All of the following must be true:**

- [ ] All validation commands pass with zero errors and warnings
- [ ] `PathValidator` validates paths with zero false positives/negatives
- [ ] Image decoding works for all supported formats (PNG, JPEG, WebP, GIF, BMP, TIFF)
- [ ] Test coverage >80% for both new modules
- [ ] No breaking changes to existing `file_ops.rs` (utilities are additive)
- [ ] Architecture boundaries respected (no imports from agent/ or providers/)
- [ ] All public functions have `///` doc comments with examples
- [ ] All tests follow naming pattern: `test_{function}_{condition}_{expected}`

#### Task 1.10: Rollback Procedure

**If validation fails:**

```bash
# 1. Stash changes
git stash save "Phase 1 incomplete - validation failed"

# 2. Verify rollback
cargo test --all-features
# Should pass with original code

# 3. Review errors
git stash show -p

# 4. Fix issues and re-apply
git stash pop
```

### Phase 2: Core File Operation Tools

**Goal:** Implement fundamental file operations as individual tools (read, write, delete, list).

**Dependencies:** Phase 1 must be complete and validated

**Estimated Effort:** ~850 lines of code with tests

#### Task 2.1: Implement `read_file` Tool

**File to create:** `src/tools/read_file.rs`

**Exact Implementation Structure:**

````rust
//! Read file tool implementation
//!
//! Provides file reading with line range support and image decoding.

use crate::config::ToolsConfig;
use crate::error::Result;
use crate::tools::file_metadata::{is_image_file, read_image_as_base64};
use crate::tools::file_utils::{check_file_size, PathValidator};
use crate::tools::{ToolExecutor, ToolResult, TOOL_READ_FILE};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Read file tool for reading file contents or returning file outlines
///
/// Supports:
/// - Reading full file contents
/// - Reading specific line ranges (start_line, end_line)
/// - Decoding and base64-encoding image files
/// - Large file outline mode (>500KB without line range)
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::read_file::ReadFileTool;
/// use xzatoma::tools::ToolExecutor;
/// use xzatoma::config::ToolsConfig;
/// use std::path::PathBuf;
/// use std::sync::Arc;
///
/// # tokio_test::block_on(async {
/// let tool = ReadFileTool::new(
///     PathBuf::from("/project"),
///     Arc::new(ToolsConfig::default())
/// );
///
/// let result = tool.execute(serde_json::json!({
///     "path": "src/main.rs"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct ReadFileTool {
    path_validator: PathValidator,
    config: Arc<ToolsConfig>,
}

impl ReadFileTool {
    /// Creates a new read file tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The base working directory
    /// * `config` - Tools configuration including size limits
    ///
    /// # Returns
    ///
    /// Returns a new ReadFileTool instance
    pub fn new(working_dir: PathBuf, config: Arc<ToolsConfig>) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
            config,
        }
    }
}

/// Parameters for read_file tool
#[derive(Debug, Deserialize)]
struct ReadFileParams {
    /// Relative path to the file
    path: String,
    /// Optional starting line number (1-based)
    #[serde(default)]
    start_line: Option<u32>,
    /// Optional ending line number (1-based, inclusive)
    #[serde(default)]
    end_line: Option<u32>,
}

#[async_trait]
impl ToolExecutor for ReadFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_READ_FILE,
            "description": "Reads the content of a file. For large files (>500KB), returns an outline with line numbers. Supports reading specific line ranges. Automatically decodes image files (PNG, JPEG, WebP, GIF, BMP, TIFF) as base64.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file within the working directory"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "Optional starting line number (1-based, inclusive)",
                        "minimum": 1
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Optional ending line number (1-based, inclusive)",
                        "minimum": 1
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        // 1. Deserialize and validate parameters
        let params: ReadFileParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // 2. Validate line range if provided
        if let (Some(start), Some(end)) = (params.start_line, params.end_line) {
            if start > end {
                return Ok(ToolResult::error(
                    format!("start_line ({}) must be <= end_line ({})", start, end)
                ));
            }
        }

        // 3. Validate path
        let file_path = match self.path_validator.validate(&params.path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid path: {}", e))),
        };

        // 4. Check if file exists
        if !file_path.exists() {
            return Ok(ToolResult::error(format!("File not found: {}", params.path)));
        }

        // 5. Handle image files
        if is_image_file(&file_path) {
            match read_image_as_base64(&file_path).await {
                Ok((base64_data, metadata)) => {
                    return Ok(ToolResult::success(format!(
                        "Image file ({}x{} {})\n\nBase64 data:\n{}",
                        metadata.width, metadata.height,
                        format!("{:?}", metadata.format).to_lowercase(),
                        base64_data
                    )).with_metadata("content_type".to_string(), "image".to_string())
                    .with_metadata("width".to_string(), metadata.width.to_string())
                    .with_metadata("height".to_string(), metadata.height.to_string()));
                }
                Err(e) => return Ok(ToolResult::error(format!("Failed to read image: {}", e))),
            }
        }

        // 6. Check file size
        let file_size = match check_file_size(&file_path, self.config.max_file_read_size as u64).await {
            Ok(size) => size,
            Err(_) => {
                // File too large - if no line range specified, return outline
                if params.start_line.is_none() && params.end_line.is_none() {
                    return self.generate_outline(&file_path).await;
                }
                // Continue to read specific lines even if file is large
                0 // placeholder, will read lines anyway
            }
        };

        // 7. Read file content
        let content = match tokio::fs::read_to_string(&file_path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::error(format!("Failed to read file: {}", e))),
        };

        // 8. Apply line range if specified
        let output = if let (Some(start), end_opt) = (params.start_line, params.end_line) {
            let lines: Vec<&str> = content.lines().collect();
            let start_idx = (start.saturating_sub(1)) as usize;
            let end_idx = end_opt.map(|e| e as usize).unwrap_or(lines.len()).min(lines.len());

            if start_idx >= lines.len() {
                return Ok(ToolResult::error(format!(
                    "start_line {} exceeds file length ({})",
                    start, lines.len()
                )));
            }

            lines[start_idx..end_idx].join("\n")
        } else {
            content
        };

        Ok(ToolResult::success(output)
            .with_metadata("file_size".to_string(), file_size.to_string()))
    }
}

impl ReadFileTool {
    /// Generates outline for large files
    ///
    /// Returns first 100 and last 100 lines with line count
    async fn generate_outline(&self, path: &PathBuf) -> Result<ToolResult> {
        let content = tokio::fs::read_to_string(path).await?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let outline = if total_lines <= 200 {
            content
        } else {
            let first_100 = lines[..100].join("\n");
            let last_100 = lines[total_lines - 100..].join("\n");
            format!(
                "# File outline (total {} lines)\n\n## First 100 lines:\n{}\n\n## Last 100 lines:\n{}",
                total_lines, first_100, last_100
            )
        };

        Ok(ToolResult::success(outline)
            .with_metadata("total_lines".to_string(), total_lines.to_string())
            .with_metadata("outline_mode".to_string(), "true".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Test naming pattern: test_{function}_{condition}_{expected}

    #[tokio::test]
    async fn test_execute_with_valid_text_file_returns_content() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "test content\nline 2").await.unwrap();

        let tool = ReadFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "test.txt"
        })).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("test content"));
    }

    #[tokio::test]
    async fn test_execute_with_line_range_returns_specified_lines() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "line 1\nline 2\nline 3\nline 4").await.unwrap();

        let tool = ReadFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "test.txt",
            "start_line": 2,
            "end_line": 3
        })).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "line 2\nline 3");
    }

    #[tokio::test]
    async fn test_execute_with_invalid_line_range_returns_error() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "content").await.unwrap();

        let tool = ReadFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "test.txt",
            "start_line": 5,
            "end_line": 2
        })).await.unwrap();

        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_execute_with_missing_file_returns_error() {
        let temp = TempDir::new().unwrap();
        let tool = ReadFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "nonexistent.txt"
        })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_with_traversal_attempt_returns_error() {
        let temp = TempDir::new().unwrap();
        let tool = ReadFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "../etc/passwd"
        })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid path"));
    }

    // Add tests for:
    // - Large file outline mode
    // - Image file reading
    // - All supported image formats
}
````

**Expected Size:** ~350 lines with comprehensive tests

#### Task 2.2: Implement `write_file` Tool

**File to create:** `src/tools/write_file.rs`

**Exact Implementation Structure:**

````rust
//! Write file tool implementation

use crate::config::ToolsConfig;
use crate::error::Result;
use crate::tools::file_utils::{ensure_parent_dirs, PathValidator};
use crate::tools::{ToolExecutor, ToolResult, TOOL_WRITE_FILE};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Write file tool for creating or overwriting files
///
/// Creates parent directories automatically if they don't exist.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::write_file::WriteFileTool;
/// # use xzatoma::tools::ToolExecutor;
/// # use xzatoma::config::ToolsConfig;
/// # use std::path::PathBuf;
/// # use std::sync::Arc;
///
/// # tokio_test::block_on(async {
/// let tool = WriteFileTool::new(
///     PathBuf::from("/project"),
///     Arc::new(ToolsConfig::default())
/// );
///
/// let result = tool.execute(serde_json::json!({
///     "path": "src/new_file.rs",
///     "content": "fn main() {}"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct WriteFileTool {
    path_validator: PathValidator,
    config: Arc<ToolsConfig>,
}

impl WriteFileTool {
    /// Creates a new write file tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The base working directory
    /// * `config` - Tools configuration
    ///
    /// # Returns
    ///
    /// Returns a new WriteFileTool instance
    pub fn new(working_dir: PathBuf, config: Arc<ToolsConfig>) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
            config,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WriteFileParams {
    path: String,
    content: String,
}

#[async_trait]
impl ToolExecutor for WriteFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_WRITE_FILE,
            "description": "Writes content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file"
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
        let params: WriteFileParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let file_path = match self.path_validator.validate(&params.path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid path: {}", e))),
        };

        // Create parent directories
        if let Err(e) = ensure_parent_dirs(&file_path).await {
            return Ok(ToolResult::error(format!("Failed to create parent directories: {}", e)));
        }

        // Write file
        match tokio::fs::write(&file_path, params.content.as_bytes()).await {
            Ok(_) => Ok(ToolResult::success(format!(
                "Successfully wrote {} bytes to {}",
                params.content.len(),
                params.path
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_new_file_creates_file() {
        let temp = TempDir::new().unwrap();
        let tool = WriteFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "test.txt",
            "content": "test content"
        })).await.unwrap();

        assert!(result.success);

        let written = tokio::fs::read_to_string(temp.path().join("test.txt")).await.unwrap();
        assert_eq!(written, "test content");
    }

    #[tokio::test]
    async fn test_execute_with_existing_file_overwrites() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "old content").await.unwrap();

        let tool = WriteFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "test.txt",
            "content": "new content"
        })).await.unwrap();

        assert!(result.success);

        let written = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(written, "new content");
    }

    #[tokio::test]
    async fn test_execute_with_nested_path_creates_directories() {
        let temp = TempDir::new().unwrap();
        let tool = WriteFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "nested/deep/test.txt",
            "content": "content"
        })).await.unwrap();

        assert!(result.success);
        assert!(temp.path().join("nested/deep/test.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_invalid_path_returns_error() {
        let temp = TempDir::new().unwrap();
        let tool = WriteFileTool::new(
            temp.path().to_path_buf(),
            Arc::new(ToolsConfig::default())
        );

        let result = tool.execute(serde_json::json!({
            "path": "../etc/passwd",
            "content": "malicious"
        })).await.unwrap();

        assert!(!result.success);
    }
}
````

**Expected Size:** ~220 lines with tests

#### Task 2.3: Implement `delete_path` Tool

**File to create:** `src/tools/delete_path.rs`

**Exact Implementation Structure:**

````rust
//! Delete path tool implementation

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_DELETE_PATH};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

/// Delete path tool for removing files and directories
///
/// Supports recursive directory deletion with safety checks.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::delete_path::DeletePathTool;
/// # use xzatoma::tools::ToolExecutor;
/// # use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let tool = DeletePathTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "path": "temp_file.txt"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct DeletePathTool {
    path_validator: PathValidator,
}

impl DeletePathTool {
    /// Creates a new delete path tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The base working directory
    ///
    /// # Returns
    ///
    /// Returns a new DeletePathTool instance
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeletePathParams {
    path: String,
    #[serde(default)]
    recursive: bool,
}

#[async_trait]
impl ToolExecutor for DeletePathTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_DELETE_PATH,
            "description": "Deletes a file or directory. For directories, set recursive=true to delete contents.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the file or directory"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to delete directory contents recursively (default: false)",
                        "default": false
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: DeletePathParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let target_path = match self.path_validator.validate(&params.path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid path: {}", e))),
        };

        if !target_path.exists() {
            return Ok(ToolResult::error(format!("Path not found: {}", params.path)));
        }

        if target_path.is_file() {
            match tokio::fs::remove_file(&target_path).await {
                Ok(_) => Ok(ToolResult::success(format!("Deleted file: {}", params.path))),
                Err(e) => Ok(ToolResult::error(format!("Failed to delete file: {}", e))),
            }
        } else if target_path.is_dir() {
            if !params.recursive {
                return Ok(ToolResult::error(
                    format!("Path is a directory. Set recursive=true to delete: {}", params.path)
                ));
            }

            match tokio::fs::remove_dir_all(&target_path).await {
                Ok(_) => Ok(ToolResult::success(format!("Deleted directory: {}", params.path))),
                Err(e) => Ok(ToolResult::error(format!("Failed to delete directory: {}", e))),
            }
        } else {
            Ok(ToolResult::error(format!("Unsupported file type: {}", params.path)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_existing_file_deletes_successfully() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        tokio::fs::write(&file_path, "content").await.unwrap();

        let tool = DeletePathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": "test.txt"
        })).await.unwrap();

        assert!(result.success);
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_execute_with_directory_and_recursive_false_returns_error() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("test_dir");
        tokio::fs::create_dir(&dir_path).await.unwrap();

        let tool = DeletePathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": "test_dir",
            "recursive": false
        })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("recursive=true"));
    }

    #[tokio::test]
    async fn test_execute_with_directory_and_recursive_true_deletes_tree() {
        let temp = TempDir::new().unwrap();
        let dir_path = temp.path().join("test_dir");
        tokio::fs::create_dir(&dir_path).await.unwrap();
        tokio::fs::write(dir_path.join("file.txt"), "content").await.unwrap();

        let tool = DeletePathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": "test_dir",
            "recursive": true
        })).await.unwrap();

        assert!(result.success);
        assert!(!dir_path.exists());
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_path_returns_error() {
        let temp = TempDir::new().unwrap();
        let tool = DeletePathTool::new(temp.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "path": "nonexistent.txt"
        })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }
}
````

**Expected Size:** ~240 lines with tests

#### Task 2.4: Implement `list_directory` Tool

**File to create:** `src/tools/list_directory.rs`

**Exact Implementation Structure:**

````rust
//! List directory tool implementation

use crate::error::Result;
use crate::tools::file_utils::PathValidator;
use crate::tools::{ToolExecutor, ToolResult, TOOL_LIST_DIRECTORY};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use walkdir::WalkDir;

/// List directory tool for listing directory contents
///
/// Supports recursive listing and pattern filtering.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::list_directory::ListDirectoryTool;
/// # use xzatoma::tools::ToolExecutor;
/// # use std::path::PathBuf;
///
/// # tokio_test::block_on(async {
/// let tool = ListDirectoryTool::new(PathBuf::from("/project"));
///
/// let result = tool.execute(serde_json::json!({
///     "path": "src"
/// })).await.unwrap();
///
/// assert!(result.success);
/// # });
/// ```
pub struct ListDirectoryTool {
    path_validator: PathValidator,
}

impl ListDirectoryTool {
    /// Creates a new list directory tool
    ///
    /// # Arguments
    ///
    /// * `working_dir` - The base working directory
    ///
    /// # Returns
    ///
    /// Returns a new ListDirectoryTool instance
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListDirectoryParams {
    path: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    pattern: Option<String>,
}

#[async_trait]
impl ToolExecutor for ListDirectoryTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": TOOL_LIST_DIRECTORY,
            "description": "Lists files and directories. Supports recursive listing and regex pattern filtering.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the directory"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Whether to list recursively (default: false)",
                        "default": false
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Optional regex pattern to filter results"
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let params: ListDirectoryParams = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let dir_path = match self.path_validator.validate(&params.path) {
            Ok(path) => path,
            Err(e) => return Ok(ToolResult::error(format!("Invalid path: {}", e))),
        };

        if !dir_path.exists() {
            return Ok(ToolResult::error(format!("Directory not found: {}", params.path)));
        }

        if !dir_path.is_dir() {
            return Ok(ToolResult::error(format!("Path is not a directory: {}", params.path)));
        }

        // Compile regex pattern if provided
        let pattern_regex = if let Some(pattern_str) = &params.pattern {
            match regex::Regex::new(pattern_str) {
                Ok(re) => Some(re),
                Err(e) => return Ok(ToolResult::error(format!("Invalid regex pattern: {}", e))),
            }
        } else {
            None
        };

        // Walk directory
        let max_depth = if params.recursive { usize::MAX } else { 1 };
        let mut entries = Vec::new();

        for entry in WalkDir::new(&dir_path).max_depth(max_depth).sort_by_file_name() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Warning: Failed to read entry: {}", e);
                    continue;
                }
            };

            // Skip the root directory itself
            if entry.path() == dir_path {
                continue;
            }

            // Get relative path
            let relative_path = match entry.path().strip_prefix(&dir_path) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            // Apply pattern filter
            if let Some(ref regex) = pattern_regex {
                if !regex.is_match(&relative_path) {
                    continue;
                }
            }

            // Format entry with type indicator
            let type_indicator = if entry.file_type().is_dir() {
                "/"
            } else if entry.file_type().is_symlink() {
                "@"
            } else {
                ""
            };

            entries.push(format!("{}{}", relative_path, type_indicator));
        }

        if entries.is_empty() {
            Ok(ToolResult::success("(empty directory)".to_string()))
        } else {
            Ok(ToolResult::success(entries.join("\n"))
                .with_metadata("count".to_string(), entries.len().to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_execute_with_directory_lists_contents() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file1.txt"), "").await.unwrap();
        tokio::fs::write(temp.path().join("file2.txt"), "").await.unwrap();
        tokio::fs::create_dir(temp.path().join("subdir")).await.unwrap();

        let tool = ListDirectoryTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": "."
        })).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(result.output.contains("subdir/"));
    }

    #[tokio::test]
    async fn test_execute_with_recursive_lists_nested_files() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir(temp.path().join("subdir")).await.unwrap();
        tokio::fs::write(temp.path().join("subdir/nested.txt"), "").await.unwrap();

        let tool = ListDirectoryTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": ".",
            "recursive": true
        })).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("subdir/nested.txt"));
    }

    #[tokio::test]
    async fn test_execute_with_pattern_filters_results() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("test.txt"), "").await.unwrap();
        tokio::fs::write(temp.path().join("test.rs"), "").await.unwrap();
        tokio::fs::write(temp.path().join("other.md"), "").await.unwrap();

        let tool = ListDirectoryTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "path": ".",
            "pattern": r"\.rs$"
        })).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("test.rs"));
        assert!(!result.output.contains("test.txt"));
        assert!(!result.output.contains("other.md"));
    }

    #[tokio::test]
    async fn test_execute_with_empty_directory_returns_empty_message() {
        let temp = TempDir::new().unwrap();
        let tool = ListDirectoryTool::new(temp.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "path": "."
        })).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output, "(empty directory)");
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_directory_returns_error() {
        let temp = TempDir::new().unwrap();
        let tool = ListDirectoryTool::new(temp.path().to_path_buf());

        let result = tool.execute(serde_json::json!({
            "path": "nonexistent"
        })).await.unwrap();

        assert!(!result.success);
    }
}
````

**Expected Size:** ~280 lines with tests

#### Task 2.5: Integration and Registry Updates

**File to modify:** `src/tools/mod.rs`

**Location 1:** Add after line 17 (after Phase 1 modules)

```rust
// Phase 2: Core file operation tools
pub mod read_file;
pub mod write_file;
pub mod delete_path;
pub mod list_directory;
```

**Location 2:** Add after tool name constants

```rust
// Re-export core file operation tools
pub use read_file::ReadFileTool;
pub use write_file::WriteFileTool;
pub use delete_path::DeletePathTool;
pub use list_directory::ListDirectoryTool;
```

**File to modify:** `src/tools/registry_builder.rs`

**Location:** Function `build_for_planning()` at line 128

**Find this code (lines 136-141):**

```rust
// Register read-only file operations tool
let file_tool_readonly =
    FileOpsReadOnlyTool::new(self.working_dir.clone(), self.tools_config.clone());
let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool_readonly);
registry.register("file_ops", file_tool_executor);
```

**Replace with:**

```rust
// Phase 2: Register read-only file operation tools for planning mode
use crate::tools::{ReadFileTool, ListDirectoryTool, TOOL_READ_FILE, TOOL_LIST_DIRECTORY};

let read_file_tool = Arc::new(ReadFileTool::new(
    self.working_dir.clone(),
    Arc::new(self.tools_config.clone()),
));
registry.register(TOOL_READ_FILE, read_file_tool);

let list_dir_tool = Arc::new(ListDirectoryTool::new(self.working_dir.clone()));
registry.register(TOOL_LIST_DIRECTORY, list_dir_tool);

// Keep old file_ops for backward compatibility during Phase 2-4
// TODO: Remove in Phase 5
let file_tool_readonly =
    FileOpsReadOnlyTool::new(self.working_dir.clone(), self.tools_config.clone());
let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool_readonly);
registry.register("file_ops", file_tool_executor);
```

**Location:** Function `build_for_write()` at line 162

**Find this code (lines 168-171):**

```rust
// Register full file operations tool
let file_tool = FileOpsTool::new(self.working_dir.clone(), self.tools_config.clone());
let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool);
registry.register("file_ops", file_tool_executor);
```

**Replace with:**

```rust
// Phase 2: Register all file operation tools for write mode
use crate::tools::{
    ReadFileTool, WriteFileTool, DeletePathTool, ListDirectoryTool,
    TOOL_READ_FILE, TOOL_WRITE_FILE, TOOL_DELETE_PATH, TOOL_LIST_DIRECTORY,
};

let working_dir = self.working_dir.clone();
let tools_config = Arc::new(self.tools_config.clone());

registry.register(
    TOOL_READ_FILE,
    Arc::new(ReadFileTool::new(working_dir.clone(), tools_config.clone())),
);
registry.register(
    TOOL_WRITE_FILE,
    Arc::new(WriteFileTool::new(working_dir.clone(), tools_config.clone())),
);
registry.register(
    TOOL_DELETE_PATH,
    Arc::new(DeletePathTool::new(working_dir.clone())),
);
registry.register(
    TOOL_LIST_DIRECTORY,
    Arc::new(ListDirectoryTool::new(working_dir.clone())),
);

// Keep old file_ops for backward compatibility during Phase 2-4
// TODO: Remove in Phase 5
let file_tool = FileOpsTool::new(self.working_dir.clone(), self.tools_config.clone());
let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool);
registry.register("file_ops", file_tool_executor);
```

#### Task 2.6: Testing Requirements

**Test Coverage Goals:**

- Each tool >80% coverage
- Integration tests for registry containing correct tools

**Integration Test:** Create `tests/phase2_tools_integration_test.rs`

```rust
use xzatoma::chat_mode::{ChatMode, SafetyMode};
use xzatoma::tools::registry_builder::ToolRegistryBuilder;
use xzatoma::tools::{TOOL_READ_FILE, TOOL_WRITE_FILE, TOOL_DELETE_PATH, TOOL_LIST_DIRECTORY};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_planning_mode_registry_contains_read_only_tools() {
    let temp = TempDir::new().unwrap();
    let builder = ToolRegistryBuilder::new(
        ChatMode::Planning,
        SafetyMode::AlwaysConfirm,
        temp.path().to_path_buf(),
    );

    let registry = builder.build_for_planning().unwrap();

    assert!(registry.get(TOOL_READ_FILE).is_some());
    assert!(registry.get(TOOL_LIST_DIRECTORY).is_some());
    assert!(registry.get(TOOL_WRITE_FILE).is_none());
    assert!(registry.get(TOOL_DELETE_PATH).is_none());
}

#[test]
fn test_write_mode_registry_contains_all_file_tools() {
    let temp = TempDir::new().unwrap();
    let builder = ToolRegistryBuilder::new(
        ChatMode::Write,
        SafetyMode::AlwaysConfirm,
        temp.path().to_path_buf(),
    );

    let registry = builder.build_for_write().unwrap();

    assert!(registry.get(TOOL_READ_FILE).is_some());
    assert!(registry.get(TOOL_WRITE_FILE).is_some());
    assert!(registry.get(TOOL_DELETE_PATH).is_some());
    assert!(registry.get(TOOL_LIST_DIRECTORY).is_some());
}

#[tokio::test]
async fn test_end_to_end_file_operations() {
    let temp = TempDir::new().unwrap();
    let builder = ToolRegistryBuilder::new(
        ChatMode::Write,
        SafetyMode::AlwaysConfirm,
        temp.path().to_path_buf(),
    );

    let registry = builder.build_for_write().unwrap();

    // Write file
    let write_tool = registry.get(TOOL_WRITE_FILE).unwrap();
    let result = write_tool.execute(serde_json::json!({
        "path": "test.txt",
        "content": "test content"
    })).await.unwrap();
    assert!(result.success);

    // Read file
    let read_tool = registry.get(TOOL_READ_FILE).unwrap();
    let result = read_tool.execute(serde_json::json!({
        "path": "test.txt"
    })).await.unwrap();
    assert!(result.success);
    assert!(result.output.contains("test content"));

    // List directory
    let list_tool = registry.get(TOOL_LIST_DIRECTORY).unwrap();
    let result = list_tool.execute(serde_json::json!({
        "path": "."
    })).await.unwrap();
    assert!(result.success);
    assert!(result.output.contains("test.txt"));

    // Delete file
    let delete_tool = registry.get(TOOL_DELETE_PATH).unwrap();
    let result = delete_tool.execute(serde_json::json!({
        "path": "test.txt"
    })).await.unwrap();
    assert!(result.success);
}
```

#### Task 2.7: Deliverables

- [ ] `src/tools/read_file.rs` (~350 lines)
- [ ] `src/tools/write_file.rs` (~220 lines)
- [ ] `src/tools/delete_path.rs` (~240 lines)
- [ ] `src/tools/list_directory.rs` (~280 lines)
- [ ] Updated `src/tools/mod.rs` (module declarations and exports)
- [ ] Updated `src/tools/registry_builder.rs` (dual registration during transition)
- [ ] Integration test `tests/phase2_tools_integration_test.rs` (~80 lines)

**Total New Code:** ~1,370 lines

#### Task 2.8: Validation Commands

```bash
# 1. Format code
cargo fmt --all

# 2. Check compilation
cargo check --all-targets --all-features

# 3. Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run Phase 2 tests
cargo test --all-features read_file write_file delete_path list_directory

# 5. Run integration tests
cargo test --all-features phase2_tools_integration

# 6. Verify registry registration
cargo test --all-features registry_builder

# 7. Optional: Coverage check
cargo tarpaulin --out Stdout --lib --packages xzatoma -- read_file write_file delete_path list_directory
```

#### Task 2.9: Success Criteria

- [ ] All validation commands pass
- [ ] Test coverage >80% for each tool
- [ ] Planning mode registry contains only `read_file` and `list_directory`
- [ ] Write mode registry contains all 4 file tools
- [ ] Old `file_ops` tool still works (backward compatibility maintained)
- [ ] End-to-end integration test passes
- [ ] All tools follow AGENTS.md requirements (thiserror, doc comments, test naming)

#### Task 2.10: Rollback Procedure

```bash
# If Phase 2 validation fails
git diff src/tools/registry_builder.rs > phase2_registry_changes.patch
git checkout -- src/tools/registry_builder.rs
# Fix issues in new tools, then re-apply registry changes
git apply phase2_registry_changes.patch
```

### Phase 3: Advanced File Manipulation Tools

**Goal:** Implement file operations for copying, moving, directory creation, and path finding.

**Dependencies:** Phase 2 must be complete and validated

**Estimated Effort:** ~950 lines of code with tests

#### Task 3.1: Implement `copy_path` Tool

**File to create:** `src/tools/copy_path.rs`

**Dependencies:** Add to `Cargo.toml` if not already present:

```toml
[dependencies]
# Verify first: grep "walkdir = " Cargo.toml
walkdir = "2.4"  # For recursive directory copying
```

**Implementation:**

````rust
//! Copy path tool implementation

use crate::error::Result;
use crate::tools::file_utils::{ensure_parent_dirs, PathValidator};
use crate::tools::{ToolExecutor, ToolResult, TOOL_COPY_PATH};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Copy path tool for copying files and directories
///
/// Supports recursive directory copying with overwrite control.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::copy_path::CopyPathTool;
/// # use xzatoma::tools::ToolExecutor;
/// # use std::path::PathBuf;
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
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            path_validator: PathValidator::new(working_dir),
        }
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
            Err(e) => return Ok(ToolResult::error(format!("Invalid destination path: {}", e))),
        };

        if !source.exists() {
            return Ok(ToolResult::error(format!("Source not found: {}", params.source_path)));
        }

        if destination.exists() && !params.overwrite {
            return Ok(ToolResult::error(format!(
                "Destination already exists: {}. Set overwrite=true to replace.",
                params.destination_path
            )));
        }

        // Create destination parent directories
        if let Err(e) = ensure_parent_dirs(&destination).await {
            return Ok(ToolResult::error(format!("Failed to create parent directories: {}", e)));
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
                Err(e) => Ok(ToolResult::error(format!("Failed to copy directory: {}", e))),
            }
        } else {
            Ok(ToolResult::error(format!("Unsupported file type: {}", params.source_path)))
        }
    }
}

impl CopyPathTool {
    async fn copy_directory(&self, source: &PathBuf, destination: &PathBuf) -> Result<usize> {
        let mut count = 0;

        for entry in WalkDir::new(source) {
            let entry = entry?;
            let rel_path = entry.path().strip_prefix(source)?;
            let dest_path = destination.join(rel_path);

            if entry.file_type().is_dir() {
                tokio::fs::create_dir_all(&dest_path).await?;
            } else {
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
        tokio::fs::write(temp.path().join("source.txt"), "content").await.unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "source_path": "source.txt",
            "destination_path": "dest.txt"
        })).await.unwrap();

        assert!(result.success);
        assert!(temp.path().join("dest.txt").exists());

        let content = tokio::fs::read_to_string(temp.path().join("dest.txt")).await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_execute_with_directory_copies_recursively() {
        let temp = TempDir::new().unwrap();
        tokio::fs::create_dir(temp.path().join("source")).await.unwrap();
        tokio::fs::write(temp.path().join("source/file.txt"), "content").await.unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "source_path": "source",
            "destination_path": "dest"
        })).await.unwrap();

        assert!(result.success);
        assert!(temp.path().join("dest/file.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_with_existing_destination_and_overwrite_false_returns_error() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "source").await.unwrap();
        tokio::fs::write(temp.path().join("dest.txt"), "dest").await.unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "source_path": "source.txt",
            "destination_path": "dest.txt",
            "overwrite": false
        })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("overwrite=true"));
    }

    #[tokio::test]
    async fn test_execute_with_existing_destination_and_overwrite_true_succeeds() {
        let temp = TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("source.txt"), "new").await.unwrap();
        tokio::fs::write(temp.path().join("dest.txt"), "old").await.unwrap();

        let tool = CopyPathTool::new(temp.path().to_path_buf());
        let result = tool.execute(serde_json::json!({
            "source_path": "source.txt",
            "destination_path": "dest.txt",
            "overwrite": true
        })).await.unwrap();

        assert!(result.success);

        let content = tokio::fs::read_to_string(temp.path().join("dest.txt")).await.unwrap();
        assert_eq!(content, "new");
    }
}
````

**Expected Size:** ~320 lines with tests

#### Task 3.2: Implement `move_path` Tool

**File to create:** `src/tools/move_path.rs`

**Implementation:** Similar structure to `copy_path`, but use `tokio::fs::rename()` with fallback to copy+delete for cross-filesystem moves.

**Expected Size:** ~300 lines with tests

#### Task 3.3: Implement `create_directory` Tool

**File to create:** `src/tools/create_directory.rs`

**Expected Size:** ~200 lines with tests

#### Task 3.4: Implement `find_path` Tool

**File to create:** `src/tools/find_path.rs`

**Dependencies:** Add to `Cargo.toml`:

```toml
[dependencies]
glob = "0.3"  # For glob pattern matching
```

**Implementation includes:**

- Glob pattern parsing
- Pagination (50 results per page, offset parameter)
- Sorting results alphabetically
- Exclude pattern support (e.g., `**/target/**`, `**/node_modules/**`)

**Expected Size:** ~280 lines with tests

#### Task 3.5: Integration and Registry Updates

Update `src/tools/mod.rs` and `src/tools/registry_builder.rs` to register Phase 3 tools.

#### Task 3.6: Testing Requirements

Integration test covering all Phase 3 tools in `tests/phase3_tools_integration_test.rs`.

#### Task 3.7: Deliverables

- [ ] `src/tools/copy_path.rs` (~320 lines)
- [ ] `src/tools/move_path.rs` (~300 lines)
- [ ] `src/tools/create_directory.rs` (~200 lines)
- [ ] `src/tools/find_path.rs` (~280 lines)
- [ ] Updated `src/tools/mod.rs`
- [ ] Updated `src/tools/registry_builder.rs`
- [ ] Updated `Cargo.toml` (added `glob = "0.3"`)
- [ ] Integration test `tests/phase3_tools_integration_test.rs` (~100 lines)

**Total New Code:** ~1,200 lines

#### Task 3.8: Validation and Success Criteria

Same validation commands as Phase 2, plus:

```bash
cargo test --all-features copy_path move_path create_directory find_path
cargo test --all-features phase3_tools_integration
```

**Success Criteria:**

- [ ] All 4 new tools pass validation
- [ ] Glob patterns work correctly
- [ ] Copy/move handle symlinks and permissions
- [ ] Registry contains 8 file operation tools in Write mode

### Phase 4: Intelligent Editing and Buffer Management

**Goal:** Implement advanced editing features with diff preview.

**Dependencies:** Phase 3 complete

**Estimated Effort:** ~350 lines

#### Task 4.1: Implement `edit_file` Tool

**File to create:** `src/tools/edit_file.rs`

**Uses existing `similar` crate dependency for diff generation.**

**Implementation includes:**

- Three modes: Create, Edit, Overwrite
- Unified diff generation using `similar::TextDiff`
- Display description for user-friendly feedback

**Expected Size:** ~350 lines with tests

#### Task 4.2-4.5: Integration, Testing, Deliverables, Success Criteria

Standard integration into registry and validation.

### Phase 5: Deprecation and Migration

**Goal:** Remove old `file_ops.rs`, update all references, complete clean migration.

**Dependencies:** Phases 1-4 complete and validated

**Estimated Effort:** ~300 lines (documentation + updates)

#### Task 5.1: Update Subagent Tool Filters

**File to modify:** `src/tools/subagent.rs`

**Location:** Function `create_filtered_registry()` (search for `filter_tools`)

**Find code that filters tools:**

```bash
grep -n "file_ops" src/tools/subagent.rs
```

**Update filter list to exclude individual tools:**

```rust
// OLD: Filter out "file_ops"
let excluded_tools = vec!["file_ops", "terminal"];

// NEW: Filter out individual file tools
let excluded_tools = vec![
    TOOL_READ_FILE,
    TOOL_WRITE_FILE,
    TOOL_DELETE_PATH,
    TOOL_LIST_DIRECTORY,
    TOOL_COPY_PATH,
    TOOL_MOVE_PATH,
    TOOL_CREATE_DIRECTORY,
    TOOL_FIND_PATH,
    TOOL_EDIT_FILE,
    "terminal",
];
```

#### Task 5.2: Update Documentation

**Files to create/modify:**

1. **Create:** `docs/explanation/file_tools_architecture.md` (~400 lines)

Content outline:

- Architecture overview
- Modular tool design rationale
- Individual tool descriptions
- Migration from monolithic file_ops
- Security considerations (path validation)
- Performance characteristics

2. **Create:** `docs/reference/file_tools_api.md` (~600 lines)

Content outline:

- API specification for all 9 tools
- Parameter schemas (JSON format)
- Return value formats
- Error codes and messages
- Usage examples for each tool

3. **Update:** `docs/reference/tools_api.md`

Add section for new file tools, mark `file_ops` as deprecated.

4. **Update:** `README.md`

Update tool inventory list.

#### Task 5.3: Deprecate `file_ops.rs`

**Files to delete:**

```bash
# Verify no external dependencies
grep -r "FileOpsTool\|FileOpsReadOnlyTool" src/ tests/ examples/
# Should only show registry_builder.rs (will be updated) and file_ops.rs itself

# Delete the file
rm src/tools/file_ops.rs
```

**File to modify:** `src/tools/mod.rs`

Remove these lines:

```rust
pub mod file_ops;  // DELETE
pub use file_ops::{...};  // DELETE all file_ops exports
```

#### Task 5.4: Update Registry Builder (Final Cleanup)

**File to modify:** `src/tools/registry_builder.rs`

**Remove backward compatibility code added in Phase 2:**

In `build_for_planning()`, **delete:**

```rust
// Keep old file_ops for backward compatibility during Phase 2-4
// TODO: Remove in Phase 5
let file_tool_readonly = ...
registry.register("file_ops", file_tool_executor);
```

In `build_for_write()`, **delete:**

```rust
// Keep old file_ops for backward compatibility during Phase 2-4
// TODO: Remove in Phase 5
let file_tool = ...
registry.register("file_ops", file_tool_executor);
```

**Remove import:**

```rust
use crate::tools::{FileOpsReadOnlyTool, FileOpsTool, ...};  // Delete FileOps* imports
```

#### Task 5.5: Final Testing and Validation

**Detection Commands:**

```bash
# 1. Find any remaining file_ops references
grep -rn "file_ops\|FileOpsTool\|FileOpsReadOnlyTool" src/ tests/
# EXPECTED OUTPUT: (no matches)

# 2. Verify registry only contains individual tools
cargo test --all-features registry_builder -- --nocapture

# 3. Run full test suite
cargo test --all-features

# 4. Verify Planning mode tools
cargo test --all-features test_planning_mode

# 5. Verify Write mode tools
cargo test --all-features test_write_mode

# 6. Run end-to-end integration tests
cargo test --all-features integration
```

**Validation Script:** Create `scripts/validate_phase5.sh`

```bash
#!/bin/bash
set -e

echo "=== Phase 5 Final Validation ==="

echo "[1/8] Checking for file_ops references..."
if grep -rn "file_ops" src/ tests/ --exclude-dir=target | grep -v "DEPRECATED"; then
    echo "ERROR: Found file_ops references"
    exit 1
fi

echo "[2/8] Verifying file_ops.rs deleted..."
if [ -f "src/tools/file_ops.rs" ]; then
    echo "ERROR: file_ops.rs still exists"
    exit 1
fi

echo "[3/8] Formatting..."
cargo fmt --all --check || (echo "Run: cargo fmt --all"; exit 1)

echo "[4/8] Compilation..."
cargo check --all-targets --all-features

echo "[5/8] Linting..."
cargo clippy --all-targets --all-features -- -D warnings

echo "[6/8] Testing..."
cargo test --all-features

echo "[7/8] Verifying tool counts..."
cargo test --all-features test_tool_registry_counts

echo "[8/8] Documentation build..."
cargo doc --no-deps

echo "✓ Phase 5 validation complete - ready for production"
```

#### Task 5.6: Deliverables

- [ ] `src/tools/file_ops.rs` **DELETED** (914 lines removed)
- [ ] Updated `src/tools/subagent.rs` (updated filter list)
- [ ] Updated `src/tools/mod.rs` (removed file_ops exports)
- [ ] Updated `src/tools/registry_builder.rs` (removed backward compat code)
- [ ] `docs/explanation/file_tools_architecture.md` (~400 lines)
- [ ] `docs/reference/file_tools_api.md` (~600 lines)
- [ ] Updated `docs/reference/tools_api.md`
- [ ] Updated `README.md`
- [ ] `scripts/validate_phase5.sh` (~50 lines)

**Net Code Change:** -914 + 1,050 = +136 lines (plus ~2,200 lines from Phases 1-4)

#### Task 5.7: Success Criteria

**All of the following must be true:**

- [ ] `cargo build --release` succeeds
- [ ] All tests pass with >80% coverage
- [ ] Zero references to `file_ops`, `FileOpsTool`, or `FileOpsReadOnlyTool` in codebase
- [ ] `src/tools/file_ops.rs` deleted
- [ ] Planning mode registry contains exactly 3 file tools: `read_file`, `list_directory`, `find_path`
- [ ] Write mode registry contains exactly 9 file tools
- [ ] No deprecation warnings in codebase
- [ ] Documentation complete and accurate
- [ ] `scripts/validate_phase5.sh` passes all checks

## Post-Implementation Summary

### Metrics

**Code Statistics:**

- **Removed:** 914 lines (`file_ops.rs`)
- **Added:** ~2,980 lines (9 tools + 2 utilities + tests + docs)
- **Net Change:** +2,066 lines
- **Test Coverage:** >80% for all modules

**Tool Inventory:**

- **Before:** 1 monolithic tool (`file_ops` with 5 operations)
- **After:** 9 granular tools
- **Planning Mode:** 3 read-only tools
- **Write Mode:** 9 full-access tools

### Architecture Improvements

1. **Separation of Concerns:** Each tool has single responsibility
2. **Type Safety:** Tool name constants prevent typos
3. **Clear API Boundaries:** Explicit parameter schemas per tool
4. **Enhanced Security:** Shared path validation prevents vulnerabilities
5. **Better Testing:** Isolated test suites per tool
6. **Feature Parity:** Matches Zed's file operations (minus editor-specific `open_tool`)

### Validation Final Checklist

Run this checklist after Phase 5:

```bash
#!/bin/bash

# Final validation checklist
echo "=== Final Implementation Validation ==="

checks_passed=0
checks_failed=0

check() {
    echo -n "Checking: $1... "
    if eval "$2" > /dev/null 2>&1; then
        echo "✓"
        ((checks_passed++))
    else
        echo "✗ FAILED"
        ((checks_failed++))
    fi
}

check "Rust format" "cargo fmt --all -- --check"
check "Compilation" "cargo check --all-targets --all-features"
check "Clippy (zero warnings)" "cargo clippy --all-targets --all-features -- -D warnings"
check "All tests pass" "cargo test --all-features"
check "Documentation builds" "cargo doc --no-deps"
check "file_ops.rs deleted" "[ ! -f src/tools/file_ops.rs ]"
check "No file_ops references" "! grep -rq 'FileOpsTool\|FileOpsReadOnlyTool' src/ tests/"

echo ""
echo "Results: $checks_passed passed, $checks_failed failed"

if [ $checks_failed -eq 0 ]; then
    echo "✓ All checks passed - implementation complete"
    exit 0
else
    echo "✗ Some checks failed - review and fix"
    exit 1
fi
```

### Rollback Strategy

**If critical issues found post-Phase 5:**

```bash
# Emergency rollback to Phase 4 (before deletion)
git log --oneline | grep "Phase 5"  # Find Phase 5 commit
git revert <commit-hash>  # Revert Phase 5 changes
cargo test --all-features  # Verify rollback successful
```

**For partial rollback (keep new tools, restore old tool):**

```bash
# Restore file_ops.rs from git history
git checkout <commit-before-phase5> -- src/tools/file_ops.rs

# Re-add exports to mod.rs
# Re-add registration to registry_builder.rs

# Both old and new tools available temporarily
cargo test --all-features
```

## Decisions Made

**All open questions have been resolved:**

1. **Image File Support:** ✓ IMPLEMENTED - Full decoding for PNG, JPEG, WebP, GIF, BMP, TIFF using `image = "0.25"` crate with base64 encoding
2. **Large File Outline Mode:** ✓ IMPLEMENTED - Simple line count outline (first 100 + last 100 lines) in `read_file` tool, no language-specific AST parsing
3. **Glob Pattern Complexity:** ✓ IMPLEMENTED - `find_path` includes exclude pattern with `.gitignore` semantics, pagination (50 per page)
4. **Copy/Move Conflict Resolution:** ✓ IMPLEMENTED - Fail by default, optional `overwrite: bool` parameter for both tools
5. **Backward Compatibility:** ✓ NO backward compatibility - clean break, `file_ops.rs` deleted in Phase 5, dual registration in Phases 2-4 only
6. **Error Handling:** ✓ IMPLEMENTED - `thiserror` for all error types per AGENTS.md
7. **Documentation:** ✓ IMPLEMENTED - `///` doc comments with runnable examples for all public items
8. **Test Naming:** ✓ IMPLEMENTED - Pattern `test_{function}_{condition}_{expected}` throughout
9. **Tool Name Constants:** ✓ IMPLEMENTED - Type-safe constants for all tool names
10. **Architecture Boundaries:** ✓ VERIFIED - Tools modules only depend on error types, no agent/provider imports

---

**Implementation Ready:** This plan is now AI-agent executable with machine-parsable instructions, exact code locations, complete function signatures, and automated validation.
