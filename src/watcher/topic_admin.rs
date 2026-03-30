//! Kafka topic administration support for watcher startup.
//!
//! This module provides a watcher-oriented abstraction for ensuring that
//! watcher topics exist before a watcher backend enters its consume loop.
//!
//! It uses the `rdkafka` admin client to create topics on the configured
//! Kafka cluster. If a topic already exists the operation is treated as
//! success so that watcher startup is idempotent.
//!
//! - define the watcher-facing topic administration interface
//! - provide deterministic topic resolution logic
//! - expose Kafka config assembly for client integration
//! - log topic ensure/create actions so behavior is observable and testable
//!
//! Both watcher backends can use this module:
//!
//! - the XZepr watcher ensures its input topic exists
//! - the generic watcher ensures its input topic exists and also ensures the
//!   output topic exists when it differs from the input topic
//!
//! # Examples
//!
//! ```
//! use xzatoma::config::KafkaWatcherConfig;
//! use xzatoma::watcher::topic_admin::WatcherTopicAdmin;
//!
//! # tokio_test::block_on(async {
//! let kafka = KafkaWatcherConfig {
//!     brokers: "localhost:9092".to_string(),
//!     topic: "plans.input".to_string(),
//!     output_topic: Some("plans.output".to_string()),
//!     group_id: "xzatoma-watcher".to_string(),
//!     auto_create_topics: true,
//!     num_partitions: 1,
//!     replication_factor: 1,
//!     security: None,
//! };
//!
//! let admin = WatcherTopicAdmin::new(&kafka).unwrap();
//! let topics = admin.topics_for_generic_watcher();
//!
//! assert_eq!(
//!     topics,
//!     vec!["plans.input".to_string(), "plans.output".to_string()]
//! );
//! # });
//! ```

use crate::config::{KafkaSecurityConfig, KafkaWatcherConfig};
use crate::error::{Result, XzatomaError};
use crate::watcher::xzepr::consumer::config::{
    SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
};
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use rdkafka::types::RDKafkaErrorCode;
use rdkafka::ClientConfig;
use std::time::Duration;
use tracing::info;

/// Kafka topic administration helper for watcher startup.
///
/// This type provides a watcher-oriented interface for ensuring topics exist
/// before consumption or publishing begins. It builds an `rdkafka`
/// `AdminClient` from the assembled Kafka configuration and calls
/// `create_topics` for each requested topic.
///
/// If a topic already exists the creation is treated as success so that
/// watcher startup is idempotent.
///
/// # Examples
///
/// ```
/// use xzatoma::config::KafkaWatcherConfig;
/// use xzatoma::watcher::topic_admin::WatcherTopicAdmin;
///
/// let kafka = KafkaWatcherConfig {
///     brokers: "localhost:9092".to_string(),
///     topic: "xzepr.events".to_string(),
///     output_topic: None,
///     group_id: "xzatoma-watcher".to_string(),
///     auto_create_topics: true,
///     num_partitions: 1,
///     replication_factor: 1,
///     security: None,
/// };
///
/// let admin = WatcherTopicAdmin::new(&kafka).unwrap();
/// assert_eq!(admin.input_topic(), "xzepr.events");
/// ```
#[derive(Debug, Clone)]
pub struct WatcherTopicAdmin {
    brokers: String,
    input_topic: String,
    output_topic: Option<String>,
    client_id: String,
    security_protocol: SecurityProtocol,
    sasl_config: Option<SaslConfig>,
    ssl_config: Option<SslConfig>,
    request_timeout: Duration,
    num_partitions: i32,
    replication_factor: i32,
}

/// A topic ensure operation recorded by the admin flow.
///
/// Each `TopicEnsureRequest` describes a single topic that should exist on
/// the Kafka cluster together with a human-readable purpose string for
/// logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicEnsureRequest {
    /// Topic name that should exist.
    pub topic: String,
    /// Human-readable reason for ensuring the topic.
    pub purpose: String,
}

