//! Transport-agnostic async JSON-RPC 2.0 client
//!
//! This module provides [`JsonRpcClient`], a channel-backed JSON-RPC 2.0 client
//! that is completely decoupled from the underlying transport. Callers wire up
//! two [`tokio::sync::mpsc`] channels (one for outbound serialized messages, one
//! for inbound serialized messages) and then call [`start_read_loop`] to process
//! responses and notifications concurrently.
//!
//! # Design
//!
//! - Outbound messages are written to `outbound_tx` as newline-free JSON strings.
//!   The transport layer is responsible for framing (e.g. newline-delimited for
//!   stdio, HTTP chunked for SSE).
//! - Inbound messages arrive on `inbound_rx` as JSON strings. The read loop
//!   classifies each message as a response, a server-initiated request, or a
//!   notification and dispatches accordingly.
//! - In-flight requests are tracked in a `pending` map keyed by `u64` request ID.
//!   Each entry is a `oneshot::Sender` that receives the `result` or `error` value
//!   when the matching response arrives.
//! - A [`tokio_util::sync::CancellationToken`] stops the read loop cleanly and
//!   drops all pending senders so that awaiting callers receive an error.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use crate::error::{Result, XzatomaError};
use crate::mcp::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Default timeout applied to every request when the caller does not specify one.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Convenience alias for a boxed, `Send`-safe async future.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// A notification handler: called with the raw `params` value when a matching
/// server notification arrives.
type NotificationHandler = Box<dyn Fn(serde_json::Value) + Send + Sync + 'static>;

/// A server-request handler: called with the raw `params` value and returns a
/// raw `result` value that is sent back as a JSON-RPC response.
type ServerRequestHandler =
    Box<dyn Fn(serde_json::Value) -> BoxFuture<'static, serde_json::Value> + Send + Sync + 'static>;

/// The pending-response map type: maps request ID to the oneshot sender.
type PendingMap =
    HashMap<u64, oneshot::Sender<std::result::Result<serde_json::Value, JsonRpcError>>>;

/// Transport-agnostic async JSON-RPC 2.0 client.
///
/// Create one with [`JsonRpcClient::new`], passing the outbound channel sender.
/// Then call [`start_read_loop`] to process incoming messages. Issue requests
/// with [`JsonRpcClient::request`] and fire-and-forget notifications with
/// [`JsonRpcClient::notify`].
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use tokio::sync::mpsc;
/// use tokio_util::sync::CancellationToken;
/// use xzatoma::mcp::client::{JsonRpcClient, start_read_loop};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
///     let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
///     let token = CancellationToken::new();
///     let client = Arc::new(JsonRpcClient::new(out_tx));
///     let _handle = start_read_loop(in_rx, token, Arc::clone(&client));
///     Ok(())
/// }
/// ```
pub struct JsonRpcClient {
    /// Monotonically increasing request ID counter.
    pub(crate) next_id: Arc<AtomicU64>,
    /// In-flight requests waiting for a response.
    pub(crate) pending: Arc<Mutex<PendingMap>>,
    /// Channel used to send serialized JSON-RPC messages to the transport.
    pub(crate) outbound_tx: mpsc::UnboundedSender<String>,
    /// Registered handlers for server-sent notifications (method -> handler).
    pub(crate) notification_handlers: Arc<Mutex<HashMap<String, NotificationHandler>>>,
    /// Registered handlers for server-initiated requests (method -> handler).
    pub(crate) server_request_handlers: Arc<Mutex<HashMap<String, ServerRequestHandler>>>,
}

