# Watch Command Implementation Plan

## Overview

This plan outlines the implementation of a new `watch` CLI command that enables XZatoma to operate in watcher mode. In watcher mode, XZatoma consumes CloudEvents messages from a Kafka topic (Redpanda), filters events based on configurable criteria, and executes plans contained in event payloads. The implementation leverages the existing XZepr consumer infrastructure while adding plan execution capabilities and structured logging.

## Current State Analysis

### Existing Infrastructure

XZatoma already has the following components that support this feature:

- **XZepr Consumer Module** (`src/xzepr/consumer/`): Complete Kafka consumer implementation with CloudEvents message parsing, authentication, and configuration
- **Plan Execution** (`src/commands/run.rs`): Existing `run_plan` and `run_plan_with_options` functions for executing YAML/JSON/Markdown plans
- **Plan Parser** (`src/tools/plan.rs`): Supports parsing plans from YAML, JSON, and Markdown formats
- **CLI Framework** (`src/cli.rs`): Clap-based command structure with existing commands (Chat, Run, Auth, Models)
- **Configuration System** (`src/config.rs`): YAML-based configuration with environment variable support
- **Logging Infrastructure**: tracing-subscriber with environment-based filtering

### Message Format

CloudEvents messages from XZepr have the following structure:

```rust
pub struct CloudEventMessage {
    pub success: bool,
    pub id: String,
    pub specversion: String,
    pub event_type: String,        // Primary filter field
    pub source: String,
    pub api_version: String,
    pub name: String,
    pub version: String,
    pub release: String,
    pub platform_id: String,
    pub package: String,
    pub data: CloudEventData,      // Contains plan payload
}
```

### Identified Requirements

1. **CLI Command**: New `watch` subcommand with configuration options
2. **Event Filtering**: Filter messages by `event_type` (minimum) and any CloudEventMessage fields (minus data)
3. **Plan Extraction**: Extract plan from event payload and validate format
4. **Plan Execution**: Execute extracted plans using existing run command logic
5. **Structured Logging**: JSON-formatted logs to STDOUT and file with correlation IDs
6. **Configuration**: Kafka connection, topic, filters, and logging settings
7. **Error Handling**: Graceful handling of invalid messages, parse failures, and execution errors
8. **Lifecycle Management**: Start, stop, and graceful shutdown capabilities

## Implementation Phases

### Phase 1: Core Watcher Infrastructure

#### Task 1.1: CLI Command and Configuration

Add `Watch` command to CLI with necessary options and configuration structure.

**File**: `src/cli.rs`

Add new command variant:

```rust
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    // ... existing commands ...

    /// Watch Kafka topic for events and execute plans
    Watch {
        /// Kafka topic to watch (overrides config)
        #[arg(short, long)]
        topic: Option<String>,

        /// Event types to process (comma-separated)
        #[arg(short = 'e', long)]
        event_types: Option<String>,

        /// Filter configuration file (YAML)
        #[arg(short = 'f', long)]
        filter_config: Option<PathBuf>,

        /// Log output file (defaults to STDOUT only)
        #[arg(short = 'l', long)]
        log_file: Option<PathBuf>,

        /// Enable JSON-formatted logging
        #[arg(long, default_value = "true")]
        json_logs: bool,

        /// Dry run mode (parse but don't execute plans)
        #[arg(long)]
        dry_run: bool,
    },
}
```

**File**: `src/config.rs`

Add watcher configuration structure:

