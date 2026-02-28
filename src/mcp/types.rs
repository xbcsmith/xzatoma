//! MCP 2025-11-25 protocol types and JSON-RPC 2.0 primitives
//!
//! This module defines every wire type used by the Model Context Protocol
//! (revision **2025-11-25**) with **2025-03-26** as a backwards-compatibility
//! fallback. All types derive `Debug`, `Clone`, `Serialize`, and `Deserialize`
//! unless noted otherwise. Struct fields are `camelCase` on the wire via
//! `#[serde(rename_all = "camelCase")]` unless the field is already camelCase
//! or a `_meta` override is required. All `Option<>` fields omit their key
//! from JSON when `None` via `#[serde(skip_serializing_if = "Option::is_none")]`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Protocol version constants
// ---------------------------------------------------------------------------

/// The most recent supported MCP protocol revision.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// Earlier protocol revision retained for backwards compatibility.
pub const PROTOCOL_VERSION_2025_03_26: &str = "2025-03-26";

/// All protocol versions that this client accepts during negotiation.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &[LATEST_PROTOCOL_VERSION, PROTOCOL_VERSION_2025_03_26];

// ---------------------------------------------------------------------------
// JSON-RPC method constants
// ---------------------------------------------------------------------------

/// Lifecycle: client sends `initialize` to open a session.
pub const METHOD_INITIALIZE: &str = "initialize";
/// Lifecycle: client sends `notifications/initialized` after the server ACKs.
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
/// Keepalive ping.
pub const METHOD_PING: &str = "ping";
/// Request a page of available tools.
pub const METHOD_TOOLS_LIST: &str = "tools/list";
/// Invoke a named tool.
pub const METHOD_TOOLS_CALL: &str = "tools/call";
/// Request a page of available resources.
pub const METHOD_RESOURCES_LIST: &str = "resources/list";
/// Read the contents of a resource by URI.
pub const METHOD_RESOURCES_READ: &str = "resources/read";
/// Subscribe to live updates for a resource URI.
pub const METHOD_RESOURCES_SUBSCRIBE: &str = "resources/subscribe";
/// Unsubscribe from a resource URI.
pub const METHOD_RESOURCES_UNSUBSCRIBE: &str = "resources/unsubscribe";
/// List URI templates for parameterized resources.
pub const METHOD_RESOURCES_TEMPLATES_LIST: &str = "resources/templates/list";
/// Request a page of available prompts.
pub const METHOD_PROMPTS_LIST: &str = "prompts/list";
/// Retrieve a rendered prompt by name.
pub const METHOD_PROMPTS_GET: &str = "prompts/get";
/// Request argument completions for a prompt or resource template.
pub const METHOD_COMPLETION_COMPLETE: &str = "completion/complete";
/// Set the server-side logging verbosity level.
pub const METHOD_LOGGING_SET_LEVEL: &str = "logging/setLevel";
/// Server-initiated: ask the client to generate a completion sample.
pub const METHOD_SAMPLING_CREATE_MESSAGE: &str = "sampling/createMessage";
/// Server-initiated: ask the client to collect structured user input.
pub const METHOD_ELICITATION_CREATE: &str = "elicitation/create";
/// Retrieve the current state of a long-running task.
pub const METHOD_TASKS_GET: &str = "tasks/get";
/// Retrieve the final result of a completed task.
pub const METHOD_TASKS_RESULT: &str = "tasks/result";
/// Request cancellation of a running task.
pub const METHOD_TASKS_CANCEL: &str = "tasks/cancel";
/// List all tasks known to the server.
pub const METHOD_TASKS_LIST: &str = "tasks/list";

// ---------------------------------------------------------------------------
// Notification constants
// ---------------------------------------------------------------------------

