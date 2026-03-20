//! MCP tool auto-approval policy
//!
//! This module is the single authoritative source for determining whether MCP
//! tool calls should be auto-approved without prompting the user.
//!
//! No inline policy checks are permitted elsewhere in the codebase. All
//! callers must delegate to [`should_auto_approve`].
//!
//! # Policy
//!
//! Auto-approval applies when either of the following conditions holds:
//!
//! - `execution_mode == ExecutionMode::FullAutonomous` -- the user has
//!   explicitly opted into unrestricted autonomous operation.
//! - `headless == true` -- the run command is non-interactive; all tool calls
//!   within a plan must proceed without blocking for user input.
//!
//! All other modes (`Interactive`, `RestrictedAutonomous`) with `headless ==
//! false` require presenting a confirmation prompt for each MCP tool
//! invocation.

use crate::config::ExecutionMode;

/// Returns `true` if MCP tool calls should be auto-approved without prompting
/// the user.
///
/// Auto-approval applies when:
///
/// - `execution_mode == ExecutionMode::FullAutonomous`: the user has
///   explicitly opted into unrestricted autonomous operation, OR
/// - `headless == true`: the run command is always non-interactive; all
///   tool calls within a plan must proceed without blocking for user input.
///
/// All other modes (`Interactive`, `RestrictedAutonomous`) require presenting
/// a confirmation prompt for each MCP tool invocation.
///
/// # Arguments
///
/// * `execution_mode` - The current agent execution mode.
/// * `headless` - Whether the agent is running in headless (non-interactive)
///   mode, such as within the `run` command executing a plan.
///
/// # Returns
///
/// `true` when the tool call may proceed without user confirmation; `false`
/// when a confirmation prompt must be displayed before calling the tool.
///
/// # Examples
///
/// ```
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::approval::should_auto_approve;
///
/// // FullAutonomous always approves regardless of headless flag.
/// assert!(should_auto_approve(ExecutionMode::FullAutonomous, false));
/// assert!(should_auto_approve(ExecutionMode::FullAutonomous, true));
///
/// // Headless always approves regardless of execution mode.
/// assert!(should_auto_approve(ExecutionMode::Interactive, true));
/// assert!(should_auto_approve(ExecutionMode::RestrictedAutonomous, true));
///
/// // Interactive non-headless requires confirmation.
/// assert!(!should_auto_approve(ExecutionMode::Interactive, false));
///
/// // RestrictedAutonomous non-headless requires confirmation.
/// assert!(!should_auto_approve(ExecutionMode::RestrictedAutonomous, false));
/// ```
pub fn should_auto_approve(execution_mode: ExecutionMode, headless: bool) -> bool {
    headless || execution_mode == ExecutionMode::FullAutonomous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_auto_approve_full_autonomous_non_headless_returns_true() {
        assert!(should_auto_approve(ExecutionMode::FullAutonomous, false));
    }

    #[test]
    fn test_should_auto_approve_full_autonomous_headless_returns_true() {
        assert!(should_auto_approve(ExecutionMode::FullAutonomous, true));
    }

    #[test]
    fn test_should_auto_approve_interactive_headless_returns_true() {
        assert!(should_auto_approve(ExecutionMode::Interactive, true));
    }

    #[test]
    fn test_should_auto_approve_restricted_autonomous_headless_returns_true() {
        assert!(should_auto_approve(
            ExecutionMode::RestrictedAutonomous,
            true
        ));
    }

    #[test]
    fn test_should_auto_approve_interactive_non_headless_returns_false() {
        assert!(!should_auto_approve(ExecutionMode::Interactive, false));
    }

    #[test]
    fn test_should_auto_approve_restricted_autonomous_non_headless_returns_false() {
        assert!(!should_auto_approve(
            ExecutionMode::RestrictedAutonomous,
            false
        ));
    }

    #[test]
    fn test_should_auto_approve_headless_flag_overrides_interactive_mode() {
        // headless=true must override even the most restrictive mode.
        assert!(should_auto_approve(ExecutionMode::Interactive, true));
    }

    #[test]
    fn test_should_auto_approve_full_autonomous_ignores_headless_false() {
        // FullAutonomous must approve regardless of headless value.
        assert!(should_auto_approve(ExecutionMode::FullAutonomous, false));
    }
}
