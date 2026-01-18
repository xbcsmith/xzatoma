//! Chat mode types and utilities
//!
//! This module defines the different modes for interactive chat:
//! - Planning mode: read-only, for creating plans
//! - Write mode: read/write, for executing tasks
//!
//! It also defines safety modes that control command confirmation behavior.

use std::fmt;

/// Chat mode for interactive sessions
///
/// Determines which tools are available and how the agent behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ChatMode {
    /// Planning mode: read-only access to files
    ///
    /// In this mode, the agent can only read files and create plans.
    /// No file modifications or terminal commands are permitted.
    Planning,

    /// Write mode: read and write access to files and terminal
    ///
    /// In this mode, the agent has full access to file operations
    /// and terminal commands (subject to safety validation).
    Write,
}

impl fmt::Display for ChatMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planning => write!(f, "PLANNING"),
            Self::Write => write!(f, "WRITE"),
        }
    }
}

impl ChatMode {
    /// Parse a chat mode from a string
    ///
    /// # Arguments
    ///
    /// * `s` - String representation of the mode ("planning" or "write")
    ///
    /// # Returns
    ///
    /// Returns the parsed ChatMode or an error if the string is invalid
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::chat_mode::ChatMode;
    ///
    /// let mode = ChatMode::parse_str("planning").unwrap();
    /// assert_eq!(mode, ChatMode::Planning);
    /// ```
    #[allow(dead_code)]
    pub fn parse_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "planning" => Ok(Self::Planning),
            "write" => Ok(Self::Write),
            other => Err(format!("Unknown chat mode: {}", other)),
        }
    }

    /// Get a user-friendly description of this mode
    ///
    /// # Returns
    ///
    /// A description of what the mode does
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Planning => "Read-only mode for creating plans",
            Self::Write => "Read/write mode for executing tasks",
        }
    }
}

/// Safety mode for command execution
///
/// Controls whether the agent must confirm dangerous operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SafetyMode {
    /// Always confirm dangerous operations
    ///
    /// The agent must explicitly confirm before executing
    /// dangerous terminal commands or destructive file operations.
    AlwaysConfirm,

    /// Never confirm operations (YOLO mode)
    ///
    /// Operations proceed without confirmation. Use with caution
    /// as this can lead to unintended side effects.
    NeverConfirm,
}

impl fmt::Display for SafetyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlwaysConfirm => write!(f, "SAFE"),
            Self::NeverConfirm => write!(f, "YOLO"),
        }
    }
}

impl SafetyMode {
    /// Parse a safety mode from a string
    ///
    /// # Arguments
    ///
    /// * `s` - String representation ("confirm", "always", "safe", "yolo", "never", or "off")
    ///
    /// # Returns
    ///
    /// Returns the parsed SafetyMode or an error if the string is invalid
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::chat_mode::SafetyMode;
    ///
    /// let mode = SafetyMode::parse_str("yolo").unwrap();
    /// assert_eq!(mode, SafetyMode::NeverConfirm);
    /// ```
    #[allow(dead_code)]
    pub fn parse_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "confirm" | "always" | "safe" | "on" => Ok(Self::AlwaysConfirm),
            "yolo" | "never" | "off" => Ok(Self::NeverConfirm),
            other => Err(format!("Unknown safety mode: {}", other)),
        }
    }

    /// Get a user-friendly description of this safety mode
    ///
    /// # Returns
    ///
    /// A description of what the mode does
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Self::AlwaysConfirm => "Confirm dangerous operations",
            Self::NeverConfirm => "Never confirm operations (YOLO)",
        }
    }
}

/// Current chat mode state
///
/// Tracks the active chat mode and safety mode during a session.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatModeState {
    /// The current chat mode
    pub chat_mode: ChatMode,
    /// The current safety mode
    pub safety_mode: SafetyMode,
}

