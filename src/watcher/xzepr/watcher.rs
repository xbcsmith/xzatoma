//! XZepr watcher service for consuming and processing CloudEvents from Kafka
//!
//! This module provides the core watcher service that:
//! 1. Connects to Kafka topics via the XZepr consumer
//! 2. Consumes XZepr CloudEvents messages
//! 3. Filters events based on configuration
//! 4. Extracts plans from event payloads
//! 5. Executes extracted plans with concurrency control
//!
//! This module was relocated from `src/watcher/watcher.rs` into
//! `src/watcher/xzepr/` as part of the generic watcher architecture.

use super::consumer::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};
use super::filter::EventFilter;
use super::plan_extractor::{PlanExtractionError, PlanExtractor};
use crate::config::{Config, KafkaWatcherConfig, WatcherConfig};
use crate::watcher::generic::result_event::GenericPlanResult;
use crate::watcher::generic::result_producer::ResultProducerTrait;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// Result type for XZepr watcher operations.
pub type WatcherResult<T> = std::result::Result<T, WatcherError>;

/// Errors that can occur in the XZepr watcher service.
#[derive(Error, Debug)]
pub enum WatcherError {
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid Kafka security protocol.
    #[error("Invalid security protocol: {protocol}")]
    InvalidSecurityProtocol {
        /// Invalid protocol value.
        protocol: String,
    },

    /// SASL mechanism was configured without a username.
    #[error("SASL username is required when mechanism is set")]
    MissingSaslUsername,

    /// SASL mechanism was configured without a password.
    #[error("SASL password required (set via config or KAFKA_SASL_PASSWORD env var)")]
    MissingSaslPassword,

    /// Invalid SASL mechanism.
    #[error("Invalid SASL mechanism: {mechanism}")]
    InvalidSaslMechanism {
        /// Invalid mechanism value.
        mechanism: String,
    },

    /// Kafka consumer error.
    #[error("Consumer error: {source}")]
    Consumer {
        /// Underlying consumer error.
        #[source]
        source: super::consumer::ConsumerError,
    },

    /// Event filtering error.
    #[error("Filter error: {0}")]
    Filter(String),

    /// Plan extraction error.
    #[error("Plan extraction error: {source}")]
    PlanExtraction {
        /// Underlying extraction error.
        #[source]
        source: PlanExtractionError,
    },

    /// Plan execution error.
    #[error("Execution error: {0}")]
    Execution(String),

    /// Result producer error.
    #[error("Producer error: {0}")]
    Producer(String),
}

impl WatcherError {
    /// Returns the watcher operation associated with this error.
    ///
    /// # Returns
    ///
    /// Returns a stable operation label for crate-level error conversion.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::xzepr::watcher::WatcherError;
    ///
    /// let error = WatcherError::MissingSaslUsername;
    /// assert_eq!(error.operation(), "security configuration");
    /// ```
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Config(_) => "configuration",
            Self::InvalidSecurityProtocol { .. }
            | Self::MissingSaslUsername
            | Self::MissingSaslPassword
            | Self::InvalidSaslMechanism { .. } => "security configuration",
            Self::Consumer { .. } => "consumer",
            Self::Filter(_) => "filter",
            Self::PlanExtraction { .. } => "plan extraction",
            Self::Execution(_) => "execution",
            Self::Producer(_) => "producer",
        }
    }
}

impl From<PlanExtractionError> for WatcherError {
    fn from(source: PlanExtractionError) -> Self {
        Self::PlanExtraction { source }
    }
}

/// Main XZepr watcher service for processing CloudEvents from Kafka.
///
/// The watcher manages the lifecycle of event consumption, filtering,
/// plan extraction, and execution. It maintains concurrent execution
/// limits and integrates with the XZepr Kafka consumer.
///
/// This type is also accessible as `crate::watcher::XzeprWatcher` via the
/// top-level watcher re-export.
///
/// # Example
///
/// ```
/// use xzatoma::config::Config;
/// use xzatoma::watcher::XzeprWatcher;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = Config::default();
/// let mut watcher = XzeprWatcher::new(config, false)?;
/// watcher.start().await?;
/// # Ok(())
/// # }
/// ```
pub struct Watcher {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    kafka_config: KafkaWatcherConfig,
    consumer: XzeprConsumer,
    filter: Arc<EventFilter>,
    extractor: Arc<PlanExtractor>,
    producer: Arc<dyn ResultProducerTrait>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
}

