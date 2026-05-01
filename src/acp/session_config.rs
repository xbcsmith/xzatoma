//! XZatoma ACP session configuration option definitions and change handlers.
//!
//! This module defines the session configuration options that XZatoma advertises
//! to Zed via the Agent-Client Protocol. Each option maps to a runtime setting
//! that can be toggled from the Zed UI without restarting the agent subprocess.
//!
//! # Option Overview
//!
//! | Config ID              | Values                                              |
//! |------------------------|-----------------------------------------------------|
//! | `safety_policy`        | always_confirm, confirm_dangerous, never_confirm    |
//! | `terminal_execution`   | interactive, restricted_autonomous, full_autonomous |
//! | `tool_routing`         | prefer_ide, prefer_local, require_ide               |
//! | `vision_input`         | enabled, disabled                                   |
//! | `subagent_delegation`  | enabled, disabled                                   |
//! | `mcp_tools`            | enabled, disabled                                   |
//! | `max_turns`            | 10, 25, 50, 100, 200                                |
//!
//! # Examples
//!
//! ```
//! use xzatoma::acp::session_config::{
//!     build_session_config_options, SessionRuntimeState, CONFIG_SAFETY_POLICY,
//! };
//! use xzatoma::Config;
//!
//! let config = Config::default();
//! let runtime = SessionRuntimeState::from_config(&config);
//! let options = build_session_config_options(&runtime);
//! assert!(!options.is_empty());
//! ```

use acp_sdk::schema as acp;
use agent_client_protocol as acp_sdk;

use crate::config::{Config, ExecutionMode};
use crate::error::{Result, XzatomaError};

// ---------------------------------------------------------------------------
// Config option ID constants
// ---------------------------------------------------------------------------

/// Config option ID for the safety confirmation policy.
pub const CONFIG_SAFETY_POLICY: &str = "safety_policy";

/// Config option ID for terminal execution mode.
pub const CONFIG_TERMINAL_EXECUTION: &str = "terminal_execution";

/// Config option ID for tool routing preference.
pub const CONFIG_TOOL_ROUTING: &str = "tool_routing";

/// Config option ID for vision input support.
pub const CONFIG_VISION_INPUT: &str = "vision_input";

/// Config option ID for subagent delegation support.
pub const CONFIG_SUBAGENT_DELEGATION: &str = "subagent_delegation";

/// Config option ID for MCP tool availability.
pub const CONFIG_MCP_TOOLS: &str = "mcp_tools";

/// Config option ID for the maximum number of agent turns per run.
pub const CONFIG_MAX_TURNS: &str = "max_turns";

// ---------------------------------------------------------------------------
// ToolRouting
// ---------------------------------------------------------------------------

/// Routing preference for tool operations.
///
/// Controls whether XZatoma prefers to execute tools via the IDE integration,
/// via the local process, or requires the IDE for all tool calls.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_config::ToolRouting;
///
/// let routing = ToolRouting::from_value_id("prefer_ide").unwrap();
/// assert_eq!(routing, ToolRouting::PreferIde);
/// assert_eq!(routing.as_value_id(), "prefer_ide");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRouting {
    /// Prefer to delegate tool calls to the IDE when available.
    PreferIde,
    /// Prefer to execute tool calls locally inside the agent process.
    PreferLocal,
    /// Require the IDE for all tool calls; fail if the IDE is unavailable.
    RequireIde,
}

