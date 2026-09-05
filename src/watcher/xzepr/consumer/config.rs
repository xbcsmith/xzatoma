//! Kafka consumer configuration for XZepr.
//!
//! This module provides configuration structs for connecting to Kafka
//! with support for various authentication mechanisms including SASL/SCRAM.
//!
//! # Example
//!
//! ```rust,no_run
//! use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
//!
//! // Create configuration with defaults
//! let config = KafkaConsumerConfig::new("localhost:9092", "xzepr.events", "my-service");
//!
//! // Or with SASL/SCRAM authentication
//! let secure_config = KafkaConsumerConfig::new("kafka.example.com:9093", "xzepr.events", "my-service")
//!     .with_sasl_scram_sha256("username", "password");
//! ```

use std::time::Duration;
use thiserror::Error;

// The canonical Kafka security types live in `crate::watcher::kafka_security`.
// They are re-exported here so existing
// `crate::watcher::xzepr::consumer::config::{...}` import paths keep working.
pub use crate::watcher::kafka_security::{SaslConfig, SaslMechanism, SecurityProtocol, SslConfig};

// Parsing is delegated to the shared helpers rather than duplicated inline.
use crate::watcher::kafka_security::{
    DEFAULT_MAX_PAYLOAD_BYTES, parse_sasl_mechanism, parse_security_protocol,
};

/// Errors that can occur during configuration.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Missing required configuration value.
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    /// Invalid security protocol specified.
    #[error("Invalid security protocol: {0}")]
    InvalidSecurityProtocol(String),

    /// Invalid SASL mechanism specified.
    #[error("Invalid SASL mechanism: {0}")]
    InvalidSaslMechanism(String),
}

/// Kafka consumer configuration.
///
/// Comprehensive configuration for connecting to Kafka as a consumer,
/// with support for authentication, encryption, and consumer group settings.
///
/// # Example
///
/// ```rust
/// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
///
/// let config = KafkaConsumerConfig::new("localhost:9092", "my-topic", "my-service")
///     .with_group_id("custom-group-id");
///
/// assert_eq!(config.group_id, "custom-group-id");
/// ```
#[derive(Debug, Clone)]
pub struct KafkaConsumerConfig {
    /// Kafka broker addresses (comma-separated).
    pub brokers: String,

    /// Topic to consume from.
    pub topic: String,

    /// Consumer group ID (defaults to `xzepr-consumer-{service_name}`).
    pub group_id: String,

    /// Service name for identification.
    pub service_name: String,

    /// Security protocol for the connection.
    pub security_protocol: SecurityProtocol,

    /// SASL configuration (required for SASL protocols).
    pub sasl_config: Option<SaslConfig>,

    /// SSL configuration (required for SSL protocols).
    pub ssl_config: Option<SslConfig>,

    /// Auto offset reset policy ("earliest" or "latest").
    pub auto_offset_reset: String,

    /// Enable auto commit of offsets.
    pub enable_auto_commit: bool,

    /// Session timeout duration.
    pub session_timeout: Duration,

    /// Broker address family preference passed to rdkafka.
    ///
    /// Valid values: `"v4"`, `"v6"`, `"any"`. Defaults to `"v4"`.
    pub broker_address_family: String,

    /// How long the consumer loop waits for a message before re-checking the
    /// shutdown flag. Defaults to 1 second.
    pub poll_interval: Duration,

    /// Maximum accepted size, in bytes, of a raw Kafka message payload.
    ///
    /// Enforced before the payload is parsed into a [`CloudEventMessage`]
    /// (see [`crate::watcher::xzepr::consumer::XzeprConsumer::process_message`]).
    /// Defaults to [`DEFAULT_MAX_PAYLOAD_BYTES`]; production callers override
    /// this with [`Self::with_max_payload_bytes`] using
    /// `watcher.execution.max_payload_bytes`.
    ///
    /// [`CloudEventMessage`]: crate::watcher::xzepr::consumer::CloudEventMessage
    pub max_payload_bytes: usize,

