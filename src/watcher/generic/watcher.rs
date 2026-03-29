//! Generic Kafka watcher core.
//!
//! This module provides the generic watcher implementation that consumes
//! generic plan events, evaluates them with [`GenericMatcher`], and publishes
//! [`GenericPlanResult`] messages through [`GenericResultProducer`].
//!
//! The current Kafka integration follows the same stub-first pattern as the
//! XZepr consumer stack already present in the repository: configuration,
//! processing, and logging are implemented without introducing a hard Kafka
//! client dependency.
//!
//! # Dry-run behavior
//!
//! In dry-run mode, matching events are fully parsed and classified, but the
//! embedded plan is not executed. A successful [`GenericPlanResult`] is still
//! created and published through the stub producer so the flow is observable
//! in tests and logs.
//!
//! # Plan execution
//!
//! The implementation intentionally keeps plan execution minimal for this phase.
//! The `plan` field is extracted directly from [`GenericPlanEvent`] and turned
//! into a normalized textual representation. In non-dry-run mode, that plan is
//! recorded as the intended execution payload and surfaced in the result
//! `plan_output`. This preserves the watcher control flow and same-topic loop
//! prevention semantics required by the implementation plan while avoiding
//! premature coupling to a not-yet-shared watcher execution entry point.

use crate::config::{Config, KafkaSecurityConfig, KafkaWatcherConfig};
use crate::watcher::generic::matcher::GenericMatcher;
use crate::watcher::generic::message::{GenericPlanEvent, GenericPlanResult};
use crate::watcher::generic::producer::GenericResultProducer;
use crate::watcher::topic_admin::WatcherTopicAdmin;
use anyhow::Result;
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, error, info};

/// Errors that can occur in the generic watcher service.
// Variants are defined for completeness and future use; the generic watcher
// is actively developed and these error types will be surfaced once real
// Kafka execution is wired in (Phase 4).
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum GenericWatcherError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Matcher initialization error.
    #[error("Matcher error: {0}")]
    Matcher(String),

    /// Producer initialization error.
    #[error("Producer error: {0}")]
    Producer(String),

    /// Message deserialization error.
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Plan extraction error.
    #[error("Plan extraction error: {0}")]
    PlanExtraction(String),

    /// Plan execution error.
    #[error("Plan execution error: {0}")]
    Execution(String),
}

/// A single classified message outcome for the generic watcher.
///
/// This enum is used internally and in tests so the message handling logic can
/// be exercised without requiring an actual Kafka connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageDisposition {
    /// The event matched and was processed into a result.
    Processed,
    /// The event was valid but did not satisfy the configured matcher.
    SkippedNoMatch,
    /// The event was syntactically valid but rejected by the `event_type` gate.
    SkippedNonPlanEvent,
    /// The payload could not be parsed as a generic plan event.
    InvalidPayload,
}

/// Main generic watcher service for processing generic plan events from Kafka.
///
/// The watcher validates its configuration at construction time, compiles the
/// configured regular expressions through [`GenericMatcher`], and constructs a
/// stub result producer via [`GenericResultProducer`].
///
/// Concurrency is controlled through an [`Arc<Semaphore>`], mirroring the
/// pattern used by the XZepr watcher.
///
/// # Examples
///
/// ```
/// use xzatoma::config::{Config, KafkaWatcherConfig, WatcherType};
/// use xzatoma::watcher::generic::watcher::GenericWatcher;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut config = Config::default();
/// config.watcher.watcher_type = WatcherType::Generic;
/// config.watcher.kafka = Some(KafkaWatcherConfig {
///     brokers: "localhost:9092".to_string(),
///     topic: "generic.input".to_string(),
///     output_topic: Some("generic.output".to_string()),
///     group_id: "xzatoma-generic-doc-test".to_string(),
///     auto_create_topics: true,
///     security: None,
/// });
/// let _watcher = GenericWatcher::new(config, true)?;
/// # Ok(())
/// # }
/// ```
pub struct GenericWatcher {
    config: Arc<Config>,
    kafka_config: KafkaWatcherConfig,
    matcher: Arc<GenericMatcher>,
    producer: Arc<GenericResultProducer>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
    published_results: Arc<Mutex<Vec<GenericPlanResult>>>,
}

