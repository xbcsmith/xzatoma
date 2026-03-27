/// ACP-specific error types and validation helpers.
///
/// This module defines transport-independent ACP domain errors used by the ACP
/// protocol model, lifecycle abstractions, and validation helpers.
///
/// The ACP layer uses a focused local error type so protocol-facing validation
/// can remain precise and descriptive before errors are adapted into broader
/// crate-level error handling.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::{AcpError, AcpValidationError};
///
/// let error = AcpValidationError::new("message.role", "role must be one of user, assistant, system, or tool");
/// assert_eq!(error.field(), "message.role");
///
/// let acp_error = AcpError::validation(error.to_string());
/// assert!(acp_error.to_string().contains("ACP validation error"));
/// ```
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result alias for ACP-local operations.
///
/// This result type is used within the ACP domain layer for strongly typed ACP
/// validation and lifecycle failures.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::{AcpError, Result};
///
/// fn example() -> Result<()> {
///     Ok(())
/// }
///
/// assert!(example().is_ok());
/// let error = AcpError::validation("invalid payload");
/// assert!(error.to_string().contains("invalid payload"));
/// ```
pub type Result<T> = std::result::Result<T, AcpError>;

/// ACP field-level validation error.
///
/// This error captures a specific ACP field path and a descriptive validation
/// message so invalid payloads can fail with precise diagnostics.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::AcpValidationError;
///
/// let error = AcpValidationError::new("run.id", "run id cannot be empty");
/// assert_eq!(error.field(), "run.id");
/// assert_eq!(error.message(), "run id cannot be empty");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("ACP validation error at '{field}': {message}")]
pub struct AcpValidationError {
    field: String,
    message: String,
}

impl AcpValidationError {
    /// Creates a new ACP validation error.
    ///
    /// # Arguments
    ///
    /// * `field` - ACP field path associated with the error
    /// * `message` - Human-readable validation message
    ///
    /// # Returns
    ///
    /// Returns a new `AcpValidationError`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpValidationError;
    ///
    /// let error = AcpValidationError::new("message.parts", "message must contain at least one part");
    /// assert_eq!(error.field(), "message.parts");
    /// ```
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }

    /// Returns the ACP field path associated with the validation error.
    ///
    /// # Returns
    ///
    /// Returns the field path.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpValidationError;
    ///
    /// let error = AcpValidationError::new("artifact.content", "content cannot be empty");
    /// assert_eq!(error.field(), "artifact.content");
    /// ```
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the validation message.
    ///
    /// # Returns
    ///
    /// Returns the validation message.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpValidationError;
    ///
    /// let error = AcpValidationError::new("artifact.content", "content cannot be empty");
    /// assert_eq!(error.message(), "content cannot be empty");
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// ACP domain error type.
///
/// This enum represents transport-independent ACP failures for validation,
/// lifecycle transitions, persistence scaffolding, and unsupported behavior.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::AcpError;
///
/// let error = AcpError::lifecycle("run cannot transition from completed to running");
/// assert!(error.to_string().contains("ACP lifecycle error"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum AcpError {
    /// ACP validation failure.
    #[error("ACP validation error: {0}")]
    Validation(String),

    /// ACP lifecycle failure.
    #[error("ACP lifecycle error: {0}")]
    Lifecycle(String),

    /// ACP persistence failure.
    #[error("ACP persistence error: {0}")]
    Persistence(String),

    /// ACP unsupported transition failure.
    #[error("ACP unsupported transition: {from} -> {to}")]
    UnsupportedTransition {
        /// Current lifecycle state.
        from: String,
        /// Requested lifecycle state.
        to: String,
    },

    /// ACP protocol failure.
    #[error("ACP protocol error: {0}")]
    Protocol(String),
}

impl AcpError {
    /// Creates an ACP validation error.
    ///
    /// # Arguments
    ///
    /// * `message` - Validation message
    ///
    /// # Returns
    ///
    /// Returns a new validation error.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpError;
    ///
    /// let error = AcpError::validation("message parts must not be empty");
    /// assert!(matches!(error, AcpError::Validation(_)));
    /// ```
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    /// Creates an ACP lifecycle error.
    ///
    /// # Arguments
    ///
    /// * `message` - Lifecycle message
    ///
    /// # Returns
    ///
    /// Returns a new lifecycle error.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpError;
    ///
    /// let error = AcpError::lifecycle("run is already terminal");
    /// assert!(matches!(error, AcpError::Lifecycle(_)));
    /// ```
    pub fn lifecycle(message: impl Into<String>) -> Self {
        Self::Lifecycle(message.into())
    }

