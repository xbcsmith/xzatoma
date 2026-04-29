//! ACP stdio transport scaffold for Zed-compatible subprocess integration.
//!
//! This module is the transport boundary for the future `xzatoma agent` command.
//! Zed launches custom ACP agents as child processes and communicates with them
//! over stdin/stdout using newline-delimited JSON-RPC. This module intentionally
//! contains only Phase 1 scaffolding: configuration normalization and a no-op
//! server entry point that preserves stdout for the protocol stream.
//!
//! Later phases will add the `agent-client-protocol` transport, initialize and
//! session handlers, prompt queueing, cancellation, and text/vision prompt
//! conversion.
//!
//! # Examples
//!
//! ```no_run
//! use xzatoma::acp::stdio::{run_stdio_agent, AcpStdioAgentOptions};
//! use xzatoma::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let options = AcpStdioAgentOptions::new(None, None, false, None);
//! run_stdio_agent(Config::default(), options).await?;
//! # Ok(())
//! # }
//! ```
use std::path::PathBuf;

use crate::config::{Config, ExecutionMode};
use crate::error::Result;

/// Runtime options for the ACP stdio agent command.
///
/// These options are derived from `xzatoma agent` CLI flags and are applied to
/// the loaded configuration for the current subprocess only. They do not persist
/// to configuration files.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::acp::stdio::AcpStdioAgentOptions;
///
/// let options = AcpStdioAgentOptions::new(
///     Some("ollama".to_string()),
///     Some("llama3.2:latest".to_string()),
///     true,
///     Some(PathBuf::from("/tmp/workspace")),
/// );
///
/// assert_eq!(options.provider.as_deref(), Some("ollama"));
/// assert_eq!(options.model.as_deref(), Some("llama3.2:latest"));
/// assert!(options.allow_dangerous);
/// assert_eq!(options.working_dir, Some(PathBuf::from("/tmp/workspace")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpStdioAgentOptions {
    /// Optional provider override, such as `copilot`, `ollama`, or `openai`.
    pub provider: Option<String>,
    /// Optional model override for the selected provider.
    pub model: Option<String>,
    /// Whether dangerous terminal commands should run without confirmation.
    pub allow_dangerous: bool,
    /// Optional fallback workspace root when the ACP client omits one.
    pub working_dir: Option<PathBuf>,
}

impl AcpStdioAgentOptions {
    /// Creates ACP stdio agent runtime options.
    ///
    /// # Arguments
    ///
    /// * `provider` - Optional provider override.
    /// * `model` - Optional model override.
    /// * `allow_dangerous` - Whether to allow dangerous terminal commands.
    /// * `working_dir` - Optional fallback workspace root.
    ///
    /// # Returns
    ///
    /// Returns initialized runtime options for the stdio agent.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::stdio::AcpStdioAgentOptions;
    ///
    /// let options = AcpStdioAgentOptions::new(None, None, false, None);
    /// assert!(options.provider.is_none());
    /// assert!(!options.allow_dangerous);
    /// ```
    pub fn new(
        provider: Option<String>,
        model: Option<String>,
        allow_dangerous: bool,
        working_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            provider,
            model,
            allow_dangerous,
            working_dir,
        }
    }
}

/// Runs the ACP stdio agent subprocess entry point.
///
/// Phase 1 intentionally does not start the JSON-RPC transport yet. It applies
/// CLI overrides to a local configuration clone, validates the effective
/// configuration, records startup information through tracing, and returns
/// successfully without writing to stdout.
///
/// Later phases will keep this public entry point and extend it to serve the
/// actual `agent-client-protocol` connection on stdin/stdout.
///
/// # Arguments
///
/// * `config` - Loaded XZatoma configuration.
/// * `options` - CLI-derived stdio agent runtime options.
///
/// # Errors
///
/// Returns an error if the effective configuration is invalid.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::acp::stdio::{run_stdio_agent, AcpStdioAgentOptions};
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// run_stdio_agent(Config::default(), AcpStdioAgentOptions::new(None, None, false, None)).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_stdio_agent(mut config: Config, options: AcpStdioAgentOptions) -> Result<()> {
    apply_stdio_agent_options(&mut config, &options);
    config.validate()?;

    tracing::info!(
        provider = %config.provider.provider_type,
        model = %current_model_name(&config),
        allow_dangerous = options.allow_dangerous,
        working_dir = ?options.working_dir,
        "ACP stdio agent scaffold initialized"
    );

    Ok(())
}

