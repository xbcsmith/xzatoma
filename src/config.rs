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
}

fn default_copilot_model() -> String {
    "gpt-5-mini".to_string()
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: default_copilot_model(),
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
    "qwen2.5-coder".to_string()
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
}

fn default_max_output() -> usize {
    1_048_576 // 1 MB
}

fn default_max_file_read() -> usize {
    10_485_760 // 10 MB
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Require explicit user confirmation for each command
    Interactive,
    /// Allow safe commands automatically, require confirmation for dangerous ones
    RestrictedAutonomous,
    /// Allow all commands without confirmation (use with caution)
    FullAutonomous,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::RestrictedAutonomous
    }
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
        }
    }

    fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| XzatomaError::Config(format!("Failed to read config file: {}", e)))?;
        serde_yaml::from_str(&contents)
            .map_err(|e| XzatomaError::Config(format!("Failed to parse config: {}", e)).into())
    }

    fn apply_env_vars(&mut self) {
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

        if let Ok(max_turns) = std::env::var("XZATOMA_MAX_TURNS") {
            if let Ok(value) = max_turns.parse() {
                self.agent.max_turns = value;
            }
        }

        if let Ok(timeout) = std::env::var("XZATOMA_TIMEOUT_SECONDS") {
            if let Ok(value) = timeout.parse() {
                self.agent.timeout_seconds = value;
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
    model: qwen2.5-coder

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
        assert_eq!(config.default_mode, ExecutionMode::RestrictedAutonomous);
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.max_stdout_bytes, 1_048_576);
        assert_eq!(config.max_stderr_bytes, 262_144);
    }
}
