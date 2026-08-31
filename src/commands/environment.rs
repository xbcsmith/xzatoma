//! Agent environment builder for command handlers.
//!
//! This module provides [`AgentEnvironment`] and [`build_agent_environment`],
//! the single canonical factory for setting up the tool/skill/MCP stack that
//! every command handler and the ACP executor requires before constructing an
//! [`crate::agent::Agent`].
//!
//! Centralising this logic eliminates the identical 50-line initialization
//! sequences that previously existed in `commands::chat::run_chat`,
//! `commands::run::run_plan_with_options`, and `acp::executor::build_tools`.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::chat_mode::{ChatMode, SafetyMode};
use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::mcp::manager::{McpClientManager, build_mcp_manager_from_config};
use crate::mcp::tool_bridge::register_mcp_tools;
use crate::skills::ActiveSkillRegistry;
use crate::tools::ToolRegistry;
use crate::tools::registry_builder::ToolRegistryBuilder;

// ---------------------------------------------------------------------------
// AgentEnvironment
// ---------------------------------------------------------------------------

/// Bundled result of the shared agent tool and MCP initialization sequence.
///
/// [`build_agent_environment`] constructs this struct from a [`Config`] so
/// that the identical initialization code does not have to be repeated in
/// every command handler.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::commands::build_agent_environment;
/// use xzatoma::config::Config;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = Config::default();
/// let env = build_agent_environment(&config, Path::new("."), true, None, None).await?;
/// // env.tool_registry, env.mcp_manager, etc. are ready to use.
/// # Ok(())
/// # }
/// ```
pub struct AgentEnvironment {
    /// Fully initialized tool registry with skills and MCP tools registered.
    pub tool_registry: ToolRegistry,
    /// Live MCP client manager shared across async tasks, or `None` when MCP
    /// auto-connect is disabled or no servers are configured.
    pub mcp_manager: Option<Arc<RwLock<McpClientManager>>>,
    /// Chat mode derived from `config.agent.chat.default_mode`.
    pub chat_mode: ChatMode,
    /// Safety mode derived from `config.agent.chat.default_safety`.
    pub safety_mode: SafetyMode,
    /// Shared active-skill registry for the session.
    pub active_skill_registry: Arc<std::sync::Mutex<ActiveSkillRegistry>>,
    /// Optional startup skill disclosure text for injection into the agent
    /// system prompt before the first turn.
    pub skill_disclosure: Option<String>,
}

// ---------------------------------------------------------------------------
// build_agent_environment
// ---------------------------------------------------------------------------