impl WatcherTopicAdmin {
    /// Construct a new topic admin helper from watcher Kafka settings.
    ///
    /// # Arguments
    ///
    /// * `config` - Watcher Kafka configuration
    ///
    /// # Returns
    ///
    /// A configured `WatcherTopicAdmin`.
    ///
    /// # Errors
    ///
    /// Returns an error if Kafka security settings are invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::topic_admin::WatcherTopicAdmin;
    ///
    /// let kafka = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans.events".to_string(),
    ///     output_topic: Some("plans.results".to_string()),
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let admin = WatcherTopicAdmin::new(&kafka).unwrap();
    /// assert_eq!(admin.input_topic(), "plans.events");
    /// assert_eq!(admin.output_topic(), Some("plans.results"));
    /// ```
    pub fn new(config: &KafkaWatcherConfig) -> Result<Self> {
        let mut admin = Self {
            brokers: config.brokers.clone(),
            input_topic: config.topic.clone(),
            output_topic: config.output_topic.clone(),
            client_id: "xzatoma-watcher-topic-admin".to_string(),
            security_protocol: SecurityProtocol::Plaintext,
            sasl_config: None,
            ssl_config: None,
            request_timeout: Duration::from_secs(30),
            num_partitions: config.num_partitions,
            replication_factor: config.replication_factor,
        };

        if let Some(security) = &config.security {
            admin.apply_security_config(security)?;
        }

        Ok(admin)
    }

    /// Return the configured input topic.
    ///
    /// # Returns
    ///
    /// The topic the watcher consumes from.
    pub fn input_topic(&self) -> &str {
        &self.input_topic
    }

    /// Return the configured output topic if one was provided.
    ///
    /// # Returns
    ///
    /// `Some(&str)` when an explicit output topic is configured, otherwise `None`.
    pub fn output_topic(&self) -> Option<&str> {
        self.output_topic.as_deref()
    }

    /// Return the Kafka client configuration as key-value settings.
    ///
    /// These settings are suitable for constructing an `rdkafka`
    /// `AdminClient` or any other Kafka client type.
    ///
    /// # Returns
    ///
    /// Kafka admin-client-style settings as key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::topic_admin::WatcherTopicAdmin;
    ///
    /// let kafka = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans.events".to_string(),
    ///     output_topic: None,
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let admin = WatcherTopicAdmin::new(&kafka).unwrap();
    /// let settings: HashMap<_, _> = admin.get_kafka_config().into_iter().collect();
    ///
    /// assert_eq!(settings.get("bootstrap.servers").unwrap(), "localhost:9092");
    /// assert_eq!(
    ///     settings.get("client.id").unwrap(),
    ///     "xzatoma-watcher-topic-admin"
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
                "socket.timeout.ms".to_string(),
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

    /// Return the topics that should exist for the XZepr watcher.
    ///
    /// The XZepr watcher consumes only its configured input topic.
    ///
    /// # Returns
    ///
    /// A one-element vector containing the input topic.
    pub fn topics_for_xzepr_watcher(&self) -> Vec<String> {
        vec![self.input_topic.clone()]
    }

    /// Return the topics that should exist for the generic watcher.
    ///
    /// The generic watcher always requires the input topic. If an explicit
    /// `output_topic` is configured and differs from the input topic, that topic
    /// is also included.
    ///
    /// # Returns
    ///
    /// A de-duplicated ordered list of topics required by the generic watcher.
    pub fn topics_for_generic_watcher(&self) -> Vec<String> {
        let mut topics = vec![self.input_topic.clone()];

        if let Some(output) = &self.output_topic {
            if output != &self.input_topic {
                topics.push(output.clone());
            }
        }

        topics
    }

    /// Build the ensure requests required for the XZepr watcher.
    ///
    /// # Returns
    ///
    /// A list of topic ensure requests for watcher startup.
    pub fn ensure_requests_for_xzepr_watcher(&self) -> Vec<TopicEnsureRequest> {
        vec![TopicEnsureRequest {
            topic: self.input_topic.clone(),
            purpose: "xzepr watcher input topic".to_string(),
        }]
    }

    /// Build the ensure requests required for the generic watcher.
    ///
    /// # Returns
    ///
    /// A list of topic ensure requests for watcher startup.
    pub fn ensure_requests_for_generic_watcher(&self) -> Vec<TopicEnsureRequest> {
        let mut requests = vec![TopicEnsureRequest {
            topic: self.input_topic.clone(),
            purpose: "generic watcher input topic".to_string(),
        }];

        if let Some(output) = &self.output_topic {
            if output != &self.input_topic {
                requests.push(TopicEnsureRequest {
                    topic: output.clone(),
                    purpose: "generic watcher output topic".to_string(),
                });
            }
        }

        requests
    }

    /// Ensure that XZepr watcher topics exist on the Kafka cluster.
    ///
    /// Creates the input topic via the Kafka admin client. If the topic
    /// already exists the operation is treated as success.
    ///
    /// # Errors
    ///
    /// Returns an error if a topic name is invalid or if topic creation
    /// fails for a reason other than the topic already existing.
    pub async fn ensure_xzepr_watcher_topics(&self) -> Result<()> {
        self.ensure_topics(&self.ensure_requests_for_xzepr_watcher())
            .await
    }

