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
//! - [`generic`]: Generic Kafka plan-event watcher (Phase 3, not yet implemented)
//!
//! The active backend is selected via the `watcher_type` configuration field
//! (added in Phase 4) or the corresponding CLI flag. Until Phase 4 is complete,
//! only the XZepr backend is available and [`XzeprWatcher`] is used directly.
//!
//! # Module Layout
//!
//! - [`generic`]: Placeholder for the Phase 3 generic Kafka watcher
//! - [`logging`]: Structured logging helpers shared across all watcher backends
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
pub mod xzepr;

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