impl std::fmt::Debug for JsonRpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonRpcClient")
            .field("next_id", &self.next_id.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl JsonRpcClient {
    /// Create a new `JsonRpcClient`.
    ///
    /// The caller is responsible for:
    /// 1. Wiring `outbound_rx` to a transport writer.
    /// 2. Calling [`start_read_loop`] with the corresponding inbound receiver.
    ///
    /// # Arguments
    ///
    /// * `outbound_tx` - Sender half of the outbound message channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    ///
    /// let (tx, _rx) = mpsc::unbounded_channel::<String>();
    /// let client = JsonRpcClient::new(tx);
    /// ```
    pub fn new(outbound_tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            outbound_tx,
            notification_handlers: Arc::new(Mutex::new(HashMap::new())),
            server_request_handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new [`JsonRpcClient`] that shares all internal state with
    /// `self`.
    ///
    /// The returned client shares the same `pending` map, `next_id` counter,
    /// `notification_handlers`, and `server_request_handlers` as the original.
    /// This allows a read loop started with an `Arc<JsonRpcClient>` to resolve
    /// responses issued by a second client that owns the value, since both
    /// clients operate on the same pending map.
    ///
    /// This is the canonical pattern for wiring `McpProtocol` (which takes
    /// `JsonRpcClient` by value) with `start_read_loop` (which takes
    /// `Arc<JsonRpcClient>`):
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::mpsc;
    /// use tokio_util::sync::CancellationToken;
    /// use xzatoma::mcp::client::{JsonRpcClient, start_read_loop};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
    /// let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
    /// let token = CancellationToken::new();
    ///
    /// let shared = Arc::new(JsonRpcClient::new(out_tx));
    /// start_read_loop(in_rx, token, Arc::clone(&shared));
    ///
    /// // proto_client shares all Arcs with `shared`; responses resolved by
    /// // the read loop via `shared` are visible to `proto_client.request()`.
    /// let proto_client = shared.clone_shared();
    /// # }
    /// ```
    pub fn clone_shared(&self) -> Self {
        Self {
            next_id: Arc::clone(&self.next_id),
            pending: Arc::clone(&self.pending),
            outbound_tx: self.outbound_tx.clone(),
            notification_handlers: Arc::clone(&self.notification_handlers),
            server_request_handlers: Arc::clone(&self.server_request_handlers),
        }
    }

    /// Send a JSON-RPC request and await the typed response.
    ///
    /// Assigns the next monotonic ID, serializes the request, sends it on the
    /// outbound channel, and waits for the matching response with an optional
    /// timeout.
    ///
    /// # Arguments
    ///
    /// * `method` - The JSON-RPC method name.
    /// * `params` - Parameters to serialize into the `params` field.
    /// * `timeout` - Optional timeout; defaults to [`DEFAULT_REQUEST_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpTransport`] if the outbound channel is closed.
    /// Returns [`XzatomaError::McpTimeout`] if no response arrives within the timeout.
    /// Returns [`XzatomaError::Mcp`] if the server returns an error response.
    /// Returns [`XzatomaError::Serialization`] if serialization or deserialization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    /// use xzatoma::mcp::types::PaginatedParams;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let (tx, _rx) = mpsc::unbounded_channel::<String>();
    ///     let client = Arc::new(JsonRpcClient::new(tx));
    ///     // In practice you'd also call start_read_loop and have a real transport.
    ///     Ok(())
    /// }
    /// ```
    pub async fn request<P, R>(
        &self,
        method: &str,
        params: P,
        timeout: Option<Duration>,
    ) -> Result<R>
    where
        P: serde::Serialize + Send,
        R: serde::de::DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        // Register the pending slot before sending so the response can never
        // arrive before we are ready to receive it.
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Serialize and send the request.
        let message = serde_json::to_string(&JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(id)),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
        })?;

        self.outbound_tx
            .send(message)
            .map_err(|_| XzatomaError::McpTransport("outbound channel closed".to_string()))?;

        // Await the response with a timeout.
        let deadline = timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT);
        let outcome =
            tokio::time::timeout(deadline, rx)
                .await
                .map_err(|_| XzatomaError::McpTimeout {
                    server: "(unknown)".to_string(),
                    method: method.to_string(),
                })?;

        // The oneshot was dropped (read loop exited) before a response arrived.
        let rpc_result = outcome.map_err(|_| {
            XzatomaError::McpTransport("read loop exited before response arrived".to_string())
        })?;

        // Promote a JSON-RPC error into an XzatomaError.
        let value = rpc_result.map_err(|e| XzatomaError::Mcp(e.message))?;

        // Deserialize the result into the caller's expected type.
        serde_json::from_value(value).map_err(|e| XzatomaError::Serialization(e).into())
    }

    /// Send a JSON-RPC notification (no response expected).
    ///
    /// Notifications have no `id` field and the server MUST NOT reply.
    ///
    /// # Arguments
    ///
    /// * `method` - The notification method name.
    /// * `params` - Parameters to serialize into the `params` field.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpTransport`] if the outbound channel is closed.
    /// Returns [`XzatomaError::Serialization`] if serialization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let (tx, _rx) = mpsc::unbounded_channel::<String>();
    ///     let client = JsonRpcClient::new(tx);
    ///     client.notify("notifications/initialized", serde_json::json!({}))?;
    ///     Ok(())
    /// }
    /// ```
    pub fn notify<P: serde::Serialize + Send>(&self, method: &str, params: P) -> Result<()> {
        let message = serde_json::to_string(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": serde_json::to_value(params)?
        }))?;

        self.outbound_tx
            .send(message)
            .map_err(|_| XzatomaError::McpTransport("outbound channel closed".to_string()))?;

        Ok(())
    }

    /// Register a handler for a server-sent notification.
    ///
    /// When the read loop receives a JSON-RPC message with a matching `method`
    /// and no `id` field, it calls `f` with the raw `params` value
    /// (`serde_json::Value::Null` when absent).
    ///
    /// Registering a second handler for the same method replaces the first.
    ///
    /// # Arguments
    ///
    /// * `method` - The notification method to listen for.
    /// * `f` - The callback to invoke.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::JsonRpcClient;
    /// use xzatoma::mcp::types::NOTIF_TOOLS_LIST_CHANGED;
    ///
    /// let (tx, _rx) = mpsc::unbounded_channel::<String>();
    /// let client = JsonRpcClient::new(tx);
    /// client.on_notification(NOTIF_TOOLS_LIST_CHANGED, |_params| {
    ///     // refresh tool list
    /// });
    /// ```
    pub fn on_notification(
        &self,
        method: impl Into<String>,
        f: impl Fn(serde_json::Value) + Send + Sync + 'static,
    ) {
        let method = method.into();
        let handlers = Arc::clone(&self.notification_handlers);
        // We use `try_lock` here because `on_notification` is sync; callers
        // must not call this while a competing lock is held on the same client.
        // In practice registration happens before `start_read_loop` is called.
        tokio::spawn(async move {
            handlers.lock().await.insert(method, Box::new(f));
        });
    }

    /// Register a handler for a server-initiated request.
    ///
    /// When the read loop receives a JSON-RPC message that has both `method`
    /// and `id` fields, it is a server-initiated request. The handler is called
    /// with the raw `params` value and its return value is sent back as the
    /// `result` field of a JSON-RPC response.
    ///
    /// Registering a second handler for the same method replaces the first.
    ///
    /// # Arguments
    ///
    /// * `method` - The request method to handle.
    /// * `f` - The async callback to invoke; returns the raw result value.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::sync::mpsc;
    /// use xzatoma::mcp::client::{BoxFuture, JsonRpcClient};
    /// use xzatoma::mcp::types::METHOD_SAMPLING_CREATE_MESSAGE;
    ///
    /// let (tx, _rx) = mpsc::unbounded_channel::<String>();
    /// let client = JsonRpcClient::new(tx);
    /// client.on_server_request(METHOD_SAMPLING_CREATE_MESSAGE, |params| {
    ///     Box::pin(async move {
    ///         serde_json::json!({ "role": "assistant", "content": { "type": "text", "text": "ok" }, "model": "mock", "stopReason": null })
    ///     })
    /// });
    /// ```
    pub fn on_server_request(
        &self,
        method: impl Into<String>,
        f: impl Fn(serde_json::Value) -> BoxFuture<'static, serde_json::Value> + Send + Sync + 'static,
    ) {
        let method = method.into();
        let handlers = Arc::clone(&self.server_request_handlers);
        tokio::spawn(async move {
            handlers.lock().await.insert(method, Box::new(f));
        });
    }
}

