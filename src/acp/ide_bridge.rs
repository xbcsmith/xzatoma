//! IDE tool bridge over an ACP connection to a Zed client.
//!
//! This module wraps `ConnectionTo<AcpClientRole>` and exposes a clean async
//! interface for every Zed-provided client capability: file reads, file writes,
//! terminal creation, terminal I/O, and terminal lifecycle management.
//!
//! The bridge is intentionally thin. It does not cache, retry, or transform
//! responses beyond what is required to map ACP SDK errors to `XzatomaError`.
//!
//! # Examples
//!
//! ```no_run
//! use std::path::Path;
//! use agent_client_protocol::{Client as AcpClientRole, ConnectionTo};
//! use agent_client_protocol::schema as acp;
//! use xzatoma::acp::ide_bridge::{IdeBridge, IdeCapabilities};
//!
//! # async fn example(
//! #     connection: ConnectionTo<AcpClientRole>,
//! #     session_id: acp::SessionId,
//! # ) -> anyhow::Result<()> {
//! let caps = IdeCapabilities {
//!     read_text_file: true,
//!     write_text_file: true,
//!     terminal: false,
//! };
//!
//! let bridge = IdeBridge::new(connection, session_id, caps);
//! let content = bridge.read_text_file(Path::new("/workspace/main.rs")).await?;
//! println!("{}", content);
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};

use acp_sdk::schema as acp;
use agent_client_protocol::{self as acp_sdk, Client as AcpClientRole, ConnectionTo};

use crate::error::{Result, XzatomaError};

/// IDE tool bridge abstraction over an ACP connection to a Zed client.
///
/// The bridge holds a clone of the active `ConnectionTo<AcpClientRole>` and provides
/// async methods for every Zed-provided client capability: file reads, file writes,
/// terminal creation, terminal I/O, and lifecycle management.
///
/// When client capabilities do not include a requested feature, the bridge returns
/// a clear `XzatomaError::Internal` error explaining what is unavailable.
///
/// # Notes on Threading
///
/// `ConnectionTo<AcpClientRole>` is `Clone + Send` but not `Sync`. Each async
/// method clones the connection before spawning a `tokio::task::spawn` task, which
/// satisfies the `Send` bound required by the Tokio scheduler. The `block_task()`
/// call inside the spawned task is safe because it runs in a dedicated task that
/// does not block the ACP event loop.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use agent_client_protocol::{Client as AcpClientRole, ConnectionTo};
/// use agent_client_protocol::schema as acp;
/// use xzatoma::acp::ide_bridge::{IdeBridge, IdeCapabilities};
///
/// # async fn example(
/// #     connection: ConnectionTo<AcpClientRole>,
/// #     session_id: acp::SessionId,
/// # ) -> anyhow::Result<()> {
/// let caps = IdeCapabilities::none();
/// let bridge = IdeBridge::new(connection, session_id, caps);
/// assert!(!bridge.capabilities().read_text_file);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct IdeBridge {
    connection: ConnectionTo<AcpClientRole>,
    session_id: acp::SessionId,
    capabilities: IdeCapabilities,
}

/// Client capabilities snapshot taken at session creation.
///
/// This snapshot is derived once from the `ClientCapabilities` received during
/// the ACP `initialize` handshake and stored alongside the bridge. Individual
/// methods on `IdeBridge` consult these flags before issuing requests so that
/// callers receive a clear error rather than a confusing protocol failure.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::ide_bridge::IdeCapabilities;
///
/// let caps = IdeCapabilities::none();
/// assert!(!caps.read_text_file);
/// assert!(!caps.write_text_file);
/// assert!(!caps.terminal);
/// ```
#[derive(Debug, Clone)]
pub struct IdeCapabilities {
    /// Whether the Zed client supports `fs/read_text_file` requests.
    pub read_text_file: bool,
    /// Whether the Zed client supports `fs/write_text_file` requests.
    pub write_text_file: bool,
    /// Whether the Zed client supports all `terminal/*` requests.
    pub terminal: bool,
}