/// Server notifies that the tool list has changed.
pub const NOTIF_TOOLS_LIST_CHANGED: &str = "notifications/tools/listChanged";
/// Server notifies that the resource list has changed.
pub const NOTIF_RESOURCES_LIST_CHANGED: &str = "notifications/resources/listChanged";
/// Server notifies that a subscribed resource's content has been updated.
pub const NOTIF_RESOURCES_UPDATED: &str = "notifications/resources/updated";
/// Server notifies that the prompt list has changed.
pub const NOTIF_PROMPTS_LIST_CHANGED: &str = "notifications/prompts/listChanged";
/// Server notifies that a task's status has changed.
pub const NOTIF_TASKS_STATUS: &str = "notifications/tasks/status";
/// Server or client reports progress on a long-running operation.
pub const NOTIF_PROGRESS: &str = "notifications/progress";
/// Either side signals that a prior request has been cancelled.
pub const NOTIF_CANCELLED: &str = "notifications/cancelled";
/// Client notifies that its root list has changed.
pub const NOTIF_ROOTS_LIST_CHANGED: &str = "notifications/roots/listChanged";

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 wire types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request object.
///
/// `jsonrpc` MUST always be `"2.0"`. `id` is `None` only for notifications
/// (use [`JsonRpcNotification`] instead for clarity).
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::JsonRpcRequest;
///
/// let req = JsonRpcRequest {
///     jsonrpc: "2.0".to_string(),
///     id: Some(serde_json::json!(1)),
///     method: "ping".to_string(),
///     params: None,
/// };
/// assert_eq!(req.jsonrpc, "2.0");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version identifier; always `"2.0"`.
    pub jsonrpc: String,
    /// Request correlation identifier. Present for requests, absent for notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// The method name to invoke.
    pub method: String,
    /// Optional method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response object.
///
/// Exactly one of `result` or `error` will be present in a valid response.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::JsonRpcResponse;
///
/// let resp = JsonRpcResponse {
///     jsonrpc: "2.0".to_string(),
///     id: Some(serde_json::json!(1)),
///     result: Some(serde_json::json!({})),
///     error: None,
/// };
/// assert!(resp.result.is_some());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Protocol version identifier; always `"2.0"`.
    pub jsonrpc: String,
    /// Mirrors the `id` from the corresponding request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// Successful result value; mutually exclusive with `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error object; mutually exclusive with `result`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
///
/// Implements `Display` as `"JSON-RPC error {code}: {message}"`.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::JsonRpcError;
///
/// let e = JsonRpcError { code: -32600, message: "Invalid Request".to_string(), data: None };
/// assert_eq!(e.to_string(), "JSON-RPC error -32600: Invalid Request");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code as defined by JSON-RPC 2.0 or the MCP spec.
    pub code: i64,
    /// Human-readable error description.
    pub message: String,
    /// Optional additional error context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

/// A JSON-RPC 2.0 notification (a request with no `id`).
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::JsonRpcNotification;
///
/// let n = JsonRpcNotification {
///     jsonrpc: "2.0".to_string(),
///     method: "notifications/initialized".to_string(),
///     params: None,
/// };
/// assert_eq!(n.method, "notifications/initialized");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// Protocol version identifier; always `"2.0"`.
    pub jsonrpc: String,
    /// The notification method name.
    pub method: String,
    /// Optional notification parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Core identity types
// ---------------------------------------------------------------------------

/// A newtype wrapper around a protocol version string.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::ProtocolVersion;
///
/// let v = ProtocolVersion::from("2025-11-25");
/// assert_eq!(v.to_string(), "2025-11-25");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolVersion(pub String);

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ProtocolVersion {
    fn from(s: String) -> Self {
        ProtocolVersion(s)
    }
}

impl From<&str> for ProtocolVersion {
    fn from(s: &str) -> Self {
        ProtocolVersion(s.to_string())
    }
}

/// Identifies a client or server implementation by name and version.
///
/// The `description` field was added in protocol revision `2025-11-25`.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::Implementation;
///
/// let info = Implementation {
///     name: "xzatoma".to_string(),
///     version: "0.2.0".to_string(),
///     description: None,
/// };
/// let json = serde_json::to_string(&info).unwrap();
/// assert!(!json.contains("description"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    /// Short name of the implementation (e.g. `"xzatoma"`).
    pub name: String,
    /// Semantic version string (e.g. `"0.2.0"`).
    pub version: String,
    /// Optional human-readable description (new in `2025-11-25`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Capability types