impl ToolRouting {
    /// Returns the stable value ID string for this routing preference.
    ///
    /// The returned string is the canonical value identifier used in ACP
    /// [`acp::SessionConfigOption`] select payloads.
    ///
    /// # Returns
    ///
    /// One of `"prefer_ide"`, `"prefer_local"`, or `"require_ide"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session_config::ToolRouting;
    ///
    /// assert_eq!(ToolRouting::PreferIde.as_value_id(), "prefer_ide");
    /// assert_eq!(ToolRouting::PreferLocal.as_value_id(), "prefer_local");
    /// assert_eq!(ToolRouting::RequireIde.as_value_id(), "require_ide");
    /// ```
    pub fn as_value_id(&self) -> &'static str {
        match self {
            Self::PreferIde => "prefer_ide",
            Self::PreferLocal => "prefer_local",
            Self::RequireIde => "require_ide",
        }
    }

    /// Parses a [`ToolRouting`] from a stable value ID string.
    ///
    /// # Arguments
    ///
    /// * `s` - One of `"prefer_ide"`, `"prefer_local"`, or `"require_ide"`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ToolRouting)` when the string is a known value ID.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Config`] when `s` is not a recognised value.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session_config::ToolRouting;
    ///
    /// assert!(ToolRouting::from_value_id("prefer_local").is_ok());
    /// assert!(ToolRouting::from_value_id("unknown").is_err());
    /// ```
    pub fn from_value_id(s: &str) -> Result<Self> {
        match s {
            "prefer_ide" => Ok(Self::PreferIde),
            "prefer_local" => Ok(Self::PreferLocal),
            "require_ide" => Ok(Self::RequireIde),
            other => Err(XzatomaError::Config(format!(
                "unknown tool_routing value: '{other}'; \
                 expected one of: 'prefer_ide', 'prefer_local', 'require_ide'"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfigChangeEffect
// ---------------------------------------------------------------------------

/// The runtime state changes that applying a config option may produce.
///
/// After [`apply_config_option_change`] validates and processes a
/// `session/set_config_option` request, the caller receives a
/// `ConfigChangeEffect` describing which fields need to be updated in the
/// running agent. Fields that are `None` were not affected by the change.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_config::{
///     apply_config_option_change, SessionRuntimeState, CONFIG_SAFETY_POLICY,
/// };
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let runtime = SessionRuntimeState::from_config(&config);
/// let (effect, _options) =
///     apply_config_option_change(CONFIG_SAFETY_POLICY, "never_confirm", &runtime).unwrap();
/// assert_eq!(effect.safety_mode_str.as_deref(), Some("yolo"));
/// ```
#[derive(Debug, Clone)]
pub struct ConfigChangeEffect {
    /// New safety mode identifier if the safety policy was changed.
    ///
    /// The string is accepted by [`crate::chat_mode::SafetyMode::parse_str`].
    pub safety_mode_str: Option<String>,
    /// New terminal execution mode if the terminal policy was changed.
    pub terminal_mode: Option<ExecutionMode>,
    /// New tool routing preference if the routing setting was changed.
    pub tool_routing: Option<ToolRouting>,
    /// New vision input enabled flag if the setting was changed.
    pub vision_enabled: Option<bool>,
    /// New subagent delegation enabled flag if the setting was changed.
    pub subagents_enabled: Option<bool>,
    /// New MCP tools enabled flag if the setting was changed.
    pub mcp_enabled: Option<bool>,
    /// New maximum turns value if the setting was changed.
    pub max_turns: Option<u32>,
}

impl ConfigChangeEffect {
    fn none() -> Self {
        Self {
            safety_mode_str: None,
            terminal_mode: None,
            tool_routing: None,
            vision_enabled: None,
            subagents_enabled: None,
            mcp_enabled: None,
            max_turns: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionRuntimeState
// ---------------------------------------------------------------------------

/// Current runtime values used to generate session config option payloads.
///
/// This struct captures the active configuration state of a running XZatoma
/// session so that [`build_session_config_options`] can mark the correct
/// value as selected in each option's drop-down.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_config::{SessionRuntimeState, ToolRouting};
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let state = SessionRuntimeState::from_config(&config);
/// assert_eq!(state.tool_routing, ToolRouting::PreferIde);
/// assert!(state.vision_enabled);
/// ```
#[derive(Debug, Clone)]
pub struct SessionRuntimeState {
    /// Active safety mode string, as accepted by [`crate::chat_mode::SafetyMode::parse_str`].
    pub safety_mode_str: String,
    /// Active terminal execution mode.
    pub terminal_mode: ExecutionMode,
    /// Active tool routing preference.
    pub tool_routing: ToolRouting,
    /// Whether vision (image) input is enabled.
    pub vision_enabled: bool,
    /// Whether subagent delegation is enabled.
    pub subagents_enabled: bool,
    /// Whether MCP tools are enabled.
    pub mcp_enabled: bool,
    /// Maximum number of agent turns per run.
    pub max_turns: u32,
}

impl SessionRuntimeState {
    /// Builds a [`SessionRuntimeState`] from the loaded [`Config`] defaults.
    ///
    /// This is the canonical way to produce the initial runtime state for a
    /// new session. Live state changes are then tracked by the caller and
    /// reflected back to Zed via [`build_session_config_options`].
    ///
    /// # Arguments
    ///
    /// * `config` - The fully-loaded and validated XZatoma configuration.
    ///
    /// # Returns
    ///
    /// A [`SessionRuntimeState`] derived from the configuration defaults.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::session_config::SessionRuntimeState;
    /// use xzatoma::Config;
    ///
    /// let state = SessionRuntimeState::from_config(&Config::default());
    /// assert!(!state.safety_mode_str.is_empty());
    /// ```
    pub fn from_config(config: &Config) -> Self {
        let safety_mode_str = config.agent.chat.default_safety.clone();
        let terminal_mode = config.agent.terminal.default_mode;
        let vision_enabled = config.acp.stdio.vision_enabled;
        let subagents_enabled = config.agent.subagent.chat_enabled;
        // Use auto_connect as the top-level "MCP is active" signal; individual
        // servers also carry their own enabled flag, but auto_connect governs
        // whether the manager attempts connections at session start.
        let mcp_enabled = config.mcp.auto_connect;
        let max_turns = config.agent.max_turns as u32;

        Self {
            safety_mode_str,
            terminal_mode,
            tool_routing: ToolRouting::PreferIde,
            vision_enabled,
            subagents_enabled,
            mcp_enabled,
            max_turns,
        }
    }
}

// ---------------------------------------------------------------------------
// build_session_config_options
// ---------------------------------------------------------------------------

/// Builds the full list of [`acp::SessionConfigOption`] entries for Zed.
///
/// Each option is a select drop-down whose currently selected value reflects
/// the state in `runtime`. Call this function again after any state change
/// and send the result to Zed to keep the UI in sync.
///
/// # Arguments
///
/// * `runtime` - The current runtime state to reflect in the option payloads.
///
/// # Returns
///
/// A `Vec<acp::SessionConfigOption>` ready to include in an ACP response.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_config::{build_session_config_options, SessionRuntimeState};
/// use xzatoma::Config;
///
/// let runtime = SessionRuntimeState::from_config(&Config::default());
/// let options = build_session_config_options(&runtime);
/// assert_eq!(options.len(), 7);
/// ```
pub fn build_session_config_options(
    runtime: &SessionRuntimeState,
) -> Vec<acp::SessionConfigOption> {
    vec![
        build_safety_policy_option(runtime),
        build_terminal_execution_option(runtime),
        build_tool_routing_option(runtime),
        build_vision_input_option(runtime),
        build_subagent_delegation_option(runtime),
        build_mcp_tools_option(runtime),
        build_max_turns_option(runtime),
    ]
}

// ---------------------------------------------------------------------------
// apply_config_option_change
// ---------------------------------------------------------------------------

/// Applies a single config option change and returns the updated option list.
///
/// Validates that `config_id` is a known option and that `value_id` is a
/// valid value for that option. On success, returns a [`ConfigChangeEffect`]
/// describing which runtime fields changed and a refreshed option list
/// reflecting the new state.
///
/// # Arguments
///
/// * `config_id` - Stable config option identifier (e.g. `"safety_policy"`).
/// * `value_id`  - The new value to apply (e.g. `"never_confirm"`).
/// * `runtime`   - Current runtime state; used to build the refreshed option list.
///
/// # Returns
///
/// Returns `Ok((ConfigChangeEffect, Vec<acp::SessionConfigOption>))` on success.
///
/// # Errors
///
/// Returns [`XzatomaError::Config`] when `config_id` or `value_id` is not
/// recognised.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_config::{
///     apply_config_option_change, SessionRuntimeState, CONFIG_MAX_TURNS,
/// };
/// use xzatoma::Config;
///
/// let runtime = SessionRuntimeState::from_config(&Config::default());
/// let (effect, options) =
///     apply_config_option_change(CONFIG_MAX_TURNS, "100", &runtime).unwrap();
/// assert_eq!(effect.max_turns, Some(100));
/// assert_eq!(options.len(), 7);
/// ```
pub fn apply_config_option_change(
    config_id: &str,
    value_id: &str,
    runtime: &SessionRuntimeState,
) -> Result<(ConfigChangeEffect, Vec<acp::SessionConfigOption>)> {
    let mut effect = ConfigChangeEffect::none();
    let mut updated = runtime.clone();

    match config_id {
        CONFIG_SAFETY_POLICY => {
            let safety_str = parse_safety_policy_value(value_id)?;
            effect.safety_mode_str = Some(safety_str.clone());
            updated.safety_mode_str = safety_str;
        }
        CONFIG_TERMINAL_EXECUTION => {
            let mode = parse_terminal_execution_value(value_id)?;
            effect.terminal_mode = Some(mode);
            updated.terminal_mode = mode;
        }
        CONFIG_TOOL_ROUTING => {
            let routing = ToolRouting::from_value_id(value_id)?;
            effect.tool_routing = Some(routing.clone());
            updated.tool_routing = routing;
        }
        CONFIG_VISION_INPUT => {
            let enabled = parse_enabled_disabled(value_id, CONFIG_VISION_INPUT)?;
            effect.vision_enabled = Some(enabled);
            updated.vision_enabled = enabled;
        }
        CONFIG_SUBAGENT_DELEGATION => {
            let enabled = parse_enabled_disabled(value_id, CONFIG_SUBAGENT_DELEGATION)?;
            effect.subagents_enabled = Some(enabled);
            updated.subagents_enabled = enabled;
        }
        CONFIG_MCP_TOOLS => {
            let enabled = parse_enabled_disabled(value_id, CONFIG_MCP_TOOLS)?;
            effect.mcp_enabled = Some(enabled);
            updated.mcp_enabled = enabled;
        }
        CONFIG_MAX_TURNS => {
            let turns = parse_max_turns_value(value_id)?;
            effect.max_turns = Some(turns);
            updated.max_turns = turns;
        }
        other => {
            return Err(XzatomaError::Config(format!(
                "unknown session config option id: '{other}'; expected one of: \
                 '{CONFIG_SAFETY_POLICY}', '{CONFIG_TERMINAL_EXECUTION}', \
                 '{CONFIG_TOOL_ROUTING}', '{CONFIG_VISION_INPUT}', \
                 '{CONFIG_SUBAGENT_DELEGATION}', '{CONFIG_MCP_TOOLS}', '{CONFIG_MAX_TURNS}'"
            )));
        }
    }

    let options = build_session_config_options(&updated);
    Ok((effect, options))
}

// ---------------------------------------------------------------------------
// Private option builders
// ---------------------------------------------------------------------------

fn build_safety_policy_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = safety_policy_value_id(&runtime.safety_mode_str).to_string();
    let options = vec![
        acp::SessionConfigSelectOption::new("always_confirm", "Always Confirm"),
        acp::SessionConfigSelectOption::new("confirm_dangerous", "Confirm Dangerous"),
        acp::SessionConfigSelectOption::new("never_confirm", "Never Confirm"),
    ];
    acp::SessionConfigOption::select(CONFIG_SAFETY_POLICY, "Safety Policy", current, options)
        .description(Some(
            "Controls when XZatoma requests confirmation before executing operations.".to_string(),
        ))
        .category(Some(acp::SessionConfigOptionCategory::Mode))
}

fn build_terminal_execution_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = terminal_execution_value_id(runtime.terminal_mode);
    let options = vec![
        acp::SessionConfigSelectOption::new("interactive", "Interactive"),
        acp::SessionConfigSelectOption::new("restricted_autonomous", "Restricted Autonomous"),
        acp::SessionConfigSelectOption::new("full_autonomous", "Full Autonomous"),
    ];
    acp::SessionConfigOption::select(
        CONFIG_TERMINAL_EXECUTION,
        "Terminal Execution",
        current,
        options,
    )
    .description(Some(
        "Controls how terminal commands are validated and executed.".to_string(),
    ))
}

fn build_tool_routing_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = runtime.tool_routing.as_value_id();
    let options = vec![
        acp::SessionConfigSelectOption::new("prefer_ide", "Prefer IDE"),
        acp::SessionConfigSelectOption::new("prefer_local", "Prefer Local"),
        acp::SessionConfigSelectOption::new("require_ide", "Require IDE"),
    ];
    acp::SessionConfigOption::select(CONFIG_TOOL_ROUTING, "Tool Routing", current, options)
        .description(Some(
            "Controls whether tool calls are delegated to the IDE or executed locally.".to_string(),
        ))
}

fn build_vision_input_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = bool_to_enabled_disabled(runtime.vision_enabled);
    let options = enabled_disabled_options();
    acp::SessionConfigOption::select(CONFIG_VISION_INPUT, "Vision Input", current, options)
        .description(Some(
            "Controls whether image and screenshot inputs are accepted in prompts.".to_string(),
        ))
}

fn build_subagent_delegation_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = bool_to_enabled_disabled(runtime.subagents_enabled);
    let options = enabled_disabled_options();
    acp::SessionConfigOption::select(
        CONFIG_SUBAGENT_DELEGATION,
        "Subagent Delegation",
        current,
        options,
    )
    .description(Some(
        "Controls whether XZatoma can spin up subagent workers to parallelize tasks.".to_string(),
    ))
}

fn build_mcp_tools_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = bool_to_enabled_disabled(runtime.mcp_enabled);
    let options = enabled_disabled_options();
    acp::SessionConfigOption::select(CONFIG_MCP_TOOLS, "MCP Tools", current, options).description(
        Some("Controls whether tools from connected MCP servers are available.".to_string()),
    )
}

fn build_max_turns_option(runtime: &SessionRuntimeState) -> acp::SessionConfigOption {
    let current = runtime.max_turns.to_string();
    let options = vec![
        acp::SessionConfigSelectOption::new("10", "10 turns"),
        acp::SessionConfigSelectOption::new("25", "25 turns"),
        acp::SessionConfigSelectOption::new("50", "50 turns"),
        acp::SessionConfigSelectOption::new("100", "100 turns"),
        acp::SessionConfigSelectOption::new("200", "200 turns"),
    ];
    acp::SessionConfigOption::select(CONFIG_MAX_TURNS, "Max Turns", current, options).description(
        Some("Maximum number of agent turns allowed per run before the agent stops.".to_string()),
    )
}

// ---------------------------------------------------------------------------
// Private value helpers
// ---------------------------------------------------------------------------

fn safety_policy_value_id(safety_mode_str: &str) -> &'static str {
    match safety_mode_str.to_lowercase().as_str() {
        "yolo" | "never" | "off" => "never_confirm",
        "confirm_dangerous" | "dangerous" => "confirm_dangerous",
        _ => "always_confirm",
    }
}

fn terminal_execution_value_id(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Interactive => "interactive",
        ExecutionMode::RestrictedAutonomous => "restricted_autonomous",
        ExecutionMode::FullAutonomous => "full_autonomous",
    }
}

