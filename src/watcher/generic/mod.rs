//! Generic Kafka watcher backend.
//!
//! This module contains the generic Kafka/Redpanda watcher implementation for
//! consuming generic plan events, matching them against configured criteria,
//! executing embedded plans, and publishing structured results.
//!
//! # Components
//!
//! - [`message`]: Generic watcher wire-format message types
//! - [`matcher`]: Regex-based event matching for generic plan events
//! - [`producer`]: Stub-first Kafka result producer for generic watcher results
//! - [`watcher`]: Core generic watcher service and dry-run processing flow
//!
//! # Loop prevention
//!
//! The generic watcher prevents same-topic re-trigger loops through the
//! `event_type` discriminator:
//!
//! - [`GenericPlanEvent`] messages must carry `event_type = "plan"`
//! - [`GenericPlanResult`] messages always carry `event_type = "result"`
//! - [`GenericMatcher`] rejects any event where `event_type != "plan"` before
//!   evaluating match criteria
//!
//! This guarantees that when the input topic and output topic are the same,
//! result messages are consumed and silently discarded instead of re-triggering
//! plan execution.
//!
//! # Matching model
//!
//! [`GenericMatcher`] is intentionally separate from the XZepr-specific
//! [`crate::watcher::xzepr::filter::EventFilter`] implementation. The generic
//! matcher operates only on [`GenericPlanEvent`] and
//! [`crate::config::GenericMatchConfig`] and supports the following modes:
//!
//! - `action` only
//! - `name` + `version`
//! - `name` + `action`
//! - `name` + `version` + `action`
//! - accept-all for all `"plan"` events when no match fields are configured
//!
//! # Examples
//!
//! ```
//! use xzatoma::config::GenericMatchConfig;
//! use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
//! use serde_json::json;
//!
//! let matcher = GenericMatcher::new(GenericMatchConfig {
//!     action: Some("deploy.*".to_string()),
//!     name: None,
//!     version: None,
//! })
//! .unwrap();
//!
//! let mut event = GenericPlanEvent::new("evt-1".to_string(), json!({"steps": []}));
//! event.action = Some("deploy-prod".to_string());
//!
//! assert!(matcher.should_process(&event));
//! ```

pub mod matcher;
pub mod message;
pub mod producer;
pub mod watcher;

pub use crate::config::GenericMatchConfig;
pub use matcher::GenericMatcher;
pub use message::{GenericPlanEvent, GenericPlanResult};
pub use producer::GenericResultProducer;
pub use watcher::GenericWatcher;
