//! Typed MCP lifecycle wrapper over [`JsonRpcClient`]
//!
//! This module provides two types that represent the two phases of an MCP
//! client session:
//!
//! - [`McpProtocol`] -- an uninitialized client. Call [`McpProtocol::initialize`]
//!   to perform the JSON-RPC `initialize` / `notifications/initialized`
//!   handshake and receive an [`InitializedMcpProtocol`].
//! - [`InitializedMcpProtocol`] -- a fully negotiated session. All MCP
//!   methods (`tools/list`, `tools/call`, `resources/*`, `prompts/*`,
//!   `tasks/*`, `ping`, `completion/complete`) are available as typed async
//!   methods. Sampling and elicitation server-request handlers can be
//!   registered via [`InitializedMcpProtocol::register_sampling_handler`] and
//!   [`InitializedMcpProtocol::register_elicitation_handler`].
//!
//! # Design
//!
//! All pagination is handled internally: `list_tools`, `list_resources`, and
//! `list_prompts` follow `nextCursor` until the server returns `null`,
//! accumulating results before returning.
//!
//! Neither type owns a transport; callers wire up channels externally and pass
//! the resulting [`JsonRpcClient`] into [`McpProtocol::new`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Result, XzatomaError};
use crate::mcp::client::{BoxFuture, JsonRpcClient};
use crate::mcp::types::{
    CallToolParams, CallToolResponse, ClientCapabilities, CompletionCompleteParams,
    CompletionCompleteResponse, ElicitationCreateParams, ElicitationResult, GetPromptParams,
    GetPromptResponse, Implementation, InitializeParams, InitializeResponse, ListPromptsResponse,
    ListResourcesResponse, ListToolsResponse, McpTool, PaginatedParams, Prompt, ReadResourceParams,
    ReadResourceResponse, Resource, ResourceContents, Task, TasksGetParams, TasksListParams,
    TasksListResponse, TasksResultParams, LATEST_PROTOCOL_VERSION, METHOD_COMPLETION_COMPLETE,
    METHOD_INITIALIZE, METHOD_INITIALIZED, METHOD_PING, METHOD_PROMPTS_GET, METHOD_PROMPTS_LIST,
    METHOD_RESOURCES_LIST, METHOD_RESOURCES_READ, METHOD_SAMPLING_CREATE_MESSAGE,
    METHOD_TASKS_CANCEL, METHOD_TASKS_GET, METHOD_TASKS_LIST, METHOD_TASKS_RESULT,
    METHOD_TOOLS_CALL, METHOD_TOOLS_LIST, SUPPORTED_PROTOCOL_VERSIONS,
};
use crate::mcp::types::{CreateMessageRequest, CreateMessageResult, TaskParams, TasksCancelParams};

// ---------------------------------------------------------------------------
// Capability flag enum
// ---------------------------------------------------------------------------

/// Identifies a specific capability that may be advertised by a server.
///
/// Used with [`InitializedMcpProtocol::capable`] to check whether the
/// negotiated server supports a given feature before issuing requests.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::protocol::ServerCapabilityFlag;
///
/// // Typically obtained from an InitializedMcpProtocol:
/// // let has_tools = session.capable(ServerCapabilityFlag::Tools);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerCapabilityFlag {
    /// Server exposes tools via `tools/list` and `tools/call`.
    Tools,
    /// Server exposes resources via `resources/list` and `resources/read`.
    Resources,
    /// Server exposes prompts via `prompts/list` and `prompts/get`.
    Prompts,
    /// Server supports `logging/setLevel` and log notifications.
    Logging,
    /// Server supports `completion/complete`.
    Completions,
    /// Server supports long-running tasks.
    Tasks,
    /// Server advertises experimental capabilities.
    Experimental,
}

// ---------------------------------------------------------------------------
// Sampling and elicitation handler traits
// ---------------------------------------------------------------------------

