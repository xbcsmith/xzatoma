//! MCP transport abstraction and implementations
//!
//! This module defines the [`Transport`] trait that all MCP transport
//! implementations must satisfy. Concrete implementations live in
//! submodules:
//!
//! - [`stdio::StdioTransport`] -- spawns a child process and communicates
//!   over its stdin/stdout pipes (newline-delimited JSON).
//! - [`http::HttpTransport`] -- Streamable HTTP/SSE transport conforming to
//!   MCP protocol revision `2025-11-25`.
//! - [`fake::FakeTransport`] -- in-process fake used in tests (cfg(test)
//!   only).
//!
//! # Design
//!
//! The [`Transport`] trait is intentionally minimal: callers `send` a
//! serialized JSON-RPC string and `receive` a stream of serialized JSON-RPC
//! strings (one per logical message). Framing, session management, and
//! reconnection are the responsibility of each concrete implementation.
//!
//! The `receive_err` stream carries transport-level diagnostics (e.g. stderr
//! output from a child process). Per the MCP spec, diagnostic output MUST
//! NOT be treated as an error condition.
//!
//! # Canonical Import Path
//!
//! ```no_run
//! use xzatoma::mcp::transport::Transport;
//! ```

use std::pin::Pin;

use futures::Stream;

use crate::error::Result;

/// Abstraction over MCP transport implementations.
///
/// Implementations exist for stdio (child process) and Streamable HTTP.
/// A [`fake::FakeTransport`] is provided for tests.
///
/// All methods are `async` or return pinned [`Stream`]s so that transport
/// implementations can drive I/O without blocking the Tokio executor.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::transport::Transport;
///
/// // Implementations are created via their own constructors; this trait is
/// // used polymorphically through `Arc<dyn Transport>`.
/// ```
#[async_trait::async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Send a complete JSON-RPC message string to the remote peer.
    ///
    /// The string MUST be a single, complete JSON object. The transport is
    /// responsible for any framing required by the underlying medium (e.g.
    /// appending a newline for stdio, or issuing an HTTP POST for SSE).
    ///
    /// # Arguments
    ///
    /// * `message` - A serialized JSON-RPC 2.0 message (request,
    ///   notification, or response).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::XzatomaError::McpTransport`] if the
    /// underlying I/O operation fails.
    async fn send(&self, message: String) -> Result<()>;

    /// Returns a stream of inbound JSON-RPC message strings.
    ///
    /// Each item in the stream is a single, complete JSON object with
    /// leading/trailing whitespace stripped. The stream ends when the
    /// transport is closed or the remote peer disconnects.
    ///
    /// # Returns
    ///
    /// A pinned, `Send`-safe [`Stream`] of `String` values.
    fn receive(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>;

    /// Returns a stream of transport-level diagnostic strings.
    ///
    /// For stdio transports this carries lines written to the child process's
    /// stderr. For HTTP transports this stream may be empty.
    ///
    /// Per the MCP specification, stderr output from a server subprocess is
    /// diagnostic only and MUST NOT be treated as an error condition.
    ///
    /// # Returns
    ///
    /// A pinned, `Send`-safe [`Stream`] of `String` values.
    fn receive_err(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>;
}

pub mod http;
pub mod stdio;

#[cfg(test)]
pub mod fake;
