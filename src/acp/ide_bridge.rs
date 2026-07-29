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
//! use agent_client_protocol::schema::v1 as acp;
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

use acp_sdk::schema::v1 as acp;
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
/// use agent_client_protocol::schema::v1 as acp;
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

/// Stable option id sent to the client for the "allow once" permission choice.
const PERMISSION_OPTION_ALLOW_ONCE: &str = "allow-once";
/// Stable option id sent to the client for the "allow for this session" choice.
const PERMISSION_OPTION_ALLOW_ALWAYS: &str = "allow-always";
/// Stable option id sent to the client for the "reject once" permission choice.
const PERMISSION_OPTION_REJECT_ONCE: &str = "reject-once";
/// Stable option id sent to the client for the "reject for this session" choice.
const PERMISSION_OPTION_REJECT_ALWAYS: &str = "reject-always";

/// Outcome of an IDE permission request presented to the Zed user.
///
/// The bridge maps the ACP `session/request_permission` response onto this small,
/// transport-independent enum so that callers do not need to depend on the ACP
/// SDK's `RequestPermissionOutcome` shape. Both "allow" choices map to
/// [`PermissionDecision::Approved`] and both "reject" choices map to
/// [`PermissionDecision::Denied`]; an interrupted prompt turn maps to
/// [`PermissionDecision::Cancelled`].
///
/// # Examples
///
/// ```
/// use xzatoma::acp::ide_bridge::PermissionDecision;
///
/// assert!(PermissionDecision::Approved.approved());
/// assert!(!PermissionDecision::Denied.approved());
/// assert!(!PermissionDecision::Cancelled.approved());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// The user approved the operation ("allow once" or "allow always").
    Approved,
    /// The user rejected the operation ("reject once" or "reject always").
    Denied,
    /// The prompt turn was cancelled before the user responded.
    Cancelled,
}

impl PermissionDecision {
    /// Returns `true` only when the decision is [`PermissionDecision::Approved`].
    ///
    /// This helper collapses the three-state decision into the single boolean
    /// the agent tools need in order to fail closed on anything but an explicit
    /// approval.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::ide_bridge::PermissionDecision;
    ///
    /// assert!(PermissionDecision::Approved.approved());
    /// assert!(!PermissionDecision::Denied.approved());
    /// assert!(!PermissionDecision::Cancelled.approved());
    /// ```
    pub fn approved(&self) -> bool {
        matches!(self, PermissionDecision::Approved)
    }
}