/// Callback invoked when the server sends a `sampling/createMessage` request.
///
/// Implementors should use the host LLM to generate a completion and return
/// the result. The future must be `'static` because it is stored inside an
/// `Arc` and called from the read-loop task.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::client::BoxFuture;
/// use xzatoma::mcp::protocol::SamplingHandler;
/// use xzatoma::mcp::types::{CreateMessageRequest, CreateMessageResult, MessageContent, Role, TextContent};
///
/// struct EchoSampler;
///
/// impl SamplingHandler for EchoSampler {
///     fn create_message<'a>(
///         &'a self,
///         _params: CreateMessageRequest,
///     ) -> BoxFuture<'a, xzatoma::error::Result<CreateMessageResult>> {
///         Box::pin(async move {
///             Ok(CreateMessageResult {
///                 role: Role::Assistant,
///                 content: MessageContent::Text(TextContent {
///                     text: "echo".to_string(),
///                     annotations: None,
///                 }),
///                 model: "mock".to_string(),
///                 stop_reason: None,
///             })
///         })
///     }
/// }
/// ```
pub trait SamplingHandler: Send + Sync {
    /// Generate a completion in response to a server-initiated sampling request.
    ///
    /// # Arguments
    ///
    /// * `params` - The sampling parameters sent by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying provider fails or is unavailable.
    fn create_message<'a>(
        &'a self,
        params: CreateMessageRequest,
    ) -> BoxFuture<'a, Result<CreateMessageResult>>;
}

/// Callback invoked when the server sends an `elicitation/create` request.
///
/// Implementors should present the elicitation to the user (via a form,
/// terminal prompt, URL redirect, etc.) and return the user's response.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::client::BoxFuture;
/// use xzatoma::mcp::protocol::ElicitationHandler;
/// use xzatoma::mcp::types::{ElicitationAction, ElicitationCreateParams, ElicitationResult};
///
/// struct AutoDecline;
///
/// impl ElicitationHandler for AutoDecline {
///     fn create_elicitation<'a>(
///         &'a self,
///         _params: ElicitationCreateParams,
///     ) -> BoxFuture<'a, xzatoma::error::Result<ElicitationResult>> {
///         Box::pin(async move {
///             Ok(ElicitationResult {
///                 action: ElicitationAction::Decline,
///                 content: None,
///             })
///         })
///     }
/// }
/// ```
pub trait ElicitationHandler: Send + Sync {
    /// Collect structured user input in response to a server-initiated elicitation.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters sent by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the UI interaction fails or is cancelled programmatically.
    fn create_elicitation<'a>(
        &'a self,
        params: ElicitationCreateParams,
    ) -> BoxFuture<'a, Result<ElicitationResult>>;
}

// ---------------------------------------------------------------------------
// McpProtocol -- uninitialized
// ---------------------------------------------------------------------------

/// An uninitialized MCP client session.
///
/// Wraps a [`JsonRpcClient`] and provides a single method,
/// [`McpProtocol::initialize`], which performs the MCP handshake and returns
/// an [`InitializedMcpProtocol`] ready for use.
///
/// # Examples
///
/// ```no_run
/// use tokio::sync::mpsc;
/// use xzatoma::mcp::client::JsonRpcClient;
/// use xzatoma::mcp::protocol::McpProtocol;
/// use xzatoma::mcp::types::{ClientCapabilities, Implementation};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let (tx, _rx) = mpsc::unbounded_channel::<String>();
///     let client = JsonRpcClient::new(tx);
///     let proto = McpProtocol::new(client);
///
///     // In practice you would also start_read_loop and connect a transport.
///     let _session = proto.initialize(
///         Implementation { name: "xzatoma".into(), version: "0.2.0".into(), description: None },
///         ClientCapabilities::default(),
///     );
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct McpProtocol {
    client: JsonRpcClient,
}