    /// Maximum time in milliseconds between consecutive `poll()` calls.
    ///
    /// Applied as the rdkafka config key `max.poll.interval.ms`.
    /// Defaults to 3 600 000 (1 hour).
    pub max_poll_interval_ms: u64,

    /// Startup stabilization window in seconds.
    ///
    /// Passed to [`crate::watcher::startup_context::QuietStartupContext`]
    /// when constructing the rdkafka consumer.
    pub startup_stabilization_secs: u64,
}

impl KafkaConsumerConfig {
    /// Creates a new configuration with sensible defaults.
    ///
    /// The consumer group ID defaults to `xzepr-consumer-{service_name}`.
    ///
    /// # Arguments
    ///
    /// * `brokers` - Comma-separated list of Kafka broker addresses
    /// * `topic` - Topic to consume from
    /// * `service_name` - Name of the consuming service
    ///
    /// # Example
    ///
    /// ```rust
    /// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "my-service");
    /// assert_eq!(config.group_id, "xzepr-consumer-my-service");
    /// ```
    pub fn new(brokers: &str, topic: &str, service_name: &str) -> Self {
        let group_id = format!("xzepr-consumer-{}", service_name);
        Self {
            brokers: brokers.to_string(),
            topic: topic.to_string(),
            group_id,
            service_name: service_name.to_string(),
            security_protocol: SecurityProtocol::default(),
            sasl_config: None,
            ssl_config: None,
            auto_offset_reset: "earliest".to_string(),
            enable_auto_commit: false,
            session_timeout: Duration::from_secs(30),
            broker_address_family: "v4".to_string(),
            poll_interval: Duration::from_secs(1),
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_poll_interval_ms: 3_600_000,
            startup_stabilization_secs: 10,
        }
    }

    /// Sets the broker address family preference.
    pub fn with_broker_address_family(mut self, family: &str) -> Self {
        self.broker_address_family = family.to_string();
        self
    }

    /// Sets the consumer poll interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Sets the maximum accepted raw Kafka payload size, in bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "my-service")
    ///     .with_max_payload_bytes(2048);
    /// assert_eq!(config.max_payload_bytes, 2048);
    /// ```
    pub fn with_max_payload_bytes(mut self, max_payload_bytes: usize) -> Self {
        self.max_payload_bytes = max_payload_bytes;
        self
    }

    /// Sets the maximum poll interval in milliseconds.
    ///
    /// Applied as the rdkafka config key `max.poll.interval.ms`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "svc")
    ///     .with_max_poll_interval_ms(1_800_000);
    /// assert_eq!(config.max_poll_interval_ms, 1_800_000);
    /// ```
    pub fn with_max_poll_interval_ms(mut self, ms: u64) -> Self {
        self.max_poll_interval_ms = ms;
        self
    }

    /// Sets the startup stabilization window in seconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "svc")
    ///     .with_startup_stabilization_secs(30);
    /// assert_eq!(config.startup_stabilization_secs, 30);
    /// ```
    pub fn with_startup_stabilization_secs(mut self, secs: u64) -> Self {
        self.startup_stabilization_secs = secs;
        self
    }

    /// Sets a custom consumer group ID.
    ///
    /// # Arguments
    ///
    /// * `group_id` - Custom consumer group ID
    ///
    /// # Example
    ///
    /// ```rust
    /// use xzatoma::watcher::xzepr::consumer::config::KafkaConsumerConfig;
    ///
    /// let config = KafkaConsumerConfig::new("localhost:9092", "events", "my-service")
    ///     .with_group_id("my-custom-group");
    /// assert_eq!(config.group_id, "my-custom-group");
    /// ```
    pub fn with_group_id(mut self, group_id: &str) -> Self {
        self.group_id = group_id.to_string();
        self
    }

