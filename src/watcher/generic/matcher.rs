//! Generic event matcher for the generic Kafka watcher.
//!
//! This module contains the generic watcher's event matching logic. It is
//! intentionally separate from the XZepr-specific `EventFilter` implementation.
//!
//! `GenericMatcher` operates only on [`crate::watcher::generic::message::GenericPlanEvent`]
//! and [`crate::config::GenericMatchConfig`].
//!
//! Matching rules:
//!
//! - `event_type` must be exactly `"plan"` or the event is always rejected.
//! - Configured `action`, `name`, and `version` fields are interpreted as regex patterns.
//! - Matching is case-insensitive by default.
//! - Missing required event fields never match.
//! - When no match fields are configured, all `"plan"` events are accepted.

use crate::config::GenericMatchConfig;
use crate::watcher::generic::message::GenericPlanEvent;
use anyhow::Result;
use regex::Regex;
use std::sync::Arc;

/// Generic event matcher for plan events.
///
/// This matcher evaluates [`GenericPlanEvent`] values against a
/// [`GenericMatchConfig`] using regex-based matching.
///
/// Unlike the XZepr watcher filter, this matcher:
///
/// - operates only on generic plan events
/// - always enforces the `event_type == "plan"` gate first
/// - supports regex matching for `action`, `name`, and `version`
///
/// # Examples
///
/// ```
/// use xzatoma::config::GenericMatchConfig;
/// use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
/// use serde_json::json;
///
/// let matcher = GenericMatcher::new(GenericMatchConfig {
///     action: Some("deploy".to_string()),
///     name: None,
///     version: None,
/// })
/// .unwrap();
///
/// let mut event = GenericPlanEvent::new("evt-1".to_string(), json!({"steps": []}));
/// event.action = Some("Deploy".to_string());
///
/// assert!(matcher.should_process(&event));
/// ```
#[derive(Debug, Clone)]
pub struct GenericMatcher {
    config: GenericMatchConfig,
    compiled_action: Option<Arc<Regex>>,
    compiled_name: Option<Arc<Regex>>,
    compiled_version: Option<Arc<Regex>>,
}

/// Active matching mode for the generic matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchMode {
    /// No match fields are configured; accept all `"plan"` events.
    AcceptAll,
    /// Match only on `action`.
    ActionOnly,
    /// Match on `name` and `version`.
    NameAndVersion,
    /// Match on `name` and `action`.
    NameAndAction,
    /// Match on `name`, `version`, and `action`.
    NameVersionAndAction,
}

impl GenericMatcher {
    /// Create a new generic matcher from configuration.
    ///
    /// All configured match fields are compiled eagerly as regex patterns so that
    /// invalid patterns fail fast at startup instead of during message handling.
    ///
    /// Matching is case-insensitive by default. If the supplied pattern does not
    /// already begin with inline regex flags, `(?i)` is prepended automatically.
    ///
    /// # Arguments
    ///
    /// * `config` - Generic watcher match configuration
    ///
    /// # Returns
    ///
    /// Returns a fully initialized matcher.
    ///
    /// # Errors
    ///
    /// Returns an error if any configured regex pattern is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::GenericMatchConfig;
    /// use xzatoma::watcher::generic::GenericMatcher;
    ///
    /// let matcher = GenericMatcher::new(GenericMatchConfig {
    ///     action: Some("deploy.*".to_string()),
    ///     name: None,
    ///     version: None,
    /// })
    /// .unwrap();
    ///
    /// assert!(matcher.summary().contains("action"));
    /// ```
    pub fn new(config: GenericMatchConfig) -> Result<Self> {
        let compiled_action = compile_optional_pattern(config.action.as_deref())?;
        let compiled_name = compile_optional_pattern(config.name.as_deref())?;
        let compiled_version = compile_optional_pattern(config.version.as_deref())?;

        Ok(Self {
            config,
            compiled_action,
            compiled_name,
            compiled_version,
        })
    }

