//! Integration tests for MCP tool bridge: executor routing, approval policy, task support, and tool registration.
//!
//! Covers tool bridge requirements:
//!
//! - `test_registry_name_uses_double_underscore_separator`
//! - `test_tool_definition_format_matches_xzatoma_convention`
//! - `test_execute_returns_success_for_text_response`
//! - `test_execute_maps_is_error_to_error_result`
//! - `test_structured_content_appended_after_delimiter`
//! - `test_should_auto_approve_false_in_full_autonomous_mode`
//! - `test_should_auto_approve_true_when_headless`
//! - `test_should_auto_approve_false_in_interactive_mode`
//! - `test_task_support_required_routes_to_call_tool_as_task`

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use xzatoma::config::ExecutionMode;
use xzatoma::mcp::approval::should_auto_approve;
use xzatoma::mcp::auth::token_store::TokenStore;
use xzatoma::mcp::client::{start_read_loop, JsonRpcClient};
use xzatoma::mcp::manager::{McpClientManager, McpServerEntry, McpServerState};
use xzatoma::mcp::protocol::InitializedMcpProtocol;
use xzatoma::mcp::server::{
    McpApprovalAction, McpServerApprovalPolicy, McpServerConfig, McpServerTransportConfig,
};
use xzatoma::mcp::tool_bridge::{register_mcp_tools, McpToolExecutor};
use xzatoma::mcp::types::{
    Implementation, InitializeResponse, McpTool, ServerCapabilities, TaskSupport, ToolExecution,
};
use xzatoma::tools::{ToolExecutor, ToolRegistry};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Build a bare `McpClientManager` with no servers registered.
fn make_manager() -> McpClientManager {
    McpClientManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))
}

/// Build a default stdio `McpServerConfig`.
fn stdio_config(id: &str) -> McpServerConfig {
    let mut tools = HashMap::new();
    tools.insert("*".to_string(), McpApprovalAction::Allow);
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
        approval: McpServerApprovalPolicy {
            trusted: true,
            default_tool_action: McpApprovalAction::Allow,
            tools,
            resource_read_action: McpApprovalAction::Allow,
            prompt_get_action: McpApprovalAction::Allow,
        },
    }
}

/// Build an `InitializedMcpProtocol` wired to in-process channels.
///
/// Returns:
/// - `Arc<InitializedMcpProtocol>` ready for insertion into a manager
/// - `mpsc::UnboundedReceiver<String>` -- reads what the client sent
/// - `mpsc::UnboundedSender<String>` -- injects server responses
fn wired_protocol() -> (
    Arc<InitializedMcpProtocol>,
    mpsc::UnboundedReceiver<String>,
    mpsc::UnboundedSender<String>,
) {
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<String>();
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

    (protocol, outbound_rx, inbound_tx)
}

/// Insert a pre-built protocol into a manager as a Connected entry.
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

/// Build an `McpTool` with the given name and optional `TaskSupport`.
fn make_tool(name: &str, task_support: Option<TaskSupport>) -> McpTool {
    McpTool {
        name: name.to_string(),
        title: None,
        description: Some(format!("Description of {}", name)),
        input_schema: serde_json::json!({"type": "object", "properties": {}}),
        output_schema: None,
        annotations: None,
        execution: task_support.map(|ts| ToolExecution {
            task_support: Some(ts),
        }),
    }
}

/// Build a JSON-RPC `CallToolResponse` payload for injection into `inbound_tx`.
fn call_tool_response_json(
    id: u64,
    text: &str,
    is_error: bool,
    structured_content: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut result = serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "isError": is_error
    });
    if let Some(sc) = structured_content {
        result["structuredContent"] = sc;
    }
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

/// Build a JSON-RPC `CallToolResponse` where the call was dispatched as a task.
/// The response meta contains a `taskId` field to simulate the task-based path.
fn call_tool_as_task_response_json(id: u64, text: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": text}],
            "isError": false,
            "_meta": {"taskId": "task-abc-123"}
        }
    })
}

/// Build an `McpToolExecutor` backed by the given manager.
fn make_executor(
    server_id: &str,
    tool_name: &str,
    task_support: Option<TaskSupport>,
    manager: Arc<RwLock<McpClientManager>>,
    execution_mode: ExecutionMode,
    headless: bool,
) -> McpToolExecutor {
    McpToolExecutor {
        server_id: server_id.to_string(),
        tool_name: tool_name.to_string(),
        registry_name: format!("{}__{}", server_id, tool_name),
        description: format!("Description of {}", tool_name),
        input_schema: serde_json::json!({"type": "object", "properties": {}}),
        task_support,
        manager,
        execution_mode,
        headless,
    }
}

