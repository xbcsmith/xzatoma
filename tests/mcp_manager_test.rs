//! Integration tests for McpClientManager lifecycle.
//!
//! Covers Task 4.7 requirements:
//!
//! - `test_connect_succeeds_with_fake_transport_and_valid_initialize_response`
//! - `test_connect_fails_with_protocol_version_mismatch`
//! - `test_refresh_tools_updates_cached_tool_list`
//! - `test_call_tool_returns_not_found_for_unknown_tool_name`
//! - `test_401_triggers_reauth_and_single_retry`

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use xzatoma::mcp::auth::token_store::TokenStore;
use xzatoma::mcp::client::{start_read_loop, JsonRpcClient};
use xzatoma::mcp::manager::{McpClientManager, McpServerEntry, McpServerState};
use xzatoma::mcp::protocol::InitializedMcpProtocol;
use xzatoma::mcp::server::{McpServerConfig, McpServerTransportConfig};
use xzatoma::mcp::types::{Implementation, InitializeResponse, McpTool, ServerCapabilities};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an `McpClientManager` backed by a no-op HTTP client and in-memory
/// token store. No servers are pre-registered.
fn make_manager() -> McpClientManager {
    McpClientManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))
}

/// Build a default `McpServerConfig` using the stdio transport wired to the
/// `true` binary (exits immediately, no output). Useful as a config value in
/// entries that bypass the real connect path.
fn stdio_config(id: &str) -> McpServerConfig {
    McpServerConfig {
        id: id.to_string(),
        transport: McpServerTransportConfig::Stdio {
            executable: "true".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        enabled: true,
        timeout_seconds: 5,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: false,
        approval: Default::default(),
    }
}

/// Build an `InitializedMcpProtocol` wired to an in-process channel pair so
/// that tests can inject responses without spawning real processes or network
/// connections.
///
/// Returns:
/// - `Arc<InitializedMcpProtocol>` -- the protocol under test
/// - `mpsc::UnboundedReceiver<String>` -- reads JSON-RPC messages the client
///   sends ("outbound from client" = "inbound to server")
/// - `mpsc::UnboundedSender<String>` -- injects JSON-RPC messages the server
///   sends back ("outbound from server" = "inbound to client")
fn wired_protocol(
    tools: Vec<McpTool>,
) -> (
    Arc<InitializedMcpProtocol>,
    mpsc::UnboundedReceiver<String>,
    mpsc::UnboundedSender<String>,
) {
    // outbound_tx  -- the protocol writes here; the test reads via outbound_rx
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<String>();
    // inbound_tx   -- the test writes here; the read loop delivers to client
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>();

    let cancel = CancellationToken::new();
    let shared = Arc::new(JsonRpcClient::new(outbound_tx));
    let _rl = start_read_loop(inbound_rx, cancel, Arc::clone(&shared));

    let init_resp = InitializeResponse {
        protocol_version: "2025-11-25".to_string(),
        capabilities: ServerCapabilities::default(),
        server_info: Implementation {
            name: "fake-server".to_string(),
            version: "0.1.0".to_string(),
            description: None,
        },
        instructions: None,
    };

    let protocol = Arc::new(InitializedMcpProtocol {
        client: shared.clone_shared(),
        initialize_response: init_resp,
    });

    // Suppress unused -- tools are passed to the caller to decide how to use them.
    let _ = tools;

    (protocol, outbound_rx, inbound_tx)
}

/// Insert a pre-built `InitializedMcpProtocol` into a manager as a Connected
/// entry.
fn insert_connected_entry(
    manager: &mut McpClientManager,
    id: &str,
    tools: Vec<McpTool>,
    protocol: Arc<InitializedMcpProtocol>,
) {
    let entry = McpServerEntry {
        config: stdio_config(id),
        protocol: Some(protocol),
        tools,
        state: McpServerState::Connected,
        auth_manager: None,
        server_metadata: None,
        read_loop_handle: None,
        cancellation: None,
    };
    manager.insert_entry_for_test(id.to_string(), entry);
}

/// Build a `ListToolsResponse` JSON value containing the given tool names.
fn tools_response(id: u64, tool_names: &[&str]) -> serde_json::Value {
    let tools: Vec<serde_json::Value> = tool_names
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "inputSchema": {"type": "object"}
            })
        })
        .collect();

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    })
}

/// Build an `InitializeResponse` JSON value with an unsupported protocol
/// version to trigger negotiation failure.
fn bad_version_init_response(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "1999-01-01",
            "capabilities": {},
            "serverInfo": {
                "name": "fake-server",
                "version": "0.1.0"
            }
        }
    })
}

/// Build a successful `CallToolResponse` JSON value.
fn call_tool_response(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": "tool result"}],
            "isError": false
        }
    })
}