```rust
/// Watcher configuration for Kafka event monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Kafka consumer configuration
    #[serde(default)]
    pub kafka: Option<KafkaWatcherConfig>,

    /// Event filtering configuration
    #[serde(default)]
    pub filters: EventFilterConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: WatcherLoggingConfig,

    /// Plan execution configuration
    #[serde(default)]
    pub execution: WatcherExecutionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaWatcherConfig {
    /// Kafka brokers (comma-separated)
    pub brokers: String,

    /// Topic to consume from
    pub topic: String,

    /// Consumer group ID
    #[serde(default = "default_watcher_group_id")]
    pub group_id: String,

    /// Security configuration
    #[serde(default)]
    pub security: Option<KafkaSecurityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaSecurityConfig {
    /// Security protocol (PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL)
    pub protocol: String,

    /// SASL mechanism (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512)
    pub sasl_mechanism: Option<String>,

    /// SASL username
    pub sasl_username: Option<String>,

    /// SASL password (prefer env var KAFKA_SASL_PASSWORD)
    pub sasl_password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventFilterConfig {
    /// Event types to process (if empty, process all)
    #[serde(default)]
    pub event_types: Vec<String>,

    /// Source pattern filter (regex)
    pub source_pattern: Option<String>,

    /// Platform ID filter
    pub platform_id: Option<String>,

    /// Package name filter
    pub package: Option<String>,

    /// API version filter
    pub api_version: Option<String>,

    /// Only process successful events
    #[serde(default = "default_success_only")]
    pub success_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherLoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Enable JSON-formatted logs
    #[serde(default = "default_json_logs")]
    pub json_format: bool,

    /// Log file path (if None, STDOUT only)
    pub file_path: Option<PathBuf>,

    /// Include full event payload in logs
    #[serde(default)]
    pub include_payload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherExecutionConfig {
    /// Allow dangerous operations in executed plans
    #[serde(default)]
    pub allow_dangerous: bool,

    /// Maximum concurrent plan executions
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_executions: usize,

    /// Execution timeout in seconds
    #[serde(default = "default_execution_timeout")]
    pub execution_timeout_secs: u64,
}

fn default_watcher_group_id() -> String {
    "xzatoma-watcher".to_string()
}

fn default_success_only() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_json_logs() -> bool {
    true
}

fn default_max_concurrent() -> usize {
    1
}

fn default_execution_timeout() -> usize {
    300
}
```

Add `watcher` field to main `Config` struct:

```rust
pub struct Config {
    pub provider: ProviderConfig,
    pub agent: AgentConfig,

    /// Watcher configuration
    #[serde(default)]
    pub watcher: WatcherConfig,
}
```

**Deliverables**:
- CLI command definition in `src/cli.rs`
- Configuration structures in `src/config.rs`
- Unit tests for configuration parsing

**Success Criteria**:
- `xzatoma watch --help` displays command documentation
- Configuration loads from YAML file with proper defaults
- Environment variables override YAML settings
- All clippy and fmt checks pass

#### Task 1.2: Event Filter Implementation

Create filtering logic to determine which events to process.

**File**: `src/watcher/filter.rs`

```rust
use crate::config::EventFilterConfig;
use crate::xzepr::CloudEventMessage;
use regex::Regex;
use anyhow::Result;

/// Event filter for determining which CloudEvents to process.
pub struct EventFilter {
    config: EventFilterConfig,
    source_regex: Option<Regex>,
}

impl EventFilter {
    /// Create a new event filter from configuration.
    pub fn new(config: EventFilterConfig) -> Result<Self> {
        let source_regex = if let Some(pattern) = &config.source_pattern {
            Some(Regex::new(pattern)?)
        } else {
            None
        };

        Ok(Self {
            config,
            source_regex,
        })
    }

    /// Check if an event should be processed based on filter criteria.
    pub fn should_process(&self, event: &CloudEventMessage) -> bool {
        // Filter by success flag
        if self.config.success_only && !event.success {
            return false;
        }

        // Filter by event type
        if !self.config.event_types.is_empty()
            && !self.config.event_types.contains(&event.event_type) {
            return false;
        }

        // Filter by source pattern
        if let Some(regex) = &self.source_regex {
            if !regex.is_match(&event.source) {
                return false;
            }
        }

        // Filter by platform_id
        if let Some(platform) = &self.config.platform_id {
            if &event.platform_id != platform {
                return false;
            }
        }

        // Filter by package
        if let Some(package) = &self.config.package {
            if &event.package != package {
                return false;
            }
        }

        // Filter by api_version
        if let Some(version) = &self.config.api_version {
            if &event.api_version != version {
                return false;
            }
        }

        true
    }

    /// Get filter summary for logging.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.config.event_types.is_empty() {
            parts.push(format!("types={}", self.config.event_types.join(",")));
        }

        if let Some(pattern) = &self.config.source_pattern {
            parts.push(format!("source~{}", pattern));
        }

        if let Some(platform) = &self.config.platform_id {
            parts.push(format!("platform={}", platform));
        }

        if self.config.success_only {
            parts.push("success=true".to_string());
        }

        if parts.is_empty() {
            "no filters (all events)".to_string()
        } else {
            parts.join(", ")
        }
    }
}
```

**Deliverables**:
- Event filter implementation
- Comprehensive unit tests covering all filter combinations
- Documentation with filter examples

**Success Criteria**:
- Filters correctly by event_type (single and multiple)
- Filters correctly by all CloudEventMessage fields (except data)
- Regex filtering works for source patterns
- Test coverage exceeds 80%

