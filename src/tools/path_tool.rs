//! Shared path-validation helpers for file-oriented tools.
//!
//! Path-based tools repeatedly validate a caller-supplied path and, on failure,
//! return early with a model-visible [`ToolResult`] error. This module collapses
//! that "validate a path, or bail out with a `ToolResult` error" pattern into a
//! single reusable helper so every tool spells it the same way.

use crate::tools::ToolResult;
use crate::tools::file_utils::PathValidator;
use std::path::PathBuf;

/// Validates `raw` with `validator`, mapping a failure into a `ToolResult` error.
///
/// On success the validated [`PathBuf`] is returned. On failure the validation
/// error is wrapped in a model-visible [`ToolResult`] error whose message is
/// `"{label}: {error}"`, preserving the phrasing tools used before this helper
/// existed (for example `"Invalid path"` or `"Invalid source path"`).
///
/// The `Err` variant carries a [`ToolResult`] so a caller whose fallible section
/// returns `Result<ToolResult, ToolResult>` can apply `?` directly and let the
/// surrounding `execute` turn it back into `Ok(ToolResult::error(..))`.
///
/// # Errors
///
/// Returns `Err(ToolResult)` when `validator` rejects `raw` (absolute paths,
/// traversal sequences, or paths that resolve outside the working directory).
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_utils::PathValidator;
/// use xzatoma::tools::path_tool::validate_or_err;
///
/// let validator = PathValidator::new(std::env::temp_dir());
/// assert!(validate_or_err(&validator, "src/main.rs", "Invalid path").is_ok());
///
/// let rejected = validate_or_err(&validator, "../escape", "Invalid path").unwrap_err();
/// assert!(rejected.error.unwrap().starts_with("Invalid path: "));
/// ```
pub fn validate_or_err(
    validator: &PathValidator,
    raw: &str,
    label: &str,
) -> std::result::Result<PathBuf, ToolResult> {
    validator
        .validate(raw)
        .map_err(|error| ToolResult::error(format!("{}: {}", label, error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_or_err_with_valid_path_returns_path() {
        let validator = PathValidator::new(std::env::temp_dir());
        assert!(validate_or_err(&validator, "src/main.rs", "Invalid path").is_ok());
    }

    #[test]
    fn test_validate_or_err_with_invalid_path_returns_labeled_tool_error() {
        let validator = PathValidator::new(std::env::temp_dir());
        let result = validate_or_err(&validator, "../escape", "Invalid source path");
        let tool_result = result.unwrap_err();
        assert!(!tool_result.success);
        assert!(
            tool_result
                .error
                .unwrap()
                .starts_with("Invalid source path: ")
        );
    }
}
