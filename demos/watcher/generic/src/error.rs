//! Error types and helper utilities.

use std::fmt;

/// The top-level error type returned by all fallible operations.
#[derive(Debug)]
pub enum AppError {
    /// An I/O error from the filesystem or network.
    Io(std::io::Error),
    /// A configuration parsing or validation failure.
    Config(String),
    /// An error returned by the AI provider.
    Provider(String),
    /// A tool execution failure.
    Tool { name: String, message: String },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "io error: {}", e),
            AppError::Config(msg) => write!(f, "config error: {}", msg),
            AppError::Provider(msg) => write!(f, "provider error: {}", msg),
            AppError::Tool { name, message } => {
                write!(f, "tool '{}' failed: {}", name, message)
            }
        }
    }
}

pub fn format_error(kind: &str, message: &str) -> String {
    format!("[{}] {}", kind.to_uppercase(), message)
}

/// Returns `true` when the error kind is considered transient and the
/// operation that produced it may safely be retried.
///
/// Transient kinds include `"io"`, `"timeout"`, and `"rate_limit"`.
/// All other kinds are treated as permanent failures.
pub fn is_recoverable(kind: &str) -> bool {
    matches!(kind, "io" | "timeout" | "rate_limit")
}

pub fn error_code(error: &AppError) -> u32 {
    match error {
        AppError::Io(_) => 1,
        AppError::Config(_) => 2,
        AppError::Provider(_) => 3,
        AppError::Tool { .. } => 4,
    }
}
