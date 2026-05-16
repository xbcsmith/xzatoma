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
/// # Arguments
///
/// * `provider` - Optional provider override such as `copilot`, `ollama`, or `openai`.
/// * `model` - Optional model override for the selected provider.
/// * `allow_dangerous` - Whether to allow dangerous terminal commands without confirmation.
/// * `working_dir` - Optional fallback workspace root when the ACP client omits one.
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
/// handle_agent(None, None, false, None, Config::default()).await?;
/// # Ok(())
/// # }
/// ```
pub async fn handle_agent(
    provider: Option<String>,
    model: Option<String>,
    allow_dangerous: bool,
    working_dir: Option<PathBuf>,
    config: Config,
) -> Result<()> {
    let options = AcpStdioAgentOptions::new(provider, model, allow_dangerous, working_dir);
    run_stdio_agent(config, options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_agent_accepts_default_config() {
        let result = handle_agent(None, None, false, None, Config::default()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_provider_and_model_overrides() {
        let result = handle_agent(
            Some("ollama".to_string()),
            Some("llama3.2:latest".to_string()),
            false,
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
            Config::default(),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_agent_accepts_allow_dangerous() {
        let result = handle_agent(None, None, true, None, Config::default()).await;

        assert!(result.is_ok());
    }
}
