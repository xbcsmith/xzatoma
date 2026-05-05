//! ACP tool call notification builders for Zed IDE rich rendering.
//!
//! This module maps XZatoma tool execution events to the ACP `ToolCall` and
//! `ToolCallUpdate` types that Zed uses to render tool activity in the agent
//! panel. Each tool invocation produces two notifications:
//!
//! 1. A `ToolCall` sent at the start of execution (`build_tool_call_start`),
//!    advertising the tool kind, a human-readable title, and the raw input.
//! 2. A `ToolCallUpdate` sent on completion or failure (`build_tool_call_completion`
//!    or `build_tool_call_failure`), updating the status and recording output.
//!
//! Both are wrapped in a `SessionUpdate` and dispatched as a `SessionNotification`
//! to the Zed client over the active ACP connection.
//!
//! # Examples
//!
//! ```
//! use xzatoma::acp::tool_notifications::{
//!     build_tool_call_completion, build_tool_call_failure, build_tool_call_start,
//!     generate_tool_call_id, tool_kind_for_name,
//! };
//! use agent_client_protocol::schema as acp;
//!
//! let id = generate_tool_call_id();
//! let input = serde_json::json!({ "path": "/workspace/src/main.rs" });
//! let tool_call = build_tool_call_start(&id, "read_file", &input);
//!
//! assert_eq!(tool_call.status, acp::ToolCallStatus::InProgress);
//! assert_eq!(tool_call.kind, acp::ToolKind::Read);
//!
//! let output = serde_json::json!({ "content": "fn main() {}" });
//! let update = build_tool_call_completion(&id, &output);
//! assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Completed));
//! ```

use acp_sdk::schema as acp;
use agent_client_protocol as acp_sdk;
use uuid::Uuid;

/// Generates a stable unique tool call ID for a single tool invocation.
///
/// The format is `xzatoma-call-{uuid_v4_simple}` where the UUID uses the
/// compact hex representation (no hyphens). This prefix makes IDs recognisable
/// as originating from XZatoma in Zed's tool call panel.
///
/// # Returns
///
/// Returns a fresh `acp::ToolCallId` that is unique within the process lifetime.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::tool_notifications::generate_tool_call_id;
///
/// let id = generate_tool_call_id();
/// assert!(id.0.starts_with("xzatoma-call-"));
/// ```
pub fn generate_tool_call_id() -> acp::ToolCallId {
    let id = format!("xzatoma-call-{}", Uuid::new_v4().simple());
    acp::ToolCallId::new(id)
}

/// Maps an XZatoma tool name to an ACP `ToolKind` for Zed's display.
///
/// The mapping determines which icon and UI treatment Zed applies to the tool
/// call in the agent panel. Unknown tool names fall back to `ToolKind::Other`.
///
/// # Mapping
///
/// | Tool names | `ToolKind` |
/// |---|---|
/// | `read_file`, `list_directory`, `find_path`, `grep`, `file_metadata`, `ide_read_text_file` | `Read` |
/// | `write_file`, `edit_file`, `create_directory`, `copy_path`, `delete_path`, `move_path`, `ide_write_text_file` | `Edit` |
/// | `terminal`, `ide_open_terminal`, `ide_terminal_output`, `ide_wait_for_terminal_exit`, `ide_kill_terminal` | `Execute` |
/// | `fetch` | `Fetch` |
/// | `plan` | `Think` |
/// | `subagent`, `parallel_subagent`, `ide_request_permission` | `Other` |
/// | anything else | `Other` |
///
/// # Arguments
///
/// * `tool_name` - The canonical tool name used in the XZatoma tool registry.
///
/// # Returns
///
/// Returns the corresponding `acp::ToolKind` variant.
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema as acp;
/// use xzatoma::acp::tool_notifications::tool_kind_for_name;
///
/// assert_eq!(tool_kind_for_name("read_file"), acp::ToolKind::Read);
/// assert_eq!(tool_kind_for_name("edit_file"), acp::ToolKind::Edit);
/// assert_eq!(tool_kind_for_name("terminal"), acp::ToolKind::Execute);
/// assert_eq!(tool_kind_for_name("fetch"), acp::ToolKind::Fetch);
/// assert_eq!(tool_kind_for_name("plan"), acp::ToolKind::Think);
/// assert_eq!(tool_kind_for_name("unknown_tool"), acp::ToolKind::Other);
/// ```
pub fn tool_kind_for_name(tool_name: &str) -> acp::ToolKind {
    match tool_name {
        "read_file" | "list_directory" | "find_path" | "grep" | "file_metadata"
        | "ide_read_text_file" => acp::ToolKind::Read,

        "write_file"
        | "edit_file"
        | "create_directory"
        | "copy_path"
        | "delete_path"
        | "move_path"
        | "ide_write_text_file" => acp::ToolKind::Edit,

        "terminal"
        | "ide_open_terminal"
        | "ide_terminal_output"
        | "ide_wait_for_terminal_exit"
        | "ide_kill_terminal" => acp::ToolKind::Execute,

        "fetch" => acp::ToolKind::Fetch,

        "plan" => acp::ToolKind::Think,

        "subagent" | "parallel_subagent" | "ide_request_permission" => acp::ToolKind::Other,

        _ => acp::ToolKind::Other,
    }
}

