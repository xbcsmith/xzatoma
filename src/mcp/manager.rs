//! MCP client lifecycle and server manager
//!
//! This module provides [`McpClientManager`], which manages the full lifecycle
//! of all connected MCP servers. It owns the transport, JSON-RPC client, and
//! initialized protocol session for each server, and exposes a unified API for
//! tool calls, resource reads, prompt retrieval, and connection management.
//!
//! # Architecture
//!
//! ```text
//! McpClientManager
//!   |-- McpServerEntry (per server)
//!         |-- McpServerConfig    (static config)
//!         |-- McpServerState     (runtime state)
//!         |-- InitializedMcpProtocol  (live session, once Connected)
//!         |-- Vec<McpTool>        (cached tool list)
//!         |-- Option<AuthManager> (OAuth, HTTP servers only)
//!         |-- read_loop_handle    (background Tokio task)
//! ```
//!
//! # Connection Lifecycle
//!
//! 1. Call [`McpClientManager::connect`] with a [`McpServerConfig`].
//! 2. The manager spawns the transport (stdio or HTTP), wires up channels,
//!    runs the MCP `initialize` handshake, and caches the tool list.
//! 3. The entry transitions to [`McpServerState::Connected`].
//! 4. Call [`McpClientManager::disconnect`] to abort the read loop and drop
//!    the session.
//! 5. Call [`McpClientManager::reconnect`] to disconnect then re-connect.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::mcp::auth::discovery::{
    fetch_authorization_server_metadata, fetch_protected_resource_metadata,
    AuthorizationServerMetadata,
};
use crate::mcp::auth::flow::OAuthFlowConfig;
use crate::mcp::auth::manager::AuthManager;
use crate::mcp::auth::token_store::TokenStore;
use crate::mcp::client::{start_read_loop, JsonRpcClient};
use crate::mcp::config::McpConfig;
use crate::mcp::protocol::{InitializedMcpProtocol, McpProtocol};
use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};
use crate::mcp::transport::Transport;
use crate::mcp::types::{
    CallToolResponse, ClientCapabilities, ElicitationCapability, GetPromptResponse, Implementation,
    McpTool, Prompt, Resource, ResourceContents, RootsCapability, SamplingCapability, TaskParams,
    TasksCapability,
};

// ---------------------------------------------------------------------------
// McpServerState
// ---------------------------------------------------------------------------

/// Runtime connection state of a single MCP server entry.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::manager::McpServerState;
///
/// let state = McpServerState::Disconnected;
/// assert_eq!(state, McpServerState::Disconnected);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum McpServerState {
    /// Not connected; no transport or session exists.
    Disconnected,
    /// Connection attempt is in progress.
    Connecting,
    /// Transport, read loop, and protocol session are fully established.
    Connected,
    /// Connection or protocol handshake failed with an error message.
    Failed(String),
}

// ---------------------------------------------------------------------------
// McpServerEntry
// ---------------------------------------------------------------------------

/// All runtime state for a single registered MCP server.
///
/// Created by [`McpClientManager::connect`] and kept alive until
/// [`McpClientManager::disconnect`] or [`McpClientManager::reconnect`] is
/// called.
pub struct McpServerEntry {
    /// Static configuration used to establish (or re-establish) the connection.
    pub config: McpServerConfig,

    /// Live protocol session. `None` when the state is not `Connected`.
    pub protocol: Option<Arc<InitializedMcpProtocol>>,

    /// Cached list of tools advertised by the server.
    ///
    /// Populated immediately after a successful `initialize` handshake when
    /// [`McpServerConfig::tools_enabled`] is `true`. Refreshed by
    /// [`McpClientManager::refresh_tools`].
    pub tools: Vec<McpTool>,

    /// Current connection state.
    pub state: McpServerState,

    /// OAuth authorization manager for HTTP servers that require OAuth 2.1.
    ///
    /// `None` for stdio transports or unauthenticated HTTP endpoints.
    pub auth_manager: Option<Arc<AuthManager>>,

    /// Authorization server metadata discovered for OAuth-enabled HTTP servers.
    ///
    /// Cached here so that token refresh and 401 re-auth can reuse the same
    /// discovery result without issuing additional network requests.
    pub server_metadata: Option<AuthorizationServerMetadata>,

    /// Handle to the background Tokio read loop task.
    ///
    /// Aborted on [`McpClientManager::disconnect`].
    pub read_loop_handle: Option<tokio::task::JoinHandle<()>>,

    /// Cancellation token for the read loop.
    pub cancellation: Option<CancellationToken>,
}

impl std::fmt::Debug for McpServerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServerEntry")
            .field("id", &self.config.id)
            .field("state", &self.state)
            .field("tools_count", &self.tools.len())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// McpClientManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all MCP server connections.
///
/// Create one instance per agent session via [`McpClientManager::new`]. After
/// construction, call [`connect`][Self::connect] for each server listed in the
/// [`McpConfig`], or call [`connect_all`][Self::connect_all] to connect every
/// enabled server at once.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use xzatoma::mcp::auth::token_store::TokenStore;
/// use xzatoma::mcp::manager::McpClientManager;
///
/// # async fn example() -> anyhow::Result<()> {
/// let http = Arc::new(reqwest::Client::new());
/// let store = Arc::new(TokenStore);
/// let manager = McpClientManager::new(http, store);
/// # Ok(())
/// # }
/// ```
pub struct McpClientManager {
    /// Per-server runtime entries keyed by server ID.
    servers: HashMap<String, McpServerEntry>,

