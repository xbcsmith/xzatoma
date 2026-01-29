//! XZepr Consumer Module
//!
//! This module provides functionality for downstream services to:
//! - Consume CloudEvents messages from XZepr Kafka topics
//! - Parse and process event data
//! - Post work lifecycle events back to XZepr
//!
//! # Overview
//!
//! The consumer module enables downstream services to integrate with XZepr's
//! event-driven architecture. It provides:
//!
//! - **Kafka Consumer**: Connects to Kafka with SASL/SCRAM authentication
//! - **Message Types**: CloudEvents 1.0.1 compatible message structs
//! - **API Client**: HTTP client for XZepr API interaction
//! - **Work Lifecycle**: Helpers for posting work.started/completed/failed events
//!
//! # Example
//!
//! ```rust,no_run
//! use xzatoma::xzepr::consumer::{
//!     KafkaConsumerConfig, XzeprConsumer, XzeprClient, MessageHandler,
//!     CloudEventMessage,
//! };
//! use std::sync::Arc;
//!
//! struct MyHandler {
//!     client: XzeprClient,
//!     receiver_id: String,
//! }
//!
//! #[async_trait::async_trait]
//! impl MessageHandler for MyHandler {
//!     async fn handle(
//!         &self,
//!         message: CloudEventMessage,
//!     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         // Post work started
//!         self.client.post_work_started(
//!             &self.receiver_id,
//!             &message.id,
//!             "my-work",
//!             "1.0.0",
//!             "kubernetes",
//!             "my-service",
//!             serde_json::json!({}),
//!         ).await?;
//!
//!         // Do work...
//!
//!         // Post work completed
//!         self.client.post_work_completed(
//!             &self.receiver_id,
//!             &message.id,
//!             "my-work",
//!             "1.0.0",
//!             "kubernetes",
//!             "my-service",
//!             true,
//!             serde_json::json!({"result": "success"}),
//!         ).await?;
//!
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize consumer
//!     let config = KafkaConsumerConfig::from_env("my-service")?;
//!     let consumer = XzeprConsumer::new(config)?;
//!
//!     // Initialize XZepr client
//!     let client = XzeprClient::from_env()?;
//!
//!     // Register/discover event receiver
//!     let receiver_id = client.discover_or_create_event_receiver(
//!         "my-service-receiver",
//!         "worker",
//!         "1.0.0",
//!         "Event receiver for my-service",
//!         serde_json::json!({"type": "object"}),
//!     ).await?;
//!
//!     // Create handler
//!     let handler = Arc::new(MyHandler { client, receiver_id });
//!
//!     // Run consumer
//!     consumer.run(handler).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Consumer Group Naming
//!
//! By default, the consumer group ID is `xzepr-consumer-{service_name}`.
//! This can be overridden using `KafkaConsumerConfig::with_group_id()` or
//! by setting the `XZEPR_KAFKA_GROUP_ID` environment variable.
//!
//! # Authentication
//!
//! The consumer supports multiple authentication mechanisms:
//!
//! - **PLAINTEXT**: No authentication (development only)
//! - **SSL**: TLS encryption without SASL
//! - **SASL_PLAINTEXT**: SASL authentication without TLS
//! - **SASL_SSL**: SASL authentication with TLS (recommended for production)
//!
//! SASL mechanisms supported:
//! - PLAIN
//! - SCRAM-SHA-256 (recommended)
//! - SCRAM-SHA-512

pub mod client;
pub mod config;
pub mod kafka;
pub mod message;

#[allow(unused_imports)]
pub use client::{
    ClientError, CreateEventReceiverRequest, CreateEventRequest, EventReceiverResponse,
    PaginatedResponse, PaginationMeta, XzeprClient, XzeprClientConfig,
};
#[allow(unused_imports)]
pub use config::{
    ConfigError, KafkaConsumerConfig, SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
};
#[allow(unused_imports)]
pub use kafka::{ConsumerError, MessageHandler, XzeprConsumer};
#[allow(unused_imports)]
pub use message::{
    CloudEventData, CloudEventMessage, EventEntity, EventReceiverEntity, EventReceiverGroupEntity,
};