// ---------------------------------------------------------------------------

/// Advertises the task-management capabilities of the remote party.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksCapability {
    /// Capability descriptor for `tasks/list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<serde_json::Value>,
    /// Capability descriptor for `tasks/cancel`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<serde_json::Value>,
    /// Capability descriptor for task request notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests: Option<serde_json::Value>,
}

/// Advertises the elicitation capabilities of the client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationCapability {
    /// Descriptor for form-based elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<serde_json::Value>,
    /// Descriptor for URL-redirect elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<serde_json::Value>,
}

/// Advertises the sampling capabilities of the client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingCapability {
    /// Descriptor for tool-call sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    /// Descriptor for context-window sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Advertises whether the client supports dynamic root-list change notifications.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    /// When `true`, the client sends `notifications/roots/listChanged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// The full set of capabilities that a client advertises to a server.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::ClientCapabilities;
///
/// let caps = ClientCapabilities::default();
/// let json = serde_json::to_value(&caps).unwrap();
/// assert_eq!(json, serde_json::json!({}));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// Experimental capability extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<serde_json::Value>,
    /// LLM sampling capability (client can handle `sampling/createMessage`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
    /// Filesystem root capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// Structured elicitation capability (client can handle `elicitation/create`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,
    /// Long-running task capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TasksCapability>,
}

/// The full set of capabilities that a server advertises to a client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Experimental capability extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<serde_json::Value>,
    /// Server supports `logging/setLevel` and log notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<serde_json::Value>,
    /// Server supports `completion/complete`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<serde_json::Value>,
    /// Server exposes prompts via `prompts/list` and `prompts/get`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<serde_json::Value>,
    /// Server exposes resources via `resources/list` and `resources/read`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
    /// Server exposes tools via `tools/list` and `tools/call`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    /// Server supports long-running tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Initialize types
// ---------------------------------------------------------------------------

/// Parameters sent by the client in the `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The protocol version the client wishes to use.
    pub protocol_version: String,
    /// Capabilities advertised by this client.
    pub capabilities: ClientCapabilities,
    /// Information identifying this client implementation.
    pub client_info: Implementation,
}

/// Response returned by the server to an `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    /// The protocol version the server has selected for this session.
    pub protocol_version: String,
    /// Capabilities advertised by this server.
    pub capabilities: ServerCapabilities,
    /// Information identifying this server implementation.
    pub server_info: Implementation,
    /// Optional human-readable instructions for the client (new in `2025-11-25`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool types
// ---------------------------------------------------------------------------

/// Whether a tool requires, optionally supports, or forbids task wrapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskSupport {
    /// Tool execution MUST create a task.
    Required,
    /// Tool execution MAY create a task.
    Optional,
    /// Tool execution MUST NOT create a task.
    Forbidden,
}

/// Execution metadata associated with a tool definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecution {
    /// Describes whether and how tasks are used for this tool's execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_support: Option<TaskSupport>,
}

/// Behavioral hints for tool display and safety classification.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Display title for UI presentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// When `true`, the tool only reads state and never mutates it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// When `true`, the tool may make irreversible changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// When `true`, calling the tool multiple times with the same arguments
    /// has the same effect as calling it once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    /// When `true`, the tool may interact with the world beyond the MCP server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// A tool exposed by an MCP server.
///
/// Named `McpTool` to avoid a naming collision with `crate::tools::Tool`.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::McpTool;
///
/// let tool = McpTool {
///     name: "search".to_string(),
///     title: None,
///     description: Some("Search the web".to_string()),
///     input_schema: serde_json::json!({ "type": "object" }),
///     output_schema: None,
///     annotations: None,
///     execution: None,
/// };
/// assert_eq!(tool.name, "search");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    /// Unique name of the tool within the server.
    pub name: String,
    /// Optional display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description of the tool's purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,
    /// JSON Schema describing the tool's output (new in `2025-11-25`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Behavioral hints for display and safety classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// Task-wrapping metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolExecution>,
}