// ---------------------------------------------------------------------------
// MCP approval policy
// ---------------------------------------------------------------------------

/// `should_auto_approve` returns `false` for `FullAutonomous`, `headless: false`.
#[test]
fn test_should_auto_approve_false_in_full_autonomous_mode() {
    assert!(
        !should_auto_approve(ExecutionMode::FullAutonomous, false),
        "FullAutonomous, headless=false must not imply MCP trust"
    );
}

/// `should_auto_approve` returns `false` when `headless: true` regardless of mode.
#[test]
fn test_should_auto_approve_false_when_headless() {
    assert!(
        !should_auto_approve(ExecutionMode::Interactive, true),
        "Interactive, headless=true must not imply MCP trust"
    );
    assert!(
        !should_auto_approve(ExecutionMode::RestrictedAutonomous, true),
        "RestrictedAutonomous, headless=true must not imply MCP trust"
    );
    assert!(
        !should_auto_approve(ExecutionMode::FullAutonomous, true),
        "FullAutonomous, headless=true must not imply MCP trust"
    );
}

/// `should_auto_approve` returns `false` for `Interactive`, `headless: false`.
#[test]
fn test_should_auto_approve_false_in_interactive_mode() {
    assert!(
        !should_auto_approve(ExecutionMode::Interactive, false),
        "Interactive, headless=false must NOT auto-approve (prompt required)"
    );
}

/// `should_auto_approve` returns `false` for `RestrictedAutonomous`,
/// `headless: false`.
#[test]
fn test_should_auto_approve_false_in_restricted_autonomous_mode() {
    assert!(
        !should_auto_approve(ExecutionMode::RestrictedAutonomous, false),
        "RestrictedAutonomous, headless=false must NOT auto-approve"
    );
}

// ---------------------------------------------------------------------------
// Task 5A.3: registry_name double-underscore separator
// ---------------------------------------------------------------------------

/// The `registry_name` is always `<server_id>__<tool_name>`.
#[test]
fn test_registry_name_uses_double_underscore_separator() {
    let manager = Arc::new(RwLock::new(make_manager()));
    let executor = make_executor(
        "my_server",
        "search_items",
        None,
        manager,
        ExecutionMode::FullAutonomous,
        false,
    );
    assert_eq!(
        executor.registry_name, "my_server__search_items",
        "registry_name must use double underscore as separator"
    );
}

/// Double-underscore rule holds even when both ids contain single underscores.
#[test]
fn test_registry_name_double_underscore_with_underscored_ids() {
    let manager = Arc::new(RwLock::new(make_manager()));
    let executor = make_executor(
        "my_corp_server",
        "do_the_thing",
        None,
        manager,
        ExecutionMode::FullAutonomous,
        false,
    );
    assert_eq!(executor.registry_name, "my_corp_server__do_the_thing");
}

// ---------------------------------------------------------------------------
// Task 5A.3: tool_definition format
// ---------------------------------------------------------------------------

/// `tool_definition()` must return an object with `"name"`, `"description"`,
/// and `"parameters"` top-level keys.
#[test]
fn test_tool_definition_format_matches_xzatoma_convention() {
    let manager = Arc::new(RwLock::new(make_manager()));
    let executor = make_executor(
        "srv",
        "my_tool",
        None,
        manager,
        ExecutionMode::FullAutonomous,
        false,
    );
    let def = executor.tool_definition();

    assert!(def.get("name").is_some(), "missing 'name' key");
    assert!(
        def.get("description").is_some(),
        "missing 'description' key"
    );
    assert!(def.get("parameters").is_some(), "missing 'parameters' key");
}

/// `"name"` in the definition equals `registry_name`.
#[test]
fn test_tool_definition_name_equals_registry_name() {
    let manager = Arc::new(RwLock::new(make_manager()));
    let executor = make_executor(
        "server",
        "tool",
        None,
        manager,
        ExecutionMode::FullAutonomous,
        false,
    );
    let def = executor.tool_definition();
    assert_eq!(
        def["name"].as_str().unwrap(),
        "server__tool",
        "definition name must match registry_name"
    );
}

// ---------------------------------------------------------------------------
// Task 5A.3: execute returns success for text response
// ---------------------------------------------------------------------------