impl Watcher {
    /// Create a new XZepr watcher instance from global configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Global XZatoma configuration containing watcher settings
    /// * `dry_run` - If true, extract plans but don't execute them
    ///
    /// # Returns
    ///
    /// Returns a configured `Watcher` instance ready to start consuming.
    ///
    /// # Errors
    ///
    /// Returns `WatcherError::Config` if watcher configuration is missing or invalid.
    /// Returns `WatcherError::Consumer` if the Kafka consumer cannot be created.
    /// Returns `WatcherError::Filter` if event filter initialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::Config;
    /// use xzatoma::watcher::XzeprWatcher;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// let config = Config::default();
    /// let watcher = XzeprWatcher::new(config, false)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: Config, dry_run: bool) -> WatcherResult<Self> {
        let watcher_config = config.watcher.clone();

        // Validate Kafka configuration exists
        let kafka_config = watcher_config.kafka.clone().ok_or_else(|| {
            WatcherError::Config("Kafka configuration is required for watcher".to_string())
        })?;

        debug!(
            brokers = %kafka_config.brokers,
            topic = %kafka_config.topic,
            "Configuring Kafka consumer"
        );

        // Build Kafka consumer configuration
        let consumer_config =
            KafkaConsumerConfig::new(&kafka_config.brokers, &kafka_config.topic, "xzatoma")
                .with_group_id(&kafka_config.group_id)
                .with_broker_address_family(&kafka_config.broker_address_family)
                .with_poll_interval(std::time::Duration::from_millis(
                    kafka_config.poll_interval_ms,
                ))
                .with_max_payload_bytes(watcher_config.execution.max_payload_bytes);

        // Apply security settings if configured
        let consumer_config = if let Some(security) = &kafka_config.security {
            Self::apply_security_config(consumer_config, security)?
        } else {
            consumer_config
        };

        // Create Kafka consumer
        let consumer = XzeprConsumer::new(consumer_config)
            .map_err(|source| WatcherError::Consumer { source })?;

        debug!("Kafka consumer created successfully");

        let producer: Arc<dyn ResultProducerTrait> =
            crate::watcher::lifecycle::build_producer(&kafka_config, dry_run)
                .map_err(|e| WatcherError::Producer(e.to_string()))?;

        // Create event filter
        let filter = Arc::new(
            EventFilter::new(watcher_config.filters.clone())
                .map_err(|e| WatcherError::Filter(e.to_string()))?,
        );

        // Create plan extractor with default strategies
        let extractor = Arc::new(PlanExtractor::new());

        // Create execution semaphore for concurrency control
        let max_concurrent = watcher_config.execution.max_concurrent_executions;
        let execution_semaphore =
            crate::watcher::lifecycle::build_execution_semaphore(max_concurrent);

        debug!(
            max_concurrent = max_concurrent,
            dry_run = dry_run,
            "Execution semaphore created"
        );

