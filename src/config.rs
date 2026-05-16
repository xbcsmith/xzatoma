//! Configuration management for XZatoma
//!
//! This module handles loading, parsing, validating, and managing
//! configuration from files, environment variables, and CLI overrides.

use crate::error::{Result, XzatomaError};
use crate::mcp::config::McpConfig;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// MCP client configuration
    #[serde(default)]
    pub mcp: McpConfig,
    /// ACP server configuration
    #[serde(default)]
    pub acp: AcpConfig,
    /// Skills discovery and parsing configuration
    #[serde(default)]
    pub skills: SkillsConfig,
}

/// Provider configuration
///
/// Specifies which AI provider to use and its settings.
///
/// # Valid Provider Types
///
/// * `"copilot"` - GitHub Copilot (OAuth device flow authentication)
/// * `"ollama"` - Ollama local or remote inference server (no auth required)
/// * `"openai"` - OpenAI API or any OpenAI-compatible inference server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Type of provider to use. Valid values: `"copilot"`, `"ollama"`, `"openai"`.
    #[serde(rename = "type")]
    pub provider_type: String,

    /// GitHub Copilot configuration
    #[serde(default)]
    pub copilot: CopilotConfig,

    /// Ollama configuration
    #[serde(default)]
    pub ollama: OllamaConfig,

    /// OpenAI (and OpenAI-compatible) provider configuration
    #[serde(default)]
    pub openai: OpenAIConfig,
}

/// GitHub Copilot provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotConfig {
    /// Model to use for Copilot
    #[serde(default = "default_copilot_model")]
    pub model: String,

    /// Internal API base URL override for local tests and mock servers.
    ///
    /// This value is intentionally skipped during serialization and
    /// deserialization so config files cannot redirect GitHub or Copilot
    /// credentials to an untrusted host. Tests may still set it directly in
    /// Rust code, and the provider validates that it targets loopback only.
    #[serde(skip)]
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
    "gpt-5-mini".to_string()
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

/// Ollama provider configuration.
///
/// Configures the connection to an Ollama local inference server.
/// The `request_timeout_seconds` field controls how long the HTTP client
/// waits for a single completion response before abandoning the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Ollama server host
    #[serde(default = "default_ollama_host")]
    pub host: String,

    /// Model to use for Ollama
    #[serde(default = "default_ollama_model")]
    pub model: String,

    /// Per-request HTTP timeout in seconds.
    ///
    /// Controls how long the HTTP client waits for a single completion
    /// response before abandoning the request. Local inference servers can
    /// take several minutes to generate a long response; set this to a value
    /// larger than your expected worst-case generation time.
    ///
    /// Defaults to 600 seconds (10 minutes). The agent-level
    /// `agent.timeout_seconds` provides a separate overall budget.
    ///
    /// Set via the `XZATOMA_OLLAMA_REQUEST_TIMEOUT` environment variable.
    #[serde(default = "default_ollama_request_timeout")]
    pub request_timeout_seconds: u64,
}

fn default_ollama_host() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2:latest".to_string()
}

fn default_ollama_request_timeout() -> u64 {
    600
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: default_ollama_host(),
            model: default_ollama_model(),
            request_timeout_seconds: default_ollama_request_timeout(),
        }
    }
}

/// OpenAI provider configuration.
///
/// Configures the OpenAI-compatible provider for XZatoma. The `base_url` field
/// allows targeting any server that implements the OpenAI Chat Completions API,
/// including llama.cpp, vLLM, Candle-vLLM, and Mistral.rs.
///
/// # Examples
///
/// ```
/// use xzatoma::config::OpenAIConfig;
///
/// let config = OpenAIConfig {
///     api_key: "sk-example".to_string(),
///     base_url: "https://api.openai.com/v1".to_string(),
///     model: "gpt-4o-mini".to_string(),
///     organization_id: None,
///     enable_streaming: true,
///     request_timeout_seconds: 600,
///     reasoning_effort: None,
/// };
/// assert_eq!(config.model, "gpt-4o-mini");
/// assert_eq!(config.base_url, "https://api.openai.com/v1");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// Bearer token used in the `Authorization: Bearer <api_key>` request header.
    ///
    /// May be left as an empty string for local inference servers that require
    /// no authentication (e.g., llama.cpp, vLLM, Mistral.rs).
    /// Set via the `XZATOMA_OPENAI_API_KEY` environment variable.
    #[serde(default = "default_openai_api_key")]
    pub api_key: String,

    /// Base URL for all API requests.
    ///
    /// Defaults to `"https://api.openai.com/v1"` for the hosted OpenAI API.
    /// Override to target any OpenAI-compatible inference server, for example:
    ///
    /// * llama.cpp: `http://localhost:8080/v1`
    /// * vLLM: `http://localhost:8000/v1`
    /// * Mistral.rs: `http://localhost:1234/v1`
    ///
    /// Set via the `XZATOMA_OPENAI_BASE_URL` environment variable.
    #[serde(default = "default_openai_base_url", alias = "host")]
    pub base_url: String,

    /// Model identifier sent in the `model` field of every request body.
    ///
    /// Defaults to `"gpt-4o-mini"`. For local servers, use the name of the
    /// model that was loaded on the server (e.g., `"llama-3.2-3b"`).
    /// Set via the `XZATOMA_OPENAI_MODEL` environment variable.
    #[serde(default = "default_openai_model")]
    pub model: String,

    /// Optional organization identifier sent as the `OpenAI-Organization`
    /// HTTP header. Has no effect when `None` or empty.
    ///
    /// Set via the `XZATOMA_OPENAI_ORG_ID` environment variable.
    #[serde(default)]
    pub organization_id: Option<String>,

    /// When `true`, use SSE streaming for responses that do not include tool
    /// calls. Requests that include tool schemas always use the non-streaming
    /// path to avoid partial tool-call accumulation issues.
    ///
    /// Set via the `XZATOMA_OPENAI_STREAMING` environment variable.
    #[serde(default = "default_openai_streaming")]
    pub enable_streaming: bool,

    /// Per-request HTTP timeout in seconds.
    ///
    /// Controls how long the HTTP client waits for a single completion
    /// response before abandoning the request. Local inference servers can
    /// take several minutes to generate a long response; set this to a value
    /// larger than your expected worst-case generation time.
    ///
    /// Defaults to 600 seconds (10 minutes). The agent-level
    /// `agent.timeout_seconds` provides a separate overall budget.
    ///
    /// Set via the `XZATOMA_OPENAI_REQUEST_TIMEOUT` environment variable.
    #[serde(default = "default_openai_request_timeout")]
    pub request_timeout_seconds: u64,

    /// Reasoning effort level for OpenAI o-series reasoning models.
    ///
    /// Accepted values: `"low"`, `"medium"`, `"high"`. Set to `None` to use
    /// the model default. Has no effect on non-reasoning models.
    ///
    /// Set at runtime via `Provider::set_thinking_effort` or the
    /// `XZATOMA_OPENAI_REASONING_EFFORT` environment variable.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_openai_api_key() -> String {
    String::new()
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_openai_streaming() -> bool {
    true
}

fn default_openai_request_timeout() -> u64 {
    600
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: default_openai_api_key(),
            base_url: default_openai_base_url(),
            model: default_openai_model(),
            organization_id: None,
            enable_streaming: default_openai_streaming(),
            request_timeout_seconds: default_openai_request_timeout(),
            reasoning_effort: None,
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

/// ACP server configuration.
///
/// Controls whether the ACP HTTP server is enabled, how it binds to the
/// network, which route layout it exposes, and which default execution
/// behavior it should advertise for future ACP run creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpConfig {
    /// Enable the ACP HTTP server.
    #[serde(default = "default_acp_enabled")]
    pub enabled: bool,

    /// Bind host for the ACP HTTP server.
    #[serde(default = "default_acp_host")]
    pub host: String,

    /// Bind port for the ACP HTTP server.
    #[serde(default = "default_acp_port")]
    pub port: u16,

    /// Route compatibility mode for ACP discovery endpoints.
    #[serde(default)]
    pub compatibility_mode: AcpCompatibilityMode,

    /// Versioned base path used when `compatibility_mode` is `Versioned`.
    #[serde(default = "default_acp_base_path")]
    pub base_path: String,

    /// Optional bearer token required for ACP HTTP requests.
    ///
    /// The token is never serialized when printing effective configuration.
    #[serde(default, skip_serializing)]
    pub auth_token: Option<String>,

    /// Maximum accepted ACP HTTP request body size in bytes.
    #[serde(default = "default_acp_max_request_bytes")]
    pub max_request_bytes: usize,

    /// Global ACP HTTP request rate limit per minute.
    #[serde(default = "default_acp_rate_limit_per_minute")]
    pub rate_limit_per_minute: usize,

    /// Default ACP run mode to advertise for future run lifecycle support.
    #[serde(default)]
    pub default_run_mode: AcpDefaultRunMode,

    /// Optional persistence tuning for future ACP session and event storage.
    #[serde(default)]
    pub persistence: AcpPersistenceConfig,

    /// ACP stdio subprocess configuration for Zed-compatible integrations.
    #[serde(default)]
    pub stdio: AcpStdioConfig,
}

fn default_acp_enabled() -> bool {
    false
}

fn default_acp_host() -> String {
    "127.0.0.1".to_string()
}

fn default_acp_port() -> u16 {
    8765
}

fn default_acp_base_path() -> String {
    "/api/v1/acp".to_string()
}

fn default_acp_max_request_bytes() -> usize {
    1024 * 1024
}

fn default_acp_rate_limit_per_minute() -> usize {
    120
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            enabled: default_acp_enabled(),
            host: default_acp_host(),
            port: default_acp_port(),
            compatibility_mode: AcpCompatibilityMode::default(),
            base_path: default_acp_base_path(),
            auth_token: None,
            max_request_bytes: default_acp_max_request_bytes(),
            rate_limit_per_minute: default_acp_rate_limit_per_minute(),
            default_run_mode: AcpDefaultRunMode::default(),
            persistence: AcpPersistenceConfig::default(),
            stdio: AcpStdioConfig::default(),
        }
    }
}

/// ACP route compatibility mode.
///
/// `Versioned` serves ACP endpoints beneath a configurable versioned base path.
/// `RootCompatible` reserves ACP-spec-style root paths such as `/ping` and
/// `/agents`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AcpCompatibilityMode {
    /// Serve ACP routes beneath a versioned base path such as `/api/v1/acp`.
    #[default]
    Versioned,
    /// Serve ACP routes at ACP root-compatible paths such as `/ping`.
    RootCompatible,
}

/// ACP default run mode configuration.
///
/// This value is configuration-only and gives later ACP run
/// lifecycle phases a stable place to read default behavior from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AcpDefaultRunMode {
    /// Prefer synchronous completion when possible.
    Sync,
    /// Prefer asynchronous acceptance with later polling or streaming.
    #[default]
    Async,
    /// Prefer streaming output when supported by the transport.
    Streaming,
}

/// ACP persistence tuning configuration.
///
/// This section stores only configuration for future ACP persistence support.
/// Validation still ensures any provided limits are sensible.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpPersistenceConfig {
    /// Enable persistence-oriented ACP state retention.
    #[serde(default = "default_acp_persistence_enabled")]
    pub enabled: bool,

    /// Maximum number of retained events per run when persistence is enabled.
    #[serde(default = "default_acp_max_events_per_run")]
    pub max_events_per_run: usize,

    /// Maximum number of retained completed runs when persistence is enabled.
    #[serde(default = "default_acp_max_completed_runs")]
    pub max_completed_runs: usize,
}

fn default_acp_persistence_enabled() -> bool {
    false
}

fn default_acp_max_events_per_run() -> usize {
    1000
}

fn default_acp_max_completed_runs() -> usize {
    1000
}

impl Default for AcpPersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: default_acp_persistence_enabled(),
            max_events_per_run: default_acp_max_events_per_run(),
            max_completed_runs: default_acp_max_completed_runs(),
        }
    }
}