impl McpProtocol {
    /// Create a new uninitialized MCP protocol session.
    ///
    /// # Arguments
    ///
    /// * `client` - A connected (channel-wired) [`JsonRpcClient`]. The caller
    ///   must have already called [`crate::mcp::client::start_read_loop`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    /// use xzatoma::mcp::protocol::McpProtocol;
    ///
    /// let (tx, _rx) = mpsc::unbounded_channel::<String>();
    /// let client = JsonRpcClient::new(tx);
    /// let _proto = McpProtocol::new(client);
    /// ```
    pub fn new(client: JsonRpcClient) -> Self {
        Self { client }
    }

    /// Perform the MCP `initialize` / `notifications/initialized` handshake.
    ///
    /// Sends an `initialize` request with the given client capabilities and
    /// identity, verifies that the server's chosen protocol version is in
    /// [`SUPPORTED_PROTOCOL_VERSIONS`], sends the `notifications/initialized`
    /// notification, and returns an [`InitializedMcpProtocol`].
    ///
    /// # Arguments
    ///
    /// * `client_info` - Name and version of this client implementation.
    /// * `capabilities` - Capabilities this client wishes to advertise.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpProtocolVersion`] if the server returns a
    /// protocol version that is not in [`SUPPORTED_PROTOCOL_VERSIONS`].
    ///
    /// Returns [`XzatomaError::McpTransport`] if the outbound channel is closed.
    ///
    /// Returns [`XzatomaError::McpTimeout`] if the server does not respond in
    /// time (default 30 s).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    /// use xzatoma::mcp::protocol::McpProtocol;
    /// use xzatoma::mcp::types::{ClientCapabilities, Implementation};
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let (tx, _rx) = mpsc::unbounded_channel::<String>();
    ///     let client = JsonRpcClient::new(tx);
    ///     let proto = McpProtocol::new(client);
    ///     // Normally you would start the read loop and inject a fake server
    ///     // response here before calling initialize.
    ///     Ok(())
    /// }
    /// ```
    pub async fn initialize(
        self,
        client_info: Implementation,
        capabilities: ClientCapabilities,
    ) -> Result<InitializedMcpProtocol> {
        let response: InitializeResponse = self
            .client
            .request(
                METHOD_INITIALIZE,
                InitializeParams {
                    protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                    capabilities,
                    client_info,
                },
                None,
            )
            .await?;

        // Verify the server selected a version we support.
        if !SUPPORTED_PROTOCOL_VERSIONS.contains(&response.protocol_version.as_str()) {
            return Err(XzatomaError::McpProtocolVersion {
                expected: SUPPORTED_PROTOCOL_VERSIONS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                got: response.protocol_version,
            }
            .into());
        }

        // Fire-and-forget the initialized notification; errors are not fatal.
        let _ = self
            .client
            .notify(METHOD_INITIALIZED, serde_json::json!({}));

        Ok(InitializedMcpProtocol {
            client: self.client,
            initialize_response: response,
        })
    }
}

// ---------------------------------------------------------------------------
// InitializedMcpProtocol -- fully negotiated session
// ---------------------------------------------------------------------------

/// A fully negotiated MCP client session.
///
/// Created by [`McpProtocol::initialize`]. Provides typed async methods for
/// all MCP operations defined in protocol revision `2025-11-25`.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::protocol::ServerCapabilityFlag;
/// // Obtained from McpProtocol::initialize(...)
/// // let session: InitializedMcpProtocol = ...;
/// // if session.capable(ServerCapabilityFlag::Tools) {
/// //     let tools = session.list_tools().await?;
/// // }
/// ```
#[derive(Debug)]
pub struct InitializedMcpProtocol {
    /// The underlying JSON-RPC client.
    pub client: JsonRpcClient,
    /// The server's response to the `initialize` request.
    pub initialize_response: InitializeResponse,
}