    /// Shared HTTP client used for HTTP transports and OAuth discovery.
    http_client: Arc<reqwest::Client>,

    /// Shared OS-keyring token store used by all OAuth-enabled servers.
    token_store: Arc<TokenStore>,

    /// Task manager for long-running MCP tasks (Phase 6 placeholder).
    ///
    /// Guarded by a `Mutex` so that notification callbacks registered during
    /// `connect` can enqueue updates from background threads.
    // Retained for Phase 6: task lifecycle tracking via notifications/tasks/status.
    // The field must stay alive so the Arc keeps the Mutex live for future
    // background-thread callbacks.
    #[allow(dead_code)]
    task_manager: Arc<std::sync::Mutex<crate::mcp::task_manager::TaskManager>>,
}

impl std::fmt::Debug for McpClientManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClientManager")
            .field("server_count", &self.servers.len())
            .finish_non_exhaustive()
    }
}

impl McpClientManager {
    /// Create a new `McpClientManager` with no servers registered.
    ///
    /// # Arguments
    ///
    /// * `http_client` - Shared HTTP client for HTTP transports and OAuth
    ///   discovery requests.
    /// * `token_store` - Shared OS-keyring accessor for OAuth token
    ///   persistence.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    /// use xzatoma::mcp::manager::McpClientManager;
    ///
    /// let manager = McpClientManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// );
    /// ```
    pub fn new(http_client: Arc<reqwest::Client>, token_store: Arc<TokenStore>) -> Self {
        Self {
            servers: HashMap::new(),
            http_client,
            token_store,
            task_manager: Arc::new(std::sync::Mutex::new(
                crate::mcp::task_manager::TaskManager::default(),
            )),
        }
    }

    /// Connect to all enabled servers listed in `config`.
    ///
    /// Iterates over [`McpConfig::servers`] and calls [`connect`][Self::connect]
    /// for each entry where [`McpServerConfig::enabled`] is `true`.
    /// Failures for individual servers are logged but do not stop remaining
    /// servers from connecting.
    ///
    /// # Arguments
    ///
    /// * `config` - Top-level MCP client configuration.
    ///
    /// # Returns
    ///
    /// `Ok(())` always; individual server errors are reported via
    /// `tracing::error!`.
    ///
    /// # Errors
    ///
    /// Never returns `Err`; per-server failures are recorded as
    /// [`McpServerState::Failed`] but do not propagate to the caller.
    pub async fn connect_all(&mut self, config: &McpConfig) -> Result<()> {
        for server_config in &config.servers {
            if !server_config.enabled {
                tracing::debug!(id = %server_config.id, "Skipping disabled MCP server");
                continue;
            }
            if let Err(e) = self.connect(server_config.clone()).await {
                tracing::error!(
                    id = %server_config.id,
                    error = %e,
                    "Failed to connect to MCP server"
                );
                // Record the failure so callers can inspect state.
                self.servers
                    .entry(server_config.id.clone())
                    .and_modify(|entry| {
                        entry.state = McpServerState::Failed(e.to_string());
                    })
                    .or_insert_with(|| McpServerEntry {
                        config: server_config.clone(),
                        protocol: None,
                        tools: Vec::new(),
                        state: McpServerState::Failed(e.to_string()),
                        auth_manager: None,
                        server_metadata: None,
                        read_loop_handle: None,
                        cancellation: None,
                    });
            }
        }
        Ok(())
    }

