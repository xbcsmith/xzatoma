//! Error types for XZatoma
//!
//! This module defines all error types used throughout the application,
//! using `thiserror` for ergonomic error handling.

// Phase 1: Allow unused variants for placeholder implementations
#![allow(dead_code)]

use thiserror::Error;

/// Main error type for XZatoma operations
///
/// This enum encompasses all possible errors that can occur during
/// agent execution, configuration loading, provider interactions,
/// tool execution, and security validations.
#[derive(Error, Debug)]
pub enum XzatomaError {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Provider-related errors (API calls, authentication, etc.)
    #[error("Provider error: {0}")]
    Provider(String),

    /// Tool execution errors
    #[error("Tool execution error: {0}")]
    Tool(String),

    /// Fetch-related errors (HTTP fetch, SSRF, timeouts, rate limits)
    #[error("Fetch error: {0}")]
    Fetch(String),

    /// Mention parsing errors (invalid mention syntax)
    #[error("Mention parsing error: {0}")]
    MentionParse(String),

    /// File loading errors (read errors, size, binary)
    #[error("File load error: {0}")]
    FileLoad(String),

    /// Search/Grep related errors
    #[error("Search error: {0}")]
    Search(String),

    /// Rate limit exceeded for an operation
    #[error("Rate limit exceeded: limit={limit}, {message}")]
    RateLimitExceeded {
        /// The configured limit that was exceeded
        limit: u32,
        /// Additional message explaining the failure
        message: String,
    },

    /// Agent exceeded maximum iteration limit
    #[error("Agent exceeded maximum iterations: limit={limit}, {message}")]
    MaxIterationsExceeded {
        /// The configured iteration limit
        limit: usize,
        /// Additional context about the failure
        message: String,
    },

    /// Command is considered dangerous and requires confirmation
    #[error("Dangerous command detected: {0}")]
    DangerousCommand(String),

    /// Command requires user confirmation but cannot proceed
    #[error("Command requires confirmation: {0}")]
    CommandRequiresConfirmation(String),

    /// Path validation failed (outside working directory)
    #[error("Path validation failed: {0}")]
    PathOutsideWorkingDirectory(String),

    /// Streaming not supported by provider
    #[error("Streaming is not supported by this provider")]
    StreamingNotSupported,

    /// Missing credentials for provider
    #[error("Missing credentials for provider: {0}")]
    MissingCredentials(String),

    /// Authentication errors (e.g., 401 Unauthorized)
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing errors
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// HTTP request errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Keyring/credential storage errors
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    /// Conversation storage errors (database operations)
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Result type alias for XZatoma operations
///
/// This is a convenience alias that uses `anyhow::Error` as the error type,
/// allowing for rich error context and easy error propagation.
pub type Result<T> = anyhow::Result<T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_display() {
        let error = XzatomaError::Config("invalid format".to_string());
        assert_eq!(error.to_string(), "Configuration error: invalid format");
    }

    #[test]
    fn test_provider_error_display() {
        let error = XzatomaError::Provider("API timeout".to_string());
        assert_eq!(error.to_string(), "Provider error: API timeout");
    }

    #[test]
    fn test_tool_error_display() {
        let error = XzatomaError::Tool("file not found".to_string());
        assert_eq!(error.to_string(), "Tool execution error: file not found");
    }

    #[test]
    fn test_max_iterations_error_display() {
        let error = XzatomaError::MaxIterationsExceeded {
            limit: 50,
            message: "stuck in loop".to_string(),
        };
        assert!(error.to_string().contains("limit=50"));
        assert!(error.to_string().contains("stuck in loop"));
    }

    #[test]
    fn test_dangerous_command_error_display() {
        let error = XzatomaError::DangerousCommand("rm -rf /".to_string());
        assert_eq!(error.to_string(), "Dangerous command detected: rm -rf /");
    }

    #[test]
    fn test_command_confirmation_error_display() {
        let error = XzatomaError::CommandRequiresConfirmation("sudo apt install".to_string());
        assert_eq!(
            error.to_string(),
            "Command requires confirmation: sudo apt install"
        );
    }

    #[test]
    fn test_path_validation_error_display() {
        let error = XzatomaError::PathOutsideWorkingDirectory("/etc/passwd".to_string());
        assert_eq!(error.to_string(), "Path validation failed: /etc/passwd");
    }

    #[test]
    fn test_streaming_not_supported_error() {
        let error = XzatomaError::StreamingNotSupported;
        assert_eq!(
            error.to_string(),
            "Streaming is not supported by this provider"
        );
    }

    #[test]
    fn test_missing_credentials_error_display() {
        let error = XzatomaError::MissingCredentials("github_copilot".to_string());
        assert_eq!(
            error.to_string(),
            "Missing credentials for provider: github_copilot"
        );
    }

    #[test]
    fn test_authentication_error_display() {
        let error = XzatomaError::Authentication("token expired".to_string());
        assert_eq!(error.to_string(), "Authentication error: token expired");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: XzatomaError = io_error.into();
        assert!(matches!(error, XzatomaError::Io(_)));
    }

    #[test]
    fn test_json_error_conversion() {
        let json_str = "{invalid json}";
        let json_error = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let error: XzatomaError = json_error.into();
        assert!(matches!(error, XzatomaError::Serialization(_)));
    }

    #[test]
    fn test_yaml_error_conversion() {
        let yaml_str = "invalid: : yaml";
        let yaml_error = serde_yaml::from_str::<serde_yaml::Value>(yaml_str).unwrap_err();
        let error: XzatomaError = yaml_error.into();
        assert!(matches!(error, XzatomaError::Yaml(_)));
    }

    #[test]
    fn test_fetch_error_display() {
        let error = XzatomaError::Fetch("timeout".to_string());
        assert_eq!(error.to_string(), "Fetch error: timeout");
    }

    #[test]
    fn test_mention_parse_error_display() {
        let error = XzatomaError::MentionParse("invalid syntax".to_string());
        assert_eq!(error.to_string(), "Mention parsing error: invalid syntax");
    }

    #[test]
    fn test_file_load_error_display() {
        let error = XzatomaError::FileLoad("not found".to_string());
        assert_eq!(error.to_string(), "File load error: not found");
    }

    #[test]
    fn test_search_error_display() {
        let error = XzatomaError::Search("grep failed".to_string());
        assert_eq!(error.to_string(), "Search error: grep failed");
    }

    #[test]
    fn test_rate_limit_exceeded_display() {
        let error = XzatomaError::RateLimitExceeded {
            limit: 10,
            message: "Too many requests".to_string(),
        };
        let s = error.to_string();
        assert!(s.contains("limit=10"));
        assert!(s.contains("Too many requests"));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<XzatomaError>();
    }

    #[test]
    fn test_storage_error_display() {
        let error = XzatomaError::Storage("database connection failed".to_string());
        assert_eq!(
            error.to_string(),
            "Storage error: database connection failed"
        );
    }
}
