//! XZepr Integration Module
//!
//! This module provides integration with XZepr for consuming CloudEvents
//! messages from Kafka and posting events back to the XZepr API.
//!
//! # Relocation Notice
//!
//! All XZepr implementation code has been relocated to `crate::watcher::xzepr`
//! as part of the generic watcher architecture (Phase 1). This module re-exports
//! everything from that canonical location to preserve full backward compatibility.
//!
//! XZepr is a fully supported, permanent watcher backend — an equal configuration
//! peer alongside the generic watcher introduced in Phase 3. No deprecation notices
//! apply.
//!
//! # Submodules
//!
//! - [`consumer`]: Kafka consumer and XZepr API client for downstream services
//!
//! # Example Usage
//!
//! See the [`consumer`] module for detailed examples and usage.

pub use crate::watcher::xzepr::consumer;

pub use crate::watcher::xzepr::consumer::{
    ClientError, CloudEventData, CloudEventMessage, ConfigError, ConsumerError,
    CreateEventReceiverRequest, CreateEventRequest, EventEntity, EventReceiverEntity,
    EventReceiverGroupEntity, EventReceiverResponse, KafkaConsumerConfig, MessageHandler,
    PaginatedResponse, PaginationMeta, SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
    XzeprClient, XzeprClientConfig, XzeprConsumer,
};