/// Start the JSON-RPC read loop as a background Tokio task.
///
/// The loop reads serialized JSON strings from `inbound_rx`, classifies each
/// message, and dispatches it:
///
/// - **Response** (has `"id"` and `"result"` or `"error"`): resolves the
///   matching pending [`oneshot`] sender.
/// - **Server-initiated request** (has `"id"` and `"method"`): calls the
///   registered handler and sends a `JsonRpcResponse` back on `outbound_tx`.
///   Responds with JSON-RPC `-32601 Method not found` when no handler is
///   registered.
/// - **Notification** (has `"method"` but no `"id"`): calls the registered
///   handler, if any. Unknown notifications are silently ignored.
///
/// On cancellation, all pending senders are dropped so that any in-flight
/// `request()` call receives a channel-closed error rather than blocking
/// indefinitely.
///
/// # Arguments
///
/// * `inbound_rx` - Receiver for inbound JSON-RPC message strings from the transport.
/// * `cancellation` - Token used to stop the loop gracefully.
/// * `client` - Shared reference to the client whose pending map to service.
///
/// # Returns
///
/// A [`tokio::task::JoinHandle`] for the background task.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use tokio::sync::mpsc;
/// use tokio_util::sync::CancellationToken;
/// use xzatoma::mcp::client::{start_read_loop, JsonRpcClient};
///
/// #[tokio::main]
/// async fn main() {
///     let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
///     let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
///     let token = CancellationToken::new();
///     let client = Arc::new(JsonRpcClient::new(out_tx));
///     let handle = start_read_loop(in_rx, token.clone(), Arc::clone(&client));
///     token.cancel();
///     handle.await.unwrap();
/// }
/// ```
pub fn start_read_loop(
    mut inbound_rx: mpsc::UnboundedReceiver<String>,
    cancellation: CancellationToken,
    client: Arc<JsonRpcClient>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;

                _ = cancellation.cancelled() => {
                    // Drop all pending senders so that callers receive a
                    // channel-closed error instead of waiting forever.
                    let mut pending = client.pending.lock().await;
                    pending.clear();
                    break;
                }

                maybe_msg = inbound_rx.recv() => {
                    let raw = match maybe_msg {
                        Some(s) => s,
                        None => {
                            // Inbound channel was closed; treat as cancellation.
                            let mut pending = client.pending.lock().await;
                            pending.clear();
                            break;
                        }
                    };

                    dispatch_message(&raw, &client).await;
                }
            }
        }
    })
}