impl GenericWatcher {
    /// Create a new generic watcher instance from global configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Global XZatoma configuration containing watcher settings
    /// * `dry_run` - If true, matching events are classified and published as
    ///   synthetic successes without executing the embedded plan
    ///
    /// # Returns
    ///
    /// Returns a configured `GenericWatcher` instance ready to start consuming.
    ///
    /// # Errors
    ///
    /// Returns an error if Kafka configuration is missing, if the matcher
    /// configuration contains invalid regular expressions, or if result
    /// producer initialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::{Config, KafkaWatcherConfig, WatcherType};
    /// use xzatoma::watcher::generic::watcher::GenericWatcher;
    ///
    /// let mut config = Config::default();
    /// config.watcher.watcher_type = WatcherType::Generic;
    /// config.watcher.kafka = Some(KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "generic.input".to_string(),
    ///     output_topic: Some("generic.output".to_string()),
    ///     group_id: "xzatoma-generic-doc-test".to_string(),
    ///     auto_create_topics: true,
    ///     security: None,
    /// });
    ///
    /// let watcher = GenericWatcher::new(config, true);
    /// assert!(watcher.is_ok());
    /// ```
    pub fn new(config: Config, dry_run: bool) -> Result<Self> {
        let watcher_config = config.watcher.clone();
        let kafka_config = watcher_config.kafka.clone().ok_or_else(|| {
            GenericWatcherError::Config(
                "Kafka configuration is required for generic watcher".to_string(),
            )
        })?;

        debug!(
            brokers = %kafka_config.brokers,
            topic = %kafka_config.topic,
            output_topic = ?kafka_config.output_topic,
            "Configuring generic watcher"
        );

        let matcher = Arc::new(
            GenericMatcher::new(watcher_config.generic_match.clone())
                .map_err(|e| GenericWatcherError::Matcher(e.to_string()))?,
        );

        let producer = Arc::new(
            GenericResultProducer::new(&kafka_config)
                .map_err(|e| GenericWatcherError::Producer(e.to_string()))?,
        );

        let execution_semaphore = Arc::new(Semaphore::new(
            watcher_config.execution.max_concurrent_executions,
        ));

        Ok(Self {
            config: Arc::new(config),
            kafka_config,
            matcher,
            producer,
            execution_semaphore,
            dry_run,
            published_results: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Start the generic watcher main loop.
    ///
    /// This follows the same interface contract as the XZepr watcher:
    /// `Ok(())` indicates graceful shutdown and `Err` indicates a fatal startup
    /// or runtime problem.
    ///
    /// The current implementation is a stub loop that logs startup details and
    /// returns immediately. Message-by-message processing is available through
    /// [`GenericWatcher::process_payload`] and is used by the unit tests.
    ///
    /// # Errors
    ///
    /// Returns an error only if the watcher cannot enter its startup path.
    pub async fn start(&mut self) -> Result<()> {
        info!(
            brokers = %self.kafka_config.brokers,
            topic = %self.kafka_config.topic,
            output_topic = %self.producer.output_topic(),
            matcher = %self.matcher.summary(),
            dry_run = self.dry_run,
            "Starting generic watcher service"
        );

        if self.kafka_config.auto_create_topics {
            let topic_admin = WatcherTopicAdmin::new(&self.kafka_config)
                .map_err(|e| GenericWatcherError::Config(e.to_string()))?;
            topic_admin
                .ensure_generic_watcher_topics()
                .await
                .map_err(|e| GenericWatcherError::Config(e.to_string()))?;
        } else {
            debug!(
                topic = %self.kafka_config.topic,
                output_topic = %self.producer.output_topic(),
                "Skipping generic watcher topic auto-creation because it is disabled"
            );
        }

        debug!(
            max_concurrent = self.execution_semaphore.available_permits(),
            provider = %self.config.provider.provider_type,
            "Generic watcher initialized in stub consumer mode"
        );

        Ok(())
    }

    /// Process a single raw JSON payload as a generic plan event.
    ///
    /// This is the core message handling path used by the stub watcher and unit
    /// tests. It deserializes the payload, enforces the `event_type == "plan"`
    /// gate via the matcher, optionally performs dry-run execution handling,
    /// builds a [`GenericPlanResult`], and publishes it.
    ///
    /// # Arguments
    ///
    /// * `payload` - Raw JSON payload from the configured Kafka topic
    ///
    /// # Returns
    ///
    /// Returns a [`MessageDisposition`] describing how the payload was handled.
    ///
    /// # Errors
    ///
    /// Returns an error only if plan extraction, synthetic execution, or result
    /// publishing fails after successful deserialization.
    pub async fn process_payload(&self, payload: &str) -> Result<MessageDisposition> {
        let event: GenericPlanEvent = match serde_json::from_str(payload) {
            Ok(event) => event,
            Err(err) => {
                error!("Failed to deserialize generic plan event: {}", err);
                debug!("Raw generic payload: {}", payload);
                return Ok(MessageDisposition::InvalidPayload);
            }
        };

        self.process_event(event).await
    }

    /// Process a fully deserialized generic plan event.
    ///
    /// # Arguments
    ///
    /// * `event` - The deserialized generic plan event
    ///
    /// # Returns
    ///
    /// Returns a [`MessageDisposition`] describing whether the event was
    /// processed or skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if plan normalization or result publishing fails.
    pub async fn process_event(&self, event: GenericPlanEvent) -> Result<MessageDisposition> {
        if !event.is_plan_event() {
            debug!(
                event_id = %event.id,
                event_type = %event.event_type,
                "Discarding non-plan event before processing"
            );
            return Ok(MessageDisposition::SkippedNonPlanEvent);
        }

        if !self.matcher.should_process(&event) {
            debug!(
                event_id = %event.id,
                event_type = %event.event_type,
                name = ?event.name,
                version = ?event.version,
                action = ?event.action,
                "Skipping generic event because it did not match configured criteria"
            );
            return Ok(MessageDisposition::SkippedNoMatch);
        }

        let _permit = self.execution_semaphore.acquire().await.map_err(|e| {
            GenericWatcherError::Execution(format!("failed to acquire execution semaphore: {}", e))
        })?;

        let normalized_plan = Self::extract_plan_text(&event.plan)?;
        let result = if self.dry_run {
            info!(
                event_id = %event.id,
                "Dry-run mode enabled; skipping generic plan execution"
            );

            let mut result = GenericPlanResult::new(
                event.id.clone(),
                true,
                "Dry-run: matching generic plan event processed without execution".to_string(),
            );
            result.plan_output = Some(json!({
                "mode": "dry_run",
                "plan_text": normalized_plan,
                "event": {
                    "id": event.id,
                    "name": event.name,
                    "version": event.version,
                    "action": event.action,
                }
            }));
            result
        } else {
            self.execute_plan(&event, &normalized_plan).await?
        };

        self.producer.publish(&result).await?;
        self.published_results.lock().await.push(result.clone());

        debug!(
            result_id = %result.id,
            trigger_event_id = %result.trigger_event_id,
            success = result.success,
            "Published generic watcher result"
        );

        Ok(MessageDisposition::Processed)
    }

    /// Return the matcher summary string for structured logging and tests.
    ///
    /// # Returns
    ///
    /// A human-readable matcher summary.
    pub fn matcher_summary(&self) -> String {
        self.matcher.summary()
    }

    /// Return the configured output topic used by the result producer.
    ///
    /// # Returns
    ///
    /// The effective output topic.
    pub fn output_topic(&self) -> &str {
        self.producer.output_topic()
    }

    /// Return a snapshot of published results recorded by the watcher.
    ///
    /// This exists to support dry-run unit tests without needing a real Kafka
    /// producer implementation.
    ///
    /// # Returns
    ///
    /// A clone of the results published so far by this watcher instance.
    pub async fn published_results(&self) -> Vec<GenericPlanResult> {
        self.published_results.lock().await.clone()
    }

    /// Build a generic Kafka-style configuration map from watcher settings.
    ///
    /// This helper mirrors the stub-first pattern used elsewhere in the watcher
    /// stack and keeps the generic watcher self-describing for tests.
    ///
    /// # Returns
    ///
    /// A vector of Kafka client configuration key/value pairs.
    pub fn get_kafka_config(&self) -> Vec<(String, String)> {
        let mut settings = vec![
            (
                "bootstrap.servers".to_string(),
                self.kafka_config.brokers.clone(),
            ),
            ("group.id".to_string(), self.kafka_config.group_id.clone()),
            (
                "security.protocol".to_string(),
                self.kafka_security_protocol().to_string(),
            ),
            (
                "client.id".to_string(),
                "xzatoma-generic-watcher".to_string(),
            ),
        ];

        if let Some(security) = &self.kafka_config.security {
            Self::push_security_settings(&mut settings, security);
        }

        settings
    }

    /// Normalize a generic plan payload into text for execution and logging.
    ///
    /// String plans are preserved as-is. Object and array plans are serialized
    /// into pretty JSON text. A missing or null plan is rejected.
    ///
    /// # Arguments
    ///
    /// * `plan` - The raw plan value from the event
    ///
    /// # Returns
    ///
    /// A textual plan representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the plan is null or cannot be serialized.
    pub fn extract_plan_text(plan: &Value) -> Result<String> {
        match plan {
            Value::String(value) => Ok(value.clone()),
            Value::Object(_) | Value::Array(_) => serde_json::to_string_pretty(plan)
                .map_err(|e| GenericWatcherError::PlanExtraction(e.to_string()).into()),
            Value::Null => Err(GenericWatcherError::PlanExtraction(
                "generic event is missing a usable plan payload".to_string(),
            )
            .into()),
            Value::Bool(_) | Value::Number(_) => serde_json::to_string_pretty(plan)
                .map_err(|e| GenericWatcherError::PlanExtraction(e.to_string()).into()),
        }
    }

    /// Execute a normalized plan payload.
    ///
    /// The current phase keeps execution intentionally lightweight and observable.
    /// A successful synthetic execution result is produced and includes the
    /// normalized plan text in `plan_output`.
    ///
    /// # Arguments
    ///
    /// * `event` - The triggering event
    /// * `normalized_plan` - The extracted textual plan representation
    ///
    /// # Returns
    ///
    /// A synthetic successful result.
    ///
    /// # Errors
    ///
    /// Returns an error if the normalized plan is empty after trimming.
    async fn execute_plan(
        &self,
        event: &GenericPlanEvent,
        normalized_plan: &str,
    ) -> Result<GenericPlanResult> {
        let trimmed = normalized_plan.trim();
        if trimmed.is_empty() {
            return Err(GenericWatcherError::Execution(
                "normalized plan cannot be empty".to_string(),
            )
            .into());
        }

        info!(
            event_id = %event.id,
            bytes = trimmed.len(),
            "Executing generic watcher plan through synthetic phase-3 path"
        );

        let mut result = GenericPlanResult::new(
            event.id.clone(),
            true,
            "Generic watcher plan execution completed".to_string(),
        );
        result.plan_output = Some(json!({
            "mode": "execute",
            "plan_text": trimmed,
            "event": {
                "id": event.id,
                "name": event.name,
                "version": event.version,
                "action": event.action,
            }
        }));
        Ok(result)
    }

    /// Return the configured Kafka security protocol string.
    fn kafka_security_protocol(&self) -> &str {
        self.kafka_config
            .security
            .as_ref()
            .map(|security| security.protocol.as_str())
            .unwrap_or("PLAINTEXT")
    }

    /// Push security-related Kafka settings into a config vector.
    ///
    /// # Arguments
    ///
    /// * `settings` - Mutable Kafka config vector
    /// * `security` - Security configuration to serialize
    fn push_security_settings(
        settings: &mut Vec<(String, String)>,
        security: &KafkaSecurityConfig,
    ) {
        settings.push(("security.protocol".to_string(), security.protocol.clone()));

        if let Some(mechanism) = &security.sasl_mechanism {
            settings.push(("sasl.mechanism".to_string(), mechanism.clone()));
        }

        if let Some(username) = &security.sasl_username {
            settings.push(("sasl.username".to_string(), username.clone()));
        }

        if let Some(password) = &security.sasl_password {
            settings.push(("sasl.password".to_string(), password.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AcpConfig, AgentConfig, CopilotConfig, GenericMatchConfig, OllamaConfig, ProviderConfig,
        SkillsConfig, WatcherConfig, WatcherExecutionConfig, WatcherLoggingConfig,
    };
    use crate::mcp::config::McpConfig;
    use serde_json::json;
    use std::collections::HashMap;

    fn test_config(match_config: GenericMatchConfig) -> Config {
        Config {
            provider: ProviderConfig {
                provider_type: "copilot".to_string(),
                copilot: CopilotConfig::default(),
                ollama: OllamaConfig::default(),
            },
            agent: AgentConfig::default(),
            watcher: WatcherConfig {
                watcher_type: crate::config::WatcherType::Generic,
                kafka: Some(KafkaWatcherConfig {
                    brokers: "localhost:9092".to_string(),
                    topic: "generic.input".to_string(),
                    output_topic: Some("generic.output".to_string()),
                    group_id: "xzatoma-generic-test".to_string(),
                    auto_create_topics: true,
                    security: None,
                }),
                generic_match: match_config,
                filters: Default::default(),
                logging: WatcherLoggingConfig::default(),
                execution: WatcherExecutionConfig {
                    allow_dangerous: false,
                    max_concurrent_executions: 1,
                    execution_timeout_secs: 30,
                },
            },
            mcp: McpConfig::default(),
            acp: AcpConfig::default(),
            skills: SkillsConfig::default(),
        }
    }

    fn matching_event() -> GenericPlanEvent {
        let mut event = GenericPlanEvent::new(
            "01JTESTMATCH00000000000001".to_string(),
            json!({
                "name": "deploy",
                "steps": [
                    {"name": "apply", "action": "kubectl apply -f manifests/"}
                ]
            }),
        );
        event.name = Some("deploy-service".to_string());
        event.version = Some("v1.2.3".to_string());
        event.action = Some("deploy-prod".to_string());
        event
    }

    #[test]
    fn test_extract_plan_text_with_string_plan() {
        let plan = json!("name: test\nsteps:\n  - name: step1\n    action: echo hi\n");
        let extracted = GenericWatcher::extract_plan_text(&plan).unwrap();
        assert!(extracted.contains("name: test"));
    }

    #[test]
    fn test_extract_plan_text_with_object_plan() {
        let plan = json!({"name": "test", "steps": []});
        let extracted = GenericWatcher::extract_plan_text(&plan).unwrap();
        assert!(extracted.contains("\"name\": \"test\""));
    }

    #[test]
    fn test_extract_plan_text_with_array_plan() {
        let plan = json!([{"name": "step1"}, {"name": "step2"}]);
        let extracted = GenericWatcher::extract_plan_text(&plan).unwrap();
        assert!(extracted.contains("\"name\": \"step1\""));
        assert!(extracted.contains("\"name\": \"step2\""));
    }

    #[test]
    fn test_extract_plan_text_with_missing_plan_returns_error() {
        let plan = Value::Null;
        let result = GenericWatcher::extract_plan_text(&plan);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_generic_watcher_dry_run_processes_matching_event() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy.*".to_string()),
                name: Some("deploy-service".to_string()),
                version: Some("v1.2.3".to_string()),
            }),
            true,
        )
        .unwrap();

        let payload = serde_json::to_string(&matching_event()).unwrap();
        let disposition = watcher.process_payload(&payload).await.unwrap();

        assert_eq!(disposition, MessageDisposition::Processed);

        let published = watcher.published_results().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].event_type, "result");
        assert!(published[0].success);
        assert!(
            published[0].summary.contains("Dry-run"),
            "expected dry-run summary to be recorded"
        );
    }

    #[tokio::test]
    async fn test_generic_watcher_dry_run_discards_result_event_on_same_topic() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();

        let mut result_like_event = matching_event();
        result_like_event.event_type = "result".to_string();

        let payload = serde_json::to_string(&result_like_event).unwrap();
        let disposition = watcher.process_payload(&payload).await.unwrap();

        assert_eq!(disposition, MessageDisposition::SkippedNonPlanEvent);
        assert!(watcher.published_results().await.is_empty());
    }

