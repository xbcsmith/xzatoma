//! Host-filtering HTTP CONNECT proxy for the XZatoma sandbox.
//!
//! This module provides a lightweight async HTTP CONNECT proxy that enforces
//! a per-connection allow-list before establishing upstream connections.  It is
//! used by the sandbox layer to let terminal and fetch tools reach only the
//! explicitly permitted hosts, while the kernel-level Landlock/Seatbelt policy
//! blocks all other outbound traffic.
//!
//! # Design
//!
//! [`SandboxProxy::start`] binds a `TcpListener` on `127.0.0.1` and spawns an
//! accept loop inside a Tokio task.  Each accepted connection is dispatched to
//! its own task via [`handle_connection`].  Before relaying data the handler:
//!
//! 1. Parses the HTTP `CONNECT` request line.
//! 2. Checks the target hostname against the configured allow-list.
//! 3. Validates the URL through [`crate::tools::fetch::SsrfValidator`] to
//!    reject private/loopback addresses even when they appear in the list.
//! 4. Connects to the upstream host and relays traffic bidirectionally.
//!
//! The proxy is stopped by calling [`SandboxProxy::shutdown`], which cancels
//! the accept loop through a `CancellationToken`.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

use crate::config::SandboxConfig;
use crate::error::SandboxError;

/// A running host-filtering HTTP CONNECT proxy.
///
/// Instances are created by [`SandboxProxy::start`] and stopped by
/// [`SandboxProxy::shutdown`].  Each instance owns its own `TcpListener`
/// and accept-loop task.  Multiple instances may run concurrently on
/// different ports.
pub struct SandboxProxy {
    /// The local address the proxy is listening on.
    addr: SocketAddr,
    /// Token used to signal the accept loop to stop.
    cancel: CancellationToken,
}