impl IdeCapabilities {
    /// Constructs an `IdeCapabilities` snapshot from the ACP `ClientCapabilities`
    /// advertised by the Zed client during initialization.
    ///
    /// # Arguments
    ///
    /// * `caps` - The `ClientCapabilities` received in the `initialize` handshake.
    ///
    /// # Returns
    ///
    /// Returns a new `IdeCapabilities` reflecting the client's declared feature set.
    ///
    /// # Examples
    ///
    /// ```
    /// use agent_client_protocol::schema as acp;
    /// use xzatoma::acp::ide_bridge::IdeCapabilities;
    ///
    /// let client_caps = acp::ClientCapabilities::new()
    ///     .fs(acp::FileSystemCapabilities::new()
    ///         .read_text_file(true)
    ///         .write_text_file(true))
    ///     .terminal(true);
    ///
    /// let caps = IdeCapabilities::from_client_capabilities(&client_caps);
    /// assert!(caps.read_text_file);
    /// assert!(caps.write_text_file);
    /// assert!(caps.terminal);
    /// ```
    pub fn from_client_capabilities(caps: &acp::ClientCapabilities) -> Self {
        Self {
            read_text_file: caps.fs.read_text_file,
            write_text_file: caps.fs.write_text_file,
            terminal: caps.terminal,
        }
    }

    /// Returns an `IdeCapabilities` with all capabilities disabled.
    ///
    /// Useful as a default when no `ClientCapabilities` are available or when
    /// constructing an `IdeBridge` in test contexts where no live connection exists.
    ///
    /// # Returns
    ///
    /// Returns an `IdeCapabilities` where every flag is `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::ide_bridge::IdeCapabilities;
    ///
    /// let caps = IdeCapabilities::none();
    /// assert!(!caps.read_text_file);
    /// assert!(!caps.write_text_file);
    /// assert!(!caps.terminal);
    /// ```
    pub fn none() -> Self {
        Self {
            read_text_file: false,
            write_text_file: false,
            terminal: false,
        }
    }
}

impl IdeBridge {
    /// Creates a new `IdeBridge` from a cloned ACP connection, session ID, and
    /// capability snapshot.
    ///
    /// # Arguments
    ///
    /// * `connection` - A clone of the active `ConnectionTo<AcpClientRole>`.
    /// * `session_id` - The ACP session ID used to scope all outgoing requests.
    /// * `capabilities` - Capability snapshot from the `initialize` handshake.
    ///
    /// # Returns
    ///
    /// Returns a new `IdeBridge` ready to issue requests.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agent_client_protocol::{Client as AcpClientRole, ConnectionTo};
    /// use agent_client_protocol::schema as acp;
    /// use xzatoma::acp::ide_bridge::{IdeBridge, IdeCapabilities};
    ///
    /// # fn example(connection: ConnectionTo<AcpClientRole>, session_id: acp::SessionId) {
    /// let bridge = IdeBridge::new(connection, session_id, IdeCapabilities::none());
    /// # }
    /// ```
    pub fn new(
        connection: ConnectionTo<AcpClientRole>,
        session_id: acp::SessionId,
        capabilities: IdeCapabilities,
    ) -> Self {
        Self {
            connection,
            session_id,
            capabilities,
        }
    }

    /// Returns a reference to the capability snapshot stored in this bridge.
    ///
    /// # Returns
    ///
    /// Returns a reference to the `IdeCapabilities` snapshot.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agent_client_protocol::{Client as AcpClientRole, ConnectionTo};
    /// use agent_client_protocol::schema as acp;
    /// use xzatoma::acp::ide_bridge::{IdeBridge, IdeCapabilities};
    ///
    /// # fn example(connection: ConnectionTo<AcpClientRole>, session_id: acp::SessionId) {
    /// let bridge = IdeBridge::new(connection, session_id, IdeCapabilities::none());
    /// assert!(!bridge.capabilities().terminal);
    /// # }
    /// ```
    pub fn capabilities(&self) -> &IdeCapabilities {
        &self.capabilities
    }

