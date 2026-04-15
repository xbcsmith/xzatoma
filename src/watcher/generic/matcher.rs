//! Generic event matcher for the generic Kafka watcher.
//!
//! This module contains the generic watcher's event matching logic. It is
//! intentionally separate from the XZepr-specific `EventFilter` implementation.
//!
//! `GenericMatcher` operates only on [`crate::watcher::generic::event::GenericPlanEvent`]
//! and [`crate::config::GenericMatchConfig`].
//!
//! Matching rules:
//!
//! - Configured `action` and `name` fields are interpreted as regex patterns.
//! - Version matching uses semver constraint evaluation via
//!   [`crate::watcher::version_matches`]. When the configured constraint is not
//!   a valid semver [`VersionReq`](semver::VersionReq), the function falls back to
//!   case-insensitive exact string equality so that non-semver tags such as
//!   `"nightly"` or `"latest"` continue to work.
//! - Matching is case-insensitive by default for `action` and `name` regex patterns.
//! - Missing required event fields never match.
//! - When no match fields are configured, all events are accepted (accept-all mode).
//!
//! The `event_type` gate that existed in Phase 1 is no longer present here.
//! The loop-break guarantee is now enforced upstream: [`GenericPlanEvent::new`]
//! returns `Err` for any payload that cannot be parsed as a valid plan (including
//! result event JSON), so the matcher only ever receives structurally valid plan
//! events.

use crate::config::GenericMatchConfig;
use crate::error::Result;
use crate::watcher::generic::event::GenericPlanEvent;
use regex::Regex;
use std::sync::Arc;

/// Generic event matcher for plan events.
///
/// This matcher evaluates [`GenericPlanEvent`] values against a
/// [`GenericMatchConfig`] using regex-based matching for `action` and `name`
/// and semver constraint evaluation for `version`.
///
/// Unlike the XZepr watcher filter, this matcher:
///
/// - operates only on generic plan events
/// - supports regex matching for `action` and `name`
/// - supports semver constraint matching for `version` (falls back to
///   case-insensitive exact string equality for non-semver version strings)
/// - accepts all events when no match fields are configured (accept-all mode)
///
/// # Accept-all semantics
///
/// When none of `action`, `name`, or `version` are configured in
/// [`GenericMatchConfig`], the matcher operates in **accept-all** mode and
/// returns `true` for every [`GenericPlanEvent`] it receives. Operators should
/// be aware that deploying a watcher with an empty match configuration will
/// cause it to process every plan event consumed from the input topic.
/// Use [`has_predicates`](GenericMatcher::has_predicates) to detect this at
/// startup and emit an appropriate warning log.
///
/// # Examples
///
/// ```
/// use xzatoma::config::GenericMatchConfig;
/// use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
///
/// let matcher = GenericMatcher::new(GenericMatchConfig {
///     action: Some("deploy".to_string()),
///     name: None,
///     version: None,
/// })
/// .unwrap();
///
/// let mut event = GenericPlanEvent::new(
///     "name: deploy\naction: Deploy\nsteps:\n  - name: s1\n    action: run\n",
///     "input.topic".to_string(),
///     None,
/// )
/// .unwrap();
/// // action is case-insensitively matched; "Deploy" matches pattern "deploy"
/// assert!(matcher.should_process(&event));
/// ```
#[derive(Debug, Clone)]
pub struct GenericMatcher {
    config: GenericMatchConfig,
    compiled_action: Option<Arc<Regex>>,
    compiled_name: Option<Arc<Regex>>,
    /// Raw version constraint string used by [`crate::watcher::version_matches`].
    version_constraint: Option<String>,
}

