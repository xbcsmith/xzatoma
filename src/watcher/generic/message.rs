//! Compatibility shim for generic watcher message types.
//!
//! This module previously contained both [`GenericPlanEvent`] and
//! [`GenericPlanResult`] in a single file. Those types now live in focused
//! single-responsibility modules:
//!
//! - [`crate::watcher::generic::event`] — inbound plan event type
//! - [`crate::watcher::generic::result_event`] — outbound plan result type
//!
//! This shim re-exports both types so that existing call sites continue to
//! compile without modification. It will be removed in a future phase once all
//! call sites have been updated to import directly from the canonical modules.
//!
//! # Migration
//!
//! Update existing imports from:
//!
//! ```text
//! use xzatoma::watcher::generic::message::GenericPlanEvent;
//! use xzatoma::watcher::generic::message::GenericPlanResult;
//! ```
//!
//! to:
//!
//! ```text
//! use xzatoma::watcher::generic::event::GenericPlanEvent;
//! use xzatoma::watcher::generic::result_event::GenericPlanResult;
//! ```

pub use super::event::GenericPlanEvent;
pub use super::result_event::GenericPlanResult;
