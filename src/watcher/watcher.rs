//! Watcher service for consuming and processing CloudEvents from Kafka
//!
//! This module provides the core watcher service that:
//! 1. Connects to Kafka topics
//! 2. Consumes CloudEvents messages
//! 3. Filters events based on configuration
//! 4. Extracts plans from event payloads
//! 5. Executes extracted plans with concurrency control

use crate::config::{Config, WatcherConfig};
use crate::watcher::{EventFilter, PlanExtractor, PlanExtractorTrait};
use crate::xzepr::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// Abstraction for executing extracted plans.
///
/// This trait allows injecting a test-friendly executor into the watcher so
/// unit and integration tests can assert behavior without invoking the full
/// `run_plan_with_options` flow (which may hit external providers).
#[async_trait]
pub trait PlanExecutor: Send + Sync + 'static {
    /// Execute a plan represented as a YAML string.
    ///
    /// * `config` - Global configuration (consumed)
    /// * `plan` - Plan YAML content
    /// * `allow_dangerous` - Escalate execution mode if true
    async fn execute_plan(
        &self,
        config: crate::config::Config,
        plan: String,
        allow_dangerous: bool,
    ) -> anyhow::Result<()>;
}

/// Default plan executor that delegates to the real run command.
pub struct RealPlanExecutor;

#[async_trait]
impl PlanExecutor for RealPlanExecutor {
    async fn execute_plan(
        &self,
        config: crate::config::Config,
        plan: String,
        allow_dangerous: bool,
    ) -> anyhow::Result<()> {
        crate::commands::r#run::run_plan_with_options(config, None, Some(plan), allow_dangerous)
            .await
    }
}

/// Errors that can occur in the watcher service
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum WatcherError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Kafka consumer error
    #[error("Consumer error: {0}")]
    Consumer(String),

    /// Event filtering error
    #[error("Filter error: {0}")]
    Filter(String),

    /// Plan extraction error
    #[error("Plan extraction error: {0}")]
    PlanExtraction(String),

    /// Plan execution error
    #[error("Execution error: {0}")]
    Execution(String),
}

/// Main watcher service for processing CloudEvents from Kafka
///
/// The watcher manages the lifecycle of event consumption, filtering,
/// plan extraction, and execution. It maintains concurrent execution
/// limits and integrates with the XZepr Kafka consumer.
///
/// # Example
///
/// ```rust,no_run
/// use xzatoma::config::Config;
/// use xzatoma::watcher::Watcher;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = Config::default();
/// let mut watcher = Watcher::new(config, false)?;
/// watcher.start().await?;
/// # Ok(())
/// # }
/// ```
pub struct Watcher {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    consumer: XzeprConsumer,
    filter: Arc<EventFilter>,
    extractor: Arc<dyn PlanExtractorTrait>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
    executor: Arc<dyn PlanExecutor>,
}