    /// Return `true` if the event should be processed.
    ///
    /// This method always enforces the type gate first:
    /// any event where `event.event_type != "plan"` is rejected immediately.
    ///
    /// Supported matching modes:
    ///
    /// - `action` only
    /// - `name` + `version`
    /// - `name` + `action`
    /// - `name` + `version` + `action`
    /// - none configured (accept all `"plan"` events)
    ///
    /// Any other partial configuration shape is treated conservatively and only
    /// matches when all configured fields match.
    ///
    /// # Arguments
    ///
    /// * `event` - Event to evaluate
    ///
    /// # Returns
    ///
    /// `true` if the event passes the type gate and configured match criteria.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::GenericMatchConfig;
    /// use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
    /// use serde_json::json;
    ///
    /// let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
    /// let event = GenericPlanEvent::new("evt-1".to_string(), json!(null));
    ///
    /// assert!(matcher.should_process(&event));
    /// ```
    pub fn should_process(&self, event: &GenericPlanEvent) -> bool {
        if !event.is_plan_event() {
            return false;
        }

        match self.mode() {
            MatchMode::AcceptAll => true,
            MatchMode::ActionOnly => self.matches_action(event),
            MatchMode::NameAndVersion => self.matches_name(event) && self.matches_version(event),
            MatchMode::NameAndAction => self.matches_name(event) && self.matches_action(event),
            MatchMode::NameVersionAndAction => {
                self.matches_name(event)
                    && self.matches_version(event)
                    && self.matches_action(event)
            }
        }
    }

    /// Return a human-readable summary of the matcher configuration.
    ///
    /// The summary includes the active matching mode and the configured regex
    /// pattern strings for structured startup logging.
    ///
    /// # Returns
    ///
    /// A summary string describing the active matcher configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::GenericMatchConfig;
    /// use xzatoma::watcher::generic::GenericMatcher;
    ///
    /// let matcher = GenericMatcher::new(GenericMatchConfig {
    ///     action: Some("deploy".to_string()),
    ///     name: Some("service-a".to_string()),
    ///     version: None,
    /// })
    /// .unwrap();
    ///
    /// let summary = matcher.summary();
    /// assert!(summary.contains("name+action"));
    /// ```
    pub fn summary(&self) -> String {
        let mode = match self.mode() {
            MatchMode::AcceptAll => "accept-all",
            MatchMode::ActionOnly => "action-only",
            MatchMode::NameAndVersion => "name+version",
            MatchMode::NameAndAction => "name+action",
            MatchMode::NameVersionAndAction => "name+version+action",
        };

        let mut parts = vec![format!("mode={mode}")];

        if let Some(action) = &self.config.action {
            parts.push(format!("action=/{action}/"));
        }

        if let Some(name) = &self.config.name {
            parts.push(format!("name=/{name}/"));
        }

        if let Some(version) = &self.config.version {
            parts.push(format!("version=/{version}/"));
        }

        parts.join(", ")
    }

    fn mode(&self) -> MatchMode {
        match (
            self.compiled_action.is_some(),
            self.compiled_name.is_some(),
            self.compiled_version.is_some(),
        ) {
            (false, false, false) => MatchMode::AcceptAll,
            (true, false, false) => MatchMode::ActionOnly,
            (false, true, true) => MatchMode::NameAndVersion,
            (true, true, false) => MatchMode::NameAndAction,
            (true, true, true) => MatchMode::NameVersionAndAction,
            _ => self.fallback_mode(),
        }
    }

    fn fallback_mode(&self) -> MatchMode {
        match (
            self.compiled_action.is_some(),
            self.compiled_name.is_some(),
            self.compiled_version.is_some(),
        ) {
            (false, false, false) => MatchMode::AcceptAll,
            (true, false, false) => MatchMode::ActionOnly,
            (false, true, true) => MatchMode::NameAndVersion,
            (true, true, false) => MatchMode::NameAndAction,
            _ => MatchMode::NameVersionAndAction,
        }
    }

