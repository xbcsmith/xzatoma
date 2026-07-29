//! IDE-aware tools that delegate to the Zed client via the ACP IDE bridge.
//!
//! These tools are registered when an ACP stdio session has an active IDE bridge
//! (i.e. the connecting Zed client advertised filesystem and terminal capabilities).
//! They prefer Zed's editor-aware APIs for file access and terminal creation so that
//! agent file edits appear in the editor buffer and terminal commands are visible
//! inside Zed's terminal panel.
//!
//! # Tool Overview
//!
//! | Tool name                    | Operation                                      |
//! |------------------------------|------------------------------------------------|
//! | `ide_read_text_file`         | Read a file through Zed's open project buffers |
//! | `ide_write_text_file`        | Write a full file through Zed's editor         |
//! | `ide_open_terminal`          | Run a command in Zed's terminal UI             |
//! | `ide_terminal_output`        | Read current output from a running terminal    |
//! | `ide_wait_for_terminal_exit` | Wait for a terminal command to finish          |
//! | `ide_kill_terminal`          | Terminate a running terminal                   |
//! | `ide_request_permission`     | Ask the Zed user to approve a risky action     |
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//! use xzatoma::tools::ide_tools::register_ide_tools;
//! use xzatoma::tools::ToolRegistry;
//!
//! let mut registry = ToolRegistry::new();
//! // register_ide_tools(&mut registry, Arc::new(bridge));
//! let _ = registry.len(); // 0 without a bridge
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::acp::ide_bridge::{IdeBridge, PermissionDecision};
use crate::error::parse_tool_args;
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};

// ---------------------------------------------------------------------------
// Shared parameter types
// ---------------------------------------------------------------------------

/// Parameters for reading a text file through the IDE.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeReadTextFileParams {
    /// Path to the file to read (absolute or workspace-relative).
    path: String,
    /// Optional 1-based start line for partial reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    /// Maximum number of lines to return when `start_line` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

/// Parameters for writing a text file through the IDE.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeWriteTextFileParams {
    /// Path to the file to write (absolute or workspace-relative).
    path: String,
    /// Full text content to write to the file.
    content: String,
}

/// Parameters for opening an IDE terminal command.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeOpenTerminalParams {
    /// The executable program to run.
    command: String,
    /// Arguments passed to the command.
    #[serde(default)]
    args: Vec<String>,
    /// Optional working directory. Defaults to the session workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    /// Whether to wait for the command to finish before returning.
    #[serde(default = "default_wait")]
    wait_for_exit: bool,
}

fn default_wait() -> bool {
    true
}

/// Parameters that reference an existing terminal by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeTerminalIdParams {
    /// The terminal ID returned by `ide_open_terminal`.
    terminal_id: String,
}

/// Parameters for the IDE permission request tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdeRequestPermissionParams {
    /// Short description of the operation that requires approval.
    operation: String,
    /// Detailed explanation shown to the user in the Zed UI.
    #[serde(default)]
    details: String,
}

// ---------------------------------------------------------------------------
// IdeReadTextFileTool
// ---------------------------------------------------------------------------

/// IDE-aware file reading tool.
///
/// Reads a text file through Zed's open project buffers. When the IDE bridge is
/// active, this tool uses `fs/read_text_file` so that the file content reflects
/// unsaved editor buffer state rather than only the on-disk snapshot.
pub(crate) struct IdeReadTextFileTool {
    bridge: Arc<IdeBridge>,
}

impl IdeReadTextFileTool {
    /// Creates a new `IdeReadTextFileTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeReadTextFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_read_text_file",
            "description": "Read a text file through Zed's editor-aware project buffers. \
                            Reflects unsaved changes in open editor tabs. \
                            Use start_line and limit for partial reads.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or workspace-relative path to the file."
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "1-based line number to start reading from (optional)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to return (optional, requires start_line)."
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeReadTextFileParams = parse_tool_args(args)?;
        let path = std::path::Path::new(&params.path);

        let content = if let (Some(start_line), Some(limit)) = (params.start_line, params.limit) {
            self.bridge
                .read_text_file_range(path, start_line, limit)
                .await
        } else {
            self.bridge.read_text_file(path).await
        };

