//! System prompt resolution for XZatoma agent sessions.
//!
//! This module implements the precedence logic that determines which system
//! prompt is active for a given agent session. Multiple configuration channels
//! can provide a system prompt; this module resolves them to a single value
//! according to a fixed priority order.
//!
//! # Precedence (highest to lowest)
//!
//! 1. Plan file `system_prompt` field
//! 2. `--system-prompt` CLI flag
//! 3. `agent.system_prompt` config file field (also populated by the
//!    `XZATOMA_SYSTEM_PROMPT` environment variable via `apply_env_vars`)
//! 4. No override -- `resolve` returns `None` and callers fall back to the
//!    mode-specific base prompt.

/// The source from which a resolved system prompt originated.
///
/// Used in trace-level logging so that operators can determine at runtime
/// which configuration channel supplied the active system prompt.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::system_prompt::SystemPromptSource;
///
/// let source = SystemPromptSource::Plan;
/// println!("{source:?}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPromptSource {
    /// The prompt came from a plan file's `system_prompt` field.
    Plan,
    /// The prompt came from the `--system-prompt` CLI flag.
    CliFlag,
    /// The prompt came from the `agent.system_prompt` configuration field.
    ///
    /// This variant also covers prompts set via the `XZATOMA_SYSTEM_PROMPT`
    /// environment variable, because `apply_env_vars` writes that variable
    /// into `config.agent.system_prompt` before any command function runs.
    Config,
    /// The prompt was built from the mode-specific default template.
    ///
    /// This variant is reserved for callers that fall back to a mode-specific
    /// base prompt when `resolve` returns `None`. It is never returned by
    /// `resolve` itself.
    Default,
}

/// A system prompt value together with the source that supplied it.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::system_prompt::{ResolvedSystemPrompt, SystemPromptSource};
///
/// let resolved = ResolvedSystemPrompt {
///     text: "You are a helpful assistant.".to_string(),
///     source: SystemPromptSource::CliFlag,
/// };
/// assert_eq!(resolved.text, "You are a helpful assistant.");
/// assert_eq!(resolved.source, SystemPromptSource::CliFlag);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSystemPrompt {
    /// The resolved system prompt text.
    pub text: String,
    /// The configuration source that provided the prompt.
    pub source: SystemPromptSource,
}

