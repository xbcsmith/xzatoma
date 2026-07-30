//! Shared watcher lifecycle helpers.
//!
//! Both watcher backends (`xzepr` and `generic`) share a small amount of
//! startup boilerplate: resolving the effective output topic, constructing the
//! execution semaphore that bounds concurrent plan executions, and building the
//! result producer (a live Kafka producer, or an in-memory fake in dry-run
//! mode).
//!
//! These helpers centralize that boilerplate so both backends stay in sync.

use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::config::KafkaWatcherConfig;
use crate::error::Result;
use crate::watcher::generic::result_producer::{
    FakeResultProducer, GenericResultProducer, ResultProducerTrait,
};

/// Resolve the effective output topic for a watcher.
///
/// Returns the explicit `output_topic` when it is set, otherwise falls back to
/// the input `topic`. Both watcher backends publish results to this topic.
///
/// The returned reference borrows from `config`, so callers that need an owned
/// value should call `.to_string()` on the result.
///
/// # Arguments
///
/// * `config` - The watcher Kafka configuration.
///
/// # Returns
///
/// The effective output topic, borrowed from `config`.
///
/// # Examples
///
/// ```
/// use xzatoma::config::KafkaWatcherConfig;
/// use xzatoma::watcher::lifecycle::resolve_output_topic;
///
/// let explicit = KafkaWatcherConfig {
///     topic: "plans.in".to_string(),
///     output_topic: Some("plans.out".to_string()),
///     ..Default::default()
/// };
/// assert_eq!(resolve_output_topic(&explicit), "plans.out");
///
/// let fallback = KafkaWatcherConfig {
///     topic: "plans.in".to_string(),
///     output_topic: None,
///     ..Default::default()
/// };
/// assert_eq!(resolve_output_topic(&fallback), "plans.in");
/// ```
pub fn resolve_output_topic(config: &KafkaWatcherConfig) -> &str {
    config
        .output_topic
        .as_deref()
        .unwrap_or(config.topic.as_str())
}

/// Build the execution semaphore that bounds concurrent plan executions.
///
/// Both watcher backends wrap a [`tokio::sync::Semaphore`] in an [`Arc`] so it
/// can be shared across spawned execution tasks.
///
/// # Arguments
///
/// * `max_concurrent` - The maximum number of concurrent plan executions.
///
/// # Returns
///
/// A shared semaphore with `max_concurrent` permits.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::lifecycle::build_execution_semaphore;
///
/// let semaphore = build_execution_semaphore(4);
/// assert_eq!(semaphore.available_permits(), 4);
/// ```
pub fn build_execution_semaphore(max_concurrent: usize) -> Arc<Semaphore> {
    Arc::new(Semaphore::new(max_concurrent))
}

/// Build the result producer used by a watcher backend.
///
/// In dry-run mode a [`FakeResultProducer`] is returned so the watcher loop can
/// run without a live Kafka broker. Otherwise a live [`GenericResultProducer`]
/// is constructed from `kafka_config`.
///
/// # Arguments
///
/// * `kafka_config` - The watcher Kafka configuration.
/// * `dry_run` - When `true`, return an in-memory fake producer.
///
/// # Returns
///
/// A shared [`ResultProducerTrait`] implementation.
///
/// # Errors
///
/// Returns an error if the live [`GenericResultProducer`] cannot be
/// constructed (invalid security settings or a broker client failure). The
/// dry-run path never fails.
///
/// # Examples
///
/// ```
/// use xzatoma::config::KafkaWatcherConfig;
/// use xzatoma::watcher::lifecycle::build_producer;
///
/// let config = KafkaWatcherConfig {
///     topic: "plans.input".to_string(),
///     ..Default::default()
/// };
///
/// // Dry-run mode returns an in-memory fake producer.
/// let producer = build_producer(&config, true).unwrap();
/// let _ = producer;
/// ```
pub fn build_producer(
    kafka_config: &KafkaWatcherConfig,
    dry_run: bool,
) -> Result<Arc<dyn ResultProducerTrait>> {
    if dry_run {
        Ok(Arc::new(FakeResultProducer::new()))
    } else {
        Ok(Arc::new(GenericResultProducer::new(kafka_config)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> KafkaWatcherConfig {
        KafkaWatcherConfig {
            topic: "plans.input".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_output_topic_uses_explicit_output_topic() {
        let mut config = base_config();
        config.output_topic = Some("plans.output".to_string());
        assert_eq!(resolve_output_topic(&config), "plans.output");
    }

    #[test]
    fn test_resolve_output_topic_falls_back_to_input_topic() {
        let config = base_config();
        assert_eq!(resolve_output_topic(&config), "plans.input");
    }

    #[test]
    fn test_build_execution_semaphore_sets_permits() {
        let semaphore = build_execution_semaphore(3);
        assert_eq!(semaphore.available_permits(), 3);
    }

    #[tokio::test]
    async fn test_build_producer_dry_run_returns_fake() {
        let config = base_config();
        let producer = build_producer(&config, true).unwrap();

        let result = crate::watcher::generic::result_event::GenericPlanResult::new(
            "trigger-1".to_string(),
            true,
            "ok".to_string(),
        );
        producer.publish(&result).await.unwrap();
    }
}
