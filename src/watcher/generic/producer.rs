//! Generic watcher result producer.
//!
//! This module defines the generic watcher result publisher abstraction used by
//! the generic Kafka watcher backend. The implementation intentionally follows
//! the same stub-first approach used by the XZepr consumer:
//!
//! - expose the full producer interface now,
//! - assemble Kafka configuration in a testable way,
//! - avoid requiring `rdkafka` at compile time,
//! - log serialized output so dry-run mode and tests have observable behavior.
//!
//! The producer publishes [`GenericPlanResult`] messages to either an explicit
//! `output_topic` or, when that is not configured, falls back to the same topic
//! used for input.

use crate::config::{KafkaSecurityConfig, KafkaWatcherConfig};
use crate::error::{Result, XzatomaError};
use crate::watcher::generic::message::GenericPlanResult;
use crate::watcher::xzepr::consumer::config::{
    SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
};
use std::time::Duration;
use tracing::info;

/// Stub Kafka result producer for the generic watcher.
///
/// This type assembles Kafka producer configuration and exposes a publish API
/// without requiring a concrete Kafka client dependency at compile time.
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
///     security: None,
/// };
///
/// let producer = GenericResultProducer::new(&config).unwrap();
/// assert_eq!(producer.output_topic(), "plans.out");
/// ```
#[derive(Debug, Clone)]
pub struct GenericResultProducer {
    brokers: String,
    input_topic: String,
    output_topic: String,
    client_id: String,
    security_protocol: SecurityProtocol,
    sasl_config: Option<SaslConfig>,
    ssl_config: Option<SslConfig>,
    request_timeout: Duration,
}

impl GenericResultProducer {
    /// Construct a new generic result producer from watcher Kafka settings.
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
    /// Returns an error if required Kafka security settings are invalid.
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

        let mut producer = Self {
            brokers: config.brokers.clone(),
            input_topic: config.topic.clone(),
            output_topic,
            client_id: "xzatoma-generic-result-producer".to_string(),
            security_protocol: SecurityProtocol::Plaintext,
            sasl_config: None,
            ssl_config: None,
            request_timeout: Duration::from_secs(30),
        };

        if let Some(security) = &config.security {
            producer.apply_security_config(security)?;
        }

        Ok(producer)
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
    /// This mirrors the stub-first style used by the XZepr consumer so the
    /// interface is testable before a concrete Kafka producer is wired in.
    ///
    /// # Returns
    ///
    /// A vector of Kafka configuration key-value pairs suitable for a future
    /// client implementation.
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

    /// Publish a generic watcher result.
    ///
    /// This is currently a stub implementation. It serializes the result to JSON
    /// and logs it at `info` level so behavior is visible and testable without a
    /// concrete Kafka producer dependency.
    ///
    /// # Arguments
    ///
    /// * `result` - The result event to publish
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::generic::{GenericPlanResult, GenericResultProducer};
    ///
    /// # tokio_test::block_on(async {
    /// let config = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans".to_string(),
    ///     output_topic: Some("results".to_string()),
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     security: None,
    /// };
    ///
    /// let producer = GenericResultProducer::new(&config).unwrap();
    /// let result = GenericPlanResult::new(
    ///     "trigger-1".to_string(),
    ///     true,
    ///     "dry-run completed".to_string(),
    /// );
    ///
    /// producer.publish(&result).await.unwrap();
    /// # });
    /// ```
    pub async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
        let payload = serde_json::to_string(result).map_err(|error| {
            XzatomaError::Watcher(format!(
                "Failed to serialize generic watcher result payload: {}",
                error
            ))
        })?;

        info!(
            topic = %self.output_topic,
            trigger_event_id = %result.trigger_event_id,
            event_type = %result.event_type,
            success = result.success,
            payload = %payload,
            "Publishing generic watcher result (stub mode)"
        );

        Ok(())
    }

    fn apply_security_config(&mut self, security: &KafkaSecurityConfig) -> Result<()> {
        self.security_protocol = parse_security_protocol(&security.protocol)?;

        if matches!(
            self.security_protocol,
            SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
        ) {
            self.ssl_config = Some(SslConfig {
                ca_location: None,
                certificate_location: None,
                key_location: None,
            });
        }

        if let Some(mechanism) = &security.sasl_mechanism {
            let username = security.sasl_username.clone().ok_or_else(|| {
                XzatomaError::Watcher("SASL username is required when mechanism is set".to_string())
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

            self.sasl_config = Some(SaslConfig {
                mechanism: parse_sasl_mechanism(mechanism)?,
                username,
                password,
            });
        }

        Ok(())
    }
}

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
    use std::collections::HashMap;

    fn base_kafka_config() -> KafkaWatcherConfig {
        KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "xzatoma-watcher".to_string(),
            auto_create_topics: true,
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

    #[tokio::test]
    async fn test_generic_result_producer_publish_stub_succeeds() {
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