/// Builds a human-readable title for a tool call given the tool name and raw input.
///
/// The title is shown in Zed's agent panel as the primary label for the tool call.
/// Where possible, the title includes the primary argument (path, URL, or command)
/// extracted from the input JSON so the user can identify which resource is being
/// accessed without expanding the detail view.
///
/// # Arguments
///
/// * `tool_name` - The canonical tool name used in the XZatoma tool registry.
/// * `input` - The raw JSON input parameters for the tool call.
///
/// # Returns
///
/// Returns a human-readable `String` title suitable for display in Zed's UI.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::tool_notifications::tool_call_title;
///
/// let input = serde_json::json!({ "path": "/workspace/main.rs" });
/// let title = tool_call_title("read_file", &input);
/// assert_eq!(title, "Reading file /workspace/main.rs");
///
/// let no_path_input = serde_json::json!({});
/// let fallback = tool_call_title("read_file", &no_path_input);
/// assert_eq!(fallback, "Reading file");
/// ```
pub fn tool_call_title(tool_name: &str, input: &serde_json::Value) -> String {
    let path = input
        .get("path")
        .or_else(|| input.get("file_path"))
        .and_then(|v| v.as_str());

    match tool_name {
        "read_file" | "ide_read_text_file" => match path {
            Some(p) => format!("Reading file {}", p),
            None => "Reading file".to_string(),
        },

        "write_file" | "ide_write_text_file" => match path {
            Some(p) => format!("Writing file {}", p),
            None => "Writing file".to_string(),
        },

        "edit_file" => match path {
            Some(p) => format!("Editing file {}", p),
            None => "Editing file".to_string(),
        },

        "list_directory" => match path {
            Some(p) => format!("Listing directory {}", p),
            None => "Listing directory".to_string(),
        },

        "find_path" => match input.get("glob").and_then(|v| v.as_str()) {
            Some(glob) => format!("Finding paths matching {}", glob),
            None => "Finding paths".to_string(),
        },

        "grep" => match input.get("regex").and_then(|v| v.as_str()) {
            Some(pattern) => format!("Searching files for {}", pattern),
            None => "Searching files".to_string(),
        },

        "file_metadata" => match path {
            Some(p) => format!("Reading metadata for {}", p),
            None => "Reading file metadata".to_string(),
        },

        "create_directory" => match path {
            Some(p) => format!("Creating directory {}", p),
            None => "Creating directory".to_string(),
        },

        "copy_path" => {
            let source = input.get("source_path").and_then(|v| v.as_str());
            match source {
                Some(src) => format!("Copying {}", src),
                None => "Copying path".to_string(),
            }
        }

        "delete_path" => match path {
            Some(p) => format!("Deleting {}", p),
            None => "Deleting path".to_string(),
        },

        "move_path" => {
            let source = input.get("source_path").and_then(|v| v.as_str());
            match source {
                Some(src) => format!("Moving {}", src),
                None => "Moving path".to_string(),
            }
        }

        "terminal" | "ide_open_terminal" => match input.get("command").and_then(|v| v.as_str()) {
            Some(cmd) => format!("Running terminal command: {}", cmd),
            None => "Running terminal command".to_string(),
        },

        "ide_terminal_output" => "Reading terminal output".to_string(),

        "ide_wait_for_terminal_exit" => "Waiting for terminal exit".to_string(),

        "ide_kill_terminal" => "Killing terminal".to_string(),

        "fetch" => match input.get("url").and_then(|v| v.as_str()) {
            Some(url) => format!("Fetching {}", url),
            None => "Fetching URL".to_string(),
        },

        "plan" => "Planning".to_string(),

        "subagent" | "parallel_subagent" => "Running subagent".to_string(),

        "ide_request_permission" => "Requesting permission".to_string(),

        _ => format!("Running tool: {}", tool_name),
    }
}

