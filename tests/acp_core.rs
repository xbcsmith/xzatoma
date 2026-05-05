use chrono::{DateTime, Utc};
use xzatoma::acp::{
    agent_message_to_acp_message, AcpAgentManifest, AcpArtifact, AcpEvent, AcpEventKind,
    AcpMessage, AcpMessagePart, AcpRole, AcpRun, AcpRunCreateRequest, AcpRunId,
    AcpRunResumeRequest, AcpRunSession, AcpRunState, AcpSessionId, AcpTextPart,
};
use xzatoma::providers::Message;

/// Creates a minimal valid ACP text message for tests.
fn test_text_message(role: AcpRole, text: &str) -> AcpMessage {
    AcpMessage::new(
        role,
        vec![AcpMessagePart::Text(AcpTextPart::new(text.to_string()))],
    )
    .expect("test message should be valid")
}

#[test]
fn test_manifest_validation_accepts_valid_manifest() {
    let manifest = AcpAgentManifest::new(
        "xzatoma".to_string(),
        "0.2.0".to_string(),
        "XZatoma ACP Agent".to_string(),
    );

    assert!(manifest.validate().is_ok());
}

#[test]
fn test_manifest_validation_rejects_invalid_agent_name() {
    let manifest = AcpAgentManifest::new(
        "Invalid Agent Name".to_string(),
        "0.2.0".to_string(),
        "Bad manifest".to_string(),
    );

    let error = manifest.validate().expect_err("manifest should be invalid");
    assert!(error.to_string().contains("name"));
}

#[test]
fn test_message_part_validation_accepts_text_part() {
    let part = AcpMessagePart::Text(AcpTextPart::new("hello from acp".to_string()));
    assert!(part.validate().is_ok());
}

#[test]
fn test_message_part_validation_rejects_empty_text_part() {
    let part = AcpMessagePart::Text(AcpTextPart::new(String::new()));
    assert!(part.validate().is_err());
}

#[test]
fn test_artifact_validation_accepts_content_without_content_url() {
    let artifact = AcpArtifact::new_inline(
        "result.txt".to_string(),
        "text/plain".to_string(),
        "integration output".to_string(),
    )
    .expect("artifact should be valid");

    assert!(artifact.validate().is_ok());
    assert!(artifact.content.is_some());
    assert!(artifact.content_url.is_none());
}

#[test]
fn test_artifact_validation_accepts_content_url_without_content() {
    let artifact = AcpArtifact::new_remote(
        "result.txt".to_string(),
        "text/plain".to_string(),
        "https://example.com/artifacts/result.txt".to_string(),
    )
    .expect("artifact should be valid");

    assert!(artifact.validate().is_ok());
    assert!(artifact.content.is_none());
    assert!(artifact.content_url.is_some());
}

#[test]
fn test_artifact_validation_rejects_content_and_content_url_together() {
    let artifact = AcpArtifact {
        name: "bad.txt".to_string(),
        mime_type: "text/plain".to_string(),
        content: Some("inline".to_string()),
        content_url: Some("https://example.com/bad.txt".to_string()),
        metadata: Default::default(),
    };

    let error = artifact.validate().expect_err("artifact should be invalid");
    assert!(error.to_string().contains("content"));
    assert!(error.to_string().contains("content_url"));
}

#[test]
fn test_run_state_transition_rules_allow_expected_forward_progress() {
    assert!(AcpRunState::Created.can_transition_to(&AcpRunState::Queued));
    assert!(AcpRunState::Queued.can_transition_to(&AcpRunState::Running));
    assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Awaiting));
    assert!(AcpRunState::Awaiting.can_transition_to(&AcpRunState::Running));
    assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Completed));
    assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Failed));
    assert!(AcpRunState::Running.can_transition_to(&AcpRunState::Cancelled));
}

#[test]
fn test_run_state_transition_rules_reject_invalid_transition() {
    assert!(!AcpRunState::Completed.can_transition_to(&AcpRunState::Running));
    assert!(!AcpRunState::Failed.can_transition_to(&AcpRunState::Queued));
    assert!(!AcpRunState::Cancelled.can_transition_to(&AcpRunState::Running));
    assert!(!AcpRunState::Created.can_transition_to(&AcpRunState::Completed));
}

