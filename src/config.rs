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

    /// Enable streaming mode for responses
    ///
    /// When enabled, the provider will use SSE (Server-Sent Events) streaming
    /// for response delivery. Defaults to true.
    #[serde(default = "default_enable_streaming")]
    pub enable_streaming: bool,

    /// Enable automatic endpoint fallback
    ///
    /// When enabled, if the preferred /responses endpoint is not supported,
    /// the provider will automatically fall back to /chat/completions endpoint.
    /// Defaults to true.
    #[serde(default = "default_enable_endpoint_fallback")]
    pub enable_endpoint_fallback: bool,

    /// Reasoning effort level: "low", "medium", "high"
    ///
    /// Controls how much reasoning the model should perform. Only applicable
    /// to models that support extended thinking.
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Include reasoning in the response
    ///
    /// When enabled, models that support extended thinking will include
    /// their reasoning process in the response. Defaults to false.
    #[serde(default = "default_include_reasoning")]
    pub include_reasoning: bool,
}

fn default_copilot_model() -> String {
    "gpt-5.3-codex".to_string()
}

fn default_enable_streaming() -> bool {
    true
}

fn default_enable_endpoint_fallback() -> bool {
    true
}

fn default_include_reasoning() -> bool {
    false
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: default_copilot_model(),
            api_base: None,
            enable_streaming: default_enable_streaming(),
            enable_endpoint_fallback: default_enable_endpoint_fallback(),
            reasoning_effort: None,
            include_reasoning: default_include_reasoning(),
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

    /// Subagent delegation settings
    #[serde(default)]
    pub subagent: SubagentConfig,
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
            subagent: SubagentConfig::default(),
        }
    }
}

/// Subagent delegation configuration
///
/// Settings for spawning and managing recursive agent instances
/// with task delegation and controlled resource usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum recursion depth for nested subagents
    ///
    /// Root agent is depth 0, first subagent spawned is depth 1.
    /// Prevents infinite recursion and stack overflow.
    /// - depth < max_depth: Allow spawning
    /// - depth >= max_depth: Return error
    #[serde(default = "default_subagent_max_depth")]
    pub max_depth: usize,

    /// Default maximum turns per subagent execution
    ///
    /// Used when subagent input does not specify max_turns.
    /// Limits conversation turns to prevent runaway execution.
    #[serde(default = "default_subagent_max_turns")]
    pub default_max_turns: usize,

    /// Maximum output size in bytes before truncation
    ///
    /// Prevents subagent output from expanding context window excessively.
    /// Output exceeding this size will be truncated with notice.
    #[serde(default = "default_subagent_output_max_size")]
    pub output_max_size: usize,

    /// Enable subagent execution telemetry
    ///
    /// When true, logs structured telemetry events (spawn, complete, error, etc).
    /// Set to false to disable telemetry logging.
    #[serde(default = "default_subagent_telemetry_enabled")]
    pub telemetry_enabled: bool,

    /// Enable conversation persistence for debugging
    ///
    /// When true, saves subagent conversations to persistent storage
    /// for replay and debugging. Phase 4 feature, currently ignored.
    #[serde(default = "default_subagent_persistence_enabled")]
    pub persistence_enabled: bool,

    /// Path to conversation database for persistence
    ///
    /// Used when persistence_enabled is true. Specifies the location
    /// of the sled database storing conversation history.
    #[serde(default = "default_persistence_path")]
    pub persistence_path: String,

    /// Maximum total subagent executions per session
    ///
    /// Limits the number of subagents that can be spawned in a single session.
    /// None means unlimited executions.
    #[serde(default = "default_max_executions")]
    pub max_executions: Option<usize>,

    /// Maximum total tokens consumable by all subagents
    ///
    /// Limits total token consumption across all subagent executions.
    /// None means unlimited tokens.
    #[serde(default = "default_max_total_tokens")]
    pub max_total_tokens: Option<usize>,

    /// Maximum wall-clock time for all subagents (seconds)
    ///
    /// Limits the total execution time for all subagents in a session.
    /// None means unlimited time.
    #[serde(default = "default_max_total_time")]
    pub max_total_time: Option<u64>,

    /// Optional provider override for subagents
    ///
    /// If None, subagents use the parent agent's provider.
    /// If Some("copilot" | "ollama"), creates a dedicated provider instance
    /// for subagents with separate model configuration.
    #[serde(default)]
    pub provider: Option<String>,

    /// Optional model override for subagents
    ///
    /// If None, uses the provider's default model.
    /// If Some("model-name"), overrides the model for the subagent provider.
    /// Only applicable when provider is also specified.
    #[serde(default)]
    pub model: Option<String>,

    /// Enable subagents in chat mode by default
    ///
    /// If false (default), subagents are disabled in chat mode unless explicitly
    /// enabled via prompt pattern detection or special commands.
    /// If true, subagents are available immediately in chat mode.
    #[serde(default = "default_chat_enabled")]
    pub chat_enabled: bool,
}