/// Resolves the effective system prompt from the available configuration sources.
///
/// Applies the following precedence order (highest to lowest):
///
/// 1. `plan_prompt` -- prompt declared in a plan file.
/// 2. `cli_flag` -- value of the `--system-prompt` CLI flag.
/// 3. `config_prompt` -- value of `agent.system_prompt` in the config file or
///    from the `XZATOMA_SYSTEM_PROMPT` environment variable (written by
///    `apply_env_vars`).
///
/// Any input that is `None` or contains only whitespace is treated as absent
/// and skipped. If all three inputs are absent, `None` is returned and the
/// caller should fall back to the mode-specific default prompt.
///
/// # Arguments
///
/// * `plan_prompt` - Optional system prompt from a plan file.
/// * `cli_flag` - Optional value of the `--system-prompt` CLI flag.
/// * `config_prompt` - Optional value from `config.agent.system_prompt`.
///
/// # Returns
///
/// `Some(ResolvedSystemPrompt)` when at least one non-blank source is present,
/// otherwise `None`.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::system_prompt::{resolve, SystemPromptSource};
///
/// // Plan wins over CLI flag
/// let result = resolve(Some("from plan"), Some("from cli"), None);
/// let resolved = result.unwrap();
/// assert_eq!(resolved.text, "from plan");
/// assert_eq!(resolved.source, SystemPromptSource::Plan);
///
/// // CLI flag wins over config
/// let result = resolve(None, Some("from cli"), Some("from config"));
/// let resolved = result.unwrap();
/// assert_eq!(resolved.text, "from cli");
/// assert_eq!(resolved.source, SystemPromptSource::CliFlag);
///
/// // Config is used when no higher-priority source is present
/// let result = resolve(None, None, Some("from config"));
/// let resolved = result.unwrap();
/// assert_eq!(resolved.text, "from config");
/// assert_eq!(resolved.source, SystemPromptSource::Config);
///
/// // All absent returns None
/// assert!(resolve(None, None, None).is_none());
/// ```
pub fn resolve(
    plan_prompt: Option<&str>,
    cli_flag: Option<&str>,
    config_prompt: Option<&str>,
) -> Option<ResolvedSystemPrompt> {
    // Treat None and whitespace-only strings as absent.
    if let Some(text) = plan_prompt.filter(|v| !v.trim().is_empty()) {
        return Some(ResolvedSystemPrompt {
            text: text.to_string(),
            source: SystemPromptSource::Plan,
        });
    }

    if let Some(text) = cli_flag.filter(|v| !v.trim().is_empty()) {
        return Some(ResolvedSystemPrompt {
            text: text.to_string(),
            source: SystemPromptSource::CliFlag,
        });
    }

    if let Some(text) = config_prompt.filter(|v| !v.trim().is_empty()) {
        return Some(ResolvedSystemPrompt {
            text: text.to_string(),
            source: SystemPromptSource::Config,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- resolve: precedence tests ---

    #[test]
    fn test_resolve_plan_wins_over_cli_and_config() {
        let result = resolve(
            Some("plan prompt"),
            Some("cli prompt"),
            Some("config prompt"),
        );
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "plan prompt");
        assert_eq!(resolved.source, SystemPromptSource::Plan);
    }

    #[test]
    fn test_resolve_cli_wins_over_config_when_no_plan() {
        let result = resolve(None, Some("cli prompt"), Some("config prompt"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "cli prompt");
        assert_eq!(resolved.source, SystemPromptSource::CliFlag);
    }

    #[test]
    fn test_resolve_config_used_when_only_source() {
        let result = resolve(None, None, Some("config prompt"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "config prompt");
        assert_eq!(resolved.source, SystemPromptSource::Config);
    }

    #[test]
    fn test_resolve_all_absent_returns_none() {
        assert!(resolve(None, None, None).is_none());
    }

    // --- resolve: blank-string handling ---

    #[test]
    fn test_resolve_blank_plan_is_treated_as_absent() {
        let result = resolve(Some("   "), Some("cli prompt"), None);
        let resolved = result.unwrap();
        assert_eq!(resolved.source, SystemPromptSource::CliFlag);
    }

    #[test]
    fn test_resolve_blank_cli_is_treated_as_absent() {
        let result = resolve(None, Some("\t\n"), Some("config prompt"));
        let resolved = result.unwrap();
        assert_eq!(resolved.source, SystemPromptSource::Config);
    }

    #[test]
    fn test_resolve_blank_config_is_treated_as_absent() {
        let result = resolve(None, None, Some("  "));
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_all_blank_returns_none() {
        assert!(resolve(Some(" "), Some(" "), Some(" ")).is_none());
    }

    // --- resolve: only one source present ---

    #[test]
    fn test_resolve_only_plan_present() {
        let result = resolve(Some("plan only"), None, None);
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "plan only");
        assert_eq!(resolved.source, SystemPromptSource::Plan);
    }

    #[test]
    fn test_resolve_only_cli_present() {
        let result = resolve(None, Some("cli only"), None);
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "cli only");
        assert_eq!(resolved.source, SystemPromptSource::CliFlag);
    }

    // --- ResolvedSystemPrompt struct ---

    #[test]
    fn test_resolved_system_prompt_fields_are_public() {
        let r = ResolvedSystemPrompt {
            text: "hello".to_string(),
            source: SystemPromptSource::Config,
        };
        assert_eq!(r.text, "hello");
        assert_eq!(r.source, SystemPromptSource::Config);
    }

    #[test]
    fn test_resolved_system_prompt_clone() {
        let original = ResolvedSystemPrompt {
            text: "test".to_string(),
            source: SystemPromptSource::Plan,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    // --- SystemPromptSource variants ---

    #[test]
    fn test_system_prompt_source_debug() {
        assert_eq!(format!("{:?}", SystemPromptSource::Plan), "Plan");
        assert_eq!(format!("{:?}", SystemPromptSource::CliFlag), "CliFlag");
        assert_eq!(format!("{:?}", SystemPromptSource::Config), "Config");
        assert_eq!(format!("{:?}", SystemPromptSource::Default), "Default");
    }

    #[test]
    fn test_system_prompt_source_equality() {
        assert_eq!(SystemPromptSource::Plan, SystemPromptSource::Plan);
        assert_ne!(SystemPromptSource::Plan, SystemPromptSource::CliFlag);
        assert_ne!(SystemPromptSource::CliFlag, SystemPromptSource::Config);
        assert_ne!(SystemPromptSource::Config, SystemPromptSource::Default);
    }
}