#### Task 1.3: Structured Logging Setup

Implement JSON-formatted structured logging with file output support.

**File**: `src/watcher/logging.rs`

```rust
use crate::config::WatcherLoggingConfig;
use anyhow::Result;
use std::fs::OpenOptions;
use std::sync::Arc;
use tracing::Level;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
    Layer,
};

/// Initialize watcher logging based on configuration.
pub fn init_watcher_logging(config: &WatcherLoggingConfig) -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))?;

    let registry = tracing_subscriber::registry().with(env_filter);

    if config.json_format {
        // JSON formatting
        let stdout_layer = fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(true);

        if let Some(file_path) = &config.file_path {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(Arc::new(file));

            registry
                .with(stdout_layer)
                .with(file_layer)
                .init();
        } else {
            registry.with(stdout_layer).init();
        }
    } else {
        // Human-readable formatting
        let stdout_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true);

        if let Some(file_path) = &config.file_path {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer()
                .with_writer(Arc::new(file));

            registry
                .with(stdout_layer)
                .with(file_layer)
                .init();
        } else {
            registry.with(stdout_layer).init();
        }
    }

    Ok(())
}

/// Create structured log fields for an event.
#[macro_export]
macro_rules! event_fields {
    ($event:expr) => {
        tracing::info_span!(
            "event",
            event_id = %$event.id,
            event_type = %$event.event_type,
            source = %$event.source,
            platform_id = %$event.platform_id,
            package = %$event.package,
            success = %$event.success
        )
    };
}
```

**Deliverables**:
- Logging initialization function
- Macro for structured event fields
- Configuration for dual output (STDOUT + file)
- Tests for logging setup

**Success Criteria**:
- JSON logs include event correlation IDs
- Logs written to both STDOUT and file when configured
- Log rotation handled gracefully
- No performance degradation from logging

#### Task 1.4: Plan Extraction from Events

Create module to extract and validate plans from event payloads.

**File**: `src/watcher/plan_extractor.rs`

```rust
use crate::tools::plan::{Plan, PlanParser};
use crate::xzepr::CloudEventMessage;
use anyhow::{anyhow, Result};
use serde_json::Value as JsonValue;

/// Strategies for extracting plans from event data.
#[derive(Debug, Clone)]
pub enum PlanExtractionStrategy {
    /// Plan is in data.events[0].payload field
    EventPayload,

    /// Plan is in data.events[0].payload.plan field
    EventPayloadPlan,

    /// Plan is the entire data field
    DataRoot,

    /// Plan is in data.plan field
    DataPlan,
}

/// Extract plan from CloudEvent message.
pub struct PlanExtractor {
    strategies: Vec<PlanExtractionStrategy>,
}

impl PlanExtractor {
    /// Create new plan extractor with default strategies.
    pub fn new() -> Self {
        Self {
            strategies: vec![
                PlanExtractionStrategy::EventPayloadPlan,
                PlanExtractionStrategy::EventPayload,
                PlanExtractionStrategy::DataPlan,
                PlanExtractionStrategy::DataRoot,
            ],
        }
    }

    /// Extract plan from CloudEvent message.
    pub fn extract(&self, event: &CloudEventMessage) -> Result<Plan> {
        for strategy in &self.strategies {
            if let Ok(plan) = self.try_extract(event, strategy) {
                tracing::debug!(
                    strategy = ?strategy,
                    "Successfully extracted plan using strategy"
                );
                return Ok(plan);
            }
        }

        Err(anyhow!(
            "Failed to extract plan from event {} using any strategy",
            event.id
        ))
    }

    fn try_extract(
        &self,
        event: &CloudEventMessage,
        strategy: &PlanExtractionStrategy,
    ) -> Result<Plan> {
        let json_value = match strategy {
            PlanExtractionStrategy::EventPayload => {
                event.data.events.first()
                    .ok_or_else(|| anyhow!("No events in data"))?
                    .payload
                    .clone()
            }
            PlanExtractionStrategy::EventPayloadPlan => {
                let event_entity = event.data.events.first()
                    .ok_or_else(|| anyhow!("No events in data"))?;

                event_entity.payload
                    .get("plan")
                    .ok_or_else(|| anyhow!("No plan field in event payload"))?
                    .clone()
            }
            PlanExtractionStrategy::DataRoot => {
                serde_json::to_value(&event.data)?
            }
            PlanExtractionStrategy::DataPlan => {
                serde_json::to_value(&event.data)?
                    .get("plan")
                    .ok_or_else(|| anyhow!("No plan field in data"))?
                    .clone()
            }
        };

        self.parse_plan_from_json(&json_value)
    }

    fn parse_plan_from_json(&self, value: &JsonValue) -> Result<Plan> {
        // Try parsing as YAML string first
        if let Some(plan_str) = value.as_str() {
            if let Ok(plan) = PlanParser::from_yaml(plan_str) {
                return Ok(plan);
            }

            if let Ok(plan) = PlanParser::from_json(plan_str) {
                return Ok(plan);
            }

            if let Ok(plan) = PlanParser::from_markdown(plan_str) {
                return Ok(plan);
            }
        }

        // Try parsing as JSON object
        let json_str = serde_json::to_string(value)?;
        if let Ok(plan) = PlanParser::from_json(&json_str) {
            return Ok(plan);
        }

        Err(anyhow!("Could not parse plan from JSON value"))
    }
}

impl Default for PlanExtractor {
    fn default() -> Self {
        Self::new()
    }
}
```