/// Builds the initial `ToolCall` notification for a tool being invoked.
///
/// The returned `ToolCall` is set to `ToolCallStatus::InProgress` and carries
/// the tool kind, human-readable title, raw input, and any file locations
/// extracted from the input. It should be sent to the Zed client as a
/// `SessionUpdate::ToolCall` immediately before tool execution begins.
///
/// # Arguments
///
/// * `id` - The unique tool call ID generated by `generate_tool_call_id`.
/// * `tool_name` - The canonical tool name used in the XZatoma tool registry.
/// * `input` - The raw JSON input parameters passed to the tool.
///
/// # Returns
///
/// Returns an `acp::ToolCall` ready for wrapping in a `SessionNotification`.
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema as acp;
/// use xzatoma::acp::tool_notifications::{build_tool_call_start, generate_tool_call_id};
///
/// let id = generate_tool_call_id();
/// let input = serde_json::json!({ "path": "/workspace/lib.rs" });
/// let tool_call = build_tool_call_start(&id, "read_file", &input);
///
/// assert_eq!(tool_call.status, acp::ToolCallStatus::InProgress);
/// assert_eq!(tool_call.kind, acp::ToolKind::Read);
/// assert!(tool_call.raw_input.is_some());
/// ```
pub fn build_tool_call_start(
    id: &acp::ToolCallId,
    tool_name: &str,
    input: &serde_json::Value,
) -> acp::ToolCall {
    let title = tool_call_title(tool_name, input);
    let kind = tool_kind_for_name(tool_name);
    let locations = build_tool_call_locations(tool_name, input);

    acp::ToolCall::new(id.clone(), title)
        .kind(kind)
        .status(acp::ToolCallStatus::InProgress)
        .raw_input(input.clone())
        .locations(locations)
}

/// Builds a `ToolCallUpdate` marking a tool call as successfully completed.
///
/// The returned update sets the status to `ToolCallStatus::Completed` and
/// records the raw output value. It should be sent to the Zed client as a
/// `SessionUpdate::ToolCallUpdate` immediately after tool execution succeeds.
///
/// # Arguments
///
/// * `id` - The tool call ID that identifies the call being updated.
/// * `output` - The raw JSON output returned by the tool.
///
/// # Returns
///
/// Returns an `acp::ToolCallUpdate` ready for wrapping in a `SessionNotification`.
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema as acp;
/// use xzatoma::acp::tool_notifications::{build_tool_call_completion, generate_tool_call_id};
///
/// let id = generate_tool_call_id();
/// let output = serde_json::json!({ "content": "hello world" });
/// let update = build_tool_call_completion(&id, &output);
///
/// assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Completed));
/// assert!(update.fields.raw_output.is_some());
/// ```
pub fn build_tool_call_completion(
    id: &acp::ToolCallId,
    output: &serde_json::Value,
) -> acp::ToolCallUpdate {
    acp::ToolCallUpdate::new(
        id.clone(),
        acp::ToolCallUpdateFields::new()
            .status(acp::ToolCallStatus::Completed)
            .raw_output(output.clone()),
    )
}