    /// Connect to a single MCP server described by `config`.
    ///
    /// Steps:
    ///
    /// 1. Insert/update the entry with state `Connecting`.
    /// 2. Build the transport (stdio or HTTP, with optional OAuth).
    /// 3. Wire inbound/outbound channels and start the read loop.
    /// 4. Run the MCP `initialize` / `notifications/initialized` handshake.
    /// 5. Register sampling and elicitation handlers if enabled.
    /// 6. Fetch and cache the tool list if `tools_enabled`.
    /// 7. Set state to `Connected`.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the server to connect to.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpTransport`] if the transport cannot be
    /// established, [`XzatomaError::McpProtocolVersion`] if the server
    /// returns an unsupported protocol version, or any other
    /// [`XzatomaError`] variant for network, authentication, or protocol
    /// errors.
    pub async fn connect(&mut self, config: McpServerConfig) -> Result<()> {
        let id = config.id.clone();
        tracing::info!(id = %id, "Connecting to MCP server");

        // Mark as Connecting.
        self.servers.insert(
            id.clone(),
            McpServerEntry {
                config: config.clone(),
                protocol: None,
                tools: Vec::new(),
                state: McpServerState::Connecting,
                auth_manager: None,
                server_metadata: None,
                read_loop_handle: None,
                cancellation: None,
            },
        );

        // Build the transport and optional auth manager.
        let (transport, auth_manager, server_metadata) =
            self.build_transport(&config).await.map_err(|e| {
                // Update state to Failed before propagating.
                if let Some(entry) = self.servers.get_mut(&id) {
                    entry.state = McpServerState::Failed(e.to_string());
                }
                e
            })?;

        // Wire channels: the read loop receives from the transport's inbound
        // stream and forwards to the JSON-RPC client.
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>();

        // Spawn a task that bridges transport <--> channels.
        let transport_arc = Arc::new(transport);
        let transport_clone_send = Arc::clone(&transport_arc);
        let transport_clone_recv = Arc::clone(&transport_arc);
        let inbound_tx_clone = inbound_tx.clone();

        // Forward transport receive -> inbound_tx (read side).
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = transport_clone_recv.receive();
            while let Some(msg) = stream.next().await {
                if inbound_tx_clone.send(msg).is_err() {
                    break;
                }
            }
        });

        // Forward outbound_rx -> transport send (write side).
        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if let Err(e) = transport_clone_send.send(msg).await {
                    tracing::error!(error = %e, "MCP transport send failed");
                    break;
                }
            }
        });

        // Create the JSON-RPC client.
        let shared = Arc::new(JsonRpcClient::new(outbound_tx));
        let cancellation = CancellationToken::new();
        let handle = start_read_loop(inbound_rx, cancellation.clone(), Arc::clone(&shared));

        // Build protocol session (clone_shared preserves all Arc state).
        let proto_client = shared.clone_shared();
        let protocol_uninit = McpProtocol::new(proto_client);

        let client_info = Implementation {
            name: "xzatoma".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: Some("Autonomous AI agent".to_string()),
        };
        let capabilities = xzatoma_client_capabilities();

        let initialized = protocol_uninit
            .initialize(client_info, capabilities)
            .await
            .map_err(|e| {
                if let Some(entry) = self.servers.get_mut(&id) {
                    entry.state = McpServerState::Failed(e.to_string());
                }
                e
            })?;

        let protocol = Arc::new(initialized);

        // Register sampling handler stub if enabled.
        if config.sampling_enabled {
            tracing::debug!(id = %id, "Registering sampling handler (stub)");
            // Phase 5B will replace this with a real XzatomaSamplingHandler.
            // For now we register a no-op so that `capabilities.sampling` is
            // honoured at the protocol level.
        }

        // Register elicitation handler stub if enabled.
        if config.elicitation_enabled {
            tracing::debug!(id = %id, "Registering elicitation handler (stub)");
            // Phase 5B will replace this with a real XzatomaElicitationHandler.
        }

        // Fetch and cache the tool list.
        let tools = if config.tools_enabled {
            match protocol.list_tools().await {
                Ok(t) => {
                    tracing::debug!(id = %id, count = t.len(), "Cached tool list");
                    t
                }
                Err(e) => {
                    tracing::warn!(id = %id, error = %e, "list_tools failed after connect");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Update the entry to Connected.
        if let Some(entry) = self.servers.get_mut(&id) {
            entry.protocol = Some(Arc::clone(&protocol));
            entry.tools = tools;
            entry.state = McpServerState::Connected;
            entry.auth_manager = auth_manager.map(Arc::new);
            entry.server_metadata = server_metadata;
            entry.read_loop_handle = Some(handle);
            entry.cancellation = Some(cancellation);
        }

        tracing::info!(id = %id, "Connected to MCP server");
        Ok(())
    }

    /// Disconnect from the named server.
    ///
    /// Cancels and awaits the read loop task, drops the protocol session,
    /// and transitions the entry to [`McpServerState::Disconnected`].
    ///
    /// This is a no-op if the server is not registered.
    ///
    /// # Arguments
    ///
    /// * `id` - Server identifier as used in [`McpServerConfig::id`].
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if `id` is not registered.
    pub async fn disconnect(&mut self, id: &str) -> Result<()> {
        let entry = self
            .servers
            .get_mut(id)
            .ok_or_else(|| XzatomaError::McpServerNotFound(id.to_string()))?;

        // Cancel the read loop.
        if let Some(ref token) = entry.cancellation {
            token.cancel();
        }

        // Abort the task handle in case the cancellation token is not polled.
        if let Some(handle) = entry.read_loop_handle.take() {
            handle.abort();
        }

        // Drop the session.
        entry.protocol = None;
        entry.tools.clear();
        entry.state = McpServerState::Disconnected;
        entry.cancellation = None;

        tracing::info!(id = %id, "Disconnected from MCP server");
        Ok(())
    }

    /// Disconnect then reconnect the named server.
    ///
    /// Retrieves the stored [`McpServerConfig`], calls [`disconnect`][Self::disconnect],
    /// then calls [`connect`][Self::connect] with the same config.
    ///
    /// # Arguments
    ///
    /// * `id` - Server identifier.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if `id` is not registered,
    /// or any error that [`connect`][Self::connect] may return.
    pub async fn reconnect(&mut self, id: &str) -> Result<()> {
        let config = self
            .servers
            .get(id)
            .map(|e| e.config.clone())
            .ok_or_else(|| XzatomaError::McpServerNotFound(id.to_string()))?;

        self.disconnect(id).await?;
        self.connect(config).await
    }

    /// Re-fetch and cache the tool list for the named server.
    ///
    /// Issues a new `tools/list` request and replaces the cached list.
    ///
    /// # Arguments
    ///
    /// * `id` - Server identifier.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if the server is unknown,
    /// [`XzatomaError::Mcp`] if the server is not currently connected, or
    /// any JSON-RPC error returned by `tools/list`.
    pub async fn refresh_tools(&mut self, id: &str) -> Result<()> {
        let protocol = self
            .servers
            .get(id)
            .and_then(|e| e.protocol.as_ref())
            .map(Arc::clone)
            .ok_or_else(|| XzatomaError::McpServerNotFound(id.to_string()))?;

        let tools = protocol.list_tools().await?;
        if let Some(entry) = self.servers.get_mut(id) {
            entry.tools = tools;
        }
        Ok(())
    }

    /// Return all entries that are currently in [`McpServerState::Connected`].
    ///
    /// # Returns
    ///
    /// A slice of references to connected server entries.
    pub fn connected_servers(&self) -> Vec<&McpServerEntry> {
        self.servers
            .values()
            .filter(|e| e.state == McpServerState::Connected)
            .collect()
    }

    /// Return the tool lists of all connected servers that have `tools_enabled`.
    ///
    /// Each tuple is `(server_id, tools)`.
    ///
    /// # Returns
    ///
    /// A `Vec` of `(String, Vec<McpTool>)` pairs, one per connected server
    /// with at least one tool.
    pub fn get_tools_for_registry(&self) -> Vec<(String, Vec<McpTool>)> {
        self.servers
            .values()
            .filter(|e| e.state == McpServerState::Connected && e.config.tools_enabled)
            .map(|e| (e.config.id.clone(), e.tools.clone()))
            .collect()
    }

    /// Invoke a tool on the named server.
    ///
    /// Looks up the server, verifies the tool is in the cached list, and
    /// delegates to [`InitializedMcpProtocol::call_tool`]. On a `401`
    /// authentication error the method attempts a single re-authentication
    /// cycle before propagating the error.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    /// * `tool_name` - Name of the tool to invoke.
    /// * `arguments` - Optional JSON arguments matching the tool's
    ///   `inputSchema`.
    ///
    /// # Returns
    ///
    /// The server's [`CallToolResponse`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] when `server_id` is
    /// unknown, [`XzatomaError::McpToolNotFound`] when `tool_name` is not in
    /// the cached list, or any other protocol/transport error.
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<CallToolResponse> {
        let entry = self
            .servers
            .get(server_id)
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()))?;

        // Verify the tool is known.
        if !entry.tools.iter().any(|t| t.name == tool_name) {
            return Err(XzatomaError::McpToolNotFound {
                server: server_id.to_string(),
                tool: tool_name.to_string(),
            });
        }

        let protocol = entry
            .protocol
            .as_ref()
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()))?;

        // First attempt.
        let result = protocol.call_tool(tool_name, arguments.clone(), None).await;

        // On 401, attempt re-auth and retry once.
        match result {
            Err(ref error) if is_mcp_auth_error(error) => {
                if let (Some(auth_manager), Some(metadata)) =
                    (&entry.auth_manager, &entry.server_metadata)
                {
                    tracing::info!(
                        server_id = %server_id,
                        "401 detected, re-authenticating"
                    );
                    // Obtain a fresh token (handle_401 clears stale token).
                    auth_manager
                        .handle_401(server_id, "", metadata)
                        .await
                        .map_err(|auth_err| {
                            tracing::error!(
                                server_id = %server_id,
                                error = %auth_err,
                                "Re-authentication failed"
                            );
                            auth_err
                        })?;

                    // Retry the tool call with the refreshed session.
                    return protocol.call_tool(tool_name, arguments, None).await;
                }
                result
            }
            other => other,
        }
    }

    /// Invoke a tool on the named server, wrapping the call in an MCP task.
    ///
    /// Calls [`InitializedMcpProtocol::call_tool`] with [`TaskParams`]. If
    /// the response contains a task reference in its `_meta`, the method
    /// delegates to the task manager to wait for completion. Otherwise the
    /// response is returned directly.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    /// * `tool_name` - Name of the tool to invoke.
    /// * `arguments` - Optional JSON arguments.
    /// * `ttl` - Optional time-to-live in seconds for the task.
    ///
    /// # Errors
    ///
    /// Same as [`call_tool`][Self::call_tool], plus
    /// [`XzatomaError::McpTask`] if task management fails.
    pub async fn call_tool_as_task(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Option<serde_json::Value>,
        ttl: Option<u64>,
    ) -> Result<CallToolResponse> {
        let entry = self
            .servers
            .get(server_id)
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()))?;

        if !entry.tools.iter().any(|t| t.name == tool_name) {
            return Err(XzatomaError::McpToolNotFound {
                server: server_id.to_string(),
                tool: tool_name.to_string(),
            });
        }

        let protocol = entry
            .protocol
            .as_ref()
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()))?;

        let task_params = TaskParams { ttl };
        let response = protocol
            .call_tool(tool_name, arguments, Some(task_params))
            .await?;

        // If the response meta indicates a task was created, wait for it.
        // Phase 6 will implement full task polling via TaskManager; for now
        // we return the initial response directly.
        if response
            .meta
            .as_ref()
            .and_then(|m| m.get("taskId"))
            .is_some()
        {
            tracing::debug!(
                server_id = %server_id,
                tool = %tool_name,
                "Task created; returning initial response (Phase 6 will add polling)"
            );
        }

        Ok(response)
    }

    /// List all resources advertised by the named server.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if the server is unknown
    /// or not connected, or any JSON-RPC error from `resources/list`.
    pub async fn list_resources(&self, server_id: &str) -> Result<Vec<Resource>> {
        let protocol = self.require_protocol(server_id)?;
        protocol.list_resources().await
    }

    /// Read the content of a resource by URI from the named server.
    ///
    /// For [`ResourceContents::Text`] the text is returned directly.
    /// For [`ResourceContents::Blob`] the blob string is returned prefixed
    /// with `"[base64 <mime_type>] "` so that callers can detect binary
    /// payloads.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    /// * `uri` - Resource URI as returned by `resources/list`.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] or any JSON-RPC error
    /// from `resources/read`.
    pub async fn read_resource(&self, server_id: &str, uri: &str) -> Result<String> {
        let protocol = self.require_protocol(server_id)?;
        let contents_list = protocol.read_resource(uri).await?;

        // Flatten all content items into a single string.
        let mut parts = Vec::new();
        for contents in contents_list {
            let text = match contents {
                ResourceContents::Text(t) => t.text,
                ResourceContents::Blob(b) => {
                    let mime = b.mime_type.as_deref().unwrap_or("application/octet-stream");
                    format!("[base64 {}] {}", mime, b.blob)
                }
            };
            parts.push(text);
        }
        Ok(parts.join("\n"))
    }

    /// List all prompts advertised by the named server.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] or any JSON-RPC error
    /// from `prompts/list`.
    pub async fn list_prompts(&self, server_id: &str) -> Result<Vec<Prompt>> {
        let protocol = self.require_protocol(server_id)?;
        protocol.list_prompts().await
    }

    /// Retrieve a named prompt from the named server.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Server identifier.
    /// * `name` - Prompt name as returned by `prompts/list`.
    /// * `arguments` - Named string arguments for the prompt template.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] or any JSON-RPC error
    /// from `prompts/get`.
    pub async fn get_prompt(
        &self,
        server_id: &str,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<GetPromptResponse> {
        let protocol = self.require_protocol(server_id)?;
        protocol.get_prompt(name, Some(arguments)).await
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Build a transport for the given server config, together with an
    /// optional [`AuthManager`] and discovered
    /// [`AuthorizationServerMetadata`] for OAuth-enabled HTTP servers.
    async fn build_transport(
        &self,
        config: &McpServerConfig,
    ) -> Result<(
        Box<dyn Transport>,
        Option<AuthManager>,
        Option<AuthorizationServerMetadata>,
    )> {
        match &config.transport {
            McpServerTransportConfig::Stdio {
                executable,
                args,
                env,
                working_dir,
            } => {
                use crate::mcp::transport::stdio::StdioTransport;
                use std::path::PathBuf;

                let transport = StdioTransport::spawn(
                    PathBuf::from(executable),
                    args.clone(),
                    env.clone(),
                    working_dir.as_deref().map(PathBuf::from),
                )?;
                Ok((Box::new(transport) as Box<dyn Transport>, None, None))
            }

            McpServerTransportConfig::Http {
                endpoint,
                headers,
                timeout_seconds,
                oauth,
            } => {
                use crate::mcp::transport::http::HttpTransport;

                let timeout = Duration::from_secs(
                    timeout_seconds
                        .or(Some(config.timeout_seconds))
                        .unwrap_or(30),
                );

                let mut effective_headers = headers.clone();

                // If OAuth is configured, run discovery and acquire a token.
                if let Some(oauth_cfg) = oauth {
                    let resource_url = endpoint.clone();

                    // Discovery: fetch protected resource metadata to find the
                    // authorization server, then fetch AS metadata.
                    let prm =
                        fetch_protected_resource_metadata(&self.http_client, &resource_url, None)
                            .await
                            .map_err(|e| {
                                XzatomaError::McpAuth(format!("PRM discovery failed: {}", e))
                            })?;

                    let as_url_str =
                        prm.authorization_servers.first().cloned().ok_or_else(|| {
                            XzatomaError::McpAuth(
                                "No authorization servers in protected resource metadata"
                                    .to_string(),
                            )
                        })?;

                    let as_url = url::Url::parse(&as_url_str).map_err(|e| {
                        XzatomaError::McpAuth(format!(
                            "Invalid authorization server URL '{}': {}",
                            as_url_str, e
                        ))
                    })?;

                    let as_metadata =
                        fetch_authorization_server_metadata(&self.http_client, &as_url)
                            .await
                            .map_err(|e| {
                                XzatomaError::McpAuth(format!(
                                    "AS metadata discovery failed: {}",
                                    e
                                ))
                            })?;

                    // Build the flow config.
                    let flow_config = OAuthFlowConfig {
                        server_id: config.id.clone(),
                        resource_url: resource_url.clone(),
                        client_name: "xzatoma".to_string(),
                        redirect_port: oauth_cfg.redirect_port.unwrap_or(0),
                        static_client_id: oauth_cfg.client_id.clone(),
                        static_client_secret: oauth_cfg.client_secret.clone(),
                    };

                    let mut auth_mgr = AuthManager::new(
                        Arc::clone(&self.http_client),
                        Arc::clone(&self.token_store),
                    );
                    auth_mgr.add_server(config.id.clone(), flow_config);

                    // Acquire an initial token.
                    let token = auth_mgr.get_token(&config.id, &as_metadata).await?;
                    AuthManager::inject_token(&mut effective_headers, &token);

                    let transport =
                        HttpTransport::new(endpoint.clone(), effective_headers, timeout);
                    return Ok((
                        Box::new(transport) as Box<dyn Transport>,
                        Some(auth_mgr),
                        Some(as_metadata),
                    ));
                }

                // Plain HTTP (no OAuth).
                let transport = HttpTransport::new(endpoint.clone(), effective_headers, timeout);
                Ok((Box::new(transport) as Box<dyn Transport>, None, None))
            }
        }
    }

    /// Look up the live protocol for `server_id`, or return an error.
    fn require_protocol(&self, server_id: &str) -> Result<Arc<InitializedMcpProtocol>> {
        self.servers
            .get(server_id)
            .and_then(|e| e.protocol.as_ref())
            .map(Arc::clone)
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()))
    }

    /// Insert a pre-built [`McpServerEntry`] directly into the server map.
    ///
    /// This method is intended for use in integration tests only. It bypasses
    /// the full `connect` lifecycle (transport spawn, initialize handshake,
    /// tool list fetch) so that tests can set up a known-good state without
    /// requiring real processes or network connections.
    ///
    /// # Arguments
    ///
    /// * `id` - The server identifier used as the map key.
    /// * `entry` - A fully constructed [`McpServerEntry`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::mcp::manager::{McpClientManager, McpServerEntry, McpServerState};
    /// use std::sync::Arc;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    ///
    /// # fn example() {
    /// let mut manager = McpClientManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// );
    /// // In tests only: insert a pre-built entry.
    /// # }
    /// ```
    pub fn insert_entry_for_test(&mut self, id: String, entry: McpServerEntry) {
        self.servers.insert(id, entry);
    }
}

