/// ACP protocol types and message adapters.
///
/// This module defines transport-independent, protocol-facing ACP data models
/// for manifests, messages, runs, sessions, errors, and events used by the ACP
/// support implementation. The structures in this module derive `serde` traits
/// so they can be serialized and deserialized cleanly, but they intentionally do
/// not depend on HTTP transport details.
///
/// It also includes validation helpers for ACP naming rules, role formats,
/// message structure, artifact field rules, and adapter helpers that convert the
/// existing provider message shape into ACP output messages.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{
///     agent_message_to_acp_message, now_rfc3339, AcpMessage, AcpMessagePart, AcpRole, AcpTextPart,
/// };
/// use xzatoma::providers::Message;
///
/// let message = AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
/// )
/// .unwrap();
/// assert_eq!(message.role, AcpRole::User);
///
/// let provider_message = Message::assistant("hi there");
/// let converted = agent_message_to_acp_message(&provider_message).unwrap();
/// assert_eq!(converted.role, AcpRole::Assistant);
///
/// let timestamp = now_rfc3339();
/// assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
/// ```
use crate::error::Result;
use crate::providers;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Returns the current UTC time as an RFC 3339 string.
///
/// The timestamp uses second precision and a trailing `Z`.
///
/// # Returns
///
/// Returns the current UTC timestamp formatted as RFC 3339.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::now_rfc3339;
///
/// let timestamp = now_rfc3339();
/// assert!(timestamp.ends_with('Z'));
/// assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
/// ```
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Validates an ACP-facing identifier.
///
/// Supported identifiers:
///
/// - must not be empty
/// - must not start or end with whitespace
/// - must start with an ASCII lowercase letter
/// - may contain only ASCII lowercase letters, digits, `_`, `-`, and `.`
///
/// # Arguments
///
/// * `field` - Field name used in error reporting
/// * `value` - Identifier value to validate
///
/// # Errors
///
/// Returns an error if the identifier violates ACP naming rules.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::validate_acp_identifier;
///
/// assert!(validate_acp_identifier("run_id", "run_123").is_ok());
/// assert!(validate_acp_identifier("run_id", "Run_123").is_err());
/// ```
pub fn validate_acp_identifier(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(crate::acp::error::AcpError::validation(format!(
            "field '{}' cannot be empty",
            field
        ))
        .into());
    }

    if value.trim() != value {
        return Err(crate::acp::error::AcpError::validation(format!(
            "field '{}' cannot contain leading or trailing whitespace",
            field
        ))
        .into());
    }

    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(crate::acp::error::AcpError::validation(format!(
            "field '{}' cannot be empty",
            field
        ))
        .into());
    };

    if !first.is_ascii_lowercase() {
        return Err(crate::acp::error::AcpError::validation(format!(
            "field '{}' must start with an ASCII lowercase letter",
            field
        ))
        .into());
    }

    if !value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(crate::acp::error::AcpError::validation(format!(
            "field '{}' must contain only ASCII lowercase letters, digits, '_', '-', or '.'",
            field
        ))
        .into());
    }

    Ok(())
}

/// Validates an ACP role string.
///
/// Supported ACP roles are:
///
/// - `system`
/// - `user`
/// - `assistant`
/// - `tool`
///
/// # Arguments
///
/// * `role` - Role value to validate
///
/// # Errors
///
/// Returns an error if the role is unsupported.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::validate_acp_role;
///
/// assert!(validate_acp_role("assistant").is_ok());
/// assert!(validate_acp_role("developer").is_err());
/// ```
pub fn validate_acp_role(role: &str) -> Result<()> {
    match role {
        "system" | "user" | "assistant" | "tool" => Ok(()),
        _ => Err(crate::acp::error::AcpError::validation(format!(
            "unsupported ACP role '{}'; expected one of: system, user, assistant, tool",
            role
        ))
        .into()),
    }
}

