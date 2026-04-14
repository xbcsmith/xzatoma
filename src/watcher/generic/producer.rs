//! Generic watcher result producer.
//!
//! This module defines the generic watcher result publisher used by the generic
//! Kafka watcher backend.
//!
//! - Exposes the full producer interface backed by a real `rdkafka`
//!   `FutureProducer`.
//! - Assembles Kafka configuration in a testable way via `get_kafka_config()`.
//! - Publishes serialized [`GenericPlanResult`] messages to the configured
//!   output topic.
//! - Logs each published result at `info` level so behavior remains observable.
//!
//! The producer publishes [`GenericPlanResult`] messages to either an explicit
//! `output_topic` or, when that is not configured, falls back to the same topic
//! used for input.

use crate::config::KafkaWatcherConfig;
use crate::error::{Result, XzatomaError};
use crate::watcher::generic::result_event::GenericPlanResult;
use crate::watcher::xzepr::consumer::config::{
    SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::ClientConfig;
use std::fmt;
use std::time::Duration;
use tracing::info;

/// Kafka result producer for the generic watcher.
///
/// This type assembles Kafka producer configuration and publishes
/// [`GenericPlanResult`] messages to the configured output topic using an
/// `rdkafka` `FutureProducer`.
///
/// # Topic resolution
///
/// The effective output topic is resolved as follows:
///
/// 1. If `KafkaWatcherConfig::output_topic` is `Some`, use that value.
/// 2. Otherwise, fall back to `KafkaWatcherConfig::topic`.
///
/// # Examples
///
/// ```
/// use xzatoma::config::KafkaWatcherConfig;
/// use xzatoma::watcher::generic::GenericResultProducer;
///
/// let config = KafkaWatcherConfig {
///     brokers: "localhost:9092".to_string(),
///     topic: "plans.in".to_string(),
///     output_topic: Some("plans.out".to_string()),
///     group_id: "xzatoma-watcher".to_string(),
///     auto_create_topics: true,
///     num_partitions: 1,
///     replication_factor: 1,
///     security: None,
/// };
///
/// let producer = GenericResultProducer::new(&config).unwrap();
/// assert_eq!(producer.output_topic(), "plans.out");
/// ```
pub struct GenericResultProducer {
    /// Kafka bootstrap servers.
    brokers: String,
    /// Topic the watcher consumes from.
    input_topic: String,
    /// Topic results are published to.
    output_topic: String,
    /// Client identifier sent to the Kafka broker.
    client_id: String,
    /// Kafka security protocol.
    security_protocol: SecurityProtocol,
    /// Optional SASL authentication configuration.
    sasl_config: Option<SaslConfig>,
    /// Optional SSL/TLS configuration.
    ssl_config: Option<SslConfig>,
    /// Timeout for Kafka produce requests.
    request_timeout: Duration,
    /// The underlying rdkafka future producer.
    producer: FutureProducer,
}

impl fmt::Debug for GenericResultProducer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenericResultProducer")
            .field("brokers", &self.brokers)
            .field("input_topic", &self.input_topic)
            .field("output_topic", &self.output_topic)
            .field("client_id", &self.client_id)
            .field("security_protocol", &self.security_protocol)
            .field("sasl_config", &self.sasl_config)
            .field("ssl_config", &self.ssl_config)
            .field("request_timeout", &self.request_timeout)
            .field("producer", &"<FutureProducer>")
            .finish()
    }
}

