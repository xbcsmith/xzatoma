/// ACP session models.
///
/// This module defines transport-independent session types for the ACP domain
/// model. These structures are protocol-facing, serializable with `serde`, and
/// intentionally focused on stable data contracts rather than runtime storage or
/// HTTP concerns.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::{
///     Session, SessionId, SessionOrigin, SessionSummary, SessionVisibility,
/// };
///
/// let session = Session::new(
///     SessionId::new("session_demo").unwrap(),
///     Some("Demo Session".to_string()),
///     SessionOrigin::User,
/// );
///
/// assert_eq!(session.id.as_str(), "session_demo");
/// assert_eq!(session.visibility, SessionVisibility::Private);
///
/// let summary = SessionSummary::from(&session);
/// assert_eq!(summary.id.as_str(), "session_demo");
/// ```
use crate::acp::error::AcpValidationError;
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Validates an ACP session identifier.
///
/// Session identifiers must:
///
/// - be non-empty
/// - be at most 128 characters
/// - start with an ASCII lowercase letter
/// - contain only ASCII lowercase letters, digits, underscores, or hyphens
///
/// # Arguments
///
/// * `value` - Candidate session identifier
///
/// # Returns
///
/// Returns `Ok(())` when the identifier is valid.
///
/// # Errors
///
/// Returns an error if the identifier violates ACP naming constraints.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::validate_session_id;
///
/// assert!(validate_session_id("session_demo").is_ok());
/// assert!(validate_session_id("SessionDemo").is_err());
/// ```
pub fn validate_session_id(value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(AcpValidationError::new("session.id", "session id cannot be empty").into());
    }

    if value.len() > 128 {
        return Err(AcpValidationError::new(
            "session.id",
            "session id cannot exceed 128 characters",
        )
        .into());
    }

    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(AcpValidationError::new("session.id", "session id cannot be empty").into());
    };

    if !first.is_ascii_lowercase() {
        return Err(AcpValidationError::new(
            "session.id",
            "session id must start with an ASCII lowercase letter",
        )
        .into());
    }

    if !value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        return Err(AcpValidationError::new(
            "session.id",
            "session id must contain only ASCII lowercase letters, digits, underscores, or hyphens",
        )
        .into());
    }

    Ok(())
}

/// ACP session identifier.
///
/// This newtype ensures session identifiers can be validated explicitly before
/// they are used in protocol-facing structures.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::SessionId;
///
/// let session_id = SessionId::new("session_demo").unwrap();
/// assert_eq!(session_id.as_str(), "session_demo");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(String);

impl SessionId {
    /// Creates a validated ACP session identifier.
    ///
    /// # Arguments
    ///
    /// * `value` - Candidate session identifier
    ///
    /// # Returns
    ///
    /// Returns a validated `SessionId`.
    ///
    /// # Errors
    ///
    /// Returns an error if the identifier is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::SessionId;
    ///
    /// let session_id = SessionId::new("session_demo").unwrap();
    /// assert_eq!(session_id.as_str(), "session_demo");
    /// ```
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_session_id(&value)?;
        Ok(Self(value))
    }

    /// Returns the underlying session identifier as a string slice.
    ///
    /// # Returns
    ///
    /// Returns the validated identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::SessionId;
    ///
    /// let session_id = SessionId::new("session_demo").unwrap();
    /// assert_eq!(session_id.as_str(), "session_demo");
    /// ```
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the session identifier and returns the inner string.
    ///
    /// # Returns
    ///
    /// Returns the owned identifier string.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::SessionId;
    ///
    /// let session_id = SessionId::new("session_demo").unwrap();
    /// assert_eq!(session_id.into_inner(), "session_demo");
    /// ```
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Origin of an ACP session.
///
/// This field captures how the session was created at the protocol level and is
/// useful for later lifecycle, persistence, and auditing features.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::SessionOrigin;
///
/// assert_eq!(SessionOrigin::default(), SessionOrigin::User);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionOrigin {
    /// Session initiated directly by a user-facing client request.
    #[default]
    User,
    /// Session resumed from previously persisted ACP state.
    Resume,
    /// Session created by an internal system workflow.
    System,
}

