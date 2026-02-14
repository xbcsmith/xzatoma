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
use thiserror::Error;

/// Errors that can occur when parsing special commands
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Unknown command was entered
    #[error("Unknown command: {0}\n\nType '/help' to see available commands")]
    UnknownCommand(String),

    /// Command was given an unsupported argument
    #[error("Unsupported argument for {command}: {arg}\n\nType '/help' to see valid usage")]
    UnsupportedArgument { command: String, arg: String },

    /// Command requires an argument but none was provided
    #[error("Command {command} requires an argument\n\nUsage: {usage}")]
    MissingArgument { command: String, usage: String },
}

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

    /// Show detailed information about a specific model
    ///
    /// Displays model capabilities, context window size, and other details.
    ShowModelInfo(String),

    /// Switch to a different model
    ///
    /// Changes the active model for the provider.
    /// May require confirmation if the context window is smaller than current conversation.
    SwitchModel(String),

    /// Display context window information
    ///
    /// Shows current token usage, context window size, remaining tokens, and usage percentage.
    ContextInfo,

    /// Summarize current context and start fresh conversation
    ///
    /// Summarizes all messages in the conversation and resets the history,
    /// optionally using a specified model for summarization.
    /// Use `/context summary` to use the configured summary model or current model.
    /// Use `/context summary --model <name>` to use a specific model for summarization.
    ContextSummary { model: Option<String> },

    /// Toggle subagent delegation on or off
    ///
    /// Enables or disables subagent tools in chat mode.
    /// Use `/subagents on` to enable, `/subagents off` to disable, or `/subagents` to toggle.
    ToggleSubagents(bool), // true = enable, false = disable

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
/// Returns Ok(SpecialCommand) for valid commands or SpecialCommand::None for non-commands.
/// Returns Err(CommandError) for invalid commands or invalid arguments.
///
/// # Errors
///
/// Returns CommandError::UnknownCommand if input starts with "/" but is not a valid command.
/// Returns CommandError::UnsupportedArgument if a command receives an invalid argument.
/// Returns CommandError::MissingArgument if a command requires an argument but none was provided.
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
/// let cmd = parse_special_command("/mode planning").unwrap();
/// assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
///
/// let cmd = parse_special_command("/yolo").unwrap();
/// assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
///
/// let cmd = parse_special_command("hello agent").unwrap();
/// assert_eq!(cmd, SpecialCommand::None);
///
/// // Invalid command returns error
/// assert!(parse_special_command("/foo").is_err());
/// ```
pub fn parse_special_command(input: &str) -> Result<SpecialCommand, CommandError> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // If input doesn't start with "/", it's not a command (except exit/quit)
    if !trimmed.starts_with('/') && lower != "exit" && lower != "quit" {
        return Ok(SpecialCommand::None);
    }

    match lower.as_str() {
        // Chat mode switching
        "/mode planning" | "/planning" => Ok(SpecialCommand::SwitchMode(ChatMode::Planning)),
        "/mode write" | "/write" => Ok(SpecialCommand::SwitchMode(ChatMode::Write)),

        // Handle /mode with no argument or invalid argument
        "/mode" => Err(CommandError::MissingArgument {
            command: "/mode".to_string(),
            usage: "/mode <planning|write>".to_string(),
        }),
        input if input.starts_with("/mode ") => {
            let arg = input[6..].trim();
            Err(CommandError::UnsupportedArgument {
                command: "/mode".to_string(),
                arg: arg.to_string(),
            })
        }

        // Safety mode switching
        "/safe" | "/safety on" => Ok(SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm)),
        "/yolo" | "/safety off" => Ok(SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm)),

        // Handle /safety with no argument or invalid argument
        "/safety" => Err(CommandError::MissingArgument {
            command: "/safety".to_string(),
            usage: "/safety <on|off>".to_string(),
        }),
        input if input.starts_with("/safety ") => {
            let arg = input[8..].trim();
            if arg != "on" && arg != "off" {
                Err(CommandError::UnsupportedArgument {
                    command: "/safety".to_string(),
                    arg: arg.to_string(),
                })
            } else {
                // Should not reach here due to earlier matches
                Ok(SpecialCommand::None)
            }
        }

        // Status and help
        "/status" => Ok(SpecialCommand::ShowStatus),
        "/help" | "/?" => Ok(SpecialCommand::Help),
        "/mentions" => Ok(SpecialCommand::Mentions),

        // Model management commands and provider auth
        "/models" => Ok(SpecialCommand::ModelsHelp),
        "/models list" => Ok(SpecialCommand::ListModels),

        // Handle /models info with model name
        input if input.starts_with("/models info ") => {
            let model_name = input[13..].trim();
            if model_name.is_empty() {
                Err(CommandError::MissingArgument {
                    command: "/models info".to_string(),
                    usage: "/models info <model_name>".to_string(),
                })
            } else {
                Ok(SpecialCommand::ShowModelInfo(model_name.to_string()))
            }
        }

        // Handle /models info without model name
        "/models info" => Err(CommandError::MissingArgument {
            command: "/models info".to_string(),
            usage: "/models info <model_name>".to_string(),
        }),

        // Handle /models with invalid subcommand
        input if input.starts_with("/models ") => {
            let rest = input[8..].trim();
            let subcommand = rest.split_whitespace().next().unwrap_or(rest);
            if subcommand != "list" && subcommand != "info" {
                Err(CommandError::UnsupportedArgument {
                    command: "/models".to_string(),
                    arg: subcommand.to_string(),
                })
            } else {
                // Should not reach here due to earlier matches
                Ok(SpecialCommand::None)
            }
        }

        "/context" | "/context info" => Ok(SpecialCommand::ContextInfo),

        // Handle /context summary with optional model parameter
        input if input.starts_with("/context summary") => {
            let rest = input[16..].trim();

            // Parse optional model parameter: --model <name> or -m <name>
            let model = if rest.is_empty() {
                None
            } else if let Some(after_flag) = rest.strip_prefix("--model") {
                let after_flag = after_flag.trim();
                if after_flag.is_empty() {
                    return Err(CommandError::MissingArgument {
                        command: "/context summary".to_string(),
                        usage: "/context summary [--model <model_name>]".to_string(),
                    });
                }
                Some(after_flag.to_string())
            } else if let Some(after_flag) = rest.strip_prefix("-m") {
                let after_flag = after_flag.trim();
                if after_flag.is_empty() {
                    return Err(CommandError::MissingArgument {
                        command: "/context summary".to_string(),
                        usage: "/context summary [-m <model_name>]".to_string(),
                    });
                }
                Some(after_flag.to_string())
            } else {
                return Err(CommandError::UnsupportedArgument {
                    command: "/context summary".to_string(),
                    arg: rest.to_string(),
                });
            };

            Ok(SpecialCommand::ContextSummary { model })
        }

        // Handle invalid /context subcommands
        input if input.starts_with("/context ") => {
            let rest = input[9..].trim();
            let subcommand = rest.split_whitespace().next().unwrap_or(rest);
            Err(CommandError::UnsupportedArgument {
                command: "/context".to_string(),
                arg: subcommand.to_string(),
            })
        }
        "/auth" => Ok(SpecialCommand::Auth(None)),
        input if input.starts_with("/auth ") => {
            let rest = input[6..].trim();
            if !rest.is_empty() {
                Ok(SpecialCommand::Auth(Some(rest.to_string())))
            } else {
                Err(CommandError::MissingArgument {
                    command: "/auth".to_string(),
                    usage: "/auth [provider]".to_string(),
                })
            }
        }

        // Model switching with arguments
        "/model" => Err(CommandError::MissingArgument {
            command: "/model".to_string(),
            usage: "/model <model_name>".to_string(),
        }),
        input if input.starts_with("/model ") => {
            let rest = input[7..].trim();
            if rest.is_empty() {
                Err(CommandError::MissingArgument {
                    command: "/model".to_string(),
                    usage: "/model <model_name>".to_string(),
                })
            } else {
                Ok(SpecialCommand::SwitchModel(rest.to_string()))
            }
        }

        // Subagent delegation commands
        "/subagents" => Ok(SpecialCommand::ToggleSubagents(true)),
        "/subagents on" | "/subagents enable" => Ok(SpecialCommand::ToggleSubagents(true)),
        "/subagents off" | "/subagents disable" => Ok(SpecialCommand::ToggleSubagents(false)),

        // Handle /subagents with invalid argument
        input if input.starts_with("/subagents ") => {
            let arg = input[11..].trim();
            Err(CommandError::UnsupportedArgument {
                command: "/subagents".to_string(),
                arg: arg.to_string(),
            })
        }

        // Exit commands
        "exit" | "quit" | "/exit" | "/quit" => Ok(SpecialCommand::Exit),

        // Unknown command starting with "/"
        input if input.starts_with('/') => {
            let cmd = input.split_whitespace().next().unwrap_or(input);
            Err(CommandError::UnknownCommand(cmd.to_string()))
        }

        // Not a special command
        _ => Ok(SpecialCommand::None),
    }
}