/// ACP agent role.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpRole;
///
/// assert_eq!(AcpRole::default(), AcpRole::User);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AcpRole {
    /// System message role.
    System,
    /// User message role.
    #[default]
    User,
    /// Assistant message role.
    Assistant,
    /// Tool message role.
    Tool,
}

impl AcpRole {
    /// Returns the role as a string slice.
    ///
    /// # Returns
    ///
    /// Returns the ACP wire-format role string.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::types::AcpRole;
    ///
    /// assert_eq!(AcpRole::Assistant.as_str(), "assistant");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }

    /// Parses an ACP role from a string.
    ///
    /// # Arguments
    ///
    /// * `value` - Role string
    ///
    /// # Errors
    ///
    /// Returns an error if the role is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::types::AcpRole;
    ///
    /// let role = AcpRole::parse("tool").unwrap();
    /// assert_eq!(role, AcpRole::Tool);
    /// ```
    pub fn parse(value: &str) -> Result<Self> {
        validate_acp_role(value)?;
        Ok(match value {
            "system" => Self::System,
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "tool" => Self::Tool,
            _ => unreachable!("validated ACP role should be exhaustive"),
        })
    }
}

impl std::fmt::Display for AcpRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// ACP text message part.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpTextPart;
///
/// let part = AcpTextPart::new("hello".to_string());
/// assert_eq!(part.text, "hello");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpTextPart {
    /// Text content.
    pub text: String,
}

impl AcpTextPart {
    /// Creates a new text part.
    ///
    /// # Arguments
    ///
    /// * `text` - Text content
    ///
    /// # Returns
    ///
    /// Returns the created text part.
    pub fn new(text: String) -> Self {
        Self { text }
    }

    /// Validates the text part.
    ///
    /// # Errors
    ///
    /// Returns an error if the text is empty or whitespace-only.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::types::AcpTextPart;
    ///
    /// let part = AcpTextPart::new("hello".to_string());
    /// assert!(part.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if self.text.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "text message part cannot be empty",
            )
            .into());
        }

        Ok(())
    }
}

/// ACP artifact.
///
/// Artifact rules require exactly one of `content` or `content_url`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpArtifact;
///
/// let artifact = AcpArtifact::new_inline(
///     "output.txt".to_string(),
///     "text/plain".to_string(),
///     "hello".to_string(),
/// )
/// .unwrap();
/// assert!(artifact.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpArtifact {
    /// Artifact display or file name.
    pub name: String,
    /// Artifact MIME type.
    pub mime_type: String,
    /// Optional inline content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional remote content URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    /// Optional protocol-facing metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl AcpArtifact {
    /// Creates an inline ACP artifact.
    ///
    /// # Arguments
    ///
    /// * `name` - Artifact name
    /// * `mime_type` - Artifact MIME type
    /// * `content` - Inline artifact content
    ///
    /// # Errors
    ///
    /// Returns an error if the artifact is invalid.
    pub fn new_inline(name: String, mime_type: String, content: String) -> Result<Self> {
        let artifact = Self {
            name,
            mime_type,
            content: Some(content),
            content_url: None,
            metadata: BTreeMap::new(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Creates a remote ACP artifact.
    ///
    /// # Arguments
    ///
    /// * `name` - Artifact name
    /// * `mime_type` - Artifact MIME type
    /// * `content_url` - Artifact URL
    ///
    /// # Errors
    ///
    /// Returns an error if the artifact is invalid.
    pub fn new_remote(name: String, mime_type: String, content_url: String) -> Result<Self> {
        let artifact = Self {
            name,
            mime_type,
            content: None,
            content_url: Some(content_url),
            metadata: BTreeMap::new(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Validates ACP artifact rules.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - name is empty
    /// - mime type is empty
    /// - both `content` and `content_url` are present
    /// - neither `content` nor `content_url` is present
    /// - `content_url` is present but empty
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(
                crate::acp::error::AcpError::validation("artifact name cannot be empty").into(),
            );
        }

        if self.mime_type.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "artifact mime_type cannot be empty",
            )
            .into());
        }

        let has_content = self
            .content
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_content_url = self
            .content_url
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);

        match (has_content, has_content_url) {
            (true, false) | (false, true) => Ok(()),
            (true, true) => Err(crate::acp::error::AcpError::validation(
                "artifact fields 'content' and 'content_url' are mutually exclusive",
            )
            .into()),
            (false, false) => Err(crate::acp::error::AcpError::validation(
                "artifact requires exactly one of 'content' or 'content_url'",
            )
            .into()),
        }
    }
}

