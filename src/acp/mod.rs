/// ACP domain model and core abstractions.
///
/// This module exposes the canonical Phase 1 ACP surface for XZatoma.
///
/// The ACP domain is intentionally transport-independent. Protocol-facing types
/// live in focused submodules, but the crate-level ACP API is unified here so
/// later phases can depend on one stable surface.
///
/// Phase 1 uses `types.rs` as the canonical protocol and lifecycle model for:
///
/// - ACP messages and message parts
/// - ACP artifacts
/// - ACP manifests
/// - ACP runs and run statuses
/// - ACP sessions
/// - ACP events used for history and streaming
/// - adapter helpers for converting provider messages into ACP messages
///
/// The sibling modules `run.rs`, `events.rs`, and `session.rs` may contain
/// supporting or compatibility abstractions, but consumers should prefer the
/// unified exports from this module.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::{
///     agent_message_to_acp_message, now_rfc3339, AcpMessage, AcpMessagePart, AcpRole,
///     AcpTextPart,
/// };
/// use xzatoma::providers::Message;
///
/// let provider_message = Message::assistant("Hello from XZatoma");
/// let acp_message = agent_message_to_acp_message(&provider_message)?;
///
/// assert_eq!(acp_message.role, AcpRole::Assistant);
/// assert!(!now_rfc3339().is_empty());
///
/// let direct_message = AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Do the work".to_string()))],
/// )?;
/// assert_eq!(direct_message.role, AcpRole::User);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub mod error;
pub mod events;
pub mod handlers;
pub mod manifest;
pub mod routes;
pub mod run;
pub mod server;
pub mod session;
pub mod types;

pub use error::{AcpError, AcpValidationError};
pub use manifest::{validate_agent_name, AcpAgentCapability, AcpAgentManifest};
pub use types::{
    agent_message_to_acp_message, now_rfc3339, validate_acp_identifier, validate_acp_role,
    validate_rfc3339, AcpAgentManifest as CanonicalAcpAgentManifest, AcpArtifact, AcpAwaitPayload,
    AcpError as ProtocolAcpError, AcpEvent, AcpEventKind, AcpMessage, AcpMessagePart, AcpRole,
    AcpRun, AcpRunCreateRequest, AcpRunId, AcpRunOutput, AcpRunResumeRequest, AcpRunSession,
    AcpRunState, AcpRunStatus, AcpSessionId, AcpTextPart,
};

use crate::error::Result;
use crate::providers::Message;

/// Converts a slice of provider-layer messages into ACP messages.
///
/// This helper applies [`agent_message_to_acp_message`] to each message in
/// order, preserving the existing conversation sequence.
///
/// # Arguments
///
/// * `messages` - Provider messages to convert
///
/// # Returns
///
/// Returns a vector of ACP messages in the same order as the input slice.
///
/// # Errors
///
/// Returns an error if any message fails ACP conversion.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::{provider_messages_to_acp_messages, AcpRole};
/// use xzatoma::providers::Message;
///
/// let messages = vec![
///     Message::system("You are helpful"),
///     Message::user("Summarize this task"),
/// ];
///
/// let converted = provider_messages_to_acp_messages(&messages)?;
/// assert_eq!(converted.len(), 2);
/// assert_eq!(converted[0].role, AcpRole::System);
/// assert_eq!(converted[1].role, AcpRole::User);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn provider_messages_to_acp_messages(messages: &[Message]) -> Result<Vec<AcpMessage>> {
    messages
        .iter()
        .map(agent_message_to_acp_message)
        .collect::<Result<Vec<_>>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Message;

    #[test]
    fn test_provider_messages_to_acp_messages_converts_multiple_messages() {
        let messages = vec![
            Message::system("system"),
            Message::user("user"),
            Message::assistant("assistant"),
        ];

        let converted = provider_messages_to_acp_messages(&messages).unwrap();

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, AcpRole::System);
        assert_eq!(converted[1].role, AcpRole::User);
        assert_eq!(converted[2].role, AcpRole::Assistant);
    }

    #[test]
    fn test_now_rfc3339_returns_non_empty_timestamp() {
        let timestamp = now_rfc3339();
        assert!(!timestamp.is_empty());
        assert!(timestamp.contains('T'));
    }
}
