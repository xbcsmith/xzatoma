//! Write mode system prompt
//!
//! This module provides the system prompt for write mode, which guides the agent
//! to execute tasks with full read/write access and terminal command execution.

use crate::chat_mode::SafetyMode;

/// Generates the system prompt for write mode
///
/// In write mode, the agent has full capabilities to read and write files,
/// create and delete files, and execute terminal commands. The safety mode
/// is prominently featured to ensure awareness of confirmation requirements.
///
/// # Arguments
///
/// * `safety` - The current SafetyMode (AlwaysConfirm or NeverConfirm)
///
/// # Returns
///
/// A system prompt string tailored for write mode
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::write_prompt::generate_write_prompt;
/// use xzatoma::chat_mode::SafetyMode;
///
/// let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
/// assert!(prompt.contains("WRITE"));
/// ```
pub fn generate_write_prompt(safety: SafetyMode) -> String {
    let safety_instructions = match safety {
        SafetyMode::AlwaysConfirm => {
            r#"SAFETY MODE: ENABLED (CONFIRMATION REQUIRED)
Your safety mode requires explicit confirmation before executing potentially dangerous operations:
- File deletions
- Command executions
- Large batch modifications
- Destructive operations

When uncertainty exists, ASK FOR CONFIRMATION before proceeding.
Phrase confirmation requests clearly: "Should I proceed with [operation]?"

Example:
User: "Delete all .log files"
You: "I can delete .log files, but this is a destructive operation. Should I proceed?"
[Wait for user confirmation before executing]"#
        }
        SafetyMode::NeverConfirm => {
            r#"SAFETY MODE: DISABLED (YOLO MODE)
Operations will proceed WITHOUT confirmation. This is a high-risk configuration.
- No confirmation needed before file deletions
- No confirmation needed for command execution
- All operations are immediate and irreversible

USE WITH EXTREME CAUTION. Ensure you understand the full impact of your actions.
Proceed efficiently without asking for confirmation."#
        }
    };

    format!(
        r#"You are in WRITE mode. You have full access to the system and can execute any task.

MODE CAPABILITIES:
You HAVE access to:
- Read files and directories
- Write and modify files
- Create new files and directories
- Delete files and directories
- Execute terminal commands
- Search and analyze the codebase
- Make comprehensive changes to the system

YOUR ROLE:
Execute tasks efficiently and effectively. Use all available tools to:
1. Understand the current state (read, search, list)
2. Plan the changes needed
3. Execute the changes (write, create, delete, run commands)
4. Verify the results
5. Report what was accomplished

IMPORTANT CAPABILITIES:
- You can run tests, builds, and development commands
- You can modify multiple files to accomplish a goal
- You can execute scripts and automation
- You can check the status and output of commands

{}

EXECUTION GUIDELINES:
1. Before major changes, review the affected files
2. Make incremental, logical changes
3. Test your changes when appropriate
4. If a command fails, analyze the error and try alternatives
5. Report the final status and any issues encountered

AVOID:
- Making unnecessary changes
- Leaving work incomplete
- Ignoring error messages
- Making risky changes without understanding the impact

Remember: You have the power to make significant changes. Use it responsibly but effectively."#,
        safety_instructions
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_mode::SafetyMode;

    #[test]
    fn test_write_prompt_safe_mode() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("WRITE"));
        assert!(prompt.contains("ENABLED"));
        assert!(prompt.to_lowercase().contains("confirmation"));
        assert!(prompt.contains("Should I proceed"));
        assert!(prompt.to_lowercase().contains("ask for confirmation"));
    }

    #[test]
    fn test_write_prompt_yolo_mode() {
        let prompt = generate_write_prompt(SafetyMode::NeverConfirm);
        assert!(prompt.contains("WRITE"));
        assert!(prompt.contains("DISABLED"));
        assert!(prompt.contains("YOLO"));
        assert!(prompt.to_lowercase().contains("without confirmation"));
    }

    #[test]
    fn test_write_prompt_includes_capabilities() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.to_lowercase().contains("read files"));
        assert!(prompt.to_lowercase().contains("write"));
        assert!(prompt.to_lowercase().contains("delete"));
        assert!(prompt.to_lowercase().contains("terminal commands"));
        assert!(prompt.to_lowercase().contains("execute"));
    }

    #[test]
    fn test_write_prompt_includes_guidelines() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("EXECUTION GUIDELINES"));
        assert!(prompt.contains("incremental"));
        assert!(prompt.contains("Test your changes"));
    }

    #[test]
    fn test_write_prompt_not_empty() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(!prompt.is_empty());
        assert!(prompt.len() > 300);
    }

    #[test]
    fn test_write_prompt_safe_vs_yolo_different() {
        let safe_prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        let yolo_prompt = generate_write_prompt(SafetyMode::NeverConfirm);

        // The prompts should be different based on safety mode
        assert_ne!(safe_prompt, yolo_prompt);

        // Safe should have confirmation language, yolo should have warning
        assert!(safe_prompt.contains("ENABLED"));
        assert!(yolo_prompt.contains("DISABLED"));
    }

    #[test]
    fn test_write_prompt_includes_example() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("Example:"));
        assert!(prompt.contains("Delete all .log files"));
    }

    #[test]
    fn test_write_prompt_role_description() {
        let prompt = generate_write_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("Execute tasks efficiently"));
        assert!(prompt.contains("YOUR ROLE:"));
    }
}
