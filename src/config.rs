//! Configuration management for XZatoma
//!
//! This module handles loading, parsing, validating, and managing
//! configuration from files, environment variables, and CLI overrides.

use crate::error::{Result, XzatomaError};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration structure for XZatoma
///
/// This structure holds all configuration needed for the agent,
/// including provider settings, agent behavior, and tool configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Provider configuration (Copilot, Ollama, etc.)
    pub provider: ProviderConfig,
    /// Agent behavior configuration
    pub agent: AgentConfig,
    /// Watcher configuration for Kafka event monitoring
    #[serde(default)]
    pub watcher: WatcherConfig,
}

/// Provider configuration
///
/// Specifies which AI provider to use and its settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Type of provider to use
    #[serde(rename = "type")]
    pub provider_type: String,

    /// GitHub Copilot configuration
    #[serde(default)]
    pub copilot: CopilotConfig,

    /// Ollama configuration
    #[serde(default)]
    pub ollama: OllamaConfig,
}

/// GitHub Copilot provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// Model to use for Copilot
    #[serde(default = "default_copilot_model")]
    pub model: String,

    /// Optional API base URL for Copilot endpoints (useful for tests and local mocks)
    ///
    /// When set, this base is used to build Copilot endpoints (e.g. `/models`,
    /// `/chat/completions`, `/copilot_internal/v2/token`) which allows tests to
    /// point the provider at a mock server.
    #[serde(default)]
    pub api_base: Option<String>,
}

fn default_copilot_model() -> String {
    "gpt-5-mini".to_string()
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: default_copilot_model(),
            api_base: None,
        }
    }
}

/// Ollama provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Ollama server host
    #[serde(default = "default_ollama_host")]
    pub host: String,

    /// Model to use for Ollama
    #[serde(default = "default_ollama_model")]
    pub model: String,
}

fn default_ollama_host() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2:latest".to_string()
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: default_ollama_host(),
            model: default_ollama_model(),
        }
    }
}

/// Agent behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum number of agent turns before stopping
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,

    /// Timeout for entire agent execution (seconds)
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

    /// Chat mode settings
    #[serde(default)]
    pub chat: ChatConfig,
}

fn default_max_turns() -> usize {
    50
}

fn default_timeout() -> u64 {
    300
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: default_max_turns(),
            timeout_seconds: default_timeout(),
            conversation: ConversationConfig::default(),
            tools: ToolsConfig::default(),
            terminal: TerminalConfig::default(),
            chat: ChatConfig::default(),
        }
    }
}

/// Chat mode configuration
///
/// Settings for interactive chat sessions, including default modes
/// and safety behaviors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Default chat mode: "planning" or "write"
    #[serde(default = "default_chat_mode")]
    pub default_mode: String,

    /// Default safety mode: "confirm" or "yolo"
    #[serde(default = "default_safety_mode")]
    pub default_safety: String,

    /// Allow switching between modes during a session
    #[serde(default = "default_allow_mode_switching")]
    pub allow_mode_switching: bool,
}

fn default_chat_mode() -> String {
    "planning".to_string()
}

fn default_safety_mode() -> String {
    "confirm".to_string()
}

fn default_allow_mode_switching() -> bool {
    true
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            default_mode: default_chat_mode(),
            default_safety: default_safety_mode(),
            allow_mode_switching: default_allow_mode_switching(),
        }
    }
}

/// Conversation management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    /// Maximum tokens allowed in conversation context
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Minimum number of turns to retain when pruning
    #[serde(default = "default_min_retain")]
    pub min_retain_turns: usize,

    /// Token threshold to trigger pruning (percentage of max_tokens)
    #[serde(default = "default_prune_threshold")]
    pub prune_threshold: f32,
}

fn default_max_tokens() -> usize {
    100_000
}

fn default_min_retain() -> usize {
    5
}

