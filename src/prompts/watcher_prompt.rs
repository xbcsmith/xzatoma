//! Watcher mode system prompt
//!
//! This module provides the system prompt for watcher mode, which instructs
//! the agent to operate autonomously — executing every task immediately using
//! available tools without asking for confirmation or describing the plan first.

use crate::chat_mode::SafetyMode;

/// Generates the system prompt for watcher (autonomous) mode.
///
/// In watcher mode the agent receives a plan from a Kafka event and must
/// execute every task without human interaction. The prompt explicitly
/// forbids describing, summarising, or asking for confirmation — the agent
/// must call tools directly and immediately.
///
/// The `safety` parameter is accepted for API consistency with other prompt
/// generators but is ignored: watcher mode always operates without confirmation
/// because there is no human present to provide it.
///
/// # Arguments
///
/// * `_safety` - Accepted for signature consistency; always ignored.
///
/// # Returns
///
/// A system prompt string for autonomous watcher execution.
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::watcher_prompt::generate_watcher_prompt;
/// use xzatoma::chat_mode::SafetyMode;
///
/// let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
/// assert!(prompt.contains("autonomous"));
/// assert!(prompt.contains("tools"));
/// ```
pub fn generate_watcher_prompt(_safety: SafetyMode) -> String {
    r#"You are an autonomous XZatoma watcher agent operating without a human in the loop.

CRITICAL OPERATING RULES:
1. Execute EVERY task immediately using your available tools.
2. Do NOT describe what you plan to do — just do it.
3. Do NOT ask for confirmation or clarification.
4. Do NOT summarise the plan before starting — start executing now.
5. Use tools for every action: terminal for commands, write_file for file writes.

AVAILABLE TOOLS AND WHEN TO USE THEM:
- terminal: run shell commands, create directories, verify files exist
- write_file: create or overwrite files with content
- read_file: read file contents when required as input for a subsequent step
- list_directory: discover files when searching for targets
- find_path / grep: search for content within files

EXECUTION APPROACH:
1. Read the task instruction carefully.
2. Identify the concrete actions required (commands to run, files to write, etc.).
3. Call the appropriate tool for each action immediately.
4. After each tool call, use the result to inform the next step.
5. Continue until ALL tasks in the plan are complete.
6. Once all tasks are done, respond with a single concise paragraph summarising
   what was accomplished and whether each task succeeded or failed.

IMPORTANT:
- You are the sole executor — there is no human waiting to approve actions.
- Errors from tools are expected; diagnose and retry or work around them.
- If a task cannot be completed, record the reason and continue to the next task.
- Never stop mid-plan to ask a question."#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_mode::SafetyMode;

    #[test]
    fn test_watcher_prompt_contains_autonomous() {
        let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
        assert!(
            prompt.contains("autonomous"),
            "watcher prompt must mention autonomous operation"
        );
    }

    #[test]
    fn test_watcher_prompt_contains_tools() {
        let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
        assert!(
            prompt.contains("tools"),
            "watcher prompt must mention tools"
        );
    }

    #[test]
    fn test_watcher_prompt_forbids_asking_for_confirmation() {
        let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
        let lower = prompt.to_lowercase();
        // The prompt must explicitly forbid asking for confirmation.
        assert!(
            lower.contains("do not ask") || lower.contains("confirmation"),
            "watcher prompt must address confirmation behaviour"
        );
    }

    #[test]
    fn test_watcher_prompt_safety_mode_ignored() {
        // Both safety modes must produce the same prompt because watcher
        // always operates without confirmation.
        let prompt_safe = generate_watcher_prompt(SafetyMode::AlwaysConfirm);
        let prompt_yolo = generate_watcher_prompt(SafetyMode::NeverConfirm);
        assert_eq!(
            prompt_safe, prompt_yolo,
            "watcher prompt must be identical regardless of safety mode"
        );
    }

    #[test]
    fn test_watcher_prompt_not_empty() {
        let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
        assert!(!prompt.is_empty());
        assert!(
            prompt.len() > 200,
            "watcher prompt must be substantive (>200 chars)"
        );
    }

    #[test]
    fn test_watcher_prompt_names_specific_tools() {
        let prompt = generate_watcher_prompt(SafetyMode::NeverConfirm);
        assert!(
            prompt.contains("terminal") || prompt.contains("write_file"),
            "watcher prompt must name at least one specific tool"
        );
    }
}