/// ACP stdio subprocess configuration.
///
/// Controls prompt input policy for ACP clients such as Zed that launch
/// XZatoma as a stdio JSON-RPC subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpStdioConfig {
    /// Persist ACP stdio session mappings for workspace resume.
    #[serde(default = "default_acp_stdio_persist_sessions")]
    pub persist_sessions: bool,

    /// Resume the most recent ACP stdio conversation for the same workspace.
    #[serde(default = "default_acp_stdio_resume_by_workspace")]
    pub resume_by_workspace: bool,

    /// Maximum number of active stdio sessions in one subprocess.
    #[serde(default = "default_acp_stdio_max_active_sessions")]
    pub max_active_sessions: usize,

    /// Inactive session timeout in seconds.
    #[serde(default = "default_acp_stdio_session_timeout_seconds")]
    pub session_timeout_seconds: u64,

    /// Per-session prompt queue capacity.
    #[serde(default = "default_acp_stdio_prompt_queue_capacity")]
    pub prompt_queue_capacity: usize,

    /// Timeout in seconds for model list advertisement during session creation.
    #[serde(default = "default_acp_stdio_model_list_timeout_seconds")]
    pub model_list_timeout_seconds: u64,

    /// Enable image input handling for ACP stdio prompt requests.
    #[serde(default = "default_acp_stdio_vision_enabled")]
    pub vision_enabled: bool,

    /// Maximum decoded bytes allowed for a single image input.
    #[serde(default = "default_acp_stdio_max_image_bytes")]
    pub max_image_bytes: usize,

    /// Image MIME types accepted from ACP prompt content blocks.
    #[serde(default = "default_acp_stdio_allowed_image_mime_types")]
    pub allowed_image_mime_types: Vec<String>,

    /// Allow image prompt content to reference local files.
    #[serde(default = "default_acp_stdio_allow_image_file_references")]
    pub allow_image_file_references: bool,

    /// Allow image prompt content to reference remote URLs.
    #[serde(default = "default_acp_stdio_allow_remote_image_urls")]
    pub allow_remote_image_urls: bool,
}

fn default_acp_stdio_persist_sessions() -> bool {
    true
}

fn default_acp_stdio_resume_by_workspace() -> bool {
    true
}

fn default_acp_stdio_max_active_sessions() -> usize {
    32
}

fn default_acp_stdio_session_timeout_seconds() -> u64 {
    3600
}

fn default_acp_stdio_prompt_queue_capacity() -> usize {
    16
}

fn default_acp_stdio_model_list_timeout_seconds() -> u64 {
    5
}

fn default_acp_stdio_vision_enabled() -> bool {
    true
}

fn default_acp_stdio_max_image_bytes() -> usize {
    10 * 1024 * 1024
}

fn default_acp_stdio_allowed_image_mime_types() -> Vec<String> {
    vec![
        "image/png".to_string(),
        "image/jpeg".to_string(),
        "image/webp".to_string(),
        "image/gif".to_string(),
    ]
}

fn default_acp_stdio_allow_image_file_references() -> bool {
    true
}

fn default_acp_stdio_allow_remote_image_urls() -> bool {
    false
}

impl Default for AcpStdioConfig {
    fn default() -> Self {
        Self {
            persist_sessions: default_acp_stdio_persist_sessions(),
            resume_by_workspace: default_acp_stdio_resume_by_workspace(),
            max_active_sessions: default_acp_stdio_max_active_sessions(),
            session_timeout_seconds: default_acp_stdio_session_timeout_seconds(),
            prompt_queue_capacity: default_acp_stdio_prompt_queue_capacity(),
            model_list_timeout_seconds: default_acp_stdio_model_list_timeout_seconds(),
            vision_enabled: default_acp_stdio_vision_enabled(),
            max_image_bytes: default_acp_stdio_max_image_bytes(),
            allowed_image_mime_types: default_acp_stdio_allowed_image_mime_types(),
            allow_image_file_references: default_acp_stdio_allow_image_file_references(),
            allow_remote_image_urls: default_acp_stdio_allow_remote_image_urls(),
        }
    }
}

#[cfg(test)]
mod acp_stdio_config_tests {
    use super::*;

    #[test]
    fn test_acp_stdio_config_default_enables_vision_with_safe_limits() {
        let config = AcpStdioConfig::default();

        assert!(config.persist_sessions);
        assert!(config.resume_by_workspace);
        assert_eq!(config.max_active_sessions, 32);
        assert_eq!(config.session_timeout_seconds, 3600);
        assert_eq!(config.prompt_queue_capacity, 16);
        assert_eq!(config.model_list_timeout_seconds, 5);
        assert!(config.vision_enabled);
        assert_eq!(config.max_image_bytes, 10 * 1024 * 1024);
        assert!(config.allow_image_file_references);
        assert!(!config.allow_remote_image_urls);
        assert_eq!(
            config.allowed_image_mime_types,
            vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ]
        );
    }

    #[test]
    fn test_config_validate_accepts_default_acp_stdio_vision_policy() {
        let config = Config::default_config();

        let result = config.validate();

        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validate_rejects_zero_acp_stdio_max_image_bytes() {
        let mut config = Config::default_config();
        config.acp.stdio.max_image_bytes = 0;

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("acp.stdio.max_image_bytes"))
        );
    }

    #[test]
    fn test_config_validate_rejects_empty_acp_stdio_allowed_image_mime_types() {
        let mut config = Config::default_config();
        config.acp.stdio.allowed_image_mime_types.clear();

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("allowed_image_mime_types cannot be empty"))
        );
    }

    #[test]
    fn test_config_validate_rejects_non_image_acp_stdio_mime_type() {
        let mut config = Config::default_config();
        config.acp.stdio.allowed_image_mime_types = vec!["application/octet-stream".to_string()];

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("must start with 'image/'"))
        );
    }

    #[test]
    fn test_config_validate_rejects_blank_acp_stdio_mime_type() {
        let mut config = Config::default_config();
        config.acp.stdio.allowed_image_mime_types = vec![" ".to_string()];

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("cannot contain empty values"))
        );
    }

    #[test]
    fn test_config_validate_rejects_zero_acp_stdio_max_active_sessions() {
        let mut config = Config::default_config();
        config.acp.stdio.max_active_sessions = 0;

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("acp.stdio.max_active_sessions"))
        );
    }

    #[test]
    fn test_config_validate_rejects_zero_acp_stdio_session_timeout_seconds() {
        let mut config = Config::default_config();
        config.acp.stdio.session_timeout_seconds = 0;

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("acp.stdio.session_timeout_seconds"))
        );
    }

    #[test]
    fn test_config_validate_rejects_zero_acp_stdio_prompt_queue_capacity() {
        let mut config = Config::default_config();
        config.acp.stdio.prompt_queue_capacity = 0;

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("acp.stdio.prompt_queue_capacity"))
        );
    }

    #[test]
    fn test_config_validate_rejects_zero_acp_stdio_model_list_timeout_seconds() {
        let mut config = Config::default_config();
        config.acp.stdio.model_list_timeout_seconds = 0;

        let result = config.validate();

        assert!(
            matches!(result, Err(XzatomaError::Config(message)) if message.contains("acp.stdio.model_list_timeout_seconds"))
        );
    }
}

/// Skills discovery and parsing configuration
///
/// Controls whether skill discovery is enabled, which roots are scanned,
/// and what hard limits are applied during discovery and validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Global skills feature flag
    #[serde(default = "default_skills_enabled")]
    pub enabled: bool,

    /// Enable project-level discovery
    #[serde(default = "default_skills_project_enabled")]
    pub project_enabled: bool,

    /// Enable user-level discovery
    #[serde(default = "default_skills_user_enabled")]
    pub user_enabled: bool,

    /// Additional absolute or config-resolved paths
    #[serde(default)]
    pub additional_paths: Vec<String>,

    /// Hard cap on valid loaded skills
    #[serde(default = "default_max_discovered_skills")]
    pub max_discovered_skills: usize,

    /// Hard cap on directories visited
    #[serde(default = "default_max_scan_directories")]
    pub max_scan_directories: usize,

    /// Maximum traversal depth per discovery root
    #[serde(default = "default_max_scan_depth")]
    pub max_scan_depth: usize,

    /// Maximum number of catalog entries disclosed to the model
    #[serde(default = "default_catalog_max_entries")]
    pub catalog_max_entries: usize,

    /// Register `activate_skill` when skills are available
    #[serde(default = "default_activation_tool_enabled")]
    pub activation_tool_enabled: bool,

    /// Require explicit trust for project-level skills
    #[serde(default = "default_project_trust_required")]
    pub project_trust_required: bool,

    /// Optional override for trust-state file path
    #[serde(default)]
    pub trust_store_path: Option<String>,

    /// Whether custom paths bypass trust checks
    #[serde(default = "default_allow_custom_paths_without_trust")]
    pub allow_custom_paths_without_trust: bool,

    /// Reject invalid skills without lenient fallback
    #[serde(default = "default_strict_frontmatter")]
    pub strict_frontmatter: bool,
}

fn default_skills_enabled() -> bool {
    true
}

fn default_skills_project_enabled() -> bool {
    true
}

fn default_skills_user_enabled() -> bool {
    true
}

fn default_max_discovered_skills() -> usize {
    256
}

fn default_max_scan_directories() -> usize {
    2000
}

fn default_max_scan_depth() -> usize {
    6
}

fn default_catalog_max_entries() -> usize {
    128
}

fn default_activation_tool_enabled() -> bool {
    true
}

fn default_project_trust_required() -> bool {
    true
}

fn default_allow_custom_paths_without_trust() -> bool {
    false
}