/// `execute` with a plain text tool response returns `ToolResult` with
/// `success == true` and the expected output text.
#[tokio::test]
async fn test_execute_returns_success_for_text_response() {
    let tool = make_tool("echo", None);
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "test_server", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let executor = make_executor(
        "test_server",
        "echo",
        None,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    );

    // Spawn a task that injects the response once the client sends the request.
    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        // Wait for the outbound tools/call request.
        let _req = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out waiting for request")
            .expect("channel closed");

        // Respond with a successful text result.
        let resp = call_tool_response_json(1, "hello from echo", false, None);
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    let result = executor
        .execute(serde_json::json!({"message": "hello"}))
        .await
        .expect("execute must not return Err");

    assert!(
        result.success,
        "ToolResult.success must be true for a non-error response"
    );
    assert!(
        result.output.contains("hello from echo"),
        "output must contain the tool's text response, got: {:?}",
        result.output
    );
}

// ---------------------------------------------------------------------------
// Task 5A.3: execute maps is_error to error result
// ---------------------------------------------------------------------------

/// When the server returns `isError: true`, `execute` must return a
/// `ToolResult` with `success == false`.
#[tokio::test]
async fn test_execute_maps_is_error_to_error_result() {
    let tool = make_tool("failing_tool", None);
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let executor = make_executor(
        "srv",
        "failing_tool",
        None,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    );

    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        let _req = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        // Inject an is_error=true response.
        let resp = call_tool_response_json(1, "something went wrong", true, None);
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    let result = executor
        .execute(serde_json::json!({}))
        .await
        .expect("execute must not return Err even for is_error responses");

    assert!(
        !result.success,
        "ToolResult.success must be false when is_error is true"
    );
    assert!(
        result.error.is_some(),
        "ToolResult.error must be set when is_error is true"
    );
    let err_text = result.error.unwrap();
    assert!(
        err_text.contains("something went wrong"),
        "error text must include the tool's text content, got: {:?}",
        err_text
    );
}

// ---------------------------------------------------------------------------
// Task 5A.3: structured_content appended after delimiter
// ---------------------------------------------------------------------------

/// When the server includes `structuredContent`, `execute` must append it
/// after a `"---"` delimiter in the output.
#[tokio::test]
async fn test_structured_content_appended_after_delimiter() {
    let tool = make_tool("structured_tool", None);
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "srv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let executor = make_executor(
        "srv",
        "structured_tool",
        None,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    );

    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        let _req = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let structured = serde_json::json!({"key": "value"});
        let resp = call_tool_response_json(1, "primary text output", false, Some(structured));
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    let result = executor
        .execute(serde_json::json!({}))
        .await
        .expect("execute must not return Err");

    assert!(result.success, "result must be successful");

    let output = &result.output;
    assert!(
        output.contains("primary text output"),
        "output must contain the primary text, got: {:?}",
        output
    );
    assert!(
        output.contains("---"),
        "output must contain the '---' delimiter before structured content, got: {:?}",
        output
    );
    assert!(
        output.contains("\"key\"") && output.contains("\"value\""),
        "output must contain the structured JSON after the delimiter, got: {:?}",
        output
    );

    // Verify ordering: text comes before the delimiter and the JSON.
    let delimiter_pos = output.find("---").unwrap();
    let text_pos = output.find("primary text output").unwrap();
    assert!(
        text_pos < delimiter_pos,
        "primary text must appear before the '---' delimiter"
    );
}

// ---------------------------------------------------------------------------
// Task 5A.3: task_support Required routes to call_tool_as_task
// ---------------------------------------------------------------------------