/// Build a fully initialized [`AgentEnvironment`] from configuration.
///
/// This is the single canonical factory for setting up the tool/skill/MCP
/// stack used by command handlers and the ACP executor. It performs:
///
/// 1. Parse [`ChatMode`] and [`SafetyMode`] from `config.agent.chat`.
/// 2. Apply `override_mode` when `Some`, replacing the config-derived mode.
/// 3. Build startup skill disclosure text.
/// 4. Build the visible skill catalog and create an [`ActiveSkillRegistry`].
/// 5. Build a [`ToolRegistry`] via [`ToolRegistryBuilder`].
/// 6. Register the `activate_skill` tool when visible skills exist.
/// 7. Build the [`McpClientManager`] via [`build_mcp_manager_from_config`].
/// 8. Register all MCP tools into the registry.
///
/// # Arguments
///
/// * `config` - Global application configuration.
/// * `working_dir` - Current working directory used for skill discovery.
/// * `headless` - Pass `true` for non-interactive (headless) invocations such
///   as the `run` command or ACP executor; `false` for interactive chat. When
///   `true`, the MCP client manager's execution context is set to headless,
///   enabling auto-approval of MCP sampling requests.
/// * `override_mode` - When `Some(mode)`, replaces the mode derived from
///   `config.agent.chat.default_mode`. Pass `None` to use the config value.
///   Watcher callers pass `Some(ChatMode::Watcher)` to obtain a tool registry
///   with full write-equivalent access and the autonomous watcher system prompt.
/// * `override_safety_mode` - When `Some(mode)`, replaces the safety mode
///   derived from `config.agent.chat.default_safety`. Pass `None` to use the
///   config value. Watcher callers pass `Some(SafetyMode::NeverConfirm)` so
///   the tool registry allows unattended command execution.
///
/// # Returns
///
/// A fully initialized [`AgentEnvironment`] ready for agent construction.
///
/// # Errors
///
/// Returns an error if skill discovery, tool registry construction, or MCP
/// tool registration fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::commands::build_agent_environment;
/// use xzatoma::config::Config;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = Config::default();
/// let env = build_agent_environment(&config, Path::new("."), true, None, None).await?;
/// assert!(env.mcp_manager.is_none()); // no MCP servers in default config
/// # Ok(())
/// # }
/// ```
pub async fn build_agent_environment(
    config: &Config,
    working_dir: &Path,
    headless: bool,
    override_mode: Option<ChatMode>,
    override_safety_mode: Option<SafetyMode>,
) -> Result<AgentEnvironment> {
    // 1. Parse chat mode and safety mode from config.
    let config_chat_mode =
        ChatMode::parse_str(&config.agent.chat.default_mode).unwrap_or(ChatMode::Planning);
    // Apply caller-supplied overrides when present.
    let chat_mode = override_mode.unwrap_or(config_chat_mode);
    let config_safety_mode = match config.agent.chat.default_safety.to_lowercase().as_str() {
        "yolo" => SafetyMode::NeverConfirm,
        _ => SafetyMode::AlwaysConfirm,
    };
    let safety_mode = override_safety_mode.unwrap_or(config_safety_mode);

    // 2. Build startup skill disclosure text.
    let skill_disclosure = super::build_startup_skill_disclosure(config, working_dir)?;

    // 3. Build the visible skill catalog and active-skill registry.
    let visible_skill_catalog = super::build_visible_skill_catalog(config, working_dir)?;
    let active_skill_registry = Arc::new(std::sync::Mutex::new(ActiveSkillRegistry::new()));

    // 4. Build tool registry.
    let mut tool_registry =
        ToolRegistryBuilder::new(chat_mode, safety_mode, working_dir.to_path_buf())
            .with_tools_config(config.agent.tools.clone())
            .with_terminal_config(config.agent.terminal.clone())
            .build()?;

    // 5. Register activate_skill tool.
    super::register_activate_skill_tool(
        &mut tool_registry,
        config,
        visible_skill_catalog,
        Arc::clone(&active_skill_registry),
    )?;

    // 6. Build MCP client manager.
    let mcp_manager = build_mcp_manager_from_config(config).await?;

    // Set the execution context on the manager so that sampling and
    // elicitation handlers receive the correct headless flag for this
    // invocation (headless=true for run/watch, false for interactive chat).
    if let Some(ref manager) = mcp_manager {
        let execution_mode = config.agent.terminal.default_mode;
        manager
            .write()
            .await
            .set_execution_context(execution_mode, headless);
    }

    // 7. Register MCP tools.
    if let Some(ref manager) = mcp_manager {
        let execution_mode = config.agent.terminal.default_mode;
        let count = register_mcp_tools(
            &mut tool_registry,
            Arc::clone(manager),
            execution_mode,
            headless,
        )
        .await
        .map_err(|e| XzatomaError::Config(format!("Failed to register MCP tools: {}", e)))?;
        if count > 0 {
            tracing::info!(count = count, "Registered MCP tools for agent environment");
        }
    }

    Ok(AgentEnvironment {
        tool_registry,
        mcp_manager,
        chat_mode,
        safety_mode,
        active_skill_registry,
        skill_disclosure,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // -----------------------------------------------------------------------
    // build_agent_environment
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_build_agent_environment_succeeds_with_default_config() {
        let config = Config::default();
        let result =
            build_agent_environment(&config, std::path::Path::new("."), true, None, None).await;
        assert!(
            result.is_ok(),
            "build_agent_environment should succeed with default config: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_build_agent_environment_headless_true_no_mcp_manager_by_default() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        // Default config has auto_connect=false so no MCP manager.
        assert!(env.mcp_manager.is_none());
        // Default config has skills.enabled=false so no disclosure.
        assert!(env.skill_disclosure.is_none());
    }

    #[tokio::test]
    async fn test_build_agent_environment_headless_false_no_mcp_manager_by_default() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), false, None, None)
            .await
            .unwrap();
        assert!(env.mcp_manager.is_none());
    }

    #[tokio::test]
    async fn test_build_agent_environment_chat_mode_defaults_to_planning() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        assert_eq!(env.chat_mode, ChatMode::Planning);
    }

    #[tokio::test]
    async fn test_build_agent_environment_safety_mode_defaults_to_always_confirm() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        assert_eq!(env.safety_mode, SafetyMode::AlwaysConfirm);
    }

    #[tokio::test]
    async fn test_build_agent_environment_yolo_safety_mode_parses_correctly() {
        let mut config = Config::default();
        config.agent.chat.default_safety = "yolo".to_string();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        assert_eq!(env.safety_mode, SafetyMode::NeverConfirm);
    }

    #[tokio::test]
    async fn test_build_agent_environment_tool_registry_is_populated() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        // The tool registry should contain at least the standard tools.
        assert!(!env.tool_registry.all_definitions().is_empty());
    }

    #[tokio::test]
    async fn test_build_agent_environment_write_mode_parsed_from_config() {
        let mut config = Config::default();
        config.agent.chat.default_mode = "write".to_string();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        assert_eq!(env.chat_mode, ChatMode::Write);
    }

    #[tokio::test]
    async fn test_build_agent_environment_active_skill_registry_is_initialized() {
        let config = Config::default();
        let env = build_agent_environment(&config, std::path::Path::new("."), true, None, None)
            .await
            .unwrap();
        // The active skill registry should start empty but be accessible.
        let registry = env.active_skill_registry.lock().unwrap();
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn test_build_agent_environment_override_mode_watcher_produces_watcher_mode() {
        use crate::chat_mode::ChatMode;
        let config = Config::default();
        let env = build_agent_environment(
            &config,
            std::path::Path::new("."),
            true,
            Some(ChatMode::Watcher),
            None,
        )
        .await
        .unwrap();
        assert_eq!(env.chat_mode, ChatMode::Watcher);
    }
}