/// Classify and dispatch a single inbound JSON string.
///
/// This is extracted from the loop body to keep `start_read_loop` readable and
/// to allow direct unit testing of the dispatch logic.
async fn dispatch_message(raw: &str, client: &Arc<JsonRpcClient>) {
    let value: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("MCP read loop: failed to parse inbound JSON: {e}");
            return;
        }
    };

    let has_id = value.get("id").is_some() && !value["id"].is_null();
    let has_method = value.get("method").is_some();
    let has_result = value.get("result").is_some();
    let has_error = value.get("error").is_some();

    if has_id && (has_result || has_error) && !has_method {
        // --- Response to a client-originated request ---
        handle_response(value, client).await;
    } else if has_id && has_method {
        // --- Server-initiated request ---
        handle_server_request(value, client).await;
    } else if has_method && !has_id {
        // --- Server-sent notification ---
        handle_notification(value, client).await;
    } else {
        tracing::debug!(
            "MCP read loop: received unclassifiable message; ignoring. \
             has_id={has_id} has_method={has_method} has_result={has_result} has_error={has_error}"
        );
    }
}

/// Resolve a pending request sender with the response value or error.
async fn handle_response(value: serde_json::Value, client: &Arc<JsonRpcClient>) {
    // Extract the numeric ID.
    let id_val = &value["id"];
    let id: u64 = if let Some(n) = id_val.as_u64() {
        n
    } else if let Some(s) = id_val.as_str() {
        match s.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                tracing::warn!("MCP read loop: response has non-integer id: {id_val}");
                return;
            }
        }
    } else {
        tracing::warn!("MCP read loop: response has non-integer id: {id_val}");
        return;
    };

    let tx = {
        let mut pending = client.pending.lock().await;
        pending.remove(&id)
    };

    let Some(tx) = tx else {
        tracing::debug!("MCP read loop: received response for unknown id {id}; ignoring");
        return;
    };

    let outcome: std::result::Result<serde_json::Value, JsonRpcError> =
        if let Some(error_val) = value.get("error") {
            match serde_json::from_value::<JsonRpcError>(error_val.clone()) {
                Ok(e) => Err(e),
                Err(_) => Err(JsonRpcError {
                    code: -32603,
                    message: format!("malformed error object: {error_val}"),
                    data: None,
                }),
            }
        } else {
            Ok(value
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        };

    // Ignore send errors: the caller may have already timed out.
    let _ = tx.send(outcome);
}