impl GenericResultProducer {
    /// Construct a new generic result producer from watcher Kafka settings.
    ///
    /// Builds the internal `FutureProducer` from the assembled Kafka
    /// configuration key-value pairs. The producer is ready to publish
    /// immediately after construction.
    ///
    /// # Arguments
    ///
    /// * `config` - Watcher Kafka configuration shared with the consumer
    ///
    /// # Returns
    ///
    /// A configured `GenericResultProducer`.
    ///
    /// # Errors
    ///
    /// Returns an error if required Kafka security settings are invalid or if
    /// the underlying `FutureProducer` cannot be created.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::generic::GenericResultProducer;
    ///
    /// let config = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans".to_string(),
    ///     output_topic: None,
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let producer = GenericResultProducer::new(&config).unwrap();
    /// assert_eq!(producer.output_topic(), "plans");
    /// ```
    pub fn new(config: &KafkaWatcherConfig) -> Result<Self> {
        let output_topic = config
            .output_topic
            .clone()
            .unwrap_or_else(|| config.topic.clone());

        let client_id = "xzatoma-generic-result-producer".to_string();
        let request_timeout = Duration::from_secs(30);
        let mut security_protocol = SecurityProtocol::Plaintext;
        let mut sasl_config = None;
        let mut ssl_config = None;

        if let Some(security) = &config.security {
            security_protocol = parse_security_protocol(&security.protocol)?;

            if matches!(
                security_protocol,
                SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
            ) {
                ssl_config = Some(SslConfig {
                    ca_location: None,
                    certificate_location: None,
                    key_location: None,
                });
            }

            if let Some(mechanism) = &security.sasl_mechanism {
                let username = security.sasl_username.clone().ok_or_else(|| {
                    XzatomaError::Watcher(
                        "SASL username is required when mechanism is set".to_string(),
                    )
                })?;

                let password = security
                    .sasl_password
                    .clone()
                    .or_else(|| std::env::var("KAFKA_SASL_PASSWORD").ok())
                    .ok_or_else(|| {
                        XzatomaError::Watcher(
                            "SASL password required (set via config or KAFKA_SASL_PASSWORD env var)"
                                .to_string(),
                        )
                    })?;

                sasl_config = Some(SaslConfig {
                    mechanism: parse_sasl_mechanism(mechanism)?,
                    username,
                    password,
                });
            }
        }

        // Build the FutureProducer from the assembled configuration.
        let producer: FutureProducer = {
            let mut client_config = ClientConfig::new();
            client_config.set("bootstrap.servers", &config.brokers);
            client_config.set("client.id", &client_id);
            client_config.set("security.protocol", security_protocol.as_str());
            client_config.set(
                "message.timeout.ms",
                request_timeout.as_millis().to_string(),
            );

            if let Some(sasl) = &sasl_config {
                client_config.set("sasl.mechanism", sasl.mechanism.as_str());
                client_config.set("sasl.username", &sasl.username);
                client_config.set("sasl.password", &sasl.password);
            }

            if let Some(ssl) = &ssl_config {
                if let Some(ca) = &ssl.ca_location {
                    client_config.set("ssl.ca.location", ca);
                }
                if let Some(cert) = &ssl.certificate_location {
                    client_config.set("ssl.certificate.location", cert);
                }
                if let Some(key) = &ssl.key_location {
                    client_config.set("ssl.key.location", key);
                }
            }

            client_config.create().map_err(|e| {
                XzatomaError::Watcher(format!("Failed to create Kafka producer: {e}"))
            })?
        };

        Ok(Self {
            brokers: config.brokers.clone(),
            input_topic: config.topic.clone(),
            output_topic,
            client_id,
            security_protocol,
            sasl_config,
            ssl_config,
            request_timeout,
            producer,
        })
    }

    /// Return the effective output topic.
    ///
    /// # Returns
    ///
    /// The topic results will be published to.
    pub fn output_topic(&self) -> &str {
        &self.output_topic
    }

    /// Return the input topic associated with this producer configuration.
    ///
    /// # Returns
    ///
    /// The topic the watcher consumes from.
    pub fn input_topic(&self) -> &str {
        &self.input_topic
    }