/// ACP message part.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpMessagePart, AcpTextPart};
///
/// let part = AcpMessagePart::Text(AcpTextPart::new("hello".to_string()));
/// assert!(part.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AcpMessagePart {
    /// Text part.
    Text(AcpTextPart),
    /// Artifact part.
    Artifact(AcpArtifact),
}

impl AcpMessagePart {
    /// Validates the message part.
    ///
    /// # Errors
    ///
    /// Returns an error if the part content is invalid.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Text(text) => text.validate(),
            Self::Artifact(artifact) => artifact.validate(),
        }
    }
}

/// ACP message.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
///
/// let message = AcpMessage::new(
///     AcpRole::Assistant,
///     vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
/// )
/// .unwrap();
///
/// assert_eq!(message.role, AcpRole::Assistant);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpMessage {
    /// Message role.
    pub role: AcpRole,
    /// Message parts.
    pub parts: Vec<AcpMessagePart>,
    /// Optional protocol-facing metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl AcpMessage {
    /// Creates a validated ACP message.
    ///
    /// # Arguments
    ///
    /// * `role` - Message role
    /// * `parts` - Message parts
    ///
    /// # Errors
    ///
    /// Returns an error if the message is invalid.
    pub fn new(role: AcpRole, parts: Vec<AcpMessagePart>) -> Result<Self> {
        let message = Self {
            role,
            parts,
            metadata: BTreeMap::new(),
        };
        message.validate()?;
        Ok(message)
    }

    /// Validates ACP message structure.
    ///
    /// # Errors
    ///
    /// Returns an error if the message has no parts or any part is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.parts.is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "ACP message must contain at least one message part",
            )
            .into());
        }

        for part in &self.parts {
            part.validate()?;
        }

        Ok(())
    }
}

/// ACP agent manifest.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpAgentManifest;
///
/// let manifest = AcpAgentManifest::new(
///     "xzatoma".to_string(),
///     "0.2.0".to_string(),
///     "XZatoma ACP Agent".to_string(),
/// );
///
/// assert!(manifest.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAgentManifest {
    /// ACP agent name.
    pub name: String,
    /// Agent version string.
    pub version: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl AcpAgentManifest {
    /// Creates a new ACP agent manifest.
    ///
    /// # Arguments
    ///
    /// * `name` - ACP agent name
    /// * `version` - Agent version
    /// * `display_name` - Display name
    ///
    /// # Returns
    ///
    /// Returns the created manifest.
    pub fn new(name: String, version: String, display_name: String) -> Self {
        Self {
            name,
            version,
            display_name,
            description: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Validates the ACP manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are invalid.
    pub fn validate(&self) -> Result<()> {
        validate_acp_identifier("manifest.name", &self.name)?;

        if self.version.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "manifest.version cannot be empty",
            )
            .into());
        }

        if self.display_name.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "manifest.display_name cannot be empty",
            )
            .into());
        }

        Ok(())
    }
}

/// ACP run identifier.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpRunId;
///
/// let run_id = AcpRunId::new("run_123".to_string()).unwrap();
/// assert_eq!(run_id.as_str(), "run_123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AcpRunId(String);

