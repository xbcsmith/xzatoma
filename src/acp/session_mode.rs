//! XZatoma ACP session mode definitions and runtime effect mapping.
//!
//! This module defines the session modes that XZatoma advertises to Zed via the
//! Agent-Client Protocol. Each mode controls what operations the agent is
//! permitted to perform and how it interacts with the safety and terminal
//! subsystems.
//!
//! # Mode Overview
//!
//! | Mode ID           | File writes | Terminal       | Confirmations       |
//! |-------------------|-------------|----------------|---------------------|
//! | `planning`        | No          | None           | Always              |
//! | `write`           | Yes         | Safe only      | Always              |
//! | `safe`            | Yes         | Safe only      | Always (Zed prompt) |
//! | `full_autonomous` | Yes         | Unrestricted   | Never               |
//!
//! # Examples
//!
//! ```
//! use xzatoma::acp::session_mode::{
//!     build_session_modes, build_session_mode_state, mode_runtime_effect, MODE_PLANNING,
//! };
//!
//! let modes = build_session_modes();
//! assert!(!modes.is_empty());
//!
//! let state = build_session_mode_state(MODE_PLANNING);
//! assert_eq!(state.current_mode_id.0.as_ref(), MODE_PLANNING);
//!
//! let effect = mode_runtime_effect(MODE_PLANNING).unwrap();
//! assert_eq!(effect.chat_mode_str, "planning");
//! ```

use acp_sdk::schema as acp;
use agent_client_protocol as acp_sdk;

use crate::config::ExecutionMode;
use crate::error::{Result, XzatomaError};

// ---------------------------------------------------------------------------
// Mode ID constants
// ---------------------------------------------------------------------------

/// Mode ID for planning (read-only analysis, no writes, no destructive terminal commands).
pub const MODE_PLANNING: &str = "planning";

/// Mode ID for write mode (file edits and non-destructive terminals with safe confirmation policy).
pub const MODE_WRITE: &str = "write";

/// Mode ID for safe mode (write-capable, requests Zed user approval for risky actions).
pub const MODE_SAFE: &str = "safe";

/// Mode ID for full-autonomous mode (write and commands without confirmations within configured limits).
pub const MODE_FULL_AUTONOMOUS: &str = "full_autonomous";

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Builds the list of [`acp::SessionMode`] entries advertised to Zed.
///
/// The returned vector contains one entry per supported XZatoma mode, with
/// a human-readable name and description that Zed displays in the mode
/// selector UI.
///
/// # Returns
///
/// A `Vec<acp::SessionMode>` containing all four XZatoma session modes.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_mode::{build_session_modes, MODE_PLANNING};
///
/// let modes = build_session_modes();
/// assert_eq!(modes.len(), 4);
/// assert_eq!(modes[0].id.0.as_ref(), MODE_PLANNING);
/// ```
pub fn build_session_modes() -> Vec<acp::SessionMode> {
    vec![
        acp::SessionMode::new(MODE_PLANNING, "Planning").description(Some(
            "Read-only analysis mode. No file writes or destructive terminal commands \
                 are permitted. Use this mode to explore, research, and plan."
                .to_string(),
        )),
        acp::SessionMode::new(MODE_WRITE, "Write").description(Some(
            "File editing and safe terminal execution are allowed. Dangerous operations \
                 require confirmation before proceeding."
                .to_string(),
        )),
        acp::SessionMode::new(MODE_SAFE, "Safe").description(Some(
            "Write-capable mode with Zed user approval required for any risky action. \
                 All potentially destructive operations trigger a confirmation prompt."
                .to_string(),
        )),
        acp::SessionMode::new(MODE_FULL_AUTONOMOUS, "Full Autonomous").description(Some(
            "Unrestricted write and terminal access within configured resource limits. \
                 No confirmations are requested. Use with care."
                .to_string(),
        )),
    ]
}

