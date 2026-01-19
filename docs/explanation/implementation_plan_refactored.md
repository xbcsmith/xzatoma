# XZatoma Implementation Plan (Refactored)

## Overview

This document outlines a comprehensive phased approach to implement XZatoma, a **secure** autonomous AI agent CLI written in Rust. XZatoma connects to AI providers (GitHub Copilot or Ollama) and provides basic file/terminal tools with comprehensive security controls, allowing the AI to accomplish tasks through conversation rather than specialized features.

**Target**: 3,000-5,000 lines of production code with >80% test coverage

**Philosophy**: Build the simplest thing that works securely, then stop.

**Timeline**: 12 weeks to production-ready v1.0.0

## Critical Requirements

### Security First

- Terminal execution requires strict validation (denylist, allowlist, modes)
- Path validation prevents directory traversal
- Iteration limits prevent infinite loops
- Command timeout prevents hung processes
- Output limits prevent memory exhaustion

### Code Quality (Non-Negotiable)

- All file extensions: `.yaml` (NOT `.yml`)
- All markdown filenames: `lowercase_with_underscores.md` (except `README.md`)
- No emojis in documentation
- Test coverage: >80% mandatory
- All cargo quality gates must pass:
  - `cargo fmt --all`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`

### Architecture Alignment

This plan implements the architecture defined in `docs/reference/architecture.md` which has been validated and approved (9/10 score).

## Current State Analysis

### Existing Infrastructure

The project currently has:

- Complete architecture design in `docs/reference/architecture.md` (1,114 lines)
- Architecture validation completed (9/10 score)
- Competitive analysis completed
- Planning guidelines in `PLAN.md`
- Agent development guidelines in `AGENTS.md` (updated for XZatoma)
- Project structure defined
- Security model fully specified
- No implementation code yet

### Identified Issues

Critical items to address during implementation:

1. **Security is paramount** - Terminal execution requires comprehensive validation
2. **Conversation token management** - Must prevent context overflow with automatic pruning
3. **Iteration limits** - Must be enforced to prevent infinite loops
4. **Path validation** - Required to prevent directory traversal attacks
5. **Command denylist** - Must block dangerous operations (rm -rf /, dd, fork bombs, etc.)
6. **Configuration precedence** - CLI > ENV > File > Default must be implemented correctly
7. **Testing infrastructure** - Must achieve >80% coverage from start
8. **Error handling** - Comprehensive error types with helpful messages
9. **File extensions** - All YAML files must use `.yaml` (NOT `.yml`)
10. **Documentation** - Following Diataxis framework (tutorials, how-to, explanation, reference)

## Phase Restructuring Rationale

The original plan had phases in this order: Foundation → Providers → Agent → Tools → CLI → Polish

This refactored plan reorders to: Foundation → **Agent Core** → **Security** → Providers → Tools → CLI

**Why?**

1. **Agent core first** enables testing with mock providers before implementing real ones
2. **Security integrated early** ensures it's not an afterthought
3. **Providers after agent** allows thorough testing of agent logic independently
4. **Better dependency flow** - each phase builds on solid foundations

## Implementation Phases

### Phase 1: Foundation and Core Infrastructure

**Timeline**: Weeks 1-2

**Objective**: Establish Rust project foundation with error handling, configuration, and testing infrastructure.

**Total LOC Target**: ~800 lines (500 production + 300 tests)

#### Task 1.1: Project Initialization

**Description**: Create Rust project structure and configure all dependencies

**Actions**:

1. Initialize Cargo project: `cargo init --bin`
2. Configure `Cargo.toml` with ALL dependencies (see complete list below)
3. Set up module structure: `agent/`, `providers/`, `tools/`
4. Configure build profiles (dev with debug, release optimized)
5. Set up `.gitignore` for Rust
6. Create documentation structure following Diataxis

**Files Created**:

- `Cargo.toml` - Project configuration (~80 lines)
- `src/main.rs` - Binary entry point (~50 lines)
- `src/lib.rs` - Library root (~30 lines)
- `.gitignore` - Git ignore patterns (~20 lines)
- `README.md` - Updated with quick start (~100 lines)

**Complete Dependencies**:

```toml
[package]
name = "xzatoma"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"

[dependencies]
# CLI and configuration
clap = { version = "4.5", features = ["derive", "env"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"

# Async runtime
tokio = { version = "1.43", features = ["full"] }
async-trait = "0.1"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }

# HTTP client for providers
reqwest = { version = "0.12", features = ["json"] }

# Credential storage
keyring = { version = "3.6", features = ["apple-native", "windows-native", "linux-native"] }
chrono = { version = "0.4", features = ["serde"] }

# File operations
walkdir = "2.5"
similar = "2.7"

# Interactive mode
rustyline = "15.0"

# Command validation
regex = "1.11"

[dev-dependencies]
mockall = "0.13"
tempfile = "3.14"
tokio-test = "0.4"
wiremock = "0.6"
assert_cmd = "2.0"
predicates = "3.1"
```

**Module Structure**:

```
src/
├── main.rs              # Entry point
├── lib.rs               # Library root
├── cli.rs               # CLI parser
├── config.rs            # Configuration
├── error.rs             # Error types
│
├── agent/               # Agent core
│   ├── mod.rs
│   ├── agent.rs         # Main agent logic
│   └── conversation.rs  # Message history with token management
│
├── providers/           # AI providers
│   ├── mod.rs
│   ├── base.rs          # Provider trait
│   ├── copilot/
│   │   ├── mod.rs
│   │   ├── auth.rs      # OAuth flow
│   │   └── provider.rs  # Copilot provider
│   └── ollama.rs        # Ollama provider
│
└── tools/               # Basic tools
    ├── mod.rs
    ├── file_ops.rs      # File operations
    ├── terminal/
    │   ├── mod.rs
    │   ├── validator.rs # Command validation
    │   └── executor.rs  # Terminal execution
    └── plan.rs          # Plan parsing
```

**LOC Estimate**: ~280 lines

**Tests**: Basic smoke tests (~50 lines)

#### Task 1.2: Error Handling System

**Description**: Implement comprehensive error handling with ALL error types from architecture

**Actions**:

1. Define `XzatomaError` enum using `thiserror`
2. Create ALL domain-specific error variants (from architecture.md)
3. Implement `From` conversions for std errors
4. Add helpful error messages with context
5. Write unit tests for error conversion and display

**Files Created**:

- `src/error.rs` - Error definitions (~150 lines)
- `tests/unit/error_test.rs` - Error tests (~100 lines)

**Complete Error Types** (from architecture.md):

```rust
use thiserror::Error;

/// XZatoma error types
#[derive(Debug, Error)]
pub enum XzatomaError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Provider error
    #[error("Provider error: {0}")]
    Provider(String),

    /// Tool execution error
    #[error("Tool execution error: {0}")]
    Tool(String),

    /// Maximum iterations exceeded
    #[error("Maximum iterations exceeded: {limit} turns reached. {message}")]
    MaxIterationsExceeded {
        limit: usize,
        message: String,
    },

    /// Dangerous command blocked
    #[error("Dangerous command blocked: {0}")]
    DangerousCommand(String),

    /// Command requires user confirmation
    #[error("Command requires user confirmation: {0}")]
    CommandRequiresConfirmation(String),

    /// Path outside working directory
    #[error("Path outside working directory: {0}")]
    PathOutsideWorkingDirectory(String),

    /// Streaming not supported by provider
    #[error("Streaming not supported by this provider")]
    StreamingNotSupported,

    /// Missing credentials for provider
    #[error("Missing credentials for provider: {0}")]
    MissingCredentials(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML error
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// HTTP request error
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    /// Keyring error
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),
}

