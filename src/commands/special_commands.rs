//! Special commands parser for interactive chat mode
//!
//! This module parses and handles special commands that can be entered during
//! interactive chat sessions. Special commands allow users to:
//! - Switch between Planning and Write modes
//! - Switch between safety modes (AlwaysConfirm and NeverConfirm)
//! - View current mode status
//! - Display help information
//! - Exit the session
//!
//! Commands are prefixed with `/` and are case-insensitive.

use crate::chat_mode::{ChatMode, SafetyMode};

/// Special commands that can be executed during interactive chat
///
/// These commands modify the session state or provide information,
/// rather than being sent to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialCommand {
    /// Switch to a different chat mode
    ///
    /// Changes between Planning (read-only) and Write (read/write) modes.
    /// When switching to Write mode, a warning is displayed.
    SwitchMode(ChatMode),

    /// Switch to a different safety mode
    ///
    /// Changes between AlwaysConfirm (safe) and NeverConfirm (YOLO) modes.
    /// Affects whether terminal commands require confirmation.
    SwitchSafety(SafetyMode),

    /// Display current mode and safety status
    ///
    /// Shows the current chat mode, safety mode, and their descriptions.
    ShowStatus,

    /// Display help information
    ///
    /// Shows all available special commands and their usage.
    Help,

    /// Exit the interactive session
    ///
    /// Gracefully closes the chat session.
    Exit,

    /// Not a special command
    ///
    /// The input should be processed as a regular agent prompt.
    None,
}

/// Parse a user input string into a special command
///
/// Checks if the input matches any special command pattern.
/// Commands are case-insensitive and may have multiple aliases.
///
/// # Arguments
///
/// * `input` - The user input string to parse
///
/// # Returns
///
/// Returns a SpecialCommand enum variant:
/// - SwitchMode, SwitchSafety, ShowStatus, Help, or Exit for special commands
/// - None if the input is not a special command
///
/// # Command Examples
///
/// Chat mode switching:
/// - `/mode planning` or `/planning` - Switch to Planning mode
/// - `/mode write` or `/write` - Switch to Write mode
///
/// Safety mode switching:
/// - `/safe` or `/safety on` - Switch to AlwaysConfirm mode
/// - `/yolo` or `/safety off` - Switch to NeverConfirm mode
///
/// Other commands:
/// - `/status` - Show current mode and safety status
/// - `/help` - Show help information
/// - `exit` or `quit` - Exit the session
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
/// use xzatoma::chat_mode::{ChatMode, SafetyMode};
///
/// let cmd = parse_special_command("/mode planning");
/// assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
///
/// let cmd = parse_special_command("/yolo");
/// assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
///
/// let cmd = parse_special_command("hello agent");
/// assert_eq!(cmd, SpecialCommand::None);
/// ```
pub fn parse_special_command(input: &str) -> SpecialCommand {
    let trimmed = input.trim().to_lowercase();

    match trimmed.as_str() {
        // Chat mode switching
        "/mode planning" | "/planning" => SpecialCommand::SwitchMode(ChatMode::Planning),
        "/mode write" | "/write" => SpecialCommand::SwitchMode(ChatMode::Write),

        // Safety mode switching
        "/safe" | "/safety on" => SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm),
        "/yolo" | "/safety off" => SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm),

        // Status and help
        "/status" => SpecialCommand::ShowStatus,
        "/help" | "/?" => SpecialCommand::Help,

        // Exit commands
        "exit" | "quit" | "/exit" | "/quit" => SpecialCommand::Exit,

        // Not a special command
        _ => SpecialCommand::None,
    }
}

/// Display help text for special commands
///
/// Shows all available special commands with their descriptions
/// and usage examples.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::commands::special_commands::print_help;
///
/// print_help();
/// ```
pub fn print_help() {
    println!(
        r#"
Special Commands for Interactive Chat Mode
===========================================

CHAT MODE SWITCHING:
  /mode planning  - Switch to Planning mode (read-only)
  /planning       - Shorthand for /mode planning
  /mode write     - Switch to Write mode (read/write)
  /write          - Shorthand for /mode write

SAFETY MODE SWITCHING:
  /safe           - Enable safety mode (require confirmations)
  /safety on      - Same as /safe
  /yolo           - Disable safety mode (YOLO mode)
  /safety off     - Same as /yolo

SESSION INFORMATION:
  /status         - Show current mode and safety status
  /help           - Show this help message
  /?              - Same as /help

SESSION CONTROL:
  exit            - Exit interactive mode
  quit            - Same as exit

NOTES:
  - Commands are case-insensitive
  - Regular text (not starting with /) is sent to the agent
  - Switching to Write mode enables powerful file and terminal tools
  - Use /safe in Write mode to require confirmation for dangerous operations
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_switch_mode_planning() {
        let cmd = parse_special_command("/mode planning");
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_switch_mode_planning_shorthand() {
        let cmd = parse_special_command("/planning");
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_switch_mode_write() {
        let cmd = parse_special_command("/mode write");
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Write));
    }

    #[test]
    fn test_parse_switch_mode_write_shorthand() {
        let cmd = parse_special_command("/write");
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Write));
    }

    #[test]
    fn test_parse_switch_safety_always_confirm() {
        let cmd = parse_special_command("/safe");
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm));
    }

    #[test]
    fn test_parse_switch_safety_always_confirm_alt() {
        let cmd = parse_special_command("/safety on");
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm));
    }

    #[test]
    fn test_parse_switch_safety_never_confirm() {
        let cmd = parse_special_command("/yolo");
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
    }

    #[test]
    fn test_parse_switch_safety_never_confirm_alt() {
        let cmd = parse_special_command("/safety off");
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
    }

    #[test]
    fn test_parse_show_status() {
        let cmd = parse_special_command("/status");
        assert_eq!(cmd, SpecialCommand::ShowStatus);
    }

    #[test]
    fn test_parse_help() {
        let cmd = parse_special_command("/help");
        assert_eq!(cmd, SpecialCommand::Help);
    }

    #[test]
    fn test_parse_help_shorthand() {
        let cmd = parse_special_command("/?");
        assert_eq!(cmd, SpecialCommand::Help);
    }

    #[test]
    fn test_parse_exit() {
        let cmd = parse_special_command("exit");
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_exit_with_slash() {
        let cmd = parse_special_command("/exit");
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_quit() {
        let cmd = parse_special_command("quit");
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_quit_with_slash() {
        let cmd = parse_special_command("/quit");
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(
            parse_special_command("/MODE PLANNING"),
            SpecialCommand::SwitchMode(ChatMode::Planning)
        );
        assert_eq!(
            parse_special_command("/WRITE"),
            SpecialCommand::SwitchMode(ChatMode::Write)
        );
        assert_eq!(
            parse_special_command("/SAFE"),
            SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm)
        );
        assert_eq!(
            parse_special_command("/YOLO"),
            SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm)
        );
    }

    #[test]
    fn test_parse_with_whitespace() {
        let cmd = parse_special_command("  /mode planning  ");
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_regular_text_returns_none() {
        let cmd = parse_special_command("hello agent");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_partial_command_returns_none() {
        let cmd = parse_special_command("/mode");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_invalid_mode_returns_none() {
        let cmd = parse_special_command("/mode invalid");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_empty_string_returns_none() {
        let cmd = parse_special_command("");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_whitespace_only_returns_none() {
        let cmd = parse_special_command("   ");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_random_command_returns_none() {
        let cmd = parse_special_command("/random");
        assert_eq!(cmd, SpecialCommand::None);
    }
}
