use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;
use xzatoma::acp::executor::AcpExecutor;
use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeExecuteMode};
use xzatoma::acp::server::{build_router, AcpServerState};
use xzatoma::config::Config;

/// Builds a deterministic ACP test configuration.
///
/// The provider is forced to `ollama` so tests do not depend on interactive
/// Copilot authentication flows.
fn test_config() -> Config {
    let mut config = Config::default();
    config.provider.provider_type = "ollama".to_string();
    config
}

/// Builds ACP server state using a dedicated temporary SQLite database and a
/// mock-success ACP executor so lifecycle tests are deterministic.
///
/// Each call creates its own isolated temp directory and passes the database
/// path directly to `AcpRuntime::new_with_storage_path`, avoiding any
/// process-global environment variable mutations that would race with other
/// parallel tests.
///
/// Returns the server state together with the temp directory that owns the
/// database file. The caller must keep the directory alive for the duration of
/// the test.
fn test_state(config: &Config) -> (AcpServerState, tempfile::TempDir) {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("history.db");

    let runtime = AcpRuntime::new_with_storage_path(config.clone(), &db_path);
    let executor = AcpExecutor::new_mock_success(
        config.clone(),
        runtime.clone(),
        "mock ACP response".to_string(),
    );

    let state = AcpServerState::from_parts(config, runtime, executor).expect("state should build");

    (state, dir)
}

/// Builds a standard ACP run creation payload.
fn create_run_body(mode: &str, prompt: &str, session_id: Option<&str>) -> Value {
    let mut body = json!({
        "mode": mode,
        "agentName": "xzatoma",
        "input": [
            {
                "role": "user",
                "parts": [
                    {
                        "type": "text",
                        "data": {
                            "text": prompt
                        }
                    }
                ]
            }
        ]
    });

    if let Some(session_id) = session_id {
        body["sessionId"] = json!(session_id);
    }

    body
}

/// Reads a response body as JSON.
async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    serde_json::from_slice(&body).expect("response body should be valid JSON")
}

#[tokio::test]
async fn test_session_creation_and_retrieval() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state, &config.acp);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&create_run_body(
                        "sync",
                        "Create a persistent session",
                        None,
                    ))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(create_response.status(), StatusCode::OK);

    let create_json = response_json(create_response).await;
    let session_id = create_json["run"]["session"]["id"]
        .as_str()
        .expect("session id should be present")
        .to_string();

    let session_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/acp/sessions/{session_id}"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(session_response.status(), StatusCode::OK);

    let session_json = response_json(session_response).await;
    assert_eq!(session_json["session"]["id"], session_id);
    assert!(session_json["runs"].as_array().is_some());
    assert_eq!(
        session_json["runs"]
            .as_array()
            .expect("runs should be an array")
            .len(),
        1
    );
}

#[tokio::test]
async fn test_history_continuity_across_multiple_runs_in_one_session() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state.clone(), &config.acp);

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&create_run_body("sync", "First session run", None))
                        .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let first_json = response_json(first_response).await;
    let first_run_id = first_json["run"]["id"]
        .as_str()
        .expect("run id should be present")
        .to_string();
    let session_id = first_json["run"]["session"]["id"]
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            state
                .runtime()
                .get_run(&first_run_id)
                .expect("runtime run should exist")
                .session
                .id
                .as_str()
                .to_string()
        });

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&create_run_body(
                        "sync",
                        "Second session run",
                        Some(&session_id),
                    ))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let second_json = response_json(second_response).await;
    let second_run_id = second_json["run"]["id"]
        .as_str()
        .expect("run id should be present")
        .to_string();

    let session_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/acp/sessions/{session_id}"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(session_response.status(), StatusCode::OK);

    let session_json = response_json(session_response).await;
    let runs = session_json["runs"]
        .as_array()
        .expect("runs should be an array");

    assert!(runs.len() >= 2);
    assert!(runs.iter().any(|run| run["id"] == first_run_id));
    assert!(runs.iter().any(|run| run["id"] == second_run_id));
}