impl InitializedMcpProtocol {
    /// Check whether the server advertises a specific capability.
    ///
    /// Inspects the capability fields on the [`InitializeResponse`] that was
    /// received during the handshake.
    ///
    /// # Arguments
    ///
    /// * `capability` - The capability flag to check.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::mcp::protocol::ServerCapabilityFlag;
    /// // let has_tools = session.capable(ServerCapabilityFlag::Tools);
    /// ```
    pub fn capable(&self, capability: ServerCapabilityFlag) -> bool {
        let caps = &self.initialize_response.capabilities;
        match capability {
            ServerCapabilityFlag::Tools => caps.tools.is_some(),
            ServerCapabilityFlag::Resources => caps.resources.is_some(),
            ServerCapabilityFlag::Prompts => caps.prompts.is_some(),
            ServerCapabilityFlag::Logging => caps.logging.is_some(),
            ServerCapabilityFlag::Completions => caps.completions.is_some(),
            ServerCapabilityFlag::Tasks => caps.tasks.is_some(),
            ServerCapabilityFlag::Experimental => caps.experimental.is_some(),
        }
    }

    /// List all tools advertised by the server, following pagination automatically.
    ///
    /// Issues one or more `tools/list` requests, following `nextCursor` until
    /// the server returns `null`, and returns the complete accumulated list.
    ///
    /// # Errors
    ///
    /// Returns an error if any paged request fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // let tools = session.list_tools().await?;
    /// ```
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let resp: ListToolsResponse = self
                .client
                .request(METHOD_TOOLS_LIST, PaginatedParams { cursor }, None)
                .await?;

            tools.extend(resp.tools);