impl AcpRunId {
    /// Creates a validated run identifier.
    ///
    /// # Arguments
    ///
    /// * `value` - Candidate run identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the identifier is invalid.
    pub fn new(value: String) -> Result<Self> {
        validate_acp_identifier("run.id", &value)?;
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// ACP session identifier.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpSessionId;
///
/// let session_id = AcpSessionId::new("session_123".to_string()).unwrap();
/// assert_eq!(session_id.as_str(), "session_123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AcpSessionId(String);

impl AcpSessionId {
    /// Creates a validated session identifier.
    ///
    /// # Arguments
    ///
    /// * `value` - Candidate session identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the identifier is invalid.
    pub fn new(value: String) -> Result<Self> {
        validate_acp_identifier("session.id", &value)?;
        Ok(Self(value))
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// ACP session reference used by runs.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpRunSession, AcpSessionId};
///
/// let session = AcpRunSession::new(AcpSessionId::new("session_123".to_string()).unwrap()).unwrap();
/// assert_eq!(session.id.as_str(), "session_123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRunSession {
    /// Session identifier.
    pub id: AcpSessionId,
    /// Creation timestamp.
    pub created_at: String,
}

impl AcpRunSession {
    /// Creates a new ACP run session.
    ///
    /// # Arguments
    ///
    /// * `id` - Validated session identifier
    ///
    /// # Errors
    ///
    /// Returns an error if the generated session is invalid.
    pub fn new(id: AcpSessionId) -> Result<Self> {
        let session = Self {
            id,
            created_at: now_rfc3339(),
        };
        session.validate()?;
        Ok(session)
    }

    /// Validates the ACP run session.
    ///
    /// # Errors
    ///
    /// Returns an error if the timestamp is invalid.
    pub fn validate(&self) -> Result<()> {
        validate_rfc3339(&self.created_at, "session.created_at")
    }
}

/// ACP run lifecycle state.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpRunState;
///
/// assert!(AcpRunState::Created.can_transition_to(&AcpRunState::Queued));
/// assert!(!AcpRunState::Completed.can_transition_to(&AcpRunState::Running));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpRunState {
    /// Run created but not yet queued.
    Created,
    /// Run accepted and queued.
    Queued,
    /// Run is actively executing.
    Running,
    /// Run is awaiting external input.
    Awaiting,
    /// Run completed successfully.
    Completed,
    /// Run failed.
    Failed,
    /// Run was cancelled.
    Cancelled,
}

impl AcpRunState {
    /// Returns `true` if the state is terminal.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::types::AcpRunState;
    ///
    /// assert!(AcpRunState::Completed.is_terminal());
    /// assert!(!AcpRunState::Running.is_terminal());
    /// ```
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Determines whether the current state can transition to `next`.
    ///
    /// # Arguments
    ///
    /// * `next` - Next state
    ///
    /// # Returns
    ///
    /// Returns `true` if the transition is valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::types::AcpRunState;
    ///
    /// assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Completed));
    /// assert!(!AcpRunState::Completed.can_transition_to(&AcpRunState::Running));
    /// ```
    pub fn can_transition_to(self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Queued)
                | (Self::Queued, Self::Running)
                | (Self::Queued, Self::Cancelled)
                | (Self::Running, Self::Awaiting)
                | (Self::Running, Self::Completed)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
                | (Self::Awaiting, Self::Running)
                | (Self::Awaiting, Self::Failed)
                | (Self::Awaiting, Self::Cancelled)
        )
    }
}

/// ACP await payload.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpAwaitPayload;
///
/// let payload = AcpAwaitPayload::new("approval_required".to_string(), "Need approval".to_string());
/// assert_eq!(payload.kind, "approval_required");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpAwaitPayload {
    /// Await discriminator.
    pub kind: String,
    /// Human-readable detail.
    pub detail: String,
}

impl AcpAwaitPayload {
    /// Creates a new await payload.
    ///
    /// # Arguments
    ///
    /// * `kind` - Await kind
    /// * `detail` - Await detail
    ///
    /// # Returns
    ///
    /// Returns the created payload.
    pub fn new(kind: String, detail: String) -> Self {
        Self { kind, detail }
    }

    /// Validates the await payload.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are empty.
    pub fn validate(&self) -> Result<()> {
        if self.kind.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "await payload kind cannot be empty",
            )
            .into());
        }

        if self.detail.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "await payload detail cannot be empty",
            )
            .into());
        }

        Ok(())
    }
}