fn default_strict_frontmatter() -> bool {
    true
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: default_skills_enabled(),
            project_enabled: default_skills_project_enabled(),
            user_enabled: default_skills_user_enabled(),
            additional_paths: Vec::new(),
            max_discovered_skills: default_max_discovered_skills(),
            max_scan_directories: default_max_scan_directories(),
            max_scan_depth: default_max_scan_depth(),
            catalog_max_entries: default_catalog_max_entries(),
            activation_tool_enabled: default_activation_tool_enabled(),
            project_trust_required: default_project_trust_required(),
            trust_store_path: None,
            allow_custom_paths_without_trust: default_allow_custom_paths_without_trust(),
            strict_frontmatter: default_strict_frontmatter(),
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
    /// for replay and debugging. Currently not active; subagent conversations
    /// are held in memory only.
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
                openai: OpenAIConfig::default(),
            },
            agent: AgentConfig::default(),
            watcher: WatcherConfig::default(),
            mcp: McpConfig::default(),
            acp: AcpConfig::default(),
            skills: SkillsConfig::default(),
        }
    }

    fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| XzatomaError::Config(format!("Failed to read config file: {}", e)))?;
        serde_yaml::from_str(&contents)
            .map_err(|e| XzatomaError::Config(format!("Failed to parse config: {}", e)))
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

        if let Ok(timeout) = std::env::var("XZATOMA_OLLAMA_REQUEST_TIMEOUT") {
            if let Ok(value) = timeout.parse::<u64>() {
                self.provider.ollama.request_timeout_seconds = value;
            } else {
                tracing::warn!("Invalid XZATOMA_OLLAMA_REQUEST_TIMEOUT: {}", timeout);
            }
        }

        if let Ok(openai_api_key) = std::env::var("XZATOMA_OPENAI_API_KEY") {
            self.provider.openai.api_key = openai_api_key;
        }

        if let Ok(openai_base_url) = std::env::var("XZATOMA_OPENAI_BASE_URL") {
            self.provider.openai.base_url = openai_base_url;
        }

        if let Ok(openai_model) = std::env::var("XZATOMA_OPENAI_MODEL") {
            self.provider.openai.model = openai_model;
        }

        if let Ok(openai_org_id) = std::env::var("XZATOMA_OPENAI_ORG_ID") {
            self.provider.openai.organization_id = Some(openai_org_id);
        }

        if let Ok(openai_streaming) = std::env::var("XZATOMA_OPENAI_STREAMING") {
            match parse_env_bool(&openai_streaming) {
                Some(value) => self.provider.openai.enable_streaming = value,
                None => tracing::warn!("Invalid XZATOMA_OPENAI_STREAMING: {}", openai_streaming),
            }
        }

        if let Ok(timeout) = std::env::var("XZATOMA_OPENAI_REQUEST_TIMEOUT") {
            if let Ok(value) = timeout.parse::<u64>() {
                self.provider.openai.request_timeout_seconds = value;
            } else {
                tracing::warn!("Invalid XZATOMA_OPENAI_REQUEST_TIMEOUT: {}", timeout);
            }
        }

        if let Ok(val) = std::env::var("XZATOMA_OPENAI_REASONING_EFFORT") {
            if val == "none" {
                self.provider.openai.reasoning_effort = None;
            } else {
                self.provider.openai.reasoning_effort = Some(val);
            }
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

        if let Ok(enabled) = std::env::var("XZATOMA_SKILLS_ENABLED") {
            match parse_env_bool(&enabled) {
                Some(value) => self.skills.enabled = value,
                None => tracing::warn!("Invalid XZATOMA_SKILLS_ENABLED: {}", enabled),
            }
        }

        if let Ok(project_enabled) = std::env::var("XZATOMA_SKILLS_PROJECT_ENABLED") {
            match parse_env_bool(&project_enabled) {
                Some(value) => self.skills.project_enabled = value,
                None => tracing::warn!(
                    "Invalid XZATOMA_SKILLS_PROJECT_ENABLED: {}",
                    project_enabled
                ),
            }
        }

        if let Ok(user_enabled) = std::env::var("XZATOMA_SKILLS_USER_ENABLED") {
            match parse_env_bool(&user_enabled) {
                Some(value) => self.skills.user_enabled = value,
                None => tracing::warn!("Invalid XZATOMA_SKILLS_USER_ENABLED: {}", user_enabled),
            }
        }

        if let Ok(activation_tool_enabled) = std::env::var("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED")
        {
            match parse_env_bool(&activation_tool_enabled) {
                Some(value) => self.skills.activation_tool_enabled = value,
                None => tracing::warn!(
                    "Invalid XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED: {}",
                    activation_tool_enabled
                ),
            }
        }

        if let Ok(project_trust_required) = std::env::var("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED") {
            match parse_env_bool(&project_trust_required) {
                Some(value) => self.skills.project_trust_required = value,
                None => tracing::warn!(
                    "Invalid XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED: {}",
                    project_trust_required
                ),
            }
        }

        if let Ok(allow_custom_paths_without_trust) =
            std::env::var("XZATOMA_SKILLS_ALLOW_CUSTOM_PATHS_WITHOUT_TRUST")
        {
            match parse_env_bool(&allow_custom_paths_without_trust) {
                Some(value) => self.skills.allow_custom_paths_without_trust = value,
                None => tracing::warn!(
                    "Invalid XZATOMA_SKILLS_ALLOW_CUSTOM_PATHS_WITHOUT_TRUST: {}",
                    allow_custom_paths_without_trust
                ),
            }
        }

        if let Ok(strict_frontmatter) = std::env::var("XZATOMA_SKILLS_STRICT_FRONTMATTER") {
            match parse_env_bool(&strict_frontmatter) {
                Some(value) => self.skills.strict_frontmatter = value,
                None => tracing::warn!(
                    "Invalid XZATOMA_SKILLS_STRICT_FRONTMATTER: {}",
                    strict_frontmatter
                ),
            }
        }

        if let Ok(additional_paths) = std::env::var("XZATOMA_SKILLS_ADDITIONAL_PATHS") {
            let parsed_paths: Vec<String> = additional_paths
                .split(':')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect();

            if parsed_paths.is_empty() {
                tracing::warn!(
                    "Invalid XZATOMA_SKILLS_ADDITIONAL_PATHS: no non-empty paths were provided"
                );
            } else {
                self.skills.additional_paths = parsed_paths;
            }
        }

        if let Ok(max_discovered_skills) = std::env::var("XZATOMA_SKILLS_MAX_DISCOVERED_SKILLS") {
            if let Ok(value) = max_discovered_skills.parse() {
                self.skills.max_discovered_skills = value;
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_SKILLS_MAX_DISCOVERED_SKILLS: {}",
                    max_discovered_skills
                );
            }
        }

        if let Ok(max_scan_directories) = std::env::var("XZATOMA_SKILLS_MAX_SCAN_DIRECTORIES") {
            if let Ok(value) = max_scan_directories.parse() {
                self.skills.max_scan_directories = value;
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_SKILLS_MAX_SCAN_DIRECTORIES: {}",
                    max_scan_directories
                );
            }
        }

        if let Ok(max_scan_depth) = std::env::var("XZATOMA_SKILLS_MAX_SCAN_DEPTH") {
            if let Ok(value) = max_scan_depth.parse() {
                self.skills.max_scan_depth = value;
            } else {
                tracing::warn!("Invalid XZATOMA_SKILLS_MAX_SCAN_DEPTH: {}", max_scan_depth);
            }
        }

        if let Ok(catalog_max_entries) = std::env::var("XZATOMA_SKILLS_CATALOG_MAX_ENTRIES") {
            if let Ok(value) = catalog_max_entries.parse() {
                self.skills.catalog_max_entries = value;
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_SKILLS_CATALOG_MAX_ENTRIES: {}",
                    catalog_max_entries
                );
            }
        }

        if let Ok(trust_store_path) = std::env::var("XZATOMA_SKILLS_TRUST_STORE_PATH") {
            let trimmed = trust_store_path.trim();
            self.skills.trust_store_path = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }

        // ---------------------------------------------------------------------
        // Watcher-specific environment variable overrides
        // Supports: XZATOMA_WATCHER_* for backend selection, matching, filters,
        // logging, execution, and XZEPR_KAFKA_* for Kafka connection overrides.
        // ---------------------------------------------------------------------

        if let Ok(watcher_type) = std::env::var("XZATOMA_WATCHER_TYPE") {
            match WatcherType::from_str_name(&watcher_type) {
                Some(value) => {
                    self.watcher.watcher_type = value;
                    tracing::debug!(
                        watcher_type = %watcher_type,
                        "Env override: XZATOMA_WATCHER_TYPE"
                    );
                }
                None => {
                    tracing::warn!("Invalid value for XZATOMA_WATCHER_TYPE: {}", watcher_type);
                }
            }
        }

        if let Ok(output_topic) = std::env::var("XZATOMA_WATCHER_OUTPUT_TOPIC") {
            if let Some(ref mut kafka_cfg) = self.watcher.kafka {
                kafka_cfg.output_topic = Some(output_topic.clone());
            } else {
                self.watcher.kafka = Some(KafkaWatcherConfig {
                    brokers: "localhost:9092".to_string(),
                    topic: "xzepr.dev.events".to_string(),
                    output_topic: Some(output_topic.clone()),
                    group_id: default_watcher_group_id(),
                    auto_create_topics: default_auto_create_topics(),
                    num_partitions: 1,
                    replication_factor: 1,
                    security: None,
                });
            }

            tracing::debug!(
                output_topic = %output_topic,
                "Env override: XZATOMA_WATCHER_OUTPUT_TOPIC"
            );
        }

        if let Ok(group_id) = std::env::var("XZATOMA_WATCHER_GROUP_ID") {
            if let Some(ref mut kafka_cfg) = self.watcher.kafka {
                kafka_cfg.group_id = group_id.clone();
            } else {
                self.watcher.kafka = Some(KafkaWatcherConfig {
                    brokers: "localhost:9092".to_string(),
                    topic: "xzepr.dev.events".to_string(),
                    output_topic: None,
                    group_id: group_id.clone(),
                    auto_create_topics: default_auto_create_topics(),
                    num_partitions: 1,
                    replication_factor: 1,
                    security: None,
                });
            }

            tracing::debug!(
                group_id = %group_id,
                "Env override: XZATOMA_WATCHER_GROUP_ID"
            );
        }

        if let Ok(action) = std::env::var("XZATOMA_WATCHER_MATCH_ACTION") {
            self.watcher.generic_match.action = Some(action.clone());
            tracing::debug!(
                action = %action,
                "Env override: XZATOMA_WATCHER_MATCH_ACTION"
            );
        }

        if let Ok(name) = std::env::var("XZATOMA_WATCHER_MATCH_NAME") {
            self.watcher.generic_match.name = Some(name.clone());
            tracing::debug!(
                name = %name,
                "Env override: XZATOMA_WATCHER_MATCH_NAME"
            );
        }

        if let Ok(version) = std::env::var("XZATOMA_WATCHER_MATCH_VERSION") {
            self.watcher.generic_match.version = Some(version.clone());
            tracing::debug!(
                version = %version,
                "Env override: XZATOMA_WATCHER_MATCH_VERSION"
            );
        }

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
                    output_topic: None,
                    group_id,
                    auto_create_topics: default_auto_create_topics(),
                    num_partitions: 1,
                    replication_factor: 1,
                    security,
                });
                tracing::debug!("Populated watcher.kafka from XZEPR_KAFKA_* env vars");
            }
        }

        // ---------------------------------------------------------------------
        // MCP environment variable overrides
        // ---------------------------------------------------------------------
        if let Ok(val) = std::env::var("XZATOMA_MCP_REQUEST_TIMEOUT") {
            if let Ok(n) = val.parse::<u64>() {
                self.mcp.request_timeout_seconds = n;
                tracing::debug!(
                    request_timeout_seconds = n,
                    "Env override: XZATOMA_MCP_REQUEST_TIMEOUT"
                );
            } else {
                tracing::warn!("Invalid value for XZATOMA_MCP_REQUEST_TIMEOUT: {}", val);
            }
        }

        if let Ok(val) = std::env::var("XZATOMA_MCP_AUTO_CONNECT") {
            self.mcp.auto_connect = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
            tracing::debug!(
                auto_connect = self.mcp.auto_connect,
                "Env override: XZATOMA_MCP_AUTO_CONNECT"
            );
        }

        // ---------------------------------------------------------------------
        // ACP environment variable overrides
        // ---------------------------------------------------------------------
        if let Ok(enabled) = std::env::var("XZATOMA_ACP_ENABLED") {
            match parse_env_bool(&enabled) {
                Some(value) => {
                    self.acp.enabled = value;
                    tracing::debug!(enabled = value, "Env override: XZATOMA_ACP_ENABLED");
                }
                None => tracing::warn!("Invalid XZATOMA_ACP_ENABLED: {}", enabled),
            }
        }

        if let Ok(host) = std::env::var("XZATOMA_ACP_HOST") {
            self.acp.host = host.clone();
            tracing::debug!(host = %host, "Env override: XZATOMA_ACP_HOST");
        }

        if let Ok(port) = std::env::var("XZATOMA_ACP_PORT") {
            if let Ok(value) = port.parse::<u16>() {
                self.acp.port = value;
                tracing::debug!(port = value, "Env override: XZATOMA_ACP_PORT");
            } else {
                tracing::warn!("Invalid XZATOMA_ACP_PORT: {}", port);
            }
        }

        if let Ok(mode) = std::env::var("XZATOMA_ACP_COMPATIBILITY_MODE") {
            match mode.to_ascii_lowercase().as_str() {
                "versioned" => {
                    self.acp.compatibility_mode = AcpCompatibilityMode::Versioned;
                    tracing::debug!(
                        compatibility_mode = "versioned",
                        "Env override: XZATOMA_ACP_COMPATIBILITY_MODE"
                    );
                }
                "root_compatible" => {
                    self.acp.compatibility_mode = AcpCompatibilityMode::RootCompatible;
                    tracing::debug!(
                        compatibility_mode = "root_compatible",
                        "Env override: XZATOMA_ACP_COMPATIBILITY_MODE"
                    );
                }
                _ => {
                    tracing::warn!("Invalid XZATOMA_ACP_COMPATIBILITY_MODE: {}", mode);
                }
            }
        }

        if let Ok(base_path) = std::env::var("XZATOMA_ACP_BASE_PATH") {
            self.acp.base_path = base_path.clone();
            tracing::debug!(base_path = %base_path, "Env override: XZATOMA_ACP_BASE_PATH");
        }

        if let Ok(auth_token) = std::env::var("XZATOMA_ACP_AUTH_TOKEN") {
            self.acp.auth_token = Some(auth_token);
            tracing::debug!("Env override: XZATOMA_ACP_AUTH_TOKEN");
        }

        if let Ok(max_request_bytes) = std::env::var("XZATOMA_ACP_MAX_REQUEST_BYTES") {
            if let Ok(value) = max_request_bytes.parse::<usize>() {
                self.acp.max_request_bytes = value;
                tracing::debug!(
                    max_request_bytes = value,
                    "Env override: XZATOMA_ACP_MAX_REQUEST_BYTES"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_MAX_REQUEST_BYTES: {}",
                    max_request_bytes
                );
            }
        }

        if let Ok(rate_limit_per_minute) = std::env::var("XZATOMA_ACP_RATE_LIMIT_PER_MINUTE") {
            if let Ok(value) = rate_limit_per_minute.parse::<usize>() {
                self.acp.rate_limit_per_minute = value;
                tracing::debug!(
                    rate_limit_per_minute = value,
                    "Env override: XZATOMA_ACP_RATE_LIMIT_PER_MINUTE"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_RATE_LIMIT_PER_MINUTE: {}",
                    rate_limit_per_minute
                );
            }
        }

        if let Ok(run_mode) = std::env::var("XZATOMA_ACP_DEFAULT_RUN_MODE") {
            match run_mode.to_ascii_lowercase().as_str() {
                "sync" => {
                    self.acp.default_run_mode = AcpDefaultRunMode::Sync;
                    tracing::debug!(
                        default_run_mode = "sync",
                        "Env override: XZATOMA_ACP_DEFAULT_RUN_MODE"
                    );
                }
                "async" => {
                    self.acp.default_run_mode = AcpDefaultRunMode::Async;
                    tracing::debug!(
                        default_run_mode = "async",
                        "Env override: XZATOMA_ACP_DEFAULT_RUN_MODE"
                    );
                }
                "streaming" => {
                    self.acp.default_run_mode = AcpDefaultRunMode::Streaming;
                    tracing::debug!(
                        default_run_mode = "streaming",
                        "Env override: XZATOMA_ACP_DEFAULT_RUN_MODE"
                    );
                }
                _ => {
                    tracing::warn!("Invalid XZATOMA_ACP_DEFAULT_RUN_MODE: {}", run_mode);
                }
            }
        }

        if let Ok(enabled) = std::env::var("XZATOMA_ACP_PERSISTENCE_ENABLED") {
            match parse_env_bool(&enabled) {
                Some(value) => {
                    self.acp.persistence.enabled = value;
                    tracing::debug!(
                        persistence_enabled = value,
                        "Env override: XZATOMA_ACP_PERSISTENCE_ENABLED"
                    );
                }
                None => tracing::warn!("Invalid XZATOMA_ACP_PERSISTENCE_ENABLED: {}", enabled),
            }
        }

        if let Ok(max_events_per_run) = std::env::var("XZATOMA_ACP_MAX_EVENTS_PER_RUN") {
            if let Ok(value) = max_events_per_run.parse::<usize>() {
                self.acp.persistence.max_events_per_run = value;
                tracing::debug!(
                    max_events_per_run = value,
                    "Env override: XZATOMA_ACP_MAX_EVENTS_PER_RUN"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_MAX_EVENTS_PER_RUN: {}",
                    max_events_per_run
                );
            }
        }

        if let Ok(max_completed_runs) = std::env::var("XZATOMA_ACP_MAX_COMPLETED_RUNS") {
            if let Ok(value) = max_completed_runs.parse::<usize>() {
                self.acp.persistence.max_completed_runs = value;
                tracing::debug!(
                    max_completed_runs = value,
                    "Env override: XZATOMA_ACP_MAX_COMPLETED_RUNS"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_MAX_COMPLETED_RUNS: {}",
                    max_completed_runs
                );
            }
        }

        if let Ok(persist_sessions) = std::env::var("XZATOMA_ACP_STDIO_PERSIST_SESSIONS") {
            match parse_env_bool(&persist_sessions) {
                Some(value) => {
                    self.acp.stdio.persist_sessions = value;
                    tracing::debug!(
                        persist_sessions = value,
                        "Env override: XZATOMA_ACP_STDIO_PERSIST_SESSIONS"
                    );
                }
                None => tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_PERSIST_SESSIONS: {}",
                    persist_sessions
                ),
            }
        }

        if let Ok(resume_by_workspace) = std::env::var("XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE") {
            match parse_env_bool(&resume_by_workspace) {
                Some(value) => {
                    self.acp.stdio.resume_by_workspace = value;
                    tracing::debug!(
                        resume_by_workspace = value,
                        "Env override: XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE"
                    );
                }
                None => tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE: {}",
                    resume_by_workspace
                ),
            }
        }

        if let Ok(max_active_sessions) = std::env::var("XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS") {
            if let Ok(value) = max_active_sessions.parse::<usize>() {
                self.acp.stdio.max_active_sessions = value;
                tracing::debug!(
                    max_active_sessions = value,
                    "Env override: XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS: {}",
                    max_active_sessions
                );
            }
        }

        if let Ok(session_timeout_seconds) =
            std::env::var("XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS")
        {
            if let Ok(value) = session_timeout_seconds.parse::<u64>() {
                self.acp.stdio.session_timeout_seconds = value;
                tracing::debug!(
                    session_timeout_seconds = value,
                    "Env override: XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS: {}",
                    session_timeout_seconds
                );
            }
        }

        if let Ok(prompt_queue_capacity) = std::env::var("XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY")
        {
            if let Ok(value) = prompt_queue_capacity.parse::<usize>() {
                self.acp.stdio.prompt_queue_capacity = value;
                tracing::debug!(
                    prompt_queue_capacity = value,
                    "Env override: XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY: {}",
                    prompt_queue_capacity
                );
            }
        }

        if let Ok(model_list_timeout_seconds) =
            std::env::var("XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS")
        {
            if let Ok(value) = model_list_timeout_seconds.parse::<u64>() {
                self.acp.stdio.model_list_timeout_seconds = value;
                tracing::debug!(
                    model_list_timeout_seconds = value,
                    "Env override: XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS: {}",
                    model_list_timeout_seconds
                );
            }
        }

        if let Ok(vision_enabled) = std::env::var("XZATOMA_ACP_STDIO_VISION_ENABLED") {
            match parse_env_bool(&vision_enabled) {
                Some(value) => {
                    self.acp.stdio.vision_enabled = value;
                    tracing::debug!(
                        vision_enabled = value,
                        "Env override: XZATOMA_ACP_STDIO_VISION_ENABLED"
                    );
                }
                None => tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_VISION_ENABLED: {}",
                    vision_enabled
                ),
            }
        }

        if let Ok(max_image_bytes) = std::env::var("XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES") {
            if let Ok(value) = max_image_bytes.parse::<usize>() {
                self.acp.stdio.max_image_bytes = value;
                tracing::debug!(
                    max_image_bytes = value,
                    "Env override: XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES"
                );
            } else {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES: {}",
                    max_image_bytes
                );
            }
        }

        if let Ok(mime_types) = std::env::var("XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES") {
            let parsed: Vec<String> = mime_types
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect();

            if parsed.is_empty() {
                tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES: no MIME types provided"
                );
            } else {
                self.acp.stdio.allowed_image_mime_types = parsed;
                tracing::debug!("Env override: XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES");
            }
        }

        if let Ok(allow_file_references) =
            std::env::var("XZATOMA_ACP_STDIO_ALLOW_IMAGE_FILE_REFERENCES")
        {
            match parse_env_bool(&allow_file_references) {
                Some(value) => {
                    self.acp.stdio.allow_image_file_references = value;
                    tracing::debug!(
                        allow_image_file_references = value,
                        "Env override: XZATOMA_ACP_STDIO_ALLOW_IMAGE_FILE_REFERENCES"
                    );
                }
                None => tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_ALLOW_IMAGE_FILE_REFERENCES: {}",
                    allow_file_references
                ),
            }
        }

        if let Ok(allow_remote_urls) = std::env::var("XZATOMA_ACP_STDIO_ALLOW_REMOTE_IMAGE_URLS") {
            match parse_env_bool(&allow_remote_urls) {
                Some(value) => {
                    self.acp.stdio.allow_remote_image_urls = value;
                    tracing::debug!(
                        allow_remote_image_urls = value,
                        "Env override: XZATOMA_ACP_STDIO_ALLOW_REMOTE_IMAGE_URLS"
                    );
                }
                None => tracing::warn!(
                    "Invalid XZATOMA_ACP_STDIO_ALLOW_REMOTE_IMAGE_URLS: {}",
                    allow_remote_urls
                ),
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
            return Err(XzatomaError::Config(
                "Provider type cannot be empty".to_string(),
            ));
        }

        let valid_providers = ["copilot", "ollama", "openai"];
        if !valid_providers.contains(&self.provider.provider_type.as_str()) {
            return Err(XzatomaError::Config(format!(
                "Invalid provider type: {}. Must be one of: {}",
                self.provider.provider_type,
                valid_providers.join(", ")
            )));
        }

        if self.agent.max_turns == 0 {
            return Err(XzatomaError::Config(
                "max_turns must be greater than 0".to_string(),
            ));
        }

        if self.agent.max_turns > 1000 {
            return Err(XzatomaError::Config(
                "max_turns must be less than or equal to 1000".to_string(),
            ));
        }

        if self.agent.timeout_seconds == 0 {
            return Err(XzatomaError::Config(
                "timeout_seconds must be greater than 0".to_string(),
            ));
        }

        if self.agent.conversation.max_tokens == 0 {
            return Err(XzatomaError::Config(
                "conversation.max_tokens must be greater than 0".to_string(),
            ));
        }

        if self.agent.conversation.prune_threshold <= 0.0
            || self.agent.conversation.prune_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.prune_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.agent.conversation.warning_threshold <= 0.0
            || self.agent.conversation.warning_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.warning_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.agent.conversation.auto_summary_threshold <= 0.0
            || self.agent.conversation.auto_summary_threshold > 1.0
        {
            return Err(XzatomaError::Config(
                "conversation.auto_summary_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.agent.conversation.auto_summary_threshold
            < self.agent.conversation.warning_threshold
        {
            return Err(XzatomaError::Config(
                "conversation.auto_summary_threshold must be >= warning_threshold".to_string(),
            ));
        }

        if self.agent.tools.max_output_size == 0 {
            return Err(XzatomaError::Config(
                "tools.max_output_size must be greater than 0".to_string(),
            ));
        }

        if self.agent.tools.max_file_read_size == 0 {
            return Err(XzatomaError::Config(
                "tools.max_file_read_size must be greater than 0".to_string(),
            ));
        }

        // Validate subagent configuration
        if self.agent.subagent.max_depth == 0 {
            return Err(XzatomaError::Config(
                "agent.subagent.max_depth must be greater than 0".to_string(),
            ));
        }

        if self.agent.subagent.max_depth > 10 {
            return Err(XzatomaError::Config(
                "agent.subagent.max_depth cannot exceed 10 (stack overflow risk)".to_string(),
            ));
        }

        if self.agent.subagent.default_max_turns == 0 {
            return Err(XzatomaError::Config(
                "agent.subagent.default_max_turns must be greater than 0".to_string(),
            ));
        }

        if self.agent.subagent.default_max_turns > 100 {
            return Err(XzatomaError::Config(
                "agent.subagent.default_max_turns cannot exceed 100".to_string(),
            ));
        }

        if self.agent.subagent.output_max_size < 1024 {
            return Err(XzatomaError::Config(
                "agent.subagent.output_max_size must be at least 1024 bytes".to_string(),
            ));
        }

        // Validate subagent provider override if specified
        if let Some(ref provider) = self.agent.subagent.provider {
            let valid_providers = ["copilot", "ollama", "openai"];
            if !valid_providers.contains(&provider.as_str()) {
                return Err(XzatomaError::Config(format!(
                    "Invalid subagent provider override: {}. Must be one of: {}",
                    provider,
                    valid_providers.join(", ")
                )));
            }
        }

        if let Some(kafka) = &self.watcher.kafka {
            if kafka.brokers.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "watcher.kafka.brokers cannot be empty".to_string(),
                ));
            }

            if kafka.topic.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "watcher.kafka.topic cannot be empty".to_string(),
                ));
            }

            if kafka.group_id.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "watcher.kafka.group_id cannot be empty".to_string(),
                ));
            }

            if let Some(output_topic) = &kafka.output_topic {
                if output_topic.trim().is_empty() {
                    return Err(XzatomaError::Config(
                        "watcher.kafka.output_topic cannot be empty when set".to_string(),
                    ));
                }
            }

            if kafka.group_id.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "watcher.kafka.group_id cannot be empty".to_string(),
                ));
            }
        }

        match self.watcher.watcher_type {
            WatcherType::Generic => {
                if self.watcher.kafka.is_none() {
                    return Err(XzatomaError::Config(
                        "watcher.kafka is required when watcher.watcher_type is generic"
                            .to_string(),
                    ));
                }

                let mut configured_patterns = Vec::new();

                if let Some(action) = &self.watcher.generic_match.action {
                    Regex::new(action).map_err(|e| {
                        XzatomaError::Config(format!(
                            "Invalid generic match regex for watcher.generic_match.action: {}",
                            e
                        ))
                    })?;
                    configured_patterns.push("action");
                }

                if let Some(name) = &self.watcher.generic_match.name {
                    Regex::new(name).map_err(|e| {
                        XzatomaError::Config(format!(
                            "Invalid generic match regex for watcher.generic_match.name: {}",
                            e
                        ))
                    })?;
                    configured_patterns.push("name");
                }

                if let Some(version) = &self.watcher.generic_match.version {
                    Regex::new(version).map_err(|e| {
                        XzatomaError::Config(format!(
                            "Invalid generic match regex for watcher.generic_match.version: {}",
                            e
                        ))
                    })?;
                    configured_patterns.push("version");
                }

                if configured_patterns.is_empty() {
                    tracing::warn!(
                        "watcher.watcher_type=generic with no generic_match fields configured; accept-all mode is active"
                    );
                }
            }
            WatcherType::XZepr => {
                if self.watcher.generic_match.action.is_some()
                    || self.watcher.generic_match.name.is_some()
                    || self.watcher.generic_match.version.is_some()
                {
                    tracing::debug!(
                        "watcher.watcher_type=xzepr; watcher.generic_match configuration is unused"
                    );
                }
            }
        }

        crate::security::normalize_http_base_url(
            &self.provider.openai.base_url,
            "provider.openai.base_url",
        )?;
        crate::security::normalize_http_base_url(
            &self.provider.ollama.host,
            "provider.ollama.host",
        )?;
        if let Some(api_base) = &self.provider.copilot.api_base {
            crate::security::validate_loopback_http_base_url(
                api_base,
                "provider.copilot.api_base",
            )?;
        }

        // Validate MCP configuration
        self.mcp.validate()?;
        self.validate_acp_config()?;
        self.validate_skills_config()?;

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

    fn validate_acp_config(&self) -> Result<()> {
        if self.acp.host.trim().is_empty() {
            return Err(XzatomaError::Config("acp.host cannot be empty".to_string()));
        }

        if self.acp.port == 0 {
            return Err(XzatomaError::Config(
                "acp.port must be greater than 0".to_string(),
            ));
        }

        if let Some(token) = &self.acp.auth_token {
            if token.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "acp.auth_token cannot be empty when set".to_string(),
                ));
            }
        }

        if self.acp.max_request_bytes == 0 {
            return Err(XzatomaError::Config(
                "acp.max_request_bytes must be greater than 0".to_string(),
            ));
        }

        if self.acp.rate_limit_per_minute == 0 {
            return Err(XzatomaError::Config(
                "acp.rate_limit_per_minute must be greater than 0".to_string(),
            ));
        }

        match self.acp.compatibility_mode {
            AcpCompatibilityMode::Versioned => {
                if self.acp.base_path.trim().is_empty() {
                    return Err(XzatomaError::Config(
                        "acp.base_path cannot be empty".to_string(),
                    ));
                }

                if !self.acp.base_path.starts_with('/') {
                    return Err(XzatomaError::Config(
                        "acp.base_path must start with '/'".to_string(),
                    ));
                }

                if self.acp.base_path == "/" {
                    return Err(XzatomaError::Config(
                        "acp.base_path cannot be '/' in versioned compatibility mode".to_string(),
                    ));
                }
            }
            AcpCompatibilityMode::RootCompatible => {
                if self.acp.base_path.trim().is_empty() {
                    return Err(XzatomaError::Config(
                        "acp.base_path cannot be empty".to_string(),
                    ));
                }

                if !self.acp.base_path.starts_with('/') {
                    return Err(XzatomaError::Config(
                        "acp.base_path must start with '/'".to_string(),
                    ));
                }
            }
        }

        if self.acp.persistence.max_events_per_run == 0 {
            return Err(XzatomaError::Config(
                "acp.persistence.max_events_per_run must be greater than 0".to_string(),
            ));
        }

        if self.acp.persistence.max_completed_runs == 0 {
            return Err(XzatomaError::Config(
                "acp.persistence.max_completed_runs must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.max_active_sessions == 0 {
            return Err(XzatomaError::Config(
                "acp.stdio.max_active_sessions must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.session_timeout_seconds == 0 {
            return Err(XzatomaError::Config(
                "acp.stdio.session_timeout_seconds must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.prompt_queue_capacity == 0 {
            return Err(XzatomaError::Config(
                "acp.stdio.prompt_queue_capacity must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.model_list_timeout_seconds == 0 {
            return Err(XzatomaError::Config(
                "acp.stdio.model_list_timeout_seconds must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.max_image_bytes == 0 {
            return Err(XzatomaError::Config(
                "acp.stdio.max_image_bytes must be greater than 0".to_string(),
            ));
        }

        if self.acp.stdio.allowed_image_mime_types.is_empty() {
            return Err(XzatomaError::Config(
                "acp.stdio.allowed_image_mime_types cannot be empty".to_string(),
            ));
        }

        for mime_type in &self.acp.stdio.allowed_image_mime_types {
            let trimmed = mime_type.trim();
            if trimmed.is_empty() {
                return Err(XzatomaError::Config(
                    "acp.stdio.allowed_image_mime_types cannot contain empty values".to_string(),
                ));
            }

            if !trimmed.starts_with("image/") {
                return Err(XzatomaError::Config(format!(
                    "acp.stdio.allowed_image_mime_types value '{}' must start with 'image/'",
                    mime_type
                )));
            }
        }

        Ok(())
    }

    fn validate_skills_config(&self) -> Result<()> {
        if self.skills.max_discovered_skills == 0 {
            return Err(XzatomaError::Config(
                "skills.max_discovered_skills must be greater than 0".to_string(),
            ));
        }

        if self.skills.max_scan_directories == 0 {
            return Err(XzatomaError::Config(
                "skills.max_scan_directories must be greater than 0".to_string(),
            ));
        }

        if self.skills.max_scan_depth == 0 {
            return Err(XzatomaError::Config(
                "skills.max_scan_depth must be greater than 0".to_string(),
            ));
        }

        if self.skills.catalog_max_entries == 0 {
            return Err(XzatomaError::Config(
                "skills.catalog_max_entries must be greater than 0".to_string(),
            ));
        }

        if self.skills.catalog_max_entries > self.skills.max_discovered_skills {
            return Err(XzatomaError::Config(
                "skills.catalog_max_entries must be less than or equal to skills.max_discovered_skills"
                    .to_string(),
            ));
        }

        for path in &self.skills.additional_paths {
            if path.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "skills.additional_paths cannot contain empty entries".to_string(),
                ));
            }
        }

        if let Some(path) = &self.skills.trust_store_path {
            if path.trim().is_empty() {
                return Err(XzatomaError::Config(
                    "skills.trust_store_path cannot be empty when set".to_string(),
                ));
            }

            let resolved = resolve_config_like_path(path);
            if resolved.as_os_str().is_empty() {
                return Err(XzatomaError::Config(
                    "skills.trust_store_path resolved to an empty path".to_string(),
                ));
            }
        }

        Ok(())
    }
}

fn parse_env_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn resolve_config_like_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }

    PathBuf::from(path)
}

impl Default for Config {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                std::env::set_var(self.key, original);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn test_apply_env_vars_overrides_acp_stdio_fields() {
        let _persist_sessions = EnvVarGuard::set("XZATOMA_ACP_STDIO_PERSIST_SESSIONS", "false");
        let _resume_by_workspace =
            EnvVarGuard::set("XZATOMA_ACP_STDIO_RESUME_BY_WORKSPACE", "false");
        let _max_active_sessions = EnvVarGuard::set("XZATOMA_ACP_STDIO_MAX_ACTIVE_SESSIONS", "8");
        let _session_timeout_seconds =
            EnvVarGuard::set("XZATOMA_ACP_STDIO_SESSION_TIMEOUT_SECONDS", "120");
        let _prompt_queue_capacity =
            EnvVarGuard::set("XZATOMA_ACP_STDIO_PROMPT_QUEUE_CAPACITY", "4");
        let _model_list_timeout_seconds =
            EnvVarGuard::set("XZATOMA_ACP_STDIO_MODEL_LIST_TIMEOUT_SECONDS", "2");
        let _vision_enabled = EnvVarGuard::set("XZATOMA_ACP_STDIO_VISION_ENABLED", "false");
        let _max_image_bytes = EnvVarGuard::set("XZATOMA_ACP_STDIO_MAX_IMAGE_BYTES", "4096");
        let _allowed_image_mime_types = EnvVarGuard::set(
            "XZATOMA_ACP_STDIO_ALLOWED_IMAGE_MIME_TYPES",
            "image/png,image/webp",
        );

        let mut config = Config::default_config();
        config.apply_env_vars();

        assert!(!config.acp.stdio.persist_sessions);
        assert!(!config.acp.stdio.resume_by_workspace);
        assert_eq!(config.acp.stdio.max_active_sessions, 8);
        assert_eq!(config.acp.stdio.session_timeout_seconds, 120);
        assert_eq!(config.acp.stdio.prompt_queue_capacity, 4);
        assert_eq!(config.acp.stdio.model_list_timeout_seconds, 2);
        assert!(!config.acp.stdio.vision_enabled);
        assert_eq!(config.acp.stdio.max_image_bytes, 4096);
        assert_eq!(
            config.acp.stdio.allowed_image_mime_types,
            vec!["image/png".to_string(), "image/webp".to_string()]
        );
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.provider.provider_type, "copilot");
        assert_eq!(config.agent.max_turns, 50);
        assert_eq!(config.agent.timeout_seconds, 300);
        assert_eq!(config.watcher.watcher_type, WatcherType::XZepr);
        assert!(!config.acp.enabled);
        assert_eq!(config.acp.host, "127.0.0.1");
        assert_eq!(config.acp.port, 8765);
        assert_eq!(
            config.acp.compatibility_mode,
            AcpCompatibilityMode::Versioned
        );
        assert_eq!(config.acp.base_path, "/api/v1/acp");
        assert!(config.acp.auth_token.is_none());
        assert_eq!(config.acp.max_request_bytes, 1024 * 1024);
        assert_eq!(config.acp.rate_limit_per_minute, 120);
        assert_eq!(config.acp.default_run_mode, AcpDefaultRunMode::Async);
        assert!(!config.acp.persistence.enabled);
        assert_eq!(config.acp.persistence.max_events_per_run, 1000);
        assert_eq!(config.acp.persistence.max_completed_runs, 1000);
        assert!(config.skills.enabled);
        assert!(config.skills.project_enabled);
        assert!(config.skills.user_enabled);
        assert_eq!(config.skills.max_discovered_skills, 256);
        assert_eq!(config.skills.max_scan_directories, 2000);
        assert_eq!(config.skills.max_scan_depth, 6);
        assert_eq!(config.skills.catalog_max_entries, 128);
        assert!(config.skills.activation_tool_enabled);
        assert!(config.skills.project_trust_required);
        assert!(config.skills.strict_frontmatter);
    }

    #[test]
    fn test_apply_env_vars_overrides_acp_config() {
        let _enabled = EnvVarGuard::set("XZATOMA_ACP_ENABLED", "true");
        let _host = EnvVarGuard::set("XZATOMA_ACP_HOST", "0.0.0.0");
        let _port = EnvVarGuard::set("XZATOMA_ACP_PORT", "9001");
        let _compatibility_mode =
            EnvVarGuard::set("XZATOMA_ACP_COMPATIBILITY_MODE", "root_compatible");
        let _base_path = EnvVarGuard::set("XZATOMA_ACP_BASE_PATH", "/acp");
        let _auth_token = EnvVarGuard::set("XZATOMA_ACP_AUTH_TOKEN", "test-token");
        let _max_request_bytes = EnvVarGuard::set("XZATOMA_ACP_MAX_REQUEST_BYTES", "4096");
        let _rate_limit_per_minute = EnvVarGuard::set("XZATOMA_ACP_RATE_LIMIT_PER_MINUTE", "60");
        let _default_run_mode = EnvVarGuard::set("XZATOMA_ACP_DEFAULT_RUN_MODE", "streaming");
        let _persistence_enabled = EnvVarGuard::set("XZATOMA_ACP_PERSISTENCE_ENABLED", "true");
        let _max_events_per_run = EnvVarGuard::set("XZATOMA_ACP_MAX_EVENTS_PER_RUN", "500");
        let _max_completed_runs = EnvVarGuard::set("XZATOMA_ACP_MAX_COMPLETED_RUNS", "250");

        let mut config = Config::default();
        config.apply_env_vars();

        assert!(config.acp.enabled);
        assert_eq!(config.acp.host, "0.0.0.0");
        assert_eq!(config.acp.port, 9001);
        assert_eq!(
            config.acp.compatibility_mode,
            AcpCompatibilityMode::RootCompatible
        );
        assert_eq!(config.acp.base_path, "/acp");
        assert_eq!(config.acp.auth_token.as_deref(), Some("test-token"));
        assert_eq!(config.acp.max_request_bytes, 4096);
        assert_eq!(config.acp.rate_limit_per_minute, 60);
        assert_eq!(config.acp.default_run_mode, AcpDefaultRunMode::Streaming);
        assert!(config.acp.persistence.enabled);
        assert_eq!(config.acp.persistence.max_events_per_run, 500);
        assert_eq!(config.acp.persistence.max_completed_runs, 250);
    }

    #[test]
    fn test_config_validation_rejects_empty_acp_host() {
        let mut config = Config::default();
        config.acp.host = String::new();

        let error = config.validate().expect_err("config should be invalid");
        assert!(error.to_string().contains("acp.host cannot be empty"));
    }

    #[test]
    fn test_config_validation_rejects_blank_acp_auth_token() {
        let mut config = Config::default();
        config.acp.auth_token = Some("   ".to_string());

        let error = config.validate().expect_err("config should be invalid");
        assert!(error.to_string().contains("acp.auth_token cannot be empty"));
    }

    #[test]
    fn test_config_validation_rejects_zero_acp_limits() {
        let mut config = Config::default();
        config.acp.max_request_bytes = 0;

        let error = config.validate().expect_err("config should be invalid");
        assert!(error
            .to_string()
            .contains("acp.max_request_bytes must be greater than 0"));

        let mut config = Config::default();
        config.acp.rate_limit_per_minute = 0;

        let error = config.validate().expect_err("config should be invalid");
        assert!(error
            .to_string()
            .contains("acp.rate_limit_per_minute must be greater than 0"));
    }

    #[test]
    fn test_config_validation_rejects_root_base_path_in_versioned_mode() {
        let mut config = Config::default();
        config.acp.base_path = "/".to_string();

        let error = config.validate().expect_err("config should be invalid");
        assert!(error
            .to_string()
            .contains("acp.base_path cannot be '/' in versioned compatibility mode"));
    }

    #[test]
    fn test_config_validation_accepts_root_compatible_mode_with_custom_base_path() {
        let mut config = Config::default();
        config.acp.compatibility_mode = AcpCompatibilityMode::RootCompatible;
        config.acp.base_path = "/ignored-but-valid".to_string();

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_rejects_zero_acp_persistence_limits() {
        let mut config = Config::default();
        config.acp.persistence.max_events_per_run = 0;

        let error = config.validate().expect_err("config should be invalid");
        assert!(error
            .to_string()
            .contains("acp.persistence.max_events_per_run must be greater than 0"));

        config.acp.persistence.max_events_per_run = 1000;
        config.acp.persistence.max_completed_runs = 0;

        let error = config.validate().expect_err("config should be invalid");
        assert!(error
            .to_string()
            .contains("acp.persistence.max_completed_runs must be greater than 0"));
    }

    #[test]
    fn test_config_validation_success() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_skills_config_defaults() {
        let config = SkillsConfig::default();
        assert!(config.enabled);
        assert!(config.project_enabled);
        assert!(config.user_enabled);
        assert!(config.additional_paths.is_empty());
        assert_eq!(config.max_discovered_skills, 256);
        assert_eq!(config.max_scan_directories, 2000);
        assert_eq!(config.max_scan_depth, 6);
        assert_eq!(config.catalog_max_entries, 128);
        assert!(config.activation_tool_enabled);
        assert!(config.project_trust_required);
        assert!(!config.allow_custom_paths_without_trust);
        assert!(config.strict_frontmatter);
        assert_eq!(config.trust_store_path, None);
    }

    #[test]
    fn test_skills_config_deserialize_with_all_fields() {
        let yaml = r#"
enabled: false
project_enabled: false
user_enabled: true
additional_paths:
  - /opt/xzatoma/skills
  - ./custom_skills
max_discovered_skills: 64
max_scan_directories: 500
max_scan_depth: 4
catalog_max_entries: 32
activation_tool_enabled: false
project_trust_required: false
trust_store_path: ~/.xzatoma/custom_skills_trust.yaml
allow_custom_paths_without_trust: true
strict_frontmatter: false
"#;

        let config: SkillsConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!config.enabled);
        assert!(!config.project_enabled);
        assert!(config.user_enabled);
        assert_eq!(
            config.additional_paths,
            vec![
                "/opt/xzatoma/skills".to_string(),
                "./custom_skills".to_string()
            ]
        );
        assert_eq!(config.max_discovered_skills, 64);
        assert_eq!(config.max_scan_directories, 500);
        assert_eq!(config.max_scan_depth, 4);
        assert_eq!(config.catalog_max_entries, 32);
        assert!(!config.activation_tool_enabled);
        assert!(!config.project_trust_required);
        assert_eq!(
            config.trust_store_path,
            Some("~/.xzatoma/custom_skills_trust.yaml".to_string())
        );
        assert!(config.allow_custom_paths_without_trust);
        assert!(!config.strict_frontmatter);
    }

    #[test]
    fn test_skills_config_deserialize_uses_defaults_for_omitted_fields() {
        let yaml = r#"
enabled: true
"#;

        let config: SkillsConfig = serde_yaml::from_str(yaml).unwrap();
        let defaults = SkillsConfig::default();

        assert_eq!(config.enabled, defaults.enabled);
        assert_eq!(config.project_enabled, defaults.project_enabled);
        assert_eq!(config.user_enabled, defaults.user_enabled);
        assert_eq!(config.additional_paths, defaults.additional_paths);
        assert_eq!(config.max_discovered_skills, defaults.max_discovered_skills);
        assert_eq!(config.max_scan_directories, defaults.max_scan_directories);
        assert_eq!(config.max_scan_depth, defaults.max_scan_depth);
        assert_eq!(config.catalog_max_entries, defaults.catalog_max_entries);
        assert_eq!(
            config.activation_tool_enabled,
            defaults.activation_tool_enabled
        );
        assert_eq!(
            config.project_trust_required,
            defaults.project_trust_required
        );
        assert_eq!(config.trust_store_path, defaults.trust_store_path);
        assert_eq!(
            config.allow_custom_paths_without_trust,
            defaults.allow_custom_paths_without_trust
        );
        assert_eq!(config.strict_frontmatter, defaults.strict_frontmatter);
    }

    #[test]
    fn test_skills_config_validation_rejects_zero_max_discovered_skills() {
        let mut config = Config::default();
        config.skills.max_discovered_skills = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_zero_max_scan_directories() {
        let mut config = Config::default();
        config.skills.max_scan_directories = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_zero_max_scan_depth() {
        let mut config = Config::default();
        config.skills.max_scan_depth = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_zero_catalog_max_entries() {
        let mut config = Config::default();
        config.skills.catalog_max_entries = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_catalog_max_entries_above_max_discovered() {
        let mut config = Config::default();
        config.skills.max_discovered_skills = 8;
        config.skills.catalog_max_entries = 9;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_empty_additional_path() {
        let mut config = Config::default();
        config.skills.additional_paths = vec!["/opt/xzatoma/skills".to_string(), "".to_string()];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_skills_config_validation_rejects_empty_trust_store_path() {
        let mut config = Config::default();
        config.skills.trust_store_path = Some("   ".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_parse_env_bool_accepts_supported_values() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool("yes"), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));
        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool("no"), Some(false));
        assert_eq!(parse_env_bool("off"), Some(false));
    }

    #[test]
    fn test_parse_env_bool_rejects_invalid_value() {
        assert_eq!(parse_env_bool("maybe"), None);
    }

    #[test]
    fn test_resolve_config_like_path_expands_home_prefix() {
        let original_home = std::env::var("HOME").ok();

        unsafe {
            std::env::set_var("HOME", "/tmp/xzatoma-home");
        }

        let resolved = resolve_config_like_path("~/skills_trust.yaml");

        match original_home {
            Some(value) => unsafe {
                std::env::set_var("HOME", value);
            },
            None => unsafe {
                std::env::remove_var("HOME");
            },
        }

        assert_eq!(
            resolved,
            PathBuf::from("/tmp/xzatoma-home/skills_trust.yaml")
        );
    }

    #[test]
    fn test_resolve_config_like_path_returns_plain_path_when_not_tilde_prefixed() {
        let resolved = resolve_config_like_path("./config/config.yaml");
        assert_eq!(resolved, PathBuf::from("./config/config.yaml"));
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_skills_fields() {
        unsafe {
            std::env::remove_var("XZATOMA_SKILLS_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_PROJECT_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_USER_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED");
            std::env::remove_var("XZATOMA_SKILLS_ALLOW_CUSTOM_PATHS_WITHOUT_TRUST");
            std::env::remove_var("XZATOMA_SKILLS_STRICT_FRONTMATTER");
            std::env::remove_var("XZATOMA_SKILLS_ADDITIONAL_PATHS");
            std::env::remove_var("XZATOMA_SKILLS_MAX_DISCOVERED_SKILLS");
            std::env::remove_var("XZATOMA_SKILLS_MAX_SCAN_DIRECTORIES");
            std::env::remove_var("XZATOMA_SKILLS_MAX_SCAN_DEPTH");
            std::env::remove_var("XZATOMA_SKILLS_CATALOG_MAX_ENTRIES");
            std::env::remove_var("XZATOMA_SKILLS_TRUST_STORE_PATH");
        }

        std::env::set_var("XZATOMA_SKILLS_ENABLED", "false");
        std::env::set_var("XZATOMA_SKILLS_PROJECT_ENABLED", "false");
        std::env::set_var("XZATOMA_SKILLS_USER_ENABLED", "false");
        std::env::set_var("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED", "false");
        std::env::set_var("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED", "false");
        std::env::set_var("XZATOMA_SKILLS_ALLOW_CUSTOM_PATHS_WITHOUT_TRUST", "true");
        std::env::set_var("XZATOMA_SKILLS_STRICT_FRONTMATTER", "false");
        std::env::set_var(
            "XZATOMA_SKILLS_ADDITIONAL_PATHS",
            "/opt/xzatoma/skills:./custom_skills",
        );
        std::env::set_var("XZATOMA_SKILLS_MAX_DISCOVERED_SKILLS", "64");
        std::env::set_var("XZATOMA_SKILLS_MAX_SCAN_DIRECTORIES", "400");
        std::env::set_var("XZATOMA_SKILLS_MAX_SCAN_DEPTH", "3");
        std::env::set_var("XZATOMA_SKILLS_CATALOG_MAX_ENTRIES", "16");
        std::env::set_var(
            "XZATOMA_SKILLS_TRUST_STORE_PATH",
            "~/.xzatoma/custom_skills_trust.yaml",
        );

        let mut config = Config::default();
        config.apply_env_vars();

        assert!(!config.skills.enabled);
        assert!(!config.skills.project_enabled);
        assert!(!config.skills.user_enabled);
        assert!(!config.skills.activation_tool_enabled);
        assert!(!config.skills.project_trust_required);
        assert!(config.skills.allow_custom_paths_without_trust);
        assert!(!config.skills.strict_frontmatter);
        assert_eq!(
            config.skills.additional_paths,
            vec![
                "/opt/xzatoma/skills".to_string(),
                "./custom_skills".to_string()
            ]
        );
        assert_eq!(config.skills.max_discovered_skills, 64);
        assert_eq!(config.skills.max_scan_directories, 400);
        assert_eq!(config.skills.max_scan_depth, 3);
        assert_eq!(config.skills.catalog_max_entries, 16);
        assert_eq!(
            config.skills.trust_store_path,
            Some("~/.xzatoma/custom_skills_trust.yaml".to_string())
        );

        unsafe {
            std::env::remove_var("XZATOMA_SKILLS_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_PROJECT_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_USER_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED");
            std::env::remove_var("XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED");
            std::env::remove_var("XZATOMA_SKILLS_ALLOW_CUSTOM_PATHS_WITHOUT_TRUST");
            std::env::remove_var("XZATOMA_SKILLS_STRICT_FRONTMATTER");
            std::env::remove_var("XZATOMA_SKILLS_ADDITIONAL_PATHS");
            std::env::remove_var("XZATOMA_SKILLS_MAX_DISCOVERED_SKILLS");
            std::env::remove_var("XZATOMA_SKILLS_MAX_SCAN_DIRECTORIES");
            std::env::remove_var("XZATOMA_SKILLS_MAX_SCAN_DEPTH");
            std::env::remove_var("XZATOMA_SKILLS_CATALOG_MAX_ENTRIES");
            std::env::remove_var("XZATOMA_SKILLS_TRUST_STORE_PATH");
        }
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
    fn test_watcher_type_default_is_xzepr() {
        assert_eq!(WatcherType::default(), WatcherType::XZepr);
    }

    #[test]
    fn test_watcher_type_roundtrip_xzepr_yaml() {
        let yaml = serde_yaml::to_string(&WatcherType::XZepr).unwrap();
        assert!(yaml.contains("xzepr"));

        let parsed: WatcherType = serde_yaml::from_str("xzepr").unwrap();
        assert_eq!(parsed, WatcherType::XZepr);
    }

    #[test]
    fn test_watcher_type_roundtrip_generic_yaml() {
        let yaml = serde_yaml::to_string(&WatcherType::Generic).unwrap();
        assert!(yaml.contains("generic"));

        let parsed: WatcherType = serde_yaml::from_str("generic").unwrap();
        assert_eq!(parsed, WatcherType::Generic);
    }

    #[test]
    fn test_watcher_config_defaults_watcher_type_when_omitted() {
        let yaml = r#"
kafka:
  brokers: localhost:9092
  topic: events
"#;

        let config: WatcherConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.watcher_type, WatcherType::XZepr);
    }

    #[test]
    fn test_generic_match_config_roundtrip_all_fields() {
        let original = GenericMatchConfig {
            action: Some("deploy.*".to_string()),
            name: Some("service-(api|web)".to_string()),
            version: Some("^v[0-9]+\\.[0-9]+$".to_string()),
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: GenericMatchConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(restored.action, original.action);
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.version, original.version);
    }

    #[test]
    fn test_generic_match_config_roundtrip_action_only() {
        let original = GenericMatchConfig {
            action: Some("deploy-prod".to_string()),
            name: None,
            version: None,
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: GenericMatchConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(restored.action, original.action);
        assert!(restored.name.is_none());
        assert!(restored.version.is_none());
    }

    #[test]
    fn test_generic_match_config_roundtrip_name_and_version_only() {
        let original = GenericMatchConfig {
            action: None,
            name: Some("service-a".to_string()),
            version: Some("1\\.2\\.3".to_string()),
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: GenericMatchConfig = serde_yaml::from_str(&yaml).unwrap();

        assert!(restored.action.is_none());
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.version, original.version);
    }

    #[test]
    fn test_generic_match_config_roundtrip_none_fields() {
        let original = GenericMatchConfig::default();

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: GenericMatchConfig = serde_yaml::from_str(&yaml).unwrap();

        assert!(restored.action.is_none());
        assert!(restored.name.is_none());
        assert!(restored.version.is_none());
    }

    #[test]
    fn test_kafka_watcher_config_roundtrip_output_topic() {
        let original = KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.in".to_string(),
            output_topic: Some("plans.out".to_string()),
            group_id: "watchers".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: KafkaWatcherConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(restored.brokers, "localhost:9092");
        assert_eq!(restored.topic, "plans.in");
        assert_eq!(restored.output_topic.as_deref(), Some("plans.out"));
        assert_eq!(restored.group_id, "watchers");
        assert!(restored.auto_create_topics);
    }

    #[test]
    fn test_kafka_watcher_config_roundtrip_auto_create_topics_false() {
        let original = KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.in".to_string(),
            output_topic: None,
            group_id: "watchers".to_string(),
            auto_create_topics: false,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        };

        let yaml = serde_yaml::to_string(&original).unwrap();
        let restored: KafkaWatcherConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(restored.brokers, "localhost:9092");
        assert_eq!(restored.topic, "plans.in");
        assert_eq!(restored.output_topic, None);
        assert_eq!(restored.group_id, "watchers");
        assert!(!restored.auto_create_topics);
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
    fn test_ollama_config_request_timeout_default() {
        let config = OllamaConfig::default();
        assert_eq!(config.request_timeout_seconds, 600);
    }

    #[test]
    fn test_ollama_config_deserialize_request_timeout() {
        let yaml =
            "host: http://localhost:11434\nmodel: llama3.2:latest\nrequest_timeout_seconds: 300\n";
        let config: OllamaConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.request_timeout_seconds, 300);
    }

    #[test]
    fn test_ollama_config_deserialize_omits_timeout_uses_default() {
        let yaml = "model: llama3.2:latest\n";
        let config: OllamaConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.request_timeout_seconds, 600);
    }

    #[test]
    fn test_apply_env_vars_overrides_ollama_request_timeout() {
        let _timeout = EnvVarGuard::set("XZATOMA_OLLAMA_REQUEST_TIMEOUT", "300");
        let mut config = Config::default();
        config.apply_env_vars();
        assert_eq!(config.provider.ollama.request_timeout_seconds, 300);
    }

    #[test]
    fn test_openai_config_defaults() {
        let config = OpenAIConfig::default();
        assert_eq!(config.api_key, "");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-4o-mini");
        assert!(config.organization_id.is_none());
        assert!(config.enable_streaming);
    }

    #[test]
    fn test_openai_config_deserialize_all_fields() {
        let yaml = r#"
api_key: "sk-test-key"
base_url: "http://localhost:8080/v1"
model: "gpt-4o"
organization_id: "org-abc123"
enable_streaming: false
"#;
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.api_key, "sk-test-key");
        assert_eq!(config.base_url, "http://localhost:8080/v1");
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.organization_id, Some("org-abc123".to_string()));
        assert!(!config.enable_streaming);
    }

    #[test]
    fn test_openai_config_deserialize_omitted_fields_use_defaults() {
        let yaml = r#"
model: gpt-4o
"#;
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_key, "");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert!(config.organization_id.is_none());
        assert!(config.enable_streaming);
    }

    #[test]
    fn test_openai_config_deserialize_host_alias_maps_to_base_url() {
        // Users coming from the Ollama config convention may write `host:`
        // instead of `base_url:`. The serde alias ensures the key is accepted
        // and mapped to the correct field rather than silently ignored.
        let yaml = r#"
host: "http://127.0.0.1:8000/v1"
model: ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M
"#;
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(
            config.base_url, "http://127.0.0.1:8000/v1",
            "host: alias must map to base_url"
        );
        assert_eq!(config.model, "ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M");
        assert_eq!(config.api_key, "");
    }

    #[test]
    fn test_openai_config_deserialize_host_alias_in_full_config() {
        // Verify the alias works when embedded inside a full Config document,
        // matching the shape of demos/chat/config-llamacpp.yaml before the
        // field name was corrected.
        let yaml = r#"
provider:
  type: openai
  openai:
    host: "http://127.0.0.1:8000/v1"
    model: ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M
agent: {}
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.provider.provider_type, "openai");
        assert_eq!(
            config.provider.openai.base_url, "http://127.0.0.1:8000/v1",
            "host: alias must map to base_url inside a full Config document"
        );
        assert_eq!(
            config.provider.openai.model,
            "ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M"
        );
    }

    #[test]
    fn test_openai_config_request_timeout_default() {
        let config = OpenAIConfig::default();
        assert_eq!(config.request_timeout_seconds, 600);
    }

    #[test]
    fn test_openai_config_deserialize_request_timeout() {
        let yaml = "base_url: http://localhost:8080/v1\nrequest_timeout_seconds: 300\n";
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.request_timeout_seconds, 300);
    }

    #[test]
    fn test_openai_config_deserialize_omits_timeout_uses_default() {
        let yaml = "model: gpt-4o\n";
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(config.request_timeout_seconds, 600);
    }

    #[test]
    fn test_apply_env_vars_overrides_openai_request_timeout() {
        let _timeout = EnvVarGuard::set("XZATOMA_OPENAI_REQUEST_TIMEOUT", "300");
        let mut config = Config::default();
        config.apply_env_vars();
        assert_eq!(config.provider.openai.request_timeout_seconds, 300);
    }

    #[test]
    fn test_openai_config_reasoning_effort_defaults_none() {
        let config = OpenAIConfig::default();
        assert!(
            config.reasoning_effort.is_none(),
            "reasoning_effort must default to None"
        );
    }

    #[test]
    fn test_openai_config_deserialize_reasoning_effort() {
        let yaml = "model: o3\nreasoning_effort: high\n";
        let config: OpenAIConfig = serde_yaml::from_str(yaml).expect("deserialize failed");
        assert_eq!(
            config.reasoning_effort,
            Some("high".to_string()),
            "reasoning_effort must deserialize from YAML"
        );
    }

    #[test]
    #[serial]
    fn test_apply_env_vars_overrides_openai_reasoning_effort() {
        let _effort = EnvVarGuard::set("XZATOMA_OPENAI_REASONING_EFFORT", "medium");
        let mut config = Config::default();
        config.apply_env_vars();
        assert_eq!(
            config.provider.openai.reasoning_effort,
            Some("medium".to_string()),
            "XZATOMA_OPENAI_REASONING_EFFORT must set reasoning_effort"
        );
    }

    #[test]
    #[serial]
    fn test_apply_env_vars_openai_reasoning_effort_none_clears_field() {
        let mut config = Config::default();
        config.provider.openai.reasoning_effort = Some("high".to_string());
        let _effort = EnvVarGuard::set("XZATOMA_OPENAI_REASONING_EFFORT", "none");
        config.apply_env_vars();
        assert!(
            config.provider.openai.reasoning_effort.is_none(),
            "XZATOMA_OPENAI_REASONING_EFFORT=none must clear the field"
        );
    }

    #[test]
    fn test_apply_env_vars_overrides_openai_fields() {
        let _api_key = EnvVarGuard::set("XZATOMA_OPENAI_API_KEY", "sk-my-key");
        let _base_url = EnvVarGuard::set("XZATOMA_OPENAI_BASE_URL", "http://localhost:8080/v1");
        let _model = EnvVarGuard::set("XZATOMA_OPENAI_MODEL", "gpt-4o");
        let _org_id = EnvVarGuard::set("XZATOMA_OPENAI_ORG_ID", "org-abc123");
        let _streaming = EnvVarGuard::set("XZATOMA_OPENAI_STREAMING", "false");

        let mut config = Config::default();
        config.apply_env_vars();

        assert_eq!(config.provider.openai.api_key, "sk-my-key");
        assert_eq!(config.provider.openai.base_url, "http://localhost:8080/v1");
        assert_eq!(config.provider.openai.model, "gpt-4o");
        assert_eq!(
            config.provider.openai.organization_id,
            Some("org-abc123".to_string())
        );
        assert!(!config.provider.openai.enable_streaming);
    }

    #[test]
    fn test_config_validation_accepts_openai_provider() {
        let mut config = Config::default();
        config.provider.provider_type = "openai".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_accepts_openai_subagent_override() {
        let mut config = Config::default();
        config.agent.subagent.provider = Some("openai".to_string());
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
    fn test_openai_config_example_parses() {
        // Ensure the OpenAI example configuration file is valid YAML and maps
        // to `Config` without errors. This verifies the file is self-consistent
        // and that all fields deserialize correctly with their documented defaults.
        let contents = std::fs::read_to_string("config/openai_config.yaml")
            .expect("Failed to read config/openai_config.yaml");
        let cfg: Config =
            serde_yaml::from_str(&contents).expect("Failed to parse config/openai_config.yaml");

        // Provider section
        assert_eq!(cfg.provider.provider_type, "openai");
        assert_eq!(cfg.provider.openai.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.provider.openai.model, "gpt-4o-mini");
        assert_eq!(cfg.provider.openai.api_key, "");
        assert!(cfg.provider.openai.organization_id.is_none());
        assert!(cfg.provider.openai.enable_streaming);

        // Agent section
        assert_eq!(cfg.agent.max_turns, 50);
        assert_eq!(cfg.agent.timeout_seconds, 300);
        assert_eq!(cfg.agent.conversation.max_tokens, 100000);
        assert_eq!(cfg.agent.conversation.prune_threshold, 0.8);
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

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_watcher_type() {
        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_TYPE");
        }

        std::env::set_var("XZATOMA_WATCHER_TYPE", "generic");

        let mut cfg = Config::default();
        cfg.apply_env_vars();

        assert_eq!(cfg.watcher.watcher_type, WatcherType::Generic);

        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_TYPE");
        }
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_watcher_output_topic() {
        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_OUTPUT_TOPIC");
        }

        let mut cfg = Config::default();
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "test-group".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });

        std::env::set_var("XZATOMA_WATCHER_OUTPUT_TOPIC", "plans.output");
        cfg.apply_env_vars();

        assert_eq!(
            cfg.watcher.kafka.as_ref().unwrap().output_topic.as_deref(),
            Some("plans.output")
        );

        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_OUTPUT_TOPIC");
        }
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_watcher_group_id() {
        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_GROUP_ID");
        }

        let mut cfg = Config::default();
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "original-group".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });

        std::env::set_var("XZATOMA_WATCHER_GROUP_ID", "override-group");
        cfg.apply_env_vars();

        assert_eq!(
            cfg.watcher.kafka.as_ref().unwrap().group_id,
            "override-group"
        );

        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_GROUP_ID");
        }
    }

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_apply_env_vars_overrides_generic_match_fields() {
        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_MATCH_ACTION");
            std::env::remove_var("XZATOMA_WATCHER_MATCH_NAME");
            std::env::remove_var("XZATOMA_WATCHER_MATCH_VERSION");
        }

        std::env::set_var("XZATOMA_WATCHER_MATCH_ACTION", "deploy.*");
        std::env::set_var("XZATOMA_WATCHER_MATCH_NAME", "service-a");
        std::env::set_var("XZATOMA_WATCHER_MATCH_VERSION", "^v1$");

        let mut cfg = Config::default();
        cfg.apply_env_vars();

        assert_eq!(
            cfg.watcher.generic_match.action.as_deref(),
            Some("deploy.*")
        );
        assert_eq!(cfg.watcher.generic_match.name.as_deref(), Some("service-a"));
        assert_eq!(cfg.watcher.generic_match.version.as_deref(), Some("^v1$"));

        unsafe {
            std::env::remove_var("XZATOMA_WATCHER_MATCH_ACTION");
            std::env::remove_var("XZATOMA_WATCHER_MATCH_NAME");
            std::env::remove_var("XZATOMA_WATCHER_MATCH_VERSION");
        }
    }

    #[test]
    fn test_config_validate_generic_accept_all_is_ok() {
        let mut cfg = Config::default();
        cfg.watcher.watcher_type = WatcherType::Generic;
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "test-group".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });

        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_config_validate_generic_valid_regex_patterns_is_ok() {
        let mut cfg = Config::default();
        cfg.watcher.watcher_type = WatcherType::Generic;
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: Some("plans.output".to_string()),
            group_id: "test-group".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });
        cfg.watcher.generic_match = GenericMatchConfig {
            action: Some("deploy.*".to_string()),
            name: Some("service-(api|web)".to_string()),
            version: Some("^v[0-9]+$".to_string()),
        };

        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_config_validate_generic_invalid_regex_returns_error() {
        let mut cfg = Config::default();
        cfg.watcher.watcher_type = WatcherType::Generic;
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "test-group".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });
        cfg.watcher.generic_match.action = Some("[broken".to_string());

        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("Invalid generic match regex"));
    }

    #[test]
    fn test_config_validate_empty_group_id_returns_error() {
        let mut cfg = Config::default();
        cfg.watcher.kafka = Some(KafkaWatcherConfig {
            brokers: "localhost:9092".to_string(),
            topic: "plans.input".to_string(),
            output_topic: None,
            group_id: "   ".to_string(),
            auto_create_topics: true,
            num_partitions: 1,
            replication_factor: 1,
            security: None,
        });

        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("watcher.kafka.group_id cannot be empty"));
    }

    #[test]
    fn test_config_validate_generic_missing_kafka_returns_error() {
        let mut cfg = Config::default();
        cfg.watcher.watcher_type = WatcherType::Generic;
        cfg.watcher.kafka = None;

        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("watcher.kafka is required"));
    }

    // Subagent configuration tests

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

