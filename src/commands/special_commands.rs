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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Display mention syntax help
    ///
    /// Shows how to use context mentions (@file, @search, @grep, @url).
    Mentions,

    /// Trigger authentication flow for a provider
    ///
    /// Use `/auth` to start authentication for the configured provider,
    /// or `/auth <provider>` to authenticate a specific provider (copilot, ollama).
    Auth(Option<String>),

    /// List available models
    ///
    /// Shows all available models from the current provider.
    ListModels,

    /// Show help specific to the models command
    ///
    /// Useful when users type `/models` without any subcommand.
    ModelsHelp,

    /// Switch to a different model
    ///
    /// Changes the active model for the provider.
    /// May require confirmation if the context window is smaller than current conversation.
    SwitchModel(String),

    /// Display context window information
    ///
    /// Shows current token usage, context window size, remaining tokens, and usage percentage.
    ShowContextInfo,

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
        "/mentions" => SpecialCommand::Mentions,

        // Model management commands and provider auth
        // `/models` with no subcommand prints help for model-management usage
        "/models" => SpecialCommand::ModelsHelp,
        "/models list" => SpecialCommand::ListModels,
        "/context" => SpecialCommand::ShowContextInfo,
        "/auth" => SpecialCommand::Auth(None),
        input if input.starts_with("/auth ") => {
            let rest = input[6..].trim();
            if !rest.is_empty() {
                SpecialCommand::Auth(Some(rest.to_string()))
            } else {
                SpecialCommand::None
            }
        }

        // Model switching with arguments
        input if input.starts_with("/model ") => {
            let rest = input[7..].trim();
            if rest == "info" {
                // For now, treat /model info as unsupported
                // (would need additional argument for specific model)
                SpecialCommand::None
            } else if !rest.is_empty() {
                SpecialCommand::SwitchModel(rest.to_string())
            } else {
                SpecialCommand::None
            }
        }

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

CONTEXT MENTIONS (Quick Reference):
  @file.rs              - Include file contents
  @file.rs#L10-20       - Include specific lines
  @search:"pattern"     - Search for literal text
  @grep:"regex"         - Search with regex patterns
  @url:https://...      - Include web content

MODEL MANAGEMENT:
  /models         - Show help for models subcommands and flags
  /models list    - Show available models from current provider
  /models info <name> - Show detailed info about a specific model
  /model <name>   - Switch to a different model
  /context        - Show context window and token usage information
  /auth [provider] - Start authentication for the provider; use `/auth` for the configured provider

SESSION INFORMATION:
  /status         - Show current mode and safety status
  /help           - Show this help message
  /?              - Same as /help
  /mentions       - Show detailed context mention help

SESSION CONTROL:
  exit            - Exit interactive mode
  quit            - Same as exit

NOTES:
  - Commands are case-insensitive
  - Regular text (not starting with /) is sent to the agent
  - Mentions (@file, @search, etc.) inject context into prompts
  - Switching to Write mode enables powerful file and terminal tools
  - Use /safe in Write mode to require confirmation for dangerous operations
  - See /mentions for complete mention syntax and examples
"#
    );
}

/// Display detailed help for the `/models` command
///
/// Shows usage, flags, and examples for `/models` subcommands such as
/// `/models list` and `/models info <name>`.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::commands::special_commands::print_models_help;
///
/// print_models_help();
/// ```
pub fn print_models_help() {
    println!(
        r#"
Models Command - Usage and Examples
===================================

The `/models` command manages and inspects the provider's available models.

USAGE:
  /models                      - Show this help message for model-management
  /models list                 - Show available models from the current provider
      Flags:
        --json      - Output pretty-printed JSON (good for tooling like jq)
        --summary   - Output a compact summary suitable for scripting/comparison

  /models info <name>          - Show detailed information about a specific model
      Flags:
        --json      - Output model info as JSON
        --summary   - Output summarized detail

EXAMPLES:
  /models
  /models list
  /models list --json
  /models list --summary
  /models info gpt-4 --summary

NOTES:
  - `--json` prints pretty JSON (useful with `jq`)
  - `--summary` prints compact, script-friendly summaries
  - Use `/models` to see this help when you don't know which subcommand to run
"#
    );
}