**Deliverables**:
- Plan extraction with multiple strategies
- Support for YAML, JSON, and Markdown plans in payloads
- Comprehensive tests with various payload formats
- Documentation of supported payload structures

**Success Criteria**:
- Extracts plans from all supported locations in event data
- Handles missing or malformed plans gracefully
- Test coverage exceeds 80%
- Logs extraction strategy used for debugging

#### Task 1.5: Module Organization

Create watcher module structure.

**File**: `src/watcher/mod.rs`

```rust
//! Watcher module for monitoring Kafka topics and executing plans.
//!
//! This module provides functionality to watch Kafka topics for CloudEvents
//! messages, filter events based on configured criteria, extract plans from
//! event payloads, and execute those plans.

pub mod filter;
pub mod logging;
pub mod plan_extractor;
pub mod watcher;

pub use filter::EventFilter;
pub use plan_extractor::{PlanExtractor, PlanExtractionStrategy};
pub use watcher::{Watcher, WatcherError};
```

Update `src/lib.rs`:

```rust
pub mod watcher;
```

**Deliverables**:
- Module organization with proper exports
- Documentation at module level
- Re-exports for public API

**Success Criteria**:
- Clean module structure following XZatoma conventions
- No circular dependencies
- Public API clearly defined

### Phase 2: Watcher Core Implementation

#### Task 2.1: Watcher Service

Implement main watcher service that ties together consumer, filter, and executor.

**File**: `src/watcher/watcher.rs`

