//! Streamable HTTP/SSE transport for MCP (`2025-11-25` specification)
//!
//! This module implements [`HttpTransport`], which speaks the `2025-11-25`
//! Streamable HTTP transport protocol. Every outbound JSON-RPC message is
//! sent as an HTTP POST. The server may reply with:
//!
//! - `application/json` -- a direct JSON response body
//! - `text/event-stream` -- an SSE stream carrying one or more JSON-RPC
//!   messages
//! - `202 Accepted` -- an acknowledgement with no body (used for
//!   notifications)
//!
//! An optional GET stream (`open_get_stream`) allows the server to push
//! unsolicited notifications via a long-lived SSE connection.
//!
//! # Session management
//!
//! After a successful `initialize` POST the server MAY return an
//! `MCP-Session-Id` response header. When present, this value is stored and
//! attached to every subsequent POST as `MCP-Session-Id: <id>`. If the
//! server returns `404` while a session is active the session is cleared and
//! `XzatomaError::Mcp("mcp session expired")` is returned.
//!
//! # Protocol version header
//!
//! Every POST MUST carry `MCP-Protocol-Version: 2025-11-25` per the spec.
//!
//! # Drop behaviour
//!
//! When the transport is dropped and a session ID is active, a synchronous
//! HTTP DELETE is issued to the endpoint with the `MCP-Session-Id` header.
//! This is spec-required session termination.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::Stream;
use tokio::sync::{mpsc, RwLock};

use crate::error::{Result, XzatomaError};
use crate::mcp::transport::Transport;