#[allow(dead_code)]
impl ChatModeState {
    /// Create a new chat mode state
    ///
    /// # Arguments
    ///
    /// * `chat_mode` - The initial chat mode
    /// * `safety_mode` - The initial safety mode
    ///
    /// # Returns
    ///
    /// A new ChatModeState instance
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode, ChatModeState};
    ///
    /// let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
    /// assert_eq!(state.chat_mode, ChatMode::Planning);
    /// ```
    pub fn new(chat_mode: ChatMode, safety_mode: SafetyMode) -> Self {
        Self {
            chat_mode,
            safety_mode,
        }
    }

    /// Switch to a new chat mode
    ///
    /// # Arguments
    ///
    /// * `new_mode` - The new chat mode
    ///
    /// # Returns
    ///
    /// The old chat mode that was replaced
    pub fn switch_mode(&mut self, new_mode: ChatMode) -> ChatMode {
        let old_mode = self.chat_mode;
        self.chat_mode = new_mode;
        old_mode
    }

    /// Switch to a new safety mode
    ///
    /// # Arguments
    ///
    /// * `new_safety` - The new safety mode
    ///
    /// # Returns
    ///
    /// The old safety mode that was replaced
    pub fn switch_safety(&mut self, new_safety: SafetyMode) -> SafetyMode {
        let old_safety = self.safety_mode;
        self.safety_mode = new_safety;
        old_safety
    }

    /// Format a prompt string with mode indicators
    ///
    /// # Returns
    ///
    /// A formatted prompt string like "[PLANNING][SAFE] >> "
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode, ChatModeState};
    ///
    /// let state = ChatModeState::new(ChatMode::Write, SafetyMode::NeverConfirm);
    /// assert_eq!(state.format_prompt(), "[WRITE][YOLO] >> ");
    /// ```
    pub fn format_prompt(&self) -> String {
        format!("[{}][{}] >> ", self.chat_mode, self.safety_mode)
    }

    /// Get the current status as a formatted string
    ///
    /// # Returns
    ///
    /// A multi-line status string
    pub fn status(&self) -> String {
        format!(
            "Mode: {} ({})\nSafety: {} ({})",
            self.chat_mode,
            self.chat_mode.description(),
            self.safety_mode,
            self.safety_mode.description()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_mode_display() {
        assert_eq!(ChatMode::Planning.to_string(), "PLANNING");
        assert_eq!(ChatMode::Write.to_string(), "WRITE");
    }

    #[test]
    fn test_chat_mode_from_str_planning() {
        let mode = ChatMode::parse_str("planning").unwrap();
        assert_eq!(mode, ChatMode::Planning);
    }

    #[test]
    fn test_chat_mode_from_str_write() {
        let mode = ChatMode::parse_str("write").unwrap();
        assert_eq!(mode, ChatMode::Write);
    }

    #[test]
    fn test_chat_mode_from_str_case_insensitive() {
        assert_eq!(ChatMode::parse_str("PLANNING").unwrap(), ChatMode::Planning);
        assert_eq!(ChatMode::parse_str("Write").unwrap(), ChatMode::Write);
    }

    #[test]
    fn test_chat_mode_from_str_invalid() {
        assert!(ChatMode::parse_str("invalid").is_err());
    }

    #[test]
    fn test_chat_mode_description() {
        assert_eq!(
            ChatMode::Planning.description(),
            "Read-only mode for creating plans"
        );
        assert_eq!(
            ChatMode::Write.description(),
            "Read/write mode for executing tasks"
        );
    }

    #[test]
    fn test_safety_mode_display() {
        assert_eq!(SafetyMode::AlwaysConfirm.to_string(), "SAFE");
        assert_eq!(SafetyMode::NeverConfirm.to_string(), "YOLO");
    }

    #[test]
    fn test_safety_mode_from_str_confirm_variants() {
        assert_eq!(
            SafetyMode::parse_str("confirm").unwrap(),
            SafetyMode::AlwaysConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("always").unwrap(),
            SafetyMode::AlwaysConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("safe").unwrap(),
            SafetyMode::AlwaysConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("on").unwrap(),
            SafetyMode::AlwaysConfirm
        );
    }

    #[test]
    fn test_safety_mode_from_str_yolo_variants() {
        assert_eq!(
            SafetyMode::parse_str("yolo").unwrap(),
            SafetyMode::NeverConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("never").unwrap(),
            SafetyMode::NeverConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("off").unwrap(),
            SafetyMode::NeverConfirm
        );
    }

    #[test]
    fn test_safety_mode_from_str_case_insensitive() {
        assert_eq!(
            SafetyMode::parse_str("CONFIRM").unwrap(),
            SafetyMode::AlwaysConfirm
        );
        assert_eq!(
            SafetyMode::parse_str("YOLO").unwrap(),
            SafetyMode::NeverConfirm
        );
    }

    #[test]
    fn test_safety_mode_from_str_invalid() {
        assert!(SafetyMode::parse_str("invalid").is_err());
    }

    #[test]
    fn test_safety_mode_description() {
        assert_eq!(
            SafetyMode::AlwaysConfirm.description(),
            "Confirm dangerous operations"
        );
        assert_eq!(
            SafetyMode::NeverConfirm.description(),
            "Never confirm operations (YOLO)"
        );
    }

    #[test]
    fn test_chat_mode_state_new() {
        let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        assert_eq!(state.chat_mode, ChatMode::Planning);
        assert_eq!(state.safety_mode, SafetyMode::AlwaysConfirm);
    }

    #[test]
    fn test_chat_mode_state_switch_mode() {
        let mut state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        let old_mode = state.switch_mode(ChatMode::Write);
        assert_eq!(old_mode, ChatMode::Planning);
        assert_eq!(state.chat_mode, ChatMode::Write);
    }

    #[test]
    fn test_chat_mode_state_switch_safety() {
        let mut state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        let old_safety = state.switch_safety(SafetyMode::NeverConfirm);
        assert_eq!(old_safety, SafetyMode::AlwaysConfirm);
        assert_eq!(state.safety_mode, SafetyMode::NeverConfirm);
    }

    #[test]
    fn test_chat_mode_state_format_prompt_planning_safe() {
        let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        assert_eq!(state.format_prompt(), "[PLANNING][SAFE] >> ");
    }

    #[test]
    fn test_chat_mode_state_format_prompt_write_yolo() {
        let state = ChatModeState::new(ChatMode::Write, SafetyMode::NeverConfirm);
        assert_eq!(state.format_prompt(), "[WRITE][YOLO] >> ");
    }

    #[test]
    fn test_chat_mode_state_status() {
        let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
        let status = state.status();
        assert!(status.contains("PLANNING"));
        assert!(status.contains("SAFE"));
        assert!(status.contains("Read-only mode"));
        assert!(status.contains("Confirm dangerous operations"));
    }

    #[test]
    fn test_chat_mode_state_clone() {
        let state1 = ChatModeState::new(ChatMode::Write, SafetyMode::NeverConfirm);
        let state2 = state1.clone();
        assert_eq!(state1.chat_mode, state2.chat_mode);
        assert_eq!(state1.safety_mode, state2.safety_mode);
    }
}