/// Response to a `tools/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResponse {
    /// Tools in this page of results.
    pub tools: Vec<McpTool>,
    /// Opaque cursor for the next page; `None` means this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Optional task parameters attached to a `tools/call` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskParams {
    /// Time-to-live for the created task in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
}

/// Parameters for a `tools/call` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolParams {
    /// Name of the tool to invoke.
    pub name: String,
    /// Arguments to pass to the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    /// Optional task-wrapping parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskParams>,
}

/// Response from a `tools/call` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResponse {
    /// The content items produced by the tool.
    pub content: Vec<ToolResponseContent>,
    /// When `true`, the tool signalled an error condition within its content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    /// Structured output matching the tool's `outputSchema` (new in `2025-11-25`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
}

/// A single content item in a tool response.
///
/// Discriminated by the `"type"` field on the wire.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::ToolResponseContent;
///
/// let c = ToolResponseContent::Text { text: "hello".to_string() };
/// let json = serde_json::to_value(&c).unwrap();
/// assert_eq!(json["type"], "text");
/// assert_eq!(json["text"], "hello");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolResponseContent {
    /// Plain text output.
    Text {
        /// The text content.
        text: String,
    },
    /// A base64-encoded image.
    Image {
        /// Base64-encoded image bytes.
        data: String,
        /// MIME type of the image (e.g. `"image/png"`).
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// A base64-encoded audio clip.
    Audio {
        /// Base64-encoded audio bytes.
        data: String,
        /// MIME type of the audio (e.g. `"audio/wav"`).
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    /// An embedded resource.
    Resource {
        /// The resource contents.
        resource: ResourceContents,
    },
}

// ---------------------------------------------------------------------------
// Task types
// ---------------------------------------------------------------------------

/// Lifecycle state of a long-running MCP task.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::TaskStatus;
///
/// let s = serde_json::to_string(&TaskStatus::InputRequired).unwrap();
/// assert_eq!(s, "\"input_required\"");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// The task is actively processing.
    Working,
    /// The task is paused waiting for user input.
    InputRequired,
    /// The task finished successfully.
    Completed,
    /// The task terminated with an error.
    Failed,
    /// The task was cancelled before completion.
    Cancelled,
}

/// A long-running task object as returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique identifier for this task.
    pub task_id: String,
    /// Current lifecycle state.
    pub status: TaskStatus,
    /// Optional human-readable status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// ISO 8601 creation timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// ISO 8601 timestamp of the most recent status change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_at: Option<String>,
    /// Time-to-live in seconds before the server may discard the task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
    /// Suggested polling interval in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
}

/// Result returned when a tool call creates a new task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskResult {
    /// The newly created task.
    pub task: Task,
}

/// Response to a `tasks/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksListResponse {
    /// Tasks in this page.
    pub tasks: Vec<Task>,
    /// Opaque cursor for the next page; `None` means this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for `tasks/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksGetParams {
    /// Identifier of the task to retrieve.
    pub task_id: String,
}

/// Parameters for `tasks/result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksResultParams {
    /// Identifier of the task whose result to retrieve.
    pub task_id: String,
}

/// Parameters for `tasks/cancel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksCancelParams {
    /// Identifier of the task to cancel.
    pub task_id: String,
}

/// Parameters for `tasks/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksListParams {
    /// Opaque cursor from a previous `tasks/list` response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Resource types
// ---------------------------------------------------------------------------

/// Text-based resource contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    /// Canonical URI that identifies this resource.
    pub uri: String,
    /// MIME type of the text (e.g. `"text/plain"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// The text content of the resource.
    pub text: String,
}