    /// Return the Kafka configuration as key-value settings.
    ///
    /// This method assembles the full set of Kafka client configuration
    /// key-value pairs from the producer's fields, suitable for passing to
    /// `rdkafka::ClientConfig`.
    ///
    /// # Returns
    ///
    /// A vector of Kafka configuration key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::generic::GenericResultProducer;
    ///
    /// let config = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans".to_string(),
    ///     output_topic: None,
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let producer = GenericResultProducer::new(&config).unwrap();
    /// let settings: HashMap<_, _> = producer.get_kafka_config().into_iter().collect();
    ///
    /// assert_eq!(settings.get("bootstrap.servers").unwrap(), "localhost:9092");
    /// assert_eq!(
    ///     settings.get("client.id").unwrap(),
    ///     "xzatoma-generic-result-producer"
    /// );
    /// ```
    pub fn get_kafka_config(&self) -> Vec<(String, String)> {
        let mut settings = vec![
            ("bootstrap.servers".to_string(), self.brokers.clone()),
            ("client.id".to_string(), self.client_id.clone()),
            (
                "security.protocol".to_string(),
                self.security_protocol.as_str().to_string(),
            ),
            (
                "message.timeout.ms".to_string(),
                self.request_timeout.as_millis().to_string(),
            ),
        ];

        if let Some(sasl) = &self.sasl_config {
            settings.push((
                "sasl.mechanism".to_string(),
                sasl.mechanism.as_str().to_string(),
            ));
            settings.push(("sasl.username".to_string(), sasl.username.clone()));
            settings.push(("sasl.password".to_string(), sasl.password.clone()));
        }

        if let Some(ssl) = &self.ssl_config {
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

    /// Publish a generic watcher result to the configured output topic.
    ///
    /// Serializes the result to JSON and sends it to the Kafka output topic
    /// using the underlying `FutureProducer`. The `trigger_event_id` is used
    /// as the Kafka message key for deterministic partitioning.
    ///
    /// Each successful publish is logged at `info` level so published results
    /// remain observable.
    ///
    /// # Arguments
    ///
    /// * `result` - The result event to publish
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Watcher` if JSON serialization fails or if the
    /// Kafka produce request fails.
    pub async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
        let payload = serde_json::to_string(result).map_err(|error| {
            XzatomaError::Watcher(format!(
                "Failed to serialize generic watcher result payload: {}",
                error
            ))
        })?;

        let record = FutureRecord::to(&self.output_topic)
            .payload(&payload)
            .key(&result.trigger_event_id);

        self.producer
            .send(record, self.request_timeout)
            .await
            .map_err(|(kafka_error, _owned_message)| {
                XzatomaError::Watcher(format!(
                    "Failed to publish result to topic '{}': {}",
                    self.output_topic, kafka_error
                ))
            })?;

        info!(
            topic = %self.output_topic,
            trigger_event_id = %result.trigger_event_id,
            event_type = %result.event_type,
            success = result.success,
            payload = %payload,
            "Published generic watcher result"
        );

        Ok(())
    }
}

/// Parse a security protocol string into the corresponding enum variant.
fn parse_security_protocol(protocol: &str) -> Result<SecurityProtocol> {
    match protocol.to_uppercase().as_str() {
        "PLAINTEXT" => Ok(SecurityProtocol::Plaintext),
        "SSL" => Ok(SecurityProtocol::Ssl),
        "SASL_PLAINTEXT" => Ok(SecurityProtocol::SaslPlaintext),
        "SASL_SSL" => Ok(SecurityProtocol::SaslSsl),
        _ => Err(XzatomaError::Watcher(format!(
            "Invalid security protocol: {}",
            protocol
        ))),
    }
}

/// Parse a SASL mechanism string into the corresponding enum variant.
fn parse_sasl_mechanism(mechanism: &str) -> Result<SaslMechanism> {
    match mechanism.to_uppercase().as_str() {
        "PLAIN" => Ok(SaslMechanism::Plain),
        "SCRAM-SHA-256" => Ok(SaslMechanism::ScramSha256),
        "SCRAM-SHA-512" => Ok(SaslMechanism::ScramSha512),
        _ => Err(XzatomaError::Watcher(format!(
            "Invalid SASL mechanism: {}",
            mechanism
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KafkaSecurityConfig;
    use std::collections::HashMap;

    fn base_kafka_config() -> KafkaWatcherConfig {
        KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "xzatoma-watcher".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        }
    }

    #[test]
    fn test_generic_result_producer_uses_input_topic_when_output_topic_not_set() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();

        assert_eq!(producer.input_topic(), "plans.input");
        assert_eq!(producer.output_topic(), "plans.input");
    }

    #[test]
    fn test_generic_result_producer_uses_explicit_output_topic_when_configured() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.output".to_string());

        let producer = GenericResultProducer::new(&config).unwrap();

        assert_eq!(producer.input_topic(), "plans.input");
        assert_eq!(producer.output_topic(), "plans.output");
    }

    #[test]
    fn test_generic_result_producer_get_kafka_config_includes_basic_settings() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();

        let settings: HashMap<_, _> = producer.get_kafka_config().into_iter().collect();

        assert_eq!(settings.get("bootstrap.servers").unwrap(), "localhost:9092");
        assert_eq!(
            settings.get("client.id").unwrap(),
            "xzatoma-generic-result-producer"
        );
        assert_eq!(settings.get("security.protocol").unwrap(), "PLAINTEXT");
        assert_eq!(settings.get("message.timeout.ms").unwrap(), "30000");
    }

    #[test]
    fn test_generic_result_producer_get_kafka_config_includes_sasl_settings() {
        let mut config = base_kafka_config();
        config.security = Some(KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
            sasl_username: Some("user1".to_string()),
            sasl_password: Some("pass1".to_string()),
        });

        let producer = GenericResultProducer::new(&config).unwrap();
        let settings: HashMap<_, _> = producer.get_kafka_config().into_iter().collect();

        assert_eq!(settings.get("security.protocol").unwrap(), "SASL_SSL");
        assert_eq!(settings.get("sasl.mechanism").unwrap(), "SCRAM-SHA-256");
        assert_eq!(settings.get("sasl.username").unwrap(), "user1");
        assert_eq!(settings.get("sasl.password").unwrap(), "pass1");
    }

    #[test]
    fn test_generic_result_producer_new_returns_error_for_invalid_protocol() {
        let mut config = base_kafka_config();
        config.security = Some(KafkaSecurityConfig {
            protocol: "INVALID".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        });

        let result = GenericResultProducer::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_generic_result_producer_new_returns_error_for_missing_sasl_username() {
        let mut config = base_kafka_config();
        config.security = Some(KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: None,
            sasl_password: Some("secret".to_string()),
        });

        let result = GenericResultProducer::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_generic_result_producer_new_returns_error_for_missing_sasl_password() {
        let mut config = base_kafka_config();
        config.security = Some(KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: Some("alice".to_string()),
            sasl_password: None,
        });

        let result = GenericResultProducer::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_generic_result_producer_debug_impl() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();

        let debug_output = format!("{:?}", producer);
        assert!(debug_output.contains("GenericResultProducer"));
        assert!(debug_output.contains("localhost:9092"));
        assert!(debug_output.contains("plans.input"));
        assert!(debug_output.contains("<FutureProducer>"));
    }

    #[ignore] // Requires a running Kafka broker
    #[tokio::test]
    async fn test_generic_result_producer_publish_succeeds() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();
        let result = GenericPlanResult::new(
            "trigger-123".to_string(),
            true,
            "synthetic execution completed".to_string(),
        );

        let publish_result = producer.publish(&result).await;
        assert!(publish_result.is_ok());
    }
}