/// When `task_support == Some(TaskSupport::Required)`, `execute` uses the
/// `call_tool_as_task` code path.  We verify two behaviors:
///
/// 1. The JSON-RPC outbound request includes the `task` field (task-wrapping
///    path is selected, not the plain `call_tool` path).
/// 2. When the server response includes `_meta.taskId`, `execute` returns
///    [`XzatomaError::McpTask`] with a message identifying the task, because
///    long-running task polling is not supported in this session.
#[tokio::test]
async fn test_task_support_required_routes_to_call_tool_as_task() {
    // -----------------------------------------------------------------------
    // Part 1: task-backed execution returns McpTask error when taskId present.
    // -----------------------------------------------------------------------
    let tool = make_tool("task_tool", Some(TaskSupport::Required));
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "task_srv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let executor = make_executor(
        "task_srv",
        "task_tool",
        Some(TaskSupport::Required),
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    );

    // Capture the outbound request params before injecting a taskId response.
    // We store the result in a shared flag so we can assert it after execute.
    let params_had_task = Arc::new(std::sync::Mutex::new(false));
    let params_flag = Arc::clone(&params_had_task);
    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        let req_str = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out waiting for task request")
            .expect("channel closed");

        let req: serde_json::Value = serde_json::from_str(&req_str).unwrap();
        let id = req["id"].as_u64().unwrap_or(1);

        // Record whether the outbound params included the "task" field.
        let has_task = req["params"].get("task").is_some();
        *params_flag.lock().unwrap() = has_task;

        // Reply with a task-style response (includes _meta.taskId).
        let resp = call_tool_as_task_response_json(id, "task completed");
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    // When the server response contains _meta.taskId, execute returns McpTask
    // error rather than a partial result, giving the caller a typed signal.
    let err = executor
        .execute(serde_json::json!({"input": "run_task"}))
        .await
        .expect_err("execute must return Err when response contains _meta.taskId");
    let err_str = err.to_string();
    assert!(
        err_str.contains("task-abc-123"),
        "McpTask error must identify the returned task ID, got: {err_str}"
    );
    assert!(
        err_str.contains("task polling is not supported"),
        "McpTask error must describe the limitation, got: {err_str}"
    );

    // Allow the spawned task to finish and then verify the outbound params.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        *params_had_task.lock().unwrap(),
        "call_tool_as_task must include a 'task' field in outbound params"
    );

    // -----------------------------------------------------------------------
    // Part 2: A separate executor also returns McpTask and has the task field.
    // -----------------------------------------------------------------------
    let tool2 = make_tool("task_tool2", Some(TaskSupport::Required));
    let (protocol2, mut outbound_rx2, inbound_tx2) = wired_protocol();

    let mut manager2 = make_manager();
    insert_connected_entry(&mut manager2, "task_srv2", vec![tool2], protocol2);
    let manager2 = Arc::new(RwLock::new(manager2));

    let executor2 = make_executor(
        "task_srv2",
        "task_tool2",
        Some(TaskSupport::Required),
        Arc::clone(&manager2),
        ExecutionMode::FullAutonomous,
        false,
    );

    let params_had_task2 = Arc::new(std::sync::Mutex::new(false));
    let params_flag2 = Arc::clone(&params_had_task2);
    let inbound2 = inbound_tx2.clone();
    tokio::spawn(async move {
        let req_str = tokio::time::timeout(Duration::from_secs(5), outbound_rx2.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let req: serde_json::Value = serde_json::from_str(&req_str).unwrap();
        let id = req["id"].as_u64().unwrap_or(1);

        let has_task = req["params"].get("task").is_some();
        *params_flag2.lock().unwrap() = has_task;

        let resp = call_tool_as_task_response_json(id, "task completed");
        inbound2
            .send(serde_json::to_string(&resp).unwrap())
            .unwrap();
    });

    let err2 = executor2
        .execute(serde_json::json!({"input": "run_task"}))
        .await
        .expect_err("execute must return Err when taskId is present");
    assert!(
        err2.to_string().contains("task-abc-123"),
        "second McpTask error must also identify the task ID"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        *params_had_task2.lock().unwrap(),
        "second call_tool_as_task must also include a 'task' field in outbound params"
    );
}

/// When `task_support` is `None`, `execute` uses the plain `call_tool` path
/// and the params do NOT contain a `"task"` field.
#[tokio::test]
async fn test_no_task_support_does_not_include_task_field_in_params() {
    let tool = make_tool("plain_tool", None);
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "plain_srv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let executor = make_executor(
        "plain_srv",
        "plain_tool",
        None,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    );

    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        let req_str = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let req: serde_json::Value = serde_json::from_str(&req_str).unwrap();
        let id = req["id"].as_u64().unwrap_or(1);

        // Plain call_tool must NOT include a "task" field.
        let params = &req["params"];
        assert!(
            params.get("task").is_none(),
            "plain call_tool must NOT include a 'task' field, got params: {}",
            params
        );

        let resp = call_tool_response_json(id, "plain result", false, None);
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    let result = executor
        .execute(serde_json::json!({}))
        .await
        .expect("execute must not return Err");

    assert!(result.success);
}

// ---------------------------------------------------------------------------
// Task 5A.3: register_mcp_tools
// ---------------------------------------------------------------------------