/// Binary (blob) resource contents, base64-encoded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    /// Canonical URI that identifies this resource.
    pub uri: String,
    /// MIME type of the binary data (e.g. `"application/octet-stream"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Base64-encoded binary data.
    pub blob: String,
}

/// Either text or binary resource contents.
///
/// Uses `#[serde(untagged)]` so the discriminator is presence of `"text"` vs
/// `"blob"` in the JSON object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ResourceContents {
    /// UTF-8 text resource.
    Text(TextResourceContents),
    /// Binary resource (base64-encoded blob).
    Blob(BlobResourceContents),
}

/// Metadata describing a resource exposed by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// Canonical URI for this resource.
    pub uri: String,
    /// Human-readable resource name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// A URI template for parameterized resource access.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplate {
    /// RFC 6570 URI template string.
    pub uri_template: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of resources matched by this template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Response to a `resources/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResponse {
    /// Resources in this page.
    pub resources: Vec<Resource>,
    /// Opaque cursor for the next page; `None` means this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for `resources/read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceParams {
    /// URI of the resource to read.
    pub uri: String,
}

/// Response to a `resources/read` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResponse {
    /// One or more content objects representing the resource's current state.
    pub contents: Vec<ResourceContents>,
}

// ---------------------------------------------------------------------------
// Prompt types
// ---------------------------------------------------------------------------

/// Conversation participant role.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::types::Role;
///
/// let r: Role = serde_json::from_str("\"user\"").unwrap();
/// assert_eq!(r, Role::User);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// A message from the human user.
    User,
    /// A message from the AI assistant.
    Assistant,
}

/// Inline plain-text content for use in prompt messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    /// The text body.
    pub text: String,
    /// Optional display or citation annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

/// Inline base64-encoded image content for use in prompt messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    /// Base64-encoded image bytes.
    pub data: String,
    /// MIME type of the image.
    pub mime_type: String,
    /// Optional display or citation annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

/// Inline base64-encoded audio content for use in prompt messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioContent {
    /// Base64-encoded audio bytes.
    pub data: String,
    /// MIME type of the audio.
    pub mime_type: String,
    /// Optional display or citation annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<serde_json::Value>,
}

/// Content within a prompt message, discriminated by `"type"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MessageContent {
    /// Plain text.
    Text(TextContent),
    /// Image data.
    Image(ImageContent),
    /// Audio data.
    Audio(AudioContent),
    /// An embedded resource.
    Resource {
        /// The embedded resource contents.
        resource: ResourceContents,
        /// Optional annotations.
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<serde_json::Value>,
    },
}

/// A single message in a prompt conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    /// Who authored this message.
    pub role: Role,
    /// The message body.
    pub content: MessageContent,
}

/// Describes a single argument accepted by a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// Argument name (used as a key when calling `prompts/get`).
    pub name: String,
    /// Human-readable description of what this argument controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When `true`, this argument must be supplied by the caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Metadata describing a prompt template exposed by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Unique name of this prompt.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Argument descriptors for this template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Response to a `prompts/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResponse {
    /// Prompts in this page.
    pub prompts: Vec<Prompt>,
    /// Opaque cursor for the next page; `None` means this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for `prompts/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptParams {
    /// Name of the prompt to retrieve.
    pub name: String,
    /// Template argument substitutions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, String>>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Response to a `prompts/get` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResponse {
    /// Human-readable description of what this prompt does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The rendered prompt messages ready to send to an LLM.
    pub messages: Vec<PromptMessage>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Sampling types
// ---------------------------------------------------------------------------

/// A hint suggesting which model to prefer for a sampling request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHint {
    /// Model name or prefix to prefer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Weighted preferences for model selection in a sampling request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    /// Ordered list of model hints from most to least preferred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,
    /// Priority weight for minimizing cost (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
    /// Priority weight for minimizing latency (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,
    /// Priority weight for maximizing quality (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,
}

/// How the model should decide whether to call a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    /// Model decides automatically.
    Auto,
    /// Model must call at least one tool.
    Required,
    /// Model must not call any tools.
    #[serde(rename = "none")]
    None_,
}