/// Visibility classification for an ACP session.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::SessionVisibility;
///
/// assert_eq!(SessionVisibility::default(), SessionVisibility::Private);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionVisibility {
    /// Session is private to the caller or owning context.
    #[default]
    Private,
    /// Session may be listed or disclosed to broader trusted contexts.
    Shared,
}

/// ACP session protocol model.
///
/// This structure is intentionally minimal and focuses on stable
/// identity and metadata that later ACP phases can extend.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::{Session, SessionId, SessionOrigin, SessionVisibility};
///
/// let session = Session::new(
///     SessionId::new("session_demo").unwrap(),
///     Some("Demo Session".to_string()),
///     SessionOrigin::User,
/// );
///
/// assert_eq!(session.id.as_str(), "session_demo");
/// assert_eq!(session.visibility, SessionVisibility::Private);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Unique ACP session identifier.
    pub id: SessionId,
    /// Optional human-readable session title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Session creation origin.
    #[serde(default)]
    pub origin: SessionOrigin,
    /// Session visibility.
    #[serde(default)]
    pub visibility: SessionVisibility,
    /// RFC 3339 creation timestamp.
    pub created_at: DateTime<Utc>,
    /// RFC 3339 last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Session {
    /// Creates a new ACP session with current timestamps.
    ///
    /// # Arguments
    ///
    /// * `id` - Validated session identifier
    /// * `title` - Optional session title
    /// * `origin` - Session origin classification
    ///
    /// # Returns
    ///
    /// Returns a new `Session` initialized with current UTC timestamps.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::{Session, SessionId, SessionOrigin};
    ///
    /// let session = Session::new(
    ///     SessionId::new("session_demo").unwrap(),
    ///     Some("Demo".to_string()),
    ///     SessionOrigin::User,
    /// );
    ///
    /// assert_eq!(session.id.as_str(), "session_demo");
    /// assert_eq!(session.title.as_deref(), Some("Demo"));
    /// ```
    pub fn new(id: SessionId, title: Option<String>, origin: SessionOrigin) -> Self {
        let now = Utc::now();
        Self {
            id,
            title,
            origin,
            visibility: SessionVisibility::Private,
            created_at: now,
            updated_at: now,
        }
    }

    /// Validates the session model.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the session is valid.
    ///
    /// # Errors
    ///
    /// Returns an error if the session identifier or title is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::{Session, SessionId, SessionOrigin};
    ///
    /// let session = Session::new(
    ///     SessionId::new("session_demo").unwrap(),
    ///     Some("Demo".to_string()),
    ///     SessionOrigin::User,
    /// );
    ///
    /// assert!(session.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_session_id(self.id.as_str())?;

        if let Some(title) = &self.title {
            if title.trim().is_empty() {
                return Err(AcpValidationError::new(
                    "session.title",
                    "session title cannot be empty",
                )
                .into());
            }

            if title.len() > 256 {
                return Err(AcpValidationError::new(
                    "session.title",
                    "session title cannot exceed 256 characters",
                )
                .into());
            }
        }

        Ok(())
    }

    /// Updates the session's `updated_at` timestamp to the current UTC time.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session::{Session, SessionId, SessionOrigin};
    ///
    /// let mut session = Session::new(
    ///     SessionId::new("session_demo").unwrap(),
    ///     None,
    ///     SessionOrigin::User,
    /// );
    ///
    /// let before = session.updated_at;
    /// session.touch();
    /// assert!(session.updated_at >= before);
    /// ```
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// Lightweight ACP session summary.
///
/// This summary shape is suitable for future list and discovery operations
/// without exposing full session internals.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session::{Session, SessionId, SessionOrigin, SessionSummary};
///
/// let session = Session::new(
///     SessionId::new("session_demo").unwrap(),
///     Some("Demo Session".to_string()),
///     SessionOrigin::User,
/// );
///
/// let summary = SessionSummary::from(&session);
/// assert_eq!(summary.id.as_str(), "session_demo");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// Unique ACP session identifier.
    pub id: SessionId,
    /// Optional human-readable session title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Session creation origin.
    pub origin: SessionOrigin,
    /// Session visibility.
    pub visibility: SessionVisibility,
    /// RFC 3339 creation timestamp.
    pub created_at: DateTime<Utc>,
    /// RFC 3339 last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            title: session.title.clone(),
            origin: session.origin.clone(),
            visibility: session.visibility.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_session_id_accepts_valid_identifier() {
        let result = validate_session_id("session_demo-1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_session_id_rejects_empty_identifier() {
        let result = validate_session_id("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_session_id_rejects_uppercase_identifier() {
        let result = validate_session_id("SessionDemo");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_session_id_rejects_invalid_character() {
        let result = validate_session_id("session.demo");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_id_new_returns_valid_identifier() {
        let session_id = SessionId::new("session_demo").unwrap();
        assert_eq!(session_id.as_str(), "session_demo");
    }

    #[test]
    fn test_session_id_new_rejects_invalid_identifier() {
        let result = SessionId::new("SessionDemo");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_new_sets_defaults() {
        let session = Session::new(
            SessionId::new("session_demo").unwrap(),
            Some("Demo Session".to_string()),
            SessionOrigin::User,
        );

        assert_eq!(session.id.as_str(), "session_demo");
        assert_eq!(session.title.as_deref(), Some("Demo Session"));
        assert_eq!(session.origin, SessionOrigin::User);
        assert_eq!(session.visibility, SessionVisibility::Private);
        assert!(session.updated_at >= session.created_at);
    }

    #[test]
    fn test_session_validate_accepts_valid_session() {
        let session = Session::new(
            SessionId::new("session_demo").unwrap(),
            Some("Demo Session".to_string()),
            SessionOrigin::User,
        );

        assert!(session.validate().is_ok());
    }

    #[test]
    fn test_session_validate_rejects_empty_title() {
        let session = Session {
            id: SessionId::new("session_demo").unwrap(),
            title: Some("   ".to_string()),
            origin: SessionOrigin::User,
            visibility: SessionVisibility::Private,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(session.validate().is_err());
    }

    #[test]
    fn test_session_touch_updates_timestamp() {
        let mut session = Session::new(
            SessionId::new("session_demo").unwrap(),
            None,
            SessionOrigin::User,
        );

        let previous_updated_at = session.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(1));
        session.touch();

        assert!(session.updated_at > previous_updated_at);
    }

    #[test]
    fn test_session_summary_from_session_copies_fields() {
        let session = Session::new(
            SessionId::new("session_demo").unwrap(),
            Some("Demo Session".to_string()),
            SessionOrigin::Resume,
        );

        let summary = SessionSummary::from(&session);

        assert_eq!(summary.id, session.id);
        assert_eq!(summary.title, session.title);
        assert_eq!(summary.origin, session.origin);
        assert_eq!(summary.visibility, session.visibility);
        assert_eq!(summary.created_at, session.created_at);
        assert_eq!(summary.updated_at, session.updated_at);
    }

    #[test]
    fn test_session_serialization_uses_camel_case() {
        let session = Session::new(
            SessionId::new("session_demo").unwrap(),
            Some("Demo Session".to_string()),
            SessionOrigin::User,
        );

        let value = serde_json::to_value(&session).unwrap();
        assert!(value.get("createdAt").is_some());
        assert!(value.get("updatedAt").is_some());
        assert!(value.get("created_at").is_none());
        assert!(value.get("updated_at").is_none());
    }
}