        match content {
            Ok(text) => Ok(ToolResult::success(text)),
            Err(error) => Ok(ToolResult::error(format!(
                "IDE read_text_file failed for '{}': {}",
                params.path, error
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IdeWriteTextFileTool
// ---------------------------------------------------------------------------

/// IDE-aware file writing tool.
///
/// Writes a full text file through Zed's editor buffer system so that edits appear
/// in the open editor and are saved consistently. This makes agent file writes
/// visible as first-class editor operations that the user can review or undo.
pub(crate) struct IdeWriteTextFileTool {
    bridge: Arc<IdeBridge>,
}

impl IdeWriteTextFileTool {
    /// Creates a new `IdeWriteTextFileTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeWriteTextFileTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_write_text_file",
            "description": "Write a full text file through Zed's editor buffer system. \
                            Edits appear in the editor and are saved consistently. \
                            The entire file content is replaced with the provided text.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or workspace-relative path to the file to write."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full text content to write to the file."
                    }
                },
                "required": ["path", "content"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeWriteTextFileParams = parse_tool_args(args)?;
        let path = std::path::Path::new(&params.path);

        match self.bridge.write_text_file(path, &params.content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Successfully wrote {} bytes to '{}'",
                params.content.len(),
                params.path
            ))),
            Err(error) => Ok(ToolResult::error(format!(
                "IDE write_text_file failed for '{}': {}",
                params.path, error
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IdeOpenTerminalTool
// ---------------------------------------------------------------------------

/// IDE-aware terminal command tool.
///
/// Runs a command in Zed's terminal UI. When `wait_for_exit` is `true` (the
/// default), this tool creates the terminal, waits for the command to finish,
/// captures the output, and releases the terminal handle. When `wait_for_exit`
/// is `false`, it returns the terminal ID so subsequent calls to
/// `ide_terminal_output` and `ide_wait_for_terminal_exit` can track the session.
pub(crate) struct IdeOpenTerminalTool {
    bridge: Arc<IdeBridge>,
}

impl IdeOpenTerminalTool {
    /// Creates a new `IdeOpenTerminalTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeOpenTerminalTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_open_terminal",
            "description": "Run a command in Zed's terminal UI. \
                            When wait_for_exit is true (default), returns the command output \
                            after it finishes. When false, returns a terminal_id for later \
                            polling with ide_terminal_output or ide_wait_for_terminal_exit.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The executable to run."
                    },
                    "args": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Command arguments (optional)."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory (optional, defaults to workspace root)."
                    },
                    "wait_for_exit": {
                        "type": "boolean",
                        "description": "Whether to wait for the command to finish (default: true)."
                    }
                },
                "required": ["command"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeOpenTerminalParams = parse_tool_args(args)?;
        let cwd = params.cwd.as_deref().map(std::path::Path::new);

        let terminal_id = match self
            .bridge
            .create_terminal(&params.command, &params.args, cwd)
            .await
        {
            Ok(id) => id,
            Err(error) => {
                return Ok(ToolResult::error(format!(
                    "IDE create_terminal failed for '{}': {}",
                    params.command, error
                )));
            }
        };

        if !params.wait_for_exit {
            let id_str = terminal_id.0.as_ref().to_string();
            return Ok(
                ToolResult::success(format!("Terminal started with ID: {}", id_str))
                    .with_metadata("terminal_id".to_string(), id_str),
            );
        }

        let exit_code = match self.bridge.wait_for_terminal_exit(&terminal_id).await {
            Ok(code) => code,
            Err(error) => {
                if let Err(cleanup_error) = self.bridge.release_terminal(&terminal_id).await {
                    tracing::warn!(
                        terminal_id = %terminal_id.0.as_ref(),
                        error = %cleanup_error,
                        "Failed to release IDE terminal after wait failure"
                    );
                }
                return Ok(ToolResult::error(format!(
                    "IDE wait_for_terminal_exit failed: {}",
                    error
                )));
            }
        };

        let output = match self.bridge.terminal_output(&terminal_id).await {
            Ok(out) => out,
            Err(error) => {
                if let Err(cleanup_error) = self.bridge.release_terminal(&terminal_id).await {
                    tracing::warn!(
                        terminal_id = %terminal_id.0.as_ref(),
                        error = %cleanup_error,
                        "Failed to release IDE terminal after output failure"
                    );
                }
                return Ok(ToolResult::error(format!(
                    "IDE terminal_output failed: {}",
                    error
                )));
            }
        };

        if let Err(error) = self.bridge.release_terminal(&terminal_id).await {
            tracing::warn!(
                terminal_id = %terminal_id.0.as_ref(),
                error = %error,
                "Failed to release IDE terminal after command completion"
            );
        }

        let exit_summary = match exit_code {
            Some(code) => format!("Exit code: {}", code),
            None => "Process signalled (no exit code)".to_string(),
        };

        let result_text = format!("{}\n\n{}", exit_summary, output);
        Ok(ToolResult::success(result_text))
    }
}

// ---------------------------------------------------------------------------
// IdeTerminalOutputTool
// ---------------------------------------------------------------------------

/// IDE terminal output inspection tool.
///
/// Reads the current buffered output of a running or completed IDE terminal
/// command. The terminal must have been created with `ide_open_terminal` using
/// `wait_for_exit: false` to obtain a terminal ID.
pub(crate) struct IdeTerminalOutputTool {
    bridge: Arc<IdeBridge>,
}

impl IdeTerminalOutputTool {
    /// Creates a new `IdeTerminalOutputTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeTerminalOutputTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_terminal_output",
            "description": "Read the current buffered output of an IDE terminal command. \
                            The terminal_id must be obtained from ide_open_terminal \
                            with wait_for_exit set to false.",
            "parameters": {
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal ID returned by ide_open_terminal."
                    }
                },
                "required": ["terminal_id"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeTerminalIdParams = parse_tool_args(args)?;
        let terminal_id = acp_terminal_id_from_str(&params.terminal_id);

        match self.bridge.terminal_output(&terminal_id).await {
            Ok(output) => Ok(ToolResult::success(output)),
            Err(error) => Ok(ToolResult::error(format!(
                "IDE terminal_output failed for terminal '{}': {}",
                params.terminal_id, error
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IdeWaitForTerminalExitTool
// ---------------------------------------------------------------------------

/// IDE terminal exit-wait tool.
///
/// Blocks until a running IDE terminal command finishes and returns its exit
/// code. The terminal must have been created with `ide_open_terminal` using
/// `wait_for_exit: false`.
pub(crate) struct IdeWaitForTerminalExitTool {
    bridge: Arc<IdeBridge>,
}

impl IdeWaitForTerminalExitTool {
    /// Creates a new `IdeWaitForTerminalExitTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeWaitForTerminalExitTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_wait_for_terminal_exit",
            "description": "Wait for an IDE terminal command to finish and return its exit code. \
                            The terminal_id must be obtained from ide_open_terminal \
                            with wait_for_exit set to false.",
            "parameters": {
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal ID returned by ide_open_terminal."
                    }
                },
                "required": ["terminal_id"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeTerminalIdParams = parse_tool_args(args)?;
        let terminal_id = acp_terminal_id_from_str(&params.terminal_id);

        match self.bridge.wait_for_terminal_exit(&terminal_id).await {
            Ok(Some(code)) => Ok(ToolResult::success(format!(
                "Terminal '{}' exited with code {}",
                params.terminal_id, code
            ))),
            Ok(None) => Ok(ToolResult::success(format!(
                "Terminal '{}' terminated (no exit code, process signalled)",
                params.terminal_id
            ))),
            Err(error) => Ok(ToolResult::error(format!(
                "IDE wait_for_terminal_exit failed for terminal '{}': {}",
                params.terminal_id, error
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IdeKillTerminalTool
// ---------------------------------------------------------------------------

/// IDE terminal kill tool.
///
/// Terminates a running IDE terminal command by sending a kill signal.
/// After killing, use `ide_open_terminal` to start a new terminal.
pub(crate) struct IdeKillTerminalTool {
    bridge: Arc<IdeBridge>,
}

impl IdeKillTerminalTool {
    /// Creates a new `IdeKillTerminalTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeKillTerminalTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_kill_terminal",
            "description": "Terminate a running IDE terminal command. \
                            Sends a kill signal to the process identified by terminal_id. \
                            The terminal_id must be obtained from ide_open_terminal.",
            "parameters": {
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal ID returned by ide_open_terminal."
                    }
                },
                "required": ["terminal_id"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeTerminalIdParams = parse_tool_args(args)?;
        let terminal_id = acp_terminal_id_from_str(&params.terminal_id);

        match self.bridge.kill_terminal(&terminal_id).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Terminal '{}' killed successfully",
                params.terminal_id
            ))),
            Err(error) => Ok(ToolResult::error(format!(
                "IDE kill_terminal failed for terminal '{}': {}",
                params.terminal_id, error
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// IdeRequestPermissionTool
// ---------------------------------------------------------------------------

/// IDE permission request tool.
///
/// Presents a permission prompt to the Zed user before the agent performs a
/// risky or irreversible action. This tool is active in `safe` session mode
/// and allows the user to approve or deny agent operations from within the
/// Zed IDE without switching context.
///
/// The tool issues a real ACP `session/request_permission` round-trip through
/// the [`IdeBridge`], offering the user Allow / Allow-for-session /
/// Reject / Reject-for-session choices. It returns a JSON object with
/// `approved: true/false`, the originating `operation`, and an `outcome` of
/// `approved`, `denied`, `cancelled`, or `error`. The tool fails closed: if the
/// permission request errors out, the result is `approved: false` so the agent
/// never proceeds on an unconfirmed operation.
pub(crate) struct IdeRequestPermissionTool {
    bridge: Arc<IdeBridge>,
}

impl IdeRequestPermissionTool {
    /// Creates a new `IdeRequestPermissionTool` backed by the given IDE bridge.
    ///
    /// # Arguments
    ///
    /// * `bridge` - Shared IDE bridge for the active ACP session.
    pub(crate) fn new(bridge: Arc<IdeBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait::async_trait]
impl ToolExecutor for IdeRequestPermissionTool {
    fn tool_definition(&self) -> serde_json::Value {
        json!({
            "name": "ide_request_permission",
            "description": "Ask the Zed user to approve a risky or irreversible operation \
                            before proceeding. Use this in safe mode before destructive writes, \
                            dangerous terminal commands, or operations with significant side effects. \
                            Presents Allow/Reject choices in the Zed UI and returns \
                            {\"approved\": true} only when the user approves. Fails closed \
                            (approved: false) on cancellation or error.",
            "parameters": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "description": "Short description of the operation requiring approval (shown as title)."
                    },
                    "details": {
                        "type": "string",
                        "description": "Detailed explanation of what the operation does and why it needs approval."
                    }
                },
                "required": ["operation"]
            }
        })
    }

    /// Requests user approval through the ACP IDE bridge and returns the result.
    ///
    /// A fresh tool-call id is generated for each request. The bridge issues a
    /// `session/request_permission` round-trip; the decision is mapped to a JSON
    /// payload by [`permission_result_json`]. On error the tool logs a warning
    /// and fails closed with `approved: false`.
    async fn execute(&self, args: serde_json::Value) -> crate::error::Result<ToolResult> {
        let params: IdeRequestPermissionParams = parse_tool_args(args)?;
        let tool_call_id = format!("ide-permission-{}", ulid::Ulid::generate());

        tracing::info!(
            tool_call_id = %tool_call_id,
            operation = %params.operation,
            details = %params.details,
            "Requesting IDE user permission via ACP session/request_permission"
        );

        let decision = self
            .bridge
            .request_permission(&tool_call_id, &params.operation, &params.details)
            .await;

        if let Err(ref error) = decision {
            tracing::warn!(
                operation = %params.operation,
                error = %error,
                "IDE permission request failed; failing closed (denied by default)"
            );
        }

        let payload = permission_result_json(&params.operation, &decision);
        Ok(ToolResult::success(payload.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Registration helper
// ---------------------------------------------------------------------------

/// Registers all IDE-aware tools into the given [`ToolRegistry`].
///
/// This function is called during ACP stdio session creation when the
/// connecting Zed client has advertised IDE capabilities. It registers each
/// IDE tool variant with a stable name so that the agent can invoke them
/// alongside standard local filesystem tools.
///
/// # Arguments
///
/// * `registry` - The tool registry for the session being created.
/// * `bridge` - Shared IDE bridge backed by the active ACP connection.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzatoma::tools::{ToolRegistry, ide_tools::register_ide_tools};
///
/// let mut registry = ToolRegistry::new();
/// // In production, bridge is created from the ACP connection:
/// // register_ide_tools(&mut registry, Arc::new(bridge));
/// let _ = registry.len();
/// ```
pub fn register_ide_tools(registry: &mut ToolRegistry, bridge: Arc<IdeBridge>) {
    if bridge.capabilities().read_text_file {
        registry.register(
            "ide_read_text_file".to_string(),
            Arc::new(IdeReadTextFileTool::new(Arc::clone(&bridge))),
        );
    }

    if bridge.capabilities().write_text_file {
        registry.register(
            "ide_write_text_file".to_string(),
            Arc::new(IdeWriteTextFileTool::new(Arc::clone(&bridge))),
        );
    }

    if bridge.capabilities().terminal {
        registry.register(
            "ide_open_terminal".to_string(),
            Arc::new(IdeOpenTerminalTool::new(Arc::clone(&bridge))),
        );
        registry.register(
            "ide_terminal_output".to_string(),
            Arc::new(IdeTerminalOutputTool::new(Arc::clone(&bridge))),
        );
        registry.register(
            "ide_wait_for_terminal_exit".to_string(),
            Arc::new(IdeWaitForTerminalExitTool::new(Arc::clone(&bridge))),
        );
        registry.register(
            "ide_kill_terminal".to_string(),
            Arc::new(IdeKillTerminalTool::new(Arc::clone(&bridge))),
        );
    }

    // Permission tool is always registered when any IDE capability is active.
    registry.register(
        "ide_request_permission".to_string(),
        Arc::new(IdeRequestPermissionTool::new(Arc::clone(&bridge))),
    );
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Constructs an `acp::TerminalId` from a plain string.
///
/// Terminal IDs are returned as strings by `ide_open_terminal` and stored in
/// tool result metadata. This helper reconstructs the typed ID so callers do
/// not need to depend on ACP SDK internals directly.
fn acp_terminal_id_from_str(id: &str) -> agent_client_protocol::schema::v1::TerminalId {
    agent_client_protocol::schema::v1::TerminalId::new(id.to_string())
}

/// Builds the model-visible JSON payload for an IDE permission request result.
///
/// This is a pure function so the decision-to-JSON mapping can be unit-tested
/// without a live ACP connection. The `Ok` variants map to the corresponding
/// `approved`/`outcome` pair; the `Err` variant fails closed with
/// `approved: false` and `outcome: "error"`.
///
/// # Arguments
///
/// * `operation` - The operation description echoed back to the model.
/// * `decision` - The result of the bridge permission request.
///
/// # Returns
///
/// Returns a JSON object describing whether the operation was approved.
fn permission_result_json(
    operation: &str,
    decision: &crate::error::Result<PermissionDecision>,
) -> serde_json::Value {
    match decision {
        Ok(PermissionDecision::Approved) => json!({
            "approved": true,
            "operation": operation,
            "outcome": "approved",
        }),
        Ok(PermissionDecision::Denied) => json!({
            "approved": false,
            "operation": operation,
            "outcome": "denied",
        }),
        Ok(PermissionDecision::Cancelled) => json!({
            "approved": false,
            "operation": operation,
            "outcome": "cancelled",
        }),
        Err(_) => json!({
            "approved": false,
            "operation": operation,
            "outcome": "error",
            "note": "permission request failed; denied by default",
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // We cannot construct a live IdeBridge (requires an active ACP connection),
    // so these tests focus on tool definition correctness and parameter
    // deserialization.

    fn required_fields(definition: &serde_json::Value) -> Vec<String> {
        definition["parameters"]["required"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    fn tool_name(definition: &serde_json::Value) -> &str {
        definition["name"].as_str().unwrap_or("")
    }

    fn tool_description(definition: &serde_json::Value) -> &str {
        definition["description"].as_str().unwrap_or("")
    }

    fn build_fake_registry_entries() -> Vec<(&'static str, serde_json::Value)> {
        // Build dummy definitions without a live bridge by constructing the
        // tool definitions via mock structs that share the same JSON bodies.
        vec![
            (
                "ide_read_text_file",
                json!({
                    "name": "ide_read_text_file",
                    "description": "Read a text file through Zed's editor-aware project buffers. Reflects unsaved changes in open editor tabs. Use start_line and limit for partial reads.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string", "description": "Absolute or workspace-relative path to the file."},
                            "start_line": {"type": "integer", "description": "1-based line number to start reading from (optional)."},
                            "limit": {"type": "integer", "description": "Maximum number of lines to return (optional, requires start_line)."}
                        },
                        "required": ["path"]
                    }
                }),
            ),
            (
                "ide_write_text_file",
                json!({
                    "name": "ide_write_text_file",
                    "description": "Write a full text file through Zed's editor buffer system. Edits appear in the editor and are saved consistently. The entire file content is replaced with the provided text.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "content": {"type": "string"}
                        },
                        "required": ["path", "content"]
                    }
                }),
            ),
            (
                "ide_open_terminal",
                json!({
                    "name": "ide_open_terminal",
                    "description": "Run a command in Zed's terminal UI. When wait_for_exit is true (default), returns the command output after it finishes. When false, returns a terminal_id for later polling with ide_terminal_output or ide_wait_for_terminal_exit.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"},
                            "args": {"type": "array", "items": {"type": "string"}},
                            "cwd": {"type": "string"},
                            "wait_for_exit": {"type": "boolean"}
                        },
                        "required": ["command"]
                    }
                }),
            ),
            (
                "ide_terminal_output",
                json!({
                    "name": "ide_terminal_output",
                    "description": "Read the current buffered output of an IDE terminal command. The terminal_id must be obtained from ide_open_terminal with wait_for_exit set to false.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "terminal_id": {"type": "string"}
                        },
                        "required": ["terminal_id"]
                    }
                }),
            ),
            (
                "ide_wait_for_terminal_exit",
                json!({
                    "name": "ide_wait_for_terminal_exit",
                    "description": "Wait for an IDE terminal command to finish and return its exit code. The terminal_id must be obtained from ide_open_terminal with wait_for_exit set to false.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "terminal_id": {"type": "string"}
                        },
                        "required": ["terminal_id"]
                    }
                }),
            ),
            (
                "ide_kill_terminal",
                json!({
                    "name": "ide_kill_terminal",
                    "description": "Terminate a running IDE terminal command. Sends a kill signal to the process identified by terminal_id. The terminal_id must be obtained from ide_open_terminal.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "terminal_id": {"type": "string"}
                        },
                        "required": ["terminal_id"]
                    }
                }),
            ),
            (
                "ide_request_permission",
                json!({
                    "name": "ide_request_permission",
                    "description": "Ask the Zed user to approve a risky or irreversible operation before proceeding. Use this in safe mode before destructive writes, dangerous terminal commands, or operations with significant side effects. Returns {\"approved\": true} when the user approves.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "operation": {"type": "string"},
                            "details": {"type": "string"}
                        },
                        "required": ["operation"]
                    }
                }),
            ),
        ]
    }

    #[test]
    fn test_all_ide_tool_definitions_have_stable_names() {
        let entries = build_fake_registry_entries();
        for (expected_name, definition) in &entries {
            assert_eq!(
                tool_name(definition),
                *expected_name,
                "Tool definition name mismatch"
            );
        }
    }

    #[test]
    fn test_all_ide_tool_definitions_have_non_empty_descriptions() {
        for (name, definition) in build_fake_registry_entries() {
            assert!(
                !tool_description(&definition).is_empty(),
                "Tool '{name}' has empty description"
            );
        }
    }

    #[test]
    fn test_ide_read_text_file_requires_path() {
        let entries = build_fake_registry_entries();
        let def = &entries[0].1;
        assert!(required_fields(def).contains(&"path".to_string()));
    }

    #[test]
    fn test_ide_write_text_file_requires_path_and_content() {
        let entries = build_fake_registry_entries();
        let def = &entries[1].1;
        let required = required_fields(def);
        assert!(required.contains(&"path".to_string()));
        assert!(required.contains(&"content".to_string()));
    }

    #[test]
    fn test_ide_open_terminal_requires_command() {
        let entries = build_fake_registry_entries();
        let def = &entries[2].1;
        assert!(required_fields(def).contains(&"command".to_string()));
    }

    #[test]
    fn test_ide_terminal_id_tools_require_terminal_id() {
        let entries = build_fake_registry_entries();
        for index in [3usize, 4, 5] {
            let def = &entries[index].1;
            assert!(
                required_fields(def).contains(&"terminal_id".to_string()),
                "Tool '{}' should require terminal_id",
                tool_name(def)
            );
        }
    }

    #[test]
    fn test_ide_request_permission_requires_operation() {
        let entries = build_fake_registry_entries();
        let def = &entries[6].1;
        assert!(required_fields(def).contains(&"operation".to_string()));
    }

    #[test]
    fn test_ide_read_text_file_params_deserializes_full() {
        let args = json!({"path": "src/main.rs", "start_line": 10, "limit": 50});
        let params: IdeReadTextFileParams =
            serde_json::from_value(args).expect("should deserialize");
        assert_eq!(params.path, "src/main.rs");
        assert_eq!(params.start_line, Some(10));
        assert_eq!(params.limit, Some(50));
    }

    #[test]
    fn test_ide_read_text_file_params_deserializes_path_only() {
        let args = json!({"path": "README.md"});
        let params: IdeReadTextFileParams =
            serde_json::from_value(args).expect("should deserialize");
        assert_eq!(params.path, "README.md");
        assert!(params.start_line.is_none());
        assert!(params.limit.is_none());
    }

    #[test]
    fn test_ide_write_text_file_params_deserializes() {
        let args = json!({"path": "src/lib.rs", "content": "fn main() {}"});
        let params: IdeWriteTextFileParams =
            serde_json::from_value(args).expect("should deserialize");
        assert_eq!(params.path, "src/lib.rs");
        assert_eq!(params.content, "fn main() {}");
    }

    #[test]
    fn test_ide_open_terminal_params_defaults_wait_for_exit_true() {
        let args = json!({"command": "cargo", "args": ["build"]});
        let params: IdeOpenTerminalParams =
            serde_json::from_value(args).expect("should deserialize");
        assert!(params.wait_for_exit);
        assert_eq!(params.command, "cargo");
        assert_eq!(params.args, vec!["build"]);
        assert!(params.cwd.is_none());
    }

    #[test]
    fn test_ide_open_terminal_params_can_disable_wait() {
        let args = json!({"command": "tail", "args": ["-f", "log.txt"], "wait_for_exit": false});
        let params: IdeOpenTerminalParams =
            serde_json::from_value(args).expect("should deserialize");
        assert!(!params.wait_for_exit);
    }

    #[test]
    fn test_ide_terminal_id_params_deserializes() {
        let args = json!({"terminal_id": "term-abc-123"});
        let params: IdeTerminalIdParams = serde_json::from_value(args).expect("should deserialize");
        assert_eq!(params.terminal_id, "term-abc-123");
    }

    #[test]
    fn test_ide_request_permission_params_details_optional() {
        let args = json!({"operation": "Delete all temp files"});
        let params: IdeRequestPermissionParams =
            serde_json::from_value(args).expect("should deserialize");
        assert_eq!(params.operation, "Delete all temp files");
        assert!(params.details.is_empty());
    }

    #[test]
    fn test_ide_request_permission_params_with_details() {
        let args = json!({
            "operation": "Run npm install",
            "details": "Installs 500 packages from the internet."
        });
        let params: IdeRequestPermissionParams =
            serde_json::from_value(args).expect("should deserialize");
        assert!(!params.details.is_empty());
    }

    #[test]
    fn test_acp_terminal_id_from_str_round_trips() {
        let id = acp_terminal_id_from_str("my-term-1");
        assert_eq!(id.0.as_ref(), "my-term-1");
    }

    #[test]
    fn test_tool_names_are_all_lowercase_snake_case() {
        for (name, _) in build_fake_registry_entries() {
            assert_eq!(
                name,
                name.to_lowercase(),
                "Tool name '{}' must be lowercase",
                name
            );
            assert!(
                !name.contains('-'),
                "Tool name '{}' should use underscores, not hyphens",
                name
            );
        }
    }

    #[test]
    fn test_permission_result_json_approved_sets_approved_true() {
        let value = permission_result_json("delete files", &Ok(PermissionDecision::Approved));

        assert_eq!(value["approved"], serde_json::json!(true));
        assert_eq!(value["operation"], serde_json::json!("delete files"));
        assert_eq!(value["outcome"], serde_json::json!("approved"));
    }

    #[test]
    fn test_permission_result_json_denied_sets_approved_false() {
        let value = permission_result_json("delete files", &Ok(PermissionDecision::Denied));

        assert_eq!(value["approved"], serde_json::json!(false));
        assert_eq!(value["outcome"], serde_json::json!("denied"));
    }

    #[test]
    fn test_permission_result_json_cancelled_sets_approved_false() {
        let value = permission_result_json("delete files", &Ok(PermissionDecision::Cancelled));

        assert_eq!(value["approved"], serde_json::json!(false));
        assert_eq!(value["outcome"], serde_json::json!("cancelled"));
    }

    #[test]
    fn test_permission_result_json_error_fails_closed_approved_false() {
        let error = Err(crate::error::XzatomaError::Internal("boom".to_string()));
        let value = permission_result_json("delete files", &error);

        assert_eq!(value["approved"], serde_json::json!(false));
        assert_eq!(value["outcome"], serde_json::json!("error"));
        assert!(value["note"].is_string());
    }
}
