//! ACP discovery integration tests.
//!
//! These tests verify the Phase 2 ACP HTTP discovery surface by booting the
//! Axum router in-process and exercising the first read-only endpoints:
//!
//! - `GET /ping`
//! - `GET /agents`
//! - `GET /agents/{name}`

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use xzatoma::acp::server::{build_router, AcpServerState};
use xzatoma::config::{AcpCompatibilityMode, Config};

#[tokio::test]
async fn test_ping_success_in_versioned_mode() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/ping")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "xzatoma-acp");
    assert!(json["timestamp"].as_str().is_some());
}

#[tokio::test]
async fn test_agents_list_shape_success() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/agents")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    let agents = json["agents"]
        .as_array()
        .expect("agents should be an array");
    assert_eq!(json["total"], 1);
    assert_eq!(json["offset"], 0);
    assert_eq!(json["limit"], 1);
    assert_eq!(agents.len(), 1);

    let agent = &agents[0];
    assert_eq!(agent["name"], "xzatoma");
    assert_eq!(agent["displayName"], "XZatoma ACP Agent");
    assert!(agent["description"].as_str().is_some());
    assert!(agent["capabilities"].is_array());
    assert!(agent["metadata"].is_object());
    assert!(agent["links"].is_array());
}

#[tokio::test]
async fn test_agents_list_supports_offset_and_limit() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/agents?offset=0&limit=1")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    let agents = json["agents"]
        .as_array()
        .expect("agents should be an array");
    assert_eq!(json["offset"], 0);
    assert_eq!(json["limit"], 1);
    assert_eq!(agents.len(), 1);
}

#[tokio::test]
async fn test_agent_by_name_success() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/agents/xzatoma")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    assert_eq!(json["name"], "xzatoma");
    assert_eq!(json["displayName"], "XZatoma ACP Agent");
    assert!(json["description"].as_str().is_some());
    assert!(json["capabilities"].is_array());

    let metadata = json["metadata"]
        .as_object()
        .expect("metadata should be an object");
    assert_eq!(
        metadata
            .get("implementation")
            .and_then(Value::as_str)
            .expect("implementation metadata should exist"),
        "xzatoma_primary_agent"
    );
    assert_eq!(
        metadata
            .get("language")
            .and_then(Value::as_str)
            .expect("language metadata should exist"),
        "rust"
    );
    assert_eq!(
        metadata
            .get("framework")
            .and_then(Value::as_str)
            .expect("framework metadata should exist"),
        "axum"
    );
    assert!(metadata
        .get("supported_input_content_types")
        .and_then(Value::as_str)
        .is_some());
    assert!(metadata
        .get("supported_output_content_types")
        .and_then(Value::as_str)
        .is_some());
    assert!(metadata
        .get("generated_at")
        .and_then(Value::as_str)
        .is_some());

    let links = json["links"].as_array().expect("links should be an array");
    assert!(!links.is_empty());
}

#[tokio::test]
async fn test_agent_by_name_not_found() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/agents/missing-agent")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    assert_eq!(json["code"], "not_found");
    assert!(json["message"]
        .as_str()
        .expect("message should be a string")
        .contains("missing-agent"));
}

#[tokio::test]
async fn test_manifest_json_schema_shape() {
    let config = Config::default();
    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/acp/agents/xzatoma")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let json: Value = serde_json::from_slice(&body).expect("body should be valid JSON");

    assert!(json.get("name").is_some());
    assert!(json.get("version").is_some());
    assert!(json.get("displayName").is_some());
    assert!(json.get("description").is_some());
    assert!(json.get("capabilities").is_some());
    assert!(json.get("metadata").is_some());
    assert!(json.get("links").is_some());

    assert!(json["name"].is_string());
    assert!(json["version"].is_string());
    assert!(json["displayName"].is_string());
    assert!(json["capabilities"].is_array());
    assert!(json["metadata"].is_object());
    assert!(json["links"].is_array());
}

#[tokio::test]
async fn test_root_compatible_mode_exposes_direct_acp_paths() {
    let mut config = Config::default();
    config.acp.compatibility_mode = AcpCompatibilityMode::RootCompatible;

    let state = AcpServerState::from_config(&config).expect("state should build");
    let app = build_router(state, &config.acp);

    let ping_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ping")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(ping_response.status(), StatusCode::OK);

    let agents_response = app
        .oneshot(
            Request::builder()
                .uri("/agents")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should succeed");
    assert_eq!(agents_response.status(), StatusCode::OK);
}
