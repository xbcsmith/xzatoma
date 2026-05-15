//! MCP tool bridge -- adapters between MCP servers and Xzatoma's ToolRegistry
//!
//! This module provides three [`crate::tools::ToolExecutor`] implementations
//! that bridge MCP server capabilities into Xzatoma's local tool registry:
//!
//! - [`McpToolExecutor`]          -- wraps a single MCP `tools/call` tool
//! - [`McpResourceToolExecutor`]  -- exposes `resources/read` as a tool
//! - [`McpPromptToolExecutor`]    -- exposes `prompts/get` as a tool
//!
//! and a helper function:
//!
//! - [`register_mcp_tools`] -- iterates all connected servers and registers
//!   each tool into a [`crate::tools::ToolRegistry`].
//!
//! # Namespacing
//!
//! Every MCP tool is registered under the name
//! `format!("{}__{}", server_id, tool_name)` (double underscore separator).
//! Both `server_id` and `tool_name` may independently contain single
//! underscores, making a single-underscore separator ambiguous.
//!
//! # Approval Policy
//!
//! All three executors delegate to [`crate::mcp::approval::approval_decision`]
//! to determine whether an operation is allowed, denied, or requires a user
//! prompt. No inline policy checks are permitted here.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};
use crate::mcp::approval::{
    approval_decision, prompt_user_approval, ApprovalDecision, McpOperation,
};
use crate::mcp::manager::McpClientManager;
use crate::mcp::types::{
    MessageContent, PromptMessage, ResourceContents, TaskSupport, ToolResponseContent,
};
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};

// ---------------------------------------------------------------------------
// McpToolExecutor
// ---------------------------------------------------------------------------

/// Adapts a single MCP server tool into Xzatoma's [`ToolExecutor`] trait.
///
/// The executor holds a reference to the [`McpClientManager`] so that calls
/// are forwarded to the correct server at execution time. Tools with
/// [`TaskSupport::Required`] are dispatched via
/// [`McpClientManager::call_tool_as_task`]; all others use
/// [`McpClientManager::call_tool`].
///
/// # Namespacing
///
/// `registry_name` is always `format!("{}__{}", server_id, tool_name)`.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::auth::token_store::TokenStore;
/// use xzatoma::mcp::manager::McpClientManager;
/// use xzatoma::mcp::tool_bridge::McpToolExecutor;
/// use xzatoma::tools::ToolExecutor;
///
/// let manager = Arc::new(RwLock::new(McpClientManager::new(
///     Arc::new(reqwest::Client::new()),
///     Arc::new(TokenStore),
/// )));
///
/// let executor = McpToolExecutor {
///     server_id: "my_server".to_string(),
///     tool_name: "search_items".to_string(),
///     registry_name: "my_server__search_items".to_string(),
///     description: "Search items by keyword".to_string(),
///     input_schema: serde_json::json!({"type": "object", "properties": {}}),
///     task_support: None,
///     manager,
///     execution_mode: ExecutionMode::FullAutonomous,
///     headless: false,
/// };
///
/// let def = executor.tool_definition();
/// assert_eq!(def["name"], "my_server__search_items");
/// ```
#[derive(Debug)]
pub struct McpToolExecutor {
    /// Identifier of the MCP server that owns this tool.
    pub server_id: String,
    /// Original tool name as reported by the MCP server.
    pub tool_name: String,
    /// Registry name: `format!("{}__{}", server_id, tool_name)`.
    pub registry_name: String,
    /// Human-readable description of the tool.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,
    /// Whether this tool requires or supports task-wrapped execution.
    pub task_support: Option<TaskSupport>,
    /// Shared reference to the live server manager.
    pub manager: Arc<RwLock<McpClientManager>>,
    /// Agent execution mode, used by the approval policy.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
}

