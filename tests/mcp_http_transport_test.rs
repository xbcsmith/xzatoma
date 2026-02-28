//! MCP HTTP transport integration tests (Phase 2)
//!
//! Tests the `HttpTransport` implementation against a `wiremock` mock server.
//! Each test verifies a specific aspect of the `2025-11-25` Streamable HTTP
//! transport specification.
//!
//! # wiremock body helpers
//!
//! Use `set_body_raw(bytes, mime)` for SSE responses so that the
//! `Content-Type` is set to `text/event-stream` exactly.  `set_body_string`
//! forces `text/plain` and would cause the transport to fall through to the
//! JSON branch.  `set_body_json` forces `application/json`.

use std::collections::HashMap;
use std::time::Duration;

use wiremock::matchers::{header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzatoma::mcp::transport::http::HttpTransport;
use xzatoma::mcp::transport::Transport;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Construct an `HttpTransport` pointing at the given wiremock base URL.
fn make_transport(base_url: &str) -> HttpTransport {
    HttpTransport::new(
        url::Url::parse(base_url).expect("valid url"),
        HashMap::new(),
        Duration::from_secs(5),
    )
}

/// Collect all currently buffered messages from `receive()` with a short
/// deadline.  Stops as soon as one `timeout` fires or the stream ends.
async fn drain_receive(transport: &HttpTransport, deadline: Duration) -> Vec<String> {
    use futures::StreamExt;

    let mut messages = Vec::new();
    let mut stream = transport.receive();

    while let Ok(Some(msg)) = tokio::time::timeout(deadline, stream.next()).await {
        messages.push(msg);
    }

    messages
}

// ---------------------------------------------------------------------------
// Task 2.7 required tests
// ---------------------------------------------------------------------------

/// POST with `application/json` response is forwarded to `receive()`.
///
/// Verifies that when the server returns `Content-Type: application/json`,
/// the response body is pushed to the inbound channel and retrievable via
/// `receive()`.
#[tokio::test]
async fn test_post_with_json_response_forwarded_to_receive() {
    let server = MockServer::start().await;

    let body = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.as_bytes().to_vec(), "application/json"),
        )
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#.to_string())
        .await
        .expect("send should succeed");

    // Give the transport a moment to receive the response.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let messages = drain_receive(&transport, Duration::from_millis(200)).await;
    assert_eq!(messages.len(), 1, "expected exactly one message");
    assert_eq!(messages[0], body);
}

/// POST with `text/event-stream` response forwards both SSE events to
/// `receive()`.
///
/// Verifies that an SSE stream with two `data:` events delivers both messages
/// to the inbound channel.
#[tokio::test]
async fn test_post_with_sse_two_events_both_forwarded() {
    let server = MockServer::start().await;

    // Build the SSE body with real newlines.
    let sse_body = concat!(
        "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"first\":true}}\n",
        "\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"second\":true}}\n",
        "\n",
    );

    Mock::given(method("POST"))
        .respond_with(
            // set_body_raw preserves our Content-Type; set_body_string would
            // override it with text/plain.
            ResponseTemplate::new(200)
                .set_body_raw(sse_body.as_bytes().to_vec(), "text/event-stream"),
        )
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string())
        .await
        .expect("send should succeed");

    // Allow the SSE parsing task to process the stream.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let messages = drain_receive(&transport, Duration::from_millis(200)).await;
    assert_eq!(
        messages.len(),
        2,
        "expected two SSE events; got: {messages:?}"
    );

    let v1: serde_json::Value = serde_json::from_str(&messages[0]).expect("valid JSON");
    let v2: serde_json::Value = serde_json::from_str(&messages[1]).expect("valid JSON");
    assert_eq!(v1["result"]["first"], true);
    assert_eq!(v2["result"]["second"], true);
}

/// POST returning `202 Accepted` pushes nothing to `receive()`.
///
/// Verifies that a `202` notification acknowledgement is a no-op: no message
/// is delivered to the inbound channel.
#[tokio::test]
async fn test_post_202_yields_nothing() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(202))
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    transport
        .send(r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#.to_string())
        .await
        .expect("send should succeed");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let messages = drain_receive(&transport, Duration::from_millis(100)).await;
    assert!(
        messages.is_empty(),
        "202 must not yield any message; got: {messages:?}"
    );
}

/// Every POST carries `MCP-Protocol-Version: 2025-11-25`.
///
/// Verifies that the mandatory `MCP-Protocol-Version` header is present on
/// every outbound POST request, regardless of whether a session is active.
#[tokio::test]
async fn test_mcp_protocol_version_header_present_on_every_post() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(header("MCP-Protocol-Version", "2025-11-25"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.as_bytes().to_vec(),
            "application/json",
        ))
        .expect(2) // Two POSTs must each carry the header.
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());

    // First POST.
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string())
        .await
        .expect("first send should succeed");

    tokio::time::sleep(Duration::from_millis(30)).await;

    // Second POST.
    transport
        .send(r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":{}}"#.to_string())
        .await
        .expect("second send should succeed");

    tokio::time::sleep(Duration::from_millis(30)).await;

    // wiremock verifies that both requests matched the `MCP-Protocol-Version`
    // matcher (the `expect(2)` assertion fires on MockServer::verify/drop).
    server.verify().await;
}

