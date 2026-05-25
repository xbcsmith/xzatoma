//! System prompts for different chat modes
//!
//! This module provides mode-specific system prompts that guide the AI agent's
//! behavior in different operating modes (Planning vs Write).

pub mod planning_prompt;
pub mod write_prompt;

use crate::chat_mode::{ChatMode, SafetyMode};
use crate::skills::activation::ActiveSkillRegistry;

/// Builds a mode-specific system prompt.
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
    build_system_prompt_with_skill_disclosure(mode, safety, None)
}

/// Builds a mode-specific system prompt with an optional skill disclosure section.
///
/// When a disclosure section is provided and non-empty, it is appended to the
/// base mode prompt with clear separation. This keeps prompt construction
/// centralized while allowing startup-time skill catalog disclosure to be
/// injected without modifying the underlying mode prompts.
///
/// # Arguments
///
/// * `mode` - The current ChatMode (Planning or Write)
/// * `safety` - The current SafetyMode (AlwaysConfirm or NeverConfirm)
/// * `skill_disclosure` - Optional rendered skill disclosure section
///
/// # Returns
///
/// A string containing the system prompt for the given mode, optionally
/// extended with a skill disclosure section
///
/// # Examples
///
/// ```
/// use xzatoma::chat_mode::{ChatMode, SafetyMode};
/// use xzatoma::prompts::build_system_prompt_with_skill_disclosure;
///
/// let prompt = build_system_prompt_with_skill_disclosure(
///     ChatMode::Planning,
///     SafetyMode::AlwaysConfirm,
///     Some("## Available Skills\n- example_skill: Example description"),
/// );
///
/// assert!(prompt.contains("PLANNING"));
/// assert!(prompt.contains("## Available Skills"));
/// assert!(prompt.contains("example_skill"));
/// ```
pub fn build_system_prompt_with_skill_disclosure(
    mode: ChatMode,
    safety: SafetyMode,
    skill_disclosure: Option<&str>,
) -> String {
    let base_prompt = match mode {
        ChatMode::Planning => planning_prompt::generate_planning_prompt(safety),
        ChatMode::Write => write_prompt::generate_write_prompt(safety),
    };

    append_skill_disclosure_section(&base_prompt, skill_disclosure)
}

/// Appends a rendered skill disclosure section to an existing prompt.
///
/// Empty or whitespace-only disclosure content is ignored.
///
/// # Arguments
///
/// * `base_prompt` - Existing system prompt content
/// * `skill_disclosure` - Optional disclosure section to append
///
/// # Returns
///
/// The original prompt when no disclosure is provided, otherwise the prompt with
/// the disclosure appended
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::append_skill_disclosure_section;
///
/// let prompt = append_skill_disclosure_section(
///     "Base prompt",
///     Some("## Available Skills\n- example_skill: Example description"),
/// );
///
/// assert!(prompt.contains("Base prompt"));
/// assert!(prompt.contains("## Available Skills"));
/// ```
pub fn append_skill_disclosure_section(
    base_prompt: &str,
    skill_disclosure: Option<&str>,
) -> String {
    match skill_disclosure.map(str::trim) {
        Some(disclosure) if !disclosure.is_empty() => {
            format!("{base_prompt}\n\n{disclosure}")
        }
        _ => base_prompt.to_string(),
    }
}

/// Builds a mode-specific system prompt with both skill disclosure and active
/// skill injection.
///
/// This helper centralizes prompt assembly:
///
/// - base mode prompt
/// - optional skill catalog disclosure
/// - optional currently active skill content
///
/// Active skills are appended after disclosure so the model sees availability
/// information first and active instruction content second.
///
/// # Arguments
///
/// * `mode` - The current ChatMode (Planning or Write)
/// * `safety` - The current SafetyMode (AlwaysConfirm or NeverConfirm)
/// * `skill_disclosure` - Optional rendered skill disclosure section
/// * `active_skills` - Active skill registry for prompt-layer injection
///
/// # Returns
///
/// A string containing the full system prompt for the current runtime state
///
/// # Examples
///
/// ```
/// use xzatoma::chat_mode::{ChatMode, SafetyMode};
/// use xzatoma::prompts::build_system_prompt_with_skills;
/// use xzatoma::skills::activation::ActiveSkillRegistry;
///
/// let registry = ActiveSkillRegistry::new();
/// let prompt = build_system_prompt_with_skills(
///     ChatMode::Planning,
///     SafetyMode::AlwaysConfirm,
///     Some("## Available Skills\n- example_skill: Example description"),
///     &registry,
/// );
///
/// assert!(prompt.contains("PLANNING"));
/// assert!(prompt.contains("## Available Skills"));
/// ```
pub fn build_system_prompt_with_skills(
    mode: ChatMode,
    safety: SafetyMode,
    skill_disclosure: Option<&str>,
    active_skills: &ActiveSkillRegistry,
) -> String {
    let prompt = build_system_prompt_with_skill_disclosure(mode, safety, skill_disclosure);
    append_active_skills_section(&prompt, active_skills)
}

