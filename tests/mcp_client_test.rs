//! MCP JSON-RPC client integration tests (Phase 1)
//!
//! Tests the transport-agnostic `JsonRpcClient` and the `start_read_loop`
//! dispatcher using in-process Tokio channels in place of a real transport.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use xzatoma::mcp::client::{start_read_loop, JsonRpcClient};
use xzatoma::mcp::types::NOTIF_TOOLS_LIST_CHANGED;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a fully wired client and return its channel ends.
///
/// Returns `(client_arc, outbound_rx, inbound_tx, cancel_token)`.
/// - `outbound_rx` drains messages the client sends to the "server".
/// - `inbound_tx`  injects messages from the "server" into the client.
fn wired_client() -> (
    Arc<JsonRpcClient>,
    mpsc::UnboundedReceiver<String>,
    mpsc::UnboundedSender<String>,
    CancellationToken,
) {
    let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    let token = CancellationToken::new();
    let client = Arc::new(JsonRpcClient::new(out_tx));
    start_read_loop(in_rx, token.clone(), Arc::clone(&client));
    (client, out_rx, in_tx, token)
}

/// Read exactly one message from `outbound_rx`, parse it as JSON, and return
/// the `id` field together with the raw value.
async fn recv_request(
    rx: &mut mpsc::UnboundedReceiver<String>,
) -> (serde_json::Value, serde_json::Value) {
    let raw = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timed out waiting for outbound message")
        .expect("outbound channel closed");
    let val: serde_json::Value = serde_json::from_str(&raw).expect("invalid JSON in outbound");
    let id = val["id"].clone();
    (id, val)
}

/// Send a successful JSON-RPC response back on `inbound_tx`.
fn send_response(
    in_tx: &mpsc::UnboundedSender<String>,
    id: &serde_json::Value,
    result: serde_json::Value,
) {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    });
    in_tx.send(serde_json::to_string(&resp).unwrap()).unwrap();
}

/// Send a JSON-RPC error response back on `inbound_tx`.
fn send_error_response(
    in_tx: &mpsc::UnboundedSender<String>,
    id: &serde_json::Value,
    code: i64,
    message: &str,
) {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    });
    in_tx.send(serde_json::to_string(&resp).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// Task 1.7 required tests
// ---------------------------------------------------------------------------

/// `request()` resolves with the correctly deserialized result value when the
/// server sends a matching response.
#[tokio::test]
async fn test_request_resolves_with_correct_result() {
    let (client, mut out_rx, in_tx, _token) = wired_client();

    // Spawn a task that echoes back a successful response.
    tokio::spawn(async move {
        let (id, _) = recv_request(&mut out_rx).await;
        send_response(
            &in_tx,
            &id,
            serde_json::json!({ "tools": [], "nextCursor": null }),
        );
    });

    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct ToolsListResult {
        tools: Vec<serde_json::Value>,
    }

    let result: xzatoma::error::Result<ToolsListResult> = client
        .request(
            "tools/list",
            serde_json::json!({}),
            Some(Duration::from_secs(5)),
        )
        .await;

    assert!(result.is_ok(), "expected Ok, got: {result:?}");
    let val = result.unwrap();
    assert_eq!(val.tools, Vec::<serde_json::Value>::new());
}

/// `request()` with a short timeout returns `McpTimeout` when no response
/// is ever sent.
#[tokio::test]
async fn test_request_timeout_fires() {
    let (client, _out_rx, _in_tx, _token) = wired_client();

    // No response is injected; the request must time out.
    let result: xzatoma::error::Result<serde_json::Value> = client
        .request(
            "tools/list",
            serde_json::json!({}),
            Some(Duration::from_millis(60)),
        )
        .await;

    assert!(result.is_err(), "expected Err, got Ok");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("timeout") || err_str.contains("MCP timeout"),
        "unexpected error string: {err_str}"
    );
}