/// ACP run status record.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpRunState, AcpRunStatus};
///
/// let status = AcpRunStatus::new(AcpRunState::Created);
/// assert_eq!(status.state, AcpRunState::Created);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRunStatus {
    /// Current run state.
    pub state: AcpRunState,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Completion timestamp when terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Optional failure reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Optional cancellation reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
}

impl AcpRunStatus {
    /// Creates a new ACP run status.
    ///
    /// # Arguments
    ///
    /// * `state` - Initial run state
    ///
    /// # Returns
    ///
    /// Returns the created status with RFC 3339 timestamps.
    pub fn new(state: AcpRunState) -> Self {
        let now = now_rfc3339();
        Self {
            state,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
            failure_reason: None,
            cancellation_reason: None,
        }
    }
}

/// ACP run output accumulation.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpRunOutput;
///
/// let output = AcpRunOutput::default();
/// assert!(output.messages.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AcpRunOutput {
    /// Accumulated output messages.
    #[serde(default)]
    pub messages: Vec<AcpMessage>,
}

/// ACP run create request.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{
///     AcpMessage, AcpMessagePart, AcpRole, AcpRunCreateRequest, AcpSessionId, AcpTextPart,
/// };
///
/// let request = AcpRunCreateRequest::new(
///     AcpSessionId::new("session_123".to_string()).unwrap(),
///     vec![AcpMessage::new(
///         AcpRole::User,
///         vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
///     )
///     .unwrap()],
/// )
/// .unwrap();
///
/// assert_eq!(request.input.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRunCreateRequest {
    /// Session identifier.
    pub session_id: AcpSessionId,
    /// Input messages for the run.
    pub input: Vec<AcpMessage>,
    /// Optional metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl AcpRunCreateRequest {
    /// Creates a validated ACP run create request.
    ///
    /// # Arguments
    ///
    /// * `session_id` - Session identifier
    /// * `input` - Input messages
    ///
    /// # Errors
    ///
    /// Returns an error if the request is invalid.
    pub fn new(session_id: AcpSessionId, input: Vec<AcpMessage>) -> Result<Self> {
        let request = Self {
            session_id,
            input,
            metadata: BTreeMap::new(),
        };
        request.validate()?;
        Ok(request)
    }

    /// Validates the ACP run create request.
    ///
    /// # Errors
    ///
    /// Returns an error if no input messages are provided or any message is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.input.is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "run create request requires input messages",
            )
            .into());
        }

        for message in &self.input {
            message.validate()?;
        }

        Ok(())
    }
}

/// ACP run resume request.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpRunId, AcpRunResumeRequest};
///
/// let request = AcpRunResumeRequest::new(AcpRunId::new("run_123".to_string()).unwrap());
/// assert_eq!(request.run_id.as_str(), "run_123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRunResumeRequest {
    /// Run identifier.
    pub run_id: AcpRunId,
}

impl AcpRunResumeRequest {
    /// Creates a new ACP run resume request.
    ///
    /// # Arguments
    ///
    /// * `run_id` - Run identifier
    ///
    /// # Returns
    ///
    /// Returns the created request.
    pub fn new(run_id: AcpRunId) -> Self {
        Self { run_id }
    }
}

/// ACP run record.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{
///     AcpMessage, AcpMessagePart, AcpRole, AcpRun, AcpRunCreateRequest, AcpRunId, AcpRunSession,
///     AcpSessionId, AcpTextPart,
/// };
///
/// let session = AcpRunSession::new(AcpSessionId::new("session_123".to_string()).unwrap()).unwrap();
/// let request = AcpRunCreateRequest::new(
///     session.id.clone(),
///     vec![AcpMessage::new(
///         AcpRole::User,
///         vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
///     )
///     .unwrap()],
/// )
/// .unwrap();
///
/// let run = AcpRun::new(AcpRunId::new("run_123".to_string()).unwrap(), request, session).unwrap();
/// assert_eq!(run.id.as_str(), "run_123");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRun {
    /// Run identifier.
    pub id: AcpRunId,
    /// Associated session.
    pub session: AcpRunSession,
    /// Original create request.
    pub request: AcpRunCreateRequest,
    /// Current status.
    pub status: AcpRunStatus,
    /// Accumulated output.
    pub output: AcpRunOutput,
    /// Optional await payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub await_payload: Option<AcpAwaitPayload>,
}

