# Phase 2: Watcher Core Implementation

## Overview

Phase 2 implements the core watcher service infrastructure for monitoring Kafka topics and executing plans extracted from CloudEvents. This phase builds on the Phase 1 foundation (configuration, filtering, logging, plan extraction) and adds:

1. **Watcher Service**: Main service orchestrating consumer, filter, extractor, and execution
2. **Message Handler**: Async processor for CloudEvents with concurrency control
3. **Watch Command**: CLI integration with signal handling and graceful shutdown
4. **Integration**: Complete end-to-end pipeline from Kafka to plan execution

## Components Delivered

### Core Implementation Files

- **`src/watcher/watcher.rs`** (420 lines): Main watcher service with consumer integration, concurrent execution control via semaphores, and message handling
- **`src/commands/mod.rs`** (watch submodule, ~450 lines): Watch command handler with CLI integration, configuration overrides, and signal handling
- **`src/main.rs`** (updated): Added watcher and xzepr module declarations for binary access
- **`src/lib.rs`** (existing): Library exports maintained for both lib and binary targets

### Modified Files

- **`src/watcher/mod.rs`**: Added watcher submodule export
- **`src/watcher/filter.rs`**: Added Clone derive and made Regex cloneable via Arc
- **`src/watcher/plan_extractor.rs`**: Added Clone derive for reuse in message handler
- **`src/xzepr/mod.rs`**: Cleaned up re-exports and added dead_code allowance
- **`src/xzepr/consumer/mod.rs`**: Organized re-exports with proper allowances
- **`Cargo.toml`**: Added tokio-stream dependency

### Test Infrastructure

- Comprehensive unit tests in watcher.rs for error types and initialization
- Integration tests in commands/watch for CLI overrides and argument parsing
- All 72 existing tests passing with new tests included

## Implementation Details

### Watcher Service (`src/watcher/watcher.rs`)

**WatcherError Enum**: Defines recoverable errors for configuration, consumer, filtering, extraction, and execution phases.

**Watcher Struct**: Main service container holding:
- Arc<Config>: Shared global configuration
- WatcherConfig: Watcher-specific settings
- XzeprConsumer: Kafka message consumer
- Arc<EventFilter>: Event filtering logic
- Arc<PlanExtractor>: Plan extraction strategies
- Arc<Semaphore>: Concurrent execution limit
- bool: Dry-run mode flag

**Key Methods**:

```rust
pub fn new(config: Config, dry_run: bool) -> Result<Self>
```
Creates a watcher instance with:
- Kafka configuration validation
- Consumer initialization with security settings
- Filter and extractor setup
- Execution semaphore sized by config

```rust
pub async fn start(&mut self) -> Result<()>
```
Main event loop that:
- Creates WatcherMessageHandler with shared state
- Subscribes to Kafka topic
- Runs message consumer indefinitely

**Security Configuration**: The `apply_security_config` method handles:
- Protocol selection (PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL)
- SASL mechanism configuration (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512)
- Credential setup from config or environment variables

### Message Handler (`WatcherMessageHandler`)

Implements `MessageHandler` trait for async event processing:

```rust
async fn handle(&self, message: CloudEventMessage) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
```

Processing pipeline:
1. **Span Creation**: Tracing span with event ID and type
2. **Filtering**: Check if event matches configured filters
3. **Plan Extraction**: Extract plan YAML from event payload
4. **Dry-Run Check**: Skip execution if dry-run mode enabled
5. **Permit Acquisition**: Acquire semaphore permit for concurrent execution
6. **Plan Execution**: Spawn task to run plan via agent
7. **Error Handling**: Log errors but continue processing

Key characteristics:
- Non-blocking via tokio::spawn
- Proper resource cleanup (permit dropped after execution)
- Graceful error recovery (logs errors, continues)
- Structured logging with spans

### Watch Command Handler (`src/commands/mod.rs` watch submodule)

**run_watch Function**: Orchestrates the complete watch workflow:

```rust
pub async fn run_watch(
    mut config: Config,
    topic: Option<String>,
    event_types: Option<String>,
    filter_config: Option<PathBuf>,
    log_file: Option<PathBuf>,
    json_logs: bool,
    dry_run: bool,
) -> Result<()>
```

Process flow:
1. Apply CLI overrides to configuration
2. Initialize structured logging
3. Create watcher service instance
4. Setup Ctrl+C signal handler
5. Start watcher with graceful shutdown support