```rust
use crate::config::{Config, WatcherConfig};
use crate::watcher::{EventFilter, PlanExtractor};
use crate::xzepr::{CloudEventMessage, KafkaConsumerConfig, MessageHandler, XzeprConsumer};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Semaphore;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Consumer error: {0}")]
    Consumer(String),

    #[error("Filter error: {0}")]
    Filter(String),

    #[error("Plan extraction error: {0}")]
    PlanExtraction(String),

    #[error("Execution error: {0}")]
    Execution(String),
}

/// Main watcher service for processing CloudEvents.
pub struct Watcher {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    consumer: XzeprConsumer,
    filter: EventFilter,
    extractor: PlanExtractor,
    execution_semaphore: Arc<Semaphore>,
}

impl Watcher {
    /// Create new watcher instance.
    pub fn new(config: Config) -> Result<Self> {
        let watcher_config = config.watcher.clone();

        // Validate watcher configuration
        let kafka_config = watcher_config.kafka
            .as_ref()
            .ok_or_else(|| WatcherError::Config(
                "Kafka configuration is required".to_string()
            ))?;

        // Build Kafka consumer config
        let consumer_config = KafkaConsumerConfig::new(
            kafka_config.brokers.clone(),
            kafka_config.topic.clone(),
            "xzatoma".to_string(),
        )
        .with_group_id(kafka_config.group_id.clone());

        // Apply security settings if configured
        let consumer_config = if let Some(security) = &kafka_config.security {
            Self::apply_security_config(consumer_config, security)?
        } else {
            consumer_config
        };

        // Create consumer
        let consumer = XzeprConsumer::new(consumer_config)
            .map_err(|e| WatcherError::Consumer(e.to_string()))?;

        // Create filter
        let filter = EventFilter::new(watcher_config.filters.clone())
            .map_err(|e| WatcherError::Filter(e.to_string()))?;

        // Create plan extractor
        let extractor = PlanExtractor::new();

        // Create execution semaphore for concurrency control
        let execution_semaphore = Arc::new(Semaphore::new(
            watcher_config.execution.max_concurrent_executions
        ));

        Ok(Self {
            config: Arc::new(config),
            watcher_config,
            consumer,
            filter,
            extractor,
            execution_semaphore,
        })
    }

    /// Start watching for events.
    pub async fn start(&mut self) -> Result<()> {
        tracing::info!(
            topic = %self.consumer.topic(),
            filters = %self.filter.summary(),
            "Starting watcher"
        );

        self.consumer.subscribe()
            .map_err(|e| WatcherError::Consumer(e.to_string()))?;

        let handler = WatcherMessageHandler {
            config: self.config.clone(),
            watcher_config: self.watcher_config.clone(),
            filter: self.filter.clone(),
            extractor: self.extractor.clone(),
            execution_semaphore: self.execution_semaphore.clone(),
        };

        self.consumer.run(handler).await
            .map_err(|e| WatcherError::Consumer(e.to_string()))?;

        Ok(())
    }

    fn apply_security_config(
        config: KafkaConsumerConfig,
        security: &KafkaSecurityConfig,
    ) -> Result<KafkaConsumerConfig> {
        // Security configuration application
        // This would integrate with existing XZepr consumer security config
        Ok(config)
    }
}

/// Message handler for watcher events.
struct WatcherMessageHandler {
    config: Arc<Config>,
    watcher_config: WatcherConfig,
    filter: EventFilter,
    extractor: PlanExtractor,
    execution_semaphore: Arc<Semaphore>,
}

#[async_trait]
impl MessageHandler for WatcherMessageHandler {
    async fn handle(&self, message: CloudEventMessage) -> Result<()> {
        let _span = tracing::info_span!(
            "handle_event",
            event_id = %message.id,
            event_type = %message.event_type,
        ).entered();

        tracing::debug!("Received event");

        // Apply filters
        if !self.filter.should_process(&message) {
            tracing::debug!("Event filtered out");
            return Ok(());
        }

        tracing::info!("Event passed filters, extracting plan");

        // Extract plan
        let plan = match self.extractor.extract(&message) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to extract plan from event"
                );
                return Err(e);
            }
        };

        tracing::info!(
            plan_name = %plan.name,
            steps = plan.step_count(),
            "Extracted plan"
        );

        // Check dry-run mode
        if self.watcher_config.execution.dry_run {
            tracing::info!("Dry-run mode: skipping execution");
            return Ok(());
        }

        // Acquire semaphore for execution
        let permit = self.execution_semaphore.acquire().await
            .map_err(|e| anyhow!("Failed to acquire execution permit: {}", e))?;

        // Execute plan
        let config = (*self.config).clone();
        let allow_dangerous = self.watcher_config.execution.allow_dangerous;

        let result = tokio::spawn(async move {
            crate::commands::r#run::run_plan_with_options(
                config,
                None,
                Some(serde_yaml::to_string(&plan).unwrap()),
                allow_dangerous,
            ).await
        }).await;

        drop(permit);

        match result {
            Ok(Ok(())) => {
                tracing::info!("Plan executed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::error!(
                    error = %e,
                    "Plan execution failed"
                );
                Err(e)
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Task join error"
                );
                Err(anyhow!("Task join error: {}", e))
            }
        }
    }
}
```

**Deliverables**:
- Complete watcher service implementation
- Integration with XZepr consumer
- Concurrent execution control
- Error handling and logging

**Success Criteria**:
- Successfully consumes from Kafka topic
- Applies filters correctly
- Extracts and executes plans
- Handles errors gracefully without crashing

#### Task 2.2: Watch Command Handler

Create command handler that integrates watcher with CLI.

**File**: `src/commands/watch.rs`

