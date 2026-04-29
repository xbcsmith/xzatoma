//! Consumer abstraction for the generic Kafka watcher.
//!
//! This module provides the [`GenericConsumerTrait`] trait and two concrete
//! implementations:
//!
//! - [`RealGenericConsumer`]: wraps [`rdkafka::consumer::StreamConsumer`] and
//!   handles transient broker errors internally by swallowing them into dummy
//!   skip messages so the watcher loop survives brief outages.
//! - [`FakeGenericConsumer`]: an in-memory queue-backed consumer for driving
//!   the watcher loop in unit and integration tests without a live Kafka
//!   connection.
//!
//! # RawKafkaMessage
//!
//! [`RawKafkaMessage`] is the boundary type between the raw Kafka byte stream
//! and the parsed event pipeline. It carries the UTF-8 payload, the source
//! topic, and an optional message key.
//!
//! # Transient error handling
//!
//! [`RealGenericConsumer::next`] classifies Kafka consumer errors using
//! [`is_transient_kafka_recv_error`]. Transient errors (broker transport
//! failures, all-brokers-down, network exceptions) are logged at `warn` level
//! and converted into a dummy-skip `Ok(...)` so the watcher loop continues.
//! Non-transient errors propagate as `Some(Err(...))`.
//!
//! # Offset commits
//!
//! [`RealGenericConsumer`] performs explicit offset commits via
//! [`rdkafka::consumer::Consumer::commit`] with a [`rdkafka::TopicPartitionList`].
//! Callers must invoke [`GenericConsumerTrait::commit`] after processing each
//! message to advance the committed offset.
//!
//! # Examples
//!
//! ```
//! use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, RawKafkaMessage};
//! use xzatoma::watcher::generic::consumer::GenericConsumerTrait;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let msg = RawKafkaMessage {
//!     payload: "name: deploy\nsteps:\n  - name: s1\n    action: echo hi\n".to_string(),
//!     topic: "plans.input".to_string(),
//!     key: Some("corr-1".to_string()),
//! };
//! let mut consumer = FakeGenericConsumer::new(vec![Ok(msg)]);
//! assert_eq!(consumer.len(), 1);
//! let item = consumer.next().await;
//! assert!(matches!(item, Some(Ok(_))));
//! consumer.commit().await.unwrap();
//! assert_eq!(consumer.commits_recorded(), 1);
//! # }
//! ```

use crate::error::{Result, XzatomaError};
use async_trait::async_trait;
use futures::StreamExt;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::{ClientConfig, Message, Offset, TopicPartitionList};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::warn;

// ---------------------------------------------------------------------------
// RawKafkaMessage
// ---------------------------------------------------------------------------

/// A raw Kafka message payload before plan parsing.
///
/// `RawKafkaMessage` is the boundary type between the raw Kafka byte stream
/// and the parsed plan event pipeline. It is the primary input to
/// [`crate::watcher::generic::event_handler::GenericEventHandler::handle`].
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::consumer::RawKafkaMessage;
///
/// let msg = RawKafkaMessage {
///     payload: "name: deploy\nsteps:\n  - name: s1\n    action: echo hi\n".to_string(),
///     topic: "plans.input".to_string(),
///     key: Some("correlation-123".to_string()),
/// };
/// assert_eq!(msg.topic, "plans.input");
/// assert!(msg.key.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RawKafkaMessage {
    /// The raw Kafka message payload (UTF-8 encoded).
    pub payload: String,
    /// The Kafka topic from which this message was consumed.
    pub topic: String,
    /// Optional Kafka message key used as the correlation key for result tracking.
    pub key: Option<String>,
}

// ---------------------------------------------------------------------------
// GenericConsumerTrait
// ---------------------------------------------------------------------------

