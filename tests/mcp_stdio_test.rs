//! MCP stdio transport integration tests (Phase 2)
//!
//! Tests the `StdioTransport` implementation against the `mcp_test_server`
//! subprocess. These tests exercise the full stdio transport pipeline:
//! spawning the subprocess, performing the `initialize` handshake, listing
//! tools, and calling the `echo` tool.
//!
//! The `mcp_test_server` binary must be built before running these tests.
//! The test harness locates it via the `CARGO_BIN_EXE_mcp_test_server`
//! environment variable that Cargo injects automatically when running
//! integration tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use xzatoma::mcp::client::{start_read_loop, JsonRpcClient};
use xzatoma::mcp::protocol::{McpProtocol, ServerCapabilityFlag};
use xzatoma::mcp::transport::stdio::StdioTransport;
use xzatoma::mcp::transport::Transport;
use xzatoma::mcp::types::{ClientCapabilities, Implementation};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to the `mcp_test_server` binary.
///
/// Cargo sets `CARGO_BIN_EXE_mcp_test_server` automatically when running
/// integration tests in the same workspace. Falls back to searching in the
/// `target/debug` directory for convenience during manual testing.
fn test_server_exe() -> PathBuf {
    // Preferred: use the Cargo-injected env var.
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_mcp_test_server") {
        return PathBuf::from(p);
    }

    // Fallback: derive from CARGO_MANIFEST_DIR.
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let debug = PathBuf::from(manifest)
            .join("target")
            .join("debug")
            .join("mcp_test_server");
        if debug.exists() {
            return debug;
        }
    }

    // Last resort: assume it is on PATH (useful for CI).
    PathBuf::from("mcp_test_server")
}

/// Spawn the test server and wire up a fully initialised `JsonRpcClient`
/// backed by the `StdioTransport`.
///
/// Returns `(session, cancel_token)`. The `CancellationToken` should be
/// cancelled after the test completes to clean up the read loop.
async fn spawn_and_initialize() -> (
    xzatoma::mcp::protocol::InitializedMcpProtocol,
    CancellationToken,
) {
    let exe = test_server_exe();

    let transport = StdioTransport::spawn(exe, vec![], HashMap::new(), None)
        .expect("failed to spawn mcp_test_server -- was it built with `cargo build`?");

    // Wire up the JsonRpcClient so that McpProtocol and the read loop share
    // the same pending map, notification handlers, and server-request handlers.
    //
    // The pattern mirrors `wired_protocol()` in src/mcp/protocol.rs tests:
    //
    //   1. Build a shared `Arc<JsonRpcClient>` and pass it to `start_read_loop`.
    //   2. Construct a second `JsonRpcClient` whose `Arc` fields alias those of
    //      the shared client, but whose `outbound_tx` is a clone of the shared
    //      sender.  This second client is given to `McpProtocol::new`.
    //
    // Both clients therefore share the same `pending` map: when the read loop
    // resolves a response via the shared Arc, the `proto_client`'s `request()`
    // awaiting on the same oneshot sender in the same map is unblocked.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    let token = CancellationToken::new();

    // `shared` is the Arc given to the read loop.
    let shared = Arc::new(JsonRpcClient::new(out_tx));
    start_read_loop(in_rx, token.clone(), Arc::clone(&shared));

    // `proto_client` aliases all Arc fields of `shared` so pending entries
    // registered by `proto_client.request()` are resolved by the read loop.
    let proto_client = shared.clone_shared();

    // Bridge: forward outbound JSON-RPC messages to the stdio transport.
    let transport = Arc::new(transport);
    let transport_send = Arc::clone(&transport);
    tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if transport_send.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Bridge: forward inbound lines from the transport to the read loop.
    let transport_recv = Arc::clone(&transport);
    tokio::spawn(async move {
        use futures::StreamExt;
        let mut stream = transport_recv.receive();
        while let Some(msg) = stream.next().await {
            if in_tx.send(msg).is_err() {
                break;
            }
        }
    });

    // Perform the MCP initialize / notifications/initialized handshake.
    let protocol = McpProtocol::new(proto_client);
    let client_info = Implementation {
        name: "xzatoma-test".to_string(),
        version: "0.0.0".to_string(),
        description: None,
    };

    let session = tokio::time::timeout(
        Duration::from_secs(10),
        protocol.initialize(client_info, ClientCapabilities::default()),
    )
    .await
    .expect("initialize timed out")
    .expect("initialize failed");

    (session, token)
}

