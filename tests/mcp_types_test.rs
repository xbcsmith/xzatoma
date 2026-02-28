//! MCP types unit tests (Phase 1)
//!
//! Validates all MCP 2025-11-25 protocol type round-trips, serialization
//! invariants, and JSON-RPC primitive behaviour.

use xzatoma::mcp::types::{
    BlobResourceContents, CallToolParams, CallToolResponse, ClientCapabilities, ElicitationAction,
    Implementation, InitializeParams, InitializeResponse, JsonRpcError, JsonRpcNotification,
    JsonRpcRequest, JsonRpcResponse, LoggingLevel, McpTool, MessageContent, PaginatedParams,
    ProgressParams, PromptMessage, ProtocolVersion, ResourceContents, Role, ServerCapabilities,
    Task, TaskStatus, TaskSupport, TasksListResponse, TextContent, TextResourceContents,
    ToolAnnotations, ToolChoiceMode, ToolExecution, ToolResponseContent, LATEST_PROTOCOL_VERSION,
    NOTIF_TOOLS_LIST_CHANGED, PROTOCOL_VERSION_2025_03_26, SUPPORTED_PROTOCOL_VERSIONS,
};

// ---------------------------------------------------------------------------
// Protocol version constants
// ---------------------------------------------------------------------------

#[test]
fn test_protocol_version_constants_are_correct() {
    assert_eq!(LATEST_PROTOCOL_VERSION, "2025-11-25");
    assert_eq!(PROTOCOL_VERSION_2025_03_26, "2025-03-26");
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-11-25"));
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-03-26"));
    assert_eq!(SUPPORTED_PROTOCOL_VERSIONS.len(), 2);
}

// ---------------------------------------------------------------------------
// Implementation type
// ---------------------------------------------------------------------------

#[test]
fn test_implementation_description_skipped_when_none() {
    let info = Implementation {
        name: "xzatoma".to_string(),
        version: "0.2.0".to_string(),
        description: None,
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(!json.contains("description"), "got: {json}");
    assert!(json.contains("xzatoma"));
    assert!(json.contains("0.2.0"));
}

#[test]
fn test_implementation_description_present_when_some() {
    let info = Implementation {
        name: "xzatoma".to_string(),
        version: "0.2.0".to_string(),
        description: Some("Autonomous AI agent".to_string()),
    };
    let val = serde_json::to_value(&info).unwrap();
    let back: Implementation = serde_json::from_value(val).unwrap();
    assert_eq!(back.description.as_deref(), Some("Autonomous AI agent"));
}

// ---------------------------------------------------------------------------
// CallToolResponse round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_call_tool_response_roundtrip() {
    let resp = CallToolResponse {
        content: vec![
            ToolResponseContent::Text {
                text: "result".to_string(),
            },
            ToolResponseContent::Image {
                data: "abc123".to_string(),
                mime_type: "image/png".to_string(),
            },
        ],
        is_error: Some(false),
        meta: Some(serde_json::json!({ "trace": "xyz" })),
        structured_content: Some(serde_json::json!({ "value": 42 })),
    };
    let val = serde_json::to_value(&resp).unwrap();
    let back: CallToolResponse = serde_json::from_value(val).unwrap();
    assert_eq!(back.is_error, Some(false));
    assert!(back.structured_content.is_some());
    assert_eq!(back.content.len(), 2);
    assert!(back.meta.is_some());
}

// ---------------------------------------------------------------------------
// TaskStatus serialization
// ---------------------------------------------------------------------------

#[test]
fn test_task_status_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&TaskStatus::InputRequired).unwrap(),
        "\"input_required\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Working).unwrap(),
        "\"working\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Completed).unwrap(),
        "\"completed\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Failed).unwrap(),
        "\"failed\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
        "\"cancelled\""
    );
}

#[test]
fn test_task_status_deserializes_snake_case() {
    let s: TaskStatus = serde_json::from_str("\"input_required\"").unwrap();
    assert_eq!(s, TaskStatus::InputRequired);
    let s: TaskStatus = serde_json::from_str("\"working\"").unwrap();
    assert_eq!(s, TaskStatus::Working);
}

// ---------------------------------------------------------------------------
// ToolResponseContent round-trips
// ---------------------------------------------------------------------------

#[test]
fn test_tool_response_content_text_roundtrip() {
    let c = ToolResponseContent::Text {
        text: "hello".to_string(),
    };
    let val = serde_json::to_value(&c).unwrap();
    assert_eq!(val["type"], "text");
    assert_eq!(val["text"], "hello");
    let back: ToolResponseContent = serde_json::from_value(val).unwrap();
    assert_eq!(back, c);
}