    /// Ensure that generic watcher topics exist on the Kafka cluster.
    ///
    /// Creates the input topic and, when configured, the output topic via
    /// the Kafka admin client. Topics that already exist are silently
    /// accepted.
    ///
    /// # Errors
    ///
    /// Returns an error if a topic name is invalid or if topic creation
    /// fails for a reason other than the topic already existing.
    pub async fn ensure_generic_watcher_topics(&self) -> Result<()> {
        self.ensure_topics(&self.ensure_requests_for_generic_watcher())
            .await
    }

    /// Ensure that the requested topics exist on the Kafka cluster.
    ///
    /// Builds an `AdminClient` from the assembled Kafka configuration,
    /// then issues a `create_topics` call for each request. The
    /// `TopicAlreadyExists` error code is treated as success so that
    /// repeated startup is idempotent.
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Watcher` if topic name validation fails,
    /// the admin client cannot be created, or a topic creation call
    /// returns a non-recoverable error.
    async fn ensure_topics(&self, requests: &[TopicEnsureRequest]) -> Result<()> {
        for request in requests {
            validate_topic_name(&request.topic)?;
        }

        let admin_client: AdminClient<DefaultClientContext> = {
            let mut client_config = ClientConfig::new();
            for (key, value) in self.get_kafka_config() {
                client_config.set(&key, &value);
            }
            client_config.create().map_err(|e| {
                XzatomaError::Watcher(format!("Failed to create Kafka admin client: {e}"))
            })?
        };

        let admin_options = AdminOptions::new();

        for request in requests {
            info!(
                brokers = %self.brokers,
                topic = %request.topic,
                purpose = %request.purpose,
                num_partitions = self.num_partitions,
                replication_factor = self.replication_factor,
                "Ensuring Kafka topic exists"
            );

            let new_topic = NewTopic::new(
                &request.topic,
                self.num_partitions,
                TopicReplication::Fixed(self.replication_factor),
            );

            let results = admin_client
                .create_topics(&[new_topic], &admin_options)
                .await
                .map_err(|e| {
                    XzatomaError::Watcher(format!(
                        "Failed to create topic '{}': {e}",
                        request.topic
                    ))
                })?;

            for result in results {
                match result {
                    Ok(_) => {
                        info!(
                            topic = %request.topic,
                            "Topic created successfully"
                        );
                    }
                    Err((topic_name, err)) => {
                        if err == RDKafkaErrorCode::TopicAlreadyExists {
                            info!(
                                topic = %topic_name,
                                "Topic already exists, treating as success"
                            );
                        } else {
                            return Err(XzatomaError::Watcher(format!(
                                "Failed to create topic '{topic_name}': {err}"
                            )));
                        }
                    }
                }
            }
        }

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
                XzatomaError::Config("SASL username is required when mechanism is set".to_string())
            })?;

            let password = security
                .sasl_password
                .clone()
                .or_else(|| std::env::var("KAFKA_SASL_PASSWORD").ok())
                .ok_or_else(|| {
                    XzatomaError::Config(
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

fn validate_topic_name(topic: &str) -> Result<()> {
    if topic.trim().is_empty() {
        return Err(XzatomaError::Config(
            "Kafka topic name cannot be empty".to_string(),
        ));
    }

    Ok(())
}

fn parse_security_protocol(protocol: &str) -> Result<SecurityProtocol> {
    match protocol.to_uppercase().as_str() {
        "PLAINTEXT" => Ok(SecurityProtocol::Plaintext),
        "SSL" => Ok(SecurityProtocol::Ssl),
        "SASL_PLAINTEXT" => Ok(SecurityProtocol::SaslPlaintext),
        "SASL_SSL" => Ok(SecurityProtocol::SaslSsl),
        _ => Err(XzatomaError::Config(format!(
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
        _ => Err(XzatomaError::Config(format!(
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
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        }
    }

    #[test]
    fn test_watcher_topic_admin_new_uses_basic_fields() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        assert_eq!(admin.input_topic(), "plans.input");
        assert_eq!(admin.output_topic(), None);
    }

    #[test]
    fn test_watcher_topic_admin_get_kafka_config_includes_basic_settings() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        let settings: HashMap<_, _> = admin.get_kafka_config().into_iter().collect();

        assert_eq!(settings.get("bootstrap.servers").unwrap(), "localhost:9092");
        assert_eq!(
            settings.get("client.id").unwrap(),
            "xzatoma-watcher-topic-admin"
        );
        assert_eq!(settings.get("security.protocol").unwrap(), "PLAINTEXT");
    }

    #[test]
    fn test_topics_for_xzepr_watcher_contains_only_input_topic() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        assert_eq!(
            admin.topics_for_xzepr_watcher(),
            vec!["plans.input".to_string()]
        );
    }

    #[test]
    fn test_topics_for_generic_watcher_contains_input_only_when_output_missing() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        assert_eq!(
            admin.topics_for_generic_watcher(),
            vec!["plans.input".to_string()]
        );
    }

    #[test]
    fn test_topics_for_generic_watcher_contains_input_and_output_when_distinct() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.output".to_string());

        let admin = WatcherTopicAdmin::new(&config).unwrap();

        assert_eq!(
            admin.topics_for_generic_watcher(),
            vec!["plans.input".to_string(), "plans.output".to_string()]
        );
    }

    #[test]
    fn test_topics_for_generic_watcher_deduplicates_same_input_and_output_topic() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.input".to_string());

        let admin = WatcherTopicAdmin::new(&config).unwrap();

        assert_eq!(
            admin.topics_for_generic_watcher(),
            vec!["plans.input".to_string()]
        );
    }

    #[test]
    fn test_ensure_requests_for_generic_watcher_include_purposes() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.output".to_string());

        let admin = WatcherTopicAdmin::new(&config).unwrap();
        let requests = admin.ensure_requests_for_generic_watcher();

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].topic, "plans.input");
        assert_eq!(requests[0].purpose, "generic watcher input topic");
        assert_eq!(requests[1].topic, "plans.output");
        assert_eq!(requests[1].purpose, "generic watcher output topic");
    }

    #[test]
    fn test_watcher_topic_admin_new_returns_error_for_invalid_protocol() {
        let mut config = base_kafka_config();
        config.security = Some(KafkaSecurityConfig {
            protocol: "INVALID".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        });

        let result = WatcherTopicAdmin::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_watcher_topic_admin_new_stores_num_partitions() {
        let mut config = base_kafka_config();
        config.num_partitions = 6;

        let admin = WatcherTopicAdmin::new(&config).unwrap();
        assert_eq!(admin.num_partitions, 6);
    }

    #[test]
    fn test_watcher_topic_admin_new_stores_replication_factor() {
        let mut config = base_kafka_config();
        config.replication_factor = 3;

        let admin = WatcherTopicAdmin::new(&config).unwrap();
        assert_eq!(admin.replication_factor, 3);
    }

    #[test]
    fn test_validate_topic_name_rejects_empty() {
        let result = validate_topic_name("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_topic_name_rejects_whitespace_only() {
        let result = validate_topic_name("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_topic_name_accepts_valid_name() {
        let result = validate_topic_name("my-topic.v1");
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // Integration tests -- require a running Kafka broker.
    //
    // Run with:
    //   cargo test --all-features -- --ignored
    // -----------------------------------------------------------------------

    #[tokio::test]
    #[ignore]
    async fn test_ensure_xzepr_watcher_topics_creates_topic_on_broker() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        let result = admin.ensure_xzepr_watcher_topics().await;
        assert!(
            result.is_ok(),
            "ensure_xzepr_watcher_topics failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_ensure_generic_watcher_topics_creates_topics_on_broker() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.output".to_string());

        let admin = WatcherTopicAdmin::new(&config).unwrap();

        let result = admin.ensure_generic_watcher_topics().await;
        assert!(
            result.is_ok(),
            "ensure_generic_watcher_topics failed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_ensure_topics_is_idempotent_on_broker() {
        let config = base_kafka_config();
        let admin = WatcherTopicAdmin::new(&config).unwrap();

        // First call creates the topic.
        let first = admin.ensure_xzepr_watcher_topics().await;
        assert!(first.is_ok(), "first ensure failed: {:?}", first.err());

        // Second call should succeed because TopicAlreadyExists is accepted.
        let second = admin.ensure_xzepr_watcher_topics().await;
        assert!(second.is_ok(), "second ensure failed: {:?}", second.err());
    }

    #[tokio::test]
    async fn test_ensure_topics_returns_error_for_empty_topic_name() {
        let config = KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "".to_string(),
            output_topic: None,
            group_id: "xzatoma-watcher".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        };

        let admin = WatcherTopicAdmin::new(&config).unwrap();
        let result = admin.ensure_xzepr_watcher_topics().await;
        assert!(result.is_err());
    }
}