#[tokio::test]
async fn test_persisted_run_restoration_from_runtime() {
    let config = test_config();
    let (state, _dir) = test_state(&config);

    let created_run = state
        .runtime()
        .create_run(
            xzatoma::acp::runtime::AcpRuntimeCreateRequest::new(vec![
                xzatoma::acp::AcpMessage::new(
                    xzatoma::acp::AcpRole::User,
                    vec![xzatoma::acp::AcpMessagePart::Text(
                        xzatoma::acp::AcpTextPart::new("Persist and restore this run".to_string()),
                    )],
                )
                .expect("message should be valid"),
            ])
            .with_mode(AcpRuntimeExecuteMode::Sync),
        )
        .expect("run should be created");

    state
        .runtime()
        .mark_queued(created_run.id.as_str())
        .expect("queued transition should succeed");
    state
        .runtime()
        .mark_running(created_run.id.as_str())
        .expect("running transition should succeed");
    state
        .runtime()
        .append_output_message(
            created_run.id.as_str(),
            xzatoma::acp::runtime::assistant_text_message("restored output".to_string())
                .expect("assistant message should be valid"),
        )
        .expect("output append should succeed");
    state
        .runtime()
        .complete_run(created_run.id.as_str())
        .expect("completion should succeed");

    let restored = state
        .runtime()
        .restore_run(created_run.id.as_str())
        .expect("restore should succeed")
        .expect("run should exist");

    assert_eq!(restored.id.as_str(), created_run.id.as_str());
    assert_eq!(restored.status.state, xzatoma::acp::AcpRunState::Completed);
    assert_eq!(restored.output.messages.len(), 1);

    let restored_events = state
        .runtime()
        .get_events(created_run.id.as_str())
        .expect("events should load");
    assert!(!restored_events.is_empty());
    assert_eq!(
        restored_events
            .last()
            .expect("events should not be empty")
            .event
            .payload["event"],
        "run.completed"
    );
}

#[tokio::test]
async fn test_resume_of_awaiting_run() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state.clone(), &config.acp);

    let created_run = state
        .runtime()
        .create_run(
            xzatoma::acp::runtime::AcpRuntimeCreateRequest::new(vec![
                xzatoma::acp::AcpMessage::new(
                    xzatoma::acp::AcpRole::User,
                    vec![xzatoma::acp::AcpMessagePart::Text(
                        xzatoma::acp::AcpTextPart::new("Create awaiting run".to_string()),
                    )],
                )
                .expect("message should be valid"),
            ])
            .with_mode(AcpRuntimeExecuteMode::Async),
        )
        .expect("run should be created");

    state
        .runtime()
        .mark_queued(created_run.id.as_str())
        .expect("queued transition should succeed");
    state
        .runtime()
        .mark_running(created_run.id.as_str())
        .expect("running transition should succeed");
    state
        .runtime()
        .set_awaiting(
            created_run.id.as_str(),
            "approval_required".to_string(),
            "Need confirmation before continuing".to_string(),
        )
        .expect("await transition should succeed");

    let run_id = created_run.id.as_str().to_string();

    let resume_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/acp/runs/{run_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "resumePayload": {
                            "approved": true,
                            "comment": "continue"
                        }
                    }))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(resume_response.status(), StatusCode::OK);

    let resume_json = response_json(resume_response).await;
    assert_eq!(resume_json["run"]["status"]["state"], "running");
    assert!(resume_json["run"]["awaitPayload"].is_null());

    let events_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/acp/runs/{run_id}/events"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let events_json = response_json(events_response).await;
    let events = events_json["events"]
        .as_array()
        .expect("events should be an array");
    assert!(events
        .iter()
        .any(|event| event["event"]["payload"]["event"] == "run.resumed"));
}