/// Watcher backend type.
///
/// This selects which watcher implementation should process Kafka messages.
/// `XZepr` remains the default for backward compatibility with existing config
/// files that omit the field.
///
/// # Examples
///
/// ```
/// use xzatoma::config::WatcherType;
///
/// let watcher_type = WatcherType::default();
/// assert_eq!(watcher_type, WatcherType::XZepr);
/// assert_eq!(watcher_type.as_str(), "xzepr");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WatcherType {
    /// Existing XZepr CloudEvents watcher.
    #[default]
    XZepr,
    /// Generic Kafka plan-event watcher.
    Generic,
}

impl WatcherType {
    /// Return the configuration string representation for this watcher type.
    ///
    /// # Returns
    ///
    /// Returns `"xzepr"` or `"generic"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::WatcherType;
    ///
    /// assert_eq!(WatcherType::XZepr.as_str(), "xzepr");
    /// assert_eq!(WatcherType::Generic.as_str(), "generic");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::XZepr => "xzepr",
            Self::Generic => "generic",
        }
    }

    /// Parse a watcher type from a user-provided string.
    ///
    /// Matching is case-insensitive and ignores leading/trailing whitespace.
    ///
    /// # Arguments
    ///
    /// * `value` - String to parse
    ///
    /// # Returns
    ///
    /// Returns `Some(WatcherType)` for recognized values, otherwise `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::WatcherType;
    ///
    /// assert_eq!(WatcherType::from_str_name("xzepr"), Some(WatcherType::XZepr));
    /// assert_eq!(WatcherType::from_str_name("GENERIC"), Some(WatcherType::Generic));
    /// assert_eq!(WatcherType::from_str_name("other"), None);
    /// ```
    pub fn from_str_name(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "xzepr" => Some(Self::XZepr),
            "generic" => Some(Self::Generic),
            _ => None,
        }
    }
}