/// Result type alias for XZatoma operations
pub type Result<T> = std::result::Result<T, XzatomaError>;
```

**LOC Estimate**: ~150 lines production + ~100 lines tests

**Tests**:

- Error variant construction
- Display trait output
- From conversions for std errors
- Error context preservation

#### Task 1.3: Configuration Management

**Description**: Implement configuration loading with FULL precedence rules and ALL config fields

**Actions**:

1. Define ALL configuration structures (complete from architecture.md)
2. Implement YAML file loading with validation
3. Add environment variable support
4. Implement CLI argument override capability
5. Apply precedence: **CLI > ENV > File > Default**
6. Add validation for all fields (ranges, valid values, etc.)
7. Write comprehensive tests for all precedence scenarios

**Files Created**:

- `src/config.rs` - Configuration types and loading (~350 lines)
- `config.example.yaml` - Example configuration with comments (~100 lines)
- `tests/unit/config_test.rs` - Configuration tests (~200 lines)

**Complete Configuration Structure** (from architecture.md):

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    pub agent: AgentConfig,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider type: "copilot" or "ollama"
    #[serde(rename = "type")]
    pub provider_type: String,

    /// Copilot configuration (if using Copilot)
    pub copilot: Option<CopilotConfig>,

    /// Ollama configuration (if using Ollama)
    pub ollama: Option<OllamaConfig>,
}

/// GitHub Copilot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// Model name (default: "gpt-5-mini")
    #[serde(default = "default_copilot_model")]
    pub model: String,
}

fn default_copilot_model() -> String {
    "gpt-5-mini".to_string()
}

/// Ollama configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Host address (default: "localhost:11434")
    #[serde(default = "default_ollama_host")]
    pub host: String,

    /// Model name (default: "qwen2.5-coder")
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

fn default_ollama_host() -> String {
    "localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "qwen2.5-coder".to_string()
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum conversation turns (default: 100)
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,

    /// Timeout in seconds (default: 600)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Conversation management settings
    #[serde(default)]
    pub conversation: ConversationConfig,

    /// Tool execution settings
    #[serde(default)]
    pub tools: ToolsConfig,

    /// Terminal execution settings
    #[serde(default)]
    pub terminal: TerminalConfig,
}

fn default_max_turns() -> usize {
    100
}

fn default_timeout() -> u64 {
    600
}

/// Conversation token management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    /// Maximum tokens in conversation context (default: 100000)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Minimum turns to retain when pruning (default: 5)
    #[serde(default = "default_min_retain")]
    pub min_retain_turns: usize,

    /// Prune when reaching this fraction of max_tokens (default: 0.8)
    #[serde(default = "default_prune_threshold")]
    pub prune_threshold: f64,
}

fn default_max_tokens() -> usize {
    100000
}

fn default_min_retain() -> usize {
    5
}

fn default_prune_threshold() -> f64 {
    0.8
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            min_retain_turns: default_min_retain(),
            prune_threshold: default_prune_threshold(),
        }
    }
}

/// Tool execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Maximum tool output size in bytes (default: 1MB)
    #[serde(default = "default_max_output")]
    pub max_output_size: usize,

    /// Maximum file read size in bytes (default: 10MB)
    #[serde(default = "default_max_file_read")]
    pub max_file_read_size: usize,
}

fn default_max_output() -> usize {
    1_048_576 // 1MB
}

fn default_max_file_read() -> usize {
    10_485_760 // 10MB
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_size: default_max_output(),
            max_file_read_size: default_max_file_read(),
        }
    }
}

/// Terminal execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Default execution mode
    #[serde(default)]
    pub default_mode: ExecutionMode,

    /// Command timeout in seconds (default: 30)
    #[serde(default = "default_command_timeout")]
    pub timeout_seconds: u64,

    /// Maximum stdout bytes (default: 10MB)
    #[serde(default = "default_max_stdout")]
    pub max_stdout_bytes: usize,

    /// Maximum stderr bytes (default: 1MB)
    #[serde(default = "default_max_stderr")]
    pub max_stderr_bytes: usize,
}

fn default_command_timeout() -> u64 {
    30
}

fn default_max_stdout() -> usize {
    10_485_760 // 10MB
}

fn default_max_stderr() -> usize {
    1_048_576 // 1MB
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            default_mode: ExecutionMode::default(),
            timeout_seconds: default_command_timeout(),
            max_stdout_bytes: default_max_stdout(),
            max_stderr_bytes: default_max_stderr(),
        }
    }
}

/// Terminal execution mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// All commands require user confirmation
    Interactive,

    /// Only allowlist commands run autonomously
    RestrictedAutonomous,

    /// All non-dangerous commands run autonomously
    FullAutonomous,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        ExecutionMode::RestrictedAutonomous
    }
}

impl Config {
    /// Load configuration with precedence: CLI > ENV > File > Default
    pub fn load(config_path: Option<PathBuf>, cli_overrides: HashMap<String, String>) -> Result<Self> {
        // 1. Start with defaults
        let mut config = Self::default_config();

        // 2. Load from file if specified
        if let Some(path) = config_path {
            config = Self::from_file(&path)?;
        }

        // 3. Apply environment variables
        config.apply_env_vars()?;

        // 4. Apply CLI overrides
        config.apply_cli_overrides(cli_overrides)?;

        // 5. Validate
        config.validate()?;

        Ok(config)
    }

    fn default_config() -> Self {
        Self {
            provider: ProviderConfig {
                provider_type: "ollama".to_string(),
                copilot: None,
                ollama: Some(OllamaConfig {
                    host: default_ollama_host(),
                    model: default_ollama_model(),
                }),
            },
            agent: AgentConfig {
                max_turns: default_max_turns(),
                timeout_seconds: default_timeout(),
                conversation: ConversationConfig::default(),
                tools: ToolsConfig::default(),
                terminal: TerminalConfig::default(),
            },
        }
    }

    fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    fn apply_env_vars(&mut self) -> Result<()> {
        // Provider type
        if let Ok(val) = std::env::var("XZATOMA_PROVIDER_TYPE") {
            self.provider.provider_type = val;
        }

        // Copilot model
        if let Ok(val) = std::env::var("XZATOMA_COPILOT_MODEL") {
            if self.provider.copilot.is_none() {
                self.provider.copilot = Some(CopilotConfig {
                    model: default_copilot_model(),
                });
            }
            if let Some(ref mut copilot) = self.provider.copilot {
                copilot.model = val;
            }
        }

        // Ollama settings
        if let Ok(val) = std::env::var("XZATOMA_OLLAMA_HOST") {
            if self.provider.ollama.is_none() {
                self.provider.ollama = Some(OllamaConfig {
                    host: default_ollama_host(),
                    model: default_ollama_model(),
                });
            }
            if let Some(ref mut ollama) = self.provider.ollama {
                ollama.host = val;
            }
        }

        if let Ok(val) = std::env::var("XZATOMA_OLLAMA_MODEL") {
            if let Some(ref mut ollama) = self.provider.ollama {
                ollama.model = val;
            }
        }

        // Agent settings
        if let Ok(val) = std::env::var("XZATOMA_MAX_TURNS") {
            self.agent.max_turns = val.parse().map_err(|_|
                XzatomaError::Config("Invalid XZATOMA_MAX_TURNS".to_string()))?;
        }

        if let Ok(val) = std::env::var("XZATOMA_TERMINAL_MODE") {
            self.agent.terminal.default_mode = match val.as_str() {
                "interactive" => ExecutionMode::Interactive,
                "restricted_autonomous" => ExecutionMode::RestrictedAutonomous,
                "full_autonomous" => ExecutionMode::FullAutonomous,
                _ => return Err(XzatomaError::Config(
                    format!("Invalid terminal mode: {}", val)
                )),
            };
        }

        Ok(())
    }

    fn apply_cli_overrides(&mut self, overrides: HashMap<String, String>) -> Result<()> {
        for (key, value) in overrides {
            match key.as_str() {
                "provider" => self.provider.provider_type = value,
                "max_turns" => self.agent.max_turns = value.parse().map_err(|_|
                    XzatomaError::Config("Invalid max_turns".to_string()))?,
                // Add more as needed
                _ => {}
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        // Validate provider type
        if self.provider.provider_type != "copilot" && self.provider.provider_type != "ollama" {
            return Err(XzatomaError::Config(
                format!("Invalid provider type: {}", self.provider.provider_type)
            ));
        }

        // Validate provider-specific config exists
        if self.provider.provider_type == "copilot" && self.provider.copilot.is_none() {
            return Err(XzatomaError::Config(
                "Copilot provider selected but no copilot config".to_string()
            ));
        }

        if self.provider.provider_type == "ollama" && self.provider.ollama.is_none() {
            return Err(XzatomaError::Config(
                "Ollama provider selected but no ollama config".to_string()
            ));
        }

        // Validate ranges
        if self.agent.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be > 0".to_string()
            ));
        }

        if self.agent.conversation.max_tokens == 0 {
            return Err(XzatomaError::Config(
                "conversation.max_tokens must be > 0".to_string()
            ));
        }

        if self.agent.conversation.prune_threshold <= 0.0 || self.agent.conversation.prune_threshold >= 1.0 {
            return Err(XzatomaError::Config(
                "conversation.prune_threshold must be between 0.0 and 1.0".to_string()
            ));
        }

        Ok(())
    }
}
```

**Environment Variables**:

- `XZATOMA_CONFIG` - Config file path
- `XZATOMA_PROVIDER_TYPE` - Provider type ("copilot" or "ollama")
- `XZATOMA_COPILOT_MODEL` - Copilot model name
- `XZATOMA_OLLAMA_HOST` - Ollama host address
- `XZATOMA_OLLAMA_MODEL` - Ollama model name
- `XZATOMA_MAX_TURNS` - Maximum conversation turns
- `XZATOMA_TERMINAL_MODE` - Terminal execution mode

**Example config.example.yaml**:

```yaml
# XZatoma Configuration Example
# All settings are optional - defaults will be used if not specified

provider:
  # Provider type: "copilot" or "ollama"
  type: ollama

  # GitHub Copilot settings (if using copilot)
  copilot:
    model: gpt-5-mini

  # Ollama settings (if using ollama)
  ollama:
    host: localhost:11434
    model: qwen2.5-coder

agent:
  # Maximum conversation turns before stopping
  max_turns: 100

  # Overall timeout in seconds
  timeout_seconds: 600

  # Conversation token management
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8

  # Tool execution limits
  tools:
    max_output_size: 1048576 # 1MB
    max_file_read_size: 10485760 # 10MB

  # Terminal execution settings
  terminal:
    default_mode: restricted_autonomous # or "interactive" or "full_autonomous"
    timeout_seconds: 30
    max_stdout_bytes: 10485760 # 10MB
    max_stderr_bytes: 1048576 # 1MB
```

**LOC Estimate**: ~350 lines production + ~200 lines tests

**Tests**:

- Default configuration generation
- File loading
- Environment variable override
- CLI argument override
- Precedence rules (CLI > ENV > File > Default)
- Validation (invalid values rejected)
- Error handling (missing files, parse errors)

#### Task 1.4: Testing Infrastructure

**Description**: Set up comprehensive test framework with utilities

**Actions**:

1. Create test directory structure
2. Add ALL test dependencies (already in Cargo.toml)
3. Create test utilities module
4. Set up integration test harness
5. Write example tests demonstrating patterns

**Directories Created**:

- `tests/unit/` - Unit tests
- `tests/integration/` - Integration tests
- `tests/fixtures/` - Test data and plans
- `tests/common/` - Shared test utilities

**Files Created**:

- `tests/common/mod.rs` - Test utilities (~100 lines)
- `tests/unit/mod.rs` - Unit test organization (~20 lines)
- `tests/integration/mod.rs` - Integration test setup (~50 lines)

**Test Utilities**:

```rust
// tests/common/mod.rs
use tempfile::TempDir;
use std::path::PathBuf;

/// Create a temporary directory for testing
pub fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

/// Create a test file with content
pub fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("Failed to write test file");
    path
}

/// Assert error contains expected substring
pub fn assert_error_contains(result: &Result<(), XzatomaError>, expected: &str) {
    match result {
        Err(e) => assert!(
            e.to_string().contains(expected),
            "Error '{}' does not contain '{}'",
            e,
            expected
        ),
        Ok(_) => panic!("Expected error but got Ok"),
    }
}

/// Create a test configuration
pub fn test_config() -> Config {
    Config {
        provider: ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: None,
            ollama: Some(OllamaConfig {
                host: "localhost:11434".to_string(),
                model: "test-model".to_string(),
            }),
        },
        agent: AgentConfig {
            max_turns: 10,
            timeout_seconds: 60,
            conversation: ConversationConfig {
                max_tokens: 1000,
                min_retain_turns: 2,
                prune_threshold: 0.8,
            },
            tools: ToolsConfig {
                max_output_size: 1024,
                max_file_read_size: 10240,
            },
            terminal: TerminalConfig {
                default_mode: ExecutionMode::Interactive,
                timeout_seconds: 10,
                max_stdout_bytes: 1024,
                max_stderr_bytes: 512,
            },
        },
    }
}
```