impl AcpRun {
    /// Creates a validated ACP run.
    ///
    /// # Arguments
    ///
    /// * `id` - Run identifier
    /// * `request` - Create request
    /// * `session` - Associated session
    ///
    /// # Errors
    ///
    /// Returns an error if any component is invalid.
    pub fn new(id: AcpRunId, request: AcpRunCreateRequest, session: AcpRunSession) -> Result<Self> {
        request.validate()?;
        session.validate()?;

        Ok(Self {
            id,
            session,
            request,
            status: AcpRunStatus::new(AcpRunState::Created),
            output: AcpRunOutput::default(),
            await_payload: None,
        })
    }

    /// Transitions the run to the provided state.
    ///
    /// # Arguments
    ///
    /// * `next` - Target state
    ///
    /// # Errors
    ///
    /// Returns an error if the transition is unsupported.
    pub fn transition_to(&mut self, next: AcpRunState) -> Result<()> {
        if !self.status.state.can_transition_to(&next) {
            return Err(crate::acp::error::AcpError::UnsupportedTransition {
                from: self.status.state.to_string(),
                to: next.to_string(),
            }
            .into());
        }

        self.status.state = next;
        self.status.updated_at = now_rfc3339();

        if next.is_terminal() {
            self.status.completed_at = Some(self.status.updated_at.clone());
        }

        Ok(())
    }

    /// Appends an output message to the run.
    ///
    /// # Arguments
    ///
    /// * `message` - Output message
    ///
    /// # Errors
    ///
    /// Returns an error if the message is invalid.
    pub fn append_output_message(&mut self, message: AcpMessage) -> Result<()> {
        message.validate()?;
        self.output.messages.push(message);
        self.status.updated_at = now_rfc3339();
        Ok(())
    }

    /// Records a run failure.
    ///
    /// # Arguments
    ///
    /// * `reason` - Failure reason
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot transition to failed.
    pub fn record_failure(&mut self, reason: String) -> Result<()> {
        self.transition_to(AcpRunState::Failed)?;
        self.status.failure_reason = Some(reason);
        Ok(())
    }

    /// Records a run cancellation.
    ///
    /// # Arguments
    ///
    /// * `reason` - Cancellation reason
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot transition to cancelled.
    pub fn record_cancellation(&mut self, reason: String) -> Result<()> {
        self.transition_to(AcpRunState::Cancelled)?;
        self.status.cancellation_reason = Some(reason);
        Ok(())
    }

    /// Sets the await payload and transitions the run to awaiting.
    ///
    /// # Arguments
    ///
    /// * `kind` - Await kind
    /// * `detail` - Await detail
    ///
    /// # Errors
    ///
    /// Returns an error if the payload is invalid or the transition is unsupported.
    pub fn set_await_payload(&mut self, kind: String, detail: String) -> Result<()> {
        let payload = AcpAwaitPayload::new(kind, detail);
        payload.validate()?;
        self.transition_to(AcpRunState::Awaiting)?;
        self.await_payload = Some(payload);
        Ok(())
    }
}

impl std::fmt::Display for AcpRunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => f.write_str("created"),
            Self::Queued => f.write_str("queued"),
            Self::Running => f.write_str("running"),
            Self::Awaiting => f.write_str("awaiting"),
            Self::Completed => f.write_str("completed"),
            Self::Failed => f.write_str("failed"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

/// ACP event kind.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpEventKind;
///
/// assert_eq!(AcpEventKind::RunStatusChanged.to_string(), "run_status_changed");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpEventKind {
    /// Session created event.
    SessionCreated,
    /// Run created event.
    RunCreated,
    /// Run status changed event.
    RunStatusChanged,
    /// Run output appended event.
    RunOutputAppended,
    /// Run completed event.
    RunCompleted,
    /// Run failed event.
    RunFailed,
    /// Run cancelled event.
    RunCancelled,
    /// Run awaiting input event.
    RunAwaitingInput,
}

