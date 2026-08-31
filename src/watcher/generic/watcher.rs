//! Generic Kafka watcher core.
//!
//! This module provides the generic watcher implementation that consumes
//! generic plan events, evaluates them with [`GenericEventHandler`], and
//! publishes [`GenericPlanResult`] messages through [`ResultProducerTrait`].
//!
//! The watcher uses [`GenericConsumerTrait`] to receive messages and enters a
//! consume loop that dispatches each message through
//! [`GenericWatcher::process_event`]. In production a
//! [`RealGenericConsumer`] wraps the rdkafka `StreamConsumer`; tests inject a
//! [`crate::watcher::generic::consumer::FakeGenericConsumer`] via
//! [`GenericWatcher::start`]'s optional consumer parameter.
//!
//! # Dry-run behavior
//!
//! In dry-run mode, matching events are fully parsed and classified, but the
//! embedded plan is not executed. A successful [`GenericPlanResult`] is still
//! created and published through the producer so the flow is observable in
//! tests and logs.
//!
//! # Plan execution
//!
//! In non-dry-run mode, the instruction derived from the resolved plan is
//! executed by a [`crate::agent::Agent`] constructed in
//! [`crate::chat_mode::ChatMode::Watcher`] (autonomous) mode. The watcher
//! system prompt instructs the LLM to call tools immediately without asking
//! for confirmation.

use crate::config::{Config, KafkaSecurityConfig, KafkaWatcherConfig, WatcherPlanExecutionMode};
use crate::error::Result;
use crate::watcher::BackOffPolicy;
use crate::watcher::generic::consumer::{
    GenericConsumerTrait, RawKafkaMessage, RealGenericConsumer,
};
use crate::watcher::generic::event_handler::{GenericEventHandler, GenericTask};
use crate::watcher::generic::matcher::GenericMatcher;
use crate::watcher::generic::result_event::GenericPlanResult;
use crate::watcher::generic::result_producer::ResultProducerTrait;
use crate::watcher::plan_executor::execute_tasks_sequentially;

use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, error, info, trace, warn};
use ulid::Ulid;

/// Errors that can occur in the generic watcher service.
#[derive(Error, Debug)]
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
    /// Reserved for backward compatibility; no longer returned by the current
    /// pipeline (result-event JSON now fails plan parsing and is classified
    /// as `InvalidPayload`).
    SkippedNonPlanEvent,
    /// The payload could not be parsed as a valid plan event.
    InvalidPayload,
}

/// Main generic watcher service for processing generic plan events from Kafka.
///
/// The watcher validates its configuration at construction time, compiles the
/// configured regular expressions through [`GenericMatcher`], wraps the matcher
/// in a [`GenericEventHandler`], and constructs a result producer via
/// [`GenericResultProducer`].
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
/// # fn example() -> anyhow::Result<()> {
/// let mut config = Config::default();
/// config.watcher.watcher_type = WatcherType::Generic;
/// config.watcher.kafka = Some(KafkaWatcherConfig {
///     brokers: "localhost:9092".to_string(),
///     topic: "generic.input".to_string(),
///     output_topic: Some("generic.output".to_string()),
///     group_id: "xzatoma-generic-doc-test".to_string(),
///     auto_create_topics: true,
///     security: None,
///     num_partitions: 1,
///     replication_factor: 1,
///     broker_address_family: "v4".to_string(),
///     poll_interval_ms: 1000,
///     ..KafkaWatcherConfig::default()
/// });
/// let _watcher = GenericWatcher::new(config, true)?;
/// # Ok(())
/// # }
/// ```
pub struct GenericWatcher {
    config: Arc<Config>,
    kafka_config: KafkaWatcherConfig,
    event_handler: GenericEventHandler,
    producer: Arc<dyn ResultProducerTrait>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
    published_results: Arc<Mutex<Vec<GenericPlanResult>>>,
    running: Arc<AtomicBool>,
}