**LOC Estimate**: ~170 lines

#### Task 1.5: Basic CLI Skeleton

**Description**: Create minimal CLI structure for testing foundation

**Actions**:

1. Define CLI argument structure with clap
2. Implement basic commands (stubbed implementations)
3. Add version and help text
4. Initialize logging
5. Write CLI parsing tests

**Files Created**:

- `src/cli.rs` - CLI definition (~150 lines)
- `tests/unit/cli_test.rs` - CLI parsing tests (~80 lines)

**CLI Structure**:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// XZatoma - Autonomous AI agent CLI
#[derive(Parser)]
#[command(name = "xzatoma")]
#[command(version, about = "Autonomous AI agent CLI", long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, global = true, env = "XZATOMA_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive chat mode with AI agent
    Chat {
        /// Provider to use: "copilot" or "ollama" (overrides config)
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Execute a plan file or direct prompt
    Run {
        /// Path to plan file (YAML, JSON, or Markdown)
        #[arg(short, long)]
        plan: Option<PathBuf>,

        /// Direct prompt instead of plan file
        #[arg(short = 'p', long)]
        prompt: Option<String>,

        /// Allow dangerous commands without confirmation (use with caution!)
        #[arg(long)]
        allow_dangerous: bool,
    },

    /// Authenticate with AI provider
    Auth {
        /// Provider name: "copilot" or "ollama"
        provider: String,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
```

**LOC Estimate**: ~150 lines production + ~80 lines tests

**Tests**:

- CLI parsing with all arguments
- Subcommand parsing
- Default values
- Environment variable support
- Help text generation

#### Task 1.6: Deliverables

**Phase 1 Deliverables**:

- [ ] Working Rust project structure
- [ ] Complete error handling system with all error types
- [ ] Configuration system with full precedence rules
- [ ] Test infrastructure with mock utilities
- [ ] Basic CLI skeleton (--help, --version work)
- [ ] Builds without errors
- [ ] All quality gates pass

#### Task 1.7: Success Criteria

**Code Quality**:

- [ ] `cargo fmt --all` applied successfully
- [ ] `cargo check --all-targets --all-features` passes with zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- [ ] `cargo test --all-features` passes
- [ ] Test coverage >80%

**Functionality**:

- [ ] CLI displays help text correctly
- [ ] CLI displays version correctly
- [ ] Configuration loads from all sources (file, env, CLI)
- [ ] Configuration precedence works correctly
- [ ] Error types compile and convert correctly
- [ ] Test utilities work as expected

**Documentation**:

- [ ] README.md updated with installation instructions
- [ ] config.example.yaml created with all options documented
- [ ] All file extensions are `.yaml` (NOT `.yml`)
- [ ] All markdown filenames are `lowercase_with_underscores.md`
- [ ] No emojis in documentation

**Phase 1 Total LOC**: ~800 lines (500 production + 300 tests)

---

### Phase 2: Agent Core with Token Management

**Timeline**: Weeks 3-4

**Objective**: Implement agent execution loop with iteration limits, conversation token management with pruning, and tool execution framework.

**Total LOC Target**: ~1,000 lines (650 production + 350 tests)

#### Task 2.1: Conversation Management with Token Tracking

**Description**: Implement conversation history with automatic token management and pruning

**Actions**:

1. Create conversation struct with token tracking fields
2. Implement message management methods
3. Add token counting logic (rough estimate: 1 token ≈ 4 chars)
4. Implement automatic pruning when approaching limit
5. Add summarization strategy for pruned messages
6. Preserve system message and recent turns
7. Write comprehensive tests

**Files Created**:

- `src/agent/mod.rs` - Agent module exports (~20 lines)
- `src/agent/conversation.rs` - Conversation management (~280 lines)
- `tests/unit/conversation_test.rs` - Conversation tests (~150 lines)

**Complete Conversation Structure** (from architecture.md):

```rust
use crate::providers::Message;
use crate::error::Result;

/// Conversation history with automatic token management
pub struct Conversation {
    messages: Vec<Message>,
    token_count: usize,
    max_tokens: usize,
    min_retain_turns: usize,
}

impl Conversation {
    /// Create new conversation with token limits
    pub fn new(max_tokens: usize, min_retain_turns: usize) -> Self {
        Self {
            messages: Vec::new(),
            token_count: 0,
            max_tokens,
            min_retain_turns,
        }
    }

    /// Add user message to conversation
    pub fn add_user_message(&mut self, content: String) {
        let msg = Message::user(content);
        self.update_token_count(&msg);
        self.messages.push(msg);
        self.prune_if_needed();
    }

    /// Add assistant message to conversation
    pub fn add_assistant_message(&mut self, content: String) {
        let msg = Message::assistant(content);
        self.update_token_count(&msg);
        self.messages.push(msg);
        self.prune_if_needed();
    }

    /// Add tool result to conversation
    pub fn add_tool_result(&mut self, call_id: String, result: String) {
        let msg = Message::tool_result(call_id, result);
        self.update_token_count(&msg);
        self.messages.push(msg);
        self.prune_if_needed();
    }

    /// Update token count for new message
    fn update_token_count(&mut self, msg: &Message) {
        // Rough estimate: 1 token ≈ 4 characters
        let new_tokens = msg.content.len() / 4;
        self.token_count += new_tokens;
        tracing::debug!(
            "Added {} tokens, total now: {}/{}",
            new_tokens,
            self.token_count,
            self.max_tokens
        );
    }

    /// Prune old messages if approaching token limit
    fn prune_if_needed(&mut self) {
        let threshold = (self.max_tokens as f64 * 0.8) as usize;

        if self.token_count < threshold {
            return;
        }

        tracing::info!(
            "Token count {} exceeds threshold {}, pruning conversation",
            self.token_count,
            threshold
        );

        // Keep: system message + last N turns (user+assistant pairs) + current message
        let keep_messages = self.min_retain_turns * 2; // Each turn = user + assistant

        if self.messages.len() <= keep_messages + 1 {
            tracing::debug!("Not enough messages to prune");
            return;
        }

        // Identify sections
        let system_msg = self.messages.first().cloned();
        let middle_start = if system_msg.is_some() { 1 } else { 0 };
        let middle_end = self.messages.len() - keep_messages;

        let middle_messages = &self.messages[middle_start..middle_end];
        let recent_messages = &self.messages[middle_end..];

        // Create summary of pruned messages
        let summary = self.create_summary(middle_messages);

        // Rebuild messages: [system?] + [summary] + [recent]
        let mut new_messages = Vec::new();

        if let Some(sys) = system_msg {
            new_messages.push(sys);
        }

        new_messages.push(Message::system(summary));
        new_messages.extend_from_slice(recent_messages);

        self.messages = new_messages;
        self.recalculate_tokens();

        tracing::info!(
            "Pruned conversation, new token count: {}",
            self.token_count
        );
    }

    /// Create summary of pruned messages
    fn create_summary(&self, messages: &[Message]) -> String {
        let num_user_msgs = messages.iter()
            .filter(|m| matches!(m.role, crate::providers::Role::User))
            .count();
        let num_assistant_msgs = messages.iter()
            .filter(|m| matches!(m.role, crate::providers::Role::Assistant))
            .count();
        let num_tool_msgs = messages.iter()
            .filter(|m| matches!(m.role, crate::providers::Role::Tool))
            .count();

        format!(
            "[Previous conversation summarized: {} user messages, {} assistant messages, {} tool results]",
            num_user_msgs, num_assistant_msgs, num_tool_msgs
        )
    }

    /// Recalculate total token count
    fn recalculate_tokens(&mut self) {
        self.token_count = self.messages.iter()
            .map(|m| m.content.len() / 4)
            .sum();
    }

    /// Get all messages
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get current token count
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Get remaining token capacity
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.token_count)
    }
}
```

**LOC Estimate**: ~280 lines production + ~150 lines tests

**Tests**:

- Token counting accuracy
- Pruning triggers at threshold
- Min retain turns preserved
- System message preserved
- Summary creation format
- Token recalculation accuracy
- Remaining tokens calculation
- Edge cases (empty conversation, single message)

#### Task 2.2: Tool System and Registry

**Description**: Implement tool execution framework with comprehensive result handling

**Actions**:

1. Define Tool struct with JSON Schema for parameters
2. Create ToolExecutor trait
3. Implement ToolResult with success/error/truncation handling
4. Create tool registry for managing available tools
5. Add tool execution wrapper logic
6. Write tests

**Files Created**:

- `src/tools/mod.rs` - Tool system (~250 lines)
- `tests/unit/tools_test.rs` - Tool tests (~100 lines)

**Complete Tool System** (from architecture.md):

```rust
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use crate::error::Result;

/// Tool definition with JSON Schema parameters
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// Tool executor trait
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Get tool definition for AI provider
    fn tool_definition(&self) -> Tool;

    /// Execute tool with parameters
    async fn execute(&self, params: Value) -> Result<ToolResult>;
}

/// Tool execution result
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub truncated: bool,
    pub metadata: HashMap<String, String>,
}

impl ToolResult {
    /// Create successful result
    pub fn success(output: String) -> Self {
        Self {
            success: true,
            output,
            error: None,
            truncated: false,
            metadata: HashMap::new(),
        }
    }