// ---------------------------------------------------------------------------
// Task 2.7 required integration tests
// ---------------------------------------------------------------------------

/// Spawn the test server, perform `initialize`, and verify that the server
/// advertises the `Tools` capability.
///
/// Also calls `tools/list` and verifies that the `"echo"` tool is present.
#[tokio::test]
async fn test_stdio_transport_initialize_and_list_tools() {
    let (session, token) = spawn_and_initialize().await;

    // The test server must advertise Tools capability.
    assert!(
        session.capable(ServerCapabilityFlag::Tools),
        "test server must advertise Tools capability"
    );

    // List tools and find "echo".
    let tools = tokio::time::timeout(Duration::from_secs(10), session.list_tools())
        .await
        .expect("list_tools timed out")
        .expect("list_tools failed");

    assert!(
        !tools.is_empty(),
        "expected at least one tool from test server"
    );

    let echo_tool = tools.iter().find(|t| t.name == "echo");
    assert!(
        echo_tool.is_some(),
        "expected 'echo' tool in tools list; got: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let echo = echo_tool.unwrap();
    assert_eq!(echo.name, "echo");
    assert!(
        echo.description
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("echo"),
        "expected description to mention 'echo'; got: {:?}",
        echo.description
    );

    token.cancel();
}

/// Call the `echo` tool with `message: "hello"` and verify the response
/// contains `"hello"` as the text content.
#[tokio::test]
async fn test_stdio_transport_call_echo_tool() {
    let (session, token) = spawn_and_initialize().await;

    let response = tokio::time::timeout(
        Duration::from_secs(10),
        session.call_tool("echo", Some(serde_json::json!({"message": "hello"})), None),
    )
    .await
    .expect("call_tool timed out")
    .expect("call_tool failed");

    assert!(
        response.is_error != Some(true),
        "expected is_error to be false or absent; got: {:?}",
        response.is_error
    );

    assert!(
        !response.content.is_empty(),
        "expected at least one content item in response"
    );

    // Find the first Text content item and verify its value.
    let text_content = response.content.iter().find_map(|c| {
        if let xzatoma::mcp::types::ToolResponseContent::Text { text } = c {
            Some(text.as_str())
        } else {
            None
        }
    });

    assert!(
        text_content.is_some(),
        "expected a Text content item in the echo response"
    );
    assert_eq!(
        text_content.unwrap(),
        "hello",
        "echo tool must return the exact input message"
    );

    token.cancel();
}

/// Verify that `ping` round-trips successfully over the stdio transport.
#[tokio::test]
async fn test_stdio_transport_ping_succeeds() {
    let (session, token) = spawn_and_initialize().await;

    tokio::time::timeout(Duration::from_secs(5), session.ping())
        .await
        .expect("ping timed out")
        .expect("ping failed");

    token.cancel();
}

/// Call `echo` multiple times sequentially and verify each call returns the
/// correct message.
#[tokio::test]
async fn test_stdio_transport_sequential_echo_calls() {
    let (session, token) = spawn_and_initialize().await;

    for msg in &["alpha", "beta", "gamma"] {
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            session.call_tool("echo", Some(serde_json::json!({"message": msg})), None),
        )
        .await
        .expect("call_tool timed out")
        .expect("call_tool failed");

        let text = response.content.iter().find_map(|c| {
            if let xzatoma::mcp::types::ToolResponseContent::Text { text } = c {
                Some(text.as_str())
            } else {
                None
            }
        });

        assert_eq!(
            text,
            Some(*msg),
            "echo tool must return '{msg}' exactly; got: {text:?}"
        );
    }

    token.cancel();
}

/// Verify that calling an unknown tool returns an error response.
///
/// The test server returns a JSON-RPC error for unknown tool names.
#[tokio::test]
async fn test_stdio_transport_unknown_tool_returns_error() {
    let (session, token) = spawn_and_initialize().await;

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        session.call_tool("nonexistent_tool_xyz", None, None),
    )
    .await
    .expect("call_tool timed out");

    // The test server returns a JSON-RPC error for unknown tools, which
    // the protocol layer should surface as an Err variant.
    assert!(
        result.is_err(),
        "expected an error for unknown tool; got Ok"
    );

    token.cancel();
}