/// Call the registered server-request handler and send a response.
async fn handle_server_request(value: serde_json::Value, client: &Arc<JsonRpcClient>) {
    let method = match value.get("method").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => return,
    };
    let id = value.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let params = value
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    // Look up the handler while holding the lock, then drop the lock before
    // awaiting so we don't deadlock if the handler calls back into the client.
    let handler_future: Option<BoxFuture<'static, serde_json::Value>> = {
        let handlers = client.server_request_handlers.lock().await;
        handlers.get(&method).map(|h| h(params))
    };

    let (result_field, error_field): (Option<serde_json::Value>, Option<serde_json::Value>) =
        if let Some(future) = handler_future {
            let result = future.await;
            (Some(result), None)
        } else {
            // JSON-RPC -32601: Method not found
            let err = serde_json::json!({
                "code": -32601,
                "message": format!("Method not found: {method}")
            });
            (None, Some(err))
        };

    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: Some(id),
        result: result_field,
        error: error_field.map(|e| crate::mcp::types::JsonRpcError {
            code: e["code"].as_i64().unwrap_or(-32603),
            message: e["message"]
                .as_str()
                .unwrap_or("internal error")
                .to_string(),
            data: None,
        }),
    };

    if let Ok(serialized) = serde_json::to_string(&response) {
        let _ = client.outbound_tx.send(serialized);
    }
}