/// Builds an [`acp::SessionModeState`] with `mode_id` as the active mode.
///
/// The returned state includes the full set of available modes so that Zed
/// can populate its mode selector immediately.
///
/// # Arguments
///
/// * `mode_id` - The currently active mode identifier. If the value does not
///   match any known mode, it is still stored as-is in `current_mode_id` so
///   that round-trips survive across restarts without data loss.
///
/// # Returns
///
/// An [`acp::SessionModeState`] with the given mode set as active.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_mode::{build_session_mode_state, MODE_WRITE};
///
/// let state = build_session_mode_state(MODE_WRITE);
/// assert_eq!(state.current_mode_id.0.as_ref(), MODE_WRITE);
/// assert_eq!(state.available_modes.len(), 4);
/// ```
pub fn build_session_mode_state(mode_id: &str) -> acp::SessionModeState {
    acp::SessionModeState::new(mode_id.to_string(), build_session_modes())
}

// ---------------------------------------------------------------------------
// Runtime effect
// ---------------------------------------------------------------------------

/// The runtime behavioral settings that a session mode maps to.
///
/// When a mode change is applied, `ModeRuntimeEffect` carries the concrete
/// configuration strings and enum values that the rest of XZatoma uses to
/// enforce the mode's constraints.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_mode::{mode_runtime_effect, MODE_FULL_AUTONOMOUS};
/// use xzatoma::config::ExecutionMode;
///
/// let effect = mode_runtime_effect(MODE_FULL_AUTONOMOUS).unwrap();
/// assert_eq!(effect.chat_mode_str, "write");
/// assert_eq!(effect.safety_mode_str, "yolo");
/// assert_eq!(effect.terminal_mode, ExecutionMode::FullAutonomous);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeRuntimeEffect {
    /// The chat mode string accepted by [`crate::chat_mode::ChatMode::parse_str`].
    pub chat_mode_str: String,
    /// The safety mode string accepted by [`crate::chat_mode::SafetyMode::parse_str`].
    pub safety_mode_str: String,
    /// The terminal execution mode to apply.
    pub terminal_mode: ExecutionMode,
}