/// Watcher configuration for Kafka event monitoring
///
/// Configures the watcher service for monitoring Kafka topics, filtering events,
/// matching generic watcher events, logging, and executing plans extracted from
/// events.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherConfig {
    /// Active watcher backend type.
    #[serde(default)]
    pub watcher_type: WatcherType,

    /// Kafka consumer configuration
    #[serde(default)]
    pub kafka: Option<KafkaWatcherConfig>,

    /// Generic watcher match configuration
    #[serde(default)]
    pub generic_match: GenericMatchConfig,

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

/// Kafka consumer configuration for the watcher.
///
/// The generic watcher also uses this structure for its result producer.
/// When `output_topic` is `None` and `watcher_type` is `generic`, results are
/// published back to the input `topic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaWatcherConfig {
    /// Kafka brokers (comma-separated)
    pub brokers: String,

    /// Topic to consume from
    pub topic: String,

    /// Output topic for publishing plan execution results (Generic watcher only).
    ///
    /// If `None`, results are published back to the input `topic`.
    #[serde(default)]
    pub output_topic: Option<String>,

    /// Consumer group ID
    #[serde(default = "default_watcher_group_id")]
    pub group_id: String,

    /// Automatically create input/output topics when watcher mode starts.
    ///
    /// When enabled, the watcher uses the Kafka AdminClient to create missing
    /// topics before entering the consume loop.
    #[serde(default = "default_auto_create_topics")]
    pub auto_create_topics: bool,

    /// Number of partitions for auto-created topics.
    ///
    /// Only used when `auto_create_topics` is `true`. Defaults to `1`.
    #[serde(default = "default_num_partitions")]
    pub num_partitions: i32,

    /// Replication factor for auto-created topics.
    ///
    /// Only used when `auto_create_topics` is `true`. Defaults to `1`.
    /// For production deployments with multiple brokers, set this to `3`.
    #[serde(default = "default_replication_factor")]
    pub replication_factor: i32,

    /// Security configuration
    #[serde(default)]
    pub security: Option<KafkaSecurityConfig>,
}