    /// Reads the full content of a text file from the Zed project via
    /// the `fs/read_text_file` client request.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file within the Zed project.
    ///
    /// # Returns
    ///
    /// Returns the file content as a `String` on success.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the client does not advertise
    /// the `read_text_file` capability, if the spawned task fails to join,
    /// or if the ACP request itself returns an error.
    pub async fn read_text_file(&self, path: &Path) -> Result<String> {
        if !self.capabilities.read_text_file {
            return Err(XzatomaError::Internal(
                "IDE client does not advertise read_text_file capability".to_string(),
            ));
        }

        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let path: PathBuf = path.to_path_buf();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::ReadTextFileRequest::new(session_id, path),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE read_text_file failed: {}", acp_err))
        })
        .map(|response| response.content)
    }

    /// Reads a range of lines from a text file in the Zed project.
    ///
    /// This is a ranged variant of `read_text_file`. It uses the `line` and
    /// `limit` parameters on `ReadTextFileRequest` to restrict output.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file within the Zed project.
    /// * `line` - 1-based line number to start reading from.
    /// * `limit` - Maximum number of lines to return.
    ///
    /// # Returns
    ///
    /// Returns the requested range of file content as a `String` on success.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the client does not advertise
    /// the `read_text_file` capability, if the spawned task fails to join,
    /// or if the ACP request returns an error.
    pub async fn read_text_file_range(&self, path: &Path, line: u32, limit: u32) -> Result<String> {
        if !self.capabilities.read_text_file {
            return Err(XzatomaError::Internal(
                "IDE client does not advertise read_text_file capability".to_string(),
            ));
        }

        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let path: PathBuf = path.to_path_buf();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::ReadTextFileRequest::new(session_id, path)
                        .line(line)
                        .limit(limit),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE read_text_file_range failed: {}", acp_err))
        })
        .map(|response| response.content)
    }

    /// Writes the full content of a text file through Zed's editor buffer system
    /// via the `fs/write_text_file` client request.
    ///
    /// # Arguments
    ///
    /// * `path` - Absolute path to the file to write within the Zed project.
    /// * `content` - Text content to write to the file.
    ///
    /// # Returns
    ///
    /// Returns `()` on success.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the client does not advertise
    /// the `write_text_file` capability, if the spawned task fails to join,
    /// or if the ACP request returns an error.
    pub async fn write_text_file(&self, path: &Path, content: &str) -> Result<()> {
        if !self.capabilities.write_text_file {
            return Err(XzatomaError::Internal(
                "IDE client does not advertise write_text_file capability".to_string(),
            ));
        }

        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let path: PathBuf = path.to_path_buf();
        let content = content.to_string();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::WriteTextFileRequest::new(session_id, path, content),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE write_text_file failed: {}", acp_err))
        })
        .map(|_response| ())
    }

    /// Creates a terminal in Zed's terminal UI via the `terminal/create` client request.
    ///
    /// # Arguments
    ///
    /// * `command` - The executable to run in the terminal.
    /// * `args` - Arguments to pass to the command.
    /// * `cwd` - Optional working directory for the command. Uses the Zed project
    ///   root if `None`.
    ///
    /// # Returns
    ///
    /// Returns the `TerminalId` assigned by Zed on success. This ID must be
    /// retained to interact with the terminal via subsequent bridge calls.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the client does not advertise the
    /// `terminal` capability, if the spawned task fails to join, or if the ACP
    /// request returns an error.
    pub async fn create_terminal(
        &self,
        command: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> Result<acp::TerminalId> {
        if !self.capabilities.terminal {
            return Err(XzatomaError::Internal(
                "IDE client does not advertise terminal capability".to_string(),
            ));
        }

        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let command = command.to_string();
        let args = args.to_vec();
        let cwd: Option<PathBuf> = cwd.map(|p| p.to_path_buf());

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::CreateTerminalRequest::new(session_id, command)
                        .args(args)
                        .cwd(cwd),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE create_terminal failed: {}", acp_err))
        })
        .map(|response| response.terminal_id)
    }

    /// Reads the current accumulated output of a Zed terminal via the
    /// `terminal/output` client request.
    ///
    /// # Arguments
    ///
    /// * `terminal_id` - The ID of the terminal to read output from.
    ///
    /// # Returns
    ///
    /// Returns all terminal output captured so far as a `String`.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the spawned task fails to join
    /// or if the ACP request returns an error.
    pub async fn terminal_output(&self, terminal_id: &acp::TerminalId) -> Result<String> {
        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let terminal_id = terminal_id.clone();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::TerminalOutputRequest::new(session_id, terminal_id),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE terminal_output failed: {}", acp_err))
        })
        .map(|response| response.output)
    }

    /// Blocks until the terminal command exits, returning the exit code via the
    /// `terminal/wait_for_exit` client request.
    ///
    /// # Arguments
    ///
    /// * `terminal_id` - The ID of the terminal to wait on.
    ///
    /// # Returns
    ///
    /// Returns `Some(exit_code)` if the process exited normally, or `None` if
    /// it was terminated by a signal.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the spawned task fails to join
    /// or if the ACP request returns an error.
    pub async fn wait_for_terminal_exit(
        &self,
        terminal_id: &acp::TerminalId,
    ) -> Result<Option<u32>> {
        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let terminal_id = terminal_id.clone();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::WaitForTerminalExitRequest::new(session_id, terminal_id),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE wait_for_terminal_exit failed: {}", acp_err))
        })
        .map(|response| response.exit_status.exit_code)
    }

    /// Sends SIGKILL (or the platform equivalent) to a running terminal via the
    /// `terminal/kill` client request.
    ///
    /// The terminal is not released after killing; call `release_terminal` when
    /// output has been collected.
    ///
    /// # Arguments
    ///
    /// * `terminal_id` - The ID of the terminal to kill.
    ///
    /// # Returns
    ///
    /// Returns `()` on success.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the spawned task fails to join
    /// or if the ACP request returns an error.
    pub async fn kill_terminal(&self, terminal_id: &acp::TerminalId) -> Result<()> {
        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let terminal_id = terminal_id.clone();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::KillTerminalRequest::new(session_id, terminal_id),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| XzatomaError::Internal(format!("IDE kill_terminal failed: {}", acp_err)))
        .map(|_response| ())
    }

    /// Releases a terminal and frees the resources held by Zed via the
    /// `terminal/release` client request.
    ///
    /// The terminal must be added to any `ToolCall` content blocks before being
    /// released, as Zed requires the terminal to be live during rendering.
    ///
    /// # Arguments
    ///
    /// * `terminal_id` - The ID of the terminal to release.
    ///
    /// # Returns
    ///
    /// Returns `()` on success.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the spawned task fails to join
    /// or if the ACP request returns an error.
    pub async fn release_terminal(&self, terminal_id: &acp::TerminalId) -> Result<()> {
        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let terminal_id = terminal_id.clone();

        tokio::task::spawn(async move {
            connection
                .send_request_to(
                    AcpClientRole,
                    acp::ReleaseTerminalRequest::new(session_id, terminal_id),
                )
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE release_terminal failed: {}", acp_err))
        })
        .map(|_response| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_sdk::schema as acp;

    // Bridge integration tests that require a live `ConnectionTo<AcpClientRole>` are
    // covered by in-memory protocol tests in `stdio.rs`. Unit tests here focus on
    // capability negotiation logic, which is testable without a live connection.

    fn all_true_client_capabilities() -> acp::ClientCapabilities {
        acp::ClientCapabilities::new()
            .fs(acp::FileSystemCapabilities::new()
                .read_text_file(true)
                .write_text_file(true))
            .terminal(true)
    }

    fn read_only_client_capabilities() -> acp::ClientCapabilities {
        acp::ClientCapabilities::new().fs(acp::FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(false))
    }

    fn terminal_only_client_capabilities() -> acp::ClientCapabilities {
        acp::ClientCapabilities::new().terminal(true)
    }

    fn empty_client_capabilities() -> acp::ClientCapabilities {
        acp::ClientCapabilities::new()
    }

    #[test]
    fn test_ide_capabilities_none_returns_all_false() {
        let caps = IdeCapabilities::none();

        assert!(!caps.read_text_file);
        assert!(!caps.write_text_file);
        assert!(!caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_from_client_capabilities_with_all_enabled() {
        let client_caps = all_true_client_capabilities();
        let caps = IdeCapabilities::from_client_capabilities(&client_caps);

        assert!(caps.read_text_file);
        assert!(caps.write_text_file);
        assert!(caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_from_client_capabilities_with_read_only_fs() {
        let client_caps = read_only_client_capabilities();
        let caps = IdeCapabilities::from_client_capabilities(&client_caps);

        assert!(caps.read_text_file);
        assert!(!caps.write_text_file);
        assert!(!caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_from_client_capabilities_with_terminal_only() {
        let client_caps = terminal_only_client_capabilities();
        let caps = IdeCapabilities::from_client_capabilities(&client_caps);

        assert!(!caps.read_text_file);
        assert!(!caps.write_text_file);
        assert!(caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_from_client_capabilities_with_empty() {
        let client_caps = empty_client_capabilities();
        let caps = IdeCapabilities::from_client_capabilities(&client_caps);

        assert!(!caps.read_text_file);
        assert!(!caps.write_text_file);
        assert!(!caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_from_client_capabilities_with_write_only_fs() {
        let client_caps = acp::ClientCapabilities::new().fs(acp::FileSystemCapabilities::new()
            .read_text_file(false)
            .write_text_file(true));
        let caps = IdeCapabilities::from_client_capabilities(&client_caps);

        assert!(!caps.read_text_file);
        assert!(caps.write_text_file);
        assert!(!caps.terminal);
    }

    #[test]
    fn test_ide_capabilities_clone_produces_independent_copy() {
        let original = IdeCapabilities {
            read_text_file: true,
            write_text_file: false,
            terminal: true,
        };
        let cloned = original.clone();

        assert_eq!(original.read_text_file, cloned.read_text_file);
        assert_eq!(original.write_text_file, cloned.write_text_file);
        assert_eq!(original.terminal, cloned.terminal);
    }

    #[test]
    fn test_ide_capabilities_debug_format_is_non_empty() {
        let caps = IdeCapabilities::none();
        let debug_str = format!("{:?}", caps);

        assert!(!debug_str.is_empty());
        assert!(debug_str.contains("IdeCapabilities"));
    }
}