    /// Creates an ACP persistence error.
    ///
    /// # Arguments
    ///
    /// * `message` - Persistence message
    ///
    /// # Returns
    ///
    /// Returns a new persistence error.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpError;
    ///
    /// let error = AcpError::persistence("session store unavailable");
    /// assert!(matches!(error, AcpError::Persistence(_)));
    /// ```
    pub fn persistence(message: impl Into<String>) -> Self {
        Self::Persistence(message.into())
    }

    /// Creates an ACP protocol error.
    ///
    /// # Arguments
    ///
    /// * `message` - Protocol message
    ///
    /// # Returns
    ///
    /// Returns a new protocol error.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::error::AcpError;
    ///
    /// let error = AcpError::protocol("unsupported ACP payload shape");
    /// assert!(matches!(error, AcpError::Protocol(_)));
    /// ```
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }
}

impl From<AcpValidationError> for AcpError {
    fn from(value: AcpValidationError) -> Self {
        Self::Validation(value.to_string())
    }
}

/// Validates that a string field is not empty after trimming whitespace.
///
/// # Arguments
///
/// * `field` - ACP field path for diagnostics
/// * `value` - String value to validate
///
/// # Returns
///
/// Returns `Ok(())` if the value is non-empty.
///
/// # Errors
///
/// Returns `AcpError::Validation` if the value is empty.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::validate_non_empty;
///
/// assert!(validate_non_empty("message.role", "user").is_ok());
/// assert!(validate_non_empty("message.role", "   ").is_err());
/// ```
pub fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AcpValidationError::new(field, "value cannot be empty").into());
    }

    Ok(())
}

/// Validates that an optional string field is non-empty when present.
///
/// # Arguments
///
/// * `field` - ACP field path for diagnostics
/// * `value` - Optional string value to validate
///
/// # Returns
///
/// Returns `Ok(())` if the value is absent or non-empty.
///
/// # Errors
///
/// Returns `AcpError::Validation` if the value is present but empty.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::validate_optional_non_empty;
///
/// assert!(validate_optional_non_empty("artifact.name", None).is_ok());
/// assert!(validate_optional_non_empty("artifact.name", Some("result.txt")).is_ok());
/// assert!(validate_optional_non_empty("artifact.name", Some("")).is_err());
/// ```
pub fn validate_optional_non_empty(field: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }

    Ok(())
}

/// Validates that two optional ACP fields are mutually exclusive.
///
/// # Arguments
///
/// * `left_field` - Name of the first field
/// * `left_present` - Whether the first field is present
/// * `right_field` - Name of the second field
/// * `right_present` - Whether the second field is present
///
/// # Returns
///
/// Returns `Ok(())` if at most one field is present.
///
/// # Errors
///
/// Returns `AcpError::Validation` if both fields are present.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::validate_mutually_exclusive;
///
/// assert!(validate_mutually_exclusive("content", true, "content_url", false).is_ok());
/// assert!(validate_mutually_exclusive("content", false, "content_url", true).is_ok());
/// assert!(validate_mutually_exclusive("content", true, "content_url", true).is_err());
/// ```
pub fn validate_mutually_exclusive(
    left_field: &str,
    left_present: bool,
    right_field: &str,
    right_present: bool,
) -> Result<()> {
    if left_present && right_present {
        return Err(AcpValidationError::new(
            format!("{left_field},{right_field}"),
            format!("'{left_field}' and '{right_field}' are mutually exclusive"),
        )
        .into());
    }

    Ok(())
}