/// Trait for consuming messages from a Kafka-like source.
///
/// Both methods are `async` so implementations may perform I/O without
/// blocking the calling thread. `Send + Sync` bounds allow the trait object
/// to be used safely across task boundaries.
///
/// # Contract
///
/// - [`next`](GenericConsumerTrait::next) returns `None` when the stream is
///   exhausted.
/// - [`commit`](GenericConsumerTrait::commit) commits the most recently
///   returned offset. Callers should call `commit` once per successfully
///   received message.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::consumer::{
///     FakeGenericConsumer, GenericConsumerTrait, RawKafkaMessage,
/// };
///
/// # #[tokio::main]
/// # async fn main() {
/// let mut consumer = FakeGenericConsumer::new(vec![
///     Ok(RawKafkaMessage {
///         payload: "test".to_string(),
///         topic: "t".to_string(),
///         key: None,
///     }),
/// ]);
///
/// let item = consumer.next().await;
/// assert!(matches!(item, Some(Ok(_))));
/// consumer.commit().await.unwrap();
/// assert_eq!(consumer.commits_recorded(), 1);
/// # }
/// ```
#[async_trait]
pub trait GenericConsumerTrait: Send + Sync {
    /// Return the next available message.
    ///
    /// Returns `None` when the stream is exhausted. Returns
    /// `Some(Err(...))` for non-transient consumer-level errors. Returns
    /// `Some(Ok(...))` for successfully received messages; implementations
    /// may also return a dummy-skip `Ok` for transient errors so the loop
    /// continues.
    ///
    /// # Errors
    ///
    /// Returns `Some(Err(XzatomaError::Watcher(...)))` for fatal Kafka errors
    /// that should stop the consumer loop.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{
    ///     FakeGenericConsumer, GenericConsumerTrait, RawKafkaMessage,
    /// };
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// assert!(consumer.next().await.is_none());
    /// # }
    /// ```
    async fn next(&mut self) -> Option<Result<RawKafkaMessage>>;