impl Watcher {
    /// Create a new watcher instance from global configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Global XZatoma configuration containing watcher settings
    /// * `dry_run` - If true, extract plans but don't execute them
    ///
    /// # Returns
    ///
    /// Returns a configured Watcher instance ready to start consuming
    ///
    /// # Errors
    ///
    /// Returns `WatcherError::Config` if watcher configuration is missing or invalid
    /// Returns `WatcherError::Consumer` if Kafka consumer cannot be created
    /// Returns `WatcherError::Filter` if event filter initialization fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use xzatoma::config::Config;
    /// use xzatoma::watcher::Watcher;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = Config::default();
    /// let watcher = Watcher::new(config, false)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: Config, dry_run: bool) -> Result<Self> {
        Self::with_executor(config, dry_run, Arc::new(RealPlanExecutor))
    }

    /// Create a watcher instance with an injected plan executor.
    ///
    /// This constructor is useful for tests that need to control/observe
    /// plan execution behavior.
    pub fn with_executor(
        config: Config,
        dry_run: bool,
        executor: Arc<dyn PlanExecutor>,
    ) -> Result<Self> {
        let watcher_config = config.watcher.clone();

        // Validate Kafka configuration exists
        let kafka_config = watcher_config.kafka.as_ref().ok_or_else(|| {
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
                .with_group_id(&kafka_config.group_id);

        // Apply security settings if configured
        let consumer_config = if let Some(security) = &kafka_config.security {
            Self::apply_security_config(consumer_config, security)?
        } else {
            consumer_config
        };

        // Create Kafka consumer
        let consumer = XzeprConsumer::new(consumer_config)
            .map_err(|e| WatcherError::Consumer(e.to_string()))?;

        debug!("Kafka consumer created successfully");

        // Create event filter
        let filter = Arc::new(
            EventFilter::new(watcher_config.filters.clone())
                .map_err(|e| WatcherError::Filter(e.to_string()))?,
        );

        // Create plan extractor with default strategies
        let extractor: Arc<dyn PlanExtractorTrait> = Arc::new(PlanExtractor::new());

        // Create execution semaphore for concurrency control
        let max_concurrent = watcher_config.execution.max_concurrent_executions;
        let execution_semaphore = Arc::new(Semaphore::new(max_concurrent));

        debug!(
            max_concurrent = max_concurrent,
            dry_run = dry_run,
            "Execution semaphore created"
        );

        Ok(Self {
            config: Arc::new(config),
            watcher_config,
            consumer,
            filter,
            extractor,
            execution_semaphore,
            dry_run,
            executor,
        })
    }

    /// Start watching for and processing events from the Kafka topic
    ///
    /// This is the main loop that consumes messages from Kafka. It will run
    /// indefinitely until an error occurs or the process is signaled to stop.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on graceful shutdown or error if processing fails
    ///
    /// # Errors
    ///
    /// Returns `WatcherError::Consumer` if subscription fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use xzatoma::config::Config;
    /// use xzatoma::watcher::Watcher;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = Config::default();
    /// let mut watcher = Watcher::new(config, false)?;
    /// watcher.start().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start(&mut self) -> Result<()> {
        info!(
            filters = %self.filter.summary(),
            dry_run = self.dry_run,
            "Starting watcher service"
        );

        // Create message handler with shared state
        let handler = WatcherMessageHandler {
            config: self.config.clone(),
            watcher_config: self.watcher_config.clone(),
            filter: self.filter.clone(),
            extractor: self.extractor.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
            dry_run: self.dry_run,
            executor: self.executor.clone(),
        };

        // Start consuming messages
        debug!("Starting message consumer loop");
        self.consumer
            .run(Arc::new(handler))
            .await
            .map_err(|e| WatcherError::Consumer(e.to_string()))?;

        Ok(())
    }

    /// Apply security configuration to Kafka consumer.
    fn apply_security_config(
        mut config: KafkaConsumerConfig,
        security: &crate::config::KafkaSecurityConfig,
    ) -> Result<KafkaConsumerConfig> {
        use crate::xzepr::consumer::config::{SaslConfig, SaslMechanism, SecurityProtocol};

        debug!(
            protocol = %security.protocol,
            "Applying security configuration"
        );

        // Parse and set security protocol
        config.security_protocol = match security.protocol.to_uppercase().as_str() {
            "PLAINTEXT" => SecurityProtocol::Plaintext,
            "SSL" => SecurityProtocol::Ssl,
            "SASL_PLAINTEXT" => SecurityProtocol::SaslPlaintext,
            "SASL_SSL" => SecurityProtocol::SaslSsl,
            _ => return Err(anyhow!("Invalid security protocol: {}", security.protocol)),
        };

        // Apply SASL settings if present
        if let Some(mechanism) = &security.sasl_mechanism {
            let username = security
                .sasl_username
                .as_ref()
                .ok_or_else(|| anyhow!("SASL username is required when mechanism is set"))?;

            let password = security
                .sasl_password
                .as_ref()
                .map(|p| p.to_string())
                .or_else(|| std::env::var("KAFKA_SASL_PASSWORD").ok())
                .ok_or_else(|| {
                    anyhow!(
                        "SASL password required (set via config or KAFKA_SASL_PASSWORD env var)"
                    )
                })?;

            debug!(mechanism = %mechanism, "Applying SASL configuration");

            let sasl_mechanism = match mechanism.to_uppercase().as_str() {
                "PLAIN" => SaslMechanism::Plain,
                "SCRAM-SHA-256" => SaslMechanism::ScramSha256,
                "SCRAM-SHA-512" => SaslMechanism::ScramSha512,
                _ => return Err(anyhow!("Invalid SASL mechanism: {}", mechanism)),
            };

            config.sasl_config = Some(SaslConfig {
                mechanism: sasl_mechanism,
                username: username.to_string(),
                password,
            });
        }

        Ok(config)
    }
}