        Ok(Self {
            config: Arc::new(config),
            watcher_config,
            kafka_config,
            consumer,
            filter,
            extractor,
            producer,
            execution_semaphore,
            dry_run,
        })
    }

    /// Replace the result producer with the provided implementation.
    ///
    /// This builder method enables injection of test doubles such as
    /// [`FakeResultProducer`](crate::watcher::generic::result_producer::FakeResultProducer)
    /// so the watcher loop can be exercised without a live Kafka broker.
    ///
    /// # Arguments
    ///
    /// * `producer` - The producer implementation to use
    ///
    /// # Returns
    ///
    /// `self` with the producer replaced.
    pub fn with_producer(mut self, producer: Arc<dyn ResultProducerTrait>) -> Self {
        self.producer = producer;
        self
    }

    /// Return the configured output topic used by the result producer.
    ///
    /// # Returns
    ///
    /// The effective output topic.
    pub fn output_topic(&self) -> &str {
        crate::watcher::lifecycle::resolve_output_topic(&self.kafka_config)
    }

    /// Start watching for and processing events from the Kafka topic.
    ///
    /// This is the main loop that consumes messages from Kafka. It will run
    /// indefinitely until an error occurs or the process is signaled to stop.
    ///
    /// Topic auto-creation is handled by `run_watch` in `commands/mod.rs`
    /// before the watcher is constructed. Callers using `XzeprWatcher`
    /// directly should ensure topics exist before calling `start()`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on graceful shutdown or error if processing fails.
    ///
    /// # Errors
    ///
    /// Returns `WatcherError::Consumer` if subscription or message consumption fails.
    ///
    /// # Example
    ///
    /// ```
    /// use xzatoma::config::Config;
    /// use xzatoma::watcher::XzeprWatcher;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// let config = Config::default();
    /// let mut watcher = XzeprWatcher::new(config, false)?;
    /// watcher.start().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start(&mut self) -> WatcherResult<()> {
        info!(
            filters = %self.filter.summary(),
            output_topic = %self.output_topic(),
            dry_run = self.dry_run,
            "Starting XZepr watcher service"
        );

        // Create message handler with shared state
        let handler = WatcherMessageHandler {
            config: self.config.clone(),
            watcher_config: self.watcher_config.clone(),
            filter: self.filter.clone(),
            extractor: self.extractor.clone(),
            producer: self.producer.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
            dry_run: self.dry_run,
        };

        // Start consuming messages
        debug!("Starting message consumer loop");
        self.consumer
            .run(Arc::new(handler))
            .await
            .map_err(|source| WatcherError::Consumer { source })?;

        Ok(())
    }

    /// Apply security configuration to a Kafka consumer config.
    ///
    /// Note: Kafka message payloads are untrusted input and must be validated
    /// by downstream handlers before use.
    ///
    /// # Arguments
    ///
    /// * `config` - The consumer config to modify
    /// * `security` - Security settings from the watcher configuration
    ///
    /// # Returns
    ///
    /// Returns the updated `KafkaConsumerConfig` on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the security protocol or SASL mechanism is invalid,
    /// or if required SASL credentials are missing.
    fn apply_security_config(
        mut config: KafkaConsumerConfig,
        security: &crate::config::KafkaSecurityConfig,
    ) -> WatcherResult<KafkaConsumerConfig> {
        use super::consumer::config::SaslConfig;
        use crate::watcher::kafka_security::{
            parse_sasl_mechanism, parse_security_protocol, warn_if_insecure,
        };

        debug!(
            protocol = %security.protocol,
            "Applying security configuration"
        );

        // Parse and set security protocol via the shared helper.
        config.security_protocol = parse_security_protocol(&security.protocol).map_err(|_| {
            WatcherError::InvalidSecurityProtocol {
                protocol: security.protocol.clone(),
            }
        })?;

        // Warn once, at the apply step, if the protocol is unencrypted.
        warn_if_insecure(&config.security_protocol);

        // Apply SASL settings if present
        if let Some(mechanism) = &security.sasl_mechanism {
            let username = security
                .sasl_username
                .as_ref()
                .ok_or(WatcherError::MissingSaslUsername)?;

            let password = security
                .sasl_password
                .as_ref()
                .map(|p| p.to_string())
                .or_else(|| std::env::var("KAFKA_SASL_PASSWORD").ok())
                .ok_or(WatcherError::MissingSaslPassword)?;

            debug!(mechanism = %mechanism, "Applying SASL configuration");

            let sasl_mechanism = parse_sasl_mechanism(mechanism).map_err(|_| {
                WatcherError::InvalidSaslMechanism {
                    mechanism: mechanism.clone(),
                }
            })?;

            config.sasl_config = Some(SaslConfig {
                mechanism: sasl_mechanism,
                username: username.to_string(),
                password,
            });
        }

        Ok(config)
    }
}

/// Message handler that processes XZepr CloudEvents from the watcher.
///
/// This handler is invoked for each message received from Kafka.
/// It applies filters, extracts plans, and executes them with
/// proper concurrency control and error handling.
#[derive(Clone)]
struct WatcherMessageHandler {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    filter: Arc<EventFilter>,
    extractor: Arc<PlanExtractor>,
    producer: Arc<dyn ResultProducerTrait>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
}

#[async_trait]
impl MessageHandler for WatcherMessageHandler {
    /// Process a CloudEvent message.
    ///
    /// # Arguments
    ///
    /// * `message` - The CloudEvent message from Kafka
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if processing completed (even if plan execution failed).
    /// Returns `Err` if message processing itself encountered an unrecoverable error.
    ///
    /// # Processing Steps
    ///
    /// 1. Check if event passes configured filters
    /// 2. Extract plan from event payload
    /// 3. Check for dry-run mode
    /// 4. Acquire execution permit (respects concurrency limit)
    /// 5. Execute plan in a spawned task
    /// 6. Log results
    async fn handle(
        &self,
        message: CloudEventMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let span = tracing::info_span!(
            "handle_event",
            event_id = %message.id,
            event_type = %message.event_type,
            source = %message.source,
        );

        let _enter = span.enter();

        debug!("Received CloudEvent message");

        // Apply event filters
        if !self.filter.should_process(&message) {
            debug!("Event filtered out by configured filters");
            return Ok(());
        }

        info!("Event passed filters, attempting plan extraction");

        // Extract plan from the event (returns YAML string)
        let plan_yaml = match self.extractor.extract(&message) {
            Ok(yaml) => {
                debug!("Successfully extracted plan from event");
                yaml
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to extract plan from event payload"
                );
                return Ok(()); // Log and continue, don't fail
            }
        };