/// Call the registered notification handler.
async fn handle_notification(value: serde_json::Value, client: &Arc<JsonRpcClient>) {
    let method = match value.get("method").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => return,
    };
    let params = value
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let handlers = client.notification_handlers.lock().await;
    if let Some(handler) = handlers.get(&method) {
        handler(params);
    } else {
        tracing::debug!("MCP read loop: no handler for notification '{method}'; ignoring");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::time::Duration;

    /// Build an in-process client with both channel ends exposed.
    fn make_client() -> (
        Arc<JsonRpcClient>,
        mpsc::UnboundedReceiver<String>,
        mpsc::UnboundedSender<String>,
    ) {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
        let (in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let client = Arc::new(JsonRpcClient::new(out_tx));
        start_read_loop(in_rx, token, Arc::clone(&client));
        (client, out_rx, in_tx)
    }

    /// Drain the outbound channel without blocking.
    fn drain_outbound(rx: &mut mpsc::UnboundedReceiver<String>) -> Vec<String> {
        let mut msgs = Vec::new();
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
        }
        msgs
    }

    #[tokio::test]
    async fn test_request_resolves_with_correct_result() {
        let (client, mut out_rx, in_tx) = make_client();

        // Spawn a task that echoes a successful response back on the inbound channel.
        let in_tx_clone = in_tx.clone();
        tokio::spawn(async move {
            // Wait for the outbound request to appear so we know the id.
            tokio::time::sleep(Duration::from_millis(5)).await;
            let sent = out_rx.recv().await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&sent).unwrap();
            let id = req["id"].clone();

            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": [], "nextCursor": null }
            });
            in_tx_clone
                .send(serde_json::to_string(&response).unwrap())
                .unwrap();
        });

        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct ToolsListResult {
            tools: Vec<serde_json::Value>,
        }

        let result: Result<ToolsListResult> = client
            .request(
                "tools/list",
                serde_json::json!({}),
                Some(Duration::from_secs(5)),
            )
            .await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert_eq!(result.unwrap().tools, Vec::<serde_json::Value>::new());
    }

    #[tokio::test]
    async fn test_request_timeout_fires() {
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
        let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let client = Arc::new(JsonRpcClient::new(out_tx));
        start_read_loop(in_rx, token, Arc::clone(&client));

        // No response is ever sent; the request must time out.
        let result: Result<serde_json::Value> = client
            .request(
                "tools/list",
                serde_json::json!({}),
                Some(Duration::from_millis(50)),
            )
            .await;

        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("timeout") || err_str.contains("MCP timeout"),
            "unexpected error: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_notification_handler_called_for_matching_method() {
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
        let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let in_tx_clone = _in_tx.clone();
        let token = CancellationToken::new();
        let client = Arc::new(JsonRpcClient::new(out_tx));
        start_read_loop(in_rx, token, Arc::clone(&client));

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        client.on_notification("notifications/tools/listChanged", move |_params| {
            counter_clone.fetch_add(1, AtomicOrdering::SeqCst);
        });

        // Give the spawn inside on_notification a chance to complete.
        tokio::time::sleep(Duration::from_millis(20)).await;

        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/tools/listChanged"
        });
        in_tx_clone
            .send(serde_json::to_string(&notif).unwrap())
            .unwrap();

        // Allow the read loop to process the notification.
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert_eq!(
            counter.load(AtomicOrdering::SeqCst),
            1,
            "handler should have been called exactly once"
        );
    }

    #[tokio::test]
    async fn test_pending_sender_dropped_cleanly_on_read_loop_exit() {
        let (out_tx, _out_rx) = mpsc::unbounded_channel::<String>();
        let (_in_tx, in_rx) = mpsc::unbounded_channel::<String>();
        let token = CancellationToken::new();
        let client = Arc::new(JsonRpcClient::new(out_tx));
        let handle = start_read_loop(in_rx, token.clone(), Arc::clone(&client));

        // Start a long-lived request (10 s timeout) before cancelling.
        let client_clone = Arc::clone(&client);
        let request_task = tokio::spawn(async move {
            let result: Result<serde_json::Value> = client_clone
                .request(
                    "tools/list",
                    serde_json::json!({}),
                    Some(Duration::from_secs(10)),
                )
                .await;
            result
        });

        // Give the request time to register in pending.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Cancel the loop.
        token.cancel();
        handle.await.unwrap();

        // The request should have resolved to an error, not hung.
        let outcome = tokio::time::timeout(Duration::from_secs(2), request_task)
            .await
            .expect("request task did not complete after loop exit")
            .expect("task panicked");

        assert!(
            outcome.is_err(),
            "expected an error after read loop exit, got Ok"
        );
    }

    #[tokio::test]
    async fn test_json_rpc_error_response_mapped_to_mcp_error() {
        let (client, mut out_rx, in_tx) = make_client();

        let in_tx_clone = in_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let sent = out_rx.recv().await.unwrap();
            let req: serde_json::Value = serde_json::from_str(&sent).unwrap();
            let id = req["id"].clone();

            let response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": "Method not found"
                }
            });
            in_tx_clone
                .send(serde_json::to_string(&response).unwrap())
                .unwrap();
        });

        let result: Result<serde_json::Value> = client
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
            "unexpected error string: {err_str}"
        );
    }

    #[tokio::test]
    async fn test_notify_sends_without_id() {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        let client = JsonRpcClient::new(out_tx);

        client
            .notify("notifications/initialized", serde_json::json!({}))
            .unwrap();

        let raw = out_rx.recv().await.unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(val["method"], "notifications/initialized");
        assert!(val.get("id").is_none(), "notifications must not have an id");
    }

    #[tokio::test]
    async fn test_multiple_concurrent_requests_resolved_correctly() {
        let (client, mut out_rx, in_tx) = make_client();

        // Respond to every outbound request with a matching result.
        let in_tx_clone = in_tx.clone();
        tokio::spawn(async move {
            loop {
                match out_rx.recv().await {
                    None => break,
                    Some(raw) => {
                        let req: serde_json::Value = serde_json::from_str(&raw).unwrap();
                        if let Some(id) = req.get("id") {
                            if id.is_null() {
                                continue;
                            }
                            let resp = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": { "echo": id }
                            });
                            in_tx_clone
                                .send(serde_json::to_string(&resp).unwrap())
                                .unwrap();
                        }
                    }
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

        // Each response must echo a different ID.
        let ids: std::collections::HashSet<u64> = [r1.unwrap(), r2.unwrap(), r3.unwrap()]
            .into_iter()
            .map(|v| v["echo"].as_u64().unwrap())
            .collect();
        assert_eq!(ids.len(), 3, "each request should have a unique ID");
    }

    #[test]
    fn test_notify_returns_error_when_channel_closed() {
        let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
        // Drop the receiver immediately so the channel is closed.
        drop(out_rx);
        let client = JsonRpcClient::new(out_tx);
        let result = client.notify("test", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_drain_helper_works() {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        out_tx.send("a".to_string()).unwrap();
        out_tx.send("b".to_string()).unwrap();
        let msgs = drain_outbound(&mut out_rx);
        assert_eq!(msgs, vec!["a", "b"]);
    }
}