/// Tool-choice constraint for a sampling request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingToolChoice {
    /// The tool-choice mode to enforce.
    pub mode: ToolChoiceMode,
}

/// Server-initiated request asking the client to generate a completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageRequest {
    /// The conversation history to complete.
    pub messages: Vec<PromptMessage>,
    /// Model selection preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
    /// Optional system prompt to prepend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// What conversational context to include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_context: Option<String>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Optional stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Provider-specific metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Tool definitions available to the sampler.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Tool-choice constraint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<SamplingToolChoice>,
}

/// The client's response to a `sampling/createMessage` server request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageResult {
    /// Role of the generated message; always `Role::Assistant`.
    pub role: Role,
    /// The generated content.
    pub content: MessageContent,
    /// The model that produced this result.
    pub model: String,
    /// Why generation stopped (e.g. `"end_turn"`, `"max_tokens"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Elicitation types
// ---------------------------------------------------------------------------

/// The interaction modality for an elicitation request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationMode {
    /// Render a structured form for the user to fill in.
    Form,
    /// Redirect the user to a URL.
    Url,
}

/// How the user responded to an elicitation request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationAction {
    /// User accepted and submitted the elicitation.
    Accept,
    /// User explicitly declined.
    Decline,
    /// User dismissed without completing.
    Cancel,
}

/// Server-initiated request asking the client to collect structured user input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationCreateParams {
    /// Interaction modality.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ElicitationMode>,
    /// Human-readable message displayed to the user.
    pub message: String,
    /// JSON Schema describing the expected response structure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_schema: Option<serde_json::Value>,
    /// URL for redirect-based elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Correlation ID provided by the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation_id: Option<String>,
}

/// The client's response to an `elicitation/create` server request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResult {
    /// How the user responded.
    pub action: ElicitationAction,
    /// The collected content, if the user accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Logging types
// ---------------------------------------------------------------------------

/// Syslog-inspired severity levels for MCP log messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    /// Verbose diagnostic information.
    Debug,
    /// General operational information.
    Info,
    /// Normal but significant events.
    Notice,
    /// Potential problems that don't prevent operation.
    Warning,
    /// Error conditions that affect a specific operation.
    Error,
    /// Severe conditions that affect broad functionality.
    Critical,
    /// Immediate action required.
    Alert,
    /// System is unusable.
    Emergency,
}

// ---------------------------------------------------------------------------
// Completion types
// ---------------------------------------------------------------------------

/// Parameters for `completion/complete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionCompleteParams {
    /// Reference to the prompt or resource template being completed.
    #[serde(rename = "ref")]
    pub r#ref: serde_json::Value,
    /// The argument whose value is being completed.
    pub argument: serde_json::Value,
}

/// A single completion result object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResult {
    /// The suggested completion strings.
    pub values: Vec<String>,
    /// Total number of completions available server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    /// Whether more pages of completions are available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

/// Response to a `completion/complete` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionCompleteResponse {
    /// The completion result.
    pub completion: CompletionResult,
}

// ---------------------------------------------------------------------------
// Other utility types
// ---------------------------------------------------------------------------

