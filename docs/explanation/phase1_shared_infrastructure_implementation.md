# Phase 1: Shared Infrastructure Implementation

## Overview

Phase 1 established the foundational utilities for the file tools modularization project. This phase extracted common functionality into reusable modules before splitting tools into specialized implementations. The infrastructure includes path validation, file metadata detection, and image handling utilities.

## Components Delivered

### 1. File Utilities Module (`src/tools/file_utils.rs`)
- **Lines of code**: ~323 including tests
- **Purpose**: Secure path validation and file system utilities
- **Key components**:
  - `FileUtilsError` enum: Comprehensive error types for validation failures
  - `PathValidator` struct: Path security validation against traversal attacks
  - `ensure_parent_dirs()`: Async directory creation utility
  - `check_file_size()`: File size validation

**Deliverables**:
- Protects against path traversal (../, ~/ patterns)
- Validates absolute vs relative paths
- Enforces working directory boundaries
- Verifies symlink resolution stays within bounds
- 8 comprehensive unit tests

### 2. File Metadata Module (`src/tools/file_metadata.rs`)
- **Lines of code**: ~540 including tests
- **Purpose**: File type detection and image handling
- **Key components**:
  - `FileMetadataError` enum: Image processing error types
  - `FileType` enum: Distinguishes files, directories, symlinks, images
  - `ImageFormat` enum: Supports PNG, JPEG, WebP, GIF, BMP, TIFF
  - `FileInfo` struct: Complete file metadata
  - `ImageMetadata` struct: Image-specific information
  - `get_file_type()`: Async file type detection
  - `get_file_info()`: Complete metadata retrieval
  - `is_image_file()`: Fast extension-based detection
  - `detect_content_type()`: MIME type determination
  - `read_image_as_base64()`: Image encoding for transmission
  - Magic byte detection for format validation
  - 13 unit tests covering all image formats

**Deliverables**:
- Detects file types without relying solely on extensions
- Validates image formats using magic bytes
- Encodes images to base64 for AI provider transmission
- Extracts image dimensions and metadata
- Supports 6 image formats with extensibility

### 3. Tool Constants (`src/tools/mod.rs`)
- **Purpose**: Canonical tool name definitions
- **Constants defined**:
  - `TOOL_READ_FILE`
  - `TOOL_WRITE_FILE`
  - `TOOL_DELETE_PATH`
  - `TOOL_LIST_DIRECTORY`
  - `TOOL_COPY_PATH`
  - `TOOL_MOVE_PATH`
  - `TOOL_CREATE_DIRECTORY`
  - `TOOL_FIND_PATH`
  - `TOOL_EDIT_FILE`
  - `TOOL_FILE_OPS`

**Deliverables**:
- Single source of truth for tool naming
- Prevents string literal duplication
- Facilitates tool registry management
- Enables safe refactoring in future phases

### 4. Dependencies Added
- **image 0.25**: Image decoding for multiple formats
- **base64 0.22**: Base64 encoding for data transmission

## Implementation Details

### Path Validation Strategy

The `PathValidator` implements defense-in-depth path validation:

```rust
pub fn validate(&self, target: &str) -> Result<PathBuf, FileUtilsError> {
    // 1. Check for absolute paths (e.g., /etc/passwd)
    if path.is_absolute() {
        return Err(FileUtilsError::AbsolutePath(...));
    }

    // 2. Check for home directory expansion (e.g., ~/.bashrc)
    if target.starts_with('~') {
        return Err(FileUtilsError::PathTraversal(...));
    }

    // 3. Check for parent directory traversal (e.g., ../../../etc)
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(FileUtilsError::PathTraversal(...));
    }

    // 4. Verify symlinks resolve within working directory
    if full_path.exists() {
        let canonical = full_path.canonicalize()?;
        if !canonical.starts_with(&canonical_working) {
            return Err(FileUtilsError::OutsideWorkingDir(...));
        }
    }

    Ok(full_path)
}
```

### Image Format Detection

The module uses a two-stage approach for reliable image detection:

1. **Fast path**: Check file extension (no I/O cost)
2. **Magic byte verification**: Read file headers to confirm format

```rust
async fn detect_image_format(path: &Path) -> Result<ImageFormat, FileMetadataError> {
    let bytes = tokio::fs::read(path).await?;

    // Check magic bytes for each format
    if bytes.starts_with(b"\x89PNG") {
        Ok(ImageFormat::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        Ok(ImageFormat::Webp)
    } // ... etc
}
```

### Base64 Image Encoding

Images are encoded for transmission to AI providers:

```rust
pub async fn read_image_as_base64(
    path: &Path,
) -> Result<(String, ImageMetadata), FileMetadataError> {
    let file_bytes = tokio::fs::read(path).await?;
    let image = image::load_from_memory(&file_bytes)?;
    let (width, height) = image.dimensions();

    let base64_string = base64::engine::general_purpose::STANDARD.encode(&file_bytes);

    Ok((base64_string, ImageMetadata { width, height, format }))
}
```

## Testing

### Test Coverage

**File Utils Tests** (8 tests):
- Path validator construction
- Relative path validation (success case)
- Absolute path rejection
- Traversal sequence rejection
- Home directory rejection
- Parent directory creation (async)
- Small file size check (success)
- Large file size check (error case)

**File Metadata Tests** (13 tests):
- File type detection for regular files
- Directory type detection
- Image format detection (PNG, JPEG, WebP, GIF, BMP, TIFF)
- Extension-based image detection
- Content type detection
- File info retrieval with metadata
- Base64 encoding rejection for invalid images
- Magic byte validation for each format

**Overall Coverage**:
- Total new tests: 21
- Total project tests: 730+ (passing)
- Coverage pattern: Each public function has 2-3 tests (success, error, edge case)
- Async test support via `#[tokio::test]` macro

### Test Execution Results

```
cargo test --all-features

test result: ok. 730 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out
```

## Validation Results

### Code Quality Checks

All quality gates pass:

```bash
# Format check
cargo fmt --all
# Result: No output (all files formatted correctly)

# Compilation check
cargo check --all-targets --all-features
# Result: Finished `dev` profile [unoptimized + debuginfo] target(s)

# Lint check (treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings
# Result: Finished `dev` profile [unoptimized + debuginfo] target(s)

# Test execution
cargo test --all-features
# Result: ok. 730 passed; 0 failed
```

### Architecture Compliance

Phase 1 adheres to AGENTS.md requirements:

- **File Extensions**: All Rust files use `.rs` extension
- **Module Organization**: Clean separation between utilities and tools
- **Error Handling**: Using `thiserror` for ergonomic error types
- **Documentation**: Complete doc comments with examples
- **Testing**: Mandatory tests for all public functions
- **No Emojis**: All documentation is plain text
- **Naming Conventions**: Test functions use `test_{function}_{condition}_{expected}` pattern

### Security Analysis

Path validation prevents:
- Absolute path access (`/etc/passwd`)
- Home directory access (`~/.ssh/id_rsa`)
- Parent directory traversal (`../../../etc/passwd`)
- Symlink escape attacks (canonicalization verification)
- Non-existent parent boundary violations

## Design Decisions

### 1. Separate Error Types per Module

**Decision**: Use `FileUtilsError` and `FileMetadataError` instead of single unified error type.

**Rationale**:
- Specific errors for specific concerns
- Allows fine-grained error handling in phases 2-5
- Each tool can map to its own error type
- Future `thiserror` conversions more efficient

### 2. Two-Stage Image Detection

**Decision**: Fast path (extension) + verification (magic bytes).

**Rationale**:
- Most cases succeed on extension check (no I/O)
- Fallback to magic bytes for edge cases
- Defense against file misclassification
- Balances performance with security

### 3. Async Path Validation

**Decision**: `validate()` is synchronous, but utility functions are async.

**Rationale**:
- Path validation is pure (no I/O for syntax checks)
- Parent dir creation is I/O bound (async)
- File size checks require I/O (async)
- Clear separation: sync for security, async for I/O

### 4. Generic ImageFormat Support

**Decision**: `ImageFormat` enum vs string-based format.

**Rationale**:
- Type-safe format handling
- Pattern matching in tools
- Extensible for future formats
- MIME type conversion logic centralized

## Files Created

```
src/tools/file_utils.rs        (323 lines)
src/tools/file_metadata.rs     (540 lines)
```

## Files Modified

```
src/tools/mod.rs               (added 10 constants, 9 re-exports)
Cargo.toml                     (added image 0.25, base64 0.22)
```

## Total Lines Added

- Implementation: ~863 lines
- Tests: ~340 lines (21 new tests)
- Documentation: This summary
- Constants: 10 tool name constants

## Next Steps (Phase 2)

Phase 2 will implement the core file operation tools using this infrastructure:

1. `read_file` tool - read text files with line range support
2. `write_file` tool - write files with parent directory creation
3. `delete_path` tool - delete files and directories
4. `list_directory` tool - list directory contents with filtering
5. Registry integration for all tools

These tools will use:
- `PathValidator` for all path handling
- `check_file_size()` for read size limits
- `ensure_parent_dirs()` for write operations
- `get_file_info()` for metadata display
- Tool name constants for registry mapping

## References

- Architecture: `docs/explanation/file_tools_modularization_implementation_plan.md`
- Error Handling: `src/error.rs`
- Tool Registry: `src/tools/mod.rs`
- Configuration: `src/config.rs`