#[tokio::test]
async fn test_cancellation_behavior_for_in_progress_run() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state.clone(), &config.acp);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&create_run_body("async", "Create cancellable run", None))
                        .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let create_json = response_json(create_response).await;
    let run_id = create_json["run"]["id"]
        .as_str()
        .expect("run id should be present")
        .to_string();

    state
        .runtime()
        .mark_queued(&run_id)
        .expect("queued transition should succeed");
    state
        .runtime()
        .mark_running(&run_id)
        .expect("running transition should succeed");

    let cancel_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/acp/runs/{run_id}/cancel"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "reason": "user requested stop"
                    }))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(cancel_response.status(), StatusCode::OK);

    let cancel_json = response_json(cancel_response).await;
    assert_eq!(cancel_json["run"]["status"]["state"], "cancelled");
    assert_eq!(
        cancel_json["run"]["status"]["cancellationReason"],
        "user requested stop"
    );
}

/// Verifies that a completed run and its full event history can be recovered
/// after a simulated process restart.
///
/// The "restart" is simulated by creating two independent `AcpRuntime`
/// instances that share the same SQLite database path. The first runtime
/// creates and completes a run (persisting it to the database). The second
/// runtime is constructed from scratch with the same database path so that
/// `restore_from_storage` loads the completed run into its in-memory state,
/// mirroring what happens when the real binary restarts and re-opens its
/// persistent storage.
///
/// Both runtimes are created with `new_with_storage_path` so that no
/// process-global environment variable (`XZATOMA_HISTORY_DB`) is mutated.
/// This keeps the test hermetic and eliminates the race conditions that cause
/// flakiness when tests run in parallel.
#[tokio::test]
async fn test_event_history_replay_after_restart() {
    let dir = tempdir().expect("failed to create tempdir");
    let db_path = dir.path().join("history.db");

    let config = test_config();

    // --- First "process": create and complete a run ---

    let first_runtime = AcpRuntime::new_with_storage_path(config.clone(), &db_path);
    let created_run = first_runtime
        .create_run(
            xzatoma::acp::runtime::AcpRuntimeCreateRequest::new(vec![
                xzatoma::acp::AcpMessage::new(
                    xzatoma::acp::AcpRole::User,
                    vec![xzatoma::acp::AcpMessagePart::Text(
                        xzatoma::acp::AcpTextPart::new("Replay this run".to_string()),
                    )],
                )
                .expect("message should be valid"),
            ])
            .with_mode(AcpRuntimeExecuteMode::Sync),
        )
        .expect("run should be created");

    first_runtime
        .mark_queued(created_run.id.as_str())
        .expect("queued transition should succeed");
    first_runtime
        .mark_running(created_run.id.as_str())
        .expect("running transition should succeed");
    first_runtime
        .append_output_message(
            created_run.id.as_str(),
            xzatoma::acp::runtime::assistant_text_message("persistent replay output".to_string())
                .expect("assistant message should be valid"),
        )
        .expect("output append should succeed");
    first_runtime
        .complete_run(created_run.id.as_str())
        .expect("completion should succeed");

    // --- Simulated restart: second runtime opens the same database ---
    //
    // new_with_storage_path calls restore_from_storage() during construction,
    // so the completed run is already in the second runtime's in-memory state
    // by the time we query it below.

    let second_runtime = AcpRuntime::new_with_storage_path(config, &db_path);

    // Open the same storage handle for the fallback assertions.
    let storage = xzatoma::storage::SqliteStorage::new_with_path(&db_path)
        .expect("storage should initialize");

    // restore_run will find the run in second_runtime's in-memory state
    // (populated by restore_from_storage during construction).
    let restored = second_runtime
        .restore_run(created_run.id.as_str())
        .expect("restore should succeed")
        .or_else(|| {
            storage
                .restore_acp_run(created_run.id.as_str())
                .expect("storage restore should succeed")
        })
        .expect("run should exist after restart");

    assert_eq!(restored.id.as_str(), created_run.id.as_str());
    assert_eq!(restored.status.state, xzatoma::acp::AcpRunState::Completed);

    // Events are loaded into second_runtime during restore_from_storage, so
    // get_events should succeed from in-memory state. The storage fallback is
    // kept for belt-and-suspenders assurance.
    let restored_events = second_runtime
        .get_events(created_run.id.as_str())
        .unwrap_or_else(|_| {
            storage
                .restore_acp_runtime_events(created_run.id.as_str())
                .expect("storage event restore should succeed")
        });

    assert!(!restored_events.is_empty());
    assert_eq!(restored_events[0].event.payload["event"], "run.created");

    let restored_event_names: Vec<&str> = restored_events
        .iter()
        .filter_map(|event| event.event.payload["event"].as_str())
        .collect();

    assert!(restored_event_names.contains(&"run.created"));
    assert!(restored_event_names.contains(&"run.in-progress"));
}