fn bool_to_enabled_disabled(value: bool) -> &'static str {
    if value {
        "enabled"
    } else {
        "disabled"
    }
}

fn enabled_disabled_options() -> Vec<acp::SessionConfigSelectOption> {
    vec![
        acp::SessionConfigSelectOption::new("enabled", "Enabled"),
        acp::SessionConfigSelectOption::new("disabled", "Disabled"),
    ]
}

fn parse_safety_policy_value(value_id: &str) -> Result<String> {
    match value_id {
        "always_confirm" => Ok("confirm".to_string()),
        "confirm_dangerous" => Ok("confirm_dangerous".to_string()),
        "never_confirm" => Ok("yolo".to_string()),
        other => Err(XzatomaError::Config(format!(
            "unknown safety_policy value: '{other}'; \
             expected one of: 'always_confirm', 'confirm_dangerous', 'never_confirm'"
        ))),
    }
}

fn parse_terminal_execution_value(value_id: &str) -> Result<ExecutionMode> {
    match value_id {
        "interactive" => Ok(ExecutionMode::Interactive),
        "restricted_autonomous" => Ok(ExecutionMode::RestrictedAutonomous),
        "full_autonomous" => Ok(ExecutionMode::FullAutonomous),
        other => Err(XzatomaError::Config(format!(
            "unknown terminal_execution value: '{other}'; \
             expected one of: 'interactive', 'restricted_autonomous', 'full_autonomous'"
        ))),
    }
}