```rust
use crate::config::Config;
use crate::watcher::{Watcher, logging::init_watcher_logging};
use anyhow::Result;

/// Run the watch command.
pub async fn run_watch(
    mut config: Config,
    topic: Option<String>,
    event_types: Option<String>,
    log_file: Option<PathBuf>,
    json_logs: bool,
    dry_run: bool,
) -> Result<()> {
    // Apply CLI overrides to config
    if let Some(topic_override) = topic {
        if let Some(ref mut kafka) = config.watcher.kafka {
            kafka.topic = topic_override;
        }
    }

    if let Some(types) = event_types {
        config.watcher.filters.event_types = types
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
    }

    if let Some(log_path) = log_file {
        config.watcher.logging.file_path = Some(log_path);
    }

    config.watcher.logging.json_format = json_logs;

    if dry_run {
        config.watcher.execution.dry_run = true;
    }

    // Initialize logging
    init_watcher_logging(&config.watcher.logging)?;

    tracing::info!("Initializing watcher");

    // Create and start watcher
    let mut watcher = Watcher::new(config)?;

    // Setup signal handlers for graceful shutdown
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tx.send(()).await.ok();
    });

    // Start watcher
    tokio::select! {
        result = watcher.start() => {
            result
        }
        _ = rx.recv() => {
            tracing::info!("Received shutdown signal");
            Ok(())
        }
    }
}
```

Update `src/commands/mod.rs`:

```rust
pub mod watch;
```

Update `src/main.rs` to handle Watch command:

```rust
match cli.command {
    // ... existing commands ...

    Commands::Watch {
        topic,
        event_types,
        filter_config,
        log_file,
        json_logs,
        dry_run,
    } => {
        tracing::info!("Starting watch mode");
        commands::watch::run_watch(
            config,
            topic,
            event_types,
            log_file,
            json_logs,
            dry_run,
        ).await?;
        Ok(())
    }
}
```

**Deliverables**:
- Watch command handler
- CLI integration in main.rs
- Signal handling for graceful shutdown
- Configuration override handling

**Success Criteria**:
- `xzatoma watch` command starts watcher successfully
- CLI arguments override config file settings
- Ctrl+C triggers graceful shutdown
- No resource leaks on shutdown

#### Task 2.3: Cargo Dependencies

Add required dependencies to `Cargo.toml`.

```toml
[dependencies]
# Existing dependencies...

# Kafka consumer (rdkafka for XZepr consumer)
rdkafka = { version = "0.36", features = ["cmake-build", "ssl", "sasl"] }

# Async runtime additions
tokio-stream = "0.1"

# Additional regex support
regex = "1.10"
```

**Deliverables**:
- Updated Cargo.toml with dependencies
- Dependency version compatibility verified
- Build successful with all features

**Success Criteria**:
- `cargo build --all-features` succeeds
- No dependency conflicts
- All features compile correctly

#### Task 2.4: Integration Tests

Create integration tests for watcher functionality.

**File**: `tests/watcher_integration_test.rs`

```rust
use xzatoma::config::{Config, WatcherConfig, EventFilterConfig};
use xzatoma::watcher::{EventFilter, PlanExtractor};
use xzatoma::xzepr::CloudEventMessage;

#[test]
fn test_event_filter_by_type() {
    let config = EventFilterConfig {
        event_types: vec!["deployment.success".to_string()],
        ..Default::default()
    };

    let filter = EventFilter::new(config).unwrap();

    let mut event = create_test_event();
    event.event_type = "deployment.success".to_string();
    assert!(filter.should_process(&event));

    event.event_type = "deployment.failure".to_string();
    assert!(!filter.should_process(&event));
}

#[test]
fn test_plan_extraction_from_event_payload() {
    let extractor = PlanExtractor::new();
    let event = create_test_event_with_plan();

    let plan = extractor.extract(&event).unwrap();
    assert_eq!(plan.name, "Test Plan");
    assert!(!plan.steps.is_empty());
}

fn create_test_event() -> CloudEventMessage {
    // Helper to create test CloudEvent
}

fn create_test_event_with_plan() -> CloudEventMessage {
    // Helper to create test CloudEvent with plan payload
}
```

**Deliverables**:
- Integration tests for filter logic
- Integration tests for plan extraction
- Mock event creation helpers
- End-to-end watcher tests with mock Kafka

**Success Criteria**:
- All integration tests pass
- Test coverage exceeds 80%
- Tests run in CI/CD pipeline

### Phase 3: Configuration and Documentation

#### Task 3.1: Configuration Examples

Create example configuration files for different use cases.

**File**: `config/watcher.yaml`

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5-mini

agent:
  max_turns: 10
  timeout_seconds: 300

watcher:
  kafka:
    brokers: "localhost:9092"
    topic: "xzepr.events"
    group_id: "xzatoma-watcher"
    security:
      protocol: "PLAINTEXT"

  filters:
    event_types:
      - "deployment.success"
      - "deployment.failure"
      - "ci.pipeline.completed"
    success_only: false
    platform_id: "kubernetes"

  logging:
    level: "info"
    json_format: true
    file_path: "/var/log/xzatoma/watcher.log"
    include_payload: false

  execution:
    allow_dangerous: false
    max_concurrent_executions: 5
    execution_timeout_secs: 600