/// Message handler that processes CloudEvents from the watcher
///
/// This handler is invoked for each message received from Kafka.
/// It applies filters, extracts plans, and executes them with
/// proper concurrency control and error handling.
#[derive(Clone)]
struct WatcherMessageHandler {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    filter: Arc<EventFilter>,
    extractor: Arc<dyn PlanExtractorTrait>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
    executor: Arc<dyn PlanExecutor>,
}

#[async_trait]
impl MessageHandler for WatcherMessageHandler {
    /// Process a CloudEvent message
    ///
    /// # Arguments
    ///
    /// * `message` - The CloudEvent message from Kafka
    ///
    /// # Returns
    ///
    /// Returns Ok(()) if processing completed (even if plan execution failed)
    /// Returns Err if message processing itself failed
    ///
    /// # Processing Steps
    ///
    /// 1. Check if event passes configured filters
    /// 2. Extract plan from event payload
    /// 3. Check for dry-run mode
    /// 4. Acquire execution permit (respects concurrency limit)
    /// 5. Execute plan in spawned task
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

        // Check if in dry-run mode
        if self.dry_run {
            info!("Dry-run mode enabled: skipping plan execution");
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
                return Err(format!("Failed to acquire execution permit: {}", e).into());
            }
        };

        debug!("Execution permit acquired, spawning plan execution task");

        // Clone values needed for the spawned task
        let config = self.config.as_ref().clone();
        let allow_dangerous = self.watcher_config.execution.allow_dangerous;
        let executor = self.executor.clone();
        let plan_to_execute = plan_yaml.clone();

        // Spawn plan execution in background task
        let execution_task = tokio::spawn(async move {
            debug!("Plan execution task started");

            let result = executor
                .execute_plan(config, plan_to_execute, allow_dangerous)
                .await;

            result
        });

        // Wait for execution to complete
        match execution_task.await {
            Ok(Ok(())) => {
                info!("Plan executed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                error!(
                    error = %e,
                    "Plan execution failed"
                );
                // Don't propagate execution errors; continue processing
                Ok(())
            }
            Err(e) => {
                error!(
                    error = %e,
                    "Task join error during plan execution"
                );
                // Don't propagate task errors; continue processing
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xzepr::consumer::message::{CloudEventData, EventEntity};
    use anyhow::anyhow;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::Mutex;

    #[test]
    fn test_watcher_error_display() {
        let err = WatcherError::Config("test error".to_string());
        assert_eq!(err.to_string(), "Configuration error: test error");

        let err = WatcherError::Consumer("kafka failed".to_string());
        assert_eq!(err.to_string(), "Consumer error: kafka failed");

        let err = WatcherError::Filter("invalid filter".to_string());
        assert_eq!(err.to_string(), "Filter error: invalid filter");

        let err = WatcherError::PlanExtraction("no plan found".to_string());
        assert_eq!(err.to_string(), "Plan extraction error: no plan found");

        let err = WatcherError::Execution("execution timeout".to_string());
        assert_eq!(err.to_string(), "Execution error: execution timeout");
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
            },
            agent: crate::config::AgentConfig::default(),
            watcher: crate::config::WatcherConfig {
                kafka: None,
                filters: Default::default(),
                logging: Default::default(),
                execution: Default::default(),
            },
        };

        let result = Watcher::new(config, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_watcher_creation_with_valid_config() {
        let mut config = Config::default();
        config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "test-topic".to_string(),
            group_id: "test-group".to_string(),
            security: None,
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
            brokers: "localhost:9092".to_string(),
            topic: "test-topic".to_string(),
            group_id: "test-group".to_string(),
            security: None,
        });

        let result = Watcher::new(config, true);
        assert!(result.is_ok());
        let watcher = result.unwrap();
        assert!(watcher.dry_run);
    }

    #[test]
    fn test_watcher_execution_config_defaults() {
        let config = Config::default();
        assert!(!config.watcher.execution.allow_dangerous);
        assert_eq!(config.watcher.execution.max_concurrent_executions, 1);
        assert_eq!(config.watcher.execution.execution_timeout_secs, 300);
    }

    // ---------------------------
    // Test helpers & mocks
    // ---------------------------

    /// A simple mock plan executor used in tests.
    struct MockExecutor {
        pub calls: Arc<tokio::sync::Mutex<Vec<String>>>,
        pub should_fail: bool,
        pub delay_ms: Option<u64>,
    }

    struct FailingExtractor;

    impl crate::watcher::PlanExtractorTrait for FailingExtractor {
        fn extract(&self, _event: &CloudEventMessage) -> Result<String> {
            Err(anyhow!("forced extraction failure"))
        }
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
                delay_ms: None,
            }
        }

        fn with_failure() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                should_fail: true,
                delay_ms: None,
            }
        }

        fn with_delay(ms: u64) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                should_fail: false,
                delay_ms: Some(ms),
            }
        }
    }

    #[async_trait::async_trait]
    impl PlanExecutor for MockExecutor {
        async fn execute_plan(
            &self,
            _config: crate::config::Config,
            plan: String,
            _allow_dangerous: bool,
        ) -> Result<()> {
            self.calls.lock().await.push(plan);
            if let Some(ms) = self.delay_ms {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            }
            if self.should_fail {
                Err(anyhow!("mock failure"))
            } else {
                Ok(())
            }
        }
    }

    // ---------------------------
    // Watcher handler unit tests
    // ---------------------------

    #[tokio::test]
    async fn test_handle_skips_filtered_event() {
        let filter_config = crate::config::EventFilterConfig {
            event_types: vec!["deployment.success".to_string()],
            source_pattern: None,
            platform_id: None,
            package: None,
            api_version: None,
            success_only: false,
        };
        let filter = Arc::new(EventFilter::new(filter_config).unwrap());
        let extractor = Arc::new(PlanExtractor::new());
        let sem = Arc::new(Semaphore::new(1));

        let mock = Arc::new(MockExecutor::new());
        let handler = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig::default(),
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: false,
            executor: mock.clone(),
        };

        let message = CloudEventMessage {
            success: true,
            id: "evt-1".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "other.event".to_string(),
            source: "src".to_string(),
            api_version: "v1".to_string(),
            name: "n".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "p".to_string(),
            package: "pkg".to_string(),
            data: CloudEventData::default(),
        };

        handler.handle(message).await.unwrap();

        let calls = mock.calls.lock().await;
        assert!(
            calls.is_empty(),
            "executor should not be called for filtered events"
        );
    }

    #[tokio::test]
    async fn test_handle_extraction_failure_does_not_execute() {
        let filter =
            Arc::new(EventFilter::new(crate::config::EventFilterConfig::default()).unwrap());
        let extractor = Arc::new(FailingExtractor {});
        let sem = Arc::new(Semaphore::new(1));
        let mock = Arc::new(MockExecutor::new());

        // Create an event where payload is a number -> parse_plan_from_json will fail
        let data = CloudEventData {
            events: vec![EventEntity {
                id: "e1".to_string(),
                name: "ev".to_string(),
                version: "1.0".to_string(),
                release: "r".to_string(),
                platform_id: "p".to_string(),
                package: "pkg".to_string(),
                description: "d".to_string(),
                payload: json!(123),
                success: true,
                event_receiver_id: "rid".to_string(),
                created_at: Utc::now(),
            }],
            event_receivers: vec![],
            event_receiver_groups: vec![],
        };

        let message = CloudEventMessage {
            success: true,
            id: "evt-2".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "src".to_string(),
            api_version: "v1".to_string(),
            name: "n".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "p".to_string(),
            package: "pkg".to_string(),
            data,
        };

        let handler = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig::default(),
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: false,
            executor: mock.clone(),
        };

        handler.handle(message).await.unwrap();
        let calls = mock.calls.lock().await;
        assert!(
            calls.is_empty(),
            "executor should not be called when extraction fails"
        );
    }

    #[tokio::test]
    async fn test_handle_dry_run_skips_execution() {
        let filter =
            Arc::new(EventFilter::new(crate::config::EventFilterConfig::default()).unwrap());
        let extractor = Arc::new(PlanExtractor::new());
        let sem = Arc::new(Semaphore::new(1));
        let mock = Arc::new(MockExecutor::new());

        // Valid plan inside payload.plan
        let event_payload = json!({"plan": "- task: hello\n  commands: echo hi"});
        let data = CloudEventData {
            events: vec![EventEntity {
                id: "e1".to_string(),
                name: "ev".to_string(),
                version: "1.0".to_string(),
                release: "r".to_string(),
                platform_id: "p".to_string(),
                package: "pkg".to_string(),
                description: "d".to_string(),
                payload: event_payload,
                success: true,
                event_receiver_id: "rid".to_string(),
                created_at: Utc::now(),
            }],
            event_receivers: vec![],
            event_receiver_groups: vec![],
        };

        let message = CloudEventMessage {
            success: true,
            id: "evt-3".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "src".to_string(),
            api_version: "v1".to_string(),
            name: "n".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "p".to_string(),
            package: "pkg".to_string(),
            data,
        };

        let mut watcher_cfg = crate::config::WatcherConfig::default();
        watcher_cfg.execution.allow_dangerous = false;

        let handler = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: watcher_cfg,
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: true,
            executor: mock.clone(),
        };

        handler.handle(message).await.unwrap();

        let calls = mock.calls.lock().await;
        assert!(
            calls.is_empty(),
            "executor should not be called in dry-run mode"
        );
    }

    #[tokio::test]
    async fn test_execution_success_and_failure_are_handled_gracefully() {
        let filter =
            Arc::new(EventFilter::new(crate::config::EventFilterConfig::default()).unwrap());
        let extractor = Arc::new(PlanExtractor::new());
        let sem = Arc::new(Semaphore::new(1));

        // Success case
        let executor_ok = Arc::new(MockExecutor::new());
        let event_payload = json!({"plan": "- task: echo\n  commands: echo ok"});
        let data = CloudEventData {
            events: vec![EventEntity {
                id: "e_ok".to_string(),
                name: "ev".to_string(),
                version: "1.0".to_string(),
                release: "r".to_string(),
                platform_id: "p".to_string(),
                package: "pkg".to_string(),
                description: "d".to_string(),
                payload: event_payload,
                success: true,
                event_receiver_id: "rid".to_string(),
                created_at: Utc::now(),
            }],
            event_receivers: vec![],
            event_receiver_groups: vec![],
        };

        let message_ok = CloudEventMessage {
            success: true,
            id: "evt-ok".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "src".to_string(),
            api_version: "v1".to_string(),
            name: "n".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "p".to_string(),
            package: "pkg".to_string(),
            data: data.clone(),
        };

        let handler_ok = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig::default(),
            filter: filter.clone(),
            extractor: extractor.clone(),
            execution_semaphore: sem.clone(),
            dry_run: false,
            executor: executor_ok.clone(),
        };

        handler_ok.handle(message_ok).await.unwrap();
        let calls_ok = executor_ok.calls.lock().await;
        assert_eq!(
            calls_ok.len(),
            1,
            "executor should be called on successful execution"
        );

        // Failure case
        let executor_fail = Arc::new(MockExecutor::with_failure());
        let message_fail = CloudEventMessage {
            success: true,
            id: "evt-fail".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "src".to_string(),
            api_version: "v1".to_string(),
            name: "n".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "p".to_string(),
            package: "pkg".to_string(),
            data,
        };

        let handler_fail = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig::default(),
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: false,
            executor: executor_fail.clone(),
        };

        // Should not propagate executor errors (handler returns Ok)
        handler_fail.handle(message_fail).await.unwrap();
        let calls_fail = executor_fail.calls.lock().await;
        assert_eq!(
            calls_fail.len(),
            1,
            "executor should be called even if it fails"
        );
    }

    // ---------------------------
    // Concurrency / performance tests
    // ---------------------------

    #[tokio::test]
    async fn test_concurrent_execution_limits() {
        let filter =
            Arc::new(EventFilter::new(crate::config::EventFilterConfig::default()).unwrap());
        let extractor = Arc::new(PlanExtractor::new());

        // Limit concurrent executions to 2
        let sem = Arc::new(Semaphore::new(2));

        // Executor that delays to simulate work
        let mock = Arc::new(MockExecutor::with_delay(100));

        let handler = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig {
                kafka: None,
                filters: Default::default(),
                logging: Default::default(),
                execution: crate::config::WatcherExecutionConfig {
                    allow_dangerous: false,
                    max_concurrent_executions: 2,
                    execution_timeout_secs: 60,
                },
            },
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: false,
            executor: mock.clone(),
        };

        let event_payload = json!({"plan": "- task: echo\n  commands: sleep 0.1"});
        let data = CloudEventData {
            events: vec![EventEntity {
                id: "e".to_string(),
                name: "ev".to_string(),
                version: "1.0".to_string(),
                release: "r".to_string(),
                platform_id: "p".to_string(),
                package: "pkg".to_string(),
                description: "d".to_string(),
                payload: event_payload,
                success: true,
                event_receiver_id: "rid".to_string(),
                created_at: Utc::now(),
            }],
            event_receivers: vec![],
            event_receiver_groups: vec![],
        };

        // Spawn 4 handler invocations concurrently
        let n = 4usize;
        let mut handles = Vec::new();
        let start = Instant::now();
        for i in 0..n {
            let h = handler.clone();
            let message = CloudEventMessage {
                success: true,
                id: format!("evt-{}", i),
                specversion: "1.0.1".to_string(),
                event_type: "test.event".to_string(),
                source: "src".to_string(),
                api_version: "v1".to_string(),
                name: "n".to_string(),
                version: "1.0.0".to_string(),
                release: "1.0.0".to_string(),
                platform_id: "p".to_string(),
                package: "pkg".to_string(),
                data: data.clone(),
            };

            handles.push(tokio::spawn(async move {
                h.handle(message).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let elapsed = start.elapsed();
        // With max_concurrent = 2 and each task ~100ms, 4 tasks should take around 200ms (allow margin)
        assert!(
            elapsed.as_millis() >= 180,
            "expected concurrent throttling to take >=180ms, got {}ms",
            elapsed.as_millis()
        );

        let calls = mock.calls.lock().await;
        assert_eq!(
            calls.len(),
            n,
            "executor should have been invoked for each message"
        );
    }

    // ---------------------------
    // Watcher helpers tests
    // ---------------------------

    #[test]
    fn test_apply_security_config_invalid_protocol() {
        let cfg = crate::xzepr::consumer::config::KafkaConsumerConfig::new(
            "localhost:9092",
            "topic",
            "svc",
        );
        let sec = crate::config::KafkaSecurityConfig {
            protocol: "BAD_PROTOCOL".to_string(),
            sasl_mechanism: None,
            sasl_username: None,
            sasl_password: None,
        };

        let res = Watcher::apply_security_config(cfg, &sec);
        assert!(res.is_err());
    }

    #[test]
    fn test_apply_security_config_missing_sasl_password() {
        // Save any existing value and then clear the environment so the fallback lookup will fail.
        let prev = std::env::var("KAFKA_SASL_PASSWORD").ok();
        std::env::remove_var("KAFKA_SASL_PASSWORD");

        let cfg = crate::xzepr::consumer::config::KafkaConsumerConfig::new(
            "localhost:9092",
            "topic",
            "svc",
        );
        let sec = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: None,
        };

        let res = Watcher::apply_security_config(cfg, &sec);
        assert!(res.is_err(), "missing SASL password should cause an error");

        // Restore prior environment state to avoid affecting other tests.
        if let Some(val) = prev {
            std::env::set_var("KAFKA_SASL_PASSWORD", val);
        } else {
            std::env::remove_var("KAFKA_SASL_PASSWORD");
        }
    }

    #[test]
    fn test_apply_security_config_invalid_sasl_mechanism() {
        let cfg = crate::xzepr::consumer::config::KafkaConsumerConfig::new(
            "localhost:9092",
            "topic",
            "svc",
        );
        let sec = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("BAD_MECH".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: Some("pass".to_string()),
        };

        let res = Watcher::apply_security_config(cfg, &sec);
        assert!(res.is_err(), "invalid SASL mechanism should cause an error");
    }

    #[test]
    fn test_apply_security_config_valid_sasl() {
        let cfg = crate::xzepr::consumer::config::KafkaConsumerConfig::new(
            "localhost:9092",
            "topic",
            "svc",
        );
        let sec = crate::config::KafkaSecurityConfig {
            protocol: "SASL_SSL".to_string(),
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: Some("user".to_string()),
            sasl_password: Some("pass".to_string()),
        };

        let applied = Watcher::apply_security_config(cfg, &sec)
            .expect("expected success for valid SASL config");
        assert!(applied.sasl_config.is_some());
        let sasl = applied.sasl_config.unwrap();
        assert_eq!(sasl.username, "user");
        assert_eq!(sasl.password, "pass");
        assert_eq!(
            sasl.mechanism,
            crate::xzepr::consumer::config::SaslMechanism::Plain
        );
    }

    #[tokio::test]
    async fn test_process_message_invokes_handler_and_executor() {
        // Prepare a WatcherMessageHandler wrapped in a bridge for the consumer stub
        let filter =
            Arc::new(EventFilter::new(crate::config::EventFilterConfig::default()).unwrap());
        let extractor = Arc::new(PlanExtractor::new());
        let sem = Arc::new(Semaphore::new(1));
        let mock = Arc::new(MockExecutor::new());

        let handler_inner = WatcherMessageHandler {
            config: Arc::new(Config::default()),
            watcher_config: crate::config::WatcherConfig::default(),
            filter,
            extractor,
            execution_semaphore: sem,
            dry_run: false,
            executor: mock.clone(),
        };

        struct Bridge {
            inner: WatcherMessageHandler,
        }

        #[async_trait::async_trait]
        impl MessageHandler for Bridge {
            async fn handle(
                &self,
                message: CloudEventMessage,
            ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                self.inner.handle(message).await
            }
        }

        let bridge = Bridge {
            inner: handler_inner,
        };

        // Construct JSON payload with plan
        let payload = r#"{
            "success": true,
            "id": "test-id",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "test-source",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0",
            "platform_id": "test",
            "package": "testpkg",
            "data": {
                "events": [{
                    "id": "e1",
                    "name": "ev",
                    "version": "1.0",
                    "release": "r",
                    "platform_id": "p",
                    "package": "pkg",
                    "description": "d",
                    "payload": {"plan": "- task: run\\n  commands: echo hi"},
                    "success": true,
                    "event_receiver_id": "rid",
                    "created_at": "2025-01-15T10:30:00Z"
                }],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }"#;

        let _ =
            crate::xzepr::consumer::kafka::XzeprConsumer::process_message(payload, &bridge).await;
        let calls = mock.calls.lock().await;
        assert_eq!(
            calls.len(),
            1,
            "executor should be invoked when processing a message with a plan"
        );
    }
}