/// The mandatory MCP protocol version sent on every POST.
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Streamable HTTP/SSE transport implementing the `2025-11-25` MCP spec.
///
/// # Examples
///
/// ```no_run
/// use std::collections::HashMap;
/// use std::time::Duration;
/// use url::Url;
/// use xzatoma::mcp::transport::http::HttpTransport;
///
/// let transport = HttpTransport::new(
///     Url::parse("http://localhost:3000/mcp").unwrap(),
///     HashMap::new(),
///     Duration::from_secs(30),
/// );
/// ```
#[derive(Debug)]
pub struct HttpTransport {
    /// Underlying reqwest HTTP client.
    http_client: Arc<reqwest::Client>,
    /// MCP endpoint URL (POST target).
    endpoint: url::Url,
    /// Active session ID, populated after `initialize` succeeds.
    session_id: Arc<RwLock<Option<String>>>,
    /// Protocol version string, always `"2025-11-25"`.
    protocol_version: String,
    /// Static extra headers merged into every request (e.g. Authorization).
    headers: HashMap<String, String>,
    /// Sender for inbound JSON-RPC message strings.
    response_tx: mpsc::UnboundedSender<String>,
    /// Shared receiver exposed via `receive()`.
    response_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>>,
    /// Sender for transport-level error/diagnostic strings.
    error_tx: mpsc::UnboundedSender<String>,
    /// Shared receiver exposed via `receive_err()`.
    error_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<String>>>,
    /// Last SSE event ID, used for stream resumption via `Last-Event-ID`.
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl HttpTransport {
    /// Construct a new [`HttpTransport`] targeting `endpoint`.
    ///
    /// The `headers` map is merged into every outbound request; callers
    /// should inject bearer tokens or API keys here. The `timeout` applies
    /// to each individual HTTP request.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The MCP server URL (e.g. `http://host/mcp`).
    /// * `headers` - Extra headers added to every request. Auth tokens go
    ///   here.
    /// * `timeout` - Per-request timeout.
    ///
    /// # Returns
    ///
    /// A fully constructed [`HttpTransport`]. No network I/O is performed
    /// at construction time.
    pub fn new(endpoint: url::Url, headers: HashMap<String, String>, timeout: Duration) -> Self {
        let http_client = Arc::new(
            reqwest::Client::builder()
                .timeout(timeout)
                .build()
                // SAFETY: Default reqwest client construction cannot fail
                // unless TLS initialisation fails, which is a fatal startup
                // condition on any supported platform.
                .expect("failed to build reqwest client"),
        );

        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (error_tx, error_rx) = mpsc::unbounded_channel();

        Self {
            http_client,
            endpoint,
            session_id: Arc::new(RwLock::new(None)),
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            headers,
            response_tx,
            response_rx: Arc::new(tokio::sync::Mutex::new(response_rx)),
            error_tx,
            error_rx: Arc::new(tokio::sync::Mutex::new(error_rx)),
            last_event_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Open a long-lived SSE GET stream to receive unsolicited server
    /// notifications.
    ///
    /// Issues an HTTP GET to `self.endpoint` with `Accept: text/event-stream`
    /// and all session headers, then spawns a background Tokio task running
    /// [`parse_sse_stream`].  Returns immediately after spawning.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpTransport`] if the GET request itself
    /// fails before streaming begins.
    pub async fn open_get_stream(&self) -> Result<()> {
        let mut req = self
            .http_client
            .get(self.endpoint.as_str())
            .header("Accept", "text/event-stream");

        // Apply session header if we have one.
        {
            let sid = self.session_id.read().await;
            if let Some(ref id) = *sid {
                req = req.header("MCP-Session-Id", id.as_str());
            }
        }
        {
            let lei = self.last_event_id.read().await;
            if let Some(ref id) = *lei {
                req = req.header("Last-Event-ID", id.as_str());
            }
        }

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let response = req.send().await.map_err(|e| {
            anyhow::anyhow!(XzatomaError::McpTransport(format!(
                "GET stream request failed: {}",
                e
            )))
        })?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(XzatomaError::McpTransport(format!(
                "GET stream returned HTTP {}",
                status
            ))));
        }

        let byte_stream = response.bytes_stream();
        let response_tx = self.response_tx.clone();
        let last_event_id = Arc::clone(&self.last_event_id);

        tokio::spawn(async move {
            parse_sse_stream(byte_stream, response_tx, last_event_id).await;
        });

        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for HttpTransport {
    /// Send a JSON-RPC message via HTTP POST to the MCP endpoint.
    ///
    /// Mandatory headers on every POST:
    ///
    /// - `Content-Type: application/json`
    /// - `Accept: application/json, text/event-stream`
    /// - `MCP-Protocol-Version: 2025-11-25`
    /// - `MCP-Session-Id: <id>` -- only when a session is active
    /// - `Last-Event-ID: <id>` -- only when reconnecting with a known event ID
    ///
    /// Response handling by `Content-Type`:
    ///
    /// - `application/json`: body read and pushed to `receive()`.
    /// - `text/event-stream`: SSE parsing task spawned; events pushed to
    ///   `receive()`.
    /// - `202 Accepted`: no-op (notification ACK).
    /// - `401 Unauthorized`: returns `XzatomaError::McpAuth`.
    /// - `404` (with active session): clears session; returns
    ///   `XzatomaError::Mcp("mcp session expired")`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails, if the server returns
    /// `401`, or if a `404` is received while a session is active.
    async fn send(&self, message: String) -> Result<()> {
        let mut req = self
            .http_client
            .post(self.endpoint.as_str())
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.protocol_version)
            .body(message);

        // Attach session ID if present.
        {
            let sid = self.session_id.read().await;
            if let Some(ref id) = *sid {
                req = req.header("MCP-Session-Id", id.as_str());
            }
        }
        // Attach last SSE event ID for resumption.
        {
            let lei = self.last_event_id.read().await;
            if let Some(ref id) = *lei {
                req = req.header("Last-Event-ID", id.as_str());
            }
        }

        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        let response = req.send().await.map_err(|e| {
            anyhow::anyhow!(XzatomaError::McpTransport(format!(
                "HTTP POST failed: {}",
                e
            )))
        })?;

        let status = response.status();

        // Handle 401 Unauthorized: extract WWW-Authenticate and surface as
        // McpAuth.
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let www_auth = response
                .headers()
                .get("WWW-Authenticate")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            return Err(anyhow::anyhow!(XzatomaError::McpAuth(www_auth)));
        }

        // Handle 404: if we have an active session it has expired.
        if status == reqwest::StatusCode::NOT_FOUND {
            let has_session = {
                let sid = self.session_id.read().await;
                sid.is_some()
            };
            if has_session {
                let mut sid = self.session_id.write().await;
                *sid = None;
                return Err(anyhow::anyhow!(XzatomaError::Mcp(
                    "mcp session expired".into()
                )));
            }
            return Err(anyhow::anyhow!(XzatomaError::McpTransport(
                "HTTP 404 Not Found".into()
            )));
        }

        // 202 Accepted = notification acknowledgement, no body expected.
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(());
        }

        // Any other non-success status is a transport error.
        if !status.is_success() {
            return Err(anyhow::anyhow!(XzatomaError::McpTransport(format!(
                "HTTP POST returned status {}",
                status
            ))));
        }

        // Capture session ID from the response header after a successful
        // request (typically set on the `initialize` response).
        if let Some(new_session_id) = response
            .headers()
            .get("MCP-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
        {
            let mut sid = self.session_id.write().await;
            if sid.is_none() {
                *sid = Some(new_session_id);
            }
        }

        // Dispatch based on response Content-Type.
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("text/event-stream") {
            let byte_stream = response.bytes_stream();
            let response_tx = self.response_tx.clone();
            let last_event_id = Arc::clone(&self.last_event_id);
            tokio::spawn(async move {
                parse_sse_stream(byte_stream, response_tx, last_event_id).await;
            });
        } else {
            // application/json or any other content type: read the full body.
            let body = response.text().await.map_err(|e| {
                anyhow::anyhow!(XzatomaError::McpTransport(format!(
                    "failed to read response body: {}",
                    e
                )))
            })?;
            if !body.is_empty() {
                let _ = self.response_tx.send(body);
            }
        }

        Ok(())
    }

    /// Returns a stream of inbound JSON-RPC message strings.
    ///
    /// Messages are delivered in the order they are received, whether from
    /// direct JSON responses or SSE events.
    fn receive(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>> {
        let rx = Arc::clone(&self.response_rx);
        Box::pin(futures::stream::unfold(rx, |rx| async move {
            let mut guard = rx.lock().await;
            let item = guard.recv().await?;
            drop(guard);
            Some((item, rx))
        }))
    }

    /// Returns a stream of transport-level diagnostic / error strings.
    fn receive_err(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>> {
        let rx = Arc::clone(&self.error_rx);
        Box::pin(futures::stream::unfold(rx, |rx| async move {
            let mut guard = rx.lock().await;
            let item = guard.recv().await?;
            drop(guard);
            Some((item, rx))
        }))
    }
}

impl Drop for HttpTransport {
    /// Issue a synchronous DELETE to terminate the MCP session.
    ///
    /// If a session ID is active a `reqwest::blocking::Client` sends an
    /// HTTP DELETE with the `MCP-Session-Id` header per the spec. This is
    /// best-effort; failures are silently ignored because `drop` cannot
    /// return an error.
    fn drop(&mut self) {
        // Read session ID without blocking the async runtime. We use
        // `try_read` to avoid blocking; if the lock is contended we skip
        // cleanup (best-effort).
        let session_id = match self.session_id.try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => return,
        };

        if let Some(sid) = session_id {
            let endpoint = self.endpoint.as_str().to_string();
            let mut extra_headers = self.headers.clone();
            extra_headers.insert("MCP-Session-Id".to_string(), sid);

            // Synchronous DELETE -- spec-required session termination.
            // We spawn a new thread to avoid blocking the async runtime.
            let _ = std::thread::spawn(move || {
                if let Ok(client) = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                {
                    let mut req = client.delete(&endpoint);
                    for (k, v) in &extra_headers {
                        req = req.header(k.as_str(), v.as_str());
                    }
                    let _ = req.send();
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// SSE parser
// ---------------------------------------------------------------------------

/// Parse an SSE byte stream and forward complete `data:` events to
/// `response_tx`.
///
/// This function is `async` and is intended to be run inside a
/// `tokio::spawn`. It consumes the stream until it ends or an error occurs.
///
/// SSE field processing:
///
/// - `id:` -- stored in `last_event_id` for subsequent reconnect headers.
/// - `data: [PING]` (case-insensitive) or `event: ping` -- silently
///   discarded per the MCP spec.
/// - All other `data:` values -- pushed to `response_tx`.
/// - `retry:` -- parsed but currently unused (reconnect is the caller's
///   responsibility).
///
/// # Arguments
///
/// * `byte_stream` - The raw HTTP response body as a stream of byte chunks.
/// * `response_tx` - Channel to forward complete data payloads.
/// * `last_event_id` - Shared last-event-ID for SSE resumption.
pub async fn parse_sse_stream(
    byte_stream: impl Stream<Item = reqwest::Result<Bytes>>,
    response_tx: mpsc::UnboundedSender<String>,
    last_event_id: Arc<RwLock<Option<String>>>,
) {
    use futures::StreamExt;

    // Buffer accumulates raw bytes between `\n\n` boundaries.
    let mut buffer = String::new();

    tokio::pin!(byte_stream);

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(_) => break,
        };

        let text = match std::str::from_utf8(&chunk) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };

        buffer.push_str(&text);

        // SSE events are separated by blank lines (`\n\n`).
        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();
            process_sse_event(&event_block, &response_tx, &last_event_id).await;
        }
    }

    // Process any remaining partial event in the buffer.
    if !buffer.is_empty() {
        process_sse_event(&buffer, &response_tx, &last_event_id).await;
    }
}

/// Process a single SSE event block (the text between two `\n\n` delimiters).
///
/// # Arguments
///
/// * `event_block` - Raw SSE event text (may contain multiple field lines).
/// * `response_tx` - Channel to forward the parsed `data:` value.
/// * `last_event_id` - Updated when an `id:` field is present.
async fn process_sse_event(
    event_block: &str,
    response_tx: &mpsc::UnboundedSender<String>,
    last_event_id: &Arc<RwLock<Option<String>>>,
) {
    let mut data_lines: Vec<&str> = Vec::new();
    let mut event_type: Option<&str> = None;
    let mut event_id: Option<&str> = None;

    for line in event_block.lines() {
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim());
        } else if let Some(value) = line.strip_prefix("id:") {
            event_id = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("event:") {
            event_type = Some(value.trim());
        } else if line.starts_with("retry:") {
            // Ignored: reconnect timing is the caller's responsibility.
        }
        // Lines starting with `:` are SSE comments; all others are ignored.
    }

    // Store event ID for SSE resumption.
    if let Some(id) = event_id {
        let mut guard = last_event_id.write().await;
        *guard = Some(id.to_string());
    }

    // Discard ping events (spec-mandated silence).
    if let Some(et) = event_type {
        if et.eq_ignore_ascii_case("ping") {
            return;
        }
    }

    // Join multi-line data values.
    let data = data_lines.join("\n");

    // Discard [PING] data values.
    if data.eq_ignore_ascii_case("[ping]") || data.is_empty() {
        return;
    }

    let _ = response_tx.send(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio_stream::StreamExt as _;

    fn make_transport(endpoint: &str) -> HttpTransport {
        HttpTransport::new(
            url::Url::parse(endpoint).unwrap(),
            HashMap::new(),
            Duration::from_secs(5),
        )
    }

    /// `new()` constructs a transport without panicking.
    #[test]
    fn test_new_does_not_panic() {
        let t = make_transport("http://localhost:9999/mcp");
        assert_eq!(t.protocol_version, "2025-11-25");
    }

    /// `receive()` returns a stream that is initially empty.
    #[tokio::test]
    async fn test_receive_initially_empty() {
        let t = make_transport("http://localhost:9999/mcp");
        let mut stream = t.receive();
        let result = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
        assert!(result.is_err(), "expected timeout on empty receive stream");
    }

    /// `receive_err()` returns a stream that is initially empty.
    #[tokio::test]
    async fn test_receive_err_initially_empty() {
        let t = make_transport("http://localhost:9999/mcp");
        let mut stream = t.receive_err();
        let result = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
        assert!(result.is_err(), "expected timeout on empty error stream");
    }

    /// `parse_sse_stream` forwards a single `data:` event correctly.
    #[tokio::test]
    async fn test_parse_sse_single_data_event_forwarded() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let last_event_id = Arc::new(RwLock::new(None::<String>));

        let sse_body = b"data: {\"jsonrpc\":\"2.0\"}\n\n".to_vec();
        let chunk: reqwest::Result<Bytes> = Ok(Bytes::from(sse_body));
        let byte_stream = futures::stream::iter(vec![chunk]);

        parse_sse_stream(byte_stream, tx, Arc::clone(&last_event_id)).await;

        let msg = rx.try_recv().expect("expected a message");
        assert_eq!(msg, r#"{"jsonrpc":"2.0"}"#);
    }

    /// `parse_sse_stream` forwards two events from a single stream.
    #[tokio::test]
    async fn test_parse_sse_two_events_both_forwarded() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let last_event_id = Arc::new(RwLock::new(None::<String>));

        let sse_body = b"data: first\n\ndata: second\n\n".to_vec();
        let byte_stream = futures::stream::iter(vec![Ok(Bytes::from(sse_body))]);

        parse_sse_stream(byte_stream, tx, Arc::clone(&last_event_id)).await;

        let m1 = rx.try_recv().expect("expected first message");
        let m2 = rx.try_recv().expect("expected second message");
        assert_eq!(m1, "first");
        assert_eq!(m2, "second");
    }

    /// `parse_sse_stream` silently drops `event: ping` events.
    #[tokio::test]
    async fn test_parse_sse_ping_event_dropped() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let last_event_id = Arc::new(RwLock::new(None::<String>));

        let sse_body = b"event: ping\ndata: ignored\n\ndata: real\n\n".to_vec();
        let byte_stream = futures::stream::iter(vec![Ok(Bytes::from(sse_body))]);

        parse_sse_stream(byte_stream, tx, Arc::clone(&last_event_id)).await;

        let msg = rx.try_recv().expect("expected the real event");
        assert_eq!(msg, "real");
        assert!(rx.try_recv().is_err(), "no more events expected");
    }

    /// `parse_sse_stream` silently drops `data: [PING]` events.
    #[tokio::test]
    async fn test_parse_sse_data_ping_dropped() {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let last_event_id = Arc::new(RwLock::new(None::<String>));

        let sse_body = b"data: [PING]\n\ndata: real\n\n".to_vec();
        let byte_stream = futures::stream::iter(vec![Ok(Bytes::from(sse_body))]);

        parse_sse_stream(byte_stream, tx, Arc::clone(&last_event_id)).await;

        let msg = rx.try_recv().expect("expected the real event");
        assert_eq!(msg, "real");
        assert!(rx.try_recv().is_err(), "no more events expected");
    }

    /// `parse_sse_stream` stores `id:` field in `last_event_id`.
    #[tokio::test]
    async fn test_parse_sse_id_field_stored() {
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
        let last_event_id = Arc::new(RwLock::new(None::<String>));

        let sse_body = b"id: evt-42\ndata: payload\n\n".to_vec();
        let byte_stream = futures::stream::iter(vec![Ok(Bytes::from(sse_body))]);

        parse_sse_stream(byte_stream, tx, Arc::clone(&last_event_id)).await;

        let guard = last_event_id.read().await;
        assert_eq!(*guard, Some("evt-42".to_string()));
    }

    /// The session ID starts as `None` after construction.
    #[tokio::test]
    async fn test_session_id_initially_none() {
        let t = make_transport("http://localhost:9999/mcp");
        let sid = t.session_id.read().await;
        assert!(sid.is_none());
    }
}
