/// ACP run lifecycle compatibility exports.
///
/// This module re-exports the canonical Phase 1 ACP run and lifecycle types from
/// `crate::acp::types`.
///
/// The ACP implementation uses `types.rs` as the single source of truth for:
///
/// - run identifiers
/// - session identifiers used by runs
/// - run create and resume requests
/// - run state transitions
/// - run status records
/// - output accumulation
/// - await payloads
/// - RFC 3339 timestamp helpers
///
/// Keeping `run.rs` as a thin compatibility layer preserves the planned module
/// layout while avoiding duplicate lifecycle models.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::run::{
///     now_rfc3339, AcpMessage, AcpMessagePart, AcpRole, AcpRun, AcpRunCreateRequest, AcpRunId,
///     AcpRunSession, AcpRunState, AcpSessionId, AcpTextPart,
/// };
///
/// let session = AcpRunSession::new(AcpSessionId::new("session_123".to_string())?)?;
/// let request = AcpRunCreateRequest::new(
///     session.id.clone(),
///     vec![AcpMessage::new(
///         AcpRole::User,
///         vec![AcpMessagePart::Text(AcpTextPart::new(
///             "Build the ACP layer".to_string(),
///         ))],
///     )?],
/// )?;
///
/// let mut run = AcpRun::new(AcpRunId::new("run_123".to_string())?, request, session)?;
/// run.transition_to(AcpRunState::Queued)?;
/// run.transition_to(AcpRunState::Running)?;
/// run.append_output_message(AcpMessage::new(
///     AcpRole::Assistant,
///     vec![AcpMessagePart::Text(AcpTextPart::new(
///         "Starting execution".to_string(),
///     ))],
/// )?)?;
///
/// assert_eq!(run.status.state, AcpRunState::Running);
/// assert!(!now_rfc3339().is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub use crate::acp::types::{
    now_rfc3339, validate_acp_identifier as validate_run_identifier, validate_rfc3339,
    AcpAwaitPayload as AwaitPayload, AcpMessage, AcpMessagePart, AcpRole, AcpRun,
    AcpRunCreateRequest, AcpRunId as RunId, AcpRunOutput as RunOutput,
    AcpRunResumeRequest as RunResumeRequest, AcpRunSession as Session, AcpRunState as RunStatus,
    AcpRunStatus, AcpSessionId as SessionId, AcpTextPart,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn test_message(role: AcpRole, text: &str) -> AcpMessage {
        AcpMessage::new(
            role,
            vec![AcpMessagePart::Text(AcpTextPart::new(text.to_string()))],
        )
        .expect("message should be valid")
    }

    #[test]
    fn test_run_module_re_exports_canonical_run_types() {
        let session = Session::new(
            SessionId::new("session_123".to_string()).expect("session id should be valid"),
        )
        .expect("session should be created");
        let request = AcpRunCreateRequest::new(
            session.id.clone(),
            vec![test_message(AcpRole::User, "Execute the task")],
        )
        .expect("request should be valid");

        let run = AcpRun::new(
            RunId::new("run_123".to_string()).expect("run id should be valid"),
            request,
            session,
        )
        .expect("run should be valid");

        assert_eq!(run.id.as_str(), "run_123");
        assert_eq!(run.status.state, RunStatus::Created);
    }

    #[test]
    fn test_run_module_re_exports_now_rfc3339() {
        let timestamp = now_rfc3339();
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    }

    #[test]
    fn test_run_module_re_exports_identifier_validation() {
        assert!(validate_run_identifier("run.id", "run_123").is_ok());
        assert!(validate_run_identifier("run.id", "Run_123").is_err());
    }
}