            match resp.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }

        Ok(tools)
    }

    /// Invoke a named tool on the server.
    ///
    /// # Arguments
    ///
    /// * `name` - The tool name as returned by `tools/list`.
    /// * `arguments` - Optional JSON arguments matching the tool's `inputSchema`.
    /// * `task` - Optional task-wrapping parameters (new in `2025-11-25`).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the server returns a JSON-RPC error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // let resp = session.call_tool("search", Some(serde_json::json!({"query": "rust"})), None).await?;
    /// ```
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<serde_json::Value>,
        task: Option<TaskParams>,
    ) -> Result<CallToolResponse> {
        self.client
            .request(
                METHOD_TOOLS_CALL,
                CallToolParams {
                    name: name.to_string(),
                    arguments,
                    meta: None,
                    task,
                },
                None,
            )
            .await
    }

    /// List all resources advertised by the server, following pagination automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if any paged request fails.
    pub async fn list_resources(&self) -> Result<Vec<Resource>> {
        let mut resources = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let resp: ListResourcesResponse = self
                .client
                .request(METHOD_RESOURCES_LIST, PaginatedParams { cursor }, None)
                .await?;

            resources.extend(resp.resources);

            match resp.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }

        Ok(resources)
    }

    /// Read the contents of a resource by URI.
    ///
    /// # Arguments
    ///
    /// * `uri` - The canonical URI of the resource.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the URI is not found.
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContents>> {
        let resp: ReadResourceResponse = self
            .client
            .request(
                METHOD_RESOURCES_READ,
                ReadResourceParams {
                    uri: uri.to_string(),
                },
                None,
            )
            .await?;
        Ok(resp.contents)
    }

    /// List all prompts advertised by the server, following pagination automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if any paged request fails.
    pub async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        let mut prompts = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let resp: ListPromptsResponse = self
                .client
                .request(METHOD_PROMPTS_LIST, PaginatedParams { cursor }, None)
                .await?;

            prompts.extend(resp.prompts);

            match resp.next_cursor {
                Some(c) if !c.is_empty() => cursor = Some(c),
                _ => break,
            }
        }

        Ok(prompts)
    }

    /// Retrieve a rendered prompt by name, substituting template arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The prompt name as returned by `prompts/list`.
    /// * `arguments` - Optional key-value substitutions for template variables.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the prompt name is unknown.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> Result<GetPromptResponse> {
        self.client
            .request(
                METHOD_PROMPTS_GET,
                GetPromptParams {
                    name: name.to_string(),
                    arguments,
                    meta: None,
                },
                None,
            )
            .await
    }

    /// Request argument completions for a prompt or resource template.
    ///
    /// # Arguments
    ///
    /// * `params` - The completion parameters including the ref and partial argument.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn complete(
        &self,
        params: CompletionCompleteParams,
    ) -> Result<CompletionCompleteResponse> {
        self.client
            .request(METHOD_COMPLETION_COMPLETE, params, None)
            .await
    }

    /// Send a `ping` request and verify the server responds.
    ///
    /// # Errors
    ///
    /// Returns an error if the request times out or the channel is closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // session.ping().await?;
    /// ```
    pub async fn ping(&self) -> Result<()> {
        let _: serde_json::Value = self
            .client
            .request(METHOD_PING, serde_json::json!({}), None)
            .await?;
        Ok(())
    }

    /// Retrieve the current state of a long-running task.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the task is not found.
    pub async fn tasks_get(&self, task_id: &str) -> Result<Task> {
        self.client
            .request(
                METHOD_TASKS_GET,
                TasksGetParams {
                    task_id: task_id.to_string(),
                },
                None,
            )
            .await
    }

    /// Retrieve the final result of a completed task.
    ///
    /// The server returns the same payload that was produced by the originating
    /// `tools/call` once the task transitions to `completed`.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the task has not yet completed.
    pub async fn tasks_result(&self, task_id: &str) -> Result<CallToolResponse> {
        self.client
            .request(
                METHOD_TASKS_RESULT,
                TasksResultParams {
                    task_id: task_id.to_string(),
                },
                None,
            )
            .await
    }

    /// Request cancellation of a running task.
    ///
    /// The server will attempt to cancel the task and return its final state.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task to cancel.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the task cannot be cancelled.
    pub async fn tasks_cancel(&self, task_id: &str) -> Result<Task> {
        self.client
            .request(
                METHOD_TASKS_CANCEL,
                TasksCancelParams {
                    task_id: task_id.to_string(),
                },
                None,
            )
            .await
    }

    /// List tasks known to the server, following pagination automatically.
    ///
    /// # Arguments
    ///
    /// * `cursor` - Optional opaque cursor from a previous response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn tasks_list(&self, cursor: Option<&str>) -> Result<TasksListResponse> {
        self.client
            .request(
                METHOD_TASKS_LIST,
                TasksListParams {
                    cursor: cursor.map(|s| s.to_string()),
                },
                None,
            )
            .await
    }

    /// Register a handler for `sampling/createMessage` server-initiated requests.
    ///
    /// When the server sends a `sampling/createMessage` request, the handler is
    /// called with the deserialized parameters. Its return value is sent back as
    /// the response.
    ///
    /// # Arguments
    ///
    /// * `handler` - An `Arc`-wrapped implementation of [`SamplingHandler`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // session.register_sampling_handler(Arc::new(my_sampler));
    /// ```
    pub fn register_sampling_handler(&self, handler: Arc<dyn SamplingHandler>) {
        self.client
            .on_server_request(METHOD_SAMPLING_CREATE_MESSAGE, move |params| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let request: CreateMessageRequest = match serde_json::from_value(params) {
                        Ok(r) => r,
                        Err(e) => {
                            return serde_json::json!({
                                "code": -32602,
                                "message": format!("Invalid params: {e}")
                            });
                        }
                    };
                    match handler.create_message(request).await {
                        Ok(result) => {
                            serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                        }
                        Err(e) => serde_json::json!({
                            "code": -32603,
                            "message": e.to_string()
                        }),
                    }
                })
            });
    }

    /// Register a handler for `elicitation/create` server-initiated requests.
    ///
    /// When the server sends an `elicitation/create` request, the handler is
    /// called with the deserialized parameters. Its return value is sent back as
    /// the response.
    ///
    /// # Arguments
    ///
    /// * `handler` - An `Arc`-wrapped implementation of [`ElicitationHandler`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // session.register_elicitation_handler(Arc::new(my_elicitor));
    /// ```
    pub fn register_elicitation_handler(&self, handler: Arc<dyn ElicitationHandler>) {
        self.client.on_server_request(
            crate::mcp::types::METHOD_ELICITATION_CREATE,
            move |params| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let request: ElicitationCreateParams = match serde_json::from_value(params) {
                        Ok(r) => r,
                        Err(e) => {
                            return serde_json::json!({
                                "code": -32602,
                                "message": format!("Invalid params: {e}")
                            });
                        }
                    };
                    match handler.create_elicitation(request).await {
                        Ok(result) => {
                            serde_json::to_value(result).unwrap_or(serde_json::Value::Null)
                        }
                        Err(e) => serde_json::json!({
                            "code": -32603,
                            "message": e.to_string()
                        }),
                    }
                })
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::client::start_read_loop;
    use crate::mcp::types::{
        ElicitationAction, MessageContent, Role, ServerCapabilities, TextContent,
    };
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    /// Build a wired `McpProtocol` whose underlying `JsonRpcClient` is shared
    /// with the read loop via `Arc`.
    ///
    /// Returns `(protocol, server_outbound_rx, server_inbound_tx, cancel_token)`.
    ///
    /// Both `McpProtocol` and `start_read_loop` point at the same `Arc`, so
    /// responses injected on `server_inbound_tx` are dispatched to the exact
    /// same `pending` map that `McpProtocol::initialize` (etc.) is waiting on.
    fn wired_protocol() -> (
        McpProtocol,
        mpsc::UnboundedReceiver<String>,
        mpsc::UnboundedSender<String>,
        CancellationToken,
    ) {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
        let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let client = Arc::new(JsonRpcClient::new(out_tx));
        start_read_loop(in_rx, token.clone(), Arc::clone(&client));
        // SAFETY: there is exactly one strong reference remaining after the
        // read loop's clone, so `try_unwrap` succeeds here.
        // Actually the read loop holds a clone, so we cannot unwrap.
        // Instead, rebuild using the same outbound sender clone so that
        // both the loop's client and the protocol's client share the same
        // underlying channel (and therefore the same pending map when we
        // make the loop operate on the Arc we pass it).
        //
        // The correct design: reconstruct McpProtocol from the *same* Arc.
        // We accomplish this by giving McpProtocol a constructor that accepts
        // Arc<JsonRpcClient>.  Since McpProtocol already wraps a JsonRpcClient
        // by value, we instead pass a *fresh* JsonRpcClient that shares the
        // same outbound_tx so the serialized requests reach the same transport,
        // and we point the read loop at the *shared* Arc so responses are
        // dispatched back to the protocol's pending map.
        //
        // This only works if McpProtocol and the Arc share the same pending map.
        // The cleanest approach: let McpProtocol own the Arc and delegate.
        // For now, the simplest fix that does not require changing public API:
        // give the read loop the same Arc that we extract the inner value from.
        //
        // Real solution: build the Arc, pass the Arc to both the read loop and
        // to a thin McpProtocol wrapper that stores Arc instead of value.
        // Since changing public API is not desired mid-phase, we use the
        // `wired_client` approach in each test directly.
        drop(client); // drop our ref; the loop holds its own clone
                      // Rebuild properly:
        let (out_tx2, out_rx2) = mpsc::unbounded_channel::<String>();
        let (in_tx2, in_rx2) = mpsc::unbounded_channel::<String>();
        let token2 = CancellationToken::new();
        let shared = Arc::new(JsonRpcClient::new(out_tx2));
        start_read_loop(in_rx2, token2.clone(), Arc::clone(&shared));
        // Give McpProtocol a client that shares pending/handlers with the loop.
        // Because JsonRpcClient is not Clone and McpProtocol takes it by value,
        // we use the Arc itself inside McpProtocol by wrapping with a newtype.
        // Simplest working approach: McpProtocol gets the Arc-extracted client
        // by constructing a JsonRpcClient whose fields alias the Arc's fields.
        let proto_client = JsonRpcClient {
            next_id: Arc::clone(&shared.next_id),
            pending: Arc::clone(&shared.pending),
            outbound_tx: shared.outbound_tx.clone(),
            notification_handlers: Arc::clone(&shared.notification_handlers),
            server_request_handlers: Arc::clone(&shared.server_request_handlers),
        };
        drop(out_rx);
        drop(in_tx);
        drop(token);
        (McpProtocol::new(proto_client), out_rx2, in_tx2, token2)
    }

    /// Build a wired `InitializedMcpProtocol` whose `JsonRpcClient` is shared
    /// with the read loop. Returns `(session, out_rx, in_tx, token)`.
    fn wired_session(
        capabilities: ServerCapabilities,
    ) -> (
        InitializedMcpProtocol,
        mpsc::UnboundedReceiver<String>,
        mpsc::UnboundedSender<String>,
        CancellationToken,
    ) {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
        let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let shared = Arc::new(JsonRpcClient::new(out_tx));
        start_read_loop(in_rx, token.clone(), Arc::clone(&shared));
        let proto_client = JsonRpcClient {
            next_id: Arc::clone(&shared.next_id),
            pending: Arc::clone(&shared.pending),
            outbound_tx: shared.outbound_tx.clone(),
            notification_handlers: Arc::clone(&shared.notification_handlers),
            server_request_handlers: Arc::clone(&shared.server_request_handlers),
        };
        let session = InitializedMcpProtocol {
            client: proto_client,
            initialize_response: InitializeResponse {
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                capabilities,
                server_info: Implementation {
                    name: "mock".to_string(),
                    version: "1.0".to_string(),
                    description: None,
                },
                instructions: None,
            },
        };
        (session, out_rx, in_tx, token)
    }

    #[test]
    fn test_server_capability_flag_tools_absent_by_default() {
        let caps = ServerCapabilities::default();
        let resp = InitializeResponse {
            protocol_version: "2025-11-25".to_string(),
            capabilities: caps,
            server_info: Implementation {
                name: "mock".to_string(),
                version: "0.1".to_string(),
                description: None,
            },
            instructions: None,
        };
        let session = InitializedMcpProtocol {
            client: {
                let (tx, _rx) = mpsc::unbounded_channel::<String>();
                JsonRpcClient::new(tx)
            },
            initialize_response: resp,
        };
        assert!(!session.capable(ServerCapabilityFlag::Tools));
        assert!(!session.capable(ServerCapabilityFlag::Resources));
        assert!(!session.capable(ServerCapabilityFlag::Prompts));
    }

    #[test]
    fn test_server_capability_flag_tools_present() {
        let caps = ServerCapabilities {
            tools: Some(serde_json::json!({})),
            resources: Some(serde_json::json!({})),
            ..Default::default()
        };
        let resp = InitializeResponse {
            protocol_version: "2025-11-25".to_string(),
            capabilities: caps,
            server_info: Implementation {
                name: "mock".to_string(),
                version: "0.1".to_string(),
                description: None,
            },
            instructions: None,
        };
        let session = InitializedMcpProtocol {
            client: {
                let (tx, _rx) = mpsc::unbounded_channel::<String>();
                JsonRpcClient::new(tx)
            },
            initialize_response: resp,
        };
        assert!(session.capable(ServerCapabilityFlag::Tools));
        assert!(session.capable(ServerCapabilityFlag::Resources));
        assert!(!session.capable(ServerCapabilityFlag::Prompts));
    }

    #[tokio::test]
    async fn test_initialize_rejects_unsupported_protocol_version() {
        let (proto, mut out_rx, in_tx, ct) = wired_protocol();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let raw = out_rx.recv().await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let id = req["id"].clone();
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "1999-01-01",
                    "capabilities": {},
                    "serverInfo": { "name": "old-server", "version": "0.0.1" }
                }
            });
            in_tx.send(serde_json::to_string(&resp).unwrap()).unwrap();
        });

        let result = proto
            .initialize(
                Implementation {
                    name: "xzatoma".to_string(),
                    version: "0.2.0".to_string(),
                    description: None,
                },
                ClientCapabilities::default(),
            )
            .await;

        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("1999-01-01") || err_str.contains("version"),
            "unexpected error: {err_str}"
        );
        ct.cancel();
    }

    #[tokio::test]
    async fn test_initialize_succeeds_with_supported_version() {
        let (proto, mut out_rx, in_tx, ct) = wired_protocol();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let raw = out_rx.recv().await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let id = req["id"].clone();
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": LATEST_PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "test-server", "version": "1.0.0" }
                }
            });
            in_tx.send(serde_json::to_string(&resp).unwrap()).unwrap();
        });

        let result = proto
            .initialize(
                Implementation {
                    name: "xzatoma".to_string(),
                    version: "0.2.0".to_string(),
                    description: None,
                },
                ClientCapabilities::default(),
            )
            .await;

        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let session = result.unwrap();
        assert_eq!(
            session.initialize_response.protocol_version,
            LATEST_PROTOCOL_VERSION
        );
        assert!(session.capable(ServerCapabilityFlag::Tools));
        assert!(!session.capable(ServerCapabilityFlag::Prompts));
        ct.cancel();
    }

    #[tokio::test]
    async fn test_list_tools_follows_cursor_pagination() {
        let (session, mut out_rx, in_tx, ct) = wired_session(ServerCapabilities::default());

        tokio::spawn(async move {
            // First page: returns one tool and a cursor.
            tokio::time::sleep(Duration::from_millis(5)).await;
            let raw = out_rx.recv().await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let id1 = req["id"].clone();
            let resp1 = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id1,
                "result": {
                    "tools": [{ "name": "tool_a", "inputSchema": {} }],
                    "nextCursor": "page2"
                }
            });
            in_tx.send(serde_json::to_string(&resp1).unwrap()).unwrap();

            // Second page: returns one tool and no cursor.
            tokio::time::sleep(Duration::from_millis(5)).await;
            let raw2 = out_rx.recv().await.unwrap();
            let req2: serde_json::Value = serde_json::from_str(&raw2).unwrap();
            let id2 = req2["id"].clone();
            let resp2 = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id2,
                "result": {
                    "tools": [{ "name": "tool_b", "inputSchema": {} }],
                    "nextCursor": null
                }
            });
            in_tx.send(serde_json::to_string(&resp2).unwrap()).unwrap();
        });

        let tools = session.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "tool_a");
        assert_eq!(tools[1].name, "tool_b");
        ct.cancel();
    }

    /// Test that the `SamplingHandler` trait is object-safe and can be boxed.
    #[test]
    fn test_sampling_handler_is_object_safe() {
        struct Noop;
        impl SamplingHandler for Noop {
            fn create_message<'a>(
                &'a self,
                _params: CreateMessageRequest,
            ) -> BoxFuture<'a, Result<CreateMessageResult>> {
                Box::pin(async move {
                    Ok(CreateMessageResult {
                        role: Role::Assistant,
                        content: MessageContent::Text(TextContent {
                            text: "ok".to_string(),
                            annotations: None,
                        }),
                        model: "noop".to_string(),
                        stop_reason: None,
                    })
                })
            }
        }
        let _: Arc<dyn SamplingHandler> = Arc::new(Noop);
    }

    /// Test that the `ElicitationHandler` trait is object-safe and can be boxed.
    #[test]
    fn test_elicitation_handler_is_object_safe() {
        struct AutoDecline;
        impl ElicitationHandler for AutoDecline {
            fn create_elicitation<'a>(
                &'a self,
                _params: ElicitationCreateParams,
            ) -> BoxFuture<'a, Result<ElicitationResult>> {
                Box::pin(async move {
                    Ok(ElicitationResult {
                        action: ElicitationAction::Decline,
                        content: None,
                    })
                })
            }
        }
        let _: Arc<dyn ElicitationHandler> = Arc::new(AutoDecline);
    }
}