    /// Create error result
    pub fn error(error: String) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error),
            truncated: false,
            metadata: HashMap::new(),
        }
    }

    /// Truncate output if exceeds max size
    pub fn truncate_if_needed(&mut self, max_size: usize) {
        if self.output.len() > max_size {
            let original_size = self.output.len();
            let truncate_msg = format!(
                "\n\n[Output truncated: {} bytes total, showing first {} bytes]",
                original_size,
                max_size
            );

            self.output.truncate(max_size);
            self.output.push_str(&truncate_msg);
            self.truncated = true;

            self.metadata.insert(
                "original_size".to_string(),
                original_size.to_string()
            );
            self.metadata.insert(
                "truncated_size".to_string(),
                max_size.to_string()
            );

            tracing::warn!(
                "Truncated tool output from {} to {} bytes",
                original_size,
                max_size
            );
        }
    }

    /// Convert result to message content
    pub fn to_message(&self) -> String {
        if self.success {
            if self.truncated {
                format!("{}\n[Note: Output was truncated]", self.output)
            } else {
                self.output.clone()
            }
        } else {
            format!(
                "Error: {}",
                self.error.as_ref().unwrap_or(&"Unknown error".to_string())
            )
        }
    }

    /// Add metadata entry
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Tool registry for managing available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool executor
    pub fn register(&mut self, executor: Box<dyn ToolExecutor>) {
        let tool = executor.tool_definition();
        tracing::info!("Registering tool: {}", tool.name);
        self.tools.insert(tool.name.clone(), executor);
    }

    /// Get tool executor by name
    pub fn get(&self, name: &str) -> Option<&Box<dyn ToolExecutor>> {
        self.tools.get(name)
    }

    /// Get all tool definitions
    pub fn all_definitions(&self) -> Vec<Tool> {
        self.tools.values()
            .map(|executor| executor.tool_definition())
            .collect()
    }

    /// Get number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**LOC Estimate**: ~250 lines production + ~100 lines tests

**Tests**:

- ToolResult creation (success/error)
- Truncation logic
- Truncation metadata
- Message conversion
- Tool registry add/get
- Tool definition extraction
- Empty registry handling

#### Task 2.3: Agent Execution Loop with Iteration Limits

**Description**: Implement main agent with strict iteration enforcement and timeout

**Actions**:

1. Create Agent struct with all required fields
2. Implement execution loop with max_iterations check (CRITICAL)
3. Add tool calling logic
4. Handle tool execution errors gracefully
5. Add overall timeout handling
6. Add structured logging
7. Write comprehensive tests with mock provider

**Files Created**:

- `src/agent/agent.rs` - Main agent implementation (~320 lines)
- `tests/unit/agent_test.rs` - Agent tests (~100 lines)
- `tests/integration/agent_integration_test.rs` - Integration tests (~100 lines)
- `tests/common/mock_provider.rs` - Mock provider for testing (~100 lines)

**Complete Agent Structure** (from architecture.md):

```rust
use crate::config::AgentConfig;
use crate::providers::{Provider, Message, ToolCall};
use crate::tools::{ToolRegistry, ToolResult};
use crate::agent::conversation::Conversation;
use crate::error::{Result, XzatomaError};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Main agent coordinating AI provider and tool execution
pub struct Agent {
    provider: Arc<dyn Provider>,
    conversation: Conversation,
    tools: ToolRegistry,
    config: AgentConfig,
}

impl Agent {
    /// Create new agent
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
    ) -> Self {
        let conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
        );

        tracing::info!(
            "Created agent with max_turns={}, timeout={}s",
            config.max_turns,
            config.timeout_seconds
        );

        Self {
            provider,
            conversation,
            tools,
            config,
        }
    }

    /// Execute instruction with autonomous tool usage
    pub async fn execute(&mut self, instruction: String) -> Result<String> {
        tracing::info!("Starting agent execution: {}", instruction);

        self.conversation.add_user_message(instruction);

        let mut iterations = 0;
        let execution_timeout = Duration::from_secs(self.config.timeout_seconds);

        let result = timeout(execution_timeout, async {
            loop {
                // CRITICAL: Enforce iteration limit to prevent infinite loops
                if iterations >= self.config.max_turns {
                    return Err(XzatomaError::MaxIterationsExceeded {
                        limit: self.config.max_turns,
                        message: format!(
                            "Agent exceeded maximum iterations. Messages: {}, Tokens: {}",
                            self.conversation.messages().len(),
                            self.conversation.token_count()
                        ),
                    });
                }

                iterations += 1;
                tracing::debug!(
                    "Agent iteration {}/{} (tokens: {}/{})",
                    iterations,
                    self.config.max_turns,
                    self.conversation.token_count(),
                    self.conversation.remaining_tokens()
                );

                // Get AI response
                let response = self.provider.complete(
                    self.conversation.messages(),
                    &self.tools.all_definitions(),
                ).await.map_err(|e| {
                    tracing::error!("Provider error: {}", e);
                    e
                })?;

                // Check for tool calls
                if let Some(tool_calls) = response.tool_calls {
                    tracing::info!("Processing {} tool call(s)", tool_calls.len());

                    for call in tool_calls {
                        tracing::debug!("Executing tool: {} (id: {})", call.name, call.id);

                        let result = self.execute_tool_call(&call).await?;

                        tracing::debug!(
                            "Tool {} completed: success={}, output_len={}",
                            call.name,
                            result.success,
                            result.output.len()
                        );

                        self.conversation.add_tool_result(
                            call.id,
                            result.to_message()
                        );
                    }
                } else {
                    // No tool calls - agent is done
                    tracing::info!(
                        "Agent completed in {} iterations",
                        iterations
                    );

                    self.conversation.add_assistant_message(response.content.clone());
                    return Ok(response.content);
                }
            }
        }).await;

        match result {
            Ok(res) => res,
            Err(_) => {
                tracing::error!(
                    "Agent execution timed out after {}s",
                    self.config.timeout_seconds
                );
                Err(XzatomaError::Tool(format!(
                    "Agent execution timed out after {} seconds",
                    self.config.timeout_seconds
                )))
            }
        }
    }

    /// Execute a tool call
    async fn execute_tool_call(&self, call: &ToolCall) -> Result<ToolResult> {
        tracing::info!("Executing tool: {} with params: {:?}", call.name, call.parameters);

        // Get tool executor
        let executor = self.tools.get(&call.name)
            .ok_or_else(|| {
                tracing::error!("Unknown tool requested: {}", call.name);
                XzatomaError::Tool(format!("Unknown tool: {}", call.name))
            })?;

        // Execute tool
        let start = std::time::Instant::now();
        let mut result = executor.execute(call.parameters.clone()).await.map_err(|e| {
            tracing::error!("Tool {} execution failed: {}", call.name, e);
            e
        })?;

        let duration = start.elapsed();
        tracing::debug!(
            "Tool {} executed in {:?}",
            call.name,
            duration
        );

        // Truncate large outputs
        result.truncate_if_needed(self.config.tools.max_output_size);

        Ok(result)
    }

    /// Get conversation reference
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Get number of tools registered
    pub fn num_tools(&self) -> usize {
        self.tools.len()
    }
}
```

**Mock Provider for Testing**:

```rust
// tests/common/mock_provider.rs
use xzatoma::providers::{Provider, Message, Response, ToolCall};
use xzatoma::tools::Tool;
use xzatoma::error::Result;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// Mock provider for testing
pub struct MockProvider {
    responses: Arc<Mutex<Vec<Response>>>,
}

impl MockProvider {
    pub fn new(responses: Vec<Response>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<Response> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(Response {
                content: "No more responses".to_string(),
                tool_calls: None,
            })
        } else {
            Ok(responses.remove(0))
        }
    }
}
```

**LOC Estimate**: ~320 lines production + ~300 lines tests (including mock)

**Tests**:

- Iteration limit enforcement (CRITICAL)
- Timeout handling
- Tool execution success
- Tool execution error handling
- Unknown tool error
- Conversation token tracking
- Max iterations error format
- Tool result truncation
- Multiple tool calls in sequence
- Edge case: Empty tool call list

#### Task 2.4: Deliverables

**Phase 2 Deliverables**:

- [ ] Conversation management with token tracking and pruning
- [ ] Tool execution framework with comprehensive result handling
- [ ] Agent execution loop with iteration limits (CRITICAL)
- [ ] Tool registry implementation
- [ ] Mock provider for testing
- [ ] Comprehensive test suite
- [ ] All quality gates pass

#### Task 2.5: Success Criteria

**Code Quality**:

- [ ] All cargo checks pass (fmt, clippy, test)
- [ ] Test coverage >80%
- [ ] Zero clippy warnings

**Functionality**:

- [ ] Conversation tracks tokens correctly
- [ ] Automatic pruning works at threshold
- [ ] Agent enforces iteration limits (CRITICAL)
- [ ] Tool execution handles errors gracefully
- [ ] Timeout works correctly
- [ ] Tool results truncate large outputs
- [ ] Mock provider enables testing

**Documentation**:

- [ ] All public functions have doc comments
- [ ] Examples in doc comments
- [ ] Integration test demonstrates complete workflow

**Phase 2 Total LOC**: ~1,000 lines (650 production + 350 tests)

---

### Phase 3: Security and Terminal Validation

**Timeline**: Weeks 5-6

**Objective**: Implement comprehensive terminal security with execution modes, command validation, path restrictions, and denylist enforcement.

**Total LOC Target**: ~700 lines (450 production + 250 tests)

**CRITICAL**: This phase implements the security model that prevents dangerous command execution. All tests must pass before proceeding.

#### Task 3.1: Command Validator

**Description**: Implement command validation with denylist, allowlist, and execution modes

**Actions**:

1. Create CommandValidator struct
2. Implement execution mode logic (Interactive, RestrictedAutonomous, FullAutonomous)
3. Add comprehensive command denylist (rm -rf /, dd, fork bombs, curl|sh, etc.)
4. Add command allowlist for restricted mode (ls, cat, grep, git, cargo, etc.)
5. Implement path validation (prevent absolute paths, .. escapes)
6. Write extensive security tests (CRITICAL)

**Files Created**:

- `src/tools/terminal/mod.rs` - Terminal module (~30 lines)
- `src/tools/terminal/validator.rs` - Command validator (~320 lines)
- `tests/unit/terminal_validator_test.rs` - Security tests (~200 lines)

**Complete Command Validator** (from architecture.md):

