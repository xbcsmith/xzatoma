//! In-process fake transport for MCP unit and integration tests
//!
//! This module provides [`FakeTransport`] and [`FakeTransportHandle`], an
//! in-process pair that replaces real network or process I/O in tests.
//!
//! # Usage
//!
//! Call [`FakeTransport::new`] to obtain a `(FakeTransport, FakeTransportHandle)`
//! pair. Wire the [`FakeTransport`] into the code under test. From the test
//! side, use the [`FakeTransportHandle`] to:
//!
//! - Read what the client sent: `handle.outbound_rx.recv().await`
//! - Inject server responses: `handle.inbound_tx.send(json_string)`
//!
//! Alternatively, call [`FakeTransport::inject_response`] directly on the
//! transport to push a [`serde_json::Value`] as a serialized inbound message.
//!
//! # Channel Wiring
//!
//! From the **client** perspective:
//!
//! - "outbound" = what the client *sends* = what the test reads via
//!   `handle.outbound_rx`.
//! - "inbound"  = what the client *receives* = what the test injects via
//!   `handle.inbound_tx`.
//!
//! ```text
//! client send() -----> outbound_tx -----> outbound_rx (handle reads)
//! handle inbound_tx -> inbound_tx  -----> inbound_rx  (client receive())
//! ```
//!
//! # Example
//!
//! ```
//! use xzatoma::mcp::transport::fake::{FakeTransport, FakeTransportHandle};
//! use xzatoma::mcp::transport::Transport;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let (transport, mut handle) = FakeTransport::new();
//!
//! // Client sends a message.
//! transport.send(r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#.to_string()).await.unwrap();
//!
//! // Test reads what was sent.
//! let sent = handle.outbound_rx.recv().await.unwrap();
//! assert!(sent.contains("ping"));
//!
//! // Test injects a server response.
//! handle.inbound_tx.send(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_string()).unwrap();
//!
//! // Client receives it.
//! use futures::StreamExt;
//! let received = transport.receive().next().await.unwrap();
//! assert!(received.contains("result"));
//! # }
//! ```

use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use tokio::sync::{mpsc, Mutex};

use crate::error::Result;
use crate::mcp::transport::Transport;

/// In-process fake transport for use in tests.
///
/// Implements the full [`Transport`] trait using in-memory channels, so tests
/// can drive the client without spawning real processes or making network
/// requests.
///
/// Create with [`FakeTransport::new`] to obtain both the transport and the
/// complementary [`FakeTransportHandle`].
#[derive(Debug)]
pub struct FakeTransport {
    /// Sender side for `send()` -- what the client writes goes here and the
    /// handle drains it via `outbound_rx`.
    outbound_tx: mpsc::UnboundedSender<String>,
    /// Shared receiver for the inbound channel -- populated by the handle's
    /// `inbound_tx`; exposed via `receive()`.
    inbound_rx: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    /// Sender used by `inject_response()` to push messages onto the inbound
    /// channel (same channel end as `handle.inbound_tx`).
    inbound_inject_tx: mpsc::UnboundedSender<String>,
}

impl FakeTransport {
    /// Create a new `(FakeTransport, FakeTransportHandle)` pair.
    ///
    /// Wire the [`FakeTransport`] into the code under test. Use the returned
    /// [`FakeTransportHandle`] from your test to observe outbound traffic and
    /// inject inbound responses.
    ///
    /// # Returns
    ///
    /// A tuple of `(FakeTransport, FakeTransportHandle)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::transport::fake::FakeTransport;
    ///
    /// let (transport, handle) = FakeTransport::new();
    /// ```
    pub fn new() -> (Self, FakeTransportHandle) {
        // Outbound: transport.send() -> handle.outbound_rx
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<String>();

        // Inbound: handle.inbound_tx -> transport.receive()
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>();

        let transport = Self {
            outbound_tx,
            inbound_rx: Arc::new(Mutex::new(inbound_rx)),
            // Clone so inject_response() can also write without the handle.
            inbound_inject_tx: inbound_tx.clone(),
        };

        let handle = FakeTransportHandle {
            outbound_rx,
            inbound_tx,
        };

        (transport, handle)
    }

