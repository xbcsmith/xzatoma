//! Kafka consumer for XZepr CloudEvents messages.
//!
//! This module provides a Kafka consumer that processes CloudEvents messages
//! from XZepr topics. It supports SASL/SCRAM authentication and provides
//! both handler-based and channel-based message processing.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use xzatoma::xzepr::consumer::{
//!     KafkaConsumerConfig, XzeprConsumer, MessageHandler, CloudEventMessage,
//! };
//!
//! struct MyHandler;
//!
//! #[async_trait::async_trait]
//! impl MessageHandler for MyHandler {
//!     async fn handle(
//!         &self,
//!         message: CloudEventMessage,
//!     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!         println!("Received event: {}", message.id);
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = KafkaConsumerConfig::from_env("my-service")?;
//!     let consumer = XzeprConsumer::new(config)?;
//!     consumer.run(Arc::new(MyHandler)).await?;
//!     Ok(())
//! }
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use super::config::KafkaConsumerConfig;
use super::message::CloudEventMessage;

/// Errors that can occur during consumer operations.
#[derive(Error, Debug)]
pub enum ConsumerError {
    /// Error from the Kafka client.
    #[error("Kafka error: {0}")]
    Kafka(String),

    /// Error deserializing message.
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] serde_json::Error),

    /// Consumer is not running.
    #[error("Consumer not running")]
    NotRunning,

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),
}

/// Handler trait for processing CloudEvents messages.
///
/// Implement this trait to define how messages should be processed.
/// The handler is called for each message received from Kafka.
///
/// # Example
///
/// ```rust
/// use xzatoma::xzepr::consumer::{MessageHandler, CloudEventMessage};
///
/// struct MyHandler;
///
/// #[async_trait::async_trait]
/// impl MessageHandler for MyHandler {
///     async fn handle(
///         &self,
///         message: CloudEventMessage,
///     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
///         println!("Processing: {}", message.event_type);
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Process a CloudEvents message.
    ///
    /// Return `Ok(())` to acknowledge the message was processed successfully.
    /// Return `Err` if processing failed (the consumer will continue with other messages).
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// XZepr Kafka consumer.
///
/// A consumer that reads CloudEvents messages from XZepr Kafka topics.
/// Supports SASL/SCRAM authentication and provides flexible message handling.
///
/// Note: This struct provides the interface but actual Kafka connectivity
/// requires the `rdkafka` feature to be enabled with the rdkafka crate.
/// The current implementation is a mock/stub that demonstrates the API
/// without requiring the rdkafka dependency.
pub struct XzeprConsumer {
    config: KafkaConsumerConfig,
    running: Arc<AtomicBool>,
}

