//! Producer abstraction and implementations for the generic Kafka watcher.
//!
//! This module provides [`ResultProducerTrait`] — the single interface for
//! publishing [`GenericPlanResult`] messages — along with three concrete
//! implementations:
//!
//! - [`GenericResultProducer`]: production-grade Kafka publisher configured
//!   with idempotent delivery settings (`acks=all`, `enable.idempotence=true`,
//!   `retries=5`, `compression.type=snappy`)
//! - [`FakeResultProducer`]: in-memory accumulator for unit and integration
//!   testing without a running broker
//! - [`BufferedResultProducer`]: dead-letter queue wrapper that retains events
//!   when the broker is unreachable and drains them automatically when it
//!   recovers
//!
//! # Idempotent delivery
//!
//! [`GenericResultProducer`] is configured with `acks=all`,
//! `enable.idempotence=true`, `retries=5`, and `compression.type=snappy` so
//! that every produce request survives transient broker errors without
//! introducing duplicates.
//!
//! # Dead-letter buffering
//!
//! [`BufferedResultProducer`] wraps any [`ResultProducerTrait`] and retains
//! publish failures in a bounded in-memory queue. On the next successful
//! publish call the queue is drained in insertion order before the new event
//! is forwarded. When the queue is at capacity ([`DEFAULT_DLQ_MAX_BUFFERED`])
//! the oldest event is dropped and a `warn!` log is emitted.

use crate::config::KafkaWatcherConfig;
use crate::error::{Result, XzatomaError};
use crate::watcher::generic::result_event::GenericPlanResult;
use crate::watcher::xzepr::consumer::config::{
    SaslConfig, SaslMechanism, SecurityProtocol, SslConfig,
};
use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rdkafka::ClientConfig;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Default maximum number of events retained in the dead-letter buffer.
///
/// When [`BufferedResultProducer`]'s buffer reaches this size the oldest entry
/// is dropped on the next publish failure and a `warn!` log is emitted.
pub const DEFAULT_DLQ_MAX_BUFFERED: usize = 100;

// -----------------------------------------------------------------------------
// Trait
// -----------------------------------------------------------------------------

/// Abstraction over publishing plan results to the output topic.
///
/// Both methods are `async` and must be implemented by all concrete producer
/// types. Implementations must be [`Send`] and [`Sync`] so they can be stored
/// behind `Arc<dyn ResultProducerTrait>` and shared across async tasks.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzatoma::watcher::generic::{FakeResultProducer, ResultProducerTrait};
/// use xzatoma::watcher::generic::result_event::GenericPlanResult;
///
/// # #[tokio::main]
/// # async fn main() {
/// let producer: Arc<dyn ResultProducerTrait> = Arc::new(FakeResultProducer::new());
/// let result = GenericPlanResult::new("t-1".to_string(), true, "ok".to_string());
/// producer.publish(&result).await.unwrap();
/// # }
/// ```
#[async_trait]
pub trait ResultProducerTrait: Send + Sync {
    /// Publish a single plan result to the configured output topic.
    ///
    /// # Arguments
    ///
    /// * `result` - The result event to serialize and publish.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Watcher`] if serialization fails or if the
    /// broker is unreachable.
    async fn publish(&self, result: &GenericPlanResult) -> Result<()>;

    /// Flush any in-flight produce requests to the broker.
    ///
    /// Callers may invoke this after a batch of publishes to ensure delivery
    /// before the process exits. For in-memory implementations the call is a
    /// no-op.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for in-flight messages to be
    ///   delivered.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Watcher`] if flushing fails before the timeout
    /// elapses.
    async fn flush(&self, timeout: Duration) -> Result<()>;
}

// -----------------------------------------------------------------------------
// GenericResultProducer
// -----------------------------------------------------------------------------

