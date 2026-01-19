//! System prompts for different chat modes
//!
//! This module provides mode-specific system prompts that guide the AI agent's
//! behavior in different operating modes (Planning vs Write).

pub mod planning_prompt;
pub mod write_prompt;

use crate::chat_mode::{ChatMode, SafetyMode};

/// Builds a mode-specific system prompt
///
/// Returns a system prompt tailored to the current chat mode and safety setting.
/// The prompt instructs the AI on what it can and cannot do in the current mode.
///
/// # Arguments
///
/// * `mode` - The current ChatMode (Planning or Write)
/// * `safety` - The current SafetyMode (AlwaysConfirm or NeverConfirm)
///
/// # Returns
///
/// A string containing the system prompt for the given mode
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::build_system_prompt;
/// use xzatoma::chat_mode::{ChatMode, SafetyMode};
///
/// let prompt = build_system_prompt(ChatMode::Planning, SafetyMode::AlwaysConfirm);
/// assert!(prompt.contains("PLANNING"));
/// assert!(prompt.contains("read"));
/// ```
pub fn build_system_prompt(mode: ChatMode, safety: SafetyMode) -> String {
    match mode {
        ChatMode::Planning => planning_prompt::generate_planning_prompt(safety),
        ChatMode::Write => write_prompt::generate_write_prompt(safety),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_planning_safe() {
        let prompt = build_system_prompt(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("PLANNING"));
        assert!(prompt.to_lowercase().contains("read"));
        assert!(prompt.to_lowercase().contains("cannot"));
        assert!(prompt.contains("ENABLED"));
    }

    #[test]
    fn test_build_system_prompt_planning_yolo() {
        let prompt = build_system_prompt(ChatMode::Planning, SafetyMode::NeverConfirm);
        assert!(prompt.contains("PLANNING"));
        assert!(prompt.to_lowercase().contains("read"));
    }

    #[test]
    fn test_build_system_prompt_write_safe() {
        let prompt = build_system_prompt(ChatMode::Write, SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("WRITE"));
        assert!(prompt.to_lowercase().contains("modify"));
        assert!(prompt.to_lowercase().contains("execute"));
        assert!(prompt.to_lowercase().contains("confirm"));
    }

    #[test]
    fn test_build_system_prompt_write_yolo() {
        let prompt = build_system_prompt(ChatMode::Write, SafetyMode::NeverConfirm);
        assert!(prompt.contains("WRITE"));
        assert!(prompt.to_lowercase().contains("modify"));
        assert!(prompt.to_lowercase().contains("execute"));
    }

    #[test]
    fn test_build_system_prompt_not_empty() {
        let modes = vec![ChatMode::Planning, ChatMode::Write];
        let safeties = vec![SafetyMode::AlwaysConfirm, SafetyMode::NeverConfirm];

        for mode in modes {
            for safety in &safeties {
                let prompt = build_system_prompt(mode, *safety);
                assert!(!prompt.is_empty());
                assert!(
                    prompt.len() > 50,
                    "Prompt too short for {:?} {:?}",
                    mode,
                    safety
                );
            }
        }
    }

    #[test]
    fn test_build_system_prompt_includes_safety_instructions() {
        let safe_prompt = build_system_prompt(ChatMode::Write, SafetyMode::AlwaysConfirm);
        let yolo_prompt = build_system_prompt(ChatMode::Write, SafetyMode::NeverConfirm);

        // Prompts should be different based on safety mode
        assert_ne!(safe_prompt, yolo_prompt);

        // Safe prompt should mention confirmation
        assert!(
            safe_prompt.to_lowercase().contains("confirm")
                || safe_prompt.to_lowercase().contains("safety")
        );
    }
}