/// Appends currently active skill content to an existing prompt.
///
/// When the active skill registry is empty, this function returns the original
/// prompt unchanged. Otherwise it appends the registry's prompt-injection-ready
/// content as a separate section.
///
/// # Arguments
///
/// * `base_prompt` - Existing prompt content
/// * `active_skills` - Active skill registry
///
/// # Returns
///
/// The original prompt or the prompt extended with active skill content
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::append_active_skills_section;
/// use xzatoma::skills::activation::ActiveSkillRegistry;
///
/// let registry = ActiveSkillRegistry::new();
/// let prompt = append_active_skills_section("Base prompt", &registry);
///
/// assert_eq!(prompt, "Base prompt");
/// ```
pub fn append_active_skills_section(
    base_prompt: &str,
    active_skills: &ActiveSkillRegistry,
) -> String {
    match active_skills.render_for_prompt_injection() {
        Some(active_section) if !active_section.trim().is_empty() => {
            format!("{base_prompt}\n\n{active_section}")
        }
        _ => base_prompt.to_string(),
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

    #[test]
    fn test_append_skill_disclosure_section_with_disclosure() {
        let prompt = append_skill_disclosure_section(
            "Base prompt",
            Some("## Available Skills\n- example_skill: Example description"),
        );

        assert!(prompt.contains("Base prompt"));
        assert!(prompt.contains("## Available Skills"));
        assert!(prompt.contains("example_skill"));
    }

    #[test]
    fn test_append_skill_disclosure_section_without_disclosure() {
        let prompt = append_skill_disclosure_section("Base prompt", None);

        assert_eq!(prompt, "Base prompt");
    }

    #[test]
    fn test_append_skill_disclosure_section_with_empty_disclosure() {
        let prompt = append_skill_disclosure_section("Base prompt", Some("   \n\t  "));

        assert_eq!(prompt, "Base prompt");
    }

    #[test]
    fn test_build_system_prompt_with_skill_disclosure_appends_section() {
        let prompt = build_system_prompt_with_skill_disclosure(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            Some("## Available Skills\n- example_skill: Example description"),
        );

        assert!(prompt.contains("PLANNING"));
        assert!(prompt.contains("## Available Skills"));
        assert!(prompt.contains("example_skill"));
    }

    #[test]
    fn test_build_system_prompt_with_skill_disclosure_omits_empty_section() {
        let base = build_system_prompt(ChatMode::Write, SafetyMode::AlwaysConfirm);
        let with_empty = build_system_prompt_with_skill_disclosure(
            ChatMode::Write,
            SafetyMode::AlwaysConfirm,
            Some(""),
        );

        assert_eq!(base, with_empty);
    }

    #[test]
    fn test_append_active_skills_section_without_active_skills() {
        let registry = ActiveSkillRegistry::new();
        let prompt = append_active_skills_section("Base prompt", &registry);

        assert_eq!(prompt, "Base prompt");
    }

    #[test]
    fn test_build_system_prompt_with_skills_includes_disclosure_when_present() {
        let registry = ActiveSkillRegistry::new();
        let prompt = build_system_prompt_with_skills(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            Some("## Available Skills\n- example_skill: Example description"),
            &registry,
        );

        assert!(prompt.contains("PLANNING"));
        assert!(prompt.contains("## Available Skills"));
        assert!(prompt.contains("example_skill"));
    }

    #[test]
    fn test_build_system_prompt_with_skills_without_disclosure_matches_base_when_no_active_skills()
    {
        let registry = ActiveSkillRegistry::new();
        let base = build_system_prompt(ChatMode::Write, SafetyMode::AlwaysConfirm);
        let prompt = build_system_prompt_with_skills(
            ChatMode::Write,
            SafetyMode::AlwaysConfirm,
            None,
            &registry,
        );

        assert_eq!(base, prompt);
    }
}