impl std::fmt::Display for AcpEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionCreated => f.write_str("session_created"),
            Self::RunCreated => f.write_str("run_created"),
            Self::RunStatusChanged => f.write_str("run_status_changed"),
            Self::RunOutputAppended => f.write_str("run_output_appended"),
            Self::RunCompleted => f.write_str("run_completed"),
            Self::RunFailed => f.write_str("run_failed"),
            Self::RunCancelled => f.write_str("run_cancelled"),
            Self::RunAwaitingInput => f.write_str("run_awaiting_input"),
        }
    }
}

/// ACP event record.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{AcpEvent, AcpEventKind};
///
/// let event = AcpEvent::new(
///     AcpEventKind::SessionCreated,
///     None,
///     serde_json::json!({"session": "session_1"}),
/// )
/// .unwrap();
///
/// assert_eq!(event.kind, AcpEventKind::SessionCreated);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpEvent {
    /// Event kind.
    pub kind: AcpEventKind,
    /// Optional run identifier for run-scoped events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Creation timestamp in RFC 3339.
    pub created_at: String,
    /// Event payload.
    pub payload: Value,
}

impl AcpEvent {
    /// Creates a validated ACP event.
    ///
    /// # Arguments
    ///
    /// * `kind` - Event kind
    /// * `run_id` - Optional run identifier
    /// * `payload` - Event payload
    ///
    /// # Errors
    ///
    /// Returns an error if the event is invalid.
    pub fn new(kind: AcpEventKind, run_id: Option<String>, payload: Value) -> Result<Self> {
        let event = Self {
            kind,
            run_id,
            created_at: now_rfc3339(),
            payload,
        };
        event.validate()?;
        Ok(event)
    }

    /// Validates the ACP event.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn validate(&self) -> Result<()> {
        validate_rfc3339(&self.created_at, "event.created_at")?;

        let run_scoped = matches!(
            self.kind,
            AcpEventKind::RunCreated
                | AcpEventKind::RunStatusChanged
                | AcpEventKind::RunOutputAppended
                | AcpEventKind::RunCompleted
                | AcpEventKind::RunFailed
                | AcpEventKind::RunCancelled
                | AcpEventKind::RunAwaitingInput
        );

        if run_scoped {
            let Some(run_id) = &self.run_id else {
                return Err(crate::acp::error::AcpError::validation(format!(
                    "event kind '{}' requires a run_id",
                    self.kind
                ))
                .into());
            };
            validate_acp_identifier("event.run_id", run_id)?;
        }

        Ok(())
    }
}