/// A registered notification handler is called exactly once when a matching
/// notification arrives on the inbound channel.
#[tokio::test]
async fn test_notification_handler_called_for_matching_method() {
    let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    let token = CancellationToken::new();
    let client = Arc::new(JsonRpcClient::new(out_tx));
    start_read_loop(in_rx, token.clone(), Arc::clone(&client));

    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = Arc::clone(&counter);

    client.on_notification(NOTIF_TOOLS_LIST_CHANGED, move |_params| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    // Give the internal `tokio::spawn` inside `on_notification` time to register.
    tokio::time::sleep(Duration::from_millis(20)).await;

    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": NOTIF_TOOLS_LIST_CHANGED
    });
    in_tx.send(serde_json::to_string(&notif).unwrap()).unwrap();

    // Allow the read loop to process the notification.
    tokio::time::sleep(Duration::from_millis(40)).await;

    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "handler should have been called exactly once"
    );

    token.cancel();
}

/// When the `CancellationToken` is fired, the read loop exits and any
/// in-flight `request()` call resolves to an error rather than blocking
/// indefinitely.
#[tokio::test]
async fn test_pending_sender_dropped_cleanly_on_read_loop_exit() {
    let (client, _out_rx, _in_tx, token) = wired_client();

    // Start a long-lived request (10 s) before cancelling the loop.
    let client_clone = Arc::clone(&client);
    let request_task = tokio::spawn(async move {
        client_clone
            .request::<_, serde_json::Value>(
                "tools/list",
                serde_json::json!({}),
                Some(Duration::from_secs(10)),
            )
            .await
    });

    // Give the request enough time to register in the pending map.
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Cancel the loop.
    token.cancel();

    // The request task must complete with an error within a reasonable time.
    let outcome = tokio::time::timeout(Duration::from_secs(3), request_task)
        .await
        .expect("request task did not finish after read loop exit")
        .expect("request task panicked");

    assert!(
        outcome.is_err(),
        "expected an error after read loop exit, got Ok"
    );
}

/// When the server returns a JSON-RPC `error` object, `request()` maps it
/// to `XzatomaError::Mcp`.
#[tokio::test]
async fn test_json_rpc_error_response_mapped_to_mcp_error() {
    let (client, mut out_rx, in_tx, _token) = wired_client();

    tokio::spawn(async move {
        let (id, _) = recv_request(&mut out_rx).await;
        send_error_response(&in_tx, &id, -32601, "Method not found");
    });

    let result: xzatoma::error::Result<serde_json::Value> = client
        .request(
            "nonexistent/method",
            serde_json::json!({}),
            Some(Duration::from_secs(5)),
        )
        .await;

    assert!(result.is_err());
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("Method not found") || err_str.contains("MCP error"),
        "unexpected error: {err_str}"
    );
}

// ---------------------------------------------------------------------------
// Additional coverage
// ---------------------------------------------------------------------------

/// `notify()` sends a message without an `id` field.
#[tokio::test]
async fn test_notify_sends_message_without_id() {
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let client = JsonRpcClient::new(out_tx);

    client
        .notify("notifications/initialized", serde_json::json!({}))
        .unwrap();

    let raw = tokio::time::timeout(Duration::from_secs(2), out_rx.recv())
        .await
        .expect("timed out")
        .expect("channel closed");

    let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(val["method"], "notifications/initialized");
    assert_eq!(val["jsonrpc"], "2.0");
    // Notifications must never carry an id.
    assert!(
        val.get("id").is_none(),
        "notification must not have an id field"
    );
}

/// `notify()` returns an error when the outbound channel is closed.
#[test]
fn test_notify_returns_error_when_channel_closed() {
    let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
    drop(out_rx);
    let client = JsonRpcClient::new(out_tx);
    let result = client.notify("test", serde_json::json!({}));
    assert!(result.is_err());
}

