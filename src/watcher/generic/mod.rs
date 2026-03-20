//! Generic Kafka watcher backend
//!
//! This module contains the generic Kafka/Redpanda watcher implementation
//! as defined in the generic watcher implementation plan.
//!
//! # Planned Functionality
//!
//! The generic watcher will:
//! - Consume plan-formatted JSON events from a configurable input topic
//! - Match events using flexible criteria via `GenericMatcher`
//!   (`action`, `name + version`, `name + action`, or `name + version + action`)
//! - Execute the embedded plan on match
//! - Publish structured results to a configurable output topic
//! - Process only events where `event_type == "plan"`, silently discarding
//!   all other event types (including `event_type == "result"`) to prevent
//!   same-topic input/output re-trigger loops
//!
//! # Message Types (Phase 2)
//!
//! Two message schemas are defined in [`message`]:
//!
//! - [`GenericPlanEvent`]: The trigger message a producer publishes to the input
//!   topic. The watcher deserializes each message, checks `event_type == "plan"`,
//!   evaluates the optional matching criteria, and executes the embedded plan on
//!   a successful match.
//!
//! - [`GenericPlanResult`]: The result message published to the output topic after
//!   plan execution. Its `event_type` is always `"result"`, which prevents
//!   re-trigger loops when the output topic and input topic are the same.
//!
//! # Matcher and Watcher (Phase 3)
//!
//! `GenericMatcher`, `GenericResultProducer`, and `GenericWatcher` will be added
//! in Phase 3. See `docs/explanation/generic_watcher_implementation_plan.md`.

pub mod message;

pub use message::{GenericPlanEvent, GenericPlanResult};