```

**File**: `config/watcher-production.yaml`

```yaml
# Production configuration with SASL/SSL
provider:
  type: copilot

watcher:
  kafka:
    brokers: "kafka-1.prod:9093,kafka-2.prod:9093,kafka-3.prod:9093"
    topic: "xzepr.production.events"
    group_id: "xzatoma-watcher-prod"
    security:
      protocol: "SASL_SSL"
      sasl_mechanism: "SCRAM-SHA-256"
      sasl_username: "xzatoma-consumer"
      # Password from env: KAFKA_SASL_PASSWORD

  filters:
    event_types:
      - "deployment.production.success"
    success_only: true
    source_pattern: "^xzepr\\.event\\.receiver\\.production\\."

  logging:
    level: "warn"
    json_format: true
    file_path: "/var/log/xzatoma/watcher.json"

  execution:
    allow_dangerous: false
    max_concurrent_executions: 10
    execution_timeout_secs: 1800
```

**Deliverables**:
- Development configuration example
- Production configuration example
- Configuration with all options documented
- Environment variable examples

**Success Criteria**:
- Configuration examples are valid YAML
- All configuration options documented
- Examples cover common use cases

#### Task 3.2: Environment Variables Reference

Create reference documentation for environment variables.

**File**: `docs/reference/watcher_environment_variables.md`

```markdown
# Watcher Environment Variables Reference

## Kafka Configuration

- `KAFKA_BROKERS`: Kafka broker addresses (comma-separated)
- `KAFKA_TOPIC`: Topic to consume from
- `KAFKA_GROUP_ID`: Consumer group ID (default: xzatoma-watcher)
- `KAFKA_SECURITY_PROTOCOL`: Security protocol (PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL)
- `KAFKA_SASL_MECHANISM`: SASL mechanism (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512)
- `KAFKA_SASL_USERNAME`: SASL username
- `KAFKA_SASL_PASSWORD`: SASL password (sensitive)

## Filter Configuration

- `WATCHER_EVENT_TYPES`: Comma-separated list of event types to process
- `WATCHER_SOURCE_PATTERN`: Regex pattern for filtering source field
- `WATCHER_PLATFORM_ID`: Filter by platform ID
- `WATCHER_PACKAGE`: Filter by package name
- `WATCHER_SUCCESS_ONLY`: Only process successful events (true/false)

## Logging Configuration

- `WATCHER_LOG_LEVEL`: Log level (trace, debug, info, warn, error)
- `WATCHER_LOG_FILE`: Path to log file
- `WATCHER_JSON_LOGS`: Enable JSON formatting (true/false)

## Execution Configuration

- `WATCHER_ALLOW_DANGEROUS`: Allow dangerous operations (true/false)
- `WATCHER_MAX_CONCURRENT`: Maximum concurrent plan executions
- `WATCHER_EXECUTION_TIMEOUT`: Execution timeout in seconds
```

**Deliverables**:
- Complete environment variable reference
- Examples for each variable
- Security best practices documented

**Success Criteria**:
- All environment variables documented
- Examples provided for each variable
- Security considerations included

#### Task 3.3: How-To Guide

Create user guide for setting up and running watcher.

**File**: `docs/how-to/setup_watcher.md`

```markdown
# How to Set Up XZatoma Watcher

## Prerequisites

- Running Kafka/Redpanda cluster
- Access to XZepr event topic
- XZatoma installed and configured

## Basic Setup

### 1. Configure Kafka Connection

Create `config/watcher.yaml`:

```yaml
watcher:
  kafka:
    brokers: "localhost:9092"
    topic: "xzepr.events"
```

### 2. Configure Event Filters

Add filters to process specific event types:

```yaml
watcher:
  filters:
    event_types:
      - "deployment.success"
```

### 3. Start Watcher

```bash
xzatoma watch --config config/watcher.yaml
```

## Advanced Configuration

### Filter by Multiple Criteria

### Secure Connection with SASL/SSL

### Concurrent Execution

### Dry Run Mode

## Troubleshooting

### Common Issues