fn default_prune_threshold() -> f32 {
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
    /// Maximum size of tool output (bytes)
    #[serde(default = "default_max_output")]
    pub max_output_size: usize,

    /// Maximum size of file to read (bytes)
    #[serde(default = "default_max_file_read")]
    pub max_file_read_size: usize,

    /// Maximum results per page for grep tool
    #[serde(default = "default_grep_max_results_per_page")]
    pub grep_max_results_per_page: usize,

    /// Number of context lines to show around grep matches
    #[serde(default = "default_grep_context_lines")]
    pub grep_context_lines: usize,

    /// Maximum file size for grep searches (bytes)
    #[serde(default = "default_grep_max_file_size")]
    pub grep_max_file_size: u64,

    /// File patterns to exclude from grep searches
    #[serde(default = "default_grep_excluded_patterns")]
    pub grep_excluded_patterns: Vec<String>,

    /// Fetch tool timeout in seconds (default: 30)
    #[serde(default = "default_fetch_timeout_seconds")]
    pub fetch_timeout_seconds: u64,

    /// Maximum size of fetched content (bytes, default: 5 MB)
    #[serde(default = "default_max_fetch_size_bytes")]
    pub max_fetch_size_bytes: usize,

    /// Maximum number of fetch requests per minute (default: 10)
    #[serde(default = "default_max_fetches_per_minute")]
    pub max_fetches_per_minute: u32,

    /// Optional allowlist of domains for fetch tool
    #[serde(default)]
    pub fetch_allowed_domains: Option<Vec<String>>,

    /// Optional blocklist of domains for fetch tool
    #[serde(default)]
    pub fetch_blocked_domains: Option<Vec<String>>,
}

fn default_max_output() -> usize {
    1_048_576 // 1 MB
}

fn default_max_file_read() -> usize {
    10_485_760 // 10 MB
}

fn default_grep_max_results_per_page() -> usize {
    20
}

fn default_grep_context_lines() -> usize {
    2
}

fn default_grep_max_file_size() -> u64 {
    1_048_576 // 1 MB
}

fn default_grep_excluded_patterns() -> Vec<String> {
    vec![
        "*.lock".to_string(),
        "target/**".to_string(),
        "node_modules/**".to_string(),
        ".git/**".to_string(),
    ]
}

fn default_fetch_timeout_seconds() -> u64 {
    30
}

fn default_max_fetch_size_bytes() -> usize {
    5 * 1024 * 1024 // 5 MB
}

fn default_max_fetches_per_minute() -> u32 {
    10
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_output_size: default_max_output(),
            max_file_read_size: default_max_file_read(),
            grep_max_results_per_page: default_grep_max_results_per_page(),
            grep_context_lines: default_grep_context_lines(),
            grep_max_file_size: default_grep_max_file_size(),
            grep_excluded_patterns: default_grep_excluded_patterns(),
            fetch_timeout_seconds: default_fetch_timeout_seconds(),
            max_fetch_size_bytes: default_max_fetch_size_bytes(),
            max_fetches_per_minute: default_max_fetches_per_minute(),
            fetch_allowed_domains: None,
            fetch_blocked_domains: None,
        }
    }
}

/// Terminal execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Default execution mode
    #[serde(default)]
    pub default_mode: ExecutionMode,

    /// Timeout for terminal commands (seconds)
    #[serde(default = "default_command_timeout")]
    pub timeout_seconds: u64,

    /// Maximum stdout size (bytes)
    #[serde(default = "default_max_stdout")]
    pub max_stdout_bytes: usize,

    /// Maximum stderr size (bytes)
    #[serde(default = "default_max_stderr")]
    pub max_stderr_bytes: usize,
}

fn default_command_timeout() -> u64 {
    30
}

fn default_max_stdout() -> usize {
    1_048_576 // 1 MB
}

fn default_max_stderr() -> usize {
    262_144 // 256 KB
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
///
/// Controls how terminal commands are validated and executed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Require explicit user confirmation for each command
    Interactive,
    /// Allow safe commands automatically, require confirmation for dangerous ones
    #[default]
    RestrictedAutonomous,
    /// Allow all commands without confirmation (use with caution)
    FullAutonomous,
}