/// Display help text for special commands
///
/// Shows all available special commands with their descriptions
/// and usage examples.
///
/// # Examples
///
/// ```
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

SUBAGENT DELEGATION:
  /subagents      - Show subagent enablement status
  /subagents on   - Enable subagent delegation
  /subagents off  - Disable subagent delegation
  /subagents enable  - Same as /subagents on
  /subagents disable - Same as /subagents off

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
  - Subagents allow delegating tasks to separate agent instances
  - Mention "subagent", "delegate", or "parallel agent" in your prompt to auto-enable subagents
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
/// ```
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
/// ```
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
        assert_eq!(
            parse_special_command("/models").unwrap(),
            SpecialCommand::ModelsHelp
        );
    }

    #[test]
    fn test_parse_special_command_models_list_returns_list_models() {
        assert_eq!(
            parse_special_command("/models list").unwrap(),
            SpecialCommand::ListModels
        );
    }

    #[test]
    fn test_parse_switch_mode_planning() {
        let cmd = parse_special_command("/mode planning").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_switch_mode_planning_shorthand() {
        let cmd = parse_special_command("/planning").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_switch_mode_write() {
        let cmd = parse_special_command("/mode write").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Write));
    }

    #[test]
    fn test_parse_switch_mode_write_shorthand() {
        let cmd = parse_special_command("/write").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Write));
    }

    #[test]
    fn test_parse_auth_without_provider() {
        let cmd = parse_special_command("/auth").unwrap();
        assert_eq!(cmd, SpecialCommand::Auth(None));
    }

    #[test]
    fn test_parse_auth_with_provider() {
        let cmd = parse_special_command("/auth copilot").unwrap();
        assert_eq!(cmd, SpecialCommand::Auth(Some("copilot".to_string())));
    }

    #[test]
    fn test_parse_switch_safety_always_confirm() {
        let cmd = parse_special_command("/safe").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm));
    }

    #[test]
    fn test_parse_switch_safety_always_confirm_alt() {
        let cmd = parse_special_command("/safety on").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm));
    }

    #[test]
    fn test_parse_switch_safety_never_confirm() {
        let cmd = parse_special_command("/yolo").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
    }

    #[test]
    fn test_parse_switch_safety_never_confirm_alt() {
        let cmd = parse_special_command("/safety off").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm));
    }

    #[test]
    fn test_parse_show_status() {
        let cmd = parse_special_command("/status").unwrap();
        assert_eq!(cmd, SpecialCommand::ShowStatus);
    }

    #[test]
    fn test_parse_help() {
        let cmd = parse_special_command("/help").unwrap();
        assert_eq!(cmd, SpecialCommand::Help);
    }

    #[test]
    fn test_parse_help_shorthand() {
        let cmd = parse_special_command("/?").unwrap();
        assert_eq!(cmd, SpecialCommand::Help);
    }

    #[test]
    fn test_parse_exit() {
        let cmd = parse_special_command("exit").unwrap();
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_exit_with_slash() {
        let cmd = parse_special_command("/exit").unwrap();
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_quit() {
        let cmd = parse_special_command("quit").unwrap();
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_quit_with_slash() {
        let cmd = parse_special_command("/quit").unwrap();
        assert_eq!(cmd, SpecialCommand::Exit);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(
            parse_special_command("/MODE PLANNING").unwrap(),
            SpecialCommand::SwitchMode(ChatMode::Planning)
        );
        assert_eq!(
            parse_special_command("/WRITE").unwrap(),
            SpecialCommand::SwitchMode(ChatMode::Write)
        );
        assert_eq!(
            parse_special_command("/SAFE").unwrap(),
            SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm)
        );
        assert_eq!(
            parse_special_command("/YOLO").unwrap(),
            SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm)
        );
    }

    #[test]
    fn test_parse_with_whitespace() {
        let cmd = parse_special_command("  /mode planning  ").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchMode(ChatMode::Planning));
    }

    #[test]
    fn test_parse_regular_text_returns_none() {
        let cmd = parse_special_command("hello agent").unwrap();
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_partial_command_returns_none() {
        let result = parse_special_command("/mod");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_mode_returns_none() {
        let result = parse_special_command("/mode invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_string_returns_none() {
        let cmd = parse_special_command("").unwrap();
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_whitespace_only_returns_none() {
        let cmd = parse_special_command("   ").unwrap();
        assert_eq!(cmd, SpecialCommand::None);
    }

    #[test]
    fn test_parse_random_command_returns_none() {
        let result = parse_special_command("/randomcommand");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_mentions() {
        let cmd = parse_special_command("/mentions").unwrap();
        assert_eq!(cmd, SpecialCommand::Mentions);
    }

    #[test]
    fn test_parse_list_models() {
        let cmd = parse_special_command("/models list").unwrap();
        assert_eq!(cmd, SpecialCommand::ListModels);
    }

    #[test]
    fn test_parse_switch_model() {
        let cmd = parse_special_command("/model gpt-4").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_switch_model_with_hyphen() {
        let cmd = parse_special_command("/model gemini-2.0").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchModel("gemini-2.0".to_string()));
    }

    #[test]
    fn test_parse_switch_model_case_insensitive() {
        let cmd = parse_special_command("/MODEL gpt-4").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_context_info() {
        let cmd = parse_special_command("/context").unwrap();
        assert_eq!(cmd, SpecialCommand::ContextInfo);
    }

    #[test]
    fn test_parse_context_info_explicit() {
        let cmd = parse_special_command("/context info").unwrap();
        assert_eq!(cmd, SpecialCommand::ContextInfo);
    }

    #[test]
    fn test_parse_context_summary_no_model() {
        let cmd = parse_special_command("/context summary").unwrap();
        assert_eq!(cmd, SpecialCommand::ContextSummary { model: None });
    }

    #[test]
    fn test_parse_context_summary_with_model_long_flag() {
        let cmd = parse_special_command("/context summary --model gpt-4").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::ContextSummary {
                model: Some("gpt-4".to_string())
            }
        );
    }

    #[test]
    fn test_parse_context_summary_with_model_short_flag() {
        let cmd = parse_special_command("/context summary -m claude-3").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::ContextSummary {
                model: Some("claude-3".to_string())
            }
        );
    }

    #[test]
    fn test_parse_context_summary_with_complex_model_name() {
        let cmd = parse_special_command("/context summary --model gpt-4-turbo-preview").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::ContextSummary {
                model: Some("gpt-4-turbo-preview".to_string())
            }
        );
    }

    #[test]
    fn test_parse_context_summary_invalid_flag() {
        let result = parse_special_command("/context summary --invalid");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/context summary");
            assert_eq!(arg, "--invalid");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }

    #[test]
    fn test_parse_context_summary_flag_no_model() {
        let result = parse_special_command("/context summary --model");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, .. }) = result {
            assert_eq!(command, "/context summary");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_context_summary_short_flag_no_model() {
        let result = parse_special_command("/context summary -m");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, .. }) = result {
            assert_eq!(command, "/context summary");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_context_invalid_subcommand() {
        let result = parse_special_command("/context invalid");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/context");
            assert_eq!(arg, "invalid");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }

    #[test]
    fn test_parse_model_command_no_args_returns_error() {
        let result = parse_special_command("/model");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, .. }) = result {
            assert_eq!(command, "/model");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_model_command_with_spaces() {
        let cmd = parse_special_command("/model   gpt-4  ").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchModel("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_model_info_not_supported() {
        let cmd = parse_special_command("/model info").unwrap();
        assert_eq!(cmd, SpecialCommand::SwitchModel("info".to_string()));
    }

    #[test]
    fn test_parse_unknown_command_returns_error() {
        let result = parse_special_command("/foo");
        assert!(result.is_err());
        if let Err(CommandError::UnknownCommand(cmd)) = result {
            assert_eq!(cmd, "/foo");
        } else {
            panic!("Expected UnknownCommand error");
        }
    }

    #[test]
    fn test_parse_unsupported_mode_arg_returns_error() {
        let result = parse_special_command("/mode invalid");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/mode");
            assert_eq!(arg, "invalid");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }

    #[test]
    fn test_parse_mode_no_arg_returns_error() {
        let result = parse_special_command("/mode");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, usage }) = result {
            assert_eq!(command, "/mode");
            assert_eq!(usage, "/mode <planning|write>");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_safety_no_arg_returns_error() {
        let result = parse_special_command("/safety");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, usage }) = result {
            assert_eq!(command, "/safety");
            assert_eq!(usage, "/safety <on|off>");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_safety_invalid_arg_returns_error() {
        let result = parse_special_command("/safety maybe");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/safety");
            assert_eq!(arg, "maybe");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }

    #[test]
    fn test_parse_models_invalid_subcommand_returns_error() {
        let result = parse_special_command("/models invalid");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/models");
            assert_eq!(arg, "invalid");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }

    #[test]
    fn test_parse_models_info_with_model_name() {
        let cmd = parse_special_command("/models info gpt-4").unwrap();
        assert_eq!(cmd, SpecialCommand::ShowModelInfo("gpt-4".to_string()));
    }

    #[test]
    fn test_parse_models_info_without_model_name() {
        let result = parse_special_command("/models info");
        assert!(result.is_err());
        if let Err(CommandError::MissingArgument { command, usage }) = result {
            assert_eq!(command, "/models info");
            assert_eq!(usage, "/models info <model_name>");
        } else {
            panic!("Expected MissingArgument error");
        }
    }

    #[test]
    fn test_parse_models_info_with_complex_model_name() {
        let cmd = parse_special_command("/models info gpt-5-mini").unwrap();
        assert_eq!(cmd, SpecialCommand::ShowModelInfo("gpt-5-mini".to_string()));
    }

    #[test]
    fn test_parse_subagents_toggle() {
        let cmd = parse_special_command("/subagents").unwrap();
        assert_eq!(cmd, SpecialCommand::ToggleSubagents(true));
    }

    #[test]
    fn test_parse_subagents_on() {
        let cmd = parse_special_command("/subagents on").unwrap();
        assert_eq!(cmd, SpecialCommand::ToggleSubagents(true));
    }

    #[test]
    fn test_parse_subagents_enable() {
        let cmd = parse_special_command("/subagents enable").unwrap();
        assert_eq!(cmd, SpecialCommand::ToggleSubagents(true));
    }

    #[test]
    fn test_parse_subagents_off() {
        let cmd = parse_special_command("/subagents off").unwrap();
        assert_eq!(cmd, SpecialCommand::ToggleSubagents(false));
    }

    #[test]
    fn test_parse_subagents_disable() {
        let cmd = parse_special_command("/subagents disable").unwrap();
        assert_eq!(cmd, SpecialCommand::ToggleSubagents(false));
    }

    #[test]
    fn test_parse_subagents_invalid_arg() {
        let result = parse_special_command("/subagents invalid");
        assert!(result.is_err());
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/subagents");
            assert_eq!(arg, "invalid");
        } else {
            panic!("Expected UnsupportedArgument error");
        }
    }
}