    /// Configures SASL/SCRAM-SHA-256 authentication.
    ///
    /// This also sets the security protocol to `SaslSsl` for encrypted connections.
    ///
    /// # Arguments
    ///
    /// * `username` - SASL username
    /// * `password` - SASL password
    ///
    /// # Example
    ///
    /// ```rust
    /// use xzatoma::watcher::xzepr::consumer::config::{KafkaConsumerConfig, SecurityProtocol};
    ///
    /// let config = KafkaConsumerConfig::new("kafka:9093", "events", "my-service")
    ///     .with_sasl_scram_sha256("user", "pass");
    /// assert_eq!(config.security_protocol, SecurityProtocol::SaslSsl);
    /// ```
    pub fn with_sasl_scram_sha256(mut self, username: &str, password: &str) -> Self {
        self.security_protocol = SecurityProtocol::SaslSsl;
        self.sasl_config = Some(SaslConfig {
            mechanism: SaslMechanism::ScramSha256,
            username: username.to_string(),
            password: password.to_string(),
        });
        self
    }

    /// Configures SASL/SCRAM-SHA-512 authentication.
    ///
    /// This also sets the security protocol to `SaslSsl` for encrypted connections.
    ///
    /// # Arguments
    ///
    /// * `username` - SASL username
    /// * `password` - SASL password
    pub fn with_sasl_scram_sha512(mut self, username: &str, password: &str) -> Self {
        self.security_protocol = SecurityProtocol::SaslSsl;
        self.sasl_config = Some(SaslConfig {
            mechanism: SaslMechanism::ScramSha512,
            username: username.to_string(),
            password: password.to_string(),
        });
        self
    }

    /// Configures SSL/TLS encryption with a CA certificate.
    ///
    /// # Arguments
    ///
    /// * `ca_location` - Path to the CA certificate file
    pub fn with_ssl(mut self, ca_location: &str) -> Self {
        self.ssl_config = Some(SslConfig {
            ca_location: Some(ca_location.to_string()),
            certificate_location: None,
            key_location: None,
        });
        self
    }

    /// Sets the auto offset reset policy.
    ///
    /// # Arguments
    ///
    /// * `policy` - Either "earliest" or "latest"
    pub fn with_auto_offset_reset(mut self, policy: &str) -> Self {
        self.auto_offset_reset = policy.to_string();
        self
    }

    /// Disables auto commit of offsets.
    ///
    /// When disabled, the consumer must manually commit offsets.
    pub fn with_manual_commit(mut self) -> Self {
        self.enable_auto_commit = false;
        self
    }