### Debugging Tips
```

**Deliverables**:
- Complete how-to guide
- Setup instructions for common scenarios
- Troubleshooting section
- Examples throughout

**Success Criteria**:
- Guide covers basic and advanced setups
- All examples are tested and working
- Troubleshooting section addresses common issues

#### Task 3.4: Architecture Documentation

Create architecture explanation for watcher system.

**File**: `docs/explanation/watcher_architecture.md`

```markdown
# Watcher Architecture

## Overview

The watcher system enables XZatoma to operate as an event-driven automation platform.

## Components

### Event Filter

Filters CloudEvents based on configurable criteria.

### Plan Extractor

Extracts execution plans from event payloads using multiple strategies.

### Watcher Service

Coordinates consumption, filtering, extraction, and execution.

## Data Flow

## Event Payload Formats

## Concurrency Control

## Error Handling Strategy
```

**Deliverables**:
- Architecture documentation
- Component diagrams (text-based)
- Data flow explanation
- Design decisions documented

**Success Criteria**:
- Architecture clearly explained
- Design decisions justified
- Integration points documented

### Phase 4: Testing and Validation

#### Task 4.1: Unit Tests

Comprehensive unit tests for all watcher components.

**Coverage Requirements**:
- Event filter: All filter combinations
- Plan extractor: All payload formats and strategies
- Watcher service: Initialization, error handling
- Configuration: Parsing, validation, overrides

**Deliverables**:
- Unit tests for all public functions
- Edge case coverage
- Error path testing
- Mock dependencies where needed

**Success Criteria**:
- Test coverage exceeds 80%
- All tests pass consistently
- No flaky tests

#### Task 4.2: Integration Tests

End-to-end testing with mock Kafka.

**Test Scenarios**:
- Event consumption and filtering
- Plan extraction from various payload formats
- Plan execution triggered by events
- Graceful shutdown on signal
- Error recovery and retry

**Deliverables**:
- Integration test suite
- Mock Kafka producer for test events
- Test fixtures for various event types
- CI/CD pipeline integration

**Success Criteria**:
- All integration tests pass
- Tests cover happy path and error scenarios
- Tests run in CI/CD without external dependencies

#### Task 4.3: Performance Testing

Validate watcher performance under load.

**Test Areas**:
- Event throughput (events/second)
- Concurrent execution handling
- Memory usage under sustained load
- Graceful degradation

**Deliverables**:
- Performance test suite
- Benchmarking results
- Resource usage documentation
- Performance tuning recommendations

**Success Criteria**:
- Handles expected event volume
- Memory usage stays within bounds
- No memory leaks detected
- Performance documented

#### Task 4.4: Quality Gates

Ensure all quality checks pass.

**Checks**:
```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

**Deliverables**:
- All formatting applied
- All clippy warnings resolved
- All tests passing
- Documentation complete

**Success Criteria**:
- Zero clippy warnings
- All tests pass
- Documentation review complete
- Code review completed

## Summary

This implementation plan delivers a production-ready `watch` command that enables XZatoma to operate as an event-driven automation platform. The implementation follows XZatoma's architecture principles, maintains high code quality standards, and provides comprehensive documentation.

### Key Features Delivered

1. **CLI Command**: `xzatoma watch` with flexible configuration options
2. **Event Filtering**: Filter by event_type and any CloudEventMessage fields
3. **Plan Extraction**: Multiple strategies for extracting plans from event payloads
4. **Structured Logging**: JSON-formatted logs to STDOUT and file with correlation IDs
5. **Concurrent Execution**: Controlled concurrent plan execution with semaphores
6. **Graceful Shutdown**: Signal handling for clean resource cleanup
7. **Comprehensive Testing**: Unit, integration, and performance tests
8. **Production-Ready**: Security, error handling, and monitoring capabilities

### Dependencies

#### New Rust Dependencies

- `rdkafka`: Kafka consumer client with SASL/SSL support
- `tokio-stream`: Async stream utilities for Kafka message handling
- `regex`: Pattern matching for event filtering (already in project)

### Implementation Timeline

- **Phase 1** (Core Infrastructure): 3-5 days
- **Phase 2** (Watcher Implementation): 3-4 days
- **Phase 3** (Configuration and Docs): 2-3 days
- **Phase 4** (Testing and Validation): 2-3 days

**Total Estimated Time**: 10-15 days

### References

- XZepr Consumer Implementation: `docs/explanation/downstream_consumer_implementation_plan.md`
- XZatoma Architecture: `docs/explanation/architecture.md`
- Plan Parsing: `src/tools/plan.rs`
- Run Command: `src/commands/run.rs`