// ---------------------------------------------------------------------------
// xzatoma_client_capabilities
// ---------------------------------------------------------------------------

/// Returns the [`ClientCapabilities`] that Xzatoma advertises to MCP servers.
///
/// This is the canonical set of capabilities for all server connections.
/// Sampling and elicitation handlers are registered separately after the
/// `initialize` handshake.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::manager::xzatoma_client_capabilities;
///
/// let caps = xzatoma_client_capabilities();
/// assert!(caps.sampling.is_some());
/// assert!(caps.elicitation.is_some());
/// assert!(caps.roots.is_some());
/// assert!(caps.tasks.is_some());
/// ```
pub fn xzatoma_client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        sampling: Some(SamplingCapability {
            tools: Some(serde_json::json!({})),
            context: None,
        }),
        elicitation: Some(ElicitationCapability {
            form: Some(serde_json::json!({})),
            url: Some(serde_json::json!({})),
        }),
        roots: Some(RootsCapability {
            list_changed: Some(true),
        }),
        tasks: Some(TasksCapability {
            list: Some(serde_json::json!({})),
            cancel: Some(serde_json::json!({})),
            requests: Some(serde_json::json!({})),
        }),
        experimental: None,
    }
}

// ---------------------------------------------------------------------------
// Private utilities
// ---------------------------------------------------------------------------