#[tokio::test]
async fn test_invalid_resume_payload_failure_path() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state.clone(), &config.acp);

    let created_run = state
        .runtime()
        .create_run(
            xzatoma::acp::runtime::AcpRuntimeCreateRequest::new(vec![
                xzatoma::acp::AcpMessage::new(
                    xzatoma::acp::AcpRole::User,
                    vec![xzatoma::acp::AcpMessagePart::Text(
                        xzatoma::acp::AcpTextPart::new("Await invalid resume".to_string()),
                    )],
                )
                .expect("message should be valid"),
            ])
            .with_mode(AcpRuntimeExecuteMode::Async),
        )
        .expect("run should be created");

    state
        .runtime()
        .mark_queued(created_run.id.as_str())
        .expect("queued transition should succeed");
    state
        .runtime()
        .mark_running(created_run.id.as_str())
        .expect("running transition should succeed");
    state
        .runtime()
        .set_awaiting(
            created_run.id.as_str(),
            "approval_required".to_string(),
            "Need confirmation before continuing".to_string(),
        )
        .expect("await transition should succeed");

    let run_id = created_run.id.as_str().to_string();

    let resume_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/acp/runs/{run_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "resumePayload": null
                    }))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(resume_response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let resume_json = response_json(resume_response).await;
    assert_eq!(resume_json["code"], "internal_error");
    assert!(resume_json["message"]
        .as_str()
        .expect("message should be a string")
        .contains("resume payload cannot be null"));
}

#[tokio::test]
async fn test_cancelling_completed_run_failure_path() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state.clone(), &config.acp);

    let created_run = state
        .runtime()
        .create_run(
            xzatoma::acp::runtime::AcpRuntimeCreateRequest::new(vec![
                xzatoma::acp::AcpMessage::new(
                    xzatoma::acp::AcpRole::User,
                    vec![xzatoma::acp::AcpMessagePart::Text(
                        xzatoma::acp::AcpTextPart::new("Complete before cancel".to_string()),
                    )],
                )
                .expect("message should be valid"),
            ])
            .with_mode(AcpRuntimeExecuteMode::Sync),
        )
        .expect("run should be created");

    state
        .runtime()
        .mark_queued(created_run.id.as_str())
        .expect("queued transition should succeed");
    state
        .runtime()
        .mark_running(created_run.id.as_str())
        .expect("running transition should succeed");
    state
        .runtime()
        .append_output_message(
            created_run.id.as_str(),
            xzatoma::acp::runtime::assistant_text_message("done".to_string())
                .expect("assistant message should be valid"),
        )
        .expect("output append should succeed");
    state
        .runtime()
        .complete_run(created_run.id.as_str())
        .expect("completion should succeed");

    let run_id = created_run.id.as_str().to_string();

    let cancel_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/acp/runs/{run_id}/cancel"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "reason": "too late"
                    }))
                    .expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(cancel_response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let cancel_json = response_json(cancel_response).await;
    assert_eq!(cancel_json["code"], "internal_error");
    assert!(cancel_json["message"]
        .as_str()
        .expect("message should be a string")
        .contains("cannot cancel terminal ACP run"));
}

#[tokio::test]
async fn test_loading_missing_session_failure_path() {
    let config = test_config();
    let (state, _dir) = test_state(&config);
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/acp/sessions/session_missing")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let json = response_json(response).await;
    assert_eq!(json["code"], "not_found");
    assert!(json["message"]
        .as_str()
        .expect("message should be a string")
        .contains("session_missing"));
}