    /// Loads configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// * `XZEPR_KAFKA_BROKERS` - Broker addresses (default: localhost:9092)
    /// * `XZEPR_KAFKA_TOPIC` - Topic name (default: xzepr.dev.events)
    /// * `XZEPR_KAFKA_GROUP_ID` - Consumer group ID (default: xzepr-consumer-{service_name})
    /// * `XZEPR_KAFKA_SECURITY_PROTOCOL` - Security protocol (default: PLAINTEXT)
    /// * `XZEPR_KAFKA_SASL_MECHANISM` - SASL mechanism (default: SCRAM-SHA-256)
    /// * `XZEPR_KAFKA_SASL_USERNAME` - SASL username (required if using SASL)
    /// * `XZEPR_KAFKA_SASL_PASSWORD` - SASL password (required if using SASL)
    /// * `XZEPR_KAFKA_SSL_CA_LOCATION` - CA certificate path
    /// * `XZEPR_KAFKA_SSL_CERT_LOCATION` - Client certificate path
    /// * `XZEPR_KAFKA_SSL_KEY_LOCATION` - Client key path
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingConfig` if required SASL credentials are not set.
    /// Returns `ConfigError::InvalidSecurityProtocol` if the protocol is invalid.
    /// Returns `ConfigError::InvalidSaslMechanism` if the mechanism is invalid.
    pub fn from_env(service_name: &str) -> Result<Self, ConfigError> {
        let brokers =
            std::env::var("XZEPR_KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string());

        let topic =
            std::env::var("XZEPR_KAFKA_TOPIC").unwrap_or_else(|_| "xzepr.dev.events".to_string());

        let group_id = std::env::var("XZEPR_KAFKA_GROUP_ID")
            .unwrap_or_else(|_| format!("xzepr-consumer-{}", service_name));

        let mut config = Self::new(&brokers, &topic, service_name).with_group_id(&group_id);

        // Load security protocol
        let protocol = std::env::var("XZEPR_KAFKA_SECURITY_PROTOCOL")
            .unwrap_or_else(|_| "PLAINTEXT".to_string());

        config.security_protocol = parse_security_protocol(&protocol)
            .map_err(|_| ConfigError::InvalidSecurityProtocol(protocol.clone()))?;

        // Load SASL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::SaslPlaintext | SecurityProtocol::SaslSsl
        ) {
            let username = std::env::var("XZEPR_KAFKA_SASL_USERNAME")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_USERNAME".to_string()))?;
            let password = std::env::var("XZEPR_KAFKA_SASL_PASSWORD")
                .map_err(|_| ConfigError::MissingConfig("XZEPR_KAFKA_SASL_PASSWORD".to_string()))?;

            let mechanism = std::env::var("XZEPR_KAFKA_SASL_MECHANISM")
                .unwrap_or_else(|_| "SCRAM-SHA-256".to_string());

            let sasl_mechanism = parse_sasl_mechanism(&mechanism)
                .map_err(|_| ConfigError::InvalidSaslMechanism(mechanism.clone()))?;

            config.sasl_config = Some(SaslConfig {
                mechanism: sasl_mechanism,
                username,
                password,
            });
        }

        // Load SSL config if needed
        if matches!(
            config.security_protocol,
            SecurityProtocol::Ssl | SecurityProtocol::SaslSsl
        ) {
            let ca_location = std::env::var("XZEPR_KAFKA_SSL_CA_LOCATION").ok();
            if ca_location.is_some() {
                config.ssl_config = Some(SslConfig {
                    ca_location,
                    certificate_location: std::env::var("XZEPR_KAFKA_SSL_CERT_LOCATION").ok(),
                    key_location: std::env::var("XZEPR_KAFKA_SSL_KEY_LOCATION").ok(),
                });
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_new_config_defaults() {
        let config = KafkaConsumerConfig::new("localhost:9092", "test-topic", "test-service");

        assert_eq!(config.brokers, "localhost:9092");
        assert_eq!(config.topic, "test-topic");
        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.group_id, "xzepr-consumer-test-service");
        assert_eq!(config.security_protocol, SecurityProtocol::Plaintext);
        assert!(config.sasl_config.is_none());
        assert!(config.ssl_config.is_none());
        assert_eq!(config.auto_offset_reset, "earliest");
        assert!(!config.enable_auto_commit);
        assert_eq!(config.session_timeout, Duration::from_secs(30));
        assert_eq!(config.max_payload_bytes, DEFAULT_MAX_PAYLOAD_BYTES);
    }

    #[test]
    fn test_with_max_payload_bytes() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_max_payload_bytes(2048);

        assert_eq!(config.max_payload_bytes, 2048);
    }

    #[test]
    fn test_with_group_id() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_group_id("custom-group");

        assert_eq!(config.group_id, "custom-group");
    }

    #[test]
    fn test_with_sasl_scram_sha256() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_sasl_scram_sha256("user", "pass");

        assert_eq!(config.security_protocol, SecurityProtocol::SaslSsl);
        let sasl = config.sasl_config.unwrap();
        assert_eq!(sasl.mechanism, SaslMechanism::ScramSha256);
        assert_eq!(sasl.username, "user");
        assert_eq!(sasl.password, "pass");
    }

    #[test]
    fn test_with_sasl_scram_sha512() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_sasl_scram_sha512("user", "pass");

        assert_eq!(config.security_protocol, SecurityProtocol::SaslSsl);
        let sasl = config.sasl_config.unwrap();
        assert_eq!(sasl.mechanism, SaslMechanism::ScramSha512);
    }

    #[test]
    fn test_with_ssl() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_ssl("/path/to/ca.pem");

        let ssl = config.ssl_config.unwrap();
        assert_eq!(ssl.ca_location, Some("/path/to/ca.pem".to_string()));
        assert!(ssl.certificate_location.is_none());
        assert!(ssl.key_location.is_none());
    }

    #[test]
    fn test_with_auto_offset_reset() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_auto_offset_reset("latest");

        assert_eq!(config.auto_offset_reset, "latest");
    }

    #[test]
    fn test_with_manual_commit() {
        let config =
            KafkaConsumerConfig::new("localhost:9092", "topic", "service").with_manual_commit();

        assert!(!config.enable_auto_commit);
    }

    #[test]
    #[serial]
    fn test_from_env_defaults() {
        // Clear any existing env vars
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_BROKERS");
            std::env::remove_var("XZEPR_KAFKA_TOPIC");
            std::env::remove_var("XZEPR_KAFKA_GROUP_ID");
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
        }

        let config = KafkaConsumerConfig::from_env("test-service").unwrap();

        assert_eq!(config.brokers, "localhost:9092");
        assert_eq!(config.topic, "xzepr.dev.events");
        assert_eq!(config.group_id, "xzepr-consumer-test-service");
        assert_eq!(config.security_protocol, SecurityProtocol::Plaintext);
    }

    #[test]
    #[serial]
    fn test_from_env_custom_values() {
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::set_var("XZEPR_KAFKA_BROKERS", "kafka1:9092,kafka2:9092");
            std::env::set_var("XZEPR_KAFKA_TOPIC", "custom.topic");
            std::env::set_var("XZEPR_KAFKA_GROUP_ID", "custom-group");
            std::env::set_var("XZEPR_KAFKA_SECURITY_PROTOCOL", "SSL");
        }

        let config = KafkaConsumerConfig::from_env("test-service").unwrap();

        assert_eq!(config.brokers, "kafka1:9092,kafka2:9092");
        assert_eq!(config.topic, "custom.topic");
        assert_eq!(config.group_id, "custom-group");
        assert_eq!(config.security_protocol, SecurityProtocol::Ssl);

        // Clean up
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_BROKERS");
            std::env::remove_var("XZEPR_KAFKA_TOPIC");
            std::env::remove_var("XZEPR_KAFKA_GROUP_ID");
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
        }
    }

    #[test]
    #[serial]
    fn test_from_env_invalid_protocol() {
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::set_var("XZEPR_KAFKA_SECURITY_PROTOCOL", "INVALID");
        }

        let result = KafkaConsumerConfig::from_env("test-service");
        assert!(matches!(
            result,
            Err(ConfigError::InvalidSecurityProtocol(_))
        ));

        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
        }
    }

    #[test]
    #[serial]
    fn test_from_env_sasl_missing_username() {
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::set_var("XZEPR_KAFKA_SECURITY_PROTOCOL", "SASL_SSL");
            std::env::remove_var("XZEPR_KAFKA_SASL_USERNAME");
        }

        let result = KafkaConsumerConfig::from_env("test-service");
        assert!(matches!(result, Err(ConfigError::MissingConfig(_))));

        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
        }
    }

    #[test]
    fn test_config_builder_chaining() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "service")
            .with_group_id("custom-group")
            .with_auto_offset_reset("latest")
            .with_manual_commit()
            .with_ssl("/path/to/ca.pem");

        assert_eq!(config.group_id, "custom-group");
        assert_eq!(config.auto_offset_reset, "latest");
        assert!(!config.enable_auto_commit);
        assert!(config.ssl_config.is_some());
    }
}