/// Maps an ACP permission outcome onto a [`PermissionDecision`].
///
/// This is a pure function with no I/O so that the option-id mapping logic can
/// be unit-tested without a live ACP connection. A `Selected` outcome whose
/// option id matches one of the two "allow" ids becomes
/// [`PermissionDecision::Approved`]; any other selected id (including the two
/// "reject" ids and unknown ids) becomes [`PermissionDecision::Denied`]. A
/// `Cancelled` outcome becomes [`PermissionDecision::Cancelled`].
///
/// # Arguments
///
/// * `outcome` - The outcome returned by the client's `session/request_permission`.
///
/// # Returns
///
/// Returns the mapped [`PermissionDecision`].
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema::v1 as acp;
/// use xzatoma::acp::ide_bridge::{decision_from_outcome, PermissionDecision};
///
/// let outcome = acp::RequestPermissionOutcome::Cancelled;
/// assert_eq!(decision_from_outcome(&outcome), PermissionDecision::Cancelled);
/// ```
pub fn decision_from_outcome(outcome: &acp::RequestPermissionOutcome) -> PermissionDecision {
    match outcome {
        acp::RequestPermissionOutcome::Cancelled => PermissionDecision::Cancelled,
        acp::RequestPermissionOutcome::Selected(selected) => {
            let option_id = selected.option_id.0.as_ref();
            if option_id == PERMISSION_OPTION_ALLOW_ONCE
                || option_id == PERMISSION_OPTION_ALLOW_ALWAYS
            {
                PermissionDecision::Approved
            } else {
                PermissionDecision::Denied
            }
        }
        // `RequestPermissionOutcome` is `#[non_exhaustive]`; treat any future
        // variant as a denial so the agent fails closed.
        _ => PermissionDecision::Denied,
    }
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
    /// use agent_client_protocol::schema::v1 as acp;
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
    /// use agent_client_protocol::schema::v1 as acp;
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
    /// use agent_client_protocol::schema::v1 as acp;
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

    /// Requests user approval for an operation via the ACP `session/request_permission`
    /// client request and returns the user's decision.
    ///
    /// The bridge presents four options to the Zed user (Allow once, Allow for the
    /// rest of the session, Reject once, Reject for the rest of the session) and
    /// maps the selected option onto a [`PermissionDecision`] using the pure
    /// [`decision_from_outcome`] function. The `operation` becomes the tool call
    /// title; `details` is attached to the tool call `raw_input` as a JSON object
    /// (`{ "operation": ..., "details": ... }`) so the client can render it.
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - A unique id identifying this permission tool call.
    /// * `operation` - Short human-readable description shown as the prompt title.
    /// * `details` - Longer explanation of what the operation does.
    ///
    /// # Returns
    ///
    /// Returns the [`PermissionDecision`] chosen by the user.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Internal` if the spawned task fails to join or if the
    /// ACP request returns an error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use agent_client_protocol::{Client as AcpClientRole, ConnectionTo};
    /// use agent_client_protocol::schema::v1 as acp;
    /// use xzatoma::acp::ide_bridge::{IdeBridge, IdeCapabilities};
    ///
    /// # async fn example(
    /// #     connection: ConnectionTo<AcpClientRole>,
    /// #     session_id: acp::SessionId,
    /// # ) -> anyhow::Result<()> {
    /// let bridge = IdeBridge::new(connection, session_id, IdeCapabilities::none());
    /// let decision = bridge
    ///     .request_permission("ide-permission-1", "Delete build cache", "Removes target/")
    ///     .await?;
    /// if decision.approved() {
    ///     println!("user approved");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn request_permission(
        &self,
        tool_call_id: &str,
        operation: &str,
        details: &str,
    ) -> Result<PermissionDecision> {
        let connection = self.connection.clone();
        let session_id = self.session_id.clone();
        let tool_call_id = tool_call_id.to_string();
        let operation = operation.to_string();
        let details = details.to_string();

        let response = tokio::task::spawn(async move {
            let options = vec![
                acp::PermissionOption::new(
                    PERMISSION_OPTION_ALLOW_ONCE,
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    PERMISSION_OPTION_ALLOW_ALWAYS,
                    "Allow for this session",
                    acp::PermissionOptionKind::AllowAlways,
                ),
                acp::PermissionOption::new(
                    PERMISSION_OPTION_REJECT_ONCE,
                    "Reject",
                    acp::PermissionOptionKind::RejectOnce,
                ),
                acp::PermissionOption::new(
                    PERMISSION_OPTION_REJECT_ALWAYS,
                    "Reject for this session",
                    acp::PermissionOptionKind::RejectAlways,
                ),
            ];

            let raw_input = serde_json::json!({
                "operation": operation,
                "details": details,
            });

            let fields = acp::ToolCallUpdateFields::new()
                .kind(acp::ToolKind::Other)
                .title(operation)
                .raw_input(raw_input);

            let tool_call = acp::ToolCallUpdate::new(tool_call_id, fields);
            let request = acp::RequestPermissionRequest::new(session_id, tool_call, options);

            connection
                .send_request_to(AcpClientRole, request)
                .block_task()
                .await
        })
        .await
        .map_err(|join_err| {
            XzatomaError::Internal(format!("IDE bridge task failed: {}", join_err))
        })?
        .map_err(|acp_err| {
            XzatomaError::Internal(format!("IDE request_permission failed: {}", acp_err))
        })?;

        Ok(decision_from_outcome(&response.outcome))
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
    use acp_sdk::schema::v1 as acp;

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

    fn selected_outcome(option_id: &str) -> acp::RequestPermissionOutcome {
        acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
            option_id.to_string(),
        ))
    }

    #[test]
    fn test_decision_from_outcome_allow_once_returns_approved() {
        let outcome = selected_outcome("allow-once");
        assert_eq!(
            decision_from_outcome(&outcome),
            PermissionDecision::Approved
        );
    }

    #[test]
    fn test_decision_from_outcome_allow_always_returns_approved() {
        let outcome = selected_outcome("allow-always");
        assert_eq!(
            decision_from_outcome(&outcome),
            PermissionDecision::Approved
        );
    }

    #[test]
    fn test_decision_from_outcome_reject_once_returns_denied() {
        let outcome = selected_outcome("reject-once");
        assert_eq!(decision_from_outcome(&outcome), PermissionDecision::Denied);
    }

    #[test]
    fn test_decision_from_outcome_reject_always_returns_denied() {
        let outcome = selected_outcome("reject-always");
        assert_eq!(decision_from_outcome(&outcome), PermissionDecision::Denied);
    }

    #[test]
    fn test_decision_from_outcome_unknown_id_returns_denied() {
        let outcome = selected_outcome("some-unexpected-id");
        assert_eq!(decision_from_outcome(&outcome), PermissionDecision::Denied);
    }

    #[test]
    fn test_decision_from_outcome_cancelled_returns_cancelled() {
        let outcome = acp::RequestPermissionOutcome::Cancelled;
        assert_eq!(
            decision_from_outcome(&outcome),
            PermissionDecision::Cancelled
        );
    }

    #[test]
    fn test_permission_decision_approved_is_true_only_for_approved() {
        assert!(PermissionDecision::Approved.approved());
        assert!(!PermissionDecision::Denied.approved());
        assert!(!PermissionDecision::Cancelled.approved());
    }
}
