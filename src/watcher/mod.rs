//! Watcher module for monitoring Kafka topics and executing plans
//!
//! This module provides the watcher service infrastructure that connects to
//! Kafka/Redpanda topics, receives events, and executes embedded plans.
//!
//! # Backends
//!
//! Two watcher backends are supported as equal configuration peers:
//!
//! - [`xzepr`]: XZepr CloudEvents-based watcher (default, full backward compatibility)
//! - [`generic`]: Generic Kafka plan-event watcher
//!
//! The active backend is selected via the `watcher_type` configuration field
//! or the corresponding CLI flag.
//!
//! # Module Layout
//!
//! - [`generic`]: Generic Kafka watcher backend
//! - [`logging`]: Structured logging helpers shared across all watcher backends
//! - [`topic_admin`]: Shared topic administration helpers for watcher startup
//! - [`xzepr`]: XZepr watcher backend (consumer, filter, plan extractor, watcher)
//!
//! # XZepr-Specific Types
//!
//! `EventFilter`, `PlanExtractor`, and other XZepr-specific types are intentionally
//! NOT re-exported at this level. They belong exclusively to the XZepr backend and
//! are accessible only via `crate::watcher::xzepr::*`. Hoisting them here would
//! falsely imply a shared interface with the generic backend.
//!
//! The one exception is [`XzeprWatcher`], re-exported here to provide the dispatch
//! call site used in `commands::watch::run_watch`.

pub mod generic;
pub mod logging;
pub mod topic_admin;
pub mod xzepr;

/// Evaluate whether a plan version satisfies a constraint string.
///
/// The function first attempts to parse both `plan_version` and `constraint`
/// as semver values. When both parse successfully the result of
/// `req.matches(&version)` is returned. When `constraint` cannot be parsed as
/// a [`semver::VersionReq`] the function falls back to a case-insensitive
/// exact string comparison so that non-semver constraint strings (e.g. `"latest"`,
/// `"nightly"`, or custom build tags) continue to work without forcing operators
/// to switch to full semver.
///
/// Note: `plan_version` must be a valid semver string. If it cannot be parsed
/// as a [`semver::Version`], `false` is returned unconditionally regardless of
/// the constraint.
///
/// # Arguments
///
/// * `plan_version` - The version string carried by the plan event.
/// * `constraint` - The operator-configured version predicate, e.g. `">=2.0.0"`,
///   `"^1"`, or a plain string such as `"1.0.0"`.
///
/// # Returns
///
/// `true` when the plan version satisfies the constraint; `false` when
/// `plan_version` is not a valid semver string or when the constraint is a
/// plain string and the values do not match case-insensitively.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::version_matches;
///
/// // Semver range constraints
/// assert!(version_matches("1.2.0", ">=1.0.0"));
/// assert!(!version_matches("0.9.0", ">=1.0.0"));
/// assert!(version_matches("2.5.1", "^2"));
/// assert!(!version_matches("3.0.0", "^2"));
///
/// // Exact semver
/// assert!(version_matches("1.0.0", "1.0.0"));
///
/// // Invalid plan version always returns false regardless of constraint
/// assert!(!version_matches("not-a-version", ">=1.0.0"));
/// assert!(!version_matches("nightly", "nightly"));
///
/// // Valid semver plan_version with non-semver constraint falls back to
/// // case-insensitive string comparison
/// assert!(!version_matches("1.0.0", "latest"));
/// ```
pub fn version_matches(plan_version: &str, constraint: &str) -> bool {
    let version = match semver::Version::parse(plan_version) {
        Ok(v) => v,
        Err(_) => return false,
    };

    match semver::VersionReq::parse(constraint) {
        Ok(req) => req.matches(&version),
        Err(_) => plan_version.eq_ignore_ascii_case(constraint),
    }
}

/// The XZepr watcher backend, re-exported for use in the watch command dispatcher.
///
/// This alias resolves to [`crate::watcher::xzepr::watcher::Watcher`]. When Phase 4
/// introduces `watcher_type` dispatch, the call site in `commands::watch::run_watch`
/// will select between `XzeprWatcher` and the generic watcher based on configuration.
///
/// # Examples
///
/// ```
/// use xzatoma::config::Config;
/// use xzatoma::watcher::XzeprWatcher;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let watcher = XzeprWatcher::new(config, false)?;
/// # Ok(())
/// # }
/// ```
pub use xzepr::watcher::Watcher as XzeprWatcher;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_matches_exact() {
        assert!(version_matches("1.0.0", "1.0.0"));
        assert!(version_matches("2.3.4", "2.3.4"));
    }

    #[test]
    fn test_version_matches_gte_range() {
        assert!(version_matches("1.2.0", ">=1.0.0"));
        assert!(version_matches("1.0.0", ">=1.0.0"));
        assert!(!version_matches("0.9.9", ">=1.0.0"));
    }

    #[test]
    fn test_version_matches_caret_range() {
        assert!(version_matches("2.5.1", "^2"));
        assert!(version_matches("2.0.0", "^2"));
        assert!(!version_matches("3.0.0", "^2"));
        assert!(!version_matches("1.9.9", "^2"));
    }

    #[test]
    fn test_version_matches_invalid_version_returns_false() {
        // A non-semver plan_version is rejected at step 1 regardless of constraint.
        assert!(!version_matches("not-a-version", ">=1.0.0"));
        assert!(!version_matches("", ">=1.0.0"));
        assert!(!version_matches("abc.def.ghi", "^1"));
    }

    #[test]
    fn test_version_matches_invalid_constraint_falls_back_to_string_equality() {
        // When the constraint is not a valid semver VersionReq, the function falls
        // back to case-insensitive exact string equality between plan_version and
        // constraint. However, plan_version must still be a valid semver::Version:
        // a non-semver plan_version is rejected unconditionally at step 1.

        // "1.0.0" is a valid Version; "latest" is not a valid VersionReq.
        // Fallback string comparison: "1.0.0" != "latest" -> false.
        assert!(!version_matches("1.0.0", "latest"));

        // Case-insensitive fallback: "1.0.0" != "LATEST" -> false.
        assert!(!version_matches("1.0.0", "LATEST"));

        // Non-semver plan versions are rejected at step 1, regardless of constraint.
        assert!(!version_matches("nightly", "nightly"));
        assert!(!version_matches("latest", "latest"));
        assert!(!version_matches("nightly", "Nightly"));
    }
}