impl Config {
    /// Load configuration from file with environment and CLI overrides
    ///
    /// # Arguments
    ///
    /// * `path` - Path to configuration file
    /// * `cli` - CLI arguments for overrides
    ///
    /// # Returns
    ///
    /// Returns the loaded and merged configuration
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be read or parsed
    pub fn load(path: &str, cli: &crate::cli::Cli) -> Result<Self> {
        let mut config = if Path::new(path).exists() {
            Self::from_file(path)?
        } else {
            tracing::warn!("Config file not found at {}, using defaults", path);
            Self::default_config()
        };

        config.apply_env_vars();
        config.apply_cli_overrides(cli);

        Ok(config)
    }

    fn default_config() -> Self {
        Self {
            provider: ProviderConfig {
                provider_type: "copilot".to_string(),
                copilot: CopilotConfig::default(),
                ollama: OllamaConfig::default(),
            },
            agent: AgentConfig::default(),
            watcher: WatcherConfig::default(),
        }
    }

    fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| XzatomaError::Config(format!("Failed to read config file: {}", e)))?;
        serde_yaml::from_str(&contents)
            .map_err(|e| XzatomaError::Config(format!("Failed to parse config: {}", e)).into())
    }

    fn apply_env_vars(&mut self) {
        // Provider overrides
        if let Ok(provider_type) = std::env::var("XZATOMA_PROVIDER") {
            self.provider.provider_type = provider_type;
        }

        if let Ok(copilot_model) = std::env::var("XZATOMA_COPILOT_MODEL") {
            self.provider.copilot.model = copilot_model;
        }

        if let Ok(ollama_host) = std::env::var("XZATOMA_OLLAMA_HOST") {
            self.provider.ollama.host = ollama_host;
        }

        if let Ok(ollama_model) = std::env::var("XZATOMA_OLLAMA_MODEL") {
            self.provider.ollama.model = ollama_model;
        }

        // Agent overrides
        if let Ok(max_turns) = std::env::var("XZATOMA_MAX_TURNS") {
            if let Ok(value) = max_turns.parse() {
                self.agent.max_turns = value;
            } else {
                tracing::warn!("Invalid XZATOMA_MAX_TURNS: {}", max_turns);
            }
        }

        if let Ok(timeout) = std::env::var("XZATOMA_TIMEOUT_SECONDS") {
            if let Ok(value) = timeout.parse() {
                self.agent.timeout_seconds = value;
            } else {
                tracing::warn!("Invalid XZATOMA_TIMEOUT_SECONDS: {}", timeout);
            }
        }

        if let Ok(mode) = std::env::var("XZATOMA_EXECUTION_MODE") {
            self.agent.terminal.default_mode = match mode.to_lowercase().as_str() {
                "interactive" => ExecutionMode::Interactive,
                "restricted_autonomous" => ExecutionMode::RestrictedAutonomous,
                "full_autonomous" => ExecutionMode::FullAutonomous,
                _ => {
                    tracing::warn!("Invalid execution mode: {}, using default", mode);
                    ExecutionMode::default()
                }
            };
        }

        // ---------------------------------------------------------------------
        // Watcher-specific environment variable overrides
        // Supports: XZATOMA_WATCHER_* for filters, logging, execution
        // and XZEPR_KAFKA_* for Kafka connection overrides.
        // ---------------------------------------------------------------------

        // Filter overrides
        if let Ok(event_types) = std::env::var("XZATOMA_WATCHER_EVENT_TYPES") {
            let types_vec: Vec<String> = event_types
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !types_vec.is_empty() {
                self.watcher.filters.event_types = types_vec.clone();
                tracing::debug!(?types_vec, "Env override: XZATOMA_WATCHER_EVENT_TYPES");
            }
        }

        if let Ok(source_pattern) = std::env::var("XZATOMA_WATCHER_SOURCE_PATTERN") {
            self.watcher.filters.source_pattern = Some(source_pattern.clone());
            tracing::debug!(source_pattern = %source_pattern, "Env override: XZATOMA_WATCHER_SOURCE_PATTERN");
        }

        if let Ok(platform_id) = std::env::var("XZATOMA_WATCHER_PLATFORM_ID") {
            self.watcher.filters.platform_id = Some(platform_id.clone());
            tracing::debug!(platform_id = %platform_id, "Env override: XZATOMA_WATCHER_PLATFORM_ID");
        }

        if let Ok(package) = std::env::var("XZATOMA_WATCHER_PACKAGE") {
            self.watcher.filters.package = Some(package.clone());
            tracing::debug!(package = %package, "Env override: XZATOMA_WATCHER_PACKAGE");
        }

        if let Ok(api_version) = std::env::var("XZATOMA_WATCHER_API_VERSION") {
            self.watcher.filters.api_version = Some(api_version.clone());
            tracing::debug!(api_version = %api_version, "Env override: XZATOMA_WATCHER_API_VERSION");
        }

        if let Ok(success_only) = std::env::var("XZATOMA_WATCHER_SUCCESS_ONLY") {
            match success_only.parse::<bool>() {
                Ok(v) => {
                    self.watcher.filters.success_only = v;
                    tracing::debug!(
                        success_only = v,
                        "Env override: XZATOMA_WATCHER_SUCCESS_ONLY"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Invalid value for XZATOMA_WATCHER_SUCCESS_ONLY: {}",
                        success_only
                    );
                }
            }
        }

        // Logging overrides
        if let Ok(level) = std::env::var("XZATOMA_WATCHER_LOG_LEVEL") {
            self.watcher.logging.level = level.clone();
            tracing::debug!(level = %level, "Env override: XZATOMA_WATCHER_LOG_LEVEL");
        }

        if let Ok(json_logs) = std::env::var("XZATOMA_WATCHER_JSON_LOGS") {
            match json_logs.parse::<bool>() {
                Ok(v) => {
                    self.watcher.logging.json_format = v;
                    tracing::debug!(json_logs = v, "Env override: XZATOMA_WATCHER_JSON_LOGS");
                }
                Err(_) => {
                    tracing::warn!("Invalid value for XZATOMA_WATCHER_JSON_LOGS: {}", json_logs);
                }
            }
        }

        if let Ok(log_file) = std::env::var("XZATOMA_WATCHER_LOG_FILE") {
            self.watcher.logging.file_path = Some(std::path::PathBuf::from(log_file.clone()));
            tracing::debug!(log_file = %log_file, "Env override: XZATOMA_WATCHER_LOG_FILE");
        }

        if let Ok(include_payload) = std::env::var("XZATOMA_WATCHER_INCLUDE_PAYLOAD") {
            match include_payload.parse::<bool>() {
                Ok(v) => {
                    self.watcher.logging.include_payload = v;
                    tracing::debug!(
                        include_payload = v,
                        "Env override: XZATOMA_WATCHER_INCLUDE_PAYLOAD"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Invalid value for XZATOMA_WATCHER_INCLUDE_PAYLOAD: {}",
                        include_payload
                    );
                }
            }
        }

        // Execution overrides
        if let Ok(allow_dangerous) = std::env::var("XZATOMA_WATCHER_ALLOW_DANGEROUS") {
            match allow_dangerous.parse::<bool>() {
                Ok(v) => {
                    self.watcher.execution.allow_dangerous = v;
                    tracing::debug!(
                        allow_dangerous = v,
                        "Env override: XZATOMA_WATCHER_ALLOW_DANGEROUS"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Invalid value for XZATOMA_WATCHER_ALLOW_DANGEROUS: {}",
                        allow_dangerous
                    );
                }
            }
        }

        if let Ok(max_conc) = std::env::var("XZATOMA_WATCHER_MAX_CONCURRENT") {
            match max_conc.parse::<usize>() {
                Ok(v) => {
                    self.watcher.execution.max_concurrent_executions = v;
                    tracing::debug!(
                        max_concurrent = v,
                        "Env override: XZATOMA_WATCHER_MAX_CONCURRENT"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Invalid value for XZATOMA_WATCHER_MAX_CONCURRENT: {}",
                        max_conc
                    );
                }
            }
        }

        if let Ok(timeout) = std::env::var("XZATOMA_WATCHER_EXECUTION_TIMEOUT") {
            match timeout.parse::<u64>() {
                Ok(v) => {
                    self.watcher.execution.execution_timeout_secs = v;
                    tracing::debug!(
                        execution_timeout_secs = v,
                        "Env override: XZATOMA_WATCHER_EXECUTION_TIMEOUT"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Invalid value for XZATOMA_WATCHER_EXECUTION_TIMEOUT: {}",
                        timeout
                    );
                }
            }
        }

        // ---------------------------------------------------------------------
        // Kafka overrides from XZEPR_KAFKA_* environment variables
        // These can populate or override the watcher.kafka block.
        // ---------------------------------------------------------------------
        let brokers_env = std::env::var("XZEPR_KAFKA_BROKERS").ok();
        let topic_env = std::env::var("XZEPR_KAFKA_TOPIC").ok();
        let group_env = std::env::var("XZEPR_KAFKA_GROUP_ID").ok();
        let protocol_env = std::env::var("XZEPR_KAFKA_SECURITY_PROTOCOL").ok();

        if brokers_env.is_some()
            || topic_env.is_some()
            || group_env.is_some()
            || protocol_env.is_some()
        {
            let brokers = brokers_env.unwrap_or_else(|| "localhost:9092".to_string());
            let topic = topic_env.unwrap_or_else(|| "xzepr.dev.events".to_string());
            let group_id = group_env.unwrap_or_else(default_watcher_group_id);

            let security = protocol_env.map(|protocol| KafkaSecurityConfig {
                protocol,
                sasl_mechanism: std::env::var("XZEPR_KAFKA_SASL_MECHANISM").ok(),
                sasl_username: std::env::var("XZEPR_KAFKA_SASL_USERNAME").ok(),
                sasl_password: std::env::var("XZEPR_KAFKA_SASL_PASSWORD").ok(),
            });

            if let Some(ref mut kafka_cfg) = self.watcher.kafka {
                kafka_cfg.brokers = brokers;
                kafka_cfg.topic = topic;
                kafka_cfg.group_id = group_id;
                kafka_cfg.security = security;
                tracing::debug!("Overrode watcher.kafka from XZEPR_KAFKA_* env vars");
            } else {
                self.watcher.kafka = Some(KafkaWatcherConfig {
                    brokers,
                    topic,
                    group_id,
                    security,
                });
                tracing::debug!("Populated watcher.kafka from XZEPR_KAFKA_* env vars");
            }
        }
    }

    fn apply_cli_overrides(&mut self, cli: &crate::cli::Cli) {
        if cli.verbose {
            tracing::debug!("Verbose mode enabled");
        }
    }

    /// Validate the configuration
    ///
    /// Ensures all configuration values are within acceptable ranges
    /// and that required fields are properly set.
    ///
    /// # Returns
    ///
    /// Returns Ok if configuration is valid
    ///
    /// # Errors
    ///
    /// Returns error if any validation check fails
    pub fn validate(&self) -> Result<()> {
        if self.provider.provider_type.is_empty() {
            return Err(XzatomaError::Config("Provider type cannot be empty".to_string()).into());
        }

        let valid_providers = ["copilot", "ollama"];
        if !valid_providers.contains(&self.provider.provider_type.as_str()) {
            return Err(XzatomaError::Config(format!(
                "Invalid provider type: {}. Must be one of: {}",
                self.provider.provider_type,
                valid_providers.join(", ")
            ))
            .into());
        }

        if self.agent.max_turns == 0 {
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
        }

        if self.agent.max_turns > 1000 {
            return Err(XzatomaError::Config(
                "max_turns must be less than or equal to 1000".to_string(),
            )
            .into());
        }

        if self.agent.timeout_seconds == 0 {
            return Err(
                XzatomaError::Config("timeout_seconds must be greater than 0".to_string()).into(),
            );
        }

        if self.agent.conversation.max_tokens == 0 {
            return Err(XzatomaError::Config(
                "conversation.max_tokens must be greater than 0".to_string(),
            )
            .into());
        }

        if self.agent.conversation.prune_threshold <= 0.0
            || self.agent.conversation.prune_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.prune_threshold must be between 0.0 and 1.0".to_string(),
            )
            .into());
        }

        if self.agent.tools.max_output_size == 0 {
            return Err(XzatomaError::Config(
                "tools.max_output_size must be greater than 0".to_string(),
            )
            .into());
        }

        if self.agent.tools.max_file_read_size == 0 {
            return Err(XzatomaError::Config(
                "tools.max_file_read_size must be greater than 0".to_string(),
            )
            .into());
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.provider_type, "copilot");
        assert_eq!(config.agent.max_turns, 50);
        assert_eq!(config.agent.timeout_seconds, 300);
    }

    #[test]
    fn test_config_validation_success() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_empty_provider() {
        let mut config = Config::default();
        config.provider.provider_type = String::new();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_provider() {
        let mut config = Config::default();
        config.provider.provider_type = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_zero_max_turns() {
        let mut config = Config::default();
        config.agent.max_turns = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_max_turns_too_large() {
        let mut config = Config::default();
        config.agent.max_turns = 1001;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_zero_timeout() {
        let mut config = Config::default();
        config.agent.timeout_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_prune_threshold() {
        let mut config = Config::default();
        config.agent.conversation.prune_threshold = 1.5;
        assert!(config.validate().is_err());

        config.agent.conversation.prune_threshold = 0.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_from_yaml() {
        let yaml = r#"
provider:
  type: ollama
  copilot:
    model: gpt-4o
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 100
  timeout_seconds: 600
  conversation:
    max_tokens: 50000
    min_retain_turns: 10
    prune_threshold: 0.75
  tools:
    max_output_size: 2097152
    max_file_read_size: 20971520
  terminal:
    default_mode: interactive
    timeout_seconds: 60
    max_stdout_bytes: 2097152
    max_stderr_bytes: 524288
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.provider.provider_type, "ollama");
        assert_eq!(config.agent.max_turns, 100);
        assert_eq!(config.agent.timeout_seconds, 600);
        assert_eq!(config.agent.conversation.max_tokens, 50000);
        assert_eq!(
            config.agent.terminal.default_mode,
            ExecutionMode::Interactive
        );
    }

    #[test]
    fn test_execution_mode_serialization() {
        let mode = ExecutionMode::Interactive;
        let yaml = serde_yaml::to_string(&mode).unwrap();
        assert!(yaml.contains("interactive"));

        let mode = ExecutionMode::RestrictedAutonomous;
        let yaml = serde_yaml::to_string(&mode).unwrap();
        assert!(yaml.contains("restricted_autonomous"));

        let mode = ExecutionMode::FullAutonomous;
        let yaml = serde_yaml::to_string(&mode).unwrap();
        assert!(yaml.contains("full_autonomous"));
    }

    #[test]
    fn test_execution_mode_default() {
        let mode = ExecutionMode::default();
        assert_eq!(mode, ExecutionMode::RestrictedAutonomous);
    }

    #[test]
    fn test_load_nonexistent_file_uses_defaults() {
        let cli = crate::cli::Cli {
            config: None,
            verbose: false,
            storage_path: None,
            command: crate::cli::Commands::Auth {
                provider: Some("copilot".to_string()),
            },
        };

        let config = Config::load("nonexistent.yaml", &cli).unwrap();
        assert_eq!(config.provider.provider_type, "copilot");
    }

    #[test]
    fn test_conversation_config_defaults() {
        let config = ConversationConfig::default();
        assert_eq!(config.max_tokens, 100_000);
        assert_eq!(config.min_retain_turns, 5);
        assert_eq!(config.prune_threshold, 0.8);
    }

    #[test]
    fn test_tools_config_defaults() {
        let config = ToolsConfig::default();
        assert_eq!(config.max_output_size, 1_048_576);
        assert_eq!(config.max_file_read_size, 10_485_760);
    }

    #[test]
    fn test_terminal_config_defaults() {
        let config = TerminalConfig::default();
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.max_stdout_bytes, 1048576);
        assert_eq!(config.max_stderr_bytes, 262144);
    }

    #[test]
    fn test_chat_config_defaults() {
        let config = ChatConfig::default();
        assert_eq!(config.default_mode, "planning");
        assert_eq!(config.default_safety, "confirm");
        assert!(config.allow_mode_switching);
    }

    #[test]
    fn test_chat_config_from_yaml() {
        let yaml = r#"
default_mode: write
default_safety: yolo
allow_mode_switching: false
"#;
        let config: ChatConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.default_mode, "write");
        assert_eq!(config.default_safety, "yolo");
        assert!(!config.allow_mode_switching);
    }

    #[test]
    fn test_agent_config_includes_chat() {
        let config = AgentConfig::default();
        assert_eq!(config.chat.default_mode, "planning");
        assert_eq!(config.chat.default_safety, "confirm");
        assert!(config.chat.allow_mode_switching);
    }

    #[test]
    fn test_example_watcher_config_parses() {
        // Ensure the example configuration file is valid YAML and maps to `Config`.
        let contents = std::fs::read_to_string("config/watcher.yaml")
            .expect("Failed to read example config/watcher.yaml");
        // Use a YAML deserializer to parse only the first document.
        let mut de = serde_yaml::Deserializer::from_str(&contents);
        // Parse only the first YAML document in the file (the example contains
        // multiple documents: development and production).
        let first_doc = de
            .next()
            .expect("No YAML document found in config/watcher.yaml");
        let cfg: Config = Config::deserialize(first_doc).expect("Failed to parse watcher config");

        // Basic sanity checks for values present in the development example
        assert!(cfg.watcher.kafka.is_some());
        let kafka = cfg.watcher.kafka.unwrap();
        assert_eq!(kafka.brokers, "localhost:9092");
        assert_eq!(kafka.topic, "xzepr.events");
        assert_eq!(cfg.watcher.filters.event_types.len(), 3);
        assert_eq!(cfg.watcher.logging.level, "info");
        assert!(cfg.watcher.logging.json_format);
        assert_eq!(cfg.watcher.execution.max_concurrent_executions, 5);
    }

    #[test]
    fn test_production_watcher_config_parses() {
        // Ensure the production example configuration file parses correctly.
        let contents = std::fs::read_to_string("config/watcher-production.yaml")
            .expect("Failed to read example config/watcher-production.yaml");
        let cfg: Config =
            serde_yaml::from_str(&contents).expect("Failed to parse watcher-production.yaml");

        // Sanity checks for production example values
        assert!(cfg.watcher.kafka.is_some());
        let kafka = cfg.watcher.kafka.unwrap();
        assert_eq!(
            kafka.brokers,
            "kafka-1.prod:9093,kafka-2.prod:9093,kafka-3.prod:9093"
        );
        assert_eq!(kafka.topic, "xzepr.production.events");
        assert_eq!(kafka.group_id, "xzatoma-watcher-prod");

        // Security block
        assert!(kafka.security.is_some());
        let sec = kafka.security.unwrap();
        assert_eq!(sec.protocol, "SASL_SSL");
        assert_eq!(sec.sasl_mechanism.unwrap(), "SCRAM-SHA-256");
        assert_eq!(sec.sasl_username.unwrap(), "xzatoma-consumer");

        // Logging and execution
        assert_eq!(cfg.watcher.logging.level, "warn");
        assert!(cfg.watcher.logging.json_format);
        assert_eq!(cfg.watcher.execution.max_concurrent_executions, 10);
        assert_eq!(cfg.watcher.execution.execution_timeout_secs, 1800);
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_populates_kafka_from_xzepr_vars() {
        // NOTE: This test mutates global environment variables. Run with:
        // `cargo test -- --ignored --test-threads=1`
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_BROKERS");
            std::env::remove_var("XZEPR_KAFKA_TOPIC");
            std::env::remove_var("XZEPR_KAFKA_GROUP_ID");
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
            std::env::remove_var("XZEPR_KAFKA_SASL_USERNAME");
            std::env::remove_var("XZEPR_KAFKA_SASL_PASSWORD");
        }

        std::env::set_var("XZEPR_KAFKA_BROKERS", "test-broker:9092");
        std::env::set_var("XZEPR_KAFKA_TOPIC", "test-topic");
        std::env::set_var("XZEPR_KAFKA_GROUP_ID", "test-group");
        std::env::set_var("XZEPR_KAFKA_SECURITY_PROTOCOL", "SASL_SSL");
        std::env::set_var("XZEPR_KAFKA_SASL_USERNAME", "user");
        std::env::set_var("XZEPR_KAFKA_SASL_PASSWORD", "pass");

        let mut cfg = Config::default();
        // apply_env_vars is private but accessible within the test module
        cfg.apply_env_vars();

        assert!(cfg.watcher.kafka.is_some());
        let kafka = cfg.watcher.kafka.unwrap();
        assert_eq!(kafka.brokers, "test-broker:9092");
        assert_eq!(kafka.topic, "test-topic");
        assert_eq!(kafka.group_id, "test-group");
        assert!(kafka.security.is_some());
        let sec = kafka.security.unwrap();
        assert_eq!(sec.protocol, "SASL_SSL");
        assert_eq!(sec.sasl_username.unwrap(), "user");
        assert_eq!(sec.sasl_password.unwrap(), "pass");

        // Cleanup environment
        unsafe {
            std::env::remove_var("XZEPR_KAFKA_BROKERS");
            std::env::remove_var("XZEPR_KAFKA_TOPIC");
            std::env::remove_var("XZEPR_KAFKA_GROUP_ID");
            std::env::remove_var("XZEPR_KAFKA_SECURITY_PROTOCOL");
            std::env::remove_var("XZEPR_KAFKA_SASL_USERNAME");
            std::env::remove_var("XZEPR_KAFKA_SASL_PASSWORD");
        }
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_watcher_fields() {
        // NOTE: This test mutates global environment variables. Run with:
        // `cargo test -- --ignored --test-threads=1`
        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_EVENT_TYPES");
            std::env::remove_var("XZATOMA_WATCHER_LOG_LEVEL");
            std::env::remove_var("XZATOMA_WATCHER_JSON_LOGS");
            std::env::remove_var("XZATOMA_WATCHER_MAX_CONCURRENT");
        }

        std::env::set_var(
            "XZATOMA_WATCHER_EVENT_TYPES",
            "deployment.success,ci.pipeline.completed",
        );
        std::env::set_var("XZATOMA_WATCHER_LOG_LEVEL", "debug");
        std::env::set_var("XZATOMA_WATCHER_JSON_LOGS", "false");
        std::env::set_var("XZATOMA_WATCHER_MAX_CONCURRENT", "3");

        let mut cfg = Config::default();
        cfg.apply_env_vars();

        assert_eq!(cfg.watcher.filters.event_types.len(), 2);
        assert_eq!(cfg.watcher.logging.level, "debug");
        assert!(!cfg.watcher.logging.json_format);
        assert_eq!(cfg.watcher.execution.max_concurrent_executions, 3);

        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_EVENT_TYPES");
            std::env::remove_var("XZATOMA_WATCHER_LOG_LEVEL");
            std::env::remove_var("XZATOMA_WATCHER_JSON_LOGS");
            std::env::remove_var("XZATOMA_WATCHER_MAX_CONCURRENT");
        }
    }
}

/// Watcher configuration for Kafka event monitoring
///
/// Configures the watcher service for monitoring Kafka topics,
/// filtering events, logging, and executing plans extracted from events.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Kafka consumer configuration for the watcher
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

/// Kafka security configuration
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

/// Event filter configuration
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

/// Watcher logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherLoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Enable JSON-formatted logs
    #[serde(default = "default_json_logs")]
    pub json_format: bool,

    /// Log file path (if None, STDOUT only)
    pub file_path: Option<std::path::PathBuf>,

    /// Include full event payload in logs
    #[serde(default)]
    pub include_payload: bool,
}

/// Watcher execution configuration
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

/// Default watcher consumer group ID
fn default_watcher_group_id() -> String {
    "xzatoma-watcher".to_string()
}

/// Default success_only value
fn default_success_only() -> bool {
    true
}

/// Default log level
fn default_log_level() -> String {
    "info".to_string()
}

/// Default JSON logs setting
fn default_json_logs() -> bool {
    true
}

/// Default max concurrent executions
fn default_max_concurrent() -> usize {
    1
}

/// Default execution timeout in seconds
fn default_execution_timeout() -> u64 {
    300
}

impl Default for WatcherLoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json_format: default_json_logs(),
            file_path: None,
            include_payload: false,
        }
    }
}

impl Default for WatcherExecutionConfig {
    fn default() -> Self {
        Self {
            allow_dangerous: false,
            max_concurrent_executions: default_max_concurrent(),
            execution_timeout_secs: default_execution_timeout(),
        }
    }
}