impl GenericWatcher {
    /// Create a new generic watcher instance from global configuration.
    ///
    /// # Arguments
    ///
    /// * `config`   - Global XZatoma configuration containing watcher settings
    /// * `dry_run`  - If true, matching events are classified and published as
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
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     broker_address_family: "v4".to_string(),
    ///     poll_interval_ms: 1000,
    ///     ..KafkaWatcherConfig::default()
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

        let matcher = GenericMatcher::new(watcher_config.generic_match.clone())
            .map_err(|e| GenericWatcherError::Matcher(e.to_string()))?;

        let event_handler = GenericEventHandler::new(Some(matcher), None)
            .with_max_payload_bytes(watcher_config.execution.max_payload_bytes);

        let producer: Arc<dyn ResultProducerTrait> =
            crate::watcher::lifecycle::build_producer(&kafka_config, dry_run)
                .map_err(|e| GenericWatcherError::Producer(e.to_string()))?;

        let execution_semaphore = crate::watcher::lifecycle::build_execution_semaphore(
            watcher_config.execution.max_concurrent_executions,
        );

        Ok(Self {
            config: Arc::new(config),
            kafka_config,
            event_handler,
            producer,
            execution_semaphore,
            dry_run,
            published_results: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Replace the result producer with the provided implementation.
    ///
    /// This builder method enables injection of test doubles such as
    /// [`crate::watcher::generic::result_producer::FakeResultProducer`]
    /// so the watcher loop can be exercised without a live Kafka broker.
    ///
    /// # Arguments
    ///
    /// * `producer` - The producer implementation to use
    ///
    /// # Returns
    ///
    /// `self` with the producer replaced.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzatoma::config::{Config, KafkaWatcherConfig, WatcherType};
    /// use xzatoma::watcher::generic::watcher::GenericWatcher;
    /// use xzatoma::watcher::generic::result_producer::FakeResultProducer;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let mut config = Config::default();
    /// config.watcher.watcher_type = WatcherType::Generic;
    /// config.watcher.kafka = Some(KafkaWatcherConfig {
    ///     brokers: "localhost:9092".to_string(),
    ///     topic: "generic.input".to_string(),
    ///     output_topic: Some("generic.output".to_string()),
    ///     group_id: "xzatoma-generic-doc-test".to_string(),
    ///     auto_create_topics: true,
    ///     security: None,
    ///     num_partitions: 1,
    ///     replication_factor: 1,
    ///     broker_address_family: "v4".to_string(),
    ///     poll_interval_ms: 1000,
    ///     ..KafkaWatcherConfig::default()
    /// });
    /// let watcher = GenericWatcher::new(config, true)?
    ///     .with_producer(Arc::new(FakeResultProducer::new()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_producer(mut self, producer: Arc<dyn ResultProducerTrait>) -> Self {
        self.producer = producer;
        self
    }

    /// Start the generic watcher consume loop.
    ///
    /// Accepts an optional pre-built consumer. When `consumer_override` is
    /// `None`, [`build_consumer`](GenericWatcher::build_consumer) constructs a
    /// [`RealGenericConsumer`] from the watcher's Kafka configuration.
    /// When `Some(consumer)` is provided (typically a
    /// [`crate::watcher::generic::consumer::FakeGenericConsumer`]), that
    /// consumer is used directly — no Kafka connection is established.
    ///
    /// The loop calls [`GenericConsumerTrait::next`] for each message, passes
    /// it to [`GenericWatcher::process_event`], then calls
    /// [`GenericConsumerTrait::commit`] to advance the committed offset.
    ///
    /// The loop exits when the consumer stream is exhausted (`None`), when
    /// [`stop`](GenericWatcher::stop) is called, or when a fatal consumer
    /// error occurs.
    ///
    /// # Arguments
    ///
    /// * `consumer_override` - Optional pre-built consumer; `None` uses the
    ///   real Kafka consumer built from [`get_kafka_config`]
    ///
    /// # Errors
    ///
    /// Returns an error if the consumer cannot be created, subscribed, or if
    /// a fatal consumer error occurs during the loop.
    pub async fn start(
        &mut self,
        consumer_override: Option<Box<dyn GenericConsumerTrait>>,
    ) -> Result<()> {
        info!(
            brokers = %self.kafka_config.brokers,
            topic = %self.kafka_config.topic,
            output_topic = %self.output_topic(),
            matcher = %self.matcher_summary(),
            dry_run = self.dry_run,
            "Starting generic watcher service"
        );

        let mut consumer: Box<dyn GenericConsumerTrait> = match consumer_override {
            Some(c) => c,
            None => Box::new(self.build_consumer()?),
        };

        self.running.store(true, Ordering::SeqCst);

        info!(
            max_concurrent = self.execution_semaphore.available_permits(),
            provider = %self.config.provider.provider_type,
            "Generic watcher consuming from Kafka"
        );

        let mut back_off = BackOffPolicy::new();

        while self.running.load(Ordering::SeqCst) {
            let message = tokio::select! {
                biased;
                msg = consumer.next() => msg,
                () = tokio::time::sleep(std::time::Duration::from_millis(self.kafka_config.poll_interval_ms)) => {
                    trace!("No messages received, checking shutdown flag");
                    continue;
                }
            };

            match message {
                Some(Ok(msg)) => {
                    if let Err(e) = self.process_event(msg).await {
                        error!(error = %e, "Failed to process generic watcher message");
                        back_off.increment();
                    } else {
                        back_off.reset();
                    }
                    if let Err(e) = consumer.commit().await {
                        warn!(error = %e, "Failed to commit Kafka offset");
                    }
                }
                Some(Err(e)) => {
                    back_off.increment();
                    warn!(
                        error = %e,
                        delay_ms = back_off.current_delay_ms(),
                        "Consumer error; backing off before stopping"
                    );
                    tokio::time::sleep(back_off.current_delay()).await;
                    self.running.store(false, Ordering::SeqCst);
                    return Err(e);
                }
                None => {
                    warn!("Message stream ended unexpectedly");
                    break;
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        info!("Generic watcher consumer stopped");
        Ok(())
    }

    /// Signal the watcher to stop consuming messages.
    ///
    /// Sets the internal running flag to `false`, which causes the consume
    /// loop in [`start`] to exit on the next iteration.
    pub fn stop(&self) {
        info!("Stopping generic watcher consumer");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Return whether the watcher consume loop is currently running.
    ///
    /// # Returns
    ///
    /// `true` if the watcher is actively consuming, `false` otherwise.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Process a single raw plan payload string as a generic plan event.
    ///
    /// Convenience wrapper used by unit tests and the legacy consume path. It
    /// constructs a [`RawKafkaMessage`] with the configured input topic and no
    /// message key, then delegates to [`process_event`].
    ///
    /// # Arguments
    ///
    /// * `payload` - Raw plan payload string (YAML or JSON)
    ///
    /// # Returns
    ///
    /// Returns a [`MessageDisposition`] describing how the payload was handled.
    ///
    /// # Errors
    ///
    /// Returns an error if plan execution or result publishing fails after a
    /// successful parse and match.
    pub async fn process_payload(&self, payload: &str) -> Result<MessageDisposition> {
        let msg = RawKafkaMessage {
            payload: payload.to_string(),
            topic: self.kafka_config.topic.clone(),
            key: None,
        };
        self.process_event(msg).await
    }

    /// Process a single raw Kafka message through the event handler pipeline.
    ///
    /// Delegates to [`GenericEventHandler::handle`] for parsing, matching, and
    /// plan resolution. On a successful match, acquires the execution semaphore
    /// and either performs a dry-run or executes the plan.
    ///
    /// # Arguments
    ///
    /// * `msg` - The raw Kafka message (payload, topic, key)
    ///
    /// # Returns
    ///
    /// Returns a [`MessageDisposition`] describing whether the event was
    /// processed or skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if plan execution or result publishing fails.
    pub async fn process_event(&self, msg: RawKafkaMessage) -> Result<MessageDisposition> {
        match self.event_handler.handle(msg).await {
            Err(e) => {
                debug!(error = %e, "Plan parse or validation failed; treating as invalid payload");
                Ok(MessageDisposition::InvalidPayload)
            }
            Ok(None) => {
                debug!("Event did not satisfy configured match criteria; skipping");
                Ok(MessageDisposition::SkippedNoMatch)
            }
            Ok(Some(task)) => {
                let _permit = self.execution_semaphore.acquire().await.map_err(|e| {
                    GenericWatcherError::Execution(format!(
                        "failed to acquire execution semaphore: {}",
                        e
                    ))
                })?;

                let result = if self.dry_run {
                    info!(
                        plan_name = %task.plan.name,
                        "Dry-run mode enabled; skipping generic plan execution"
                    );

                    let trigger_id = task
                        .correlation_key
                        .clone()
                        .unwrap_or_else(|| Ulid::generate().to_string());

                    let mut result = GenericPlanResult::new(
                        trigger_id,
                        true,
                        "Dry-run: matching generic plan event processed without execution"
                            .to_string(),
                    );
                    result.plan_output = Some(json!({
                        "mode": "dry_run",
                        "plan_name": task.plan.name,
                        "instruction": task.instruction,
                        "step_count": task.plan.steps.len(),
                    }));
                    result
                } else {
                    self.execute_plan(&task).await?
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
        }
    }

    /// Return the matcher summary string for structured logging and tests.
    ///
    /// # Returns
    ///
    /// A human-readable matcher summary.
    pub fn matcher_summary(&self) -> String {
        self.event_handler
            .matcher
            .as_ref()
            .map(|m| m.summary())
            .unwrap_or_else(|| "accept-all".to_string())
    }

    /// Return the configured output topic used by the result producer.
    ///
    /// # Returns
    ///
    /// The effective output topic.
    pub fn output_topic(&self) -> &str {
        self.kafka_config
            .output_topic
            .as_deref()
            .unwrap_or(self.kafka_config.topic.as_str())
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
            (
                "broker.address.family".to_string(),
                self.kafka_config.broker_address_family.clone(),
            ),
            (
                "max.poll.interval.ms".to_string(),
                self.kafka_config.max_poll_interval_ms.to_string(),
            ),
        ];

        if let Some(security) = &self.kafka_config.security {
            Self::push_security_settings(&mut settings, security);
        }

        settings
    }

    /// Execute a validated plan task via the standard agent execution path.
    ///
    /// Builds and executes an agent in `ChatMode::Watcher` (autonomous mode)
    /// rather than delegating to `run_plan_with_options`.
    ///
    /// # Arguments
    ///
    /// * `task` - The resolved and validated plan task
    ///
    /// # Returns
    ///
    /// A [`GenericPlanResult`] reflecting the actual execution outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if the task instruction is empty after trimming.
    async fn execute_plan(&self, task: &GenericTask) -> Result<GenericPlanResult> {
        let trimmed = task.instruction.trim();
        // For step-based plans the instruction is required; task-based plans
        // execute via execute_tasks_sequentially instead.
        if trimmed.is_empty() && task.plan.tasks.is_empty() {
            return Err(GenericWatcherError::Execution(
                "task instruction cannot be empty".to_string(),
            )
            .into());
        }

        info!(
            plan_name = %task.plan.name,
            bytes = trimmed.len(),
            "Executing generic watcher plan via Agent in ChatMode::Watcher"
        );

        let config = self.config.as_ref().clone();

        let working_dir = std::env::current_dir().map_err(|e| {
            GenericWatcherError::Execution(format!("failed to get working directory: {}", e))
        })?;

        let env = crate::commands::build_agent_environment(
            &config,
            &working_dir,
            true,
            Some(crate::chat_mode::ChatMode::Watcher),
            Some(crate::chat_mode::SafetyMode::NeverConfirm),
        )
        .await
        .map_err(|e| {
            GenericWatcherError::Execution(format!("failed to build agent environment: {}", e))
        })?;

        let provider =
            crate::providers::create_provider(&config.provider.provider_type, &config.provider)
                .await
                .map_err(|e| {
                    GenericWatcherError::Execution(format!("failed to create provider: {}", e))
                })?;

        let mut agent = crate::agent::Agent::new_with_mode(
            provider,
            env.tool_registry,
            config.agent.clone(),
            crate::chat_mode::ChatMode::Watcher,
            crate::chat_mode::SafetyMode::NeverConfirm,
        )
        .map_err(|e| {
            GenericWatcherError::Execution(format!("failed to create watcher agent: {}", e))
        })?;

        // Resolve and inject the user-defined system prompt.
        // Plan system_prompt takes precedence over config/CLI-level prompt
        // (handled automatically by crate::agent::resolve).
        let resolved_sp = crate::agent::resolve(
            task.plan.system_prompt.as_deref(),
            None,
            config.agent.system_prompt.as_deref(),
        );
        if let Some(ref resolved) = resolved_sp {
            tracing::debug!(
                source = ?resolved.source,
                length = resolved.text.len(),
                "Injecting system prompt into generic watcher agent session"
            );
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!(
                    source = ?resolved.source,
                    system_prompt = %resolved.text,
                    "Generic watcher agent session system prompt"
                );
            }
            agent
                .conversation_mut()
                .add_system_message(resolved.text.clone());
        }

        if let Some(d) = &env.skill_disclosure {
            agent.conversation_mut().add_system_message(d.clone());
        }

        if let Ok(Some(sp)) =
            crate::commands::build_active_skill_prompt_injection(&env.active_skill_registry)
        {
            agent.set_transient_system_messages(vec![sp]);
        }

        // Branch on execution_mode: PerTask runs each task separately in a shared
        // session; SingleShot collapses everything into one agent.execute call.
        let use_per_task = matches!(
            config.watcher.execution.execution_mode,
            WatcherPlanExecutionMode::PerTask
        ) && !task.plan.tasks.is_empty();

        let (success, summary, task_outcomes_opt) = if use_per_task {
            let outcomes = execute_tasks_sequentially(&task.plan, &mut agent).await?;
            let overall_success = outcomes.iter().all(|o| o.success);
            let final_summary = agent
                .execute(
                    "Summarise the results of all tasks completed above in one paragraph."
                        .to_string(),
                )
                .await
                .unwrap_or_else(|e| format!("Summary generation failed: {}", e));
            let outcomes_json: Vec<serde_json::Value> = outcomes
                .iter()
                .map(|o| {
                    json!({
                        "id": o.id,
                        "success": o.success,
                        "summary": o.summary,
                        "iterations": o.iterations,
                    })
                })
                .collect();
            (overall_success, final_summary, Some(outcomes_json))
        } else {
            // Single-shot path: use full instruction string (tasks or steps).
            let instruction = if !task.plan.tasks.is_empty() {
                task.plan.to_instruction()
            } else {
                trimmed.to_string()
            };
            match agent.execute(instruction).await {
                Ok(response) => (true, response, None),
                Err(e) => (
                    false,
                    format!("Generic watcher plan execution failed: {}", e),
                    None,
                ),
            }
        };

        let trigger_id = task
            .correlation_key
            .clone()
            .unwrap_or_else(|| Ulid::generate().to_string());

        let mut result = GenericPlanResult::new(trigger_id, success, summary);
        result.task_outcomes = task_outcomes_opt;
        result.plan_output = Some(json!({
            "mode": if task.plan.tasks.is_empty() { "execute_steps" } else { "execute_tasks" },
            "plan_name": task.plan.name,
            "success": success,
        }));
        Ok(result)
    }

    /// Build a configured and subscribed [`RealGenericConsumer`] from the
    /// watcher's Kafka settings.
    ///
    /// Sets `enable.auto.commit=false` so that offset advances require an
    /// explicit call to [`GenericConsumerTrait::commit`].
    ///
    /// # Returns
    ///
    /// A ready-to-use [`RealGenericConsumer`] subscribed to the input topic.
    ///
    /// # Errors
    ///
    /// Returns an error if the consumer cannot be created or if subscription
    /// to the configured input topic fails.
    fn build_consumer(&self) -> Result<RealGenericConsumer> {
        RealGenericConsumer::from_config_with_startup(
            &self.get_kafka_config(),
            &self.kafka_config.topic,
            self.kafka_config.startup_stabilization_secs,
        )
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
        WatcherPlanExecutionMode,
    };
    use crate::mcp::config::McpConfig;
    use std::collections::HashMap;

    fn test_config(match_config: GenericMatchConfig) -> Config {
        Config {
            provider: ProviderConfig {
                provider_type: "copilot".to_string(),
                copilot: CopilotConfig::default(),
                ollama: OllamaConfig::default(),
                openai: crate::config::OpenAIConfig::default(),
            },
            agent: AgentConfig::default(),
            watcher: WatcherConfig {
                watcher_type: crate::config::WatcherType::Generic,
                kafka: Some(KafkaWatcherConfig {
                    topic: "generic.input".to_string(),
                    output_topic: Some("generic.output".to_string()),
                    group_id: "xzatoma-generic-test".to_string(),
                    ..Default::default()
                }),
                generic_match: match_config,
                filters: Default::default(),
                logging: WatcherLoggingConfig::default(),
                execution: WatcherExecutionConfig {
                    allow_dangerous: false,
                    max_concurrent_executions: 1,
                    execution_timeout_secs: 30,
                    execution_mode: WatcherPlanExecutionMode::PerTask,
                    ..Default::default()
                },
            },
            mcp: McpConfig::default(),
            acp: AcpConfig::default(),
            skills: SkillsConfig::default(),
            log: crate::config::LogConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_execute_plan_injects_config_system_prompt() {
        // Verify that config.agent.system_prompt is resolved and injected when
        // the plan does not carry its own system_prompt field.
        use crate::agent::resolve;
        let result = resolve(None, None, Some("you are a watcher agent"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "you are a watcher agent");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::Config);
    }

    #[tokio::test]
    async fn test_execute_plan_plan_system_prompt_wins_over_config() {
        // Verify that a plan-level system_prompt overrides config.agent.system_prompt.
        use crate::agent::resolve;
        let result = resolve(Some("from the plan"), None, Some("from config"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "from the plan");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::Plan);
    }

    #[test]
    fn test_execute_plan_no_system_prompt_resolves_to_none() {
        use crate::agent::resolve;
        let result = resolve(None, None, None);
        assert!(result.is_none());
    }

    /// A valid CloudEvents plan payload used by watcher tests.
    /// Plan has action "deploy-prod" which matches "deploy.*" but not "rollback.*".
    const MATCHING_PLAN_CE: &str = r#"{"id":"01JTEST000000000000000001","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","version":"v1.2.3","action":"deploy-prod","steps":[{"name":"apply","action":"kubectl apply -f manifests/"}]}}"#;

    /// A valid CloudEvents plan payload whose action does NOT match "rollback.*".
    const NON_MATCHING_PLAN_CE: &str = r#"{"id":"01JTEST000000000000000002","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","action":"deploy-prod","steps":[{"name":"apply","action":"kubectl apply -f manifests/"}]}}"#;

    // -------------------------------------------------------------------------
    // Non-ignored tests (no Kafka broker required)
    // -------------------------------------------------------------------------

    #[test]
    fn test_generic_watcher_new_requires_kafka_config() {
        let mut config = test_config(GenericMatchConfig::default());
        config.watcher.kafka = None;

        let result = GenericWatcher::new(config, true);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_generic_watcher_process_payload_invalid_content_returns_invalid_payload() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        let disposition = watcher.process_payload("not a valid plan").await.unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);
    }

    #[tokio::test]
    async fn test_generic_watcher_process_payload_result_json_returns_invalid_payload() {
        // A GenericPlanResult JSON consumed back on the same topic must be
        // discarded as InvalidPayload (loop-break guarantee).
        let result_json = r#"{
            "id": "01JRESULT",
            "event_type": "result",
            "trigger_event_id": "01JTRIGGER",
            "success": true,
            "summary": "done",
            "timestamp": "2025-01-01T00:00:00Z"
        }"#;
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        let disposition = watcher.process_payload(result_json).await.unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);
    }

    #[tokio::test]
    async fn test_generic_watcher_process_payload_non_matching_event_returns_skipped_no_match() {
        // Matcher configured for "rollback.*"; plan has action "deploy-prod".
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("rollback.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();
        let disposition = watcher.process_payload(NON_MATCHING_PLAN_CE).await.unwrap();
        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
    }

    #[tokio::test]
    async fn test_generic_watcher_process_payload_empty_steps_returns_invalid_payload() {
        let ce = r#"{"id":"01J","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"test","steps":[]}}"#;
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        let disposition = watcher.process_payload(ce).await.unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);
    }

    #[tokio::test]
    async fn test_generic_watcher_matcher_summary_accept_all() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        let summary = watcher.matcher_summary();
        assert!(
            summary.contains("accept-all"),
            "default config should produce accept-all summary"
        );
    }

    #[tokio::test]
    async fn test_generic_watcher_matcher_summary_action_only() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();
        let summary = watcher.matcher_summary();
        assert!(
            summary.contains("action"),
            "action-only config must appear in summary"
        );
    }

    // -------------------------------------------------------------------------
    // Ignored tests (require a running Kafka broker for the publish step)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_generic_watcher_dry_run_processes_matching_event() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let disposition = watcher.process_payload(MATCHING_PLAN_CE).await.unwrap();

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

        let disposition = watcher.process_payload(MATCHING_PLAN_CE).await.unwrap();

        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
        assert!(watcher.published_results().await.is_empty());
    }

    #[tokio::test]
    async fn test_generic_watcher_process_event_matching_action_is_processed() {
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy-prod".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let msg = RawKafkaMessage {
            payload: MATCHING_PLAN_CE.to_string(),
            topic: "generic.input".to_string(),
            key: Some("01JTESTMATCH00000000000001".to_string()),
        };

        let disposition = watcher.process_event(msg).await.unwrap();

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
                action: Some("rollback".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap();

        let msg = RawKafkaMessage {
            payload: MATCHING_PLAN_CE.to_string(),
            topic: "generic.input".to_string(),
            key: None,
        };

        let disposition = watcher.process_event(msg).await.unwrap();

        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
        assert!(watcher.published_results().await.is_empty());
    }

    #[test]
    fn test_generic_watcher_get_kafka_config_includes_security_settings() {
        let mut config = test_config(GenericMatchConfig::default());
        config.watcher.kafka = Some(KafkaWatcherConfig {
            topic: "generic.input".to_string(),
            output_topic: Some("generic.output".to_string()),
            group_id: "xzatoma-generic-test".to_string(),
            security: Some(KafkaSecurityConfig {
                protocol: "SASL_SSL".to_string(),
                sasl_mechanism: Some("SCRAM-SHA-256".to_string()),
                sasl_username: Some("user".to_string()),
                sasl_password: Some("pass".to_string()),
            }),
            ..Default::default()
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
    fn test_generic_watcher_output_topic_uses_producer_resolution() {
        let watcher =
            GenericWatcher::new(test_config(GenericMatchConfig::default()), true).unwrap();
        assert_eq!(watcher.output_topic(), "generic.output");
    }

    // -------------------------------------------------------------------------
    // FakeGenericConsumer + FakeResultProducer integration tests
    // -------------------------------------------------------------------------

    use crate::watcher::generic::consumer::{FakeGenericConsumer, RawKafkaMessage as TestRawMsg};
    use crate::watcher::generic::result_producer::FakeResultProducer;
    use std::sync::atomic::Ordering as AtomicOrdering;

    #[tokio::test]
    async fn test_watcher_processes_matching_event_via_fake_consumer() {
        let fake_producer = Arc::new(FakeResultProducer::new());
        let mut watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap()
        .with_producer(fake_producer.clone());

        let items = vec![Ok(TestRawMsg {
            payload: MATCHING_PLAN_CE.to_string(),
            topic: "generic.input".to_string(),
            key: Some("key-p5-001".to_string()),
        })];

        let fake_consumer = FakeGenericConsumer::new(items);
        watcher.start(Some(Box::new(fake_consumer))).await.unwrap();

        let published = fake_producer.published_events().await;
        assert_eq!(
            published.len(),
            1,
            "one matching event must produce one result"
        );
        assert!(published[0].success, "dry-run result must be successful");

        let recorded = watcher.published_results().await;
        assert_eq!(recorded.len(), 1);
    }

    #[tokio::test]
    async fn test_watcher_discards_non_plan_event() {
        // A GenericPlanResult JSON consumed back on the same topic fails plan
        // parsing and is classified as InvalidPayload (loop-break guarantee).
        let result_json = r#"{"id":"01J","event_type":"result","success":true,"summary":"done","trigger_event_id":"01T","timestamp":"2025-01-01T00:00:00Z"}"#;
        let fake_producer = Arc::new(FakeResultProducer::new());
        let watcher = GenericWatcher::new(test_config(GenericMatchConfig::default()), true)
            .unwrap()
            .with_producer(fake_producer.clone());

        let msg = TestRawMsg {
            payload: result_json.to_string(),
            topic: "generic.input".to_string(),
            key: None,
        };

        // Verify the disposition directly: InvalidPayload is returned for
        // non-plan payloads (SkippedNonPlanEvent is reserved for back-compat
        // and is no longer emitted by the current pipeline).
        let disposition = watcher.process_event(msg.clone()).await.unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);

        // No result must have been published.
        assert!(
            fake_producer.published_events().await.is_empty(),
            "non-plan event must not produce a result"
        );
    }

    #[tokio::test]
    async fn test_watcher_skips_non_matching_event() {
        let fake_producer = Arc::new(FakeResultProducer::new());
        let watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("rollback.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap()
        .with_producer(fake_producer.clone());

        // MATCHING_PLAN_CE has action "deploy-prod" which does NOT match "rollback.*"
        let msg = TestRawMsg {
            payload: MATCHING_PLAN_CE.to_string(),
            topic: "generic.input".to_string(),
            key: None,
        };

        let disposition = watcher.process_event(msg).await.unwrap();
        assert_eq!(disposition, MessageDisposition::SkippedNoMatch);
        assert!(
            fake_producer.published_events().await.is_empty(),
            "non-matching event must not produce a result"
        );
    }

    #[tokio::test]
    async fn test_watcher_invalid_json_produces_invalid_payload() {
        let fake_producer = Arc::new(FakeResultProducer::new());
        let watcher = GenericWatcher::new(test_config(GenericMatchConfig::default()), true)
            .unwrap()
            .with_producer(fake_producer.clone());

        let disposition = watcher
            .process_payload("not valid json or yaml")
            .await
            .unwrap();
        assert_eq!(disposition, MessageDisposition::InvalidPayload);
        assert!(fake_producer.published_events().await.is_empty());
    }

    #[tokio::test]
    async fn test_watcher_commit_called_after_each_message() {
        let fake_producer = Arc::new(FakeResultProducer::new());
        let mut watcher = GenericWatcher::new(
            test_config(GenericMatchConfig {
                action: Some("deploy.*".to_string()),
                name: None,
                version: None,
            }),
            true,
        )
        .unwrap()
        .with_producer(fake_producer.clone());

        let items = vec![
            Ok(TestRawMsg {
                payload: MATCHING_PLAN_CE.to_string(),
                topic: "generic.input".to_string(),
                key: None,
            }),
            // non-matching: still gets a commit
            Ok(TestRawMsg {
                payload: NON_MATCHING_PLAN_CE.to_string(),
                topic: "generic.input".to_string(),
                key: None,
            }),
            // invalid: still gets a commit
            Ok(TestRawMsg {
                payload: "not a plan at all".to_string(),
                topic: "generic.input".to_string(),
                key: None,
            }),
        ];

        let fake_consumer = FakeGenericConsumer::new(items);
        let commit_handle = fake_consumer.commit_counter();

        watcher.start(Some(Box::new(fake_consumer))).await.unwrap();

        assert_eq!(
            commit_handle.load(AtomicOrdering::SeqCst),
            3,
            "commit must be called once per received message regardless of disposition"
        );
    }
}