/// Maps a session mode ID to its runtime behavioral settings.
///
/// Returns a [`ModeRuntimeEffect`] describing which chat mode, safety mode,
/// and terminal execution mode should be activated when the named session mode
/// is applied.
///
/// # Arguments
///
/// * `mode_id` - One of the stable mode ID constants: [`MODE_PLANNING`],
///   [`MODE_WRITE`], [`MODE_SAFE`], or [`MODE_FULL_AUTONOMOUS`].
///
/// # Returns
///
/// Returns `Ok(ModeRuntimeEffect)` when `mode_id` is recognised.
///
/// # Errors
///
/// Returns [`XzatomaError::Config`] when `mode_id` is not a known mode.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::session_mode::{mode_runtime_effect, MODE_PLANNING, MODE_SAFE};
/// use xzatoma::config::ExecutionMode;
///
/// let planning = mode_runtime_effect(MODE_PLANNING).unwrap();
/// assert_eq!(planning.chat_mode_str, "planning");
/// assert_eq!(planning.terminal_mode, ExecutionMode::Interactive);
///
/// let safe = mode_runtime_effect(MODE_SAFE).unwrap();
/// assert_eq!(safe.terminal_mode, ExecutionMode::RestrictedAutonomous);
/// ```
pub fn mode_runtime_effect(mode_id: &str) -> Result<ModeRuntimeEffect> {
    match mode_id {
        MODE_PLANNING => Ok(ModeRuntimeEffect {
            chat_mode_str: "planning".to_string(),
            safety_mode_str: "confirm".to_string(),
            terminal_mode: ExecutionMode::Interactive,
        }),
        MODE_WRITE => Ok(ModeRuntimeEffect {
            chat_mode_str: "write".to_string(),
            safety_mode_str: "confirm".to_string(),
            terminal_mode: ExecutionMode::RestrictedAutonomous,
        }),
        MODE_SAFE => Ok(ModeRuntimeEffect {
            chat_mode_str: "write".to_string(),
            safety_mode_str: "confirm".to_string(),
            terminal_mode: ExecutionMode::RestrictedAutonomous,
        }),
        MODE_FULL_AUTONOMOUS => Ok(ModeRuntimeEffect {
            chat_mode_str: "write".to_string(),
            safety_mode_str: "yolo".to_string(),
            terminal_mode: ExecutionMode::FullAutonomous,
        }),
        other => Err(XzatomaError::Config(format!(
            "unknown session mode id: '{other}'; expected one of: \
             '{MODE_PLANNING}', '{MODE_WRITE}', '{MODE_SAFE}', '{MODE_FULL_AUTONOMOUS}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_session_modes_returns_all_four_modes() {
        let modes = build_session_modes();
        assert_eq!(modes.len(), 4);
    }

    #[test]
    fn test_build_session_modes_ids_are_stable_constants() {
        let modes = build_session_modes();
        let ids: Vec<&str> = modes.iter().map(|m| m.id.0.as_ref()).collect();
        assert_eq!(ids[0], MODE_PLANNING);
        assert_eq!(ids[1], MODE_WRITE);
        assert_eq!(ids[2], MODE_SAFE);
        assert_eq!(ids[3], MODE_FULL_AUTONOMOUS);
    }

    #[test]
    fn test_build_session_modes_all_have_non_empty_names() {
        for mode in build_session_modes() {
            assert!(!mode.name.is_empty(), "mode '{}' has empty name", mode.id);
        }
    }

    #[test]
    fn test_build_session_modes_all_have_non_empty_descriptions() {
        for mode in build_session_modes() {
            let desc = mode.description.as_deref().unwrap_or("");
            assert!(!desc.is_empty(), "mode '{}' has empty description", mode.id);
        }
    }

    #[test]
    fn test_build_session_mode_state_sets_current_mode_id() {
        let state = build_session_mode_state(MODE_WRITE);
        assert_eq!(state.current_mode_id.0.as_ref(), MODE_WRITE);
    }

    #[test]
    fn test_build_session_mode_state_includes_all_available_modes() {
        let state = build_session_mode_state(MODE_PLANNING);
        assert_eq!(state.available_modes.len(), 4);
    }

    #[test]
    fn test_build_session_mode_state_preserves_unknown_mode_id() {
        // Unknown IDs must round-trip without error so agent state survives restarts.
        let state = build_session_mode_state("unknown_future_mode");
        assert_eq!(state.current_mode_id.0.as_ref(), "unknown_future_mode");
    }

    #[test]
    fn test_mode_runtime_effect_planning() {
        let effect = mode_runtime_effect(MODE_PLANNING).unwrap();
        assert_eq!(effect.chat_mode_str, "planning");
        assert_eq!(effect.safety_mode_str, "confirm");
        assert_eq!(effect.terminal_mode, ExecutionMode::Interactive);
    }

    #[test]
    fn test_mode_runtime_effect_write() {
        let effect = mode_runtime_effect(MODE_WRITE).unwrap();
        assert_eq!(effect.chat_mode_str, "write");
        assert_eq!(effect.safety_mode_str, "confirm");
        assert_eq!(effect.terminal_mode, ExecutionMode::RestrictedAutonomous);
    }

    #[test]
    fn test_mode_runtime_effect_safe() {
        let effect = mode_runtime_effect(MODE_SAFE).unwrap();
        assert_eq!(effect.chat_mode_str, "write");
        assert_eq!(effect.safety_mode_str, "confirm");
        assert_eq!(effect.terminal_mode, ExecutionMode::RestrictedAutonomous);
    }

    #[test]
    fn test_mode_runtime_effect_full_autonomous() {
        let effect = mode_runtime_effect(MODE_FULL_AUTONOMOUS).unwrap();
        assert_eq!(effect.chat_mode_str, "write");
        assert_eq!(effect.safety_mode_str, "yolo");
        assert_eq!(effect.terminal_mode, ExecutionMode::FullAutonomous);
    }

    #[test]
    fn test_mode_runtime_effect_invalid_mode_returns_config_error() {
        let result = mode_runtime_effect("nonexistent_mode");
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("nonexistent_mode"),
            "error message should mention the unknown mode id"
        );
    }

    #[test]
    fn test_mode_runtime_effect_empty_string_returns_config_error() {
        assert!(mode_runtime_effect("").is_err());
    }

    #[test]
    fn test_mode_constants_are_lowercase() {
        for id in [MODE_PLANNING, MODE_WRITE, MODE_SAFE, MODE_FULL_AUTONOMOUS] {
            assert_eq!(
                id,
                id.to_lowercase(),
                "mode constant '{id}' must be lowercase"
            );
        }
    }

    #[test]
    fn test_mode_runtime_effect_is_deterministic() {
        // Calling the function twice returns identical results.
        let a = mode_runtime_effect(MODE_WRITE).unwrap();
        let b = mode_runtime_effect(MODE_WRITE).unwrap();
        assert_eq!(a, b);
    }
}