#[async_trait::async_trait]
impl ToolExecutor for McpToolExecutor {
    /// Returns the tool definition in Xzatoma's `{ "name", "description",
    /// "parameters" }` format.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    /// use xzatoma::config::ExecutionMode;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    /// use xzatoma::mcp::manager::McpClientManager;
    /// use xzatoma::mcp::tool_bridge::McpToolExecutor;
    /// use xzatoma::tools::ToolExecutor;
    ///
    /// let manager = Arc::new(RwLock::new(McpClientManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// )));
    ///
    /// let executor = McpToolExecutor {
    ///     server_id: "s".to_string(),
    ///     tool_name: "t".to_string(),
    ///     registry_name: "s__t".to_string(),
    ///     description: "desc".to_string(),
    ///     input_schema: serde_json::json!({"type":"object"}),
    ///     task_support: None,
    ///     manager,
    ///     execution_mode: ExecutionMode::FullAutonomous,
    ///     headless: false,
    /// };
    ///
    /// let def = executor.tool_definition();
    /// assert!(def.get("name").is_some());
    /// assert!(def.get("description").is_some());
    /// assert!(def.get("parameters").is_some());
    /// ```
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.registry_name,
            "description": self.description,
            "parameters": self.input_schema,
        })
    }

    /// Execute the MCP tool.
    ///
    /// If auto-approval is not in effect, a confirmation prompt is printed to
    /// stderr and a single line is read from stdin. Any response other than
    /// `"y"` or `"yes"` (case-insensitive) causes the call to return a
    /// rejected [`ToolResult`] without contacting the server.
    ///
    /// Tools with [`TaskSupport::Required`] are dispatched via
    /// [`McpClientManager::call_tool_as_task`]; all others use
    /// [`McpClientManager::call_tool`].
    ///
    /// # Arguments
    ///
    /// * `args` - Tool arguments as a JSON value.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError`] if JSON serialization of `structured_content`
    /// fails, or propagates transport/protocol errors from the manager.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let response = {
            let guard = self.manager.read().await;
            let Some(policy) = guard.approval_policy_for_server(&self.server_id) else {
                return Ok(ToolResult::error(format!(
                    "MCP server '{}' is not registered",
                    self.server_id
                )));
            };
            match approval_decision(
                &policy,
                self.execution_mode,
                self.headless,
                McpOperation::ToolCall {
                    server_id: &self.server_id,
                    tool_name: &self.tool_name,
                },
            ) {
                ApprovalDecision::Allow => {}
                ApprovalDecision::Prompt => {
                    let description = format!(
                        "MCP tool call: {}/{} with args: {}.",
                        self.server_id, self.tool_name, args
                    );
                    if !prompt_user_approval(&description)? {
                        return Ok(ToolResult::error(format!(
                            "User rejected MCP tool call: {}",
                            self.registry_name
                        )));
                    }
                }
                ApprovalDecision::Deny => {
                    return Ok(ToolResult::error(format!(
                        "MCP tool call requires explicit approval policy: {}",
                        self.registry_name
                    )));
                }
            }

            // --- Dispatch to manager ---
            if self.task_support == Some(TaskSupport::Required) {
                guard
                    .call_tool_as_task(&self.server_id, &self.tool_name, Some(args), None)
                    .await?
            } else {
                guard
                    .call_tool(&self.server_id, &self.tool_name, Some(args))
                    .await?
            }
        };

        // --- Map is_error responses ---
        if response.is_error == Some(true) {
            let text = extract_text_content(&response.content);
            return Ok(ToolResult::error(text));
        }

        // --- Build success output ---
        let mut text_content = extract_text_content(&response.content);

        if let Some(structured) = &response.structured_content {
            let pretty = serde_json::to_string_pretty(structured).map_err(|e| {
                XzatomaError::Tool(format!(
                    "Failed to serialize structured_content for {}: {}",
                    self.registry_name, e
                ))
            })?;
            text_content.push_str("\n---\n");
            text_content.push_str(&pretty);
        }

        Ok(ToolResult::success(text_content))
    }
}

// ---------------------------------------------------------------------------
// McpResourceToolExecutor
// ---------------------------------------------------------------------------