/// Applies ACP stdio agent CLI options to a configuration clone.
///
/// Provider and model overrides are applied to the in-memory configuration used
/// by the current subprocess. When `allow_dangerous` is set, terminal execution
/// mode is escalated to full autonomous operation for this subprocess only.
///
/// # Arguments
///
/// * `config` - Mutable configuration to update.
/// * `options` - CLI-derived options to apply.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::stdio::{apply_stdio_agent_options, AcpStdioAgentOptions};
/// use xzatoma::config::{Config, ExecutionMode};
///
/// let mut config = Config::default();
/// let options = AcpStdioAgentOptions::new(
///     Some("ollama".to_string()),
///     Some("llama3.2:latest".to_string()),
///     true,
///     None,
/// );
///
/// apply_stdio_agent_options(&mut config, &options);
///
/// assert_eq!(config.provider.provider_type, "ollama");
/// assert_eq!(config.provider.ollama.model, "llama3.2:latest");
/// assert_eq!(config.agent.terminal.default_mode, ExecutionMode::FullAutonomous);
/// ```
pub fn apply_stdio_agent_options(config: &mut Config, options: &AcpStdioAgentOptions) {
    if let Some(provider) = &options.provider {
        config.provider.provider_type = provider.clone();
    }

    if let Some(model) = &options.model {
        match config.provider.provider_type.as_str() {
            "copilot" => config.provider.copilot.model = model.clone(),
            "ollama" => config.provider.ollama.model = model.clone(),
            "openai" => config.provider.openai.model = model.clone(),
            _ => {}
        }
    }

    if options.allow_dangerous {
        config.agent.terminal.default_mode = ExecutionMode::FullAutonomous;
        config.agent.chat.default_safety = "yolo".to_string();
    }
}

fn current_model_name(config: &Config) -> &str {
    match config.provider.provider_type.as_str() {
        "copilot" => &config.provider.copilot.model,
        "ollama" => &config.provider.ollama.model,
        "openai" => &config.provider.openai.model,
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_new_sets_fields() {
        let options = AcpStdioAgentOptions::new(
            Some("openai".to_string()),
            Some("gpt-4o".to_string()),
            true,
            Some(PathBuf::from("/tmp/workspace")),
        );

        assert_eq!(options.provider.as_deref(), Some("openai"));
        assert_eq!(options.model.as_deref(), Some("gpt-4o"));
        assert!(options.allow_dangerous);
        assert_eq!(options.working_dir, Some(PathBuf::from("/tmp/workspace")));
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_copilot_model() {
        let mut config = Config::default();
        config.provider.provider_type = "copilot".to_string();

        let options = AcpStdioAgentOptions::new(None, Some("gpt-4o".to_string()), false, None);

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "copilot");
        assert_eq!(config.provider.copilot.model, "gpt-4o");
        assert_eq!(
            config.agent.terminal.default_mode,
            ExecutionMode::RestrictedAutonomous
        );
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_ollama_model() {
        let mut config = Config::default();

        let options = AcpStdioAgentOptions::new(
            Some("ollama".to_string()),
            Some("llama3.2:latest".to_string()),
            false,
            None,
        );

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "ollama");
        assert_eq!(config.provider.ollama.model, "llama3.2:latest");
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_openai_model() {
        let mut config = Config::default();

        let options = AcpStdioAgentOptions::new(
            Some("openai".to_string()),
            Some("gpt-4o-mini".to_string()),
            false,
            None,
        );

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "openai");
        assert_eq!(config.provider.openai.model, "gpt-4o-mini");
    }

    #[test]
    fn test_apply_stdio_agent_options_allow_dangerous_sets_full_autonomous_and_yolo() {
        let mut config = Config::default();
        let options = AcpStdioAgentOptions::new(None, None, true, None);

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(
            config.agent.terminal.default_mode,
            ExecutionMode::FullAutonomous
        );
        assert_eq!(config.agent.chat.default_safety, "yolo");
    }

    #[tokio::test]
    async fn test_run_stdio_agent_accepts_default_config() {
        let config = Config::default();
        let options = AcpStdioAgentOptions::new(None, None, false, None);

        let result = run_stdio_agent(config, options).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_stdio_agent_rejects_invalid_provider() {
        let config = Config::default();
        let options = AcpStdioAgentOptions::new(Some("invalid".to_string()), None, false, None);

        let result = run_stdio_agent(config, options).await;

        assert!(result.is_err());
    }
}