/// Kafka result producer for the generic watcher.
///
/// This type assembles Kafka producer configuration and publishes
/// [`GenericPlanResult`] messages to the configured output topic using an
/// `rdkafka` `FutureProducer`.
///
/// # Idempotent delivery
///
/// The internal `FutureProducer` is configured with `acks=all`,
/// `enable.idempotence=true`, `retries=5`, and `compression.type=snappy` to
/// prevent duplicates in the face of transient broker failures.
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
/// ```no_run
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
    /// configuration. Idempotent delivery settings (`acks=all`,
    /// `enable.idempotence=true`, `retries=5`, `compression.type=snappy`) are
    /// applied unconditionally. The producer is ready to publish immediately
    /// after construction.
    ///
    /// # Arguments
    ///
    /// * `config` - Watcher Kafka configuration shared with the consumer.
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
    /// ```no_run
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
        // Idempotent delivery settings are applied unconditionally so that
        // every produce request is retried safely without introducing
        // duplicate messages.
        let producer: FutureProducer = {
            let mut client_config = ClientConfig::new();
            client_config.set("bootstrap.servers", &config.brokers);
            client_config.set("client.id", &client_id);
            client_config.set("security.protocol", security_protocol.as_str());
            client_config.set(
                "message.timeout.ms",
                request_timeout.as_millis().to_string(),
            );

            // Idempotent delivery settings.
            client_config.set("acks", "all");
            client_config.set("retries", "5");
            client_config.set("compression.type", "snappy");
            client_config.set("enable.idempotence", "true");

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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::generic::GenericResultProducer;
    ///
    /// let config = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans.in".to_string(),
    ///     output_topic: Some("plans.out".to_string()),
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let producer = GenericResultProducer::new(&config).unwrap();
    /// assert_eq!(producer.output_topic(), "plans.out");
    /// ```
    pub fn output_topic(&self) -> &str {
        &self.output_topic
    }

    /// Return the input topic associated with this producer configuration.
    ///
    /// # Returns
    ///
    /// The topic the watcher consumes from.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::config::KafkaWatcherConfig;
    /// use xzatoma::watcher::generic::GenericResultProducer;
    ///
    /// let config = KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "plans.in".to_string(),
    ///     output_topic: None,
    ///     group_id: "watcher-group".to_string(),
    ///     auto_create_topics: true,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     security: None,
    /// };
    ///
    /// let producer = GenericResultProducer::new(&config).unwrap();
    /// assert_eq!(producer.input_topic(), "plans.in");
    /// ```
    pub fn input_topic(&self) -> &str {
        &self.input_topic
    }

    /// Return the Kafka configuration as key-value settings.
    ///
    /// Assembles the full set of Kafka client configuration key-value pairs
    /// from the producer's fields, including idempotent delivery settings.
    /// Suitable for use in tests and diagnostic tooling.
    ///
    /// # Returns
    ///
    /// A vector of Kafka configuration key-value pairs.
    ///
    /// # Examples
    ///
    /// ```no_run
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
    /// assert_eq!(settings.get("acks").unwrap(), "all");
    /// assert_eq!(settings.get("enable.idempotence").unwrap(), "true");
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
            // Idempotent delivery settings.
            ("acks".to_string(), "all".to_string()),
            ("retries".to_string(), "5".to_string()),
            ("compression.type".to_string(), "snappy".to_string()),
            ("enable.idempotence".to_string(), "true".to_string()),
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
}