/// Builds a `ToolCallUpdate` marking a tool call as failed with an error message.
///
/// The returned update sets the status to `ToolCallStatus::Failed` and records
/// a JSON object containing the error string as the raw output. It should be
/// sent to the Zed client as a `SessionUpdate::ToolCallUpdate` when tool
/// execution produces an error.
///
/// # Arguments
///
/// * `id` - The tool call ID that identifies the call being updated.
/// * `error` - A human-readable description of the failure.
///
/// # Returns
///
/// Returns an `acp::ToolCallUpdate` ready for wrapping in a `SessionNotification`.
///
/// # Examples
///
/// ```
/// use agent_client_protocol::schema as acp;
/// use xzatoma::acp::tool_notifications::{build_tool_call_failure, generate_tool_call_id};
///
/// let id = generate_tool_call_id();
/// let update = build_tool_call_failure(&id, "file not found");
///
/// assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Failed));
/// let raw = update.fields.raw_output.as_ref().unwrap();
/// assert_eq!(raw["error"], "file not found");
/// ```
pub fn build_tool_call_failure(id: &acp::ToolCallId, error: &str) -> acp::ToolCallUpdate {
    acp::ToolCallUpdate::new(
        id.clone(),
        acp::ToolCallUpdateFields::new()
            .status(acp::ToolCallStatus::Failed)
            .raw_output(serde_json::json!({ "error": error })),
    )
}