/// Validates that a string field matches a predicate.
///
/// # Arguments
///
/// * `field` - ACP field path for diagnostics
/// * `value` - Value to validate
/// * `predicate` - Predicate function
/// * `message` - Error message when the predicate fails
///
/// # Returns
///
/// Returns `Ok(())` if the predicate succeeds.
///
/// # Errors
///
/// Returns `AcpError::Validation` if the predicate fails.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::error::validate_with_predicate;
///
/// let result = validate_with_predicate(
///     "message.role",
///     "user",
///     |value| matches!(value, "user" | "assistant"),
///     "role must be user or assistant",
/// );
///
/// assert!(result.is_ok());
/// ```
pub fn validate_with_predicate<F>(
    field: &str,
    value: &str,
    predicate: F,
    message: &str,
) -> Result<()>
where
    F: FnOnce(&str) -> bool,
{
    if !predicate(value) {
        return Err(AcpValidationError::new(field, message).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_validation_error_new_sets_fields() {
        let error = AcpValidationError::new("message.role", "invalid role");
        assert_eq!(error.field(), "message.role");
        assert_eq!(error.message(), "invalid role");
    }

    #[test]
    fn test_acp_validation_error_display_is_descriptive() {
        let error = AcpValidationError::new("message.role", "invalid role");
        assert_eq!(
            error.to_string(),
            "ACP validation error at 'message.role': invalid role"
        );
    }

    #[test]
    fn test_acp_error_validation_constructor() {
        let error = AcpError::validation("bad payload");
        assert!(matches!(error, AcpError::Validation(_)));
        assert_eq!(error.to_string(), "ACP validation error: bad payload");
    }

    #[test]
    fn test_acp_error_lifecycle_constructor() {
        let error = AcpError::lifecycle("bad transition");
        assert!(matches!(error, AcpError::Lifecycle(_)));
        assert_eq!(error.to_string(), "ACP lifecycle error: bad transition");
    }

    #[test]
    fn test_acp_error_persistence_constructor() {
        let error = AcpError::persistence("store failed");
        assert!(matches!(error, AcpError::Persistence(_)));
        assert_eq!(error.to_string(), "ACP persistence error: store failed");
    }

    #[test]
    fn test_acp_error_protocol_constructor() {
        let error = AcpError::protocol("unsupported payload");
        assert!(matches!(error, AcpError::Protocol(_)));
        assert_eq!(error.to_string(), "ACP protocol error: unsupported payload");
    }

    #[test]
    fn test_acp_error_unsupported_transition_display() {
        let error = AcpError::UnsupportedTransition {
            from: "completed".to_string(),
            to: "running".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "ACP unsupported transition: completed -> running"
        );
    }

    #[test]
    fn test_from_acp_validation_error_converts_to_acp_error() {
        let error: AcpError = AcpValidationError::new("run.id", "run id cannot be empty").into();
        assert!(matches!(error, AcpError::Validation(_)));
        assert!(error.to_string().contains("run.id"));
    }

    #[test]
    fn test_validate_non_empty_accepts_non_empty_value() {
        let result = validate_non_empty("message.role", "user");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_non_empty_rejects_empty_value() {
        let result = validate_non_empty("message.role", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_non_empty_rejects_whitespace_only_value() {
        let result = validate_non_empty("message.role", "   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_optional_non_empty_accepts_none() {
        let result = validate_optional_non_empty("artifact.name", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_non_empty_accepts_non_empty_some() {
        let result = validate_optional_non_empty("artifact.name", Some("result.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_optional_non_empty_rejects_empty_some() {
        let result = validate_optional_non_empty("artifact.name", Some(""));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mutually_exclusive_accepts_single_present_left() {
        let result = validate_mutually_exclusive("content", true, "content_url", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mutually_exclusive_accepts_single_present_right() {
        let result = validate_mutually_exclusive("content", false, "content_url", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mutually_exclusive_accepts_both_absent() {
        let result = validate_mutually_exclusive("content", false, "content_url", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mutually_exclusive_rejects_both_present() {
        let result = validate_mutually_exclusive("content", true, "content_url", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_with_predicate_accepts_matching_value() {
        let result = validate_with_predicate(
            "message.role",
            "assistant",
            |value| matches!(value, "user" | "assistant"),
            "role must be user or assistant",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_with_predicate_rejects_non_matching_value() {
        let result = validate_with_predicate(
            "message.role",
            "system",
            |value| matches!(value, "user" | "assistant"),
            "role must be user or assistant",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_acp_error_serializes_and_deserializes() {
        let error = AcpError::UnsupportedTransition {
            from: "running".to_string(),
            to: "pending".to_string(),
        };

        let json = serde_json::to_string(&error).unwrap();
        let restored: AcpError = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, error);
    }

    #[test]
    fn test_acp_validation_error_serializes_and_deserializes() {
        let error = AcpValidationError::new("message.parts", "must contain at least one part");

        let json = serde_json::to_string(&error).unwrap();
        let restored: AcpValidationError = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, error);
    }
}