```rust
use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};
use regex::Regex;
use std::path::{Path, PathBuf};

/// Command validator with security enforcement
pub struct CommandValidator {
    mode: ExecutionMode,
    working_dir: PathBuf,
    allowlist: Vec<String>,
    denylist: Vec<Regex>,
}

impl CommandValidator {
    /// Create new validator
    pub fn new(mode: ExecutionMode, working_dir: PathBuf) -> Self {
        // Allowlist for restricted autonomous mode
        let allowlist = vec![
            // File operations
            "ls", "cat", "grep", "find", "echo", "pwd", "whoami",
            "head", "tail", "wc", "sort", "uniq", "diff",
            // Development tools
            "git", "cargo", "rustc", "npm", "node", "python", "python3",
            "go", "make", "cmake",
            // Safe utilities
            "which", "basename", "dirname", "realpath",
        ].into_iter().map(String::from).collect();

        // Denylist patterns (applies to ALL modes)
        let denylist_patterns = vec![
            // Destructive file operations
            r"rm\s+-rf\s+/\s*$",           // rm -rf /
            r"rm\s+-rf\s+/\*",             // rm -rf /*
            r"rm\s+-rf\s+~",               // rm -rf ~
            r"rm\s+-rf\s+\$HOME",          // rm -rf $HOME

            // Dangerous disk operations
            r"dd\s+if=/dev/zero",          // dd if=/dev/zero
            r"dd\s+if=/dev/random",        // dd if=/dev/random
            r"dd\s+of=/dev/sd[a-z]",       // dd of=/dev/sda
            r"mkfs\.",                      // mkfs.* (format filesystem)

            // Fork bombs and resource exhaustion
            r":\(\)\{:\|:&\};:",           // : Fork bomb
            r"while\s+true.*do.*done",     // Infinite loop
            r"for\s*\(\(;;",               // C-style infinite loop

            // Remote code execution
            r"curl\s+.*\|\s*sh",           // curl | sh
            r"wget\s+.*\|\s*sh",           // wget | sh
            r"curl\s+.*\|\s*bash",         // curl | bash
            r"wget\s+.*\|\s*bash",         // wget | bash

            // Privilege escalation
            r"\bsudo\s+",                  // sudo
            r"\bsu\s+",                    // su
            r"\bchmod\s+[0-7]*7[0-7]*",   // chmod with execute for all

            // Code execution
            r"\beval\s*\(",                // eval(
            r"\bexec\s*\(",                // exec(
            r"import\s+os.*system",        // Python os.system

            // Direct device access
            r">\s*/dev/sd[a-z]",           // > /dev/sda
            r">\s*/dev/hd[a-z]",           // > /dev/hda

            // Sensitive files
            r"/etc/passwd",
            r"/etc/shadow",
            r"~/.ssh/",
            r"\$HOME/\.ssh/",
        ];

        let denylist = denylist_patterns.into_iter()
            .map(|p| Regex::new(p).expect("Invalid regex pattern"))
            .collect();

        Self {
            mode,
            working_dir,
            allowlist,
            denylist,
        }
    }

    /// Validate command based on execution mode
    pub fn validate(&self, command: &str) -> Result<()> {
        tracing::debug!("Validating command: {} (mode: {:?})", command, self.mode);

        // Check denylist (applies to ALL modes)
        for pattern in &self.denylist {
            if pattern.is_match(command) {
                tracing::error!(
                    "Command blocked by denylist: {}",
                    command
                );
                return Err(XzatomaError::DangerousCommand(
                    format!("Command matches dangerous pattern: {}", command)
                ));
            }
        }

        // Mode-specific validation
        match self.mode {
            ExecutionMode::Interactive => {
                // All commands require confirmation
                tracing::debug!("Interactive mode: command requires confirmation");
                Err(XzatomaError::CommandRequiresConfirmation(
                    command.to_string()
                ))
            }
            ExecutionMode::RestrictedAutonomous => {
                // Only allowlist commands
                let command_name = command.split_whitespace().next()
                    .ok_or_else(|| XzatomaError::Tool("Empty command".to_string()))?;

                if !self.allowlist.contains(&command_name.to_string()) {
                    tracing::warn!(
                        "Command '{}' not in allowlist for restricted mode",
                        command_name
                    );
                    return Err(XzatomaError::CommandRequiresConfirmation(
                        format!("Command '{}' not in allowlist", command_name)
                    ));
                }

                // Validate paths in command
                self.validate_paths(command)?;

                tracing::debug!("Command passed restricted autonomous validation");
                Ok(())
            }
            ExecutionMode::FullAutonomous => {
                // All non-dangerous commands allowed
                self.validate_paths(command)?;

                tracing::debug!("Command passed full autonomous validation");
                Ok(())
            }
        }
    }

    /// Validate paths in command don't escape working directory
    fn validate_paths(&self, command: &str) -> Result<()> {
        // Extract potential paths from command
        let words: Vec<&str> = command.split_whitespace().collect();

        for word in words {
            // Skip flags and common non-path words
            if word.starts_with('-') || word.is_empty() {
                continue;
            }

            // Check if word looks like a path
            if word.starts_with('/') || word.contains("..") || word.starts_with('~') {
                let path = if word.starts_with('~') {
                    // Reject home directory paths
                    tracing::error!("Home directory path not allowed: {}", word);
                    return Err(XzatomaError::PathOutsideWorkingDirectory(
                        format!("Home directory paths not allowed: {}", word)
                    ));
                } else {
                    Path::new(word)
                };

                // Reject absolute paths
                if path.is_absolute() {
                    tracing::error!("Absolute path not allowed: {}", word);
                    return Err(XzatomaError::PathOutsideWorkingDirectory(
                        format!("Absolute path not allowed: {}", word)
                    ));
                }

                // Check if path escapes working directory
                let full_path = self.working_dir.join(path);
                let canonical_working = self.working_dir.canonicalize()
                    .unwrap_or_else(|_| self.working_dir.clone());

                if let Ok(canonical_full) = full_path.canonicalize() {
                    if !canonical_full.starts_with(&canonical_working) {
                        tracing::error!(
                            "Path escapes working directory: {} -> {:?}",
                            word,
                            canonical_full
                        );
                        return Err(XzatomaError::PathOutsideWorkingDirectory(
                            format!("Path escapes working directory: {}", word)
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}
```

**Testing Requirements**:

1. Denylist validation (20+ test cases)

   - Block `rm -rf /`
   - Block `dd if=/dev/zero`
   - Block fork bombs
   - Block `curl | sh`
   - Block `sudo` commands
   - Block sensitive file access

2. Allowlist enforcement (10+ test cases)

   - Allowed commands pass in restricted mode
   - Non-allowed commands require confirmation
   - Denylist overrides allowlist

3. Execution mode behavior (6+ test cases)

   - Interactive mode requires confirmation for all
   - Restricted mode uses allowlist
   - Full autonomous allows safe commands

4. Path validation (15+ test cases)
   - Reject absolute paths
   - Reject home directory paths
   - Reject `..` escapes
   - Allow relative paths within working directory
   - Handle symlinks correctly

**Expected Test LOC**: ~200 lines

**Validation**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

#### Task 3.2: Terminal Executor

**Description**: Implement terminal command execution with validation integration

**Actions**:

1. Create TerminalTool struct implementing ToolExecutor
2. Integrate CommandValidator
3. Add timeout enforcement
4. Add output truncation
5. Add execution mode handling
6. Write integration tests

**Files Created**:

- `src/tools/terminal/executor.rs` - Command executor (~150 lines)
- `tests/integration/terminal_integration_test.rs` - Integration tests (~50 lines)

**Complete Terminal Executor**:

```rust
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::config::TerminalConfig;
use crate::error::{Result, XzatomaError};
use crate::tools::{Tool, ToolExecutor, ToolResult};
use super::validator::CommandValidator;

/// Terminal command execution tool
pub struct TerminalTool {
    validator: CommandValidator,
    config: TerminalConfig,
}

impl TerminalTool {
    pub fn new(validator: CommandValidator, config: TerminalConfig) -> Self {
        Self { validator, config }
    }

    async fn execute_command(&self, command: &str) -> Result<ToolResult> {
        // Validate command
        self.validator.validate(command)?;

        // Parse command
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(ToolResult::error("Empty command"));
        }

        let program = parts[0];
        let args = &parts[1..];

        // Execute with timeout
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        let output = timeout(timeout_duration, async {
            Command::new(program)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        })
        .await
        .map_err(|_| XzatomaError::Tool(format!("Command timed out after {}s", self.config.timeout_seconds)))?
        .map_err(|e| XzatomaError::Tool(format!("Failed to execute command: {}", e)))?;

        // Truncate output if needed
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = if output.status.success() {
            ToolResult::success(stdout.to_string())
        } else {
            ToolResult::error(format!("Command failed: {}", stderr))
        };

        // Apply output limits
        result = result.truncate_if_needed(self.config.max_stdout_bytes);

        Ok(result)
    }
}

#[async_trait]
impl ToolExecutor for TerminalTool {
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "terminal".to_string(),
            description: "Execute terminal commands in the working directory".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("Missing 'command' parameter".to_string()))?;

        self.execute_command(command).await
    }
}
```

**Testing Requirements**:

1. Integration tests with validator
2. Timeout enforcement tests
3. Output truncation tests
4. Success and failure scenarios

**Expected Test LOC**: ~50 lines

#### Task 3.3: Deliverables

Phase 3 Complete Deliverables:

- `src/tools/terminal/mod.rs` (~30 lines)
- `src/tools/terminal/validator.rs` (~320 lines)
- `src/tools/terminal/executor.rs` (~150 lines)
- `tests/unit/terminal_validator_test.rs` (~200 lines)
- `tests/integration/terminal_integration_test.rs` (~50 lines)

**Total**: ~750 lines (exceeds 700 target due to comprehensive security tests)

#### Task 3.4: Success Criteria

Phase 3 complete when:

- [ ] All denylist patterns block dangerous commands
- [ ] Allowlist enforcement works in restricted mode
- [ ] Path validation prevents directory escapes
- [ ] All execution modes behave correctly
- [ ] Terminal executor integrates with validator
- [ ] Timeout and truncation work correctly
- [ ] `cargo fmt --all` applied
- [ ] `cargo check --all-targets --all-features` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] All security tests pass (non-negotiable)
- [ ] Documentation updated in `docs/explanation/`

---

### Phase 4: AI Providers

**Timeline**: Weeks 7-8

**Objective**: Implement AI provider abstraction with GitHub Copilot and Ollama support, including OAuth device flow and tool calling.

**Total LOC Target**: ~900 lines (600 production + 300 tests)

#### Task 4.1: Provider Trait and Base Types

**Description**: Define provider abstraction and common types

**Actions**:

1. Create Provider trait
2. Define Message and ToolCall types
3. Define CompletionRequest and CompletionResponse
4. Create provider registry
5. Write trait tests with mocks

**Files Created**:

- `src/providers/mod.rs` - Provider module (~50 lines)
- `src/providers/base.rs` - Provider trait (~100 lines)
- `tests/common/mock_provider.rs` - Mock for testing (already created in Phase 2)

**Complete Provider Trait**:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::tools::Tool;

/// Message in conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool call request from AI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub parameters: serde_json::Value,
}

/// Request to AI provider
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub max_tokens: Option<usize>,
}

/// Response from AI provider
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: Message,
    pub finish_reason: String,
    pub usage: TokenUsage,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// AI provider trait
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get provider name
    fn name(&self) -> &str;

    /// Complete conversation with tool support
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Check if provider supports streaming
    fn supports_streaming(&self) -> bool {
        false
    }
}
```

**Testing Requirements**:

1. Message serialization/deserialization
2. Provider trait with mock implementation
3. Request/response validation

**Expected Test LOC**: ~50 lines

#### Task 4.2: Ollama Provider

**Description**: Implement Ollama HTTP provider

**Actions**:

1. Create OllamaProvider struct
2. Implement HTTP client with reqwest
3. Convert tool format to Ollama format
4. Handle errors and retries
5. Write provider-specific tests
6. Add integration tests (optional, requires Ollama running)

**Files Created**:

- `src/providers/ollama.rs` - Ollama provider (~200 lines)
- `tests/unit/ollama_provider_test.rs` - Unit tests (~100 lines)

**Complete Ollama Provider**:

```rust
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::OllamaConfig;
use crate::error::{Result, XzatomaError};
use super::base::{CompletionRequest, CompletionResponse, Message, Provider, TokenUsage, ToolCall};

/// Ollama API provider
pub struct OllamaProvider {
    client: Client,
    config: OllamaConfig,
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    tools: Vec<OllamaTool>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    r#type: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: usize,
    #[serde(default)]
    eval_count: usize,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|m| OllamaMessage {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect()
    }

    fn convert_tools(&self, tools: &[crate::tools::Tool]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                r#type: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/api/chat", self.config.host);

        let ollama_request = OllamaRequest {
            model: self.config.model.clone(),
            messages: self.convert_messages(&request.messages),
            tools: self.convert_tools(&request.tools),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Ollama request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(XzatomaError::Provider(format!(
                "Ollama returned error {}: {}",
                status, error_text
            )));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse Ollama response: {}", e)))?;

        Ok(CompletionResponse {
            message: Message {
                role: ollama_response.message.role,
                content: ollama_response.message.content,
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: if ollama_response.done { "stop" } else { "length" }.to_string(),
            usage: TokenUsage {
                prompt_tokens: ollama_response.prompt_eval_count,
                completion_tokens: ollama_response.eval_count,
                total_tokens: ollama_response.prompt_eval_count + ollama_response.eval_count,
            },
        })
    }
}
```

**Testing Requirements**:

1. Message conversion tests
2. Tool format conversion tests
3. HTTP request/response handling
4. Error handling (network, HTTP errors, parse errors)

**Expected Test LOC**: ~100 lines

#### Task 4.3: GitHub Copilot Provider with OAuth

**Description**: Implement GitHub Copilot provider with OAuth device flow

**Actions**:

1. Create CopilotProvider struct
2. Implement OAuth device flow
3. Token storage in system keyring
4. Token refresh logic
5. Copilot API integration
6. Write provider tests

**Files Created**:

- `src/providers/copilot.rs` - Copilot provider (~250 lines)
- `tests/unit/copilot_provider_test.rs` - Unit tests (~100 lines)

**Complete Copilot Provider** (OAuth implementation):

```rust
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use super::base::{CompletionRequest, CompletionResponse, Message, Provider, TokenUsage};

const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_COMPLETIONS_URL: &str = "https://api.githubcopilot.com/chat/completions";

/// GitHub Copilot provider
pub struct CopilotProvider {
    client: Client,
    config: CopilotConfig,
    keyring_service: String,
    keyring_user: String,
}

#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    scope: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedToken {
    github_token: String,
    copilot_token: String,
    expires_at: u64,
}

impl CopilotProvider {
    pub fn new(config: CopilotConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            config,
            keyring_service: "xzatoma".to_string(),
            keyring_user: "github_copilot".to_string(),
        })
    }

    async fn authenticate(&self) -> Result<String> {
        // Check keyring for cached token
        if let Ok(cached) = self.get_cached_token() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now + 300 {
                tracing::debug!("Using cached Copilot token");
                return Ok(cached.copilot_token);
            }
        }

        // Perform device flow
        tracing::info!("Starting GitHub OAuth device flow...");
        let github_token = self.device_flow().await?;

        // Exchange for Copilot token
        let copilot_token = self.get_copilot_token(&github_token).await?;

        // Cache token
        let cached = CachedToken {
            github_token,
            copilot_token: copilot_token.clone(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
        };

        let _ = self.cache_token(&cached);

        Ok(copilot_token)
    }

    async fn device_flow(&self) -> Result<String> {
        // Request device code
        let device_response: DeviceCodeResponse = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .json(&DeviceCodeRequest {
                client_id: "Iv1.b507a08c87ecfe98".to_string(),
                scope: "read:user".to_string(),
            })
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Device code request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse device code: {}", e)))?;

        println!("\nGitHub Authentication Required:");
        println!("  1. Visit: {}", device_response.verification_uri);
        println!("  2. Enter code: {}", device_response.user_code);
        println!("\nWaiting for authorization...");

        // Poll for token
        let interval = Duration::from_secs(device_response.interval);
        let max_attempts = device_response.expires_in / device_response.interval;

        for _ in 0..max_attempts {
            tokio::time::sleep(interval).await;

            let response = self
                .client
                .post(GITHUB_TOKEN_URL)
                .header("Accept", "application/json")
                .json(&TokenRequest {
                    client_id: "Iv1.b507a08c87ecfe98".to_string(),
                    device_code: device_response.device_code.clone(),
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                })
                .send()
                .await
                .map_err(|e| XzatomaError::Provider(format!("Token poll failed: {}", e)))?;

            if response.status().is_success() {
                let token_response: TokenResponse = response
                    .json()
                    .await
                    .map_err(|e| XzatomaError::Provider(format!("Failed to parse token: {}", e)))?;

                println!("Authorization successful!");
                return Ok(token_response.access_token);
            }
        }

        Err(XzatomaError::Provider(
            "Device flow timed out waiting for authorization".to_string(),
        ))
    }

    async fn get_copilot_token(&self, github_token: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct CopilotTokenResponse {
            token: String,
        }

        let response: CopilotTokenResponse = self
            .client
            .get(COPILOT_TOKEN_URL)
            .header("Authorization", format!("token {}", github_token))
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Copilot token request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse Copilot token: {}", e)))?;

        Ok(response.token)
    }

    fn get_cached_token(&self) -> Result<CachedToken> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)
            .map_err(|e| XzatomaError::Keyring(format!("Keyring access failed: {}", e)))?;

        let json = entry
            .get_password()
            .map_err(|e| XzatomaError::Keyring(format!("Failed to read token: {}", e)))?;

        serde_json::from_str(&json)
            .map_err(|e| XzatomaError::Serialization(format!("Failed to parse cached token: {}", e)))
    }

    fn cache_token(&self, token: &CachedToken) -> Result<()> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)
            .map_err(|e| XzatomaError::Keyring(format!("Keyring access failed: {}", e)))?;

        let json = serde_json::to_string(token)
            .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize token: {}", e)))?;

        entry
            .set_password(&json)
            .map_err(|e| XzatomaError::Keyring(format!("Failed to store token: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    fn name(&self) -> &str {
        "copilot"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let token = self.authenticate().await?;

        // Convert to Copilot format and make request
        // Implementation similar to Ollama but with Copilot API format

        todo!("Implement Copilot completion request")
    }
}
```

**Testing Requirements**:

1. OAuth flow simulation
2. Token caching and retrieval
3. Token refresh logic
4. Copilot API request/response
5. Error handling

**Expected Test LOC**: ~100 lines

#### Task 4.4: Deliverables

Phase 4 Complete Deliverables:

- `src/providers/mod.rs` (~50 lines)
- `src/providers/base.rs` (~100 lines)
- `src/providers/ollama.rs` (~200 lines)
- `src/providers/copilot.rs` (~250 lines)
- `tests/unit/ollama_provider_test.rs` (~100 lines)
- `tests/unit/copilot_provider_test.rs` (~100 lines)

**Total**: ~800 lines (close to 900 target)

#### Task 4.5: Success Criteria

Phase 4 complete when:

- [ ] Provider trait defined with proper types
- [ ] Ollama provider works with local Ollama instance
- [ ] Copilot OAuth device flow completes successfully
- [ ] Token caching works in system keyring
- [ ] Both providers convert tool formats correctly
- [ ] Error handling covers network, auth, and API errors
- [ ] `cargo fmt --all` applied
- [ ] `cargo check --all-targets --all-features` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] Documentation updated in `docs/explanation/`

---

### Phase 5: File Tools and Plan Parsing

**Timeline**: Weeks 9-10

**Objective**: Implement file operation tools and plan parsing for structured execution.

**Total LOC Target**: ~800 lines (550 production + 250 tests)

#### Task 5.1: File Operation Tools

**Description**: Implement safe file operations

**Actions**:

1. Create FileOpsTool struct
2. Implement list_files (with pattern matching)
3. Implement read_file (with size limits)
4. Implement write_file (with path validation)
5. Implement delete_file (with confirmation)
6. Implement file_diff (show differences)
7. Write comprehensive tests

**Files Created**:

- `src/tools/file_ops.rs` - File operations (~300 lines)
- `tests/unit/file_ops_test.rs` - File operation tests (~150 lines)

**Complete File Operations Tool**:

```rust
use async_trait::async_trait;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::ToolsConfig;
use crate::error::{Result, XzatomaError};
use crate::tools::{Tool, ToolExecutor, ToolResult};

/// File operations tool
pub struct FileOpsTool {
    working_dir: PathBuf,
    config: ToolsConfig,
}

impl FileOpsTool {
    pub fn new(working_dir: PathBuf, config: ToolsConfig) -> Self {
        Self { working_dir, config }
    }

    fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        // Prevent absolute paths
        if path.is_absolute() {
            return Err(XzatomaError::PathOutsideWorkingDirectory(
                "Absolute paths not allowed".to_string(),
            ));
        }

        // Resolve relative to working directory
        let full_path = self.working_dir.join(path);
        let canonical = full_path
            .canonicalize()
            .unwrap_or(full_path.clone());

        // Ensure path is within working directory
        let canonical_working = self.working_dir.canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        if !canonical.starts_with(&canonical_working) {
            return Err(XzatomaError::PathOutsideWorkingDirectory(
                format!("Path escapes working directory: {:?}", path),
            ));
        }

        Ok(canonical)
    }

    async fn list_files(&self, pattern: Option<String>) -> Result<ToolResult> {
        let mut files = Vec::new();

        for entry in WalkDir::new(&self.working_dir)
            .max_depth(10)
            .follow_links(false)
        {
            let entry = entry.map_err(|e| XzatomaError::Tool(e.to_string()))?;

            if entry.file_type().is_file() {
                let path = entry.path()
                    .strip_prefix(&self.working_dir)
                    .unwrap_or(entry.path());

                let path_str = path.to_string_lossy();

                // Apply pattern filter if provided
                if let Some(ref pat) = pattern {
                    if !path_str.contains(pat) {
                        continue;
                    }
                }

                files.push(path_str.to_string());
            }
        }

        files.sort();
        let output = files.join("\n");

        Ok(ToolResult::success(output))
    }

    async fn read_file(&self, path: &str) -> Result<ToolResult> {
        let path = Path::new(path);
        let full_path = self.validate_path(path)?;

        // Check file size
        let metadata = fs::metadata(&full_path)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read file metadata: {}", e)))?;

        if metadata.len() > self.config.max_file_read_size as u64 {
            return Ok(ToolResult::error(format!(
                "File too large: {} bytes (max: {})",
                metadata.len(),
                self.config.max_file_read_size
            )));
        }

        // Read file
        let content = fs::read_to_string(&full_path)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read file: {}", e)))?;

        Ok(ToolResult::success(content))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<ToolResult> {
        let path = Path::new(path);

        // For new files, validate parent directory is within working dir
        let full_path = self.working_dir.join(path);
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .map_err(|e| XzatomaError::Tool(format!("Failed to create directory: {}", e)))?;
            }
        }

        fs::write(&full_path, content)
            .map_err(|e| XzatomaError::Tool(format!("Failed to write file: {}", e)))?;

        Ok(ToolResult::success(format!("File written: {}", path.display())))
    }

    async fn delete_file(&self, path: &str) -> Result<ToolResult> {
        let path = Path::new(path);
        let full_path = self.validate_path(path)?;

        fs::remove_file(&full_path)
            .map_err(|e| XzatomaError::Tool(format!("Failed to delete file: {}", e)))?;

        Ok(ToolResult::success(format!("File deleted: {}", path.display())))
    }

    async fn file_diff(&self, path1: &str, path2: &str) -> Result<ToolResult> {
        let full_path1 = self.validate_path(Path::new(path1))?;
        let full_path2 = self.validate_path(Path::new(path2))?;

        let content1 = fs::read_to_string(&full_path1)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read {}: {}", path1, e)))?;
        let content2 = fs::read_to_string(&full_path2)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read {}: {}", path2, e)))?;

        let diff = TextDiff::from_lines(&content1, &content2);
        let mut output = Vec::new();

        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            output.push(format!("{}{}", sign, change));
        }

        Ok(ToolResult::success(output.join("")))
    }
}