impl XzeprConsumer {
    /// Creates a new consumer from configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Kafka consumer configuration
    ///
    /// # Errors
    ///
    /// Returns `ConsumerError::Kafka` if the consumer cannot be created.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use xzatoma::xzepr::consumer::{KafkaConsumerConfig, XzeprConsumer};
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "my-service");
    /// let consumer = XzeprConsumer::new(config).unwrap();
    /// ```
    pub fn new(config: KafkaConsumerConfig) -> Result<Self, ConsumerError> {
        info!(
            brokers = %config.brokers,
            topic = %config.topic,
            group_id = %config.group_id,
            security_protocol = %config.security_protocol.as_str(),
            "Creating XZepr consumer"
        );

        Ok(Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Returns the Kafka configuration as a key-value map.
    ///
    /// This can be used with the rdkafka crate's ClientConfig.
    pub fn get_kafka_config(&self) -> Vec<(String, String)> {
        let mut settings = vec![
            ("bootstrap.servers".to_string(), self.config.brokers.clone()),
            ("group.id".to_string(), self.config.group_id.clone()),
            (
                "auto.offset.reset".to_string(),
                self.config.auto_offset_reset.clone(),
            ),
            (
                "enable.auto.commit".to_string(),
                self.config.enable_auto_commit.to_string(),
            ),
            (
                "session.timeout.ms".to_string(),
                self.config.session_timeout.as_millis().to_string(),
            ),
            (
                "client.id".to_string(),
                format!("xzepr-consumer-{}", self.config.service_name),
            ),
            (
                "security.protocol".to_string(),
                self.config.security_protocol.as_str().to_string(),
            ),
        ];

        // Add SASL configuration
        if let Some(sasl) = &self.config.sasl_config {
            settings.push((
                "sasl.mechanism".to_string(),
                sasl.mechanism.as_str().to_string(),
            ));
            settings.push(("sasl.username".to_string(), sasl.username.clone()));
            settings.push(("sasl.password".to_string(), sasl.password.clone()));
        }

        // Add SSL configuration
        if let Some(ssl) = &self.config.ssl_config {
            if let Some(ca) = &ssl.ca_location {
                settings.push(("ssl.ca.location".to_string(), ca.clone()));
            }
            if let Some(cert) = &ssl.certificate_location {
                settings.push(("ssl.certificate.location".to_string(), cert.clone()));
            }
            if let Some(key) = &ssl.key_location {
                settings.push(("ssl.key.location".to_string(), key.clone()));
            }
        }

        settings
    }

    /// Returns the topic this consumer is configured to consume from.
    pub fn topic(&self) -> &str {
        &self.config.topic
    }

    /// Returns the service name.
    pub fn service_name(&self) -> &str {
        &self.config.service_name
    }

    /// Returns the consumer group ID.
    pub fn group_id(&self) -> &str {
        &self.config.group_id
    }

    /// Checks if the consumer is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stops the consumer.
    ///
    /// This sets the running flag to false, which will cause the run loop
    /// to exit on the next iteration.
    pub fn stop(&self) {
        info!(service = %self.config.service_name, "Stopping consumer");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Runs the consumer with the given message handler.
    ///
    /// This method blocks until the consumer is stopped or an error occurs.
    /// Messages are processed sequentially through the handler.
    ///
    /// # Arguments
    ///
    /// * `handler` - Handler for processing messages
    ///
    /// # Errors
    ///
    /// Returns `ConsumerError::Kafka` if there's a Kafka error.
    ///
    /// # Note
    ///
    /// This is a stub implementation. To use with real Kafka, enable the
    /// `rdkafka` feature and use the rdkafka crate.
    pub async fn run<H: MessageHandler + 'static>(
        &self,
        _handler: Arc<H>,
    ) -> Result<(), ConsumerError> {
        self.running.store(true, Ordering::SeqCst);

        info!(
            service = %self.config.service_name,
            topic = %self.config.topic,
            "Starting consumer (stub mode - no actual Kafka connection)"
        );

        // This is a stub that demonstrates the API.
        // In a real implementation, this would:
        // 1. Create rdkafka StreamConsumer
        // 2. Subscribe to the topic
        // 3. Stream messages and call handler.handle() for each

        while self.running.load(Ordering::SeqCst) {
            // Simulate waiting for messages
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            debug!(
                service = %self.config.service_name,
                "Waiting for messages... (stub mode)"
            );
        }

        info!(service = %self.config.service_name, "Consumer stopped");
        Ok(())
    }

    /// Runs the consumer and sends messages to a channel.
    ///
    /// This provides an alternative to the handler pattern, allowing messages
    /// to be processed in a separate task.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender for messages
    ///
    /// # Errors
    ///
    /// Returns `ConsumerError::Kafka` if there's a Kafka error.
    ///
    /// # Note
    ///
    /// This is a stub implementation.
    pub async fn run_with_channel(
        &self,
        _sender: mpsc::Sender<CloudEventMessage>,
    ) -> Result<(), ConsumerError> {
        self.running.store(true, Ordering::SeqCst);

        info!(
            service = %self.config.service_name,
            topic = %self.config.topic,
            "Starting consumer with channel (stub mode)"
        );

        while self.running.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        info!(service = %self.config.service_name, "Consumer stopped");
        Ok(())
    }

    /// Processes a single message payload.
    ///
    /// This is a utility method for parsing and handling a message payload.
    /// It can be used when integrating with rdkafka or other Kafka clients.
    ///
    /// # Arguments
    ///
    /// * `payload` - JSON payload as a string
    /// * `handler` - Handler to process the message
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the message was processed successfully.
    pub async fn process_message<H: MessageHandler>(
        payload: &str,
        handler: &H,
    ) -> Result<(), ConsumerError> {
        match serde_json::from_str::<CloudEventMessage>(payload) {
            Ok(event) => {
                debug!(
                    event_id = %event.id,
                    event_type = %event.event_type,
                    "Processing CloudEvent"
                );

                if let Err(e) = handler.handle(event).await {
                    error!("Error handling message: {}", e);
                    // Continue processing other messages
                }
                Ok(())
            }
            Err(e) => {
                error!("Error parsing CloudEvent: {}", e);
                debug!("Raw payload: {}", payload);
                Err(ConsumerError::Deserialization(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler {
        received: Arc<tokio::sync::Mutex<Vec<CloudEventMessage>>>,
    }

    impl TestHandler {
        fn new() -> Self {
            Self {
                received: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl MessageHandler for TestHandler {
        async fn handle(
            &self,
            message: CloudEventMessage,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.received.lock().await.push(message);
            Ok(())
        }
    }

    #[test]
    fn test_consumer_new() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service");
        let consumer = XzeprConsumer::new(config).unwrap();

        assert_eq!(consumer.topic(), "test-topic");
        assert_eq!(consumer.service_name(), "test-service");
        assert_eq!(consumer.group_id(), "xzepr-consumer-test-service");
        assert!(!consumer.is_running());
    }

    #[test]
    fn test_consumer_kafka_config() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service")
            .with_sasl_scram_sha256("user", "pass");
        let consumer = XzeprConsumer::new(config).unwrap();

        let kafka_config = consumer.get_kafka_config();
        let config_map: std::collections::HashMap<_, _> = kafka_config.into_iter().collect();

        assert_eq!(
            config_map.get("bootstrap.servers").unwrap(),
            "localhost:9092"
        );
        assert_eq!(
            config_map.get("group.id").unwrap(),
            "xzepr-consumer-test-service"
        );
        assert_eq!(config_map.get("security.protocol").unwrap(), "SASL_SSL");
        assert_eq!(config_map.get("sasl.mechanism").unwrap(), "SCRAM-SHA-256");
        assert_eq!(config_map.get("sasl.username").unwrap(), "user");
        assert_eq!(config_map.get("sasl.password").unwrap(), "pass");
    }

    #[test]
    fn test_consumer_stop() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service");
        let consumer = XzeprConsumer::new(config).unwrap();

        consumer.running.store(true, Ordering::SeqCst);
        assert!(consumer.is_running());

        consumer.stop();
        assert!(!consumer.is_running());
    }

    #[tokio::test]
    async fn test_process_message_valid() {
        let handler = TestHandler::new();

        let payload = r#"{
            "success": true,
            "id": "test-id",
            "specversion": "1.0.1",
            "type": "test.event",
            "source": "test-source",
            "api_version": "v1",
            "name": "test.event",
            "version": "1.0.0",
            "release": "1.0.0",
            "platform_id": "test",
            "package": "testpkg",
            "data": {
                "events": [],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }"#;

        let result = XzeprConsumer::process_message(payload, &handler).await;
        assert!(result.is_ok());

        let received = handler.received.lock().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].id, "test-id");
        assert_eq!(received[0].event_type, "test.event");
    }

    #[tokio::test]
    async fn test_process_message_invalid_json() {
        let handler = TestHandler::new();
        let result = XzeprConsumer::process_message("invalid json", &handler).await;
        assert!(matches!(result, Err(ConsumerError::Deserialization(_))));
    }

    #[tokio::test]
    async fn test_run_and_stop() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service");
        let consumer = Arc::new(XzeprConsumer::new(config).unwrap());
        let handler = Arc::new(TestHandler::new());

        let consumer_clone = consumer.clone();
        let handle = tokio::spawn(async move { consumer_clone.run(handler).await });

        // Give it time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(consumer.is_running());

        // Stop the consumer
        consumer.stop();

        // Wait for it to finish
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;

        assert!(result.is_ok());
    }

    #[test]
    fn test_consumer_with_ssl_config() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service")
            .with_ssl("/path/to/ca.pem");
        let consumer = XzeprConsumer::new(config).unwrap();

        let kafka_config = consumer.get_kafka_config();
        let config_map: std::collections::HashMap<_, _> = kafka_config.into_iter().collect();

        assert_eq!(
            config_map.get("ssl.ca.location").unwrap(),
            "/path/to/ca.pem"
        );
    }
}