    /// Commit the most recently returned offset.
    ///
    /// For [`RealGenericConsumer`] this advances the consumer group offset in
    /// the Kafka broker. For [`FakeGenericConsumer`] this increments an
    /// internal counter used for test assertions.
    ///
    /// # Errors
    ///
    /// Returns `Err(XzatomaError::Watcher(...))` if the Kafka commit call
    /// fails (e.g. broker unavailable).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, GenericConsumerTrait};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// consumer.commit().await.unwrap();
    /// assert_eq!(consumer.commits_recorded(), 1);
    /// # }
    /// ```
    async fn commit(&mut self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// is_transient_kafka_recv_error
// ---------------------------------------------------------------------------

/// Returns `true` when the Kafka error string contains a known transient
/// broker connectivity error substring.
///
/// The following substrings are classified as transient:
/// - `"BrokerTransportFailure"`
/// - `"AllBrokersDown"`
/// - `"NetworkException"`
///
/// All other error strings return `false`.
fn is_transient_kafka_recv_error(err: &KafkaError) -> bool {
    let s = err.to_string();
    s.contains("BrokerTransportFailure")
        || s.contains("AllBrokersDown")
        || s.contains("NetworkException")
}

// ---------------------------------------------------------------------------
// RealGenericConsumer
// ---------------------------------------------------------------------------

/// A Kafka consumer wrapping [`rdkafka::consumer::StreamConsumer`].
///
/// `RealGenericConsumer` implements [`GenericConsumerTrait`] against a live
/// Kafka cluster. It handles transient connectivity errors internally
/// (returning a dummy-skip `Ok`) and performs explicit offset commits via
/// [`rdkafka::consumer::Consumer::commit`].
///
/// Construct with [`RealGenericConsumer::new`] if you already have a
/// configured and subscribed [`StreamConsumer`], or with
/// [`RealGenericConsumer::from_config`] to build one from a slice of Kafka
/// key-value settings and a topic name.
///
/// # Manual commit mode
///
/// [`from_config`](RealGenericConsumer::from_config) sets
/// `enable.auto.commit=false` so every offset advance requires an explicit
/// call to [`commit`](GenericConsumerTrait::commit).
pub struct RealGenericConsumer {
    inner: StreamConsumer,
    /// The most recently received message, stored for semantic tracking.
    pending_commit: Option<RawKafkaMessage>,
    /// The topic, partition, and offset of the most recently received message.
    commit_offset: Option<(String, i32, i64)>,
}

impl RealGenericConsumer {
    /// Wrap an already-configured and subscribed [`StreamConsumer`].
    ///
    /// # Arguments
    ///
    /// * `inner` - A configured and subscribed `StreamConsumer`
    ///
    /// # Returns
    ///
    /// A new `RealGenericConsumer` ready to yield messages.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use rdkafka::ClientConfig;
    /// use rdkafka::consumer::{Consumer, StreamConsumer};
    /// use xzatoma::watcher::generic::consumer::RealGenericConsumer;
    ///
    /// let inner: StreamConsumer = ClientConfig::new()
    ///     .set("bootstrap.servers", "localhost:9092")
    ///     .set("group.id", "my-group")
    ///     .set("enable.auto.commit", "false")
    ///     .create()
    ///     .unwrap();
    /// inner.subscribe(&["my-topic"]).unwrap();
    /// let _consumer = RealGenericConsumer::new(inner);
    /// ```
    pub fn new(inner: StreamConsumer) -> Self {
        Self {
            inner,
            pending_commit: None,
            commit_offset: None,
        }
    }

    /// Build a `RealGenericConsumer` from Kafka configuration key-value pairs.
    ///
    /// Applies `enable.auto.commit=false` to disable automatic offset
    /// management and then subscribes the consumer to `topic`.
    ///
    /// # Arguments
    ///
    /// * `kafka_settings` - Slice of `(key, value)` Kafka client config pairs
    /// * `topic`          - The Kafka topic to subscribe to
    ///
    /// # Returns
    ///
    /// A configured and subscribed `RealGenericConsumer`.
    ///
    /// # Errors
    ///
    /// Returns `Err(XzatomaError::Watcher(...))` if the `StreamConsumer`
    /// cannot be created or if subscription to the topic fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use xzatoma::watcher::generic::consumer::RealGenericConsumer;
    ///
    /// # fn example() -> xzatoma::error::Result<()> {
    /// let settings = vec![
    ///     ("bootstrap.servers".to_string(), "localhost:9092".to_string()),
    ///     ("group.id".to_string(), "my-group".to_string()),
    /// ];
    /// let consumer = RealGenericConsumer::from_config(&settings, "my-topic")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_config(kafka_settings: &[(String, String)], topic: &str) -> Result<Self> {
        let mut client_config = ClientConfig::new();
        for (key, value) in kafka_settings {
            client_config.set(key, value);
        }
        client_config.set("enable.auto.commit", "false");

        let inner: StreamConsumer = client_config.create().map_err(|e| {
            XzatomaError::Watcher(format!("Failed to create Kafka consumer: {}", e))
        })?;

        inner.subscribe(&[topic]).map_err(|e| {
            XzatomaError::Watcher(format!("Failed to subscribe to topic '{}': {}", topic, e))
        })?;

        Ok(Self::new(inner))
    }
}

impl std::fmt::Debug for RealGenericConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealGenericConsumer")
            .field("pending_commit", &self.pending_commit)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl GenericConsumerTrait for RealGenericConsumer {
    /// Yield the next raw message from the Kafka stream.
    ///
    /// On `Some(Ok(msg))`, stores the message's topic, partition, and offset
    /// for use by [`commit`](GenericConsumerTrait::commit).
    ///
    /// On `Some(Err(e))`, checks
    /// [`is_transient_kafka_recv_error`]: transient errors are logged at
    /// `warn` and converted to a dummy-skip `Some(Ok(...))` so the loop
    /// continues; fatal errors propagate as `Some(Err(...))`.
    ///
    /// Returns `None` when the stream ends.
    ///
    /// # Errors
    ///
    /// Returns `Some(Err(XzatomaError::Watcher(...)))` for fatal Kafka
    /// consumer errors.
    async fn next(&mut self) -> Option<Result<RawKafkaMessage>> {
        let msg_result = self.inner.stream().next().await;
        match msg_result {
            Some(Ok(borrowed_msg)) => {
                let payload = match borrowed_msg.payload_view::<str>() {
                    Some(Ok(s)) => s.to_string(),
                    Some(Err(_)) => {
                        return Some(Err(XzatomaError::Watcher(
                            "Failed to decode Kafka message payload as UTF-8".to_string(),
                        )));
                    }
                    None => String::new(),
                };
                let topic = borrowed_msg.topic().to_string();
                let key = borrowed_msg
                    .key_view::<str>()
                    .and_then(|r| r.ok())
                    .map(|s| s.to_string());
                let partition = borrowed_msg.partition();
                let offset = borrowed_msg.offset();
                let raw_msg = RawKafkaMessage {
                    payload,
                    topic: topic.clone(),
                    key,
                };
                self.pending_commit = Some(raw_msg.clone());
                self.commit_offset = Some((topic, partition, offset));
                Some(Ok(raw_msg))
            }
            Some(Err(e)) => {
                if is_transient_kafka_recv_error(&e) {
                    warn!(
                        error = %e,
                        "Transient Kafka consumer error; returning dummy skip to continue loop"
                    );
                    // Return a dummy skip message so the watcher loop continues.
                    // The empty payload will fail plan parsing and be classified
                    // as InvalidPayload without producing a result.
                    Some(Ok(RawKafkaMessage {
                        payload: String::new(),
                        topic: String::new(),
                        key: None,
                    }))
                } else {
                    Some(Err(XzatomaError::Watcher(format!(
                        "Fatal Kafka consumer error: {}",
                        e
                    ))))
                }
            }
            None => None,
        }
    }

    /// Commit the offset for the most recently received message.
    ///
    /// Uses [`rdkafka::consumer::Consumer::commit`] with an explicit
    /// [`TopicPartitionList`] to advance the committed offset by one past the
    /// last returned message. Clears `pending_commit` after a successful
    /// commit.
    ///
    /// This is a no-op when no message has been received since the last
    /// commit.
    ///
    /// # Errors
    ///
    /// Returns `Err(XzatomaError::Watcher(...))` if the broker commit call
    /// fails.
    async fn commit(&mut self) -> Result<()> {
        if let Some((topic, partition, offset)) = self.commit_offset.take() {
            let mut tpl = TopicPartitionList::new();
            tpl.add_partition_offset(&topic, partition, Offset::Offset(offset + 1))
                .map_err(|e| {
                    XzatomaError::Watcher(format!("Failed to build commit offset list: {}", e))
                })?;
            self.inner.commit(&tpl, CommitMode::Async).map_err(|e| {
                XzatomaError::Watcher(format!("Failed to commit Kafka offset: {}", e))
            })?;
            self.pending_commit = None;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FakeGenericConsumer
// ---------------------------------------------------------------------------

/// An in-memory consumer for driving the watcher loop in tests.
///
/// `FakeGenericConsumer` stores a pre-loaded queue of `Result<RawKafkaMessage>`
/// items that are returned in insertion order by
/// [`next`](GenericConsumerTrait::next). When the queue is exhausted `next`
/// returns `None`, causing the watcher loop to exit naturally.
///
/// Commit calls increment an atomic counter accessible via
/// [`commits_recorded`](FakeGenericConsumer::commits_recorded). Because the
/// counter is stored behind an [`Arc`], a clone of the counter handle can be
/// obtained via [`commit_counter`](FakeGenericConsumer::commit_counter) before
/// the consumer is moved into [`start`](crate::watcher::generic::GenericWatcher::start).
///
/// This type is intentionally `pub` (not `#[cfg(test)]`) so it can be used in
/// integration tests and external test harnesses.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, RawKafkaMessage};
/// use xzatoma::watcher::generic::consumer::GenericConsumerTrait;
///
/// # #[tokio::main]
/// # async fn main() {
/// let items = vec![
///     Ok(RawKafkaMessage { payload: "p1".to_string(), topic: "t".to_string(), key: None }),
///     Ok(RawKafkaMessage { payload: "p2".to_string(), topic: "t".to_string(), key: None }),
/// ];
/// let commit_handle = {
///     let c = FakeGenericConsumer::new(items);
///     c.commit_counter()
/// };
/// // ... the consumer can be moved into start() and the handle used afterward
/// drop(commit_handle);
/// # }
/// ```
pub struct FakeGenericConsumer {
    queue: VecDeque<Result<RawKafkaMessage>>,
    commit_count: Arc<AtomicUsize>,
}

impl std::fmt::Debug for FakeGenericConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeGenericConsumer")
            .field("queue_len", &self.queue.len())
            .field("commits", &self.commit_count.load(Ordering::SeqCst))
            .finish()
    }
}

impl FakeGenericConsumer {
    /// Create a new `FakeGenericConsumer` pre-loaded with the given items.
    ///
    /// Items are returned in insertion order by
    /// [`next`](GenericConsumerTrait::next).
    ///
    /// # Arguments
    ///
    /// * `items` - Items to enqueue; may be `Ok(msg)` or `Err(e)` values
    ///
    /// # Returns
    ///
    /// A new `FakeGenericConsumer` with all items enqueued.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::FakeGenericConsumer;
    ///
    /// let consumer = FakeGenericConsumer::new(vec![]);
    /// assert!(consumer.is_empty());
    /// ```
    pub fn new(items: Vec<Result<RawKafkaMessage>>) -> Self {
        Self {
            queue: VecDeque::from(items),
            commit_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Return the number of commits recorded so far.
    ///
    /// Each call to [`commit`](GenericConsumerTrait::commit) increments this
    /// counter by one.
    ///
    /// # Returns
    ///
    /// The number of commits recorded.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, GenericConsumerTrait};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// assert_eq!(consumer.commits_recorded(), 0);
    /// consumer.commit().await.unwrap();
    /// assert_eq!(consumer.commits_recorded(), 1);
    /// # }
    /// ```
    pub fn commits_recorded(&self) -> usize {
        self.commit_count.load(Ordering::SeqCst)
    }

    /// Return a shared handle to the commit counter.
    ///
    /// The returned [`Arc<AtomicUsize>`] shares the same counter as this
    /// consumer instance. This allows the commit count to be inspected after
    /// the consumer has been moved (e.g. into
    /// [`start`](crate::watcher::generic::GenericWatcher::start)).
    ///
    /// # Returns
    ///
    /// A cloned `Arc` pointing to the internal commit counter.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::Ordering;
    /// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, GenericConsumerTrait};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// let handle = consumer.commit_counter();
    /// consumer.commit().await.unwrap();
    /// assert_eq!(handle.load(Ordering::SeqCst), 1);
    /// # }
    /// ```
    pub fn commit_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.commit_count)
    }

    /// Return `true` when the message queue is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::FakeGenericConsumer;
    ///
    /// let consumer = FakeGenericConsumer::new(vec![]);
    /// assert!(consumer.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Return the number of items remaining in the message queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, RawKafkaMessage};
    ///
    /// let items = vec![
    ///     Ok(RawKafkaMessage { payload: "p".to_string(), topic: "t".to_string(), key: None }),
    /// ];
    /// let consumer = FakeGenericConsumer::new(items);
    /// assert_eq!(consumer.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

#[async_trait]
impl GenericConsumerTrait for FakeGenericConsumer {
    /// Pop and return the front item from the queue.
    ///
    /// Returns `None` when the queue is exhausted.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{
    ///     FakeGenericConsumer, GenericConsumerTrait, RawKafkaMessage,
    /// };
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// assert!(consumer.next().await.is_none());
    /// # }
    /// ```
    async fn next(&mut self) -> Option<Result<RawKafkaMessage>> {
        self.queue.pop_front()
    }

    /// Increment the commit counter by one.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::consumer::{FakeGenericConsumer, GenericConsumerTrait};
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut consumer = FakeGenericConsumer::new(vec![]);
    /// consumer.commit().await.unwrap();
    /// assert_eq!(consumer.commits_recorded(), 1);
    /// # }
    /// ```
    async fn commit(&mut self) -> Result<()> {
        self.commit_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::XzatomaError;

    fn make_ok_msg(payload: &str) -> Result<RawKafkaMessage> {
        Ok(RawKafkaMessage {
            payload: payload.to_string(),
            topic: "test.topic".to_string(),
            key: None,
        })
    }

    fn make_err_msg() -> Result<RawKafkaMessage> {
        Err(XzatomaError::Watcher("test error".to_string()))
    }

    // -----------------------------------------------------------------------
    // FakeGenericConsumer constructor / helper method tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_empty_consumer_is_empty() {
        let consumer = FakeGenericConsumer::new(vec![]);
        assert!(consumer.is_empty(), "empty consumer must report is_empty");
        assert_eq!(consumer.len(), 0, "empty consumer len must be 0");
    }

    #[test]
    fn test_new_consumer_with_messages_reports_len() {
        let items = vec![make_ok_msg("a"), make_ok_msg("b"), make_ok_msg("c")];
        let consumer = FakeGenericConsumer::new(items);
        assert!(!consumer.is_empty());
        assert_eq!(consumer.len(), 3);
    }

    // -----------------------------------------------------------------------
    // next() behaviour
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_next_returns_messages_in_order() {
        let items = vec![
            make_ok_msg("first"),
            make_ok_msg("second"),
            make_ok_msg("third"),
        ];
        let mut consumer = FakeGenericConsumer::new(items);

        let m1 = consumer.next().await.unwrap().unwrap();
        let m2 = consumer.next().await.unwrap().unwrap();
        let m3 = consumer.next().await.unwrap().unwrap();

        assert_eq!(m1.payload, "first");
        assert_eq!(m2.payload, "second");
        assert_eq!(m3.payload, "third");
    }

    #[tokio::test]
    async fn test_next_exhausted_returns_none() {
        let mut consumer = FakeGenericConsumer::new(vec![make_ok_msg("only")]);
        let _ = consumer.next().await;
        let result = consumer.next().await;
        assert!(result.is_none(), "exhausted consumer must return None");
    }

    #[tokio::test]
    async fn test_next_returns_error_items() {
        let items = vec![make_err_msg()];
        let mut consumer = FakeGenericConsumer::new(items);
        let result = consumer.next().await;
        assert!(
            matches!(result, Some(Err(_))),
            "Err items must be returned as Some(Err(...))"
        );
    }

    // -----------------------------------------------------------------------
    // commit() counter tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_commit_increments_counter() {
        let mut consumer = FakeGenericConsumer::new(vec![]);
        consumer.commit().await.unwrap();
        assert_eq!(consumer.commits_recorded(), 1);
    }

    #[tokio::test]
    async fn test_commit_increments_each_call() {
        let mut consumer = FakeGenericConsumer::new(vec![]);
        consumer.commit().await.unwrap();
        consumer.commit().await.unwrap();
        consumer.commit().await.unwrap();
        assert_eq!(consumer.commits_recorded(), 3);
    }

    // -----------------------------------------------------------------------
    // Trait object dispatch
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_trait_object_dispatch() {
        let items = vec![make_ok_msg("via-dyn")];
        let mut consumer = FakeGenericConsumer::new(items);
        let consumer_ref: &mut dyn GenericConsumerTrait = &mut consumer;

        let result = consumer_ref.next().await;
        assert!(matches!(result, Some(Ok(_))));
        consumer_ref.commit().await.unwrap();

        assert_eq!(consumer.commits_recorded(), 1);
    }

    // -----------------------------------------------------------------------
    // is_transient_kafka_recv_error tests
    // -----------------------------------------------------------------------

    fn make_kafka_error(msg: &str) -> KafkaError {
        // KafkaError::ClientCreation carries an arbitrary string description
        // and exists in rdkafka 0.36. Its Display output is
        // "KafkaError (Client creation error: <msg>)", so the transient
        // substrings (BrokerTransportFailure, AllBrokersDown, NetworkException)
        // remain detectable by is_transient_kafka_recv_error.
        KafkaError::ClientCreation(msg.to_string())
    }

    #[test]
    fn test_is_transient_broker_transport_failure() {
        let err = make_kafka_error("BrokerTransportFailure: connection refused");
        assert!(
            is_transient_kafka_recv_error(&err),
            "BrokerTransportFailure must be transient"
        );
    }

    #[test]
    fn test_is_transient_all_brokers_down() {
        let err = make_kafka_error("AllBrokersDown");
        assert!(
            is_transient_kafka_recv_error(&err),
            "AllBrokersDown must be transient"
        );
    }

    #[test]
    fn test_is_transient_network_exception() {
        let err = make_kafka_error("NetworkException: timeout");
        assert!(
            is_transient_kafka_recv_error(&err),
            "NetworkException must be transient"
        );
    }

    #[test]
    fn test_is_not_transient_unknown_topic() {
        let err = make_kafka_error("UnknownTopicOrPartition");
        assert!(
            !is_transient_kafka_recv_error(&err),
            "UnknownTopicOrPartition must not be transient"
        );
    }

    #[test]
    fn test_is_not_transient_non_message_error() {
        let err = make_kafka_error("OffsetOutOfRange");
        assert!(
            !is_transient_kafka_recv_error(&err),
            "OffsetOutOfRange must not be transient"
        );
    }
}