    /// Inject a [`serde_json::Value`] as a server response.
    ///
    /// The value is serialized to a JSON string and pushed onto the inbound
    /// channel, so the next call to [`Transport::receive`] will yield it.
    ///
    /// # Arguments
    ///
    /// * `response` - The JSON-RPC response or notification to inject.
    ///
    /// # Panics
    ///
    /// Panics if the inbound channel has been closed (which only happens if
    /// all receivers are dropped before this call).
    pub fn inject_response(&self, response: serde_json::Value) {
        let serialized =
            serde_json::to_string(&response).expect("FakeTransport: failed to serialize response");
        self.inbound_inject_tx
            .send(serialized)
            .expect("FakeTransport: inbound channel closed before inject_response");
    }
}

/// The test-side handle for a [`FakeTransport`].
///
/// Use this to:
///
/// - Read messages the client under test sent: `outbound_rx.recv().await`
/// - Inject server responses the client will receive: `inbound_tx.send(...)`
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::transport::fake::FakeTransport;
/// use xzatoma::mcp::transport::Transport;
///
/// # #[tokio::main]
/// # async fn main() {
/// let (transport, mut handle) = FakeTransport::new();
///
/// // Inject a response before the client calls receive().
/// handle.inbound_tx.send(r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#.to_string()).unwrap();
///
/// // Client sends something.
/// transport.send("{}".to_string()).await.unwrap();
///
/// // Read what was sent.
/// let sent = handle.outbound_rx.recv().await.unwrap();
/// assert_eq!(sent, "{}");
/// # }
/// ```
#[derive(Debug)]
pub struct FakeTransportHandle {
    /// Receives messages that the client sent via [`Transport::send`].
    ///
    /// Drain this in tests to assert on outbound traffic.
    pub outbound_rx: mpsc::UnboundedReceiver<String>,
    /// Sends server responses into the client's [`Transport::receive`] stream.
    ///
    /// Push JSON-RPC strings here to simulate server replies.
    pub inbound_tx: mpsc::UnboundedSender<String>,
}

#[async_trait::async_trait]
impl Transport for FakeTransport {
    /// Record the outbound message so the test can read it via
    /// [`FakeTransportHandle::outbound_rx`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::XzatomaError::McpTransport`] if the outbound
    /// channel is closed (i.e. the [`FakeTransportHandle`] was dropped).
    async fn send(&self, message: String) -> Result<()> {
        self.outbound_tx.send(message).map_err(|e| {
            anyhow::anyhow!(crate::error::XzatomaError::McpTransport(format!(
                "FakeTransport outbound channel closed: {}",
                e
            )))
        })
    }

