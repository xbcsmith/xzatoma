//! End-to-end integration tests for Phase 5B: MCP Tool Execution via Registry
//!
//! Covers Task 5B.5 requirement:
//!
//! - `test_end_to_end_tool_call_via_registry` -- spawn the test MCP server from
//!   Phase 2; build a `McpClientManager`; connect to the server using server ID
//!   `"test_server"`; call `register_mcp_tools`; call
//!   `registry.get("test_server__echo").unwrap().execute(json!({"message":
//!   "hello"})).await`; assert result output is `"hello"`.
//!
//! The `mcp_test_server` binary must be built before running these tests.
//! Cargo sets `CARGO_BIN_EXE_mcp_test_server` automatically when running
//! integration tests in the same workspace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use xzatoma::config::ExecutionMode;
use xzatoma::mcp::auth::token_store::TokenStore;
use xzatoma::mcp::manager::McpClientManager;
use xzatoma::mcp::server::{
    McpApprovalAction, McpServerApprovalPolicy, McpServerConfig, McpServerTransportConfig,
};
use xzatoma::mcp::tool_bridge::register_mcp_tools;
use xzatoma::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to the `mcp_test_server` binary.
///
/// Cargo sets `CARGO_BIN_EXE_mcp_test_server` automatically when running
/// integration tests within the same workspace. Falls back to searching in
/// `target/debug` for manual test runs.
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

/// Build a `McpServerConfig` that spawns the `mcp_test_server` binary via
/// stdio transport.
///
/// The server ID is `"test_server"` so that tools are registered under the
/// name `"test_server__<tool_name>"`.
fn test_server_config() -> McpServerConfig {
    let mut tools = HashMap::new();
    tools.insert("*".to_string(), McpApprovalAction::Allow);
    McpServerConfig {
        id: "test_server".to_string(),
        transport: McpServerTransportConfig::Stdio {
            executable: test_server_exe().to_string_lossy().into_owned(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        },
        enabled: true,
        timeout_seconds: 15,
        tools_enabled: true,
        resources_enabled: false,
        prompts_enabled: false,
        sampling_enabled: false,
        elicitation_enabled: false,
        approval: McpServerApprovalPolicy {
            trusted: true,
            default_tool_action: McpApprovalAction::Allow,
            tools,
            resource_read_action: McpApprovalAction::Allow,
            prompt_get_action: McpApprovalAction::Allow,
        },
    }
}

// ---------------------------------------------------------------------------
// Task 5B.5: test_end_to_end_tool_call_via_registry
// ---------------------------------------------------------------------------

/// End-to-end test that flows from `ToolRegistry::get` through
/// `McpToolExecutor::execute` to the test server subprocess and back.
///
/// Steps:
/// 1. Spawn the `mcp_test_server` binary via `McpClientManager::connect`.
/// 2. Call `register_mcp_tools` to populate a `ToolRegistry`.
/// 3. Look up `"test_server__echo"` from the registry.
/// 4. Call `execute(json!({"message": "hello"}))`.
/// 5. Assert that the output is `"hello"`.
#[tokio::test]
async fn test_end_to_end_tool_call_via_registry() {
    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    // Connect to the test server.  The binary is spawned as a subprocess.
    let config = test_server_config();
    let connect_result =
        tokio::time::timeout(Duration::from_secs(15), manager.connect(config)).await;

    match connect_result {
        Err(_) => panic!("McpClientManager::connect timed out -- is mcp_test_server built?"),
        Ok(Err(e)) => panic!(
            "McpClientManager::connect failed: {}\n\
             Run `cargo build --bin mcp_test_server` first.",
            e
        ),
        Ok(Ok(())) => {}
    }

    // Verify the server registered at least one tool.
    let connected = manager.connected_servers();
    assert!(
        !connected.is_empty(),
        "expected at least one connected server after connect()"
    );

    // Wrap in Arc<RwLock<>> for register_mcp_tools.
    let manager = Arc::new(RwLock::new(manager));

    // Build a ToolRegistry and register MCP tools into it.
    // headless=true and FullAutonomous mode so no approval prompts are shown.
    let mut registry = ToolRegistry::new();
    let count = tokio::time::timeout(
        Duration::from_secs(10),
        register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            true,
        ),
    )
    .await
    .expect("register_mcp_tools timed out")
    .expect("register_mcp_tools returned an error");

    assert!(
        count > 0,
        "expected at least one MCP tool to be registered; got 0"
    );

    // The test server exposes the "echo" tool.  After registration it must be
    // accessible under the namespaced name "test_server__echo".
    let executor = registry.get("test_server__echo").unwrap_or_else(|| {
        let registered: Vec<String> = registry.tool_names();
        panic!(
            "expected 'test_server__echo' in registry; found: {:?}",
            registered
        )
    });

    // Call the echo tool with {"message": "hello"}.
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        executor.execute(serde_json::json!({"message": "hello"})),
    )
    .await
    .expect("executor.execute timed out")
    .expect("executor.execute returned an error");

    assert!(
        result.success,
        "echo tool call must succeed; error: {:?}",
        result.error
    );

    assert_eq!(
        result.output, "hello",
        "echo tool must return the exact input message; got: {:?}",
        result.output
    );
}

