//! Test utilities for XZatoma
//!
//! This module provides common test utilities including temporary directory
//! management, test file creation, and assertion helpers.

use crate::config::Config;
use crate::error::XzatomaError;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary directory for testing
///
/// # Returns
///
/// Returns a TempDir that will be cleaned up when dropped
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::temp_dir;
///
/// let dir = temp_dir();
/// let path = dir.path();
/// // Use the temporary directory
/// ```
pub fn temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temporary directory")
}

/// Create a test file with the given content
///
/// # Arguments
///
/// * `dir` - Directory to create the file in
/// * `name` - Name of the file
/// * `content` - Content to write to the file
///
/// # Returns
///
/// Returns the path to the created file
///
/// # Panics
///
/// Panics if file creation or writing fails
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::{temp_dir, create_test_file};
///
/// let dir = temp_dir();
/// let file_path = create_test_file(&dir, "test.txt", "content");
/// ```
pub fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("Failed to write test file");
    path
}

/// Assert that an error contains the expected message
///
/// # Arguments
///
/// * `result` - Result to check
/// * `expected` - Expected error message substring
///
/// # Panics
///
/// Panics if the result is Ok or if the error doesn't contain the expected message
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::assert_error_contains;
/// use xzatoma::error::XzatomaError;
///
/// let result: Result<(), XzatomaError> = Err(XzatomaError::Config("invalid".to_string()));
/// assert_error_contains(result, "invalid");
/// ```
pub fn assert_error_contains<T>(result: Result<T, XzatomaError>, expected: &str) {
    match result {
        Ok(_) => panic!("Expected error containing '{}' but got Ok", expected),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(expected),
                "Error message '{}' does not contain '{}'",
                error_msg,
                expected
            );
        }
    }
}

/// Create a test configuration with default values
///
/// # Returns
///
/// Returns a Config instance suitable for testing
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::test_config;
///
/// let config = test_config();
/// assert_eq!(config.provider.provider_type, "copilot");
/// ```
pub fn test_config() -> Config {
    Config::default()
}

/// Create a test configuration YAML string
///
/// # Returns
///
/// Returns a YAML string with test configuration
pub fn test_config_yaml() -> String {
    r#"
provider:
  type: ollama
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  timeout_seconds: 60
  conversation:
    max_tokens: 10000
    min_retain_turns: 3
    prune_threshold: 0.8
  tools:
    max_output_size: 1048576
    max_file_read_size: 10485760
  terminal:
    default_mode: restricted_autonomous
    timeout_seconds: 30
    max_stdout_bytes: 1048576
    max_stderr_bytes: 262144
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_creation() {
        let dir = temp_dir();
        assert!(dir.path().exists());
    }

    #[test]
    fn test_create_test_file() {
        let dir = temp_dir();
        let path = create_test_file(&dir, "test.txt", "content");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_assert_error_contains_success() {
        let result: Result<(), XzatomaError> =
            Err(XzatomaError::Config("test error message".to_string()));
        assert_error_contains(result, "test error");
    }

    #[test]
    #[should_panic(expected = "Expected error containing")]
    fn test_assert_error_contains_ok() {
        let result: Result<(), XzatomaError> = Ok(());
        assert_error_contains(result, "error");
    }

    #[test]
    #[should_panic(expected = "does not contain")]
    fn test_assert_error_contains_wrong_message() {
        let result: Result<(), XzatomaError> =
            Err(XzatomaError::Config("different error".to_string()));
        assert_error_contains(result, "not present");
    }

    #[test]
    fn test_test_config() {
        let config = test_config();
        assert_eq!(config.provider.provider_type, "copilot");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_test_config_yaml() {
        let yaml = test_config_yaml();
        assert!(yaml.contains("provider:"));
        assert!(yaml.contains("agent:"));
        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        assert!(config.validate().is_ok());
    }
}
