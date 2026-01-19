//! XZepr Integration Module
//!
//! This module provides integration with XZepr for consuming CloudEvents
//! messages from Kafka and posting events back to the XZepr API.
//!
//! # Overview
//!
//! XZepr is an event-driven platform that publishes CloudEvents 1.0.1
//! compatible messages to Kafka topics. This module enables downstream
//! services to:
//!
//! 1. **Consume Events**: Read CloudEvents from XZepr Kafka topics
//! 2. **Process Work**: Handle events and perform business logic
//! 3. **Report Status**: Post work lifecycle events back to XZepr
//!
//! # Submodules
//!
//! - [`consumer`]: Kafka consumer and XZepr API client for downstream services
//!
//! # Example Usage
//!
//! See the [`consumer`] module for detailed examples and usage.

pub mod consumer;

pub use consumer::{
    ClientError, CloudEventData, CloudEventMessage, ConfigError, ConsumerError,
    CreateEventReceiverRequest, CreateEventRequest, EventEntity, EventReceiverEntity,
    EventReceiverGroupEntity, EventReceiverResponse, KafkaConsumerConfig, MessageHandler,
    PaginatedResponse, PaginationMeta, SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
    XzeprClient, XzeprClientConfig, XzeprConsumer,
};
