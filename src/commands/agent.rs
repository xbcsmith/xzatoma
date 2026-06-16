//! ACP stdio agent command handler.
//!
//! This module implements the CLI-facing `xzatoma agent` command. The command is
//! designed to be launched as a subprocess by Zed or another ACP-compatible
//! client that communicates over stdin/stdout using newline-delimited JSON-RPC.
//!
//! This handler is intentionally small: it constructs stdio-agent
//! runtime options from CLI flags and delegates all transport-specific behavior
//! to `crate::acp::stdio`. The handler must not write human-readable output to
//! stdout because stdout is reserved for the ACP protocol stream.
//!
//! # Examples
//!
//! ```no_run
//! use std::path::PathBuf;
//! use xzatoma::commands::agent::handle_agent;
//! use xzatoma::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! handle_agent(
//!     Some("ollama".to_string()),
//!     Some("llama3.2:latest".to_string()),
//!     false,
//!     Some(PathBuf::from(".")),
//!     None,
//!     Config::default(),
//! )
//! .await?;
//! # Ok(())
//! # }
//! ```
use std::path::PathBuf;

use crate::acp::stdio::{run_stdio_agent, AcpStdioAgentOptions};
use crate::config::Config;
use crate::error::Result;

/// Handles the `xzatoma agent` ACP stdio subprocess command.
///
/// This command is the Zed-facing ACP entry point. It applies no protocol logic
/// directly; instead, it packages CLI overrides into [`AcpStdioAgentOptions`]
/// and delegates to [`run_stdio_agent`].
///
/// The `system_prompt` CLI flag is resolved against any value already present in
/// `config.agent.system_prompt` (e.g. from the `XZATOMA_SYSTEM_PROMPT` env var)
/// using the standard precedence rule: CLI flag wins. The resolved prompt is
/// written back into `config.agent.system_prompt` before `run_stdio_agent` is
/// called so that the ACP stdio session creation path can read it.
///
/// # Arguments
///
/// * `provider` - Optional provider override such as `copilot`, `ollama`, or `openai`.
/// * `model` - Optional model override for the selected provider.
/// * `allow_dangerous` - Whether to allow dangerous terminal commands without confirmation.
/// * `working_dir` - Optional fallback workspace root when the ACP client omits one.
/// * `system_prompt` - Optional system prompt CLI flag override for the agent session.
/// * `config` - Loaded XZatoma configuration.
///
/// # Errors
///
/// Returns an error if the effective ACP stdio agent configuration is invalid
/// or if the stdio agent runtime fails.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::commands::agent::handle_agent;
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// handle_agent(None, None, false, None, None, Config::default()).await?;
/// # Ok(())
/// # }
/// ```
pub async fn handle_agent(
    provider: Option<String>,
    model: Option<String>,
    allow_dangerous: bool,
    working_dir: Option<PathBuf>,
    system_prompt: Option<String>,
    mut config: Config,
) -> Result<()> {
    // Resolve the CLI flag against any config/env value already in config.
    // CLI flag takes precedence over config.agent.system_prompt.
    let resolved = crate::agent::resolve(
        None,
        system_prompt.as_deref(),
        config.agent.system_prompt.as_deref(),
    );
    if let Some(ref r) = resolved {
        tracing::debug!(
            source = ?r.source,
            length = r.text.len(),
            "agent command system_prompt resolved"
        );
        if tracing::enabled!(tracing::Level::TRACE) {
            tracing::trace!(
                source = ?r.source,
                system_prompt = %r.text,
                "agent session system prompt"
            );
        }
        config.agent.system_prompt = Some(r.text.clone());
    }
    let options = AcpStdioAgentOptions::new(provider, model, allow_dangerous, working_dir);
    run_stdio_agent(config, options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_agent_accepts_default_config() {
        let result = handle_agent(None, None, false, None, None, Config::default()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_provider_and_model_overrides() {
        let result = handle_agent(
            Some("ollama".to_string()),
            Some("llama3.2:latest".to_string()),
            false,
            None,
            None,
            Config::default(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_rejects_invalid_provider_override() {
        let result = handle_agent(
            Some("invalid".to_string()),
            None,
            false,
            None,
            None,
            Config::default(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_working_dir_override() {
        let result = handle_agent(
            None,
            None,
            false,
            Some(PathBuf::from("/tmp/xzatoma-zed-workspace")),
            None,
            Config::default(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_allow_dangerous() {
        let result = handle_agent(None, None, true, None, None, Config::default()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_system_prompt_override() {
        let result = handle_agent(
            None,
            None,
            false,
            None,
            Some("You are a helpful assistant.".to_string()),
            Config::default(),
        )
        .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_agent_cli_system_prompt_wins_over_config() {
        // Verify resolver precedence: CLI flag beats config value.
        let resolved = crate::agent::resolve(None, Some("from cli"), Some("from config")).unwrap();
        assert_eq!(resolved.text, "from cli");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::CliFlag);
    }

    #[test]
    fn test_handle_agent_config_system_prompt_used_when_no_cli_flag() {
        // Verify resolver precedence: config value is used when CLI is absent.
        let resolved = crate::agent::resolve(None, None, Some("from config")).unwrap();
        assert_eq!(resolved.text, "from config");
        assert_eq!(resolved.source, crate::agent::SystemPromptSource::Config);
    }

    #[test]
    fn test_handle_agent_no_system_prompt_resolves_to_none() {
        assert!(crate::agent::resolve(None, None, None).is_none());
    }
}