#[test]
fn test_run_transition_updates_status_deterministically() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_123".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Build the ACP layer")],
    )
    .expect("request should be valid");

    let mut run = AcpRun::new(
        AcpRunId::new("run_123".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    assert_eq!(run.status.state, AcpRunState::Created);

    run.transition_to(AcpRunState::Queued)
        .expect("created -> queued should succeed");
    assert_eq!(run.status.state, AcpRunState::Queued);

    run.transition_to(AcpRunState::Running)
        .expect("queued -> running should succeed");
    assert_eq!(run.status.state, AcpRunState::Running);

    run.transition_to(AcpRunState::Completed)
        .expect("running -> completed should succeed");
    assert_eq!(run.status.state, AcpRunState::Completed);
    assert!(run.status.completed_at.is_some());
}

#[test]
fn test_run_transition_records_failure() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_failure".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Fail this run")],
    )
    .expect("request should be valid");

    let mut run = AcpRun::new(
        AcpRunId::new("run_failure".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    run.transition_to(AcpRunState::Queued)
        .expect("created -> queued should succeed");
    run.transition_to(AcpRunState::Running)
        .expect("queued -> running should succeed");
    run.record_failure("provider timeout".to_string())
        .expect("failure should be recorded");

    assert_eq!(run.status.state, AcpRunState::Failed);
    assert_eq!(
        run.status.failure_reason.as_deref(),
        Some("provider timeout")
    );
    assert!(run.status.completed_at.is_some());
}

#[test]
fn test_run_transition_records_cancellation() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_cancel".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Cancel this run")],
    )
    .expect("request should be valid");

    let mut run = AcpRun::new(
        AcpRunId::new("run_cancel".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    run.transition_to(AcpRunState::Queued)
        .expect("created -> queued should succeed");
    run.transition_to(AcpRunState::Running)
        .expect("queued -> running should succeed");
    run.record_cancellation("user requested stop".to_string())
        .expect("cancellation should be recorded");

    assert_eq!(run.status.state, AcpRunState::Cancelled);
    assert_eq!(
        run.status.cancellation_reason.as_deref(),
        Some("user requested stop")
    );
    assert!(run.status.completed_at.is_some());
}

#[test]
fn test_run_supports_output_accumulation() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_output".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Generate output")],
    )
    .expect("request should be valid");

    let mut run = AcpRun::new(
        AcpRunId::new("run_output".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    run.append_output_message(test_text_message(AcpRole::Assistant, "First response"))
        .expect("append should succeed");
    run.append_output_message(test_text_message(AcpRole::Assistant, "Second response"))
        .expect("append should succeed");

    assert_eq!(run.output.messages.len(), 2);
}

#[test]
fn test_run_supports_optional_await_payload() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_await".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Wait for approval")],
    )
    .expect("request should be valid");

    let mut run = AcpRun::new(
        AcpRunId::new("run_await".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    run.transition_to(AcpRunState::Queued)
        .expect("created -> queued should succeed");
    run.transition_to(AcpRunState::Running)
        .expect("queued -> running should succeed");
    run.set_await_payload(
        "approval_required".to_string(),
        "Need user confirmation".to_string(),
    )
    .expect("await payload should be set");

    assert_eq!(run.status.state, AcpRunState::Awaiting);
    assert!(run.await_payload.is_some());
}

#[test]
fn test_event_serialization_round_trip() {
    let event = AcpEvent::new(
        AcpEventKind::RunStatusChanged,
        Some("run_serialize".to_string()),
        serde_json::json!({
            "state": "running",
            "message": "run entered execution"
        }),
    )
    .expect("event should be valid");

    let serialized = serde_json::to_string(&event).expect("event should serialize");
    let restored: AcpEvent =
        serde_json::from_str(&serialized).expect("event should deserialize cleanly");

    assert_eq!(restored.kind, AcpEventKind::RunStatusChanged);
    assert_eq!(restored.run_id.as_deref(), Some("run_serialize"));
    assert_eq!(restored.payload["state"], "running");
}

#[test]
fn test_rfc3339_timestamp_formatting_is_valid() {
    let timestamp = xzatoma::acp::now_rfc3339();
    let parsed = DateTime::parse_from_rfc3339(&timestamp);
    assert!(parsed.is_ok(), "timestamp should be RFC 3339");
}

#[test]
fn test_provider_message_conversion_maps_user_message_to_acp_message() {
    let provider_message = Message::user("Hello ACP");
    let acp_message =
        agent_message_to_acp_message(&provider_message).expect("conversion should succeed");

    assert_eq!(acp_message.role, AcpRole::User);
    assert_eq!(acp_message.parts.len(), 1);

    match &acp_message.parts[0] {
        AcpMessagePart::Text(text) => assert_eq!(text.text, "Hello ACP"),
        other => panic!("expected text part, got {other:?}"),
    }
}

#[test]
fn test_provider_message_conversion_maps_assistant_message_to_acp_message() {
    let provider_message = Message::assistant("I can help with that.");
    let acp_message =
        agent_message_to_acp_message(&provider_message).expect("conversion should succeed");

    assert_eq!(acp_message.role, AcpRole::Assistant);
    assert_eq!(acp_message.parts.len(), 1);
}

#[test]
fn test_provider_message_conversion_maps_tool_message_to_acp_message() {
    let provider_message = Message::tool_result("call_123", "tool output");
    let acp_message =
        agent_message_to_acp_message(&provider_message).expect("conversion should succeed");

    assert_eq!(acp_message.role, AcpRole::Tool);
    assert_eq!(acp_message.parts.len(), 1);
}

#[test]
fn test_provider_message_conversion_rejects_unknown_role() {
    let provider_message = Message {
        role: "unknown".to_string(),
        content: Some("mystery".to_string()),
        content_parts: None,
        tool_calls: None,
        tool_call_id: None,
    };

    let result = agent_message_to_acp_message(&provider_message);
    assert!(result.is_err());

    let error = result.expect_err("unknown role should fail");
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn test_provider_message_conversion_preserves_empty_content_as_validation_error() {
    let provider_message = Message {
        role: "assistant".to_string(),
        content: Some(String::new()),
        content_parts: None,
        tool_calls: None,
        tool_call_id: None,
    };

    let result = agent_message_to_acp_message(&provider_message);
    assert!(result.is_err());
}

#[test]
fn test_run_create_request_validation_rejects_empty_input_messages() {
    let session_id =
        AcpSessionId::new("session_empty_input".to_string()).expect("session id should be valid");
    let result = AcpRunCreateRequest::new(session_id, Vec::new());
    assert!(result.is_err());
}

#[test]
fn test_run_resume_request_holds_run_identifier() {
    let run_id = AcpRunId::new("run_resume".to_string()).expect("run id should be valid");
    let request = AcpRunResumeRequest::new(run_id.clone());

    assert_eq!(request.run_id.as_str(), run_id.as_str());
}

#[test]
fn test_session_identifier_validation_rejects_invalid_format() {
    let result = AcpSessionId::new("bad session id".to_string());
    assert!(result.is_err());
}

#[test]
fn test_run_identifier_validation_rejects_invalid_format() {
    let result = AcpRunId::new("bad run id".to_string());
    assert!(result.is_err());
}

#[test]
fn test_message_validation_rejects_empty_parts() {
    let result = AcpMessage::new(AcpRole::User, Vec::new());
    assert!(result.is_err());
}

#[test]
fn test_event_validation_rejects_missing_run_id_for_run_scoped_kind() {
    let result = AcpEvent::new(
        AcpEventKind::RunStatusChanged,
        None,
        serde_json::json!({"state": "running"}),
    );
    assert!(result.is_err());
}

#[test]
fn test_event_created_at_is_rfc3339() {
    let event = AcpEvent::new(
        AcpEventKind::SessionCreated,
        None,
        serde_json::json!({"session": "session_1"}),
    )
    .expect("event should be valid");

    let parsed = DateTime::parse_from_rfc3339(&event.created_at);
    assert!(parsed.is_ok());
}

#[test]
fn test_run_created_at_and_updated_at_are_rfc3339() {
    let session =
        AcpRunSession::new(AcpSessionId::new("session_time".to_string()).expect("session id"))
            .expect("session should be valid");
    let request = AcpRunCreateRequest::new(
        session.id.clone(),
        vec![test_text_message(AcpRole::User, "Track timestamps")],
    )
    .expect("request should be valid");

    let run = AcpRun::new(
        AcpRunId::new("run_time".to_string()).expect("id"),
        request,
        session,
    )
    .expect("run should be valid");

    let created = DateTime::parse_from_rfc3339(&run.status.created_at);
    let updated = DateTime::parse_from_rfc3339(&run.status.updated_at);

    assert!(created.is_ok());
    assert!(updated.is_ok());

    let created_utc: DateTime<Utc> = created.expect("created").with_timezone(&Utc);
    let updated_utc: DateTime<Utc> = updated.expect("updated").with_timezone(&Utc);
    assert!(updated_utc >= created_utc);
}