/// `register_mcp_tools` with no connected servers registers zero tool
/// executors but always registers `mcp_read_resource` and `mcp_get_prompt`.
#[tokio::test]
async fn test_register_mcp_tools_with_no_servers_registers_zero_mcp_tools() {
    let manager = Arc::new(RwLock::new(make_manager()));
    let mut registry = ToolRegistry::new();

    let count = register_mcp_tools(
        &mut registry,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    )
    .await
    .expect("register_mcp_tools must not fail");

    assert_eq!(
        count, 0,
        "zero tool executors when no servers are connected"
    );
    assert!(
        registry.get("mcp_read_resource").is_some(),
        "mcp_read_resource must always be registered"
    );
    assert!(
        registry.get("mcp_get_prompt").is_some(),
        "mcp_get_prompt must always be registered"
    );
}

/// `register_mcp_tools` registers one executor per tool and returns the count.
#[tokio::test]
async fn test_register_mcp_tools_registers_one_executor_per_tool() {
    let tool_a = make_tool("alpha", None);
    let tool_b = make_tool("beta", None);

    let (protocol, _out_rx, _in_tx) = wired_protocol();
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "my_server", vec![tool_a, tool_b], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let mut registry = ToolRegistry::new();
    let count = register_mcp_tools(
        &mut registry,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    )
    .await
    .expect("register_mcp_tools must not fail");

    assert_eq!(count, 2, "should register exactly 2 tool executors");
    assert!(
        registry.get("my_server__alpha").is_some(),
        "my_server__alpha must be in registry"
    );
    assert!(
        registry.get("my_server__beta").is_some(),
        "my_server__beta must be in registry"
    );
}

/// The registered executor's `tool_definition()` carries the double-underscore
/// namespaced name.
#[tokio::test]
async fn test_register_mcp_tools_executor_definition_uses_namespaced_name() {
    let tool = make_tool("my_tool", None);

    let (protocol, _out_rx, _in_tx) = wired_protocol();
    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "my_srv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    let mut registry = ToolRegistry::new();
    register_mcp_tools(
        &mut registry,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    )
    .await
    .expect("register_mcp_tools must not fail");

    let executor = registry
        .get("my_srv__my_tool")
        .expect("my_srv__my_tool must be registered");

    let def = executor.tool_definition();
    assert_eq!(
        def["name"].as_str().unwrap(),
        "my_srv__my_tool",
        "definition name must be the double-underscore namespaced name"
    );
    assert!(def.get("description").is_some());
    assert!(def.get("parameters").is_some());
}

/// Tools from multiple servers are each registered under the correct namespaced
/// name.
#[tokio::test]
async fn test_register_mcp_tools_multiple_servers_all_registered() {
    let (proto_a, _out_a, _in_a) = wired_protocol();
    let (proto_b, _out_b, _in_b) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(
        &mut manager,
        "server_one",
        vec![make_tool("toolx", None)],
        proto_a,
    );
    insert_connected_entry(
        &mut manager,
        "server_two",
        vec![make_tool("tooly", None)],
        proto_b,
    );
    let manager = Arc::new(RwLock::new(manager));

    let mut registry = ToolRegistry::new();
    let count = register_mcp_tools(
        &mut registry,
        Arc::clone(&manager),
        ExecutionMode::FullAutonomous,
        false,
    )
    .await
    .expect("register_mcp_tools must not fail");

    assert_eq!(count, 2);
    assert!(registry.get("server_one__toolx").is_some());
    assert!(registry.get("server_two__tooly").is_some());
}

// ---------------------------------------------------------------------------
// Headless auto-approve: execute skips prompt when headless=true
// ---------------------------------------------------------------------------

/// When `headless: true`, `execute` must not block on stdin even for
/// `Interactive` mode.  This test verifies the call completes without hanging.
#[tokio::test]
async fn test_execute_headless_interactive_does_not_prompt() {
    let tool = make_tool("headless_tool", None);
    let (protocol, mut outbound_rx, inbound_tx) = wired_protocol();

    let mut manager = make_manager();
    insert_connected_entry(&mut manager, "hsrv", vec![tool], protocol);
    let manager = Arc::new(RwLock::new(manager));

    // headless=true, mode=Interactive -- must auto-approve.
    let executor = make_executor(
        "hsrv",
        "headless_tool",
        None,
        Arc::clone(&manager),
        ExecutionMode::Interactive,
        true, // headless
    );

    let inbound = inbound_tx.clone();
    tokio::spawn(async move {
        let _req = tokio::time::timeout(Duration::from_secs(5), outbound_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let resp = call_tool_response_json(1, "headless result", false, None);
        inbound.send(serde_json::to_string(&resp).unwrap()).unwrap();
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        executor.execute(serde_json::json!({})),
    )
    .await
    .expect("execute timed out -- headless interactive path must not block on stdin")
    .expect("execute must not return Err");

    assert!(result.success);
    assert!(result.output.contains("headless result"));
}