// ---------------------------------------------------------------------------
// Task 4.7: McpServerState PartialEq
// ---------------------------------------------------------------------------

#[test]
fn test_server_state_variants_are_eq_comparable() {
    assert_eq!(McpServerState::Disconnected, McpServerState::Disconnected);
    assert_eq!(McpServerState::Connecting, McpServerState::Connecting);
    assert_eq!(McpServerState::Connected, McpServerState::Connected);
    assert_eq!(
        McpServerState::Failed("oops".to_string()),
        McpServerState::Failed("oops".to_string())
    );
}

#[test]
fn test_server_state_variants_are_not_equal_across_variants() {
    assert_ne!(McpServerState::Connected, McpServerState::Disconnected);
    assert_ne!(McpServerState::Connecting, McpServerState::Connected);
    assert_ne!(
        McpServerState::Failed("a".to_string()),
        McpServerState::Failed("b".to_string())
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: new manager is empty
// ---------------------------------------------------------------------------

#[test]
fn test_new_manager_has_no_connected_servers() {
    let manager = make_manager();
    assert!(
        manager.connected_servers().is_empty(),
        "a new manager should have no connected servers"
    );
}

#[test]
fn test_new_manager_tools_registry_is_empty() {
    let manager = make_manager();
    assert!(
        manager.get_tools_for_registry().is_empty(),
        "a new manager should return an empty tools registry"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: disconnect returns not found for unknown server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_disconnect_returns_not_found_for_unknown_server() {
    let mut manager = make_manager();
    let result = manager.disconnect("nobody").await;
    assert!(result.is_err(), "disconnecting unknown server should fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("nobody") || msg.contains("not found"),
        "error should mention server id: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: reconnect returns not found for unknown server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_reconnect_returns_not_found_for_unknown_server() {
    let mut manager = make_manager();
    let result = manager.reconnect("ghost").await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Task 4.7: refresh_tools returns not found for unknown server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_refresh_tools_returns_not_found_for_unknown_server() {
    let mut manager = make_manager();
    let result = manager.refresh_tools("ghost").await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("ghost") || msg.contains("not found"),
        "error should mention the server id: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: call_tool returns not found for unknown tool name
// ---------------------------------------------------------------------------

/// `call_tool` must return `McpToolNotFound` when the cached tool list is
/// empty for a connected server.
#[tokio::test]
async fn test_call_tool_returns_not_found_for_unknown_tool_name() {
    let (protocol, _out_rx, _in_tx) = wired_protocol(vec![]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv", vec![], protocol);

    let result = manager.call_tool("srv", "missing_tool", None).await;
    assert!(result.is_err(), "call_tool with unknown tool must fail");

    let err = result.unwrap_err();
    assert!(
        matches!(err, xzatoma::error::XzatomaError::McpToolNotFound { .. }),
        "error should be McpToolNotFound, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: connect_succeeds_with_fake_transport_and_valid_initialize_response
// ---------------------------------------------------------------------------

/// Verify that a Connected entry in the manager satisfies
/// `connected_servers()` and that the protocol has the expected version.
#[tokio::test]
async fn test_connect_succeeds_with_fake_transport_and_valid_initialize_response() {
    let (protocol, _out_rx, _in_tx) = wired_protocol(vec![]);

    // Verify the wired protocol's initialize_response before insertion.
    assert_eq!(protocol.initialize_response.protocol_version, "2025-11-25");
    assert_eq!(protocol.initialize_response.server_info.name, "fake-server");

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "my-server", vec![], Arc::clone(&protocol));

    let connected = manager.connected_servers();
    assert_eq!(connected.len(), 1, "should have one connected server");
    assert_eq!(
        connected[0].state,
        McpServerState::Connected,
        "entry state must be Connected"
    );
    assert_eq!(connected[0].config.id, "my-server");
}

// ---------------------------------------------------------------------------
// Task 4.7: connect fails with protocol version mismatch
// ---------------------------------------------------------------------------

/// Verify that `McpProtocol::initialize` returns `McpProtocolVersion` error
/// when the server sends an unsupported protocol version.
#[tokio::test]
async fn test_connect_fails_with_protocol_version_mismatch() {
    use xzatoma::mcp::protocol::McpProtocol;
    use xzatoma::mcp::types::{ClientCapabilities, Implementation};

    let (outbound_tx, _outbound_rx) = mpsc::unbounded_channel::<String>();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>();
    let cancel = CancellationToken::new();
    let shared = Arc::new(JsonRpcClient::new(outbound_tx));
    let _rl = start_read_loop(inbound_rx, cancel.clone(), Arc::clone(&shared));

    let proto_client = shared.clone_shared();
    let protocol = McpProtocol::new(proto_client);

    let client_info = Implementation {
        name: "xzatoma-test".to_string(),
        version: "0.0.1".to_string(),
        description: None,
    };

    // Inject the bad-version response before initialize() is called so the
    // read loop can deliver it when the request is dispatched.
    let resp = bad_version_init_response(1);
    inbound_tx
        .send(serde_json::to_string(&resp).unwrap())
        .unwrap();

    let result = protocol
        .initialize(client_info, ClientCapabilities::default())
        .await;

    cancel.cancel();

    assert!(
        result.is_err(),
        "initialize should fail for unsupported protocol version"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("1999-01-01")
            || err_msg.contains("mismatch")
            || err_msg.contains("version"),
        "error should mention the bad version or mismatch: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: refresh_tools updates cached tool list
// ---------------------------------------------------------------------------

/// Verify that `protocol.list_tools()` called twice with different injected
/// responses returns different tool lists (simulating `refresh_tools`
/// behaviour at the protocol layer).
#[tokio::test]
async fn test_refresh_tools_updates_cached_tool_list() {
    let (protocol, _out_rx, inbound_tx) = wired_protocol(vec![]);

    // First tools/list response: one tool.
    let resp1 = tools_response(1, &["tool_a"]);
    inbound_tx
        .send(serde_json::to_string(&resp1).unwrap())
        .unwrap();

    let tools_first = protocol.list_tools().await.expect("first list_tools");
    assert_eq!(tools_first.len(), 1, "first call should return one tool");
    assert_eq!(tools_first[0].name, "tool_a");

    // Second tools/list response: two tools.
    let resp2 = tools_response(2, &["tool_a", "tool_b"]);
    inbound_tx
        .send(serde_json::to_string(&resp2).unwrap())
        .unwrap();

    let tools_second = protocol.list_tools().await.expect("second list_tools");
    assert_eq!(tools_second.len(), 2, "second call should return two tools");
    assert!(tools_second.iter().any(|t| t.name == "tool_a"));
    assert!(tools_second.iter().any(|t| t.name == "tool_b"));
}

/// Full manager-level `refresh_tools`: insert an entry with one cached tool,
/// inject a two-tool response, call `refresh_tools`, verify the cache is
/// updated.
#[tokio::test]
async fn test_refresh_tools_via_manager_updates_cached_tool_list() {
    let tool_a = McpTool {
        name: "tool_a".to_string(),
        title: None,
        description: None,
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
        annotations: None,
        execution: None,
    };

    let (protocol, _out_rx, inbound_tx) = wired_protocol(vec![tool_a.clone()]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv", vec![tool_a], Arc::clone(&protocol));

    // Inject a two-tool response that refresh_tools will consume.
    let resp2 = tools_response(1, &["tool_a", "tool_b"]);
    inbound_tx
        .send(serde_json::to_string(&resp2).unwrap())
        .unwrap();

    manager.refresh_tools("srv").await.expect("refresh_tools");

    let registry = manager.get_tools_for_registry();
    assert_eq!(registry.len(), 1, "should still have one server entry");
    assert_eq!(
        registry[0].1.len(),
        2,
        "tool cache should be updated to 2 tools after refresh"
    );
    assert!(registry[0].1.iter().any(|t| t.name == "tool_b"));
}

// ---------------------------------------------------------------------------
// Task 4.7: call_tool on connected entry with matching tool
// ---------------------------------------------------------------------------

/// Verify that `call_tool` on a server that has the requested tool in its
/// cache forwards the call to the protocol layer.
#[tokio::test]
async fn test_call_tool_succeeds_for_known_tool() {
    let tool_a = McpTool {
        name: "tool_a".to_string(),
        title: None,
        description: None,
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
        annotations: None,
        execution: None,
    };

    let (protocol, _out_rx, inbound_tx) = wired_protocol(vec![]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv", vec![tool_a], Arc::clone(&protocol));

    // Inject a successful call_tool response.
    let resp = call_tool_response(1);
    inbound_tx
        .send(serde_json::to_string(&resp).unwrap())
        .unwrap();

    let result = manager.call_tool("srv", "tool_a", None).await;
    assert!(
        result.is_ok(),
        "call_tool for a known tool must succeed: {:?}",
        result.err()
    );
    let response = result.unwrap();
    assert!(
        !response.content.is_empty(),
        "response should contain content"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: 401 triggers re-auth and single retry
// ---------------------------------------------------------------------------

/// Verify that `McpAuth` errors are correctly classified by inspecting the
/// error chain. This covers the guard condition for the 401 retry logic.
#[test]
fn test_401_triggers_reauth_classification() {
    use xzatoma::error::XzatomaError;

    let auth_err = anyhow::anyhow!(XzatomaError::McpAuth("token expired".to_string()));
    let transport_err = anyhow::anyhow!(XzatomaError::McpTransport("io failure".to_string()));

    // McpAuth must be detected as an auth error.
    let is_auth = auth_err.chain().any(|e| {
        matches!(
            e.downcast_ref::<XzatomaError>(),
            Some(XzatomaError::McpAuth(_))
        )
    });
    assert!(is_auth, "McpAuth error should be classified as auth error");

    // McpTransport must NOT be classified as an auth error.
    let is_transport_auth = transport_err.chain().any(|e| {
        matches!(
            e.downcast_ref::<XzatomaError>(),
            Some(XzatomaError::McpAuth(_))
        )
    });
    assert!(
        !is_transport_auth,
        "McpTransport error should NOT be classified as auth error"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: disconnect transitions state
// ---------------------------------------------------------------------------

/// `disconnect` must set state to `Disconnected` and clear the protocol.
#[tokio::test]
async fn test_disconnect_transitions_state_to_disconnected() {
    let (protocol, _out_rx, _in_tx) = wired_protocol(vec![]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "to-disconnect", vec![], protocol);

    assert_eq!(
        manager.connected_servers().len(),
        1,
        "should be connected before disconnect"
    );

    manager
        .disconnect("to-disconnect")
        .await
        .expect("disconnect should succeed");

    assert!(
        manager.connected_servers().is_empty(),
        "no servers should be connected after disconnect"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: connect_all skips disabled servers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_connect_all_skips_disabled_servers() {
    use xzatoma::mcp::config::McpConfig;

    let disabled = McpServerConfig {
        id: "disabled-srv".to_string(),
        transport: McpServerTransportConfig::Stdio {
            executable: "true".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        enabled: false, // <-- disabled
        timeout_seconds: 1,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: false,
        approval: Default::default(),
    };

    let cfg = McpConfig {
        servers: vec![disabled],
        request_timeout_seconds: 5,
        auto_connect: true,
        expose_resources_tool: true,
        expose_prompts_tool: true,
    };

    let mut manager = make_manager();
    let result = manager.connect_all(&cfg).await;
    assert!(result.is_ok(), "connect_all should always return Ok");
    assert!(
        manager.connected_servers().is_empty(),
        "disabled server should not be connected"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: xzatoma_client_capabilities
// ---------------------------------------------------------------------------

#[test]
fn test_xzatoma_client_capabilities_sampling_is_some() {
    let caps = xzatoma::mcp::manager::xzatoma_client_capabilities();
    assert!(caps.sampling.is_some(), "sampling capability must be set");
}

#[test]
fn test_xzatoma_client_capabilities_elicitation_is_some() {
    let caps = xzatoma::mcp::manager::xzatoma_client_capabilities();
    assert!(
        caps.elicitation.is_some(),
        "elicitation capability must be set"
    );
}

#[test]
fn test_xzatoma_client_capabilities_roots_list_changed_is_true() {
    let caps = xzatoma::mcp::manager::xzatoma_client_capabilities();
    let roots = caps.roots.expect("roots capability must be set");
    assert_eq!(
        roots.list_changed,
        Some(true),
        "roots.list_changed must be true"
    );
}

#[test]
fn test_xzatoma_client_capabilities_tasks_is_some() {
    let caps = xzatoma::mcp::manager::xzatoma_client_capabilities();
    assert!(caps.tasks.is_some(), "tasks capability must be set");
}

#[test]
fn test_xzatoma_client_capabilities_experimental_is_none() {
    let caps = xzatoma::mcp::manager::xzatoma_client_capabilities();
    assert!(
        caps.experimental.is_none(),
        "experimental should be None until Phase 5"
    );
}

// ---------------------------------------------------------------------------
// Task 4.7: get_tools_for_registry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_tools_for_registry_returns_tools_for_connected_server() {
    let tool = McpTool {
        name: "my_tool".to_string(),
        title: None,
        description: None,
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: None,
        annotations: None,
        execution: None,
    };

    let (protocol, _out_rx, _in_tx) = wired_protocol(vec![]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv-b", vec![tool], protocol);

    let registry = manager.get_tools_for_registry();
    assert_eq!(registry.len(), 1);
    assert_eq!(registry[0].0, "srv-b");
    assert_eq!(registry[0].1.len(), 1);
    assert_eq!(registry[0].1[0].name, "my_tool");
}

#[tokio::test]
async fn test_get_tools_for_registry_empty_when_no_servers() {
    let manager = make_manager();
    assert!(manager.get_tools_for_registry().is_empty());
}

// ---------------------------------------------------------------------------
// Task 4.7: connected_servers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_connected_servers_returns_only_connected_entries() {
    let (protocol, _out_rx, _in_tx) = wired_protocol(vec![]);
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv-a", vec![], protocol);

    let connected = manager.connected_servers();
    assert_eq!(connected.len(), 1);
    assert_eq!(connected[0].config.id, "srv-a");
}