#[test]
fn test_tool_response_content_image_roundtrip() {
    let c = ToolResponseContent::Image {
        data: "base64data".to_string(),
        mime_type: "image/png".to_string(),
    };
    let val = serde_json::to_value(&c).unwrap();
    assert_eq!(val["type"], "image");
    assert_eq!(val["mimeType"], "image/png");
    let back: ToolResponseContent = serde_json::from_value(val).unwrap();
    assert_eq!(back, c);
}

#[test]
fn test_tool_response_content_audio_roundtrip() {
    let c = ToolResponseContent::Audio {
        data: "audiodata".to_string(),
        mime_type: "audio/wav".to_string(),
    };
    let val = serde_json::to_value(&c).unwrap();
    assert_eq!(val["type"], "audio");
    let back: ToolResponseContent = serde_json::from_value(val).unwrap();
    assert_eq!(back, c);
}

// ---------------------------------------------------------------------------
// JsonRpcError Display
// ---------------------------------------------------------------------------

#[test]
fn test_json_rpc_error_display() {
    let e = JsonRpcError {
        code: -32600,
        message: "Invalid Request".to_string(),
        data: None,
    };
    assert_eq!(e.to_string(), "JSON-RPC error -32600: Invalid Request");
}

#[test]
fn test_json_rpc_error_with_data_display() {
    let e = JsonRpcError {
        code: -32601,
        message: "Method not found".to_string(),
        data: Some(serde_json::json!({ "detail": "ping" })),
    };
    assert!(e.to_string().contains("-32601"));
    assert!(e.to_string().contains("Method not found"));
}

// ---------------------------------------------------------------------------
// JSON-RPC wire types
// ---------------------------------------------------------------------------

#[test]
fn test_json_rpc_request_roundtrip() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(42)),
        method: "tools/list".to_string(),
        params: Some(serde_json::json!({ "cursor": null })),
    };
    let val = serde_json::to_value(&req).unwrap();
    assert_eq!(val["jsonrpc"], "2.0");
    assert_eq!(val["id"], 42);
    let back: JsonRpcRequest = serde_json::from_value(val).unwrap();
    assert_eq!(back.method, "tools/list");
    assert_eq!(back.jsonrpc, "2.0");
}

#[test]
fn test_json_rpc_request_id_none_omitted() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: "notifications/initialized".to_string(),
        params: None,
    };
    let val = serde_json::to_value(&req).unwrap();
    // When id is None it should be absent or null; never a meaningful value.
    assert!(val.get("id").map_or(true, |v| v.is_null()));
}

#[test]
fn test_json_rpc_response_roundtrip() {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!(1)),
        result: Some(serde_json::json!({ "ok": true })),
        error: None,
    };
    let val = serde_json::to_value(&resp).unwrap();
    assert_eq!(val["result"]["ok"], true);
    assert!(val.get("error").map_or(true, |v| v.is_null()));
}

#[test]
fn test_json_rpc_notification_roundtrip() {
    let n = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: NOTIF_TOOLS_LIST_CHANGED.to_string(),
        params: None,
    };
    let val = serde_json::to_value(&n).unwrap();
    let back: JsonRpcNotification = serde_json::from_value(val).unwrap();
    assert_eq!(back.method, NOTIF_TOOLS_LIST_CHANGED);
}

// ---------------------------------------------------------------------------
// ProtocolVersion newtype
// ---------------------------------------------------------------------------

#[test]
fn test_protocol_version_display() {
    let v = ProtocolVersion::from("2025-11-25");
    assert_eq!(v.to_string(), "2025-11-25");
}

#[test]
fn test_protocol_version_from_string() {
    let v = ProtocolVersion::from("2025-03-26".to_string());
    assert_eq!(v.0, "2025-03-26");
}

#[test]
fn test_protocol_version_eq() {
    let a = ProtocolVersion::from("2025-11-25");
    let b = ProtocolVersion("2025-11-25".to_string());
    assert_eq!(a, b);
    let c = ProtocolVersion::from("2025-03-26");
    assert_ne!(a, c);
}

// ---------------------------------------------------------------------------
// Capability types
// ---------------------------------------------------------------------------

#[test]
fn test_client_capabilities_empty_is_empty_json_object() {
    let caps = ClientCapabilities::default();
    let val = serde_json::to_value(&caps).unwrap();
    assert_eq!(val, serde_json::json!({}));
}

#[test]
fn test_server_capabilities_empty_is_empty_json_object() {
    let caps = ServerCapabilities::default();
    let val = serde_json::to_value(&caps).unwrap();
    assert_eq!(val, serde_json::json!({}));
}

// ---------------------------------------------------------------------------
// Role serialization
// ---------------------------------------------------------------------------

#[test]
fn test_role_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
    assert_eq!(
        serde_json::to_string(&Role::Assistant).unwrap(),
        "\"assistant\""
    );
}