#[async_trait]
impl ResultProducerTrait for GenericResultProducer {
    /// Publish a generic watcher result to the configured output topic.
    ///
    /// Serializes the result to JSON and sends it to the Kafka output topic
    /// using the underlying `FutureProducer`. The `trigger_event_id` is used
    /// as the Kafka message key for deterministic partitioning.
    ///
    /// Each successful publish is logged at `info` level.
    ///
    /// # Arguments
    ///
    /// * `result` - The result event to publish.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Watcher`] if JSON serialization fails or if
    /// the Kafka produce request fails.
    async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
        let payload = serde_json::to_string(result).map_err(|e| {
            XzatomaError::Watcher(format!(
                "Failed to serialize generic watcher result payload: {e}"
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
                    "Failed to publish result to topic '{}': {kafka_error}",
                    self.output_topic
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

    /// Flush all in-flight produce requests to the broker.
    ///
    /// Delegates to the underlying `rdkafka` `FutureProducer::flush`, waiting
    /// up to `timeout` for all pending messages to be acknowledged.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for in-flight messages.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Watcher`] if flushing fails before the timeout
    /// elapses.
    async fn flush(&self, timeout: Duration) -> Result<()> {
        self.producer
            .flush(timeout)
            .map_err(|e| XzatomaError::Watcher(format!("Failed to flush Kafka producer: {e}")))
    }
}

// -----------------------------------------------------------------------------
// FakeResultProducer
// -----------------------------------------------------------------------------

/// In-memory result producer for unit and integration testing.
///
/// `FakeResultProducer` accumulates published results in an internal `Vec`
/// protected by a [`tokio::sync::Mutex`]. No network I/O is performed.
///
/// This type is declared `pub` (not `#[cfg(test)]`) so it can be used by
/// integration tests in the `tests/` directory without requiring a live Kafka
/// broker.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::{FakeResultProducer, ResultProducerTrait};
/// use xzatoma::watcher::generic::result_event::GenericPlanResult;
///
/// # #[tokio::main]
/// # async fn main() {
/// let producer = FakeResultProducer::new();
/// let result = GenericPlanResult::new("t-1".to_string(), true, "ok".to_string());
/// producer.publish(&result).await.unwrap();
///
/// let events = producer.published_events().await;
/// assert_eq!(events.len(), 1);
/// assert_eq!(events[0].trigger_event_id, "t-1");
/// # }
/// ```
pub struct FakeResultProducer {
    /// Accumulated published events, in insertion order.
    published: Mutex<Vec<GenericPlanResult>>,
}

impl fmt::Debug for FakeResultProducer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeResultProducer").finish_non_exhaustive()
    }
}

impl Default for FakeResultProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeResultProducer {
    /// Create a new empty fake producer with no accumulated events.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::FakeResultProducer;
    ///
    /// let producer = FakeResultProducer::new();
    /// ```
    pub fn new() -> Self {
        Self {
            published: Mutex::new(Vec::new()),
        }
    }

    /// Return a clone of all events published so far, in insertion order.
    ///
    /// # Returns
    ///
    /// A `Vec` containing clones of every [`GenericPlanResult`] passed to
    /// [`publish`](ResultProducerTrait::publish) since construction.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::FakeResultProducer;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let producer = FakeResultProducer::new();
    /// assert!(producer.published_events().await.is_empty());
    /// # }
    /// ```
    pub async fn published_events(&self) -> Vec<GenericPlanResult> {
        self.published.lock().await.clone()
    }
}

#[async_trait]
impl ResultProducerTrait for FakeResultProducer {
    async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
        self.published.lock().await.push(result.clone());
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> Result<()> {
        // No-op: no in-flight messages in the in-memory implementation.
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// BufferedResultProducer
// -----------------------------------------------------------------------------

/// Dead-letter queue wrapper for resilient result publication.
///
/// `BufferedResultProducer` wraps any [`ResultProducerTrait`] implementation
/// and buffers publish failures in a bounded in-memory queue. When the inner
/// producer fails to publish an event, the event is appended to the buffer
/// instead of surfacing an error to the caller. On the next `publish` call the
/// buffer is drained before the new event is forwarded, so events are delivered
/// in insertion order when the broker recovers.
///
/// When the buffer reaches `max_buffered` events the oldest entry is dropped
/// and a `warn!` log is emitted so operators are alerted to persistent broker
/// unavailability.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use xzatoma::watcher::generic::{
///     BufferedResultProducer, FakeResultProducer, ResultProducerTrait,
///     DEFAULT_DLQ_MAX_BUFFERED,
/// };
/// use xzatoma::watcher::generic::result_event::GenericPlanResult;
///
/// # #[tokio::main]
/// # async fn main() {
/// let inner = Arc::new(FakeResultProducer::new());
/// let buffered = BufferedResultProducer::new(inner.clone(), DEFAULT_DLQ_MAX_BUFFERED);
///
/// let result = GenericPlanResult::new("t-1".to_string(), true, "ok".to_string());
/// buffered.publish(&result).await.unwrap();
///
/// assert_eq!(inner.published_events().await.len(), 1);
/// assert_eq!(buffered.pending_count().await, 0);
/// # }
/// ```
pub struct BufferedResultProducer {
    /// The wrapped inner producer that receives successfully forwarded events.
    inner: Arc<dyn ResultProducerTrait>,
    /// Dead-letter buffer of events that failed to publish, in insertion order.
    buffer: Mutex<VecDeque<GenericPlanResult>>,
    /// Maximum number of events to retain before dropping the oldest.
    max_buffered: usize,
}

impl fmt::Debug for BufferedResultProducer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferedResultProducer")
            .field("max_buffered", &self.max_buffered)
            .finish_non_exhaustive()
    }
}

impl BufferedResultProducer {
    /// Construct a new buffered producer wrapping an existing inner producer.
    ///
    /// # Arguments
    ///
    /// * `inner` - The underlying producer to delegate successful publishes to.
    /// * `max_buffered` - Maximum number of events to retain in the buffer.
    ///   Use [`DEFAULT_DLQ_MAX_BUFFERED`] for the project default.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzatoma::watcher::generic::{
    ///     BufferedResultProducer, FakeResultProducer, DEFAULT_DLQ_MAX_BUFFERED,
    /// };
    ///
    /// let inner = Arc::new(FakeResultProducer::new());
    /// let producer = BufferedResultProducer::new(inner, DEFAULT_DLQ_MAX_BUFFERED);
    /// ```
    pub fn new(inner: Arc<dyn ResultProducerTrait>, max_buffered: usize) -> Self {
        Self {
            inner,
            buffer: Mutex::new(VecDeque::new()),
            max_buffered,
        }
    }

    /// Return the current number of events waiting in the dead-letter buffer.
    ///
    /// # Returns
    ///
    /// The count of events buffered due to prior publish failures.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzatoma::watcher::generic::{
    ///     BufferedResultProducer, FakeResultProducer, DEFAULT_DLQ_MAX_BUFFERED,
    /// };
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let inner = Arc::new(FakeResultProducer::new());
    /// let producer = BufferedResultProducer::new(inner, DEFAULT_DLQ_MAX_BUFFERED);
    /// assert_eq!(producer.pending_count().await, 0);
    /// # }
    /// ```
    pub async fn pending_count(&self) -> usize {
        self.buffer.lock().await.len()
    }
}

#[async_trait]
impl ResultProducerTrait for BufferedResultProducer {
    /// Publish a result, first draining any previously buffered events.
    ///
    /// **Drain phase**: attempt to publish each buffered event via `inner` in
    /// insertion order. Stop draining on the first failure so that events are
    /// never reordered.
    ///
    /// **Publish phase**: attempt to publish `result` via `inner`. If that
    /// fails, append `result` to the buffer. If the buffer is at capacity,
    /// drop the oldest entry and emit a `warn!` log.
    ///
    /// This method always returns `Ok(())`. Publish failures are absorbed into
    /// the buffer rather than propagated to the caller so the watcher consume
    /// loop is not interrupted by transient broker unavailability.
    ///
    /// # Arguments
    ///
    /// * `result` - The result event to publish.
    ///
    /// # Errors
    ///
    /// Always returns `Ok(())`.
    async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
        let mut buf = self.buffer.lock().await;

        // Drain phase: forward buffered events to inner in insertion order.
        // Stop on the first failure; do not reorder events.
        while let Some(buffered) = buf.front().cloned() {
            if self.inner.publish(&buffered).await.is_err() {
                // Inner is still unavailable; stop draining.
                break;
            }
            buf.pop_front();
        }

        // Publish phase: attempt to forward the new event.
        if self.inner.publish(result).await.is_err() {
            if buf.len() >= self.max_buffered {
                buf.pop_front();
                warn!(
                    max_buffered = self.max_buffered,
                    "Dead-letter buffer full; dropping oldest buffered result"
                );
            }
            buf.push_back(result.clone());
        }

        Ok(())
    }

    /// Drain the buffer and delegate to the inner producer's flush.
    ///
    /// Attempts to drain the dead-letter buffer before calling
    /// `inner.flush(timeout)` so that any recovered events are delivered
    /// promptly.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for in-flight messages.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by `inner.flush`.
    async fn flush(&self, timeout: Duration) -> Result<()> {
        let mut buf = self.buffer.lock().await;

        // Drain buffered events before flushing.
        while let Some(buffered) = buf.front().cloned() {
            if self.inner.publish(&buffered).await.is_err() {
                break;
            }
            buf.pop_front();
        }

        // Release the lock before calling inner.flush to avoid holding the
        // buffer lock across a potentially long-running await.
        drop(buf);

        self.inner.flush(timeout).await
    }
}

// -----------------------------------------------------------------------------
// Private helpers
// -----------------------------------------------------------------------------

/// Parse a security protocol string into the corresponding enum variant.
fn parse_security_protocol(protocol: &str) -> Result<SecurityProtocol> {
    match protocol.to_uppercase().as_str() {
        "PLAINTEXT" => Ok(SecurityProtocol::Plaintext),
        "SSL" => Ok(SecurityProtocol::Ssl),
        "SASL_PLAINTEXT" => Ok(SecurityProtocol::SaslPlaintext),
        "SASL_SSL" => Ok(SecurityProtocol::SaslSsl),
        _ => Err(XzatomaError::Watcher(format!(
            "Invalid security protocol: {protocol}"
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
            "Invalid SASL mechanism: {mechanism}"
        ))),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KafkaSecurityConfig;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // -------------------------------------------------------------------------
    // Test double: ControlledProducer
    //
    // Fails the first `fail_count` calls to `publish`, then succeeds for all
    // subsequent calls. Used to simulate transient broker unavailability and
    // subsequent recovery without a running Kafka broker.
    // -------------------------------------------------------------------------

    struct ControlledProducer {
        /// Number of remaining publish calls that will return an error.
        fail_remaining: AtomicUsize,
        /// Events that were successfully published (fail_remaining was 0).
        published: Mutex<Vec<GenericPlanResult>>,
    }

    impl ControlledProducer {
        fn new(fail_count: usize) -> Self {
            Self {
                fail_remaining: AtomicUsize::new(fail_count),
                published: Mutex::new(Vec::new()),
            }
        }

        async fn published_events(&self) -> Vec<GenericPlanResult> {
            self.published.lock().await.clone()
        }
    }

    impl fmt::Debug for ControlledProducer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ControlledProducer").finish_non_exhaustive()
        }
    }