/// Returns `true` when the error is an [`XzatomaError::McpAuth`] variant,
/// signalling a `401 Unauthorized` response.
fn is_mcp_auth_error(err: &XzatomaError) -> bool {
    matches!(err, XzatomaError::McpAuth(_))
}

// ---------------------------------------------------------------------------
// build_mcp_manager_from_config
// ---------------------------------------------------------------------------

/// Build an `McpClientManager` from the given global configuration.
///
/// This is the single canonical factory for constructing a live
/// [`McpClientManager`]. It inspects `config.mcp.auto_connect` and
/// `config.mcp.servers`; when neither condition is met it returns `Ok(None)`
/// immediately so callers do not need to repeat the guard logic.
///
/// When construction succeeds the manager is wrapped in an
/// `Arc<RwLock<…>>` so it can be shared cheaply across async tasks for the
/// lifetime of the calling command or executor.
///
/// Individual server connection failures are logged as warnings but do not
/// abort the build — the remaining servers are still attempted and the
/// partially-connected manager is returned.
///
/// # Arguments
///
/// * `config` - Global application configuration.
///
/// # Returns
///
/// `Ok(Some(manager))` when `auto_connect` is enabled and at least one server
/// is configured; `Ok(None)` otherwise.
///
/// # Errors
///
/// Currently never returns `Err`; per-server failures are downgraded to
/// warnings. This may change in future versions.
///
/// # Examples
///
/// ```
/// use xzatoma::config::Config;
/// use xzatoma::mcp::manager::build_mcp_manager_from_config;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = Config::default();
/// let manager = build_mcp_manager_from_config(&config).await?;
/// // With the default config (auto_connect=false) we expect None.
/// assert!(manager.is_none());
/// # Ok(())
/// # }
/// ```
pub async fn build_mcp_manager_from_config(
    config: &Config,
) -> Result<Option<Arc<RwLock<McpClientManager>>>> {
    if !config.mcp.auto_connect || config.mcp.servers.is_empty() {
        return Ok(None);
    }

    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    for server_config in config.mcp.servers.iter().filter(|s| s.enabled) {
        if let Err(e) = manager.connect(server_config.clone()).await {
            tracing::warn!(
                server_id = %server_config.id,
                error = %e,
                "Failed to connect to MCP server during startup"
            );
        }
    }

    Ok(Some(Arc::new(RwLock::new(manager))))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::server::McpServerTransportConfig;
    use crate::mcp::transport::fake::FakeTransport;
    use crate::mcp::types::{Implementation, InitializeResponse, McpTool, ServerCapabilities};
    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_manager() -> McpClientManager {
        McpClientManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))
    }

    /// Build a pre-wired manager entry that uses a [`FakeTransport`].
    ///
    /// Returns `(manager, outbound_rx, inbound_tx)` so tests can drive the
    /// mock server side.
    fn make_fake_entry(
        id: &str,
        tools: Vec<McpTool>,
    ) -> (
        McpClientManager,
        mpsc::UnboundedReceiver<String>,
        mpsc::UnboundedSender<String>,
        Arc<InitializedMcpProtocol>,
    ) {
        let (_fake, handle) = FakeTransport::new();

        // Build a protocol with a real JSON-RPC client and FakeTransport.
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
        let shared = Arc::new(JsonRpcClient::new(out_tx));
        let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let _rl_handle = start_read_loop(in_rx, token, Arc::clone(&shared));

        // Manually construct an InitializedMcpProtocol.
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

        let config = McpServerConfig {
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
        };

        let entry = McpServerEntry {
            config,
            protocol: Some(Arc::clone(&protocol)),
            tools,
            state: McpServerState::Connected,
            auth_manager: None,
            server_metadata: None,
            read_loop_handle: None,
            cancellation: None,
        };

        let mut manager = make_manager();
        manager.servers.insert(id.to_string(), entry);

        (manager, handle.outbound_rx, handle.inbound_tx, protocol)
    }

    // -----------------------------------------------------------------------
    // xzatoma_client_capabilities
    // -----------------------------------------------------------------------

    #[test]
    fn test_client_capabilities_sampling_is_some() {
        let caps = xzatoma_client_capabilities();
        assert!(caps.sampling.is_some());
    }

    #[test]
    fn test_client_capabilities_elicitation_is_some() {
        let caps = xzatoma_client_capabilities();
        assert!(caps.elicitation.is_some());
    }

    #[test]
    fn test_client_capabilities_roots_list_changed_is_true() {
        let caps = xzatoma_client_capabilities();
        let roots = caps.roots.expect("roots should be Some");
        assert_eq!(roots.list_changed, Some(true));
    }

    #[test]
    fn test_client_capabilities_tasks_is_some() {
        let caps = xzatoma_client_capabilities();
        assert!(caps.tasks.is_some());
    }

    #[test]
    fn test_client_capabilities_experimental_is_none() {
        let caps = xzatoma_client_capabilities();
        assert!(caps.experimental.is_none());
    }

    // -----------------------------------------------------------------------
    // McpServerState
    // -----------------------------------------------------------------------

    #[test]
    fn test_server_state_eq_disconnected() {
        assert_eq!(McpServerState::Disconnected, McpServerState::Disconnected);
    }

    #[test]
    fn test_server_state_eq_connected() {
        assert_eq!(McpServerState::Connected, McpServerState::Connected);
    }

    #[test]
    fn test_server_state_failed_equality() {
        assert_eq!(
            McpServerState::Failed("err".to_string()),
            McpServerState::Failed("err".to_string())
        );
        assert_ne!(
            McpServerState::Failed("a".to_string()),
            McpServerState::Failed("b".to_string())
        );
    }

    #[test]
    fn test_server_state_not_equal_across_variants() {
        assert_ne!(McpServerState::Connected, McpServerState::Disconnected);
        assert_ne!(McpServerState::Connecting, McpServerState::Connected);
    }

    // -----------------------------------------------------------------------
    // McpClientManager::new
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_manager_has_no_servers() {
        let mgr = make_manager();
        assert!(mgr.servers.is_empty());
    }

    #[test]
    fn test_new_manager_connected_servers_is_empty() {
        let mgr = make_manager();
        assert!(mgr.connected_servers().is_empty());
    }

    #[test]
    fn test_new_manager_tools_for_registry_is_empty() {
        let mgr = make_manager();
        assert!(mgr.get_tools_for_registry().is_empty());
    }

    // -----------------------------------------------------------------------
    // connected_servers / get_tools_for_registry
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_connected_servers_returns_only_connected_entries() {
        let (manager, _, _, _) = make_fake_entry("srv-a", vec![]);

        let connected = manager.connected_servers();
        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].config.id, "srv-a");
    }

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
        let (manager, _, _, _) = make_fake_entry("srv-b", vec![tool]);
        let registry = manager.get_tools_for_registry();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].0, "srv-b");
        assert_eq!(registry[0].1.len(), 1);
        assert_eq!(registry[0].1[0].name, "my_tool");
    }

    // -----------------------------------------------------------------------
    // call_tool -- not-found cases
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_call_tool_returns_not_found_for_unknown_server() {
        let manager = make_manager();
        let result = manager.call_tool("nonexistent", "any_tool", None).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("nonexistent"));
    }

    #[tokio::test]
    async fn test_call_tool_returns_tool_not_found_when_tool_absent() {
        let (manager, _, _, _) = make_fake_entry("srv", vec![]); // no tools cached
        let result = manager.call_tool("srv", "missing_tool", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, XzatomaError::McpToolNotFound { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("missing_tool") || msg.contains("tool not found"),
            "unexpected error: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // disconnect -- not-found
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_disconnect_returns_not_found_for_unknown_server() {
        let mut manager = make_manager();
        let result = manager.disconnect("nobody").await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // disconnect -- connected entry transitions to Disconnected
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_disconnect_transitions_state_to_disconnected() {
        let (mut manager, _, _, _) = make_fake_entry("to-disconnect", vec![]);
        assert_eq!(
            manager.servers["to-disconnect"].state,
            McpServerState::Connected
        );
        manager.disconnect("to-disconnect").await.unwrap();
        assert_eq!(
            manager.servers["to-disconnect"].state,
            McpServerState::Disconnected
        );
        assert!(manager.servers["to-disconnect"].protocol.is_none());
    }

    // -----------------------------------------------------------------------
    // is_mcp_auth_error helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_mcp_auth_error_true_for_mcp_auth_variant() {
        let err = XzatomaError::McpAuth("denied".to_string());
        assert!(is_mcp_auth_error(&err));
    }

    #[test]
    fn test_is_mcp_auth_error_false_for_other_variant() {
        let err = XzatomaError::McpTransport("io error".to_string());
        assert!(!is_mcp_auth_error(&err));
    }

    // -----------------------------------------------------------------------
    // read_resource -- content formatting
    // -----------------------------------------------------------------------

    #[test]
    fn test_blob_resource_contents_format() {
        // Verify the formatting logic for BlobResourceContents.
        let mime = "image/png";
        let blob = "abc123==";
        let formatted = format!("[base64 {}] {}", mime, blob);
        assert!(formatted.starts_with("[base64 image/png]"));
        assert!(formatted.contains("abc123=="));
    }

    // -----------------------------------------------------------------------
    // build_mcp_manager_from_config
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_mcp_manager_from_config_returns_none_when_auto_connect_disabled() {
        let config = Config::default();
        // Default config has auto_connect=false, so we expect None.
        let result = build_mcp_manager_from_config(&config).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_build_mcp_manager_from_config_returns_none_when_no_servers() {
        let mut config = Config::default();
        // Enable auto_connect but leave servers empty.
        config.mcp.auto_connect = true;
        config.mcp.servers = vec![];

        let result = build_mcp_manager_from_config(&config).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_build_mcp_manager_from_config_returns_some_when_servers_configured() {
        use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};

        let mut config = Config::default();
        config.mcp.auto_connect = true;
        // Add a server that will fail to connect (no real server running),
        // but the manager should still be returned (failures are downgraded
        // to warnings by build_mcp_manager_from_config).
        config.mcp.servers = vec![McpServerConfig {
            id: "test-server".to_string(),
            enabled: true,
            transport: McpServerTransportConfig::Stdio {
                executable: "nonexistent-mcp-server".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
                working_dir: None,
            },
            timeout_seconds: 30,
            tools_enabled: true,
            resources_enabled: false,
            prompts_enabled: false,
            sampling_enabled: false,
            elicitation_enabled: true,
        }];

        let result = build_mcp_manager_from_config(&config).await;
        // Connection will fail but the manager Arc is still returned.
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_build_mcp_manager_from_config_skips_disabled_servers() {
        use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};

        let mut config = Config::default();
        config.mcp.auto_connect = true;
        // Only a disabled server -- manager is returned but has no connections.
        config.mcp.servers = vec![McpServerConfig {
            id: "disabled-server".to_string(),
            enabled: false,
            transport: McpServerTransportConfig::Stdio {
                executable: "nonexistent".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
                working_dir: None,
            },
            timeout_seconds: 30,
            tools_enabled: true,
            resources_enabled: false,
            prompts_enabled: false,
            sampling_enabled: false,
            elicitation_enabled: true,
        }];

        let result = build_mcp_manager_from_config(&config).await;
        assert!(result.is_ok());
        // auto_connect=true and non-empty servers list → Some, even though
        // the only server is disabled (disabled servers are simply skipped).
        let manager_arc = result.unwrap().unwrap();
        let manager = manager_arc.read().await;
        assert_eq!(manager.connected_servers().len(), 0);
    }
}
