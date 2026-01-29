//! Watcher service for consuming and processing CloudEvents from Kafka
//!
//! This module provides the core watcher service that:
//! 1. Connects to Kafka topics
//! 2. Consumes CloudEvents messages
//! 3. Filters events based on configuration
//! 4. Extracts plans from event payloads
//! 5. Executes extracted plans with concurrency control

use crate::config::{Config, WatcherConfig};
use crate::watcher::{EventFilter, PlanExtractor};
use crate::xzepr::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

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
    extractor: Arc<PlanExtractor>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
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
        let extractor = Arc::new(PlanExtractor::new());

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
    extractor: Arc<PlanExtractor>,
    execution_semaphore: Arc<Semaphore>,
    dry_run: bool,
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

        // Spawn plan execution in background task
        let execution_task = tokio::spawn(async move {
            debug!("Plan execution task started");

            let result = crate::commands::r#run::run_plan_with_options(
                config,
                None,
                Some(plan_yaml),
                allow_dangerous,
            )
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
}