impl SandboxProxy {
    /// Bind a proxy listener and start the accept loop.
    ///
    /// Binds a `TcpListener` to `127.0.0.1:<cfg.proxy_port>`.  When
    /// `cfg.proxy_port` is `0` the OS assigns a free ephemeral port, which is
    /// useful in tests where a fixed port may already be in use.  The actual
    /// bound address is available via [`SandboxProxy::addr`].
    ///
    /// The accept loop is spawned as a Tokio background task and runs until
    /// [`SandboxProxy::shutdown`] is called.  Each accepted connection is
    /// further dispatched to an independent sub-task.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Sandbox configuration.  `proxy_port` determines the listen
    ///   port; `network_allowed_hosts` is the hostname allow-list enforced for
    ///   every `CONNECT` request.
    ///
    /// # Returns
    ///
    /// `Ok(SandboxProxy)` once the listener is bound and the accept loop is
    /// running.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::Engine`] when the OS refuses to bind the port
    /// (e.g. permission denied or port already in use).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # tokio_test::block_on(async {
    /// use xzatoma::config::SandboxConfig;
    /// use xzatoma::sandbox::proxy::SandboxProxy;
    ///
    /// let cfg = SandboxConfig {
    ///     proxy_port: 0,
    ///     network_allowed_hosts: vec!["example.com".to_string()],
    ///     ..Default::default()
    /// };
    /// let proxy = SandboxProxy::start(&cfg).await.unwrap();
    /// println!("proxy listening on {}", proxy.addr());
    /// proxy.shutdown();
    /// # })
    /// ```
    pub async fn start(cfg: &SandboxConfig) -> Result<Self, SandboxError> {
        let bind_addr = format!("127.0.0.1:{}", cfg.proxy_port);
        let listener = TcpListener::bind(&bind_addr).await.map_err(|e| {
            SandboxError::Engine(format!(
                "Failed to bind proxy listener at {}: {}",
                bind_addr, e
            ))
        })?;
        let addr = listener.local_addr().map_err(|e| {
            SandboxError::Engine(format!("Failed to obtain proxy local address: {}", e))
        })?;

        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let allowed_hosts = Arc::new(cfg.network_allowed_hosts.clone());

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _peer)) => {
                                let hosts = allowed_hosts.clone();
                                tokio::spawn(async move {
                                    handle_connection(stream, hosts).await;
                                });
                            }
                            Err(e) => {
                                tracing::warn!("sandbox proxy: accept error: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self { addr, cancel })
    }

    /// Return the local socket address the proxy is bound to.
    ///
    /// # Returns
    ///
    /// The `SocketAddr` on which the proxy is accepting `CONNECT` requests.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Signal the accept loop to stop.
    ///
    /// Sends a cancellation signal to the background accept task.  The task
    /// exits on its next `select!` iteration.  In-flight connection handlers
    /// are not forcibly terminated; they run to completion (or until the
    /// client/upstream closes the connection).
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

impl Drop for SandboxProxy {
    /// Cancel the accept loop when the proxy is dropped.
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Handle a single HTTP CONNECT tunnel request.
///
/// Reads and parses the `CONNECT` request from `stream`, enforces the
/// allow-list and SSRF policy, dials the upstream host, and then relays
/// traffic bidirectionally until either side closes the connection.
///
/// # Arguments
///
/// * `stream` - The accepted client-side TCP stream.
/// * `allowed_hosts` - Hostname allow-list from the sandbox configuration.
async fn handle_connection(mut stream: TcpStream, allowed_hosts: Arc<Vec<String>>) {
    // Read the HTTP CONNECT request (up to 4 KiB).
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf).await {
        Ok(0) | Err(_) => return,
        Ok(n) => n,
    };

    let request = match std::str::from_utf8(&buf[..n]) {
        Ok(s) => s,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    };

    // Parse first line: CONNECT host:port HTTP/1.x
    let first_line = match request.lines().next() {
        Some(line) => line.trim(),
        None => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    };

    let parts: Vec<&str> = first_line.splitn(3, ' ').collect();
    if parts.len() != 3 || parts[0] != "CONNECT" {
        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        return;
    }

    // Split "host:port" — rsplit_once handles hostnames that contain colons
    // (IPv6 literals) correctly: only the last colon is the port delimiter.
    let host_port = parts[1];
    let (host, port_str) = match host_port.rsplit_once(':') {
        Some(pair) => pair,
        None => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    };

    let port: u16 = match port_str.parse() {
        Ok(p) => p,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
            return;
        }
    };

    // Enforce the hostname allow-list.
    if !allowed_hosts.iter().any(|h| h == host) {
        let _ = stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n")
            .await;
        return;
    }

    // SSRF validation: reject loopback / RFC-1918 addresses even when the
    // caller placed them in the allow-list.
    let validator = crate::tools::fetch::SsrfValidator::new();
    if validator
        .validate(&format!("https://{}:{}", host, port))
        .is_err()
    {
        let _ = stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n")
            .await;
        return;
    }

    // Dial the upstream.
    let mut upstream = match TcpStream::connect((host, port)).await {
        Ok(s) => s,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            return;
        }
    };

    // Acknowledge the tunnel.
    if stream
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .await
        .is_err()
    {
        return;
    }

    // Relay traffic in both directions until either side closes.
    // Network resets during relay are normal; ignore the error.
    let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    fn test_sandbox_config(allowed_hosts: Vec<String>) -> crate::config::SandboxConfig {
        crate::config::SandboxConfig {
            enabled: true,
            network_enabled: true,
            network_allowed_hosts: allowed_hosts,
            proxy_port: 0,
            ..Default::default()
        }
    }

    /// Start the proxy with an empty allow-list; any CONNECT target must be
    /// rejected with 407.
    #[tokio::test]
    async fn test_proxy_blocks_unlisted_host() {
        let cfg = test_sandbox_config(vec![]);
        let proxy = SandboxProxy::start(&cfg).await.expect("proxy must start");

        let mut conn = TcpStream::connect(proxy.addr())
            .await
            .expect("must connect to proxy");
        conn.write_all(b"CONNECT evil.internal:443 HTTP/1.1\r\nHost: evil.internal\r\n\r\n")
            .await
            .expect("must write CONNECT request");

        let mut buf = [0u8; 512];
        let n = conn.read(&mut buf).await.expect("must read response");
        let response = std::str::from_utf8(&buf[..n]).unwrap_or("");

        proxy.shutdown();
        assert!(
            response.contains("407"),
            "expected 407 for unlisted host, got: {:?}",
            response
        );
    }

    /// Even when `localhost` is in the allow-list, the SSRF validator must
    /// still reject it with 407 because it resolves to a loopback address.
    #[tokio::test]
    async fn test_proxy_blocks_private_ip_even_if_listed() {
        let cfg = test_sandbox_config(vec!["localhost".to_string()]);
        let proxy = SandboxProxy::start(&cfg).await.expect("proxy must start");

        let mut conn = TcpStream::connect(proxy.addr())
            .await
            .expect("must connect to proxy");
        conn.write_all(b"CONNECT localhost:80 HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("must write CONNECT request");

        let mut buf = [0u8; 512];
        let n = conn.read(&mut buf).await.expect("must read response");
        let response = std::str::from_utf8(&buf[..n]).unwrap_or("");

        proxy.shutdown();
        assert!(
            response.contains("407"),
            "expected 407 for private/loopback host, got: {:?}",
            response
        );
    }

    /// When a public host appears in the allow-list, the proxy should return
    /// 200 and establish the tunnel.
    ///
    /// Gated with `#[ignore]` because it requires outbound network access.
    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_proxy_allows_listed_host() {
        let cfg = test_sandbox_config(vec!["example.com".to_string()]);
        let proxy = SandboxProxy::start(&cfg).await.expect("proxy must start");

        let mut conn = TcpStream::connect(proxy.addr())
            .await
            .expect("must connect to proxy");
        conn.write_all(b"CONNECT example.com:80 HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .expect("must write CONNECT request");

        let mut buf = [0u8; 512];
        let n = conn.read(&mut buf).await.expect("must read response");
        let response = std::str::from_utf8(&buf[..n]).unwrap_or("");

        proxy.shutdown();
        assert!(
            response.contains("200"),
            "expected 200 for listed public host, got: {:?}",
            response
        );
    }
}
