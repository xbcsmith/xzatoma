//! Watcher module for monitoring Kafka topics and executing plans
//!
//! This module provides functionality to watch Kafka topics for CloudEvents
//! messages, filter events based on configured criteria, extract plans from
//! event payloads, and execute those plans.
//!
//! # Overview
//!
//! The watcher is an autonomous component that:
//! 1. Connects to a Kafka topic
//! 2. Consumes CloudEvents messages
//! 3. Filters events based on configured criteria
//! 4. Extracts execution plans from event payloads
//! 5. Executes extracted plans using the agent
//!
//! # Modules
//!
//! - [`filter`]: Event filtering by type, source, platform, etc.
//! - [`logging`]: Structured logging configuration
//! - [`plan_extractor`]: Plan extraction from event payloads

pub mod filter;
pub mod logging;
pub mod plan_extractor;

pub use filter::EventFilter;
pub use plan_extractor::{PlanExtractionStrategy, PlanExtractor};