/// A filesystem root declared by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    /// URI of the root (e.g. `"file:///home/user/project"`).
    pub uri: String,
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Parameters for the `notifications/cancelled` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledParams {
    /// The `id` of the request being cancelled.
    pub request_id: serde_json::Value,
    /// Human-readable reason for cancellation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Parameters for the `notifications/progress` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressParams {
    /// Opaque token identifying the long-running operation.
    pub progress_token: serde_json::Value,
    /// How much work has been completed so far.
    pub progress: f64,
    /// Optional status message to display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Total amount of work, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Optional extension metadata.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Generic paginated request parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedParams {
    /// Opaque cursor from a previous paged response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_constants_are_correct() {
        assert_eq!(LATEST_PROTOCOL_VERSION, "2025-11-25");
        assert_eq!(PROTOCOL_VERSION_2025_03_26, "2025-03-26");
        assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-11-25"));
        assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-03-26"));
    }

    #[test]
    fn test_implementation_description_skipped_when_none() {
        let info = Implementation {
            name: "xzatoma".to_string(),
            version: "0.2.0".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("description"));
        assert!(json.contains("xzatoma"));
    }

    #[test]
    fn test_implementation_roundtrip_with_description() {
        let info = Implementation {
            name: "xzatoma".to_string(),
            version: "0.2.0".to_string(),
            description: Some("Autonomous AI agent".to_string()),
        };
        let val = serde_json::to_value(&info).unwrap();
        let back: Implementation = serde_json::from_value(val).unwrap();
        assert_eq!(back.description.as_deref(), Some("Autonomous AI agent"));
    }

    #[test]
    fn test_call_tool_response_roundtrip() {
        let resp = CallToolResponse {
            content: vec![ToolResponseContent::Text {
                text: "result".to_string(),
            }],
            is_error: Some(false),
            meta: Some(serde_json::json!({ "trace": "abc" })),
            structured_content: Some(serde_json::json!({ "value": 42 })),
        };
        let val = serde_json::to_value(&resp).unwrap();
        let back: CallToolResponse = serde_json::from_value(val).unwrap();
        assert_eq!(back.is_error, Some(false));
        assert!(back.structured_content.is_some());
        assert_eq!(back.content.len(), 1);
    }

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
        let back: ToolResponseContent = serde_json::from_value(val).unwrap();
        assert_eq!(back, c);
    }

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
    }

    #[test]
    fn test_json_rpc_request_omits_id_when_none() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let val = serde_json::to_value(&req).unwrap();
        assert!(val.get("id").is_none() || val["id"].is_null());
    }

    #[test]
    fn test_protocol_version_newtype_display() {
        let v = ProtocolVersion::from("2025-11-25");
        assert_eq!(v.to_string(), "2025-11-25");
    }

    #[test]
    fn test_protocol_version_eq() {
        let a = ProtocolVersion::from("2025-11-25");
        let b = ProtocolVersion("2025-11-25".to_string());
        assert_eq!(a, b);
    }

    #[test]
    fn test_role_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
    }

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

    #[test]
    fn test_tool_choice_mode_none_serializes_as_none_string() {
        let mode = ToolChoiceMode::None_;
        let s = serde_json::to_string(&mode).unwrap();
        assert_eq!(s, "\"none\"");
    }

    #[test]
    fn test_client_capabilities_empty_is_empty_json_object() {
        let caps = ClientCapabilities::default();
        let val = serde_json::to_value(&caps).unwrap();
        assert_eq!(val, serde_json::json!({}));
    }

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
    }

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

    #[test]
    fn test_logging_level_ordering() {
        assert!(LoggingLevel::Debug < LoggingLevel::Info);
        assert!(LoggingLevel::Info < LoggingLevel::Warning);
        assert!(LoggingLevel::Warning < LoggingLevel::Error);
        assert!(LoggingLevel::Error < LoggingLevel::Critical);
        assert!(LoggingLevel::Critical < LoggingLevel::Emergency);
    }

    #[test]
    fn test_paginated_params_cursor_skipped_when_none() {
        let p = PaginatedParams { cursor: None };
        let val = serde_json::to_value(&p).unwrap();
        assert_eq!(val, serde_json::json!({}));
    }

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
        let back: InitializeParams = serde_json::from_value(val).unwrap();
        assert_eq!(back.protocol_version, LATEST_PROTOCOL_VERSION);
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

    #[test]
    fn test_call_tool_params_meta_serialized_as_underscore_meta() {
        let p = CallToolParams {
            name: "search".to_string(),
            arguments: None,
            meta: Some(serde_json::json!({ "x": 1 })),
            task: None,
        };
        let val = serde_json::to_value(&p).unwrap();
        assert!(val.get("_meta").is_some());
        assert!(val.get("meta").is_none());
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
}