/// Adapts MCP `resources/read` into Xzatoma's [`ToolExecutor`] trait.
///
/// Registered under the name `"mcp_read_resource"`. Accepts `server_id` and
/// `uri` arguments and returns the resource content as a string.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::auth::token_store::TokenStore;
/// use xzatoma::mcp::manager::McpClientManager;
/// use xzatoma::mcp::tool_bridge::McpResourceToolExecutor;
/// use xzatoma::tools::ToolExecutor;
///
/// let manager = Arc::new(RwLock::new(McpClientManager::new(
///     Arc::new(reqwest::Client::new()),
///     Arc::new(TokenStore),
/// )));
///
/// let executor = McpResourceToolExecutor {
///     manager,
///     execution_mode: ExecutionMode::FullAutonomous,
///     headless: false,
/// };
///
/// let def = executor.tool_definition();
/// assert_eq!(def["name"], "mcp_read_resource");
/// ```
#[derive(Debug)]
pub struct McpResourceToolExecutor {
    /// Shared reference to the live server manager.
    pub manager: Arc<RwLock<McpClientManager>>,
    /// Agent execution mode, used by the approval policy.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
}

#[async_trait::async_trait]
impl ToolExecutor for McpResourceToolExecutor {
    /// Returns the tool definition for `mcp_read_resource`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    /// use xzatoma::config::ExecutionMode;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    /// use xzatoma::mcp::manager::McpClientManager;
    /// use xzatoma::mcp::tool_bridge::McpResourceToolExecutor;
    /// use xzatoma::tools::ToolExecutor;
    ///
    /// let manager = Arc::new(RwLock::new(McpClientManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// )));
    ///
    /// let executor = McpResourceToolExecutor {
    ///     manager,
    ///     execution_mode: ExecutionMode::Interactive,
    ///     headless: true,
    /// };
    /// let def = executor.tool_definition();
    /// assert_eq!(def["name"], "mcp_read_resource");
    /// assert!(def["parameters"]["properties"]["server_id"].is_object());
    /// assert!(def["parameters"]["properties"]["uri"].is_object());
    /// ```
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "mcp_read_resource",
            "description": "Read a resource from a connected MCP server by URI",
            "parameters": {
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "MCP server identifier"
                    },
                    "uri": {
                        "type": "string",
                        "description": "Resource URI to read"
                    }
                },
                "required": ["server_id", "uri"]
            }
        })
    }

    /// Execute the `mcp_read_resource` tool.
    ///
    /// Extracts `server_id` and `uri` from `args`, applies the approval
    /// policy, and calls [`McpClientManager::read_resource`].
    ///
    /// # Arguments
    ///
    /// * `args` - Must contain `"server_id"` (string) and `"uri"` (string).
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Tool`] if required arguments are missing, or
    /// propagates transport/protocol errors from the manager.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let server_id = args["server_id"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("mcp_read_resource: missing 'server_id'".into()))?
            .to_string();
        let uri = args["uri"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("mcp_read_resource: missing 'uri'".into()))?
            .to_string();

        let guard = self.manager.read().await;
        let Some(policy) = guard.approval_policy_for_server(&server_id) else {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not registered",
                server_id
            )));
        };
        match approval_decision(
            &policy,
            self.execution_mode,
            self.headless,
            McpOperation::ResourceRead {
                server_id: &server_id,
            },
        ) {
            ApprovalDecision::Allow => {}
            ApprovalDecision::Prompt => {
                if !prompt_user_approval(&format!("MCP resource read: {}/{}", server_id, uri))? {
                    return Ok(ToolResult::error(format!(
                        "User rejected MCP resource read: {}/{}",
                        server_id, uri
                    )));
                }
            }
            ApprovalDecision::Deny => {
                return Ok(ToolResult::error(format!(
                    "MCP resource read requires explicit approval policy: {}/{}",
                    server_id, uri
                )));
            }
        };
        match guard.read_resource(&server_id, &uri).await {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// McpPromptToolExecutor
// ---------------------------------------------------------------------------

/// Adapts MCP `prompts/get` into Xzatoma's [`ToolExecutor`] trait.
///
/// Registered under the name `"mcp_get_prompt"`. Accepts `server_id`,
/// `prompt_name`, and optional `arguments` and returns the formatted prompt
/// messages as a string.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::auth::token_store::TokenStore;
/// use xzatoma::mcp::manager::McpClientManager;
/// use xzatoma::mcp::tool_bridge::McpPromptToolExecutor;
/// use xzatoma::tools::ToolExecutor;
///
/// let manager = Arc::new(RwLock::new(McpClientManager::new(
///     Arc::new(reqwest::Client::new()),
///     Arc::new(TokenStore),
/// )));
///
/// let executor = McpPromptToolExecutor {
///     manager,
///     execution_mode: ExecutionMode::FullAutonomous,
///     headless: false,
/// };
///
/// let def = executor.tool_definition();
/// assert_eq!(def["name"], "mcp_get_prompt");
/// ```
#[derive(Debug)]
pub struct McpPromptToolExecutor {
    /// Shared reference to the live server manager.
    pub manager: Arc<RwLock<McpClientManager>>,
    /// Agent execution mode, used by the approval policy.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
}

