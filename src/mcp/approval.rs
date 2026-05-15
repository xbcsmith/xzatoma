//! MCP approval policy and interactive confirmation helpers.
//!
//! This module is the single authoritative source for determining whether MCP
//! operations may proceed. Headless execution and full autonomous mode do not
//! grant implicit MCP trust; callers must use an explicit per-server approval
//! policy.

use std::io::{BufRead, Write};

use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};
use crate::mcp::server::{McpApprovalAction, McpServerApprovalPolicy};

/// Runtime approval decision for an MCP operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// Execute the operation without prompting.
    Allow,
    /// Ask the user before executing the operation.
    Prompt,
    /// Reject the operation before contacting the MCP server.
    Deny,
}

/// MCP operation being evaluated by the approval policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpOperation<'a> {
    /// Tool call on a specific server.
    ToolCall {
        /// MCP server identifier.
        server_id: &'a str,
        /// Original MCP tool name.
        tool_name: &'a str,
    },
    /// Resource read from a specific server.
    ResourceRead {
        /// MCP server identifier.
        server_id: &'a str,
    },
    /// Prompt retrieval from a specific server.
    PromptGet {
        /// MCP server identifier.
        server_id: &'a str,
    },
}

/// Determines whether an MCP operation is allowed, denied, or requires prompt.
///
/// Explicit deny rules always win. Explicit allow rules require the server to
/// be marked trusted. Headless execution cannot prompt, so prompt decisions are
/// converted to deny for headless runs.
pub fn approval_decision(
    policy: &McpServerApprovalPolicy,
    execution_mode: ExecutionMode,
    headless: bool,
    operation: McpOperation<'_>,
) -> ApprovalDecision {
    let configured_action = match operation {
        McpOperation::ToolCall { tool_name, .. } => policy
            .tools
            .get(tool_name)
            .or_else(|| policy.tools.get("*"))
            .copied()
            .unwrap_or(policy.default_tool_action),
        McpOperation::ResourceRead { .. } => policy.resource_read_action,
        McpOperation::PromptGet { .. } => policy.prompt_get_action,
    };

    match configured_action {
        McpApprovalAction::Deny => ApprovalDecision::Deny,
        McpApprovalAction::Allow if policy.trusted => ApprovalDecision::Allow,
        McpApprovalAction::Allow | McpApprovalAction::Prompt => {
            if headless {
                ApprovalDecision::Deny
            } else if execution_mode == ExecutionMode::Interactive
                || execution_mode == ExecutionMode::RestrictedAutonomous
                || execution_mode == ExecutionMode::FullAutonomous
            {
                ApprovalDecision::Prompt
            } else {
                ApprovalDecision::Deny
            }
        }
    }
}

/// Returns `true` if legacy callers should auto-approve without policy.
///
/// New MCP tool, resource, and prompt paths must use [`approval_decision`]
/// instead. This helper remains for older non-tool paths until they receive
/// explicit per-operation trust metadata.
pub fn should_auto_approve(execution_mode: ExecutionMode, headless: bool) -> bool {
    headless || execution_mode == ExecutionMode::FullAutonomous
}

/// Prompt the user interactively for approval of an MCP operation.
///
/// Prints `"{description} Allow? [y/N] "` to stderr, flushes the stream so
/// the prompt is visible before blocking, then reads a single line from stdin.
/// Returns `Ok(true)` when the user types `"y"` or `"yes"` (case-insensitive),
/// and `Ok(false)` for any other input.
///
/// # Arguments
///
/// * `description` - Human-readable description of the operation being
///   approved.
///
/// # Returns
///
/// `Ok(true)` when the user approves, `Ok(false)` when the user rejects or
/// provides no input.
///
/// # Errors
///
/// Returns [`XzatomaError::Tool`] if reading from stdin fails.
pub fn prompt_user_approval(description: &str) -> Result<bool> {
    eprint!("{} Allow? [y/N] ", description);
    if let Err(error) = std::io::stderr().flush() {
        tracing::warn!(%error, "Failed to flush MCP approval prompt");
    }

    let mut line = String::new();
    let stdin = std::io::stdin();
    let mut locked = stdin.lock();
    locked
        .read_line(&mut line)
        .map_err(|e| XzatomaError::Tool(format!("Failed to read approval from stdin: {}", e)))?;

    let trimmed = line.trim().to_lowercase();
    Ok(trimmed == "y" || trimmed == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn trusted_policy_with_tool(action: McpApprovalAction) -> McpServerApprovalPolicy {
        let mut tools = HashMap::new();
        tools.insert("search".to_string(), action);
        McpServerApprovalPolicy {
            trusted: true,
            default_tool_action: McpApprovalAction::Prompt,
            tools,
            resource_read_action: McpApprovalAction::Prompt,
            prompt_get_action: McpApprovalAction::Prompt,
        }
    }

    #[test]
    fn test_approval_decision_trusted_allowed_tool_returns_allow() {
        let policy = trusted_policy_with_tool(McpApprovalAction::Allow);
        let decision = approval_decision(
            &policy,
            ExecutionMode::FullAutonomous,
            true,
            McpOperation::ToolCall {
                server_id: "srv",
                tool_name: "search",
            },
        );
        assert_eq!(decision, ApprovalDecision::Allow);
    }

    #[test]
    fn test_approval_decision_untrusted_headless_prompt_returns_deny() {
        let policy = McpServerApprovalPolicy::default();
        let decision = approval_decision(
            &policy,
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
    fn test_approval_decision_explicit_deny_wins() {
        let policy = trusted_policy_with_tool(McpApprovalAction::Deny);
        let decision = approval_decision(
            &policy,
            ExecutionMode::FullAutonomous,
            false,
            McpOperation::ToolCall {
                server_id: "srv",
                tool_name: "search",
            },
        );
        assert_eq!(decision, ApprovalDecision::Deny);
    }

    #[test]
    fn test_approval_decision_untrusted_interactive_prompts() {
        let policy = McpServerApprovalPolicy::default();
        let decision = approval_decision(
            &policy,
            ExecutionMode::Interactive,
            false,
            McpOperation::ResourceRead { server_id: "srv" },
        );
        assert_eq!(decision, ApprovalDecision::Prompt);
    }

    #[test]
    fn test_should_auto_approve_legacy_full_autonomous_returns_true() {
        assert!(should_auto_approve(ExecutionMode::FullAutonomous, false));
    }

    #[test]
    fn test_prompt_user_approval_signature_is_callable() {
        let _f: fn(&str) -> Result<bool> = prompt_user_approval;
    }
}