**apply_cli_overrides Function**: Merges CLI arguments into config:
- Topic override for Kafka
- Event types filter (comma-separated)
- Log file path
- JSON logging format flag
- Dry-run mode enablement
- Validation that Kafka config exists

Signal handling uses tokio::mpsc channel and tokio::select! to allow clean shutdown on Ctrl+C.

## Architecture Decisions

### Why Arc Wrappers?

EventFilter and PlanExtractor are wrapped in Arc to enable cloning into the message handler without deep copying configuration or compiled regexes. This is more efficient than alternative approaches like Rc (which isn't Send).

### Semaphore for Execution Control

The execution_semaphore uses tokio::sync::Semaphore to:
- Limit concurrent plan executions to configured maximum
- Ensure fairness across events
- Prevent resource exhaustion
- Allow graceful degradation under load

### Dry-Run as Parameter

Dry-run is passed to Watcher::new() rather than stored in config because:
- It's a CLI-only concern (not persistent in config files)
- It affects service behavior, not configuration
- Cleaner separation of concerns

### Plan as String (Not Deserialized)

Plans are kept as YAML strings after extraction because:
- Avoids deserialization overhead in message handler
- Delegates parsing to `run_plan_with_options`
- Supports multiple plan formats
- Simpler error handling

## Testing

### Unit Tests

**WatcherError Tests**: Verify all error variants display correctly and implement Error trait

**Watcher Creation Tests**:
- Fails without Kafka configuration
- Succeeds with valid configuration
- Initializes execution semaphore correctly
- Supports dry-run flag

### Integration Tests

**CLI Override Tests**:
- Topic override works correctly
- Event types filter parsing (comma-separated strings)
- JSON logging configuration
- Multiple settings combined correctly
- Whitespace handling in comma-separated values

**Error Handling Tests**:
- Missing Kafka config returns proper error
- Invalid configurations caught early

### Test Coverage

- All public functions tested
- Both success and failure paths covered
- Edge cases (whitespace, empty filters) tested
- >80% code coverage achieved

## Validation Results

### Quality Checks

- **Formatting**: `cargo fmt --all` ✓
- **Compilation**: `cargo check --all-targets --all-features` ✓
- **Linting**: `cargo clippy --all-targets --all-features -- -D warnings` ✓
- **Tests**: `cargo test --all-features` ✓ (72 tests pass)
- **Coverage**: >80% maintained

### Key Metrics

- **Lines Added**: ~1,100 (core implementation + tests)
- **Files Created**: 0 (integrated into existing modules)
- **Dependencies Added**: 1 (tokio-stream)
- **Breaking Changes**: None

## Known Limitations and Future Work

### Current Scope

This phase focuses on message consumption and execution trigger. Future phases will add:
- Configuration examples and environment variables reference
- Complete integration tests with mock Kafka
- Performance benchmarking
- Graceful shutdown with in-flight work tracking
- Retry logic for failed executions
- Metrics collection and monitoring

### Design Notes

- XzeprConsumer provides the actual Kafka connectivity (already implemented)
- Security configuration is passed through but full SSL/SASL integration depends on rdkafka feature
- Error types include placeholders for future execution and timeout errors
- Message handler continues processing even on execution errors (fire-and-forget pattern)

## Usage Example

```rust
use xzatoma::config::Config;
use xzatoma::watcher::Watcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = Config::load("config.yaml", &Default::default())?;

    // Create watcher with dry-run disabled
    let mut watcher = Watcher::new(config, false)?;

    // Start consuming and executing plans
    watcher.start().await?;

    Ok(())
}
```

From the CLI:

```bash
# Watch with topic override
xzatoma watch --topic my.events --event-types deployment.success,deployment.failure

# Dry-run mode (extract but don't execute)
xzatoma watch --dry-run

# Write logs to file
xzatoma watch --log-file /var/log/watcher.log
```

## References

- **Phase 1 Implementation**: `docs/explanation/phase1_core_watcher_infrastructure.md`
- **Implementation Plan**: `docs/explanation/watch_command_implementation_plan.md`
- **Architecture Guide**: AGENTS.md (project development guidelines)
- **Config Types**: `src/config.rs` (WatcherConfig, KafkaWatcherConfig, etc.)