/// Display detailed help for context mentions
///
/// Shows mention syntax, examples, and best practices for all mention types.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::commands::special_commands::print_mention_help;
///
/// print_mention_help();
/// ```
pub fn print_mention_help() {
    println!(
        r#"
Context Mentions for XZatoma
=============================

Context mentions let you include file contents, search results, and web content
in your prompts. Use @mention syntax to reference relevant information.

FILE MENTIONS
=============
Include file contents from your project.

Syntax:
  @filename                 - Simple file reference
  @path/to/file.rs        - Full path
  @file.rs#L10-20          - Specific line range
  @file.rs#L10-           - From line 10 to end
  @file.rs#L-20           - Start to line 20

Examples:
  Review @config.yaml
  Check the error handler: @src/error.rs#L50-100
  Show @README.md
  Include @src/lib.rs

Smart Features:
  - Abbreviations: @lib → src/lib.rs, @main → src/main.rs
  - Fuzzy matching: suggests similar filenames if exact not found
  - Line range caching: fast repeated access to same file

SEARCH MENTIONS
===============
Find literal text patterns across your codebase.

Syntax:
  @search:"pattern"         - Find exact text (case-sensitive)
  @search:"multi word"      - Patterns with spaces need quotes

Examples:
  @search:"TODO"
  @search:"error handling"
  @search:"pub fn"
  Find all async functions: @search:"async fn"

Features:
  - Case-sensitive matching
  - Shows file name and line number
  - Results limited to 100 matches
  - Good for specific identifiers

GREP MENTIONS
==============
Find patterns using regular expressions.

Syntax:
  @grep:"regex_pattern"     - Regex with Rust syntax
  @grep:"(?i)case"          - Case-insensitive (with (?i))

Examples:
  @grep:"^pub fn"           - All public function definitions
  @grep:"impl.*Error"       - All Error trait implementations
  @grep:"Result"            - Find Result types
  @grep:"(?i)error"         - Case-insensitive error matching
  @grep:"TODO|FIXME"        - Find common markers

Regex Features:
  - ^ = start of line
  - $ = end of line
  - . = any character
  - * = zero or more
  - + = one or more
  - [abc] = character class
  - | = alternation (or)
  - () = grouping
  - w = word character (use with backslash in actual regex)
  - d = digit (use with backslash in actual regex)
  - s = whitespace (use with backslash in actual regex)

URL MENTIONS
============
Include content from web URLs.

Syntax:
  @url:https://example.com  - Fetch and include web content

Examples:
  @url:https://docs.rs/tokio/latest/tokio/
  @url:https://raw.githubusercontent.com/user/repo/file
  @url:https://api.github.com/repos/user/repo
  Learn from: @url:https://example.com/documentation

Features:
  - Fetches HTTP/HTTPS content
  - Converts HTML to readable text
  - Formats JSON for readability
  - Caches results (24 hours)
  - Prevents SSRF attacks (blocks localhost, private IPs)

Security:
  - Blocks access to localhost and 127.0.0.1
  - Blocks private IP addresses (10.x, 192.168.x, etc.)
  - Only allows HTTP/HTTPS
  - Enforces 60-second timeout
  - Limits content to 1 MB
  - Rate-limited per domain

COMBINING MENTIONS
==================
Use multiple mentions in one prompt:

  Review @config.yaml and implement based on:
  @url:https://example.com/specification

  Include these patterns: @grep:"pub async fn "

  But avoid: @search:"TODO" and @search:"FIXME"

TIPS AND BEST PRACTICES
=======================
- Mentions are fast: use them instead of asking agent to read files
- Be specific: @src/module/file.rs is better than @file.rs
- Use line ranges: @large_file.rs#L100-200 instead of whole file
- Combine strategically: don't overwhelm with too many mentions
- Check errors: agent reports which mentions failed to load
- Leverage caching: second mention of same file is instant

TROUBLESHOOTING
===============
File not found:
  - Use full path: @src/path/to/file.rs
  - Check spelling and capitalization
  - Agent suggests similar filenames with fuzzy matching

Search returns nothing:
  - Verify spelling exactly
  - Try different search terms
  - Try @grep with relaxed regex: @grep:"[Tt]odo "

SSRF blocked (for URLs):
  - Cannot access localhost or private IPs
  - Use public URLs instead
  - Works with public documentation sites

URL fetch timeout:
  - Large pages may be slow
  - Try specific pages instead of homepage
  - URL results are cached after first fetch

For more details, see the user guide: docs/how-to/use_context_mentions.md
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_special_command_models_bare_returns_models_help() {
        assert_eq!(parse_special_command("/models"), SpecialCommand::ModelsHelp);
    }

    #[test]
    fn test_parse_special_command_models_list_returns_list_models() {
        assert_eq!(
            parse_special_command("/models list"),
            SpecialCommand::ListModels
        );
    }

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
    fn test_parse_auth_without_provider() {
        let cmd = parse_special_command("/auth");
        assert_eq!(cmd, SpecialCommand::Auth(None));
    }

    #[test]
    fn test_parse_auth_with_provider() {
        let cmd = parse_special_command("/auth copilot");
        assert_eq!(cmd, SpecialCommand::Auth(Some("copilot".to_string())));
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

    #[test]
    fn test_parse_mentions() {
        let cmd = parse_special_command("/mentions");
        assert_eq!(cmd, SpecialCommand::Mentions);
    }

    #[test]
    fn test_parse_list_models() {
        let cmd = parse_special_command("/models list");
        assert_eq!(cmd, SpecialCommand::ListModels);
    }

    #[test]
    fn test_parse_switch_model() {
        let cmd = parse_special_command("/model gpt-4");
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_switch_model_with_hyphen() {
        let cmd = parse_special_command("/model gemini-2.0");
        assert_eq!(cmd, SpecialCommand::SwitchModel("gemini-2.0".to_string()));
    }

    #[test]
    fn test_parse_switch_model_case_insensitive() {
        let cmd = parse_special_command("/MODEL GPT-4");
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_show_context_info() {
        let cmd = parse_special_command("/context");
        assert_eq!(cmd, SpecialCommand::ShowContextInfo);
    }

    #[test]
    fn test_parse_model_command_no_args_returns_none() {
        let cmd = parse_special_command("/model");
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_model_command_with_spaces() {
        let cmd = parse_special_command("/model  gpt-4  ");
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_model_info_not_supported() {
        let cmd = parse_special_command("/model info");
        assert_eq!(cmd, SpecialCommand::None);
    }
}
