use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};

use serde_json::{json, Value};
use tower::ServiceExt;
use xzatoma::acp::executor::AcpExecutor;
use xzatoma::acp::runtime::AcpRuntime;
use xzatoma::acp::server::{build_router, AcpServerState};
use xzatoma::config::Config;

fn test_config() -> Config {
    let mut config = Config::default();
    config.provider.provider_type = "ollama".to_string();
    config
}

fn test_state(config: &Config) -> AcpServerState {
    let runtime = AcpRuntime::new_in_memory(config.clone());
    let executor = AcpExecutor::new_mock_success(
        config.clone(),
        runtime.clone(),
        "mock ACP lifecycle test response".to_string(),
    );

    AcpServerState::from_parts(config, runtime, executor).expect("state should build")
}

fn test_create_run_body(mode: &str, prompt: &str) -> Value {
    json!({
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
    })
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    serde_json::from_slice(&body).expect("response body should be valid JSON")
}

#[tokio::test]
async fn test_create_run_sync_success_returns_completed_run() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body(
                        "sync",
                        "Reply with a short greeting",
                    ))
                    .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let json = response_json(response).await;
    assert_eq!(json["mode"], "sync");
    assert_eq!(json["run"]["status"]["state"], "completed");

    let output_messages = json["run"]["output"]["messages"]
        .as_array()
        .expect("output messages should be an array");
    assert_eq!(output_messages.len(), 1);
    assert_eq!(output_messages[0]["role"], "assistant");
}

#[tokio::test]
async fn test_create_run_async_returns_accepted_and_status_polling_works() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body("async", "Count to three"))
                        .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(create_response.status(), StatusCode::ACCEPTED);

    let create_json = response_json(create_response).await;
    assert_eq!(create_json["mode"], "async");

    let run_id = create_json["run"]["id"]
        .as_str()
        .expect("run id should be a string")
        .to_string();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let poll_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/acp/runs/{run_id}"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(poll_response.status(), StatusCode::OK);

    let poll_json = response_json(poll_response).await;
    assert_eq!(poll_json["runId"], run_id);
    assert!(matches!(
        poll_json["state"].as_str(),
        Some("queued") | Some("running") | Some("completed") | Some("failed")
    ));
}

#[tokio::test]
async fn test_create_run_stream_returns_sse_response() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body("stream", "Stream a short answer"))
                        .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content type should be present");
    assert!(content_type.starts_with("text/event-stream"));

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("stream body should read");
    let text = String::from_utf8(body.to_vec()).expect("stream body should be UTF-8");

    assert!(text.contains("event: run.created"));
    assert!(text.contains("event: run.in-progress"));
    assert!(text.contains("event: message.created"));
    assert!(text.contains("event: message.part"));
    assert!(text.contains("event: message.completed"));
    assert!(text.contains("event: run.completed"));
}

#[tokio::test]
async fn test_get_run_events_returns_ordered_replayable_history() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body("sync", "Generate one sentence"))
                        .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let create_json = response_json(create_response).await;
    let run_id = create_json["run"]["id"]
        .as_str()
        .expect("run id should be a string");

    let events_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/acp/runs/{run_id}/events"))
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(events_response.status(), StatusCode::OK);

    let events_json = response_json(events_response).await;
    let events = events_json["events"]
        .as_array()
        .expect("events should be an array");

    assert!(events.len() >= 5);

    let sequences: Vec<u64> = events
        .iter()
        .map(|event| {
            event["sequence"]
                .as_u64()
                .expect("sequence should be an unsigned integer")
        })
        .collect();

    let mut sorted = sequences.clone();
    sorted.sort_unstable();
    assert_eq!(sequences, sorted);

    assert_eq!(events[0]["event"]["payload"]["event"], "run.created");
    assert_eq!(
        events.last().expect("events should not be empty")["event"]["payload"]["event"],
        "run.completed"
    );
}

#[tokio::test]
async fn test_invalid_input_handling_rejects_unsupported_artifact_input() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "mode": "sync",
                        "agentName": "xzatoma",
                        "input": [
                            {
                                "role": "user",
                                "parts": [
                                    {
                                        "type": "artifact",
                                        "data": {
                                            "name": "image.png",
                                            "mimeType": "image/png",
                                            "contentUrl": "https://example.com/image.png"
                                        }
                                    }
                                ]
                            }
                        ]
                    }))
                    .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = response_json(response).await;
    assert_eq!(json["code"], "invalid_request");
    assert!(json["message"]
        .as_str()
        .expect("message should be a string")
        .contains("unsupported"));
}

#[tokio::test]
async fn test_not_found_run_lookup_returns_not_found() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/runs/run_missing")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(run_response.status(), StatusCode::NOT_FOUND);

    let run_json = response_json(run_response).await;
    assert_eq!(run_json["code"], "not_found");

    let events_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/runs/run_missing/events")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(events_response.status(), StatusCode::NOT_FOUND);

    let events_json = response_json(events_response).await;
    assert_eq!(events_json["code"], "not_found");
}

#[tokio::test]
async fn test_large_output_event_accumulation_behavior() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let large_prompt = format!("Echo this payload: {}", "x".repeat(16 * 1024));

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body("sync", &large_prompt))
                        .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(create_response.status(), StatusCode::OK);

    let create_json = response_json(create_response).await;
    let run_id = create_json["run"]["id"]
        .as_str()
        .expect("run id should be a string");

    let events_response = app
        .oneshot(
            Request::builder()
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

    assert!(events.iter().any(|event| {
        event["event"]["payload"]["event"] == "message.part"
            && event["event"]["payload"]["text"].is_string()
    }));
}

#[tokio::test]
async fn test_streaming_event_ordering_regression() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body(
                        "stream",
                        "Return one structured line",
                    ))
                    .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("stream body should read");
    let text = String::from_utf8(body.to_vec()).expect("stream body should be UTF-8");

    let positions = [
        text.find("event: run.created")
            .expect("run.created should exist"),
        text.find("event: run.in-progress")
            .expect("run.in-progress should exist"),
        text.find("event: message.created")
            .expect("message.created should exist"),
        text.find("event: message.part")
            .expect("message.part should exist"),
        text.find("event: message.completed")
            .expect("message.completed should exist"),
        text.find("event: run.completed")
            .expect("run.completed should exist"),
    ];

    for pair in positions.windows(2) {
        assert!(pair[0] < pair[1]);
    }
}

#[tokio::test]
async fn test_duplicate_completion_prevention_regression() {
    let config = test_config();
    let state = test_state(&config);
    let app = build_router(state, &config.acp);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/acp/runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&test_create_run_body("sync", "Complete exactly once"))
                        .expect("request JSON should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    let create_json = response_json(create_response).await;
    let run_id = create_json["run"]["id"]
        .as_str()
        .expect("run id should be a string");

    let events_response = app
        .oneshot(
            Request::builder()
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

    let completion_events = events
        .iter()
        .filter(|event| event["event"]["payload"]["event"] == "run.completed")
        .count();

    assert_eq!(completion_events, 1);
}