        info!("Plan extracted and ready for execution");

        let trigger_event_id = message.id.clone();
        let event_type = message.event_type.clone();

        // Check if in dry-run mode
        if self.dry_run {
            info!("Dry-run mode enabled: skipping plan execution");

            let mut result = GenericPlanResult::new(
                trigger_event_id,
                true,
                "Dry-run: XZepr plan extracted and processed without execution".to_string(),
            );
            result.plan_output = Some(json!({
                "mode": "dry_run",
                "source_event_type": event_type,
            }));

            if let Err(e) = self.producer.publish(&result).await {
                warn!(error = %e, "Failed to publish dry-run XZepr watcher result");
            }

            return Ok(());
        }

        // Attempt to acquire execution permit (respects max concurrent executions)
        let _permit = match self.execution_semaphore.acquire().await {
            Ok(p) => p,
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to acquire execution permit"
                );
                return Err(Box::new(WatcherError::Execution(format!(
                    "failed to acquire execution permit: {}",
                    e
                ))));
            }
        };

        debug!("Execution permit acquired, spawning plan execution task");

        // Clone values needed for the spawned task
        let config = self.config.as_ref().clone();
        let _allow_dangerous = self.watcher_config.execution.allow_dangerous;

        // Spawn plan execution in background task
        let execution_task = tokio::spawn(async move {
            debug!("Plan execution task started");

            let working_dir = std::env::current_dir().map_err(|e| {
                crate::error::XzatomaError::Config(format!(
                    "Failed to get working directory: {}",
                    e
                ))
            })?;

            let env = crate::commands::build_agent_environment(
                &config,
                &working_dir,
                true,
                Some(crate::chat_mode::ChatMode::Watcher),
                Some(crate::chat_mode::SafetyMode::NeverConfirm),
            )
            .await?;

            let provider =
                crate::providers::create_provider(&config.provider.provider_type, &config.provider)
                    .await?;

            let mut agent = crate::agent::Agent::new_with_mode(
                provider,
                env.tool_registry,
                config.agent.clone(),
                crate::chat_mode::ChatMode::Watcher,
                crate::chat_mode::SafetyMode::NeverConfirm,
            )?;

            // Parse the plan early to extract any plan-level system_prompt for
            // resolution. The same result is reused for execution below.
            let plan_parse_result = crate::tools::plan::PlanParser::from_yaml(&plan_yaml);

            // Resolve system prompt: plan system_prompt wins over config/CLI.
            let plan_sp = plan_parse_result
                .as_ref()
                .ok()
                .and_then(|p| p.system_prompt.as_deref());
            let resolved_sp =
                crate::agent::resolve(plan_sp, None, config.agent.system_prompt.as_deref());

            // Inject resolved system prompt before skill disclosure.
            if let Some(ref resolved) = resolved_sp {
                tracing::debug!(
                    source = ?resolved.source,
                    length = resolved.text.len(),
                    "Injecting system prompt into XZepr watcher agent session"
                );
                if tracing::enabled!(tracing::Level::TRACE) {
                    tracing::trace!(
                        source = ?resolved.source,
                        system_prompt = %resolved.text,
                        "XZepr watcher agent session system prompt"
                    );
                }
                agent
                    .conversation_mut()
                    .add_system_message(resolved.text.clone());
            }

            if let Some(disclosure) = &env.skill_disclosure {
                agent
                    .conversation_mut()
                    .add_system_message(disclosure.clone());
            }

            if let Ok(Some(skill_prompt)) =
                crate::commands::build_active_skill_prompt_injection(&env.active_skill_registry)
            {
                agent.set_transient_system_messages(vec![skill_prompt]);
            }

            let exec_result: crate::error::Result<(bool, String, Option<Vec<serde_json::Value>>)> =
                match plan_parse_result {
                    Ok(plan) => {
                        let use_per_task = matches!(
                            config.watcher.execution.execution_mode,
                            crate::config::WatcherPlanExecutionMode::PerTask
                        ) && !plan.tasks.is_empty();

                        if use_per_task {
                            let outcomes =
                                crate::watcher::plan_executor::execute_tasks_sequentially(
                                    &plan, &mut agent,
                                )
                                .await?;
                            let success = outcomes.iter().all(|o| o.success);
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
                                    serde_json::json!({
                                        "id": o.id,
                                        "success": o.success,
                                        "summary": o.summary,
                                        "iterations": o.iterations,
                                    })
                                })
                                .collect();
                            Ok((success, final_summary, Some(outcomes_json)))
                        } else {
                            let instruction = plan.to_instruction();
                            match agent.execute(instruction).await {
                                Ok(response) => Ok((true, response, None)),
                                Err(e) => {
                                    Ok((false, format!("XZepr plan execution failed: {}", e), None))
                                }
                            }
                        }
                    }
                    Err(parse_err) => {
                        warn!(
                            error = %parse_err,
                            "Failed to parse XZepr plan YAML; falling back to single-shot execution"
                        );
                        match agent.execute(plan_yaml).await {
                            Ok(response) => Ok((true, response, None)),
                            Err(e) => {
                                Ok((false, format!("XZepr plan execution failed: {}", e), None))
                            }
                        }
                    }
                };
            exec_result
        });

        // Wait for execution to complete and publish the result
        let (success, summary, task_outcomes_opt) = match execution_task.await {
            Ok(Ok((success, summary, outcomes))) => (success, summary, outcomes),
            Ok(Err(e)) => (
                false,
                format!("XZepr watcher plan execution failed: {}", e),
                None,
            ),
            Err(e) => (
                false,
                format!("XZepr watcher plan execution task join failed: {}", e),
                None,
            ),
        };

        if success {
            info!("Plan executed successfully");
        } else {
            error!(summary = %summary, "Plan execution failed");
        }

        let has_task_outcomes = task_outcomes_opt.is_some();
        let mut result = GenericPlanResult::new(trigger_event_id, success, summary);
        result.task_outcomes = task_outcomes_opt;
        result.plan_output = Some(json!({
            "mode": if has_task_outcomes { "execute_tasks" } else { "execute" },
            "source_event_type": event_type,
            "success": success,
        }));

        if let Err(e) = self.producer.publish(&result).await {
            warn!(
                error = %e,
                success = success,
                "Failed to publish XZepr watcher result"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_error_display() {
        let err = WatcherError::Config("test error".to_string());
        assert_eq!(err.to_string(), "Configuration error: test error");

        let err = WatcherError::Consumer {
            source: crate::watcher::xzepr::consumer::ConsumerError::Config(
                "kafka failed".to_string(),
            ),
        };
        assert_eq!(
            err.to_string(),
            "Consumer error: Configuration error: kafka failed"
        );

        let err = WatcherError::Filter("invalid filter".to_string());
        assert_eq!(err.to_string(), "Filter error: invalid filter");

        let err = WatcherError::PlanExtraction {
            source: PlanExtractionError::NoStrategyMatched {
                event_id: "event-1".to_string(),
            },
        };
        assert!(err.to_string().contains("Plan extraction error"));

        let err = WatcherError::Execution("execution timeout".to_string());
        assert_eq!(err.to_string(), "Execution error: execution timeout");

        let err = WatcherError::Producer("kafka unavailable".to_string());
        assert_eq!(err.to_string(), "Producer error: kafka unavailable");
    }

    #[test]
    fn test_watcher_error_is_error_trait() {
        let err = WatcherError::Config("test".to_string());
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_watcher_creation_requires_kafka_config() {
        let config = Config {
            provider: crate::config::ProviderConfig {
                provider_type: "copilot".to_string(),
                copilot: Default::default(),
                ollama: Default::default(),
                openai: Default::default(),
            },
            agent: crate::config::AgentConfig::default(),
            watcher: crate::config::WatcherConfig {
                watcher_type: crate::config::WatcherType::XZepr,
                kafka: None,
                generic_match: Default::default(),
                filters: Default::default(),
                logging: Default::default(),
                execution: Default::default(),
            },
            mcp: crate::mcp::config::McpConfig::default(),
            acp: crate::config::AcpConfig::default(),
            skills: crate::config::SkillsConfig::default(),
            log: crate::config::LogConfig::default(),
        };

        let result = Watcher::new(config, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_watcher_creation_with_valid_config() {
        let mut config = Config::default();
        config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
            topic: "test-topic".to_string(),
            group_id: "test-group".to_string(),
            ..Default::default()
        });

        let result = Watcher::new(config, false);
        assert!(result.is_ok());
        let watcher = result.unwrap();
        assert_eq!(
            watcher.watcher_config.execution.max_concurrent_executions,
            1
        );
        assert!(!watcher.dry_run);
    }

    #[test]
    fn test_watcher_creation_with_dry_run() {
        let mut config = Config::default();
        config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
            topic: "test-topic".to_string(),
            group_id: "test-group".to_string(),
            ..Default::default()
        });

        let result = Watcher::new(config, true);
        assert!(result.is_ok());
        let watcher = result.unwrap();
        assert!(watcher.dry_run);
    }

    #[test]
    fn test_apply_security_config_rejects_invalid_protocol() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "xzatoma");
        let security = crate::config::KafkaSecurityConfig {
            protocol: "NOPE".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        };

        let error = Watcher::apply_security_config(config, &security).unwrap_err();
        assert!(matches!(
            error,
            WatcherError::InvalidSecurityProtocol { .. }
        ));
        assert_eq!(error.operation(), "security configuration");
    }

    #[test]
    fn test_apply_security_config_plaintext_returns_ok_with_warning_side_effect() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "xzatoma");
        let security = crate::config::KafkaSecurityConfig {
            protocol: "PLAINTEXT".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        };

        // The unencrypted-traffic warning is only a side effect; the function
        // must still succeed and return the configured protocol.
        let result = Watcher::apply_security_config(config, &security);
        assert!(result.is_ok());
    }

    #[test]
    fn test_apply_security_config_rejects_missing_sasl_username() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "xzatoma");
        let security = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: None,
            sasl_password: Some("secret".to_string()),
        };

        let error = Watcher::apply_security_config(config, &security).unwrap_err();
        assert!(matches!(error, WatcherError::MissingSaslUsername));
    }

    #[test]
    fn test_apply_security_config_rejects_missing_sasl_password() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "xzatoma");
        let security = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: None,
        };

        let error = Watcher::apply_security_config(config, &security).unwrap_err();
        assert!(matches!(error, WatcherError::MissingSaslPassword));
    }

    #[test]
    fn test_apply_security_config_rejects_invalid_sasl_mechanism() {
        let config = KafkaConsumerConfig::new("localhost:9092", "topic", "xzatoma");
        let security = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("INVALID".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: Some("secret".to_string()),
        };

        let error = Watcher::apply_security_config(config, &security).unwrap_err();
        assert!(matches!(error, WatcherError::InvalidSaslMechanism { .. }));
    }

    #[test]
    fn test_watcher_output_topic_uses_explicit_output_topic_when_configured() {
        let mut config = Config::default();
        config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
            topic: "xzepr.events".to_string(),
            output_topic: Some("xzepr.results".to_string()),
            group_id: "test-group".to_string(),
            ..Default::default()
        });

        let watcher = Watcher::new(config, false).unwrap();
        assert_eq!(watcher.output_topic(), "xzepr.results");
    }

    #[test]
    fn test_watcher_output_topic_falls_back_to_input_topic() {
        let mut config = Config::default();
        config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
            topic: "xzepr.events".to_string(),
            group_id: "test-group".to_string(),
            ..Default::default()
        });

        let watcher = Watcher::new(config, false).unwrap();
        assert_eq!(watcher.output_topic(), "xzepr.events");
    }

    #[test]
    fn test_watcher_execution_config_defaults() {
        let config = Config::default();
        assert!(!config.watcher.execution.allow_dangerous);
        assert_eq!(config.watcher.execution.max_concurrent_executions, 1);
        assert_eq!(config.watcher.execution.execution_timeout_secs, 300);
    }

    #[test]
    fn test_xzepr_watcher_system_prompt_resolve_plan_wins_over_config() {
        // The plan-level system_prompt overrides the config-level one.
        use crate::agent::resolve;
        let result = resolve(Some("from the xzepr plan"), None, Some("from config"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "from the xzepr plan");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::Plan);
    }

    #[test]
    fn test_xzepr_watcher_system_prompt_config_used_when_plan_has_none() {
        use crate::agent::resolve;
        let result = resolve(None, None, Some("from config"));
        let resolved = result.unwrap();
        assert_eq!(resolved.text, "from config");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::Config);
    }

    #[test]
    fn test_xzepr_watcher_system_prompt_none_when_no_sources() {
        use crate::agent::resolve;
        assert!(resolve(None, None, None).is_none());
    }
}