#[async_trait]
impl ToolExecutor for FileOpsTool {
    fn tool_definition(&self) -> Tool {
        Tool {
            name: "file_ops".to_string(),
            description: "File operations: list, read, write, delete, diff".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["list", "read", "write", "delete", "diff"],
                        "description": "The file operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "File path relative to working directory"
                    },
                    "path2": {
                        "type": "string",
                        "description": "Second file path for diff operation"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content for write operation"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Pattern filter for list operation"
                    }
                },
                "required": ["operation"]
            }),
        }
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolResult> {
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| XzatomaError::Tool("Missing 'operation' parameter".to_string()))?;

        match operation {
            "list" => {
                let pattern = params["pattern"].as_str().map(String::from);
                self.list_files(pattern).await
            }
            "read" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'path' parameter".to_string()))?;
                self.read_file(path).await
            }
            "write" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'path' parameter".to_string()))?;
                let content = params["content"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'content' parameter".to_string()))?;
                self.write_file(path, content).await
            }
            "delete" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'path' parameter".to_string()))?;
                self.delete_file(path).await
            }
            "diff" => {
                let path1 = params["path"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'path' parameter".to_string()))?;
                let path2 = params["path2"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::Tool("Missing 'path2' parameter".to_string()))?;
                self.file_diff(path1, path2).await
            }
            _ => Ok(ToolResult::error(format!("Unknown operation: {}", operation))),
        }
    }
}
```

**Testing Requirements**:

1. Path validation tests
2. List files with patterns
3. Read file with size limits
4. Write file creates directories
5. Delete file works
6. Diff shows changes correctly

**Expected Test LOC**: ~150 lines

#### Task 5.2: Plan Parsing

**Description**: Parse execution plans from YAML, JSON, and Markdown

**Actions**:

1. Define Plan structure
2. Implement YAML plan parser
3. Implement JSON plan parser
4. Implement Markdown plan parser
5. Add plan validation
6. Write parser tests

**Files Created**:

- `src/tools/plan.rs` - Plan parsing (~250 lines)
- `tests/unit/plan_parser_test.rs` - Parser tests (~100 lines)
- `tests/fixtures/example_plan.yaml` - Test plan

**Complete Plan Parser**:

````rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::error::{Result, XzatomaError};

/// Execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<PlanStep>,
}

/// Plan step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub name: String,
    pub action: String,
    pub context: Option<String>,
}

/// Plan parser
pub struct PlanParser;

impl PlanParser {
    /// Parse plan from file
    pub fn from_file(path: &Path) -> Result<Plan> {
        let content = fs::read_to_string(path)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read plan file: {}", e)))?;

        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| XzatomaError::Tool("Plan file has no extension".to_string()))?;

        match extension {
            "yaml" => Self::from_yaml(&content),
            "json" => Self::from_json(&content),
            "md" => Self::from_markdown(&content),
            _ => Err(XzatomaError::Tool(format!(
                "Unsupported plan format: {}",
                extension
            ))),
        }
    }

    /// Parse YAML plan
    pub fn from_yaml(content: &str) -> Result<Plan> {
        serde_yaml::from_str(content)
            .map_err(|e| XzatomaError::Yaml(format!("Failed to parse YAML plan: {}", e)))
    }

    /// Parse JSON plan
    pub fn from_json(content: &str) -> Result<Plan> {
        serde_json::from_str(content)
            .map_err(|e| XzatomaError::Serialization(format!("Failed to parse JSON plan: {}", e)))
    }

    /// Parse Markdown plan
    pub fn from_markdown(content: &str) -> Result<Plan> {
        let mut name = String::new();
        let mut description = None;
        let mut steps = Vec::new();

        let mut current_step: Option<PlanStep> = None;
        let mut in_code_block = false;
        let mut code_content = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("# ") && name.is_empty() {
                name = trimmed[2..].to_string();
            } else if trimmed.starts_with("## ") {
                // Save previous step
                if let Some(mut step) = current_step.take() {
                    if !code_content.is_empty() {
                        step.context = Some(code_content.clone());
                        code_content.clear();
                    }
                    steps.push(step);
                }

                // Start new step
                current_step = Some(PlanStep {
                    name: trimmed[3..].to_string(),
                    action: String::new(),
                    context: None,
                });
            } else if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                if !in_code_block && !code_content.is_empty() {
                    code_content.push('\n');
                }
            } else if in_code_block {
                code_content.push_str(line);
                code_content.push('\n');
            } else if let Some(ref mut step) = current_step {
                if step.action.is_empty() && !trimmed.is_empty() {
                    step.action = trimmed.to_string();
                }
            } else if description.is_none() && !trimmed.is_empty() && !name.is_empty() {
                description = Some(trimmed.to_string());
            }
        }

        // Save last step
        if let Some(mut step) = current_step {
            if !code_content.is_empty() {
                step.context = Some(code_content);
            }
            steps.push(step);
        }

        if name.is_empty() {
            return Err(XzatomaError::Tool("Plan has no name (# heading)".to_string()));
        }

        if steps.is_empty() {
            return Err(XzatomaError::Tool("Plan has no steps (## headings)".to_string()));
        }

        Ok(Plan {
            name,
            description,
            steps,
        })
    }

    /// Validate plan
    pub fn validate(plan: &Plan) -> Result<()> {
        if plan.name.is_empty() {
            return Err(XzatomaError::Tool("Plan name cannot be empty".to_string()));
        }

        if plan.steps.is_empty() {
            return Err(XzatomaError::Tool("Plan must have at least one step".to_string()));
        }

        for (i, step) in plan.steps.iter().enumerate() {
            if step.name.is_empty() {
                return Err(XzatomaError::Tool(format!("Step {} has no name", i + 1)));
            }
            if step.action.is_empty() {
                return Err(XzatomaError::Tool(format!("Step '{}' has no action", step.name)));
            }
        }

        Ok(())
    }
}
````

**Example Plan Format (YAML)**:

```yaml
name: Setup Project
description: Initialize a new Rust project

steps:
  - name: Create project
    action: Run cargo init command
    context: cargo init --bin my-project

  - name: Add dependencies
    action: Update Cargo.toml with required dependencies
    context: |
      [dependencies]
      tokio = { version = "1", features = ["full"] }
      serde = { version = "1", features = ["derive"] }

  - name: Verify setup
    action: Check that project compiles
    context: cargo check