    #[async_trait]
    impl ResultProducerTrait for ControlledProducer {
        async fn publish(&self, result: &GenericPlanResult) -> Result<()> {
            let remaining = self.fail_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.fail_remaining.fetch_sub(1, Ordering::SeqCst);
                return Err(XzatomaError::Watcher(
                    "ControlledProducer: simulated broker failure".to_string(),
                ));
            }
            self.published.lock().await.push(result.clone());
            Ok(())
        }

        async fn flush(&self, _timeout: Duration) -> Result<()> {
            Ok(())
        }
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

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

    fn make_result(trigger_id: &str) -> GenericPlanResult {
        GenericPlanResult::new(trigger_id.to_string(), true, "ok".to_string())
    }

    // -------------------------------------------------------------------------
    // GenericResultProducer tests
    // -------------------------------------------------------------------------

    #[test]
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
    fn test_generic_result_producer_uses_input_topic_when_output_topic_not_set() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();

        assert_eq!(producer.input_topic(), "plans.input");
        assert_eq!(producer.output_topic(), "plans.input");
    }

    #[test]
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
    fn test_generic_result_producer_uses_explicit_output_topic_when_configured() {
        let mut config = base_kafka_config();
        config.output_topic = Some("plans.output".to_string());

        let producer = GenericResultProducer::new(&config).unwrap();

        assert_eq!(producer.input_topic(), "plans.input");
        assert_eq!(producer.output_topic(), "plans.output");
    }

    #[test]
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
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
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
    fn test_generic_result_producer_get_kafka_config_includes_idempotent_settings() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();
        let settings: HashMap<_, _> = producer.get_kafka_config().into_iter().collect();

        assert_eq!(settings.get("acks").unwrap(), "all");
        assert_eq!(settings.get("retries").unwrap(), "5");
        assert_eq!(settings.get("compression.type").unwrap(), "snappy");
        assert_eq!(settings.get("enable.idempotence").unwrap(), "true");
    }

    #[test]
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
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
    #[ignore = "instantiates an rdkafka FutureProducer, which can attempt broker communication"]
    fn test_generic_result_producer_debug_impl() {
        let config = base_kafka_config();
        let producer = GenericResultProducer::new(&config).unwrap();

        let debug_output = format!("{producer:?}");
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

    // -------------------------------------------------------------------------
    // FakeResultProducer tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_fake_producer_new_is_empty() {
        let producer = FakeResultProducer::new();
        assert!(producer.published_events().await.is_empty());
    }

    #[tokio::test]
    async fn test_fake_producer_records_single_event() {
        let producer = FakeResultProducer::new();
        let result = make_result("t-1");

        producer.publish(&result).await.unwrap();

        let events = producer.published_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trigger_event_id, "t-1");
    }

    #[tokio::test]
    async fn test_fake_producer_records_multiple_events_in_order() {
        let producer = FakeResultProducer::new();

        producer.publish(&make_result("first")).await.unwrap();
        producer.publish(&make_result("second")).await.unwrap();
        producer.publish(&make_result("third")).await.unwrap();

        let events = producer.published_events().await;
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].trigger_event_id, "first");
        assert_eq!(events[1].trigger_event_id, "second");
        assert_eq!(events[2].trigger_event_id, "third");
    }

    #[tokio::test]
    async fn test_fake_producer_flush_is_noop() {
        let producer = FakeResultProducer::new();
        // flush must not panic and must return Ok regardless of published state.
        let result = producer.flush(Duration::from_millis(10)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_trait_object_dispatch() {
        // Keep a concrete Arc<FakeResultProducer> for inspection and a separate
        // Arc<dyn ResultProducerTrait> for publishing to verify dispatch routes
        // through the trait correctly.
        let fake = Arc::new(FakeResultProducer::new());
        let trait_obj: Arc<dyn ResultProducerTrait> = fake.clone();

        let result = make_result("trait-dispatch");
        trait_obj.publish(&result).await.unwrap();

        // Verify the event reached the concrete FakeResultProducer.
        let events = fake.published_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trigger_event_id, "trait-dispatch");
    }

    // -------------------------------------------------------------------------
    // BufferedResultProducer tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_buffered_publish_success_leaves_buffer_empty() {
        // Inner always succeeds (fail_count = 0).
        let inner = Arc::new(FakeResultProducer::new());
        let buffered = BufferedResultProducer::new(inner.clone(), DEFAULT_DLQ_MAX_BUFFERED);

        buffered.publish(&make_result("t-1")).await.unwrap();

        assert_eq!(buffered.pending_count().await, 0);
        assert_eq!(inner.published_events().await.len(), 1);
    }

    #[tokio::test]
    async fn test_buffered_publish_buffers_on_broker_failure() {
        // ControlledProducer fails its first call.
        let inner = Arc::new(ControlledProducer::new(1));
        let buffered = BufferedResultProducer::new(inner.clone(), DEFAULT_DLQ_MAX_BUFFERED);

        // Publish: drain (nothing buffered), then inner.publish(t-1) fails.
        // t-1 is appended to the buffer.
        buffered.publish(&make_result("t-1")).await.unwrap();

        assert_eq!(buffered.pending_count().await, 1);
        assert!(inner.published_events().await.is_empty());
    }

    #[tokio::test]
    async fn test_buffered_multiple_failures_accumulate() {
        // With fail_count=5, three consecutive publishes will each fail both the
        // drain attempt and the new-event publish, causing the buffer to grow to 3.
        //
        // publish(t-1): drain empty.      inner.publish(t-1) fail (5->4). buffer=[t-1]
        // publish(t-2): drain t-1 fail (4->3), stop. inner.publish(t-2) fail (3->2). buffer=[t-1,t-2]
        // publish(t-3): drain t-1 fail (2->1), stop. inner.publish(t-3) fail (1->0). buffer=[t-1,t-2,t-3]
        let inner = Arc::new(ControlledProducer::new(5));
        let buffered = BufferedResultProducer::new(inner.clone(), DEFAULT_DLQ_MAX_BUFFERED);

        buffered.publish(&make_result("t-1")).await.unwrap();
        buffered.publish(&make_result("t-2")).await.unwrap();
        buffered.publish(&make_result("t-3")).await.unwrap();

        assert_eq!(buffered.pending_count().await, 3);
        assert!(inner.published_events().await.is_empty());
    }

    #[tokio::test]
    async fn test_buffered_drains_buffer_when_broker_recovers() {
        // ControlledProducer fails its first call only, then succeeds.
        //
        // publish(t-1): drain empty. inner.publish(t-1) fail (1->0). buffer=[t-1]
        // publish(t-2): drain t-1: success (0). pop t-1. buffer=[].
        //               inner.publish(t-2): success. buffer=[]
        // inner.published = [t-1, t-2] in insertion order.
        let inner = Arc::new(ControlledProducer::new(1));
        let buffered = BufferedResultProducer::new(inner.clone(), DEFAULT_DLQ_MAX_BUFFERED);

        buffered.publish(&make_result("t-1")).await.unwrap();
        // Buffer holds t-1 after the first failure.
        assert_eq!(buffered.pending_count().await, 1);

        // On the second publish the drain succeeds, clearing the buffer, and
        // the new event is also forwarded successfully.
        buffered.publish(&make_result("t-2")).await.unwrap();

        assert_eq!(buffered.pending_count().await, 0);

        let published = inner.published_events().await;
        assert_eq!(published.len(), 2);
        // Ordering must be preserved: buffered t-1 is drained before t-2.
        assert_eq!(published[0].trigger_event_id, "t-1");
        assert_eq!(published[1].trigger_event_id, "t-2");
    }

    #[tokio::test]
    async fn test_buffered_drops_oldest_when_buffer_full() {
        // max_buffered = 2. ControlledProducer fails first 5 calls.
        //
        // publish(t-1): drain empty. inner fail (5->4). buffer=[t-1]
        // publish(t-2): drain t-1 fail (4->3), stop. inner.publish(t-2) fail (3->2).
        //               buffer=[t-1,t-2]  (now at capacity)
        // publish(t-3): drain t-1 fail (2->1), stop. inner.publish(t-3) fail (1->0).
        //               buffer full: drop t-1, push t-3. buffer=[t-2,t-3]
        //               fail_remaining is now 0.
        // publish(t-4): drain t-2: success. drain t-3: success. buffer=[].
        //               inner.publish(t-4): success. buffer=[]
        // inner.published = [t-2, t-3, t-4]  (t-1 was dropped)
        let inner = Arc::new(ControlledProducer::new(5));
        let buffered = BufferedResultProducer::new(inner.clone(), 2);

        buffered.publish(&make_result("t-1")).await.unwrap();
        buffered.publish(&make_result("t-2")).await.unwrap();
        // Buffer is now full at capacity 2.
        assert_eq!(buffered.pending_count().await, 2);

        // This publish triggers the capacity drop: t-1 is evicted, t-3 enters.
        buffered.publish(&make_result("t-3")).await.unwrap();
        assert_eq!(buffered.pending_count().await, 2);

        // Broker has now recovered (fail_remaining=0). Drain and publish t-4.
        buffered.publish(&make_result("t-4")).await.unwrap();
        assert_eq!(buffered.pending_count().await, 0);

        let published = inner.published_events().await;
        assert_eq!(published.len(), 3);
        // t-1 was evicted; t-2, t-3, t-4 were published in order.
        assert_eq!(published[0].trigger_event_id, "t-2");
        assert_eq!(published[1].trigger_event_id, "t-3");
        assert_eq!(published[2].trigger_event_id, "t-4");
    }
}