fn default_subagent_max_depth() -> usize {
    3
}

fn default_subagent_max_turns() -> usize {
    10
}

fn default_subagent_output_max_size() -> usize {
    1_048_576 // 1 MB
}

fn default_subagent_telemetry_enabled() -> bool {
    true
}

fn default_subagent_persistence_enabled() -> bool {
    false
}

fn default_persistence_path() -> String {
    let home = std::env::var("HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".xzatoma")
        .join("conversations.db")
        .to_string_lossy()
        .to_string()
}

fn default_max_executions() -> Option<usize> {
    None
}

fn default_max_total_tokens() -> Option<usize> {
    None
}

fn default_max_total_time() -> Option<u64> {
    None
}

fn default_chat_enabled() -> bool {
    false
}

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_depth: default_subagent_max_depth(),
            default_max_turns: default_subagent_max_turns(),
            output_max_size: default_subagent_output_max_size(),
            telemetry_enabled: default_subagent_telemetry_enabled(),
            persistence_enabled: default_subagent_persistence_enabled(),
            persistence_path: default_persistence_path(),
            max_executions: default_max_executions(),
            max_total_tokens: default_max_total_tokens(),
            max_total_time: default_max_total_time(),
            provider: None,
            model: None,
            chat_enabled: default_chat_enabled(),
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

    /// Persist special commands in conversation history
    #[serde(default = "default_persist_special_commands")]
    pub persist_special_commands: bool,
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

fn default_persist_special_commands() -> bool {
    true
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            default_mode: default_chat_mode(),
            default_safety: default_safety_mode(),
            allow_mode_switching: default_allow_mode_switching(),
            persist_special_commands: default_persist_special_commands(),
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

    /// Token percentage threshold for warning status (0.0-1.0)
    /// Default: 0.85 (warn at 85% context usage)
    #[serde(default = "default_warning_threshold")]
    pub warning_threshold: f32,

    /// Token percentage threshold for automatic summarization (0.0-1.0)
    /// Default: 0.90 (auto-summarize at 90% context usage)
    #[serde(default = "default_auto_summary_threshold")]
    pub auto_summary_threshold: f32,

    /// Model to use for automatic summarization (e.g., "gpt-4", "claude-3")
    /// If None, uses the default provider model
    #[serde(default)]
    pub summary_model: Option<String>,
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

fn default_warning_threshold() -> f32 {
    0.85
}

fn default_auto_summary_threshold() -> f32 {
    0.90
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            min_retain_turns: default_min_retain(),
            prune_threshold: default_prune_threshold(),
            warning_threshold: default_warning_threshold(),
            auto_summary_threshold: default_auto_summary_threshold(),
            summary_model: None,
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
    5_242_880 // 5 MB
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

        if self.agent.conversation.warning_threshold <= 0.0
            || self.agent.conversation.warning_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.warning_threshold must be between 0.0 and 1.0".to_string(),
            )
            .into());
        }

        if self.agent.conversation.auto_summary_threshold <= 0.0
            || self.agent.conversation.auto_summary_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.auto_summary_threshold must be between 0.0 and 1.0".to_string(),
            )
            .into());
        }

        if self.agent.conversation.auto_summary_threshold
            < self.agent.conversation.warning_threshold
        {
            return Err(XzatomaError::Config(
                "conversation.auto_summary_threshold must be >= warning_threshold".to_string(),
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

        // Validate subagent configuration
        if self.agent.subagent.max_depth == 0 {
            return Err(XzatomaError::Config(
                "agent.subagent.max_depth must be greater than 0".to_string(),
            )
            .into());
        }

        if self.agent.subagent.max_depth > 10 {
            return Err(XzatomaError::Config(
                "agent.subagent.max_depth cannot exceed 10 (stack overflow risk)".to_string(),
            )
            .into());
        }

        if self.agent.subagent.default_max_turns == 0 {
            return Err(XzatomaError::Config(
                "agent.subagent.default_max_turns must be greater than 0".to_string(),
            )
            .into());
        }

        if self.agent.subagent.default_max_turns > 100 {
            return Err(XzatomaError::Config(
                "agent.subagent.default_max_turns cannot exceed 100".to_string(),
            )
            .into());
        }

        if self.agent.subagent.output_max_size < 1024 {
            return Err(XzatomaError::Config(
                "agent.subagent.output_max_size must be at least 1024 bytes".to_string(),
            )
            .into());
        }

        // Validate subagent provider override if specified
        if let Some(ref provider) = self.agent.subagent.provider {
            let valid_providers = ["copilot", "ollama"];
            if !valid_providers.contains(&provider.as_str()) {
                return Err(XzatomaError::Config(format!(
                    "Invalid subagent provider override: {}. Must be one of: {}",
                    provider,
                    valid_providers.join(", ")
                ))
                .into());
            }
        }

        Ok(())
    }

    /// Check if special commands should be persisted in conversation history
    ///
    /// # Returns
    ///
    /// Returns true if special commands should be persisted (default is true)
    pub fn should_persist_commands(&self) -> bool {
        self.agent.chat.persist_special_commands
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
    fn test_config_should_persist_commands_default_on() {
        let config = Config::default();
        assert!(config.should_persist_commands());
    }

    #[test]
    fn test_config_should_persist_commands_can_disable() {
        let mut config = Config::default();
        config.agent.chat.persist_special_commands = false;
        assert!(!config.should_persist_commands());
    }

    #[test]
    fn test_chat_config_persist_special_commands_default() {
        let chat_config = ChatConfig::default();
        assert!(chat_config.persist_special_commands);
    }

    #[test]
    fn test_config_from_yaml() {
        let yaml = r#"
provider:
  type: ollama
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

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
        assert_eq!(config.warning_threshold, 0.85);
        assert_eq!(config.auto_summary_threshold, 0.90);
        assert_eq!(config.summary_model, None);
    }

    #[test]
    fn test_conversation_config_warning_threshold_validation() {
        let mut config = Config::default();
        config.agent.conversation.warning_threshold = 0.0;
        assert!(config.validate().is_err());

        config.agent.conversation.warning_threshold = 1.5;
        assert!(config.validate().is_err());

        config.agent.conversation.warning_threshold = 0.85;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_conversation_config_auto_summary_threshold_validation() {
        let mut config = Config::default();
        config.agent.conversation.auto_summary_threshold = 0.0;
        assert!(config.validate().is_err());

        config.agent.conversation.auto_summary_threshold = 1.5;
        assert!(config.validate().is_err());

        config.agent.conversation.auto_summary_threshold = 0.90;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_conversation_config_threshold_ordering_validation() {
        let mut config = Config::default();
        config.agent.conversation.warning_threshold = 0.90;
        config.agent.conversation.auto_summary_threshold = 0.85;
        assert!(config.validate().is_err());

        config.agent.conversation.warning_threshold = 0.85;
        config.agent.conversation.auto_summary_threshold = 0.90;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tools_config_defaults() {
        let config = ToolsConfig::default();
        assert_eq!(config.max_output_size, 5_242_880);
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
    fn test_subagent_config_defaults() {
        let config = SubagentConfig::default();
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.default_max_turns, 10);
        assert_eq!(config.output_max_size, 1_048_576);
        assert!(config.telemetry_enabled);
        assert!(!config.persistence_enabled);
    }

    #[test]
    fn test_subagent_config_validation_max_depth_zero() {
        let mut config = Config::default();
        config.agent.subagent.max_depth = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_subagent_config_validation_max_depth_too_large() {
        let mut config = Config::default();
        config.agent.subagent.max_depth = 11;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_subagent_config_validation_default_max_turns_zero() {
        let mut config = Config::default();
        config.agent.subagent.default_max_turns = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_subagent_config_validation_output_size_too_small() {
        let mut config = Config::default();
        config.agent.subagent.output_max_size = 512;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_subagent_config_from_yaml() {
        let yaml = r#"
max_depth: 5
default_max_turns: 20
output_max_size: 8192
telemetry_enabled: false
persistence_enabled: true
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.default_max_turns, 20);
        assert_eq!(config.output_max_size, 8192);
        assert!(!config.telemetry_enabled);
        assert!(config.persistence_enabled);
    }

    #[test]
    fn test_agent_config_includes_subagent() {
        let config = AgentConfig::default();
        assert_eq!(config.subagent.max_depth, 3);
        assert_eq!(config.subagent.default_max_turns, 10);
        assert!(config.subagent.telemetry_enabled);
    }

    #[test]
    fn test_subagent_config_deserialize_with_provider_override() {
        let yaml = r#"
max_depth: 5
provider: copilot
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.provider, Some("copilot".to_string()));
        assert_eq!(config.model, None);
        assert!(!config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_deserialize_with_model_override() {
        let yaml = r#"
max_depth: 5
model: gpt-5.3-codex
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.provider, None);
        assert_eq!(config.model, Some("gpt-5.3-codex".to_string()));
        assert!(!config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_deserialize_with_both_overrides() {
        let yaml = r#"
provider: ollama
model: llama3.2:3b
chat_enabled: true
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.provider, Some("ollama".to_string()));
        assert_eq!(config.model, Some("llama3.2:3b".to_string()));
        assert!(config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_deserialize_backward_compatibility() {
        let yaml = r#"
max_depth: 3
default_max_turns: 10
output_max_size: 4096
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.provider, None);
        assert_eq!(config.model, None);
        assert!(!config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_validation_invalid_provider() {
        let mut config = Config::default();
        config.agent.subagent.provider = Some("invalid_provider".to_string());
        let result = config.validate();
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid subagent provider override"));
    }

    #[test]
    fn test_subagent_config_validation_copilot_provider() {
        let mut config = Config::default();
        config.agent.subagent.provider = Some("copilot".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_subagent_config_validation_ollama_provider() {
        let mut config = Config::default();
        config.agent.subagent.provider = Some("ollama".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_subagent_config_default_chat_enabled() {
        let config = SubagentConfig::default();
        assert!(!config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_chat_enabled_in_yaml() {
        let yaml = r#"
chat_enabled: true
"#;
        let config: SubagentConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.chat_enabled);
    }

    #[test]
    fn test_subagent_config_provider_none_is_valid() {
        let config = Config::default();
        assert_eq!(config.agent.subagent.provider, None);
        assert!(config.validate().is_ok());
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

    // Phase 5: Enhanced subagent configuration tests

    #[test]
    fn test_subagent_config_provider_override_copilot() {
        let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    provider: copilot
    model: gpt-5.1-codex-mini
    chat_enabled: false
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.provider, Some("copilot".to_string()));
        assert_eq!(
            cfg.agent.subagent.model,
            Some("gpt-5.1-codex-mini".to_string())
        );
        assert!(!cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_provider_override_ollama() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 10
  subagent:
    provider: ollama
    model: granite3.2:2b
    chat_enabled: true
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.provider, Some("ollama".to_string()));
        assert_eq!(cfg.agent.subagent.model, Some("granite3.2:2b".to_string()));
        assert!(cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_model_override_no_provider() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    model: gpt-5.1-codex-mini
    chat_enabled: false
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.provider, None);
        assert_eq!(
            cfg.agent.subagent.model,
            Some("gpt-5.1-codex-mini".to_string())
        );
    }

    #[test]
    fn test_subagent_config_chat_enabled_true() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    chat_enabled: true
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert!(cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_chat_enabled_false() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    chat_enabled: false
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert!(!cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_chat_enabled_defaults_to_false() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert!(!cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_all_fields_valid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 20
  subagent:
    max_depth: 5
    default_max_turns: 15
    output_max_size: 8192
    telemetry_enabled: true
    persistence_enabled: false
    persistence_path: /tmp/conversations.db
    max_executions: 100
    max_total_tokens: 50000
    max_total_time: 3600
    provider: ollama
    model: llama3.2:3b
    chat_enabled: true
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_depth, 5);
        assert_eq!(cfg.agent.subagent.default_max_turns, 15);
        assert_eq!(cfg.agent.subagent.output_max_size, 8192);
        assert!(cfg.agent.subagent.telemetry_enabled);
        assert!(!cfg.agent.subagent.persistence_enabled);
        assert_eq!(cfg.agent.subagent.max_executions, Some(100));
        assert_eq!(cfg.agent.subagent.max_total_tokens, Some(50000));
        assert_eq!(cfg.agent.subagent.max_total_time, Some(3600));
        assert_eq!(cfg.agent.subagent.provider, Some("ollama".to_string()));
        assert_eq!(cfg.agent.subagent.model, Some("llama3.2:3b".to_string()));
        assert!(cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_invalid_provider_type() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    provider: invalid_provider
    chat_enabled: false
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid subagent provider"));
    }

    #[test]
    fn test_subagent_config_max_depth_boundary_valid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    max_depth: 10
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_depth, 10);
    }

    #[test]
    fn test_subagent_config_max_depth_zero_invalid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    max_depth: 0
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_depth must be greater than 0"));
    }

    #[test]
    fn test_subagent_config_max_depth_exceeds_limit_invalid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    max_depth: 11
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed 10"));
    }

    #[test]
    fn test_subagent_config_default_max_turns_boundary_valid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    default_max_turns: 100
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.default_max_turns, 100);
    }

    #[test]
    fn test_subagent_config_default_max_turns_zero_invalid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    default_max_turns: 0
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("default_max_turns must be greater than 0"));
    }

    #[test]
    fn test_subagent_config_default_max_turns_exceeds_limit_invalid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    default_max_turns: 101
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot exceed 100"));
    }

    #[test]
    fn test_subagent_config_output_max_size_boundary_valid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    output_max_size: 1024
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.output_max_size, 1024);
    }

    #[test]
    fn test_subagent_config_output_max_size_too_small_invalid() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    output_max_size: 512
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        let result = cfg.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least 1024 bytes"));
    }

    #[test]
    fn test_subagent_config_empty_section_uses_defaults() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent: {}
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_depth, 3);
        assert_eq!(cfg.agent.subagent.default_max_turns, 10);
        assert_eq!(cfg.agent.subagent.output_max_size, 1_048_576);
        assert!(cfg.agent.subagent.telemetry_enabled);
        assert!(!cfg.agent.subagent.persistence_enabled);
        assert_eq!(cfg.agent.subagent.provider, None);
        assert_eq!(cfg.agent.subagent.model, None);
        assert!(!cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_no_section_uses_defaults() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_depth, 3);
        assert_eq!(cfg.agent.subagent.default_max_turns, 10);
        assert!(!cfg.agent.subagent.chat_enabled);
    }

    #[test]
    fn test_subagent_config_optional_quota_fields() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    max_executions: 50
    max_total_tokens: 500000
    max_total_time: 7200
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_executions, Some(50));
        assert_eq!(cfg.agent.subagent.max_total_tokens, Some(500000));
        assert_eq!(cfg.agent.subagent.max_total_time, Some(7200));
    }

    #[test]
    fn test_subagent_config_optional_quota_none_defaults() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_executions, None);
        assert_eq!(cfg.agent.subagent.max_total_tokens, None);
        assert_eq!(cfg.agent.subagent.max_total_time, None);
    }

    #[test]
    fn test_subagent_config_persistence_fields() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    persistence_enabled: true
    persistence_path: /tmp/xzatoma_conversations.db
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert!(cfg.agent.subagent.persistence_enabled);
        assert_eq!(
            cfg.agent.subagent.persistence_path,
            "/tmp/xzatoma_conversations.db"
        );
    }

    #[test]
    fn test_subagent_config_telemetry_fields() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  max_turns: 10
  subagent:
    telemetry_enabled: false
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert!(!cfg.agent.subagent.telemetry_enabled);
    }

    #[test]
    fn test_cost_optimized_example_from_plan() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 50
  subagent:
    provider: ollama
    model: llama3.2:3b
    chat_enabled: true
    max_executions: 10
    max_total_tokens: 50000
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.provider.provider_type, "copilot");
        assert_eq!(cfg.agent.subagent.provider, Some("ollama".to_string()));
        assert_eq!(cfg.agent.subagent.model, Some("llama3.2:3b".to_string()));
    }

    #[test]
    fn test_provider_mixing_example_from_plan() {
        let config = r#"
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 15
  subagent:
    provider: ollama
    model: llama3.2:3b
    chat_enabled: false
    max_depth: 2
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.max_depth, 2);
    }

    #[test]
    fn test_speed_optimized_example_from_plan() {
        let config = r#"
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 10
  subagent:
    model: granite3.2:2b
    chat_enabled: true
    default_max_turns: 5
"#;

        let cfg: Config = serde_yaml::from_str(config).unwrap();
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.agent.subagent.model, Some("granite3.2:2b".to_string()));
        assert_eq!(cfg.agent.subagent.default_max_turns, 5);
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