fn parse_enabled_disabled(value_id: &str, config_id: &str) -> Result<bool> {
    match value_id {
        "enabled" => Ok(true),
        "disabled" => Ok(false),
        other => Err(XzatomaError::Config(format!(
            "unknown '{config_id}' value: '{other}'; expected 'enabled' or 'disabled'"
        ))),
    }
}

fn parse_max_turns_value(value_id: &str) -> Result<u32> {
    match value_id {
        "10" => Ok(10),
        "25" => Ok(25),
        "50" => Ok(50),
        "100" => Ok(100),
        "200" => Ok(200),
        other => Err(XzatomaError::Config(format!(
            "unknown max_turns value: '{other}'; \
             expected one of: '10', '25', '50', '100', '200'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_runtime() -> SessionRuntimeState {
        SessionRuntimeState::from_config(&Config::default())
    }

    // --- SessionRuntimeState::from_config ---

    #[test]
    fn test_session_runtime_state_from_config_default_safety_is_confirm() {
        let state = default_runtime();
        assert_eq!(state.safety_mode_str, "confirm");
    }

    #[test]
    fn test_session_runtime_state_from_config_default_terminal_mode() {
        let state = default_runtime();
        assert_eq!(state.terminal_mode, ExecutionMode::RestrictedAutonomous);
    }

    #[test]
    fn test_session_runtime_state_from_config_default_tool_routing_is_prefer_ide() {
        let state = default_runtime();
        assert_eq!(state.tool_routing, ToolRouting::PreferIde);
    }

    #[test]
    fn test_session_runtime_state_from_config_vision_enabled_reflects_config() {
        let config = Config::default();
        let state = SessionRuntimeState::from_config(&config);
        assert_eq!(state.vision_enabled, config.acp.stdio.vision_enabled);
    }

    #[test]
    fn test_session_runtime_state_from_config_max_turns_reflects_agent_config() {
        let config = Config::default();
        let state = SessionRuntimeState::from_config(&config);
        assert_eq!(state.max_turns, config.agent.max_turns as u32);
    }

    // --- build_session_config_options ---

    #[test]
    fn test_build_session_config_options_returns_seven_options() {
        let options = build_session_config_options(&default_runtime());
        assert_eq!(options.len(), 7);
    }

    #[test]
    fn test_build_session_config_options_ids_are_stable() {
        let options = build_session_config_options(&default_runtime());
        let ids: Vec<&str> = options.iter().map(|o| o.id.0.as_ref()).collect();
        assert!(ids.contains(&CONFIG_SAFETY_POLICY));
        assert!(ids.contains(&CONFIG_TERMINAL_EXECUTION));
        assert!(ids.contains(&CONFIG_TOOL_ROUTING));
        assert!(ids.contains(&CONFIG_VISION_INPUT));
        assert!(ids.contains(&CONFIG_SUBAGENT_DELEGATION));
        assert!(ids.contains(&CONFIG_MCP_TOOLS));
        assert!(ids.contains(&CONFIG_MAX_TURNS));
    }

    #[test]
    fn test_build_session_config_options_all_have_non_empty_names() {
        for option in build_session_config_options(&default_runtime()) {
            assert!(
                !option.name.is_empty(),
                "option '{}' has an empty name",
                option.id
            );
        }
    }

    #[test]
    fn test_build_session_config_options_all_have_non_empty_descriptions() {
        for option in build_session_config_options(&default_runtime()) {
            let desc = option.description.as_deref().unwrap_or("");
            assert!(
                !desc.is_empty(),
                "option '{}' has an empty description",
                option.id
            );
        }
    }

    // --- apply_config_option_change: valid inputs ---

    #[test]
    fn test_apply_config_option_change_safety_policy_never_confirm() {
        let runtime = default_runtime();
        let (effect, options) =
            apply_config_option_change(CONFIG_SAFETY_POLICY, "never_confirm", &runtime).unwrap();
        assert_eq!(effect.safety_mode_str.as_deref(), Some("yolo"));
        assert_eq!(options.len(), 7);
    }

    #[test]
    fn test_apply_config_option_change_safety_policy_always_confirm() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_SAFETY_POLICY, "always_confirm", &runtime).unwrap();
        assert_eq!(effect.safety_mode_str.as_deref(), Some("confirm"));
    }

    #[test]
    fn test_apply_config_option_change_safety_policy_confirm_dangerous() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_SAFETY_POLICY, "confirm_dangerous", &runtime)
                .unwrap();
        assert_eq!(effect.safety_mode_str.as_deref(), Some("confirm_dangerous"));
    }

    #[test]
    fn test_apply_config_option_change_terminal_execution_full_autonomous() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_TERMINAL_EXECUTION, "full_autonomous", &runtime)
                .unwrap();
        assert_eq!(effect.terminal_mode, Some(ExecutionMode::FullAutonomous));
    }

    #[test]
    fn test_apply_config_option_change_terminal_execution_interactive() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_TERMINAL_EXECUTION, "interactive", &runtime).unwrap();
        assert_eq!(effect.terminal_mode, Some(ExecutionMode::Interactive));
    }

    #[test]
    fn test_apply_config_option_change_tool_routing_prefer_local() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_TOOL_ROUTING, "prefer_local", &runtime).unwrap();
        assert_eq!(effect.tool_routing, Some(ToolRouting::PreferLocal));
    }

    #[test]
    fn test_apply_config_option_change_tool_routing_require_ide() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_TOOL_ROUTING, "require_ide", &runtime).unwrap();
        assert_eq!(effect.tool_routing, Some(ToolRouting::RequireIde));
    }

    #[test]
    fn test_apply_config_option_change_vision_input_disabled() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_VISION_INPUT, "disabled", &runtime).unwrap();
        assert_eq!(effect.vision_enabled, Some(false));
    }

    #[test]
    fn test_apply_config_option_change_subagent_delegation_enabled() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_SUBAGENT_DELEGATION, "enabled", &runtime).unwrap();
        assert_eq!(effect.subagents_enabled, Some(true));
    }

    #[test]
    fn test_apply_config_option_change_mcp_tools_disabled() {
        let runtime = default_runtime();
        let (effect, _) =
            apply_config_option_change(CONFIG_MCP_TOOLS, "disabled", &runtime).unwrap();
        assert_eq!(effect.mcp_enabled, Some(false));
    }

    #[test]
    fn test_apply_config_option_change_max_turns_100() {
        let runtime = default_runtime();
        let (effect, _) = apply_config_option_change(CONFIG_MAX_TURNS, "100", &runtime).unwrap();
        assert_eq!(effect.max_turns, Some(100));
    }

    #[test]
    fn test_apply_config_option_change_max_turns_all_valid_values() {
        let runtime = default_runtime();
        for (value_id, expected) in [
            ("10", 10u32),
            ("25", 25),
            ("50", 50),
            ("100", 100),
            ("200", 200),
        ] {
            let (effect, _) =
                apply_config_option_change(CONFIG_MAX_TURNS, value_id, &runtime).unwrap();
            assert_eq!(effect.max_turns, Some(expected));
        }
    }

    #[test]
    fn test_apply_config_option_change_returns_refreshed_option_list() {
        let runtime = default_runtime();
        let (_effect, options) =
            apply_config_option_change(CONFIG_TERMINAL_EXECUTION, "interactive", &runtime).unwrap();
        assert_eq!(options.len(), 7);
    }

    // --- apply_config_option_change: invalid inputs ---

    #[test]
    fn test_apply_config_option_change_unknown_config_id_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change("nonexistent_option", "some_value", &runtime);
        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("nonexistent_option"));
    }

    #[test]
    fn test_apply_config_option_change_invalid_safety_value_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change(CONFIG_SAFETY_POLICY, "invalid_value", &runtime);
        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("invalid_value"));
    }

    #[test]
    fn test_apply_config_option_change_invalid_terminal_value_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change(CONFIG_TERMINAL_EXECUTION, "turbo_mode", &runtime);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_config_option_change_invalid_tool_routing_value_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change(CONFIG_TOOL_ROUTING, "cloud_routing", &runtime);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_config_option_change_invalid_vision_value_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change(CONFIG_VISION_INPUT, "maybe", &runtime);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_config_option_change_invalid_max_turns_value_returns_error() {
        let runtime = default_runtime();
        let result = apply_config_option_change(CONFIG_MAX_TURNS, "999", &runtime);
        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("999"));
    }

    // --- ToolRouting ---

    #[test]
    fn test_tool_routing_round_trips_all_variants() {
        for routing in [
            ToolRouting::PreferIde,
            ToolRouting::PreferLocal,
            ToolRouting::RequireIde,
        ] {
            let id = routing.as_value_id();
            let parsed = ToolRouting::from_value_id(id).unwrap();
            assert_eq!(parsed, routing);
        }
    }

    #[test]
    fn test_tool_routing_from_value_id_unknown_returns_error() {
        assert!(ToolRouting::from_value_id("cloud_ide").is_err());
    }

    #[test]
    fn test_tool_routing_value_ids_are_lowercase_snake_case() {
        for routing in [
            ToolRouting::PreferIde,
            ToolRouting::PreferLocal,
            ToolRouting::RequireIde,
        ] {
            let id = routing.as_value_id();
            assert_eq!(id, id.to_lowercase(), "value id '{id}' must be lowercase");
        }
    }
}