/// Default number of partitions for auto-created Kafka topics.
fn default_num_partitions() -> i32 {
    1
}

/// Default replication factor for auto-created Kafka topics.
fn default_replication_factor() -> i32 {
    1
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

/// Generic watcher match configuration.
///
/// `GenericMatchConfig` and `EventFilterConfig` are intentionally separate:
///
/// - `GenericMatchConfig` is used only by the generic watcher backend
/// - `EventFilterConfig` is used only by the XZepr watcher backend
///
/// All configured fields are interpreted as regular expressions by the generic
/// matcher and are matched case-insensitively by default.
///
/// # Examples
///
/// ```
/// use xzatoma::config::GenericMatchConfig;
///
/// let cfg = GenericMatchConfig {
///     action: Some("deploy.*".to_string()),
///     name: Some("service-a".to_string()),
///     version: None,
/// };
///
/// assert_eq!(cfg.action.as_deref(), Some("deploy.*"));
/// assert_eq!(cfg.name.as_deref(), Some("service-a"));
/// assert!(cfg.version.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenericMatchConfig {
    /// Regex pattern matched against the event action field.
    #[serde(default)]
    pub action: Option<String>,

    /// Regex pattern matched against the event name field.
    #[serde(default)]
    pub name: Option<String>,

    /// Regex pattern matched against the event version field.
    #[serde(default)]
    pub version: Option<String>,
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

/// Default topic auto-creation setting
fn default_auto_create_topics() -> bool {
    true
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