/// Session ID is captured from the first response and sent on subsequent
/// requests.
///
/// Verifies the full session round-trip: the server returns
/// `MCP-Session-Id: test-session-1` on the first response; the second
/// request must carry `MCP-Session-Id: test-session-1`.
#[tokio::test]
async fn test_session_id_captured_and_sent_on_subsequent_requests() {
    let server = MockServer::start().await;

    // First request: server returns a session ID.
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains("initialize"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("MCP-Session-Id", "test-session-1")
                .set_body_raw(
                    r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1.0"}}}"#
                        .as_bytes()
                        .to_vec(),
                    "application/json",
                ),
        )
        .mount(&server)
        .await;

    // Second request: must carry the session ID.
    Mock::given(method("POST"))
        .and(header("MCP-Session-Id", "test-session-1"))
        .and(wiremock::matchers::body_string_contains("tools/list"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#.as_bytes().to_vec(),
            "application/json",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());

    // First POST (initialize) -- server sets session ID.
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string())
        .await
        .expect("initialize send should succeed");

    // Allow session ID to be written.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second POST (tools/list) -- must include MCP-Session-Id.
    transport
        .send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#.to_string())
        .await
        .expect("tools/list send should succeed");

    tokio::time::sleep(Duration::from_millis(50)).await;

    server.verify().await;
}

/// A `404` response while a session is active returns
/// `XzatomaError::Mcp("mcp session expired")`.
///
/// Verifies that after a session is established, a `404` response causes the
/// session to be cleared and the correct error variant is returned.
#[tokio::test]
async fn test_404_with_session_id_emits_mcp_error() {
    let server = MockServer::start().await;

    // First request: establish a session.
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_string_contains("initialize"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("MCP-Session-Id", "session-abc")
                .set_body_raw(
                    r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"t","version":"1"}}}"#
                        .as_bytes()
                        .to_vec(),
                    "application/json",
                ),
        )
        .mount(&server)
        .await;

    // Second request: server returns 404, simulating session expiry.
    Mock::given(method("POST"))
        .and(header("MCP-Session-Id", "session-abc"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());

    // Establish session.
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string())
        .await
        .expect("initialize should succeed");

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Issue request that will hit 404.
    let result = transport
        .send(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#.to_string())
        .await;

    assert!(result.is_err(), "expected error on 404 with active session");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("mcp session expired") || err_str.contains("MCP error"),
        "unexpected error string: {err_str}"
    );
}

/// SSE stream with `event: ping` followed by a real `data:` event delivers
/// only the real event.
///
/// Verifies that ping events are silently discarded per the MCP specification
/// and do not appear in `receive()`.
#[tokio::test]
async fn test_ping_sse_events_are_silently_dropped() {
    let server = MockServer::start().await;

    let sse_body = concat!(
        "event: ping\n",
        "data: ignored-ping-payload\n",
        "\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"real\":true}}\n",
        "\n",
    );

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_body.as_bytes().to_vec(), "text/event-stream"),
        )
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#.to_string())
        .await
        .expect("send should succeed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let messages = drain_receive(&transport, Duration::from_millis(200)).await;
    assert_eq!(
        messages.len(),
        1,
        "only the real event should be received; got: {messages:?}"
    );

    let v: serde_json::Value = serde_json::from_str(&messages[0]).expect("valid JSON");
    assert_eq!(v["result"]["real"], true);
}

/// A `401 Unauthorized` response returns `XzatomaError::McpAuth`.
///
/// Verifies that when the server returns `401`, the `WWW-Authenticate` header
/// value is surfaced as `XzatomaError::McpAuth`.
#[tokio::test]
async fn test_401_returns_mcp_auth_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"mcp\""),
        )
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    let result = transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#.to_string())
        .await;

    assert!(result.is_err(), "expected error on 401");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("MCP auth") || err_str.contains("Bearer"),
        "unexpected error string: {err_str}"
    );
}

/// `data: [PING]` SSE events are silently discarded.
///
/// Verifies that a `data: [PING]` event (case-insensitive) is treated
/// identically to `event: ping` and does not appear in `receive()`.
#[tokio::test]
async fn test_data_ping_sse_events_are_silently_dropped() {
    let server = MockServer::start().await;

    let sse_body = concat!(
        "data: [PING]\n",
        "\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"pong\"}\n",
        "\n",
    );

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(sse_body.as_bytes().to_vec(), "text/event-stream"),
        )
        .mount(&server)
        .await;

    let transport = make_transport(&server.uri());
    transport
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#.to_string())
        .await
        .expect("send should succeed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let messages = drain_receive(&transport, Duration::from_millis(200)).await;
    assert_eq!(
        messages.len(),
        1,
        "only the real event should be received; got: {messages:?}"
    );
    let v: serde_json::Value = serde_json::from_str(&messages[0]).expect("valid JSON");
    assert_eq!(v["result"], "pong");
}