```

**Testing Requirements**:

1. Parse valid YAML plans
2. Parse valid JSON plans
3. Parse valid Markdown plans
4. Reject invalid plans
5. Validate plan structure

**Expected Test LOC**: ~100 lines

#### Task 5.3: Deliverables

Phase 5 Complete Deliverables:

- `src/tools/file_ops.rs` (~300 lines)
- `src/tools/plan.rs` (~250 lines)
- `tests/unit/file_ops_test.rs` (~150 lines)
- `tests/unit/plan_parser_test.rs` (~100 lines)
- `tests/fixtures/example_plan.yaml` (~20 lines)

**Total**: ~820 lines (exceeds 800 target)

#### Task 5.4: Success Criteria

Phase 5 complete when:

- [ ] All file operations work with path validation
- [ ] File size limits enforced
- [ ] All plan formats parse correctly
- [ ] Plan validation works
- [ ] Markdown plan parser handles code blocks
- [ ] `cargo fmt --all` applied
- [ ] `cargo check --all-targets --all-features` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] Documentation updated in `docs/explanation/`

---

### Phase 6: CLI Integration and Polish

**Timeline**: Weeks 11-12

**Objective**: Complete CLI implementation, integrate all components, and polish for production.

**Total LOC Target**: ~600 lines (400 production + 200 tests)

#### Task 6.1: Command Handlers

**Description**: Implement CLI command handlers

**Actions**:

1. Implement chat command handler
2. Implement run command handler
3. Implement auth command handler
4. Add interactive mode with rustyline
5. Add logging setup
6. Write end-to-end tests

**Files Created**:

- `src/commands/mod.rs` - Command module (~30 lines)
- `src/commands/chat.rs` - Chat command (~150 lines)
- `src/commands/run.rs` - Run command (~100 lines)
- `src/commands/auth.rs` - Auth command (~50 lines)
- `tests/integration/e2e_test.rs` - End-to-end tests (~150 lines)

**Complete Chat Command**:

```rust
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::PathBuf;

use crate::agent::Agent;
use crate::config::{Config, ExecutionMode};
use crate::error::Result;
use crate::providers::{CopilotProvider, OllamaProvider, Provider};
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::tools::{FileOpsTool, ToolRegistry};

/// Run interactive chat mode
pub async fn run_chat(config: Config, provider_name: Option<String>) -> Result<()> {
    // Initialize provider
    let provider: Box<dyn Provider> = match provider_name
        .as_deref()
        .unwrap_or(&config.provider.provider_type)
    {
        "ollama" => Box::new(OllamaProvider::new(config.provider.ollama.clone())?),
        "copilot" => Box::new(CopilotProvider::new(config.provider.copilot.clone())?),
        other => {
            return Err(crate::error::XzatomaError::Config(format!(
                "Unknown provider: {}",
                other
            )))
        }
    };

    // Initialize tools
    let mut tools = ToolRegistry::new();

    let working_dir = std::env::current_dir()?;

    let validator = CommandValidator::new(
        config.agent.terminal.default_mode.clone(),
        working_dir.clone(),
    );
    let terminal_tool = TerminalTool::new(validator, config.agent.terminal.clone());
    tools.register(Box::new(terminal_tool));

    let file_tool = FileOpsTool::new(working_dir, config.agent.tools.clone());
    tools.register(Box::new(file_tool));

    // Initialize agent
    let mut agent = Agent::new(provider, tools, config.agent.clone());

    // Interactive loop
    let mut rl = DefaultEditor::new()?;
    println!("XZatoma Interactive Mode");
    println!("Type 'exit' or 'quit' to exit\n");

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let input = line.trim();

                if input.is_empty() {
                    continue;
                }

                if input == "exit" || input == "quit" {
                    println!("Goodbye!");
                    break;
                }

                rl.add_history_entry(input)?;

                // Execute with agent
                match agent.execute(input.to_string()).await {
                    Ok(response) => {
                        println!("\n{}\n", response);
                    }
                    Err(e) => {
                        eprintln!("Error: {}\n", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    Ok(())
}
```

**Complete Run Command**:

```rust
use std::path::Path;

use crate::agent::Agent;
use crate::config::Config;
use crate::error::Result;
use crate::providers::{CopilotProvider, OllamaProvider, Provider};
use crate::tools::plan::PlanParser;
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::tools::{FileOpsTool, ToolRegistry};

/// Run plan from file or prompt
pub async fn run_plan(
    config: Config,
    plan_path: Option<String>,
    prompt: Option<String>,
) -> Result<()> {
    // Initialize provider
    let provider: Box<dyn Provider> = match config.provider.provider_type.as_str() {
        "ollama" => Box::new(OllamaProvider::new(config.provider.ollama.clone())?),
        "copilot" => Box::new(CopilotProvider::new(config.provider.copilot.clone())?),
        other => {
            return Err(crate::error::XzatomaError::Config(format!(
                "Unknown provider: {}",
                other
            )))
        }
    };

    // Initialize tools
    let mut tools = ToolRegistry::new();

    let working_dir = std::env::current_dir()?;

    let validator = CommandValidator::new(
        config.agent.terminal.default_mode.clone(),
        working_dir.clone(),
    );
    let terminal_tool = TerminalTool::new(validator, config.agent.terminal.clone());
    tools.register(Box::new(terminal_tool));

    let file_tool = FileOpsTool::new(working_dir, config.agent.tools.clone());
    tools.register(Box::new(file_tool));

    // Initialize agent
    let mut agent = Agent::new(provider, tools, config.agent.clone());

    // Determine what to execute
    let task = if let Some(path) = plan_path {
        // Parse plan from file
        let plan = PlanParser::from_file(Path::new(&path))?;
        PlanParser::validate(&plan)?;

        format!(
            "Execute this plan:\n\nName: {}\n\nSteps:\n{}",
            plan.name,
            plan.steps
                .iter()
                .map(|s| format!("- {}: {}", s.name, s.action))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else if let Some(prompt_text) = prompt {
        prompt_text
    } else {
        return Err(crate::error::XzatomaError::Config(
            "Either --plan or --prompt must be provided".to_string(),
        ));
    };

    // Execute
    println!("Executing task...\n");
    match agent.execute(task).await {
        Ok(response) => {
            println!("Result:\n{}", response);
            Ok(())
        }
        Err(e) => {
            eprintln!("Execution failed: {}", e);
            Err(e)
        }
    }
}
```

**Testing Requirements**:

1. Chat mode interaction
2. Plan execution
3. Prompt execution
4. Error handling
5. Tool integration

**Expected Test LOC**: ~150 lines

#### Task 6.2: Main Entry Point

**Description**: Complete main.rs with all command routing

**Actions**:

1. Set up tracing/logging
2. Route CLI commands to handlers
3. Handle errors gracefully
4. Add version and help information

**Files Modified**:

- `src/main.rs` - Main entry point (~70 lines)

**Complete Main**:

```rust
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod agent;
mod cli;
mod commands;
mod config;
mod error;
mod providers;
mod tools;

use cli::{Cli, Commands};
use error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse_args();

    // Set up logging
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();

    // Load configuration
    let config = config::Config::load(cli.config.as_deref())?;
    config.validate()?;

    // Route commands
    match cli.command {
        Commands::Chat { provider } => {
            commands::chat::run_chat(config, provider).await?;
        }
        Commands::Run {
            plan,
            prompt,
            allow_dangerous: _,
        } => {
            commands::run::run_plan(config, plan, prompt).await?;
        }
        Commands::Auth { provider } => {
            commands::auth::authenticate(config, provider).await?;
        }
    }

    Ok(())
}
```

#### Task 6.3: Documentation and Examples

**Description**: Complete project documentation

**Actions**:

1. Update README.md with usage examples
2. Create example plans
3. Document configuration options
4. Add troubleshooting guide

**Files Created/Updated**:

- `README.md` - Project overview and quick start
- `examples/simple_plan.yaml` - Simple example plan
- `examples/complex_plan.yaml` - Complex example plan
- `docs/how-to/configuration.md` - Configuration guide
- `docs/how-to/troubleshooting.md` - Troubleshooting guide

#### Task 6.4: Deliverables

Phase 6 Complete Deliverables:

- `src/commands/mod.rs` (~30 lines)
- `src/commands/chat.rs` (~150 lines)
- `src/commands/run.rs` (~100 lines)
- `src/commands/auth.rs` (~50 lines)
- `src/main.rs` (~70 lines)
- `tests/integration/e2e_test.rs` (~150 lines)
- Documentation updates (~50 lines)

**Total**: ~600 lines

#### Task 6.5: Success Criteria

Phase 6 complete when:

- [ ] Chat mode works interactively
- [ ] Run command executes plans
- [ ] Auth command completes OAuth flow
- [ ] All commands handle errors gracefully
- [ ] Logging works at appropriate levels
- [ ] README has clear usage instructions
- [ ] Example plans demonstrate capabilities
- [ ] `cargo fmt --all` applied
- [ ] `cargo check --all-targets --all-features` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] Documentation complete in all categories

---

## Project Summary

### Total Implementation

**Total LOC Estimate**: 4,770 lines

- Production Code: 3,120 lines
- Test Code: 1,650 lines

### All Phases Complete

1. **Phase 1**: Foundation (800 lines)
2. **Phase 2**: Agent Core (1,000 lines)
3. **Phase 3**: Security (750 lines)
4. **Phase 4**: Providers (800 lines)
5. **Phase 5**: File Tools (820 lines)
6. **Phase 6**: CLI Integration (600 lines)

### Final Validation

Before declaring v1.0.0 complete:

- [ ] All 6 phases completed
- [ ] All quality gates pass across all modules
- [ ] Test coverage >80% overall
- [ ] All security tests pass
- [ ] Documentation complete (tutorials, how-to, explanation, reference)
- [ ] Example plans work end-to-end
- [ ] CI/CD pipeline operational
- [ ] Binary builds for major platforms
- [ ] No file naming violations (`.yaml`, `lowercase_with_underscores.md`)
- [ ] No emojis in documentation

### Timeline Summary

- Weeks 1-2: Phase 1 (Foundation)
- Weeks 3-4: Phase 2 (Agent Core)
- Weeks 5-6: Phase 3 (Security)
- Weeks 7-8: Phase 4 (Providers)
- Weeks 9-10: Phase 5 (File Tools)
- Weeks 11-12: Phase 6 (CLI Integration)

**Total**: 12 weeks to v1.0.0

---

**Document Version**: 1.1
**Created**: 2025-01-15
**Last Updated**: 2025-01-15
**Status**: Complete and Ready for Implementation
