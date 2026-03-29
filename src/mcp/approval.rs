//! MCP tool auto-approval policy and interactive confirmation helpers
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

use std::io::{BufRead, Write};

use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};

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

/// Prompt the user interactively for approval of an MCP operation.
///
/// Prints `"{description} Allow? [y/N] "` to stderr, flushes the stream so
/// the prompt is visible before blocking, then reads a single line from stdin.
/// Returns `Ok(true)` when the user types `"y"` or `"yes"` (case-insensitive),
/// and `Ok(false)` for any other input (including an empty line, which
/// corresponds to the default `N`).
///
/// This function is the single canonical stdin-based approval prompt. All
/// interactive MCP executor implementations must delegate here rather than
/// duplicating the read logic inline.
///
/// # Arguments
///
/// * `description` - Human-readable description of the operation being
///   approved, printed before `" Allow? [y/N] "`.
///
/// # Returns
///
/// `Ok(true)` when the user approves, `Ok(false)` when the user rejects or
/// provides no input.
///
/// # Errors
///
/// Returns [`XzatomaError::Tool`] if reading from stdin fails.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::approval::prompt_user_approval;
///
/// // In a test or interactive context, this would read from stdin.
/// // let approved = prompt_user_approval("delete /tmp/foo")?;
/// ```
pub fn prompt_user_approval(description: &str) -> Result<bool> {
    eprint!("{} Allow? [y/N] ", description);
    // Flush stderr so the prompt is visible before we block on stdin.
    let _ = std::io::stderr().flush();

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

    // prompt_user_approval cannot be tested with live stdin in a unit test, but
    // we can verify the function exists and has the correct signature by
    // referencing it without calling it.
    #[test]
    fn test_prompt_user_approval_signature_is_callable() {
        // Confirm the function is importable and has the expected type.
        // We do NOT call it (it would block on stdin during `cargo test`).
        let _f: fn(&str) -> Result<bool> = prompt_user_approval;
    }
}