    fn matches_action(&self, event: &GenericPlanEvent) -> bool {
        match (&self.compiled_action, &event.action) {
            (Some(regex), Some(value)) => regex.is_match(value),
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    fn matches_name(&self, event: &GenericPlanEvent) -> bool {
        match (&self.compiled_name, &event.name) {
            (Some(regex), Some(value)) => regex.is_match(value),
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    fn matches_version(&self, event: &GenericPlanEvent) -> bool {
        match (&self.compiled_version, &event.version) {
            (Some(regex), Some(value)) => regex.is_match(value),
            (None, _) => true,
            (Some(_), None) => false,
        }
    }
}

/// Compile an optional regex pattern.
///
/// Matching is case-insensitive by default. If the pattern does not begin with
/// inline flags, `(?i)` is prepended automatically.
///
/// # Errors
///
/// Returns an error if the regex pattern is invalid.
fn compile_optional_pattern(pattern: Option<&str>) -> Result<Option<Arc<Regex>>> {
    match pattern {
        Some(pattern) => Ok(Some(Arc::new(compile_pattern(pattern)?))),
        None => Ok(None),
    }
}

/// Compile a single regex pattern with default case-insensitive behavior.
///
/// # Errors
///
/// Returns an error if the pattern is invalid.
fn compile_pattern(pattern: &str) -> Result<Regex> {
    // Now we have 2 problems.
    let effective_pattern = if starts_with_inline_flags(pattern) {
        pattern.to_string()
    } else {
        format!("(?i){pattern}")
    };

    Ok(Regex::new(&effective_pattern)?)
}

/// Return `true` if the regex pattern starts with inline flags.
fn starts_with_inline_flags(pattern: &str) -> bool {
    pattern.starts_with("(?")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_event(event_type: &str) -> GenericPlanEvent {
        let mut event = GenericPlanEvent::new("evt-1".to_string(), json!({"steps": []}));
        event.event_type = event_type.to_string();
        event
    }

    #[test]
    fn test_generic_matcher_accept_all_mode_accepts_plan_events() {
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
        let event = make_event("plan");

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_type_gate_rejects_result_event_in_accept_all_mode() {
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
        let event = make_event("result");

        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_type_gate_rejects_non_plan_event_types() {
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();

        assert!(!matcher.should_process(&make_event("")));
        assert!(!matcher.should_process(&make_event("unknown")));
        assert!(!matcher.should_process(&make_event("PLAN")));
    }

    #[test]
    fn test_generic_matcher_action_only_mode_matches_literal_pattern() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut event = make_event("plan");
        event.action = Some("deploy".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_action_only_mode_rejects_missing_action() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let event = make_event("plan");
        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_name_and_version_mode_matches_literal_patterns() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: Some("service-a".to_string()),
            version: Some("1.2.3".to_string()),
        })
        .unwrap();

        let mut event = make_event("plan");
        event.name = Some("service-a".to_string());
        event.version = Some("1.2.3".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_name_and_action_mode_matches_literal_patterns() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: Some("service-a".to_string()),
            version: None,
        })
        .unwrap();

        let mut event = make_event("plan");
        event.name = Some("service-a".to_string());
        event.action = Some("deploy".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_name_version_action_mode_matches_literal_patterns() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: Some("service-a".to_string()),
            version: Some("1.2.3".to_string()),
        })
        .unwrap();

        let mut event = make_event("plan");
        event.name = Some("service-a".to_string());
        event.version = Some("1.2.3".to_string());
        event.action = Some("deploy".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_type_gate_rejects_result_event_regardless_of_match_mode() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: Some("service-a".to_string()),
            version: Some("1.2.3".to_string()),
        })
        .unwrap();

        let mut event = make_event("result");
        event.name = Some("service-a".to_string());
        event.version = Some("1.2.3".to_string());
        event.action = Some("deploy".to_string());

        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_regex_pattern_matches_expected_values() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy.*".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut deploy_prod = make_event("plan");
        deploy_prod.action = Some("deploy-prod".to_string());

        let mut deployment = make_event("plan");
        deployment.action = Some("deployment".to_string());

        let mut rollback = make_event("plan");
        rollback.action = Some("rollback".to_string());

        assert!(matcher.should_process(&deploy_prod));
        assert!(matcher.should_process(&deployment));
        assert!(!matcher.should_process(&rollback));
    }

    #[test]
    fn test_generic_matcher_case_insensitive_matching() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut mixed = make_event("plan");
        mixed.action = Some("Deploy".to_string());

        let mut upper = make_event("plan");
        upper.action = Some("DEPLOY".to_string());

        assert!(matcher.should_process(&mixed));
        assert!(matcher.should_process(&upper));
    }

    #[test]
    fn test_generic_matcher_invalid_regex_returns_error() {
        let result = GenericMatcher::new(GenericMatchConfig {
            action: Some("[".to_string()),
            name: None,
            version: None,
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_generic_matcher_missing_event_field_does_not_match_required_field() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut event = make_event("plan");
        event.name = Some("service-a".to_string());
        event.action = None;

        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_summary_includes_mode_and_patterns() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: Some("service-a".to_string()),
            version: None,
        })
        .unwrap();

        let summary = matcher.summary();

        assert!(summary.contains("mode=name+action"));
        assert!(summary.contains("action=/deploy/"));
        assert!(summary.contains("name=/service-a/"));
    }

    #[test]
    fn test_compile_pattern_preserves_inline_flags_without_prepending_default() {
        let regex = compile_pattern("(?i)deploy").unwrap();
        assert!(regex.is_match("DEPLOY"));
    }
}