#[test]
fn test_role_deserializes_from_lowercase() {
    let r: Role = serde_json::from_str("\"user\"").unwrap();
    assert_eq!(r, Role::User);
    let r: Role = serde_json::from_str("\"assistant\"").unwrap();
    assert_eq!(r, Role::Assistant);
}

// ---------------------------------------------------------------------------
// TaskSupport serialization
// ---------------------------------------------------------------------------

#[test]
fn test_task_support_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&TaskSupport::Required).unwrap(),
        "\"required\""
    );
    assert_eq!(
        serde_json::to_string(&TaskSupport::Optional).unwrap(),
        "\"optional\""
    );
    assert_eq!(
        serde_json::to_string(&TaskSupport::Forbidden).unwrap(),
        "\"forbidden\""
    );
}

// ---------------------------------------------------------------------------
// ToolChoiceMode -- None_ serializes as "none"
// ---------------------------------------------------------------------------

#[test]
fn test_tool_choice_mode_none_serializes_as_none_string() {
    assert_eq!(
        serde_json::to_string(&ToolChoiceMode::None_).unwrap(),
        "\"none\""
    );
    assert_eq!(
        serde_json::to_string(&ToolChoiceMode::Auto).unwrap(),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&ToolChoiceMode::Required).unwrap(),
        "\"required\""
    );
}

// ---------------------------------------------------------------------------
// ResourceContents untagged discrimination
// ---------------------------------------------------------------------------

#[test]
fn test_resource_contents_untagged_text() {
    let rc = ResourceContents::Text(TextResourceContents {
        uri: "file:///foo.txt".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: "hello".to_string(),
    });
    let val = serde_json::to_value(&rc).unwrap();
    assert_eq!(val["text"], "hello");
    assert!(val.get("blob").is_none());
    let back: ResourceContents = serde_json::from_value(val).unwrap();
    assert_eq!(back, rc);
}

#[test]
fn test_resource_contents_untagged_blob() {
    let rc = ResourceContents::Blob(BlobResourceContents {
        uri: "file:///foo.bin".to_string(),
        mime_type: None,
        blob: "AAEC".to_string(),
    });
    let val = serde_json::to_value(&rc).unwrap();
    assert_eq!(val["blob"], "AAEC");
    assert!(val.get("text").is_none());
    let back: ResourceContents = serde_json::from_value(val).unwrap();
    assert_eq!(back, rc);
}

// ---------------------------------------------------------------------------
// Elicitation action serialization
// ---------------------------------------------------------------------------

#[test]
fn test_elicitation_action_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&ElicitationAction::Accept).unwrap(),
        "\"accept\""
    );
    assert_eq!(
        serde_json::to_string(&ElicitationAction::Decline).unwrap(),
        "\"decline\""
    );
    assert_eq!(
        serde_json::to_string(&ElicitationAction::Cancel).unwrap(),
        "\"cancel\""
    );
}

// ---------------------------------------------------------------------------
// LoggingLevel ordering
// ---------------------------------------------------------------------------

#[test]
fn test_logging_level_ordering() {
    assert!(LoggingLevel::Debug < LoggingLevel::Info);
    assert!(LoggingLevel::Info < LoggingLevel::Warning);
    assert!(LoggingLevel::Warning < LoggingLevel::Error);
    assert!(LoggingLevel::Error < LoggingLevel::Critical);
    assert!(LoggingLevel::Critical < LoggingLevel::Alert);
    assert!(LoggingLevel::Alert < LoggingLevel::Emergency);
}

// ---------------------------------------------------------------------------
// PaginatedParams cursor skipped when None
// ---------------------------------------------------------------------------

#[test]
fn test_paginated_params_cursor_skipped_when_none() {
    let p = PaginatedParams { cursor: None };
    let val = serde_json::to_value(&p).unwrap();
    assert_eq!(val, serde_json::json!({}));
}

#[test]
fn test_paginated_params_cursor_present_when_some() {
    let p = PaginatedParams {
        cursor: Some("abc".to_string()),
    };
    let val = serde_json::to_value(&p).unwrap();
    assert_eq!(val["cursor"], "abc");
}

// ---------------------------------------------------------------------------
// _meta field serialization
// ---------------------------------------------------------------------------

#[test]
fn test_call_tool_params_meta_serialized_as_underscore_meta() {
    let p = CallToolParams {
        name: "search".to_string(),
        arguments: None,
        meta: Some(serde_json::json!({ "x": 1 })),
        task: None,
    };
    let val = serde_json::to_value(&p).unwrap();
    assert!(val.get("_meta").is_some(), "expected _meta key, got: {val}");
    assert!(val.get("meta").is_none(), "must not have plain 'meta' key");
}