// ---------------------------------------------------------------------------
// Additional coverage: multiple sequential calls through the registry
// ---------------------------------------------------------------------------

/// Call the `echo` tool multiple times sequentially to verify the executor
/// correctly reuses the underlying connection across calls.
#[tokio::test]
async fn test_end_to_end_sequential_echo_calls_via_registry() {
    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    let config = test_server_config();
    let connect_result =
        tokio::time::timeout(Duration::from_secs(15), manager.connect(config)).await;

    match connect_result {
        Err(_) => panic!("McpClientManager::connect timed out"),
        Ok(Err(e)) => panic!("McpClientManager::connect failed: {}", e),
        Ok(Ok(())) => {}
    }

    let manager = Arc::new(RwLock::new(manager));
    let mut registry = ToolRegistry::new();

    tokio::time::timeout(
        Duration::from_secs(10),
        register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            true,
        ),
    )
    .await
    .expect("register_mcp_tools timed out")
    .expect("register_mcp_tools returned an error");

    let executor = registry
        .get("test_server__echo")
        .expect("test_server__echo must be in registry");

    for msg in &["alpha", "beta", "gamma", "delta"] {
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            executor.execute(serde_json::json!({"message": msg})),
        )
        .await
        .unwrap_or_else(|_| panic!("execute timed out for message '{}'", msg))
        .unwrap_or_else(|e| panic!("execute failed for message '{}': {}", msg, e));

        assert!(
            result.success,
            "echo call must succeed for message '{}'; error: {:?}",
            msg, result.error
        );
        assert_eq!(
            result.output, *msg,
            "echo must return '{}' exactly; got: {:?}",
            msg, result.output
        );
    }
}

// ---------------------------------------------------------------------------
// Additional coverage: tool_definition matches expected schema
// ---------------------------------------------------------------------------

/// Verify that the registered `McpToolExecutor` for `test_server__echo` has
/// a correctly structured tool definition (name, description, parameters).
#[tokio::test]
async fn test_end_to_end_tool_definition_is_well_formed() {
    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    let config = test_server_config();
    tokio::time::timeout(Duration::from_secs(15), manager.connect(config))
        .await
        .expect("connect timed out")
        .expect("connect failed");

    let manager = Arc::new(RwLock::new(manager));
    let mut registry = ToolRegistry::new();

    tokio::time::timeout(
        Duration::from_secs(10),
        register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            true,
        ),
    )
    .await
    .expect("register_mcp_tools timed out")
    .expect("register_mcp_tools returned an error");

    let executor = registry
        .get("test_server__echo")
        .expect("test_server__echo must be in registry");

    let def = executor.tool_definition();

    assert_eq!(
        def["name"], "test_server__echo",
        "tool definition name must be the namespaced registry name"
    );

    assert!(
        def["description"].is_string(),
        "tool definition must have a string description; got: {:?}",
        def["description"]
    );
    assert!(
        !def["description"].as_str().unwrap_or("").is_empty(),
        "tool definition description must not be empty"
    );

    assert!(
        def["parameters"].is_object(),
        "tool definition must have a parameters object; got: {:?}",
        def["parameters"]
    );
}

// ---------------------------------------------------------------------------
// Additional coverage: tool_names exposed after registration
// ---------------------------------------------------------------------------

/// After `register_mcp_tools`, the registry must expose the namespaced tool
/// name via `tool_names()`.
#[tokio::test]
async fn test_end_to_end_registry_contains_namespaced_tool_name() {
    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    let config = test_server_config();
    tokio::time::timeout(Duration::from_secs(15), manager.connect(config))
        .await
        .expect("connect timed out")
        .expect("connect failed");

    let manager = Arc::new(RwLock::new(manager));
    let mut registry = ToolRegistry::new();

    tokio::time::timeout(
        Duration::from_secs(10),
        register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            true,
        ),
    )
    .await
    .expect("register_mcp_tools timed out")
    .expect("register_mcp_tools returned an error");

    let names: Vec<String> = registry.tool_names();
    assert!(
        names.iter().any(|n| n == "test_server__echo"),
        "registry tool_names() must include 'test_server__echo'; found: {:?}",
        names
    );
}
