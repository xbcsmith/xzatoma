//! Generic Kafka watcher backend.
//!
//! This module contains the generic Kafka/Redpanda watcher implementation for
//! consuming generic plan events, matching them against configured criteria,
//! executing embedded plans, and publishing structured results.
//!
//! # Components
//!
//! - [`event`]: Inbound plan event type ([`GenericPlanEvent`]) and raw message
//!   bridge ([`RawKafkaMessage`])
//! - [`result_event`]: Outbound plan result type ([`GenericPlanResult`])
//! - [`event_handler`]: Five-step event pipeline ([`GenericEventHandler`]) and
//!   task type ([`GenericTask`])
//! - [`matcher`]: Regex-based event matching for generic plan events
//! - [`producer`]: Kafka result producer for generic watcher results
//! - [`watcher`]: Core generic watcher service and dry-run processing flow
//!
//! # Loop prevention
//!
//! The generic watcher prevents same-topic re-trigger loops through early plan
//! parsing:
//!
//! - [`GenericPlanEvent::new`] calls [`PlanParser::parse_string`] on the raw
//!   Kafka payload and returns `Err` if the payload cannot be parsed as a
//!   valid [`Plan`].
//! - [`GenericPlanResult`] messages published to the output topic carry JSON
//!   fields (`id`, `event_type`, `trigger_event_id`, etc.) that do not match
//!   the [`Plan`] schema, so they fail plan parsing when consumed back on the
//!   same topic.
//! - The [`GenericEventHandler`] propagates the parse error and the watcher
//!   classifies the message as `InvalidPayload` — no execution, no new result.
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
//! - accept-all for all valid plan events when no match fields are configured
//!
//! # Examples
//!
//! ```
//! use xzatoma::config::GenericMatchConfig;
//! use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
//!
//! let matcher = GenericMatcher::new(GenericMatchConfig {
//!     action: Some("deploy.*".to_string()),
//!     name: None,
//!     version: None,
//! })
//! .unwrap();
//!
//! let mut event = GenericPlanEvent::new(
//!     "name: deploy\naction: deploy-prod\nsteps:\n  - name: s1\n    action: kubectl apply\n",
//!     "input.topic".to_string(),
//!     None,
//! )
//! .unwrap();
//!
//! assert!(matcher.should_process(&event));
//! ```
//!
//! [`Plan`]: crate::tools::plan::Plan
//! [`PlanParser::parse_string`]: crate::tools::plan::PlanParser::parse_string

pub mod event;
pub mod event_handler;
pub mod matcher;
pub mod producer;
pub mod result_event;
pub mod watcher;

pub use crate::config::GenericMatchConfig;
pub use event::{GenericPlanEvent, RawKafkaMessage};
pub use event_handler::{GenericEventHandler, GenericTask};
pub use matcher::GenericMatcher;
pub use producer::GenericResultProducer;
pub use result_event::GenericPlanResult;
pub use watcher::GenericWatcher;