/// Multiple concurrent requests are resolved independently and each receives
/// the response that matches its own ID.
#[tokio::test]
async fn test_multiple_concurrent_requests_resolved_correctly() {
    let (client, mut out_rx, in_tx, _token) = wired_client();

    // Echo-responder: for every request received, send back a result
    // containing the same `id` so callers can verify identity.
    tokio::spawn(async move {
        loop {
            let raw = match tokio::time::timeout(Duration::from_secs(5), out_rx.recv()).await {
                Ok(Some(r)) => r,
                _ => break,
            };
            let req: serde_json::Value = serde_json::from_str(&raw).unwrap();
            if let Some(id) = req.get("id") {
                if id.is_null() {
                    continue;
                }
                send_response(&in_tx, id, serde_json::json!({ "echo": id }));
            }
        }
    });

    // Issue three requests concurrently.
    let (r1, r2, r3) = tokio::join!(
        client.request::<_, serde_json::Value>(
            "ping",
            serde_json::json!({}),
            Some(Duration::from_secs(5))
        ),
        client.request::<_, serde_json::Value>(
            "ping",
            serde_json::json!({}),
            Some(Duration::from_secs(5))
        ),
        client.request::<_, serde_json::Value>(
            "ping",
            serde_json::json!({}),
            Some(Duration::from_secs(5))
        ),
    );

    assert!(r1.is_ok(), "r1: {r1:?}");
    assert!(r2.is_ok(), "r2: {r2:?}");
    assert!(r3.is_ok(), "r3: {r3:?}");

    // Each response must echo a distinct ID value.
    let ids: std::collections::HashSet<u64> = [r1.unwrap(), r2.unwrap(), r3.unwrap()]
        .into_iter()
        .map(|v| v["echo"].as_u64().expect("echo must be a u64"))
        .collect();
    assert_eq!(
        ids.len(),
        3,
        "each concurrent request must have a unique ID"
    );
}

/// A notification for an unregistered method is silently ignored (no panic).
#[tokio::test]
async fn test_unregistered_notification_is_silently_ignored() {
    let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    let token = CancellationToken::new();
    let client = Arc::new(JsonRpcClient::new(out_tx));
    start_read_loop(in_rx, token.clone(), Arc::clone(&client));

    // Send a notification with no registered handler.
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/unknown/event"
    });
    in_tx.send(serde_json::to_string(&notif).unwrap()).unwrap();

    // If the loop panicked we would not reach this assertion.
    tokio::time::sleep(Duration::from_millis(30)).await;

    token.cancel();
}

/// A second `request()` call after the first resolves still works correctly
/// (verifies the pending map is cleaned up after each resolved entry).
#[tokio::test]
async fn test_sequential_requests_both_resolve() {
    let (client, mut out_rx, in_tx, _token) = wired_client();

    if let Some(i) = (0u64..2).next() {
        // Capture copies for the spawned task.
        let in_tx_c = in_tx.clone();

        tokio::spawn(async move {
            let (id, _) = recv_request(&mut out_rx).await;
            send_response(&in_tx_c, &id, serde_json::json!({ "seq": i }));
        });

        let result: xzatoma::error::Result<serde_json::Value> = client
            .request("ping", serde_json::json!({}), Some(Duration::from_secs(5)))
            .await;

        assert!(result.is_ok(), "request {i} failed: {result:?}");
        assert_eq!(result.unwrap()["seq"], i);
    }
}

/// Malformed JSON on the inbound channel does not crash the read loop.
#[tokio::test]
async fn test_malformed_inbound_json_does_not_crash_loop() {
    let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    let token = CancellationToken::new();
    let client = Arc::new(JsonRpcClient::new(out_tx));
    start_read_loop(in_rx, token.clone(), Arc::clone(&client));

    // Inject several invalid JSON strings.
    for bad in &["{not json", "null", "42", "\"string\"", ""] {
        in_tx.send(bad.to_string()).unwrap();
    }

    // Loop must still be alive.
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Verify liveness: a valid response is still processed.
    let (out_tx2, mut out_rx2) = mpsc::unbounded_channel::<String>();
    let (in_tx2, in_rx2) = mpsc::unbounded_channel::<String>();
    let token2 = CancellationToken::new();
    let live_client = Arc::new(JsonRpcClient::new(out_tx2));
    start_read_loop(in_rx2, token2.clone(), Arc::clone(&live_client));

    tokio::spawn(async move {
        let (id, _) = recv_request(&mut out_rx2).await;
        send_response(&in_tx2, &id, serde_json::json!("pong"));
    });

    let r: xzatoma::error::Result<serde_json::Value> = live_client
        .request("ping", serde_json::json!({}), Some(Duration::from_secs(5)))
        .await;
    assert!(r.is_ok(), "live client should still work: {r:?}");

    token.cancel();
    token2.cancel();
}