#[async_trait::async_trait]
impl ToolExecutor for McpPromptToolExecutor {
    /// Returns the tool definition for `mcp_get_prompt`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    /// use xzatoma::config::ExecutionMode;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    /// use xzatoma::mcp::manager::McpClientManager;
    /// use xzatoma::mcp::tool_bridge::McpPromptToolExecutor;
    /// use xzatoma::tools::ToolExecutor;
    ///
    /// let manager = Arc::new(RwLock::new(McpClientManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// )));
    ///
    /// let executor = McpPromptToolExecutor {
    ///     manager,
    ///     execution_mode: ExecutionMode::Interactive,
    ///     headless: false,
    /// };
    /// let def = executor.tool_definition();
    /// assert_eq!(def["name"], "mcp_get_prompt");
    /// assert!(def["parameters"]["properties"]["server_id"].is_object());
    /// assert!(def["parameters"]["properties"]["prompt_name"].is_object());
    /// ```
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "mcp_get_prompt",
            "description": "Retrieve a named prompt template from a connected MCP server",
            "parameters": {
                "type": "object",
                "properties": {
                    "server_id": {
                        "type": "string",
                        "description": "MCP server identifier"
                    },
                    "prompt_name": {
                        "type": "string",
                        "description": "Name of the prompt to retrieve"
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Named string arguments for the prompt template",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["server_id", "prompt_name"]
            }
        })
    }

    /// Execute the `mcp_get_prompt` tool.
    ///
    /// Extracts `server_id`, `prompt_name`, and optional `arguments` from
    /// `args`, applies the approval policy, and calls
    /// [`McpClientManager::get_prompt`]. The response messages are formatted
    /// as `"[<role>]\n<content_text>"` blocks separated by blank lines.
    ///
    /// # Arguments
    ///
    /// * `args` - Must contain `"server_id"` and `"prompt_name"` (strings).
    ///   `"arguments"` is optional and defaults to an empty map.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Tool`] if required arguments are missing, or
    /// propagates transport/protocol errors from the manager.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let server_id = args["server_id"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("mcp_get_prompt: missing 'server_id'".into()))?
            .to_string();
        let prompt_name = args["prompt_name"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("mcp_get_prompt: missing 'prompt_name'".into()))?
            .to_string();

        // Parse optional arguments map; default to empty.
        let arguments: HashMap<String, String> = match args.get("arguments") {
            Some(serde_json::Value::Object(map)) => map
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect(),
            _ => HashMap::new(),
        };

        let guard = self.manager.read().await;
        let Some(policy) = guard.approval_policy_for_server(&server_id) else {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not registered",
                server_id
            )));
        };
        match approval_decision(
            &policy,
            self.execution_mode,
            self.headless,
            McpOperation::PromptGet {
                server_id: &server_id,
            },
        ) {
            ApprovalDecision::Allow => {}
            ApprovalDecision::Prompt => {
                if !prompt_user_approval(&format!("MCP prompt get: {}/{}", server_id, prompt_name))?
                {
                    return Ok(ToolResult::error(format!(
                        "User rejected MCP prompt get: {}/{}",
                        server_id, prompt_name
                    )));
                }
            }
            ApprovalDecision::Deny => {
                return Ok(ToolResult::error(format!(
                    "MCP prompt get requires explicit approval policy: {}/{}",
                    server_id, prompt_name
                )));
            }
        };
        match guard.get_prompt(&server_id, &prompt_name, arguments).await {
            Ok(response) => {
                let formatted = format_prompt_messages(&response.messages);
                Ok(ToolResult::success(formatted))
            }
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// register_mcp_tools
// ---------------------------------------------------------------------------

/// Register all MCP server tools into a [`ToolRegistry`].
///
/// Iterates every connected server in `manager`, constructs an
/// [`McpToolExecutor`] for each tool, and registers it under
/// `format!("{}__{}", server_id, tool_name)`.
///
/// A [`tracing::warn!`] is emitted when a registry name would overwrite an
/// existing entry. The old entry is silently replaced by
/// [`ToolRegistry::register`].
///
/// Also registers one [`McpResourceToolExecutor`] under `"mcp_read_resource"`
/// and one [`McpPromptToolExecutor`] under `"mcp_get_prompt"`.
///
/// # Arguments
///
/// * `registry`       - The tool registry to register tools into.
/// * `manager`        - Shared reference to the live server manager.
/// * `execution_mode` - Current agent execution mode (forwarded to executors).
/// * `headless`       - Whether the agent is running headless (forwarded to executors).
///
/// # Returns
///
/// The total number of MCP `tools/call` executors registered (does not count
/// the resource and prompt executors).
///
/// # Errors
///
/// Currently infallible; returns `Ok(count)` always. The signature uses
/// `Result` so future error conditions can be propagated without a breaking
/// change.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::auth::token_store::TokenStore;
/// use xzatoma::mcp::manager::McpClientManager;
/// use xzatoma::mcp::tool_bridge::register_mcp_tools;
/// use xzatoma::tools::ToolRegistry;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let manager = Arc::new(RwLock::new(McpClientManager::new(
///     Arc::new(reqwest::Client::new()),
///     Arc::new(TokenStore),
/// )));
///
/// let mut registry = ToolRegistry::new();
/// let count = register_mcp_tools(
///     &mut registry,
///     Arc::clone(&manager),
///     ExecutionMode::FullAutonomous,
///     false,
/// )
/// .await?;
///
/// // No connected servers -- only the two built-in MCP tools are added.
/// assert_eq!(count, 0);
/// assert!(registry.get("mcp_read_resource").is_some());
/// assert!(registry.get("mcp_get_prompt").is_some());
/// # Ok(())
/// # }
/// ```
pub async fn register_mcp_tools(
    registry: &mut ToolRegistry,
    manager: Arc<RwLock<McpClientManager>>,
    execution_mode: ExecutionMode,
    headless: bool,
) -> Result<usize> {
    // Collect the (server_id, tools) pairs while holding only a read lock,
    // then drop the lock before mutating the registry.
    let pairs: Vec<(String, Vec<crate::mcp::types::McpTool>)> = {
        let guard = manager.read().await;
        guard.get_tools_for_registry()
    };

    let mut count: usize = 0;

    for (server_id, tools) in pairs {
        for tool in tools {
            let registry_name = format!("{}__{}", server_id, tool.name);

            // Warn if this name is already occupied.
            if registry.get(&registry_name).is_some() {
                tracing::warn!(
                    registry_name = %registry_name,
                    "MCP tool registration: overwriting existing registry entry"
                );
            }

            let task_support = tool.execution.as_ref().and_then(|e| e.task_support.clone());

            let executor = McpToolExecutor {
                server_id: server_id.clone(),
                tool_name: tool.name.clone(),
                registry_name: registry_name.clone(),
                description: tool.description.unwrap_or_default(),
                input_schema: tool.input_schema.clone(),
                task_support,
                manager: Arc::clone(&manager),
                execution_mode,
                headless,
            };

            registry.register(registry_name, Arc::new(executor));
            count += 1;
        }
    }

    // Always register the generic resource and prompt executors.
    registry.register(
        "mcp_read_resource",
        Arc::new(McpResourceToolExecutor {
            manager: Arc::clone(&manager),
            execution_mode,
            headless,
        }),
    );

    registry.register(
        "mcp_get_prompt",
        Arc::new(McpPromptToolExecutor {
            manager: Arc::clone(&manager),
            execution_mode,
            headless,
        }),
    );

    Ok(count)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract and join all `Text` items from a [`ToolResponseContent`] slice.
///
/// Non-text variants (Image, Audio, Resource) are skipped. If no text items
/// are present an empty string is returned.
fn extract_text_content(content: &[ToolResponseContent]) -> String {
    content
        .iter()
        .filter_map(|item| {
            if let ToolResponseContent::Text { text } = item {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format a slice of [`PromptMessage`]s as `"[<role>]\n<content_text>"` blocks
/// separated by blank lines.
///
/// The role label is the `Debug` representation of the `Role` variant,
/// uppercased (e.g. `[USER]`, `[ASSISTANT]`). Content is extracted from the
/// message's `MessageContent` variant; non-text variants produce a concise
/// placeholder string.
fn format_prompt_messages(messages: &[PromptMessage]) -> String {
    messages
        .iter()
        .map(|msg| {
            let role_label = format!("{:?}", msg.role).to_uppercase();
            let content_text = match &msg.content {
                MessageContent::Text(t) => t.text.clone(),
                MessageContent::Image(_) => "[image content]".to_string(),
                MessageContent::Audio(_) => "[audio content]".to_string(),
                MessageContent::Resource { resource, .. } => match resource {
                    ResourceContents::Text(t) => t.text.clone(),
                    ResourceContents::Blob(b) => {
                        let mime = b.mime_type.as_deref().unwrap_or("application/octet-stream");
                        format!("[base64 {}]", mime)
                    }
                },
            };
            format!("[{}]\n{}", role_label, content_text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use crate::mcp::auth::token_store::TokenStore;
    use crate::mcp::manager::McpClientManager;

    fn make_manager() -> McpClientManager {
        McpClientManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))
    }

    fn make_executor(
        server_id: &str,
        tool_name: &str,
        task_support: Option<TaskSupport>,
        execution_mode: ExecutionMode,
        headless: bool,
    ) -> McpToolExecutor {
        let manager = Arc::new(RwLock::new(make_manager()));
        McpToolExecutor {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            registry_name: format!("{}__{}", server_id, tool_name),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            task_support,
            manager,
            execution_mode,
            headless,
        }
    }

    // -----------------------------------------------------------------------
    // Approval policy
    // -----------------------------------------------------------------------

    #[test]
    fn test_approval_decision_headless_default_policy_denies() {
        let decision = approval_decision(
            &Default::default(),
            ExecutionMode::Interactive,
            true,
            McpOperation::ToolCall {
                server_id: "srv",
                tool_name: "search",
            },
        );
        assert_eq!(decision, ApprovalDecision::Deny);
    }

    #[test]
    fn test_approval_decision_interactive_default_policy_prompts() {
        let decision = approval_decision(
            &Default::default(),
            ExecutionMode::Interactive,
            false,
            McpOperation::ToolCall {
                server_id: "srv",
                tool_name: "search",
            },
        );
        assert_eq!(decision, ApprovalDecision::Prompt);
    }

    // -----------------------------------------------------------------------
    // registry_name double-underscore separator
    // -----------------------------------------------------------------------

    #[test]
    fn test_registry_name_uses_double_underscore_separator() {
        let executor = make_executor(
            "my_server",
            "search_items",
            None,
            ExecutionMode::FullAutonomous,
            false,
        );
        assert_eq!(executor.registry_name, "my_server__search_items");
    }

    #[test]
    fn test_registry_name_with_plain_ids() {
        let executor = make_executor("server", "tool", None, ExecutionMode::FullAutonomous, false);
        assert_eq!(executor.registry_name, "server__tool");
    }

    // -----------------------------------------------------------------------
    // tool_definition format
    // -----------------------------------------------------------------------

    #[test]
    fn test_tool_definition_format_matches_xzatoma_convention() {
        let executor = make_executor(
            "srv",
            "do_thing",
            None,
            ExecutionMode::FullAutonomous,
            false,
        );
        let def = executor.tool_definition();
        assert!(def.get("name").is_some(), "missing 'name' key");
        assert!(
            def.get("description").is_some(),
            "missing 'description' key"
        );
        assert!(def.get("parameters").is_some(), "missing 'parameters' key");
    }

    #[test]
    fn test_tool_definition_name_equals_registry_name() {
        let executor = make_executor("alpha", "beta", None, ExecutionMode::FullAutonomous, false);
        let def = executor.tool_definition();
        assert_eq!(def["name"], "alpha__beta");
    }

    #[test]
    fn test_resource_executor_tool_definition_name() {
        let manager = Arc::new(RwLock::new(make_manager()));
        let exec = McpResourceToolExecutor {
            manager,
            execution_mode: ExecutionMode::FullAutonomous,
            headless: false,
        };
        assert_eq!(exec.tool_definition()["name"], "mcp_read_resource");
    }

    #[test]
    fn test_prompt_executor_tool_definition_name() {
        let manager = Arc::new(RwLock::new(make_manager()));
        let exec = McpPromptToolExecutor {
            manager,
            execution_mode: ExecutionMode::FullAutonomous,
            headless: false,
        };
        assert_eq!(exec.tool_definition()["name"], "mcp_get_prompt");
    }

    // -----------------------------------------------------------------------
    // extract_text_content helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_text_content_joins_text_items() {
        let items = vec![
            ToolResponseContent::Text {
                text: "hello".to_string(),
            },
            ToolResponseContent::Text {
                text: "world".to_string(),
            },
        ];
        assert_eq!(extract_text_content(&items), "hello\nworld");
    }

    #[test]
    fn test_extract_text_content_skips_non_text() {
        let items = vec![
            ToolResponseContent::Image {
                data: "abc".to_string(),
                mime_type: "image/png".to_string(),
            },
            ToolResponseContent::Text {
                text: "only_text".to_string(),
            },
        ];
        assert_eq!(extract_text_content(&items), "only_text");
    }

    #[test]
    fn test_extract_text_content_empty_slice_returns_empty_string() {
        assert_eq!(extract_text_content(&[]), "");
    }

    // -----------------------------------------------------------------------
    // format_prompt_messages helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_prompt_messages_single_user_message() {
        use crate::mcp::types::{MessageContent, PromptMessage, Role, TextContent};
        let msgs = vec![PromptMessage {
            role: Role::User,
            content: MessageContent::Text(TextContent {
                text: "Hello".to_string(),
                annotations: None,
            }),
        }];
        let output = format_prompt_messages(&msgs);
        assert!(output.contains("[USER]"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_format_prompt_messages_multiple_messages_separated_by_blank_line() {
        use crate::mcp::types::{MessageContent, PromptMessage, Role, TextContent};
        let msgs = vec![
            PromptMessage {
                role: Role::User,
                content: MessageContent::Text(TextContent {
                    text: "Q".to_string(),
                    annotations: None,
                }),
            },
            PromptMessage {
                role: Role::Assistant,
                content: MessageContent::Text(TextContent {
                    text: "A".to_string(),
                    annotations: None,
                }),
            },
        ];
        let output = format_prompt_messages(&msgs);
        assert!(output.contains("[USER]"));
        assert!(output.contains("[ASSISTANT]"));
        assert!(output.contains("\n\n"));
    }

    // -----------------------------------------------------------------------
    // register_mcp_tools with no servers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_mcp_tools_no_servers_returns_zero() {
        let manager = Arc::new(RwLock::new(make_manager()));
        let mut registry = ToolRegistry::new();
        let count = register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            false,
        )
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_register_mcp_tools_always_registers_resource_and_prompt_executors() {
        let manager = Arc::new(RwLock::new(make_manager()));
        let mut registry = ToolRegistry::new();
        register_mcp_tools(
            &mut registry,
            Arc::clone(&manager),
            ExecutionMode::FullAutonomous,
            false,
        )
        .await
        .unwrap();

        assert!(
            registry.get("mcp_read_resource").is_some(),
            "mcp_read_resource must be registered even with no servers"
        );
        assert!(
            registry.get("mcp_get_prompt").is_some(),
            "mcp_get_prompt must be registered even with no servers"
        );
    }
}