/// Builds `ToolCallLocation` entries for file-targeting tools.
///
/// Location entries enable Zed's "follow-along" feature, which navigates the
/// editor to the file being accessed as the agent works. Only tools that
/// operate on a specific file path produce location entries; all other tools
/// return an empty vector.
///
/// The function attempts to extract a file path from the `"path"` key in the
/// input JSON, falling back to `"file_path"` if `"path"` is absent.
///
/// # File-targeting tools
///
/// `read_file`, `write_file`, `edit_file`, `file_metadata`,
/// `ide_read_text_file`, `ide_write_text_file`
///
/// # Arguments
///
/// * `tool_name` - The canonical tool name used in the XZatoma tool registry.
/// * `input` - The raw JSON input parameters passed to the tool.
///
/// # Returns
///
/// Returns a `Vec<acp::ToolCallLocation>` with zero or one entries.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::tool_notifications::build_tool_call_locations;
///
/// let input = serde_json::json!({ "path": "/workspace/main.rs" });
/// let locations = build_tool_call_locations("read_file", &input);
/// assert_eq!(locations.len(), 1);
/// assert_eq!(locations[0].path.to_str().unwrap(), "/workspace/main.rs");
///
/// let terminal_input = serde_json::json!({ "command": "cargo build" });
/// let no_locations = build_tool_call_locations("terminal", &terminal_input);
/// assert!(no_locations.is_empty());
/// ```
pub fn build_tool_call_locations(
    tool_name: &str,
    input: &serde_json::Value,
) -> Vec<acp::ToolCallLocation> {
    const FILE_TARGETING_TOOLS: &[&str] = &[
        "read_file",
        "write_file",
        "edit_file",
        "file_metadata",
        "ide_read_text_file",
        "ide_write_text_file",
    ];

    if !FILE_TARGETING_TOOLS.contains(&tool_name) {
        return Vec::new();
    }

    let path = input
        .get("path")
        .or_else(|| input.get("file_path"))
        .and_then(|v| v.as_str());

    match path {
        Some(p) => vec![acp::ToolCallLocation::new(p)],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_sdk::schema as acp;

    // -----------------------------------------------------------------------
    // generate_tool_call_id
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_tool_call_id_is_unique() {
        let first = generate_tool_call_id();
        let second = generate_tool_call_id();

        assert_ne!(first.0, second.0, "two generated IDs must be distinct");
    }

    #[test]
    fn test_generate_tool_call_id_starts_with_prefix() {
        let id = generate_tool_call_id();

        assert!(
            id.0.starts_with("xzatoma-call-"),
            "ID must start with 'xzatoma-call-', got: {}",
            id.0
        );
    }

    #[test]
    fn test_generate_tool_call_id_has_expected_length() {
        let id = generate_tool_call_id();
        // "xzatoma-call-" (13) + 32 hex chars (simple UUID) = 45
        assert_eq!(
            id.0.len(),
            45,
            "ID must be 45 characters (prefix + simple UUID), got: {}",
            id.0
        );
    }

    // -----------------------------------------------------------------------
    // tool_kind_for_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_tool_kind_for_name_maps_read_tools() {
        for tool in &[
            "read_file",
            "list_directory",
            "find_path",
            "grep",
            "file_metadata",
            "ide_read_text_file",
        ] {
            assert_eq!(
                tool_kind_for_name(tool),
                acp::ToolKind::Read,
                "expected Read for tool '{}'",
                tool
            );
        }
    }

    #[test]
    fn test_tool_kind_for_name_maps_write_tools() {
        for tool in &[
            "write_file",
            "edit_file",
            "create_directory",
            "copy_path",
            "delete_path",
            "move_path",
            "ide_write_text_file",
        ] {
            assert_eq!(
                tool_kind_for_name(tool),
                acp::ToolKind::Edit,
                "expected Edit for tool '{}'",
                tool
            );
        }
    }

    #[test]
    fn test_tool_kind_for_name_maps_terminal() {
        for tool in &[
            "terminal",
            "ide_open_terminal",
            "ide_terminal_output",
            "ide_wait_for_terminal_exit",
            "ide_kill_terminal",
        ] {
            assert_eq!(
                tool_kind_for_name(tool),
                acp::ToolKind::Execute,
                "expected Execute for tool '{}'",
                tool
            );
        }
    }

    #[test]
    fn test_tool_kind_for_name_maps_fetch() {
        assert_eq!(tool_kind_for_name("fetch"), acp::ToolKind::Fetch);
    }

    #[test]
    fn test_tool_kind_for_name_maps_plan_to_think() {
        assert_eq!(tool_kind_for_name("plan"), acp::ToolKind::Think);
    }

    #[test]
    fn test_tool_kind_for_name_maps_subagent_to_other() {
        assert_eq!(tool_kind_for_name("subagent"), acp::ToolKind::Other);
        assert_eq!(
            tool_kind_for_name("parallel_subagent"),
            acp::ToolKind::Other
        );
    }

    #[test]
    fn test_tool_kind_for_name_maps_ide_request_permission_to_other() {
        assert_eq!(
            tool_kind_for_name("ide_request_permission"),
            acp::ToolKind::Other
        );
    }

    #[test]
    fn test_tool_kind_for_name_maps_unknown_to_other() {
        assert_eq!(tool_kind_for_name("not_a_real_tool"), acp::ToolKind::Other);
        assert_eq!(tool_kind_for_name(""), acp::ToolKind::Other);
        assert_eq!(tool_kind_for_name("GREP"), acp::ToolKind::Other);
    }

    // -----------------------------------------------------------------------
    // tool_call_title
    // -----------------------------------------------------------------------

    #[test]
    fn test_tool_call_title_for_read_file_with_path() {
        let input = serde_json::json!({ "path": "/workspace/src/main.rs" });
        let title = tool_call_title("read_file", &input);

        assert_eq!(title, "Reading file /workspace/src/main.rs");
    }

    #[test]
    fn test_tool_call_title_for_read_file_without_path() {
        let input = serde_json::json!({});
        let title = tool_call_title("read_file", &input);

        assert_eq!(title, "Reading file");
    }

    #[test]
    fn test_tool_call_title_for_write_file_with_path() {
        let input = serde_json::json!({ "path": "/workspace/output.txt" });
        let title = tool_call_title("write_file", &input);

        assert_eq!(title, "Writing file /workspace/output.txt");
    }

    #[test]
    fn test_tool_call_title_for_write_file_without_path() {
        let input = serde_json::json!({});
        let title = tool_call_title("write_file", &input);

        assert_eq!(title, "Writing file");
    }

    #[test]
    fn test_tool_call_title_for_terminal_with_command() {
        let input = serde_json::json!({ "command": "cargo test" });
        let title = tool_call_title("terminal", &input);

        assert_eq!(title, "Running terminal command: cargo test");
    }

    #[test]
    fn test_tool_call_title_for_terminal_without_command() {
        let input = serde_json::json!({});
        let title = tool_call_title("terminal", &input);

        assert_eq!(title, "Running terminal command");
    }

    #[test]
    fn test_tool_call_title_for_fetch_with_url() {
        let input = serde_json::json!({ "url": "https://example.com/api" });
        let title = tool_call_title("fetch", &input);

        assert_eq!(title, "Fetching https://example.com/api");
    }

    #[test]
    fn test_tool_call_title_for_fetch_without_url() {
        let input = serde_json::json!({});
        let title = tool_call_title("fetch", &input);

        assert_eq!(title, "Fetching URL");
    }

    #[test]
    fn test_tool_call_title_for_grep_with_regex() {
        let input = serde_json::json!({ "regex": "fn main" });
        let title = tool_call_title("grep", &input);

        assert_eq!(title, "Searching files for fn main");
    }

    #[test]
    fn test_tool_call_title_for_plan() {
        let input = serde_json::json!({});
        let title = tool_call_title("plan", &input);

        assert_eq!(title, "Planning");
    }

    #[test]
    fn test_tool_call_title_for_unknown_tool() {
        let input = serde_json::json!({});
        let title = tool_call_title("my_custom_tool", &input);

        assert_eq!(title, "Running tool: my_custom_tool");
    }

    #[test]
    fn test_tool_call_title_uses_file_path_fallback_key() {
        let input = serde_json::json!({ "file_path": "/workspace/notes.md" });
        let title = tool_call_title("read_file", &input);

        assert_eq!(title, "Reading file /workspace/notes.md");
    }

    #[test]
    fn test_tool_call_title_prefers_path_over_file_path() {
        let input = serde_json::json!({
            "path": "/preferred/path.rs",
            "file_path": "/fallback/path.rs"
        });
        let title = tool_call_title("read_file", &input);

        assert_eq!(title, "Reading file /preferred/path.rs");
    }

    // -----------------------------------------------------------------------
    // build_tool_call_start
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tool_call_start_has_in_progress_status() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({ "path": "/workspace/main.rs" });
        let tool_call = build_tool_call_start(&id, "read_file", &input);

        assert_eq!(tool_call.status, acp::ToolCallStatus::InProgress);
    }

    #[test]
    fn test_build_tool_call_start_sets_correct_kind() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({});
        let tool_call = build_tool_call_start(&id, "write_file", &input);

        assert_eq!(tool_call.kind, acp::ToolKind::Edit);
    }

    #[test]
    fn test_build_tool_call_start_includes_raw_input() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({ "path": "/workspace/lib.rs", "recursive": true });
        let tool_call = build_tool_call_start(&id, "read_file", &input);

        assert!(tool_call.raw_input.is_some());
        let stored = tool_call.raw_input.unwrap();
        assert_eq!(stored["path"], "/workspace/lib.rs");
    }

    #[test]
    fn test_build_tool_call_start_sets_title() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({ "path": "/workspace/config.toml" });
        let tool_call = build_tool_call_start(&id, "read_file", &input);

        assert_eq!(tool_call.title, "Reading file /workspace/config.toml");
    }

    #[test]
    fn test_build_tool_call_start_includes_locations_for_file_tool() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({ "path": "/workspace/main.rs" });
        let tool_call = build_tool_call_start(&id, "read_file", &input);

        assert_eq!(tool_call.locations.len(), 1);
        assert_eq!(
            tool_call.locations[0].path.to_str().unwrap(),
            "/workspace/main.rs"
        );
    }

    #[test]
    fn test_build_tool_call_start_has_no_locations_for_non_file_tool() {
        let id = generate_tool_call_id();
        let input = serde_json::json!({ "command": "ls -la" });
        let tool_call = build_tool_call_start(&id, "terminal", &input);

        assert!(tool_call.locations.is_empty());
    }

    // -----------------------------------------------------------------------
    // build_tool_call_completion
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tool_call_completion_has_completed_status() {
        let id = generate_tool_call_id();
        let output = serde_json::json!({ "content": "fn main() {}" });
        let update = build_tool_call_completion(&id, &output);

        assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Completed));
    }

    #[test]
    fn test_build_tool_call_completion_includes_raw_output() {
        let id = generate_tool_call_id();
        let output = serde_json::json!({ "lines": 42, "content": "hello" });
        let update = build_tool_call_completion(&id, &output);

        assert!(update.fields.raw_output.is_some());
        let stored = update.fields.raw_output.unwrap();
        assert_eq!(stored["lines"], 42);
    }

    #[test]
    fn test_build_tool_call_completion_preserves_tool_call_id() {
        let id = generate_tool_call_id();
        let output = serde_json::json!({});
        let update = build_tool_call_completion(&id, &output);

        assert_eq!(update.tool_call_id.0, id.0);
    }

    // -----------------------------------------------------------------------
    // build_tool_call_failure
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tool_call_failure_has_failed_status() {
        let id = generate_tool_call_id();
        let update = build_tool_call_failure(&id, "permission denied");

        assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Failed));
    }

    #[test]
    fn test_build_tool_call_failure_encodes_error_in_raw_output() {
        let id = generate_tool_call_id();
        let update = build_tool_call_failure(&id, "file not found: /tmp/missing.rs");

        let raw = update
            .fields
            .raw_output
            .as_ref()
            .expect("raw_output must be present");
        assert_eq!(raw["error"], "file not found: /tmp/missing.rs");
    }

    #[test]
    fn test_build_tool_call_failure_preserves_tool_call_id() {
        let id = generate_tool_call_id();
        let update = build_tool_call_failure(&id, "timed out");

        assert_eq!(update.tool_call_id.0, id.0);
    }

    // -----------------------------------------------------------------------
    // build_tool_call_locations
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_tool_call_locations_extracts_path_for_read_file() {
        let input = serde_json::json!({ "path": "/workspace/src/lib.rs" });
        let locations = build_tool_call_locations("read_file", &input);

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].path.to_str().unwrap(), "/workspace/src/lib.rs");
    }

    #[test]
    fn test_build_tool_call_locations_extracts_path_for_all_file_tools() {
        for tool in &[
            "read_file",
            "write_file",
            "edit_file",
            "file_metadata",
            "ide_read_text_file",
            "ide_write_text_file",
        ] {
            let input = serde_json::json!({ "path": "/workspace/target.rs" });
            let locations = build_tool_call_locations(tool, &input);

            assert_eq!(
                locations.len(),
                1,
                "expected 1 location for tool '{}', got {}",
                tool,
                locations.len()
            );
        }
    }

    #[test]
    fn test_build_tool_call_locations_falls_back_to_file_path_key() {
        let input = serde_json::json!({ "file_path": "/workspace/alt.rs" });
        let locations = build_tool_call_locations("read_file", &input);

        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].path.to_str().unwrap(), "/workspace/alt.rs");
    }

    #[test]
    fn test_build_tool_call_locations_empty_when_no_path_in_input() {
        let input = serde_json::json!({});
        let locations = build_tool_call_locations("read_file", &input);

        assert!(locations.is_empty());
    }

    #[test]
    fn test_build_tool_call_locations_empty_for_non_file_tool() {
        for tool in &[
            "terminal",
            "fetch",
            "grep",
            "list_directory",
            "find_path",
            "plan",
            "subagent",
            "unknown_tool",
        ] {
            let input = serde_json::json!({ "path": "/workspace/main.rs" });
            let locations = build_tool_call_locations(tool, &input);

            assert!(
                locations.is_empty(),
                "expected no locations for tool '{}', got {}",
                tool,
                locations.len()
            );
        }
    }

    #[test]
    fn test_build_tool_call_locations_empty_for_ide_terminal_tools() {
        let input = serde_json::json!({ "path": "/workspace/main.rs" });

        for tool in &[
            "ide_open_terminal",
            "ide_terminal_output",
            "ide_wait_for_terminal_exit",
            "ide_kill_terminal",
        ] {
            let locations = build_tool_call_locations(tool, &input);
            assert!(
                locations.is_empty(),
                "expected no locations for terminal tool '{}', got {}",
                tool,
                locations.len()
            );
        }
    }
}