#[test]
fn test_progress_params_meta_serialized_as_underscore_meta() {
    let p = ProgressParams {
        progress_token: serde_json::json!("tok1"),
        progress: 0.5,
        message: None,
        total: Some(1.0),
        meta: Some(serde_json::json!({})),
    };
    let val = serde_json::to_value(&p).unwrap();
    assert!(val.get("_meta").is_some());
}

// ---------------------------------------------------------------------------
// InitializeParams round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_params_roundtrip() {
    let params = InitializeParams {
        protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: None,
        },
    };
    let val = serde_json::to_value(&params).unwrap();
    // camelCase on the wire
    assert!(val.get("protocolVersion").is_some());
    assert!(val.get("clientInfo").is_some());
    let back: InitializeParams = serde_json::from_value(val).unwrap();
    assert_eq!(back.protocol_version, LATEST_PROTOCOL_VERSION);
    assert_eq!(back.client_info.name, "test");
}

// ---------------------------------------------------------------------------
// InitializeResponse round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_response_roundtrip() {
    let resp = InitializeResponse {
        protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
        capabilities: ServerCapabilities {
            tools: Some(serde_json::json!({})),
            ..Default::default()
        },
        server_info: Implementation {
            name: "server".to_string(),
            version: "2.0".to_string(),
            description: None,
        },
        instructions: Some("Use tools wisely.".to_string()),
    };
    let val = serde_json::to_value(&resp).unwrap();
    assert_eq!(val["protocolVersion"], LATEST_PROTOCOL_VERSION);
    assert!(val["capabilities"]["tools"].is_object());
    let back: InitializeResponse = serde_json::from_value(val).unwrap();
    assert_eq!(back.instructions.as_deref(), Some("Use tools wisely."));
}

// ---------------------------------------------------------------------------
// McpTool round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_mcp_tool_roundtrip() {
    let tool = McpTool {
        name: "search".to_string(),
        title: Some("Web Search".to_string()),
        description: Some("Search the web".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "query": { "type": "string" } }
        }),
        output_schema: None,
        annotations: Some(ToolAnnotations {
            title: None,
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: None,
            open_world_hint: Some(true),
        }),
        execution: Some(ToolExecution {
            task_support: Some(TaskSupport::Optional),
        }),
    };
    let val = serde_json::to_value(&tool).unwrap();
    assert_eq!(val["name"], "search");
    assert!(val.get("outputSchema").is_none());
    let back: McpTool = serde_json::from_value(val).unwrap();
    assert_eq!(back.name, "search");
    assert_eq!(
        back.execution.unwrap().task_support,
        Some(TaskSupport::Optional)
    );
}

// ---------------------------------------------------------------------------
// Task round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_task_roundtrip() {
    let task = Task {
        task_id: "task-001".to_string(),
        status: TaskStatus::Working,
        status_message: Some("Processing...".to_string()),
        created_at: Some("2025-11-25T00:00:00Z".to_string()),
        last_updated_at: None,
        ttl: Some(3600),
        poll_interval: Some(500),
    };
    let val = serde_json::to_value(&task).unwrap();
    assert_eq!(val["taskId"], "task-001");
    assert_eq!(val["status"], "working");
    assert!(val.get("lastUpdatedAt").is_none());
    let back: Task = serde_json::from_value(val).unwrap();
    assert_eq!(back.task_id, "task-001");
    assert_eq!(back.ttl, Some(3600));
}

// ---------------------------------------------------------------------------
// TasksListResponse round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_tasks_list_response_roundtrip() {
    let resp = TasksListResponse {
        tasks: vec![Task {
            task_id: "t1".to_string(),
            status: TaskStatus::Completed,
            status_message: None,
            created_at: None,
            last_updated_at: None,
            ttl: None,
            poll_interval: None,
        }],
        next_cursor: None,
    };
    let val = serde_json::to_value(&resp).unwrap();
    assert_eq!(val["tasks"][0]["taskId"], "t1");
    assert!(val.get("nextCursor").is_none());
}

// ---------------------------------------------------------------------------
// PromptMessage round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_prompt_message_text_roundtrip() {
    let msg = PromptMessage {
        role: Role::User,
        content: MessageContent::Text(TextContent {
            text: "Hello".to_string(),
            annotations: None,
        }),
    };
    let val = serde_json::to_value(&msg).unwrap();
    assert_eq!(val["role"], "user");
    assert_eq!(val["content"]["type"], "text");
    assert_eq!(val["content"]["text"], "Hello");
    let back: PromptMessage = serde_json::from_value(val).unwrap();
    if let MessageContent::Text(t) = back.content {
        assert_eq!(t.text, "Hello");
    } else {
        panic!("expected Text content");
    }
}