/// ACP protocol error payload.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::AcpError;
///
/// let error = AcpError::validation("invalid message");
/// assert_eq!(error.code, "validation_error");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpError {
    /// Stable ACP error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl AcpError {
    /// Creates a validation ACP error.
    ///
    /// # Arguments
    ///
    /// * `message` - Error message
    ///
    /// # Returns
    ///
    /// Returns a validation ACP error payload.
    pub fn validation(message: &str) -> Self {
        Self {
            code: "validation_error".to_string(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// Validates an RFC 3339 timestamp string.
///
/// # Arguments
///
/// * `value` - Timestamp string
/// * `field` - Field name for diagnostics
///
/// # Errors
///
/// Returns an error if the timestamp is not valid RFC 3339.
pub fn validate_rfc3339(value: &str, field: &str) -> Result<()> {
    chrono::DateTime::parse_from_rfc3339(value).map_err(|error| {
        crate::acp::error::AcpError::validation(format!(
            "field '{}' must be a valid RFC 3339 timestamp: {}",
            field, error
        ))
    })?;
    Ok(())
}

/// Converts an existing provider message into an ACP message.
///
/// This adapter is used to expose current XZatoma behavior through ACP-facing
/// message output without rewriting the provider layer.
///
/// # Arguments
///
/// * `message` - Existing provider message
///
/// # Errors
///
/// Returns an error if the provider message role is unsupported or the content
/// cannot produce a valid ACP message.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::types::{agent_message_to_acp_message, AcpRole};
/// use xzatoma::providers::Message;
///
/// let provider_message = Message::user("hello");
/// let acp_message = agent_message_to_acp_message(&provider_message).unwrap();
/// assert_eq!(acp_message.role, AcpRole::User);
/// ```
pub fn agent_message_to_acp_message(message: &providers::Message) -> Result<AcpMessage> {
    let role = AcpRole::parse(&message.role)?;

    let content = message.content.as_deref().unwrap_or_default().to_string();
    let part = AcpMessagePart::Text(AcpTextPart::new(content));
    AcpMessage::new(role, vec![part])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_acp_identifier_accepts_valid_value() {
        assert!(validate_acp_identifier("test", "run_123").is_ok());
    }

    #[test]
    fn test_validate_acp_identifier_rejects_uppercase() {
        assert!(validate_acp_identifier("test", "Run_123").is_err());
    }

    #[test]
    fn test_validate_acp_role_accepts_supported_role() {
        assert!(validate_acp_role("assistant").is_ok());
    }

    #[test]
    fn test_validate_acp_role_rejects_unsupported_role() {
        assert!(validate_acp_role("developer").is_err());
    }

    #[test]
    fn test_text_part_validation_rejects_empty_text() {
        let part = AcpTextPart::new(String::new());
        assert!(part.validate().is_err());
    }

    #[test]
    fn test_artifact_validation_rejects_both_content_and_content_url() {
        let artifact = AcpArtifact {
            name: "bad.txt".to_string(),
            mime_type: "text/plain".to_string(),
            content: Some("inline".to_string()),
            content_url: Some("https://example.com/file.txt".to_string()),
            metadata: BTreeMap::new(),
        };

        assert!(artifact.validate().is_err());
    }

    #[test]
    fn test_message_validation_rejects_empty_parts() {
        let message = AcpMessage {
            role: AcpRole::User,
            parts: Vec::new(),
            metadata: BTreeMap::new(),
        };

        assert!(message.validate().is_err());
    }

    #[test]
    fn test_run_state_transition_rules() {
        assert!(AcpRunState::Created.can_transition_to(&AcpRunState::Queued));
        assert!(AcpRunState::Queued.can_transition_to(&AcpRunState::Running));
        assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Completed));
        assert!(!AcpRunState::Completed.can_transition_to(&AcpRunState::Running));
    }

    #[test]
    fn test_event_round_trip_serialization() {
        let event = AcpEvent::new(
            AcpEventKind::RunStatusChanged,
            Some("run_123".to_string()),
            serde_json::json!({"state": "running"}),
        )
        .unwrap();

        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: AcpEvent = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.kind, AcpEventKind::RunStatusChanged);
        assert_eq!(decoded.run_id.as_deref(), Some("run_123"));
    }

    #[test]
    fn test_now_rfc3339_returns_valid_timestamp() {
        let timestamp = now_rfc3339();
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    }

    #[test]
    fn test_agent_message_to_acp_message_converts_provider_message() {
        let provider_message = providers::Message::assistant("hello");
        let message = agent_message_to_acp_message(&provider_message).unwrap();

        assert_eq!(message.role, AcpRole::Assistant);
        assert_eq!(message.parts.len(), 1);

        match &message.parts[0] {
            AcpMessagePart::Text(text) => assert_eq!(text.text, "hello"),
            other => panic!("expected text part, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_message_to_acp_message_rejects_unknown_role() {
        let provider_message = providers::Message {
            role: "developer".to_string(),
            content: Some("hello".to_string()),
            content_parts: None,
            tool_calls: None,
            tool_call_id: None,
        };

        assert!(agent_message_to_acp_message(&provider_message).is_err());
    }
}