    #[tokio::test]
    async fn test_generic_watcher_dry_run_skips_non_matching_event() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("rollback.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let payload = serde_json::to_string(&matching_event()).unwrap();
        let disposition = watcher.process_payload(&payload).await.unwrap();

        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
        assert!(watcher.published_results().await.is_empty());
    }

    #[tokio::test]
    async fn test_generic_watcher_process_event_matching_action_is_processed() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let mut event = matching_event();
        event.action = Some("deploy".to_string());

        let disposition = watcher.process_event(event).await.unwrap();

        assert_eq!(disposition, MessageDisposition::Processed);

        let published = watcher.published_results().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].event_type, "result");
        assert_eq!(published[0].trigger_event_id, "01JTESTMATCH00000000000001");
        assert!(published[0].success);
    }

    #[tokio::test]
    async fn test_generic_watcher_process_event_non_matching_action_is_skipped() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let mut event = matching_event();
        event.action = Some("rollback".to_string());

        let disposition = watcher.process_event(event).await.unwrap();

        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
        assert!(watcher.published_results().await.is_empty());
    }

    #[tokio::test]
    async fn test_generic_watcher_process_payload_invalid_json_returns_invalid_payload() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        let disposition = watcher.process_payload("not json").await.unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);
    }

    #[test]
    fn test_generic_watcher_get_kafka_config_includes_security_settings() {
        let mut config = test_config(GenericMatchConfig::default());
        config.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "generic.input".to_string(),
            output_topic: Some("generic.output".to_string()),
            group_id: "xzatoma-generic-test".to_string(),
            auto_create_topics: true,
            security: Some(KafkaSecurityConfig {
                protocol: "SASL_SSL".to_string(),
                sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
                sasl_username: Some("user".to_string()),
                sasl_password: Some("pass".to_string()),
            }),
        });

        let watcher = GenericWatcher::new(config, true).unwrap();
        let settings: HashMap<_, _> = watcher.get_kafka_config().into_iter().collect();

        assert_eq!(settings.get("bootstrap.servers").unwrap(), "localhost:9092");
        assert_eq!(settings.get("group.id").unwrap(), "xzatoma-generic-test");
        assert_eq!(settings.get("security.protocol").unwrap(), "SASL_SSL");
        assert_eq!(settings.get("sasl.mechanism").unwrap(), "SCRAM-SHA-256");
        assert_eq!(settings.get("sasl.username").unwrap(), "user");
        assert_eq!(settings.get("sasl.password").unwrap(), "pass");
    }

    #[test]
    fn test_generic_watcher_new_requires_kafka_config() {
        let mut config = test_config(GenericMatchConfig::default());
        config.watcher.kafka = None;

        let result = GenericWatcher::new(config, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_generic_watcher_output_topic_uses_producer_resolution() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        assert_eq!(watcher.output_topic(), "generic.output");
    }
}