    /// Returns a stream of messages injected via
    /// [`FakeTransportHandle::inbound_tx`] or [`FakeTransport::inject_response`].
    fn receive(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>> {
        let rx = Arc::clone(&self.inbound_rx);
        Box::pin(futures::stream::unfold(rx, |rx| async move {
            let mut guard = rx.lock().await;
            let item = guard.recv().await?;
            drop(guard);
            Some((item, rx))
        }))
    }

    /// Always returns an empty stream (the fake transport has no stderr).
    fn receive_err(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>> {
        Box::pin(futures::stream::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use futures::StreamExt;

    /// `new()` constructs without panicking and the handle channels are open.
    #[test]
    fn test_new_succeeds() {
        let (_transport, _handle) = FakeTransport::new();
    }

    /// `send()` delivers the message to `handle.outbound_rx`.
    #[tokio::test]
    async fn test_send_delivers_to_handle_outbound_rx() {
        let (transport, mut handle) = FakeTransport::new();

        transport
            .send(r#"{"jsonrpc":"2.0","method":"ping"}"#.to_string())
            .await
            .unwrap();

        let received = tokio::time::timeout(Duration::from_secs(2), handle.outbound_rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        assert_eq!(received, r#"{"jsonrpc":"2.0","method":"ping"}"#);
    }

    /// `receive()` yields messages injected via `handle.inbound_tx`.
    #[tokio::test]
    async fn test_receive_yields_message_from_handle_inbound_tx() {
        let (transport, handle) = FakeTransport::new();

        handle
            .inbound_tx
            .send(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_string())
            .unwrap();

        let mut stream = transport.receive();
        let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out")
            .expect("stream ended");

        assert_eq!(msg, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    }

    /// `inject_response` serializes and delivers a `serde_json::Value`.
    #[tokio::test]
    async fn test_inject_response_serializes_value() {
        let (transport, _handle) = FakeTransport::new();

        transport.inject_response(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": { "status": "ok" }
        }));

        let mut stream = transport.receive();
        let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out")
            .expect("stream ended");

        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["result"]["status"], "ok");
    }

    /// `receive_err()` is always empty for the fake transport.
    ///
    /// `futures::stream::empty()` resolves immediately with `None`, so
    /// `timeout` returns `Ok(None)` rather than `Err(Elapsed)`.
    #[tokio::test]
    async fn test_receive_err_always_empty() {
        let (transport, _handle) = FakeTransport::new();

        let mut err_stream = transport.receive_err();
        let result = tokio::time::timeout(Duration::from_millis(50), err_stream.next()).await;
        // The empty stream resolves immediately with None (Ok(None)), or times
        // out (Err). Either way no diagnostic message must be present.
        match result {
            Ok(None) | Err(_) => {}
            Ok(Some(msg)) => {
                panic!("receive_err should yield no messages for FakeTransport, got: {msg:?}")
            }
        }
    }

    /// Multiple messages sent via `send()` all appear in `outbound_rx` in order.
    #[tokio::test]
    async fn test_send_multiple_messages_ordered() {
        let (transport, mut handle) = FakeTransport::new();

        for i in 0u32..3 {
            transport.send(format!("msg-{}", i)).await.unwrap();
        }

        for i in 0u32..3 {
            let msg = handle.outbound_rx.recv().await.unwrap();
            assert_eq!(msg, format!("msg-{}", i));
        }
    }

    /// Multiple injected responses arrive on `receive()` in order.
    #[tokio::test]
    async fn test_receive_multiple_messages_ordered() {
        let (transport, handle) = FakeTransport::new();

        for i in 0u32..3 {
            handle.inbound_tx.send(format!("resp-{}", i)).unwrap();
        }

        let mut stream = transport.receive();
        for i in 0u32..3 {
            let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
                .await
                .expect("timed out")
                .expect("stream ended");
            assert_eq!(msg, format!("resp-{}", i));
        }
    }

    /// `send()` returns an error when the handle is dropped (channel closed).
    #[tokio::test]
    async fn test_send_returns_error_when_handle_dropped() {
        let (transport, handle) = FakeTransport::new();
        drop(handle);

        let result = transport.send("test".to_string()).await;
        assert!(
            result.is_err(),
            "send should fail when handle outbound_rx is dropped"
        );
    }

    /// `FakeTransport` satisfies the `Transport` trait object bound.
    #[test]
    fn test_fake_transport_is_object_safe() {
        let (transport, _handle) = FakeTransport::new();
        let _boxed: Box<dyn Transport> = Box::new(transport);
    }

    /// `inject_response` and `handle.inbound_tx` both write to the same
    /// inbound channel; all messages arrive on `receive()`.
    #[tokio::test]
    async fn test_inject_response_and_handle_inbound_tx_share_channel() {
        let (transport, handle) = FakeTransport::new();

        // Inject via handle.
        handle
            .inbound_tx
            .send(r#"{"via":"handle"}"#.to_string())
            .unwrap();

        // Inject via transport convenience method.
        transport.inject_response(serde_json::json!({"via": "inject_response"}));

        let mut stream = transport.receive();

        let m1 = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out")
            .expect("stream ended");
        let m2 = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timed out")
            .expect("stream ended");

        // Both messages must arrive; order is insertion order.
        let v1: serde_json::Value = serde_json::from_str(&m1).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&m2).unwrap();
        assert_eq!(v1["via"], "handle");
        assert_eq!(v2["via"], "inject_response");
    }
}