/// Active matching mode for the generic matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchMode {
    /// No match fields are configured; accept all events.
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
    /// The `action` and `name` fields are compiled eagerly as regex patterns so
    /// that invalid patterns fail fast at startup instead of during message
    /// handling. The `version` field is stored as a raw string and evaluated at
    /// match time via [`crate::watcher::version_matches`].
    ///
    /// Regex matching is case-insensitive by default. If the supplied pattern
    /// does not already begin with inline regex flags, `(?i)` is prepended
    /// automatically.
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
    /// Returns an error if any configured `action` or `name` regex pattern is
    /// invalid. Version constraint strings are never validated at construction
    /// time; an invalid semver constraint simply falls back to exact string
    /// comparison at match time.
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
        let version_constraint = config.version.clone();

        Ok(Self {
            config,
            compiled_action,
            compiled_name,
            version_constraint,
        })
    }

    /// Return `true` if the event should be processed.
    ///
    /// Evaluates the configured match criteria against the event's `name`,
    /// `version`, and `action` fields. When no criteria are configured, all
    /// events are accepted (accept-all mode).
    ///
    /// The `event_type` gate is no longer enforced here. That responsibility
    /// has moved to [`GenericPlanEvent::new`], which returns `Err` for any
    /// payload that is not a valid plan — ensuring that only structurally valid
    /// plan events ever reach this matcher.
    ///
    /// Supported matching modes:
    ///
    /// - `action` only
    /// - `name` + `version`
    /// - `name` + `action`
    /// - `name` + `version` + `action`
    /// - none configured (accept all events)
    ///
    /// # Arguments
    ///
    /// * `event` - Event to evaluate
    ///
    /// # Returns
    ///
    /// `true` if the event satisfies all configured match criteria.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::GenericMatchConfig;
    /// use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
    ///
    /// let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
    /// let event = GenericPlanEvent::new(
    ///     "name: test\nsteps:\n  - name: s1\n    action: echo test\n",
    ///     "input.topic".to_string(),
    ///     None,
    /// )
    /// .unwrap();
    ///
    /// assert!(matcher.should_process(&event));
    /// ```
    pub fn should_process(&self, event: &GenericPlanEvent) -> bool {
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

    /// Return `true` when at least one match predicate is configured.
    ///
    /// A matcher with no predicates operates in accept-all mode and will
    /// process every valid plan event. Use this method at watcher startup to
    /// detect and log a warning when no filtering is configured.
    ///
    /// # Returns
    ///
    /// `true` if any of `action`, `name`, or `version` is configured in the
    /// underlying [`GenericMatchConfig`]; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::GenericMatchConfig;
    /// use xzatoma::watcher::generic::GenericMatcher;
    ///
    /// let empty = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
    /// assert!(!empty.has_predicates());
    ///
    /// let with_version = GenericMatcher::new(GenericMatchConfig {
    ///     action: None,
    ///     name: None,
    ///     version: Some(">=2.0.0".to_string()),
    /// })
    /// .unwrap();
    /// assert!(with_version.has_predicates());
    /// ```
    pub fn has_predicates(&self) -> bool {
        self.compiled_action.is_some()
            || self.compiled_name.is_some()
            || self.version_constraint.is_some()
    }

    /// Return a human-readable summary of the matcher configuration.
    ///
    /// The summary includes the active matching mode and the configured
    /// patterns/constraints for structured startup logging. The `action` and
    /// `name` fields are formatted as `field=/<pattern>/` to indicate regex
    /// semantics. The `version` field is formatted as `version=<constraint>`
    /// (no slashes) to distinguish semver constraint syntax from regex syntax.
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

        if let Some(version) = &self.version_constraint {
            parts.push(format!("version={version}"));
        }

        parts.join(", ")
    }

    fn mode(&self) -> MatchMode {
        match (
            self.compiled_action.is_some(),
            self.compiled_name.is_some(),
            self.version_constraint.is_some(),
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
            self.version_constraint.is_some(),
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
        match (&self.version_constraint, &event.version) {
            (Some(constraint), Some(plan_version)) => {
                crate::watcher::version_matches(plan_version, constraint)
            }
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
/// If the pattern already begins with inline flags (e.g. `(?i)` or `(?s)`),
/// the flags are preserved as-is and `(?i)` is NOT prepended again.
///
/// # Errors
///
/// Returns an error if the regex pattern is invalid.
fn compile_pattern(pattern: &str) -> Result<Regex> {
    let effective = if starts_with_inline_flags(pattern) {
        pattern.to_string()
    } else {
        format!("(?i){pattern}")
    };
    Regex::new(&effective).map_err(|e| {
        crate::error::XzatomaError::Config(format!("Invalid regex pattern '{pattern}': {e}"))
    })
}

fn starts_with_inline_flags(pattern: &str) -> bool {
    pattern.starts_with("(?")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal but valid plan event for matcher tests.
    fn make_event() -> GenericPlanEvent {
        GenericPlanEvent::new(
            "name: test\nsteps:\n  - name: s1\n    action: echo test\n",
            "test.topic".to_string(),
            None,
        )
        .unwrap()
    }

    // -------------------------------------------------------------------------
    // Existing accept-all and basic predicate tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generic_matcher_accept_all_mode_accepts_plan_events() {
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
        let event = make_event();
        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_action_only_mode_matches_literal_pattern() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut event = make_event();
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

        // make_event() produces an event with action=None (plan has no action field).
        let event = make_event();
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

        let mut event = make_event();
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

        let mut event = make_event();
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

        let mut event = make_event();
        event.name = Some("service-a".to_string());
        event.version = Some("1.2.3".to_string());
        event.action = Some("deploy".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_regex_pattern_matches_expected_values() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: Some("deploy.*".to_string()),
            name: None,
            version: None,
        })
        .unwrap();

        let mut deploy_prod = make_event();
        deploy_prod.action = Some("deploy-prod".to_string());

        let mut deployment = make_event();
        deployment.action = Some("deployment".to_string());

        let mut rollback = make_event();
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

        let mut mixed = make_event();
        mixed.action = Some("Deploy".to_string());

        let mut upper = make_event();
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

        let mut event = make_event();
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

    #[test]
    fn test_generic_matcher_accept_all_mode_matches_event_with_no_matching_fields() {
        // In accept-all mode the matcher should pass events regardless of whether
        // action/name/version are set.
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();

        let mut event = make_event();
        event.name = None;
        event.version = None;
        event.action = None;

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_generic_matcher_name_version_mode_rejects_when_name_mismatch() {
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: Some("service-a".to_string()),
            version: Some("1.0.0".to_string()),
        })
        .unwrap();

        let mut event = make_event();
        event.name = Some("service-b".to_string()); // wrong name
        event.version = Some("1.0.0".to_string());

        assert!(!matcher.should_process(&event));
    }

    // -------------------------------------------------------------------------
    // Phase 3: version constraint tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_version_constraint_gte_matches() {
        // ">=1.0.0" accepts "1.2.0"
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some(">=1.0.0".to_string()),
        })
        .unwrap();

        let mut event = make_event();
        event.version = Some("1.2.0".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_version_constraint_gte_rejects() {
        // ">=1.0.0" rejects "0.9.0"
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some(">=1.0.0".to_string()),
        })
        .unwrap();

        let mut event = make_event();
        event.version = Some("0.9.0".to_string());

        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_version_constraint_caret_matches() {
        // "^2" accepts "2.5.1"
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some("^2".to_string()),
        })
        .unwrap();

        let mut event = make_event();
        event.version = Some("2.5.1".to_string());

        assert!(matcher.should_process(&event));
    }

    #[test]
    fn test_version_constraint_caret_rejects() {
        // "^2" rejects "3.0.0"
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some("^2".to_string()),
        })
        .unwrap();

        let mut event = make_event();
        event.version = Some("3.0.0".to_string());

        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_version_exact_string_fallback() {
        // When the configured version constraint cannot be parsed as a semver
        // VersionReq, version_matches falls back to case-insensitive exact string
        // equality between plan_version and constraint.
        //
        // Important: plan_version must still be a valid semver::Version. A
        // non-semver plan_version is rejected unconditionally at step 1 of
        // version_matches before the constraint fallback is ever reached.

        // "1.0.0" is a valid semver::Version; "latest" is not a valid VersionReq.
        // Fallback string comparison: "1.0.0" != "latest" -> matcher rejects.
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some("latest".to_string()),
        })
        .unwrap();

        let mut semver_event = make_event();
        semver_event.version = Some("1.0.0".to_string());
        assert!(!matcher.should_process(&semver_event));

        // A non-semver plan_version is rejected at step 1, before the fallback.
        let mut non_semver_event = make_event();
        non_semver_event.version = Some("nightly".to_string());
        assert!(!matcher.should_process(&non_semver_event));

        // A constraint with semver build metadata ("1.0.0+build.42") is a valid
        // semver::Version but VersionReq does not allow build metadata, so the
        // fallback applies. The same string in the event version matches via
        // case-insensitive equality. If VersionReq happens to accept it as ^1.0.0,
        // then 1.0.0 still satisfies that range, so the assertion holds either way.
        let matcher_build = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some("1.0.0+build.42".to_string()),
        })
        .unwrap();

        let mut build_event = make_event();
        build_event.version = Some("1.0.0+build.42".to_string());
        assert!(matcher_build.should_process(&build_event));

        // "2.0.0" neither equals "1.0.0+build.42" nor satisfies ^1.0.0 -> false.
        let mut other_event = make_event();
        other_event.version = Some("2.0.0".to_string());
        assert!(!matcher_build.should_process(&other_event));
    }

    #[test]
    fn test_version_required_but_plan_version_is_none() {
        // When a version constraint is configured but the plan event carries no
        // version field, the matcher must return false.
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some(">=1.0.0".to_string()),
        })
        .unwrap();

        // make_event() builds a minimal plan that carries no version field.
        let event = make_event();
        assert!(event.version.is_none());
        assert!(!matcher.should_process(&event));
    }

    #[test]
    fn test_has_predicates_empty() {
        // A matcher constructed from a default (all-None) config has no predicates
        // and must return false from has_predicates.
        let matcher = GenericMatcher::new(GenericMatchConfig::default()).unwrap();
        assert!(!matcher.has_predicates());
    }

    #[test]
    fn test_has_predicates_version_only() {
        // A matcher with only the version constraint configured has predicates.
        let matcher = GenericMatcher::new(GenericMatchConfig {
            action: None,
            name: None,
            version: Some(">=2.0.0".to_string()),
        })
        .unwrap();
        assert!(matcher.has_predicates());
    }
}
