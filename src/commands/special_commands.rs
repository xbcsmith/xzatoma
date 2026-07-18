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
//! The unified command contract:
//! - Typing a command bare (e.g. `/mode`) returns per-command help.
//! - Typing `/<command> status` returns the current state for that command.
//! - Typing `/<command> <action>` performs the action.
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

    /// Display the list of tools available to the agent in this session
    ///
    /// Shows every tool the agent can currently invoke.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let cmd = parse_special_command("/tools").unwrap();
    /// assert_eq!(cmd, SpecialCommand::ListTools);
    /// ```
    ListTools,

    /// Display the list of active skills loaded for this workspace
    ///
    /// Shows every skill currently disclosed to the agent for this workspace.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let cmd = parse_special_command("/skills").unwrap();
    /// assert_eq!(cmd, SpecialCommand::ListSkills);
    /// ```
    ListSkills,

    /// Display connected MCP servers and the tools they expose
    ///
    /// Shows each connected MCP server along with the tools it makes
    /// available to the agent.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let cmd = parse_special_command("/mcp").unwrap();
    /// assert_eq!(cmd, SpecialCommand::ShowMcpStatus);
    /// ```
    ShowMcpStatus,

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

    /// Set or replace the active system prompt for this chat session.
    ///
    /// Replaces the first system message in the conversation history.
    /// Skill disclosure messages that follow it are not affected.
    /// Use `/system <text>` to provide the new prompt text.
    SetSystemPrompt(String),

    /// Toggle live streaming of model output tokens.
    ///
    /// When enabled, response and reasoning tokens are printed to the
    /// terminal as they arrive. When disabled, the full response is
    /// printed only after the model finishes.
    ///
    /// Use `/streaming on` or `/streaming enable` to enable.
    /// Use `/streaming off` or `/streaming disable` to disable.
    ToggleStreaming(bool),

    /// Show help text for the `/mode` command.
    ///
    /// Emitted when the user types `/mode` with no argument, displaying
    /// all available chat modes, their aliases, and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/mode").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowModeHelp);
    /// ```
    ShowModeHelp,

    /// Show the currently active chat mode.
    ///
    /// Emitted when the user types `/mode status`, reporting the active
    /// chat mode without modifying it.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/mode status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowModeStatus);
    /// ```
    ShowModeStatus,

    /// Show help text for the `/safety` command.
    ///
    /// Emitted when the user types `/safety` with no argument, displaying
    /// all available safety policies, their aliases, and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/safety").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSafetyHelp);
    /// ```
    ShowSafetyHelp,

    /// Show the currently active safety confirmation policy.
    ///
    /// Emitted when the user types `/safety status`, reporting the active
    /// confirmation policy without modifying it.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/safety status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSafetyStatus);
    /// ```
    ShowSafetyStatus,

    /// Show help text for the `/model` command.
    ///
    /// Emitted when the user types `/model` with no argument, displaying
    /// the model-switching syntax and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/model").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowModelHelp);
    /// ```
    ShowModelHelp,

    /// Show the currently active AI model name.
    ///
    /// Emitted when the user types `/model status`, reporting the active
    /// model name without switching to a different model.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/model status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowModelStatus);
    /// ```
    ShowModelStatus,

    /// Show help text for the `/streaming` command.
    ///
    /// Emitted when the user types `/streaming` with no argument, displaying
    /// all valid toggle values, their aliases, and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/streaming").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowStreamingHelp);
    /// ```
    ShowStreamingHelp,

    /// Show the current token-streaming setting.
    ///
    /// Emitted when the user types `/streaming status`, reporting whether
    /// streaming is enabled or disabled without changing the setting.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/streaming status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowStreamingStatus);
    /// ```
    ShowStreamingStatus,

    /// Show help text for the `/system` command.
    ///
    /// Emitted when the user types `/system` with no argument, displaying
    /// the system-prompt syntax and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/system").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSystemHelp);
    /// ```
    ShowSystemHelp,

    /// Show the active system prompt text.
    ///
    /// Emitted when the user types `/system status`, displaying the current
    /// system prompt without replacing it.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/system status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSystemStatus);
    /// ```
    ShowSystemStatus,

    /// Show help text for the `/subagents` command.
    ///
    /// Emitted when the user types `/subagents` with no argument, displaying
    /// all valid toggle values, their aliases, and usage instructions.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/subagents").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSubagentsHelp);
    /// ```
    ShowSubagentsHelp,

    /// Show the current subagent delegation setting.
    ///
    /// Emitted when the user types `/subagents status`, reporting whether
    /// subagent delegation is enabled or disabled without changing the setting.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::special_commands::{parse_special_command, SpecialCommand};
    ///
    /// let result = parse_special_command("/subagents status").unwrap();
    /// assert_eq!(result, SpecialCommand::ShowSubagentsStatus);
    /// ```
    ShowSubagentsStatus,

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
/// The unified command contract is:
/// - Typing a command bare (e.g. `/mode`) returns the per-command help variant.
/// - Typing `/<command> status` returns the per-command status variant.
/// - Typing `/<command> <action>` performs the action.
///
/// # Arguments
///
/// * `input` - The user input string to parse
///
/// # Returns
///
/// Returns `Ok(SpecialCommand)` for valid commands or `SpecialCommand::None` for
/// non-commands. Returns `Err(CommandError)` for unknown commands or invalid
/// arguments.
///
/// # Errors
///
/// Returns `CommandError::UnknownCommand` if input starts with `/` but is not a
/// valid command.
/// Returns `CommandError::UnsupportedArgument` if a command receives an
/// unrecognised argument.
///
/// # Command Examples
///
/// Chat mode switching:
/// - `/mode` - Show mode help
/// - `/mode status` - Show the currently active mode
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
/// - `/system status` - Show the active system prompt
/// - `/system <text>` - Set the active system prompt
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
/// let cmd = parse_special_command("/mode").unwrap();
/// assert_eq!(cmd, SpecialCommand::ShowModeHelp);
///
/// let cmd = parse_special_command("hello agent").unwrap();
/// assert_eq!(cmd, SpecialCommand::None);
///
/// // Unknown command returns error
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

        // Handle /mode with no argument, status subcommand, or invalid argument
        "/mode" => Ok(SpecialCommand::ShowModeHelp),
        "/mode status" => Ok(SpecialCommand::ShowModeStatus),
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

        // Handle /safety with no argument, status subcommand, or invalid argument
        "/safety" => Ok(SpecialCommand::ShowSafetyHelp),
        "/safety status" => Ok(SpecialCommand::ShowSafetyStatus),
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
        "/tools" => Ok(SpecialCommand::ListTools),
        "/skills" => Ok(SpecialCommand::ListSkills),
        "/mcp" => Ok(SpecialCommand::ShowMcpStatus),
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
        "/model" => Ok(SpecialCommand::ShowModelHelp),
        "/model status" => Ok(SpecialCommand::ShowModelStatus),
        input if input.starts_with("/model ") => {
            let rest = input[7..].trim();
            Ok(SpecialCommand::SwitchModel(rest.to_string()))
        }

        // Subagent delegation commands
        "/subagents on" | "/subagents enable" => Ok(SpecialCommand::ToggleSubagents(true)),
        "/subagents off" | "/subagents disable" => Ok(SpecialCommand::ToggleSubagents(false)),
        "/subagents status" => Ok(SpecialCommand::ShowSubagentsStatus),

        // Handle /subagents with invalid argument
        input if input.starts_with("/subagents ") => {
            let arg = input[11..].trim();
            Err(CommandError::UnsupportedArgument {
                command: "/subagents".to_string(),
                arg: arg.to_string(),
            })
        }

        "/subagents" => Ok(SpecialCommand::ShowSubagentsHelp),

        // Set system prompt: preserve original case of the prompt text
        "/system" => Ok(SpecialCommand::ShowSystemHelp),
        "/system status" => Ok(SpecialCommand::ShowSystemStatus),
        input if input.starts_with("/system ") => {
            // Use `trimmed` (not `lower`) to preserve the original case of the text
            let text = trimmed[8..].trim();
            if text.is_empty() {
                Ok(SpecialCommand::ShowSystemHelp)
            } else {
                Ok(SpecialCommand::SetSystemPrompt(text.to_string()))
            }
        }

        // Streaming toggle commands
        "/streaming on" | "/streaming enable" => Ok(SpecialCommand::ToggleStreaming(true)),
        "/streaming off" | "/streaming disable" => Ok(SpecialCommand::ToggleStreaming(false)),

        // Handle /streaming with no argument, status subcommand, or invalid argument
        "/streaming" => Ok(SpecialCommand::ShowStreamingHelp),
        "/streaming status" => Ok(SpecialCommand::ShowStreamingStatus),
        input if input.starts_with("/streaming ") => {
            let arg = input[11..].trim();
            Err(CommandError::UnsupportedArgument {
                command: "/streaming".to_string(),
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
    println!("{}", format_help_text());
}

/// Build the help text for special commands as an owned string.
///
/// This is the string-returning equivalent of [`print_help`], used by
/// callers such as the stdio ACP agent that must not write directly to
/// stdout (stdout is the JSON-RPC wire channel in that context).
///
/// # Returns
///
/// Returns the full special-commands help text.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_help_text;
///
/// let text = format_help_text();
/// assert!(text.contains("/help"));
/// ```
pub fn format_help_text() -> String {
    r#"
Special Commands for Interactive Chat Mode
===========================================

Type any command alone for per-command help. Add `status` to see the current value.

CHAT MODE SWITCHING:
  /mode           - Show mode help and available modes
  /mode status    - Show the currently active mode
  /mode planning  - Switch to Planning mode (read-only)
  /planning       - Shorthand for /mode planning
  /mode write     - Switch to Write mode (read/write)
  /write          - Shorthand for /mode write

SAFETY MODE SWITCHING:
  /safety         - Show safety help and available policies
  /safety status  - Show the currently active safety policy
  /safe           - Enable safety mode (require confirmations)
  /safety on      - Same as /safe
  /yolo           - Disable safety mode (YOLO mode)
  /safety off     - Same as /yolo

SUBAGENT DELEGATION:
  /subagents         - Show subagent help and current state
  /subagents status  - Show the current subagent setting
  /subagents on      - Enable subagent delegation
  /subagents off     - Disable subagent delegation
  /subagents enable  - Same as /subagents on
  /subagents disable - Same as /subagents off

CONTEXT MENTIONS (Quick Reference):
  @file.rs              - Include file contents
  @file.rs#L10-20       - Include specific lines
  @path/to/dir          - List directory contents
  @search:"pattern"     - Search for literal text
  @grep:"regex"         - Search with regex patterns
  @url:https://...      - Include web content

MODEL MANAGEMENT:
  /model          - Show model help
  /model status   - Show currently active model
  /model <name>   - Switch to a different model
  /models         - Show help for models subcommands and flags
  /models list    - Show available models from current provider
  /models info <name> - Show detailed info about a specific model
  /auth [provider] - Start authentication for the provider; use `/auth` for the configured provider

CONTEXT WINDOW MANAGEMENT:
  /context info              - Show context window usage and token statistics
  /context summary           - Summarize conversation and reset context window
  /context summary -m MODEL  - Summarize using a specific model (for cost optimization)

SYSTEM PROMPT:
  /system         - Show system prompt help
  /system status  - Show the active system prompt text
  /system <text>  - Replace the active system prompt for this session
                    (replaces the first system message; skill disclosures are kept)

STREAMING:
  /streaming         - Show streaming help
  /streaming status  - Show the current streaming setting
  /streaming on      - Enable live token streaming to terminal
  /streaming off     - Disable live token streaming
  /streaming enable  - Same as /streaming on
  /streaming disable - Same as /streaming off

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
  - Type a command alone for per-command help; add `status` to inspect the current value
  - Regular text (not starting with /) is sent to the agent
  - Mentions (@file, @search, etc.) inject context into prompts
  - Switching to Write mode enables powerful file and terminal tools
  - Use /safe in Write mode to require confirmation for dangerous operations
  - Subagents allow delegating tasks to separate agent instances
  - Mention "subagent", "delegate", or "parallel agent" in your prompt to auto-enable subagents
  - See /mentions for complete mention syntax and examples
"#
    .to_string()
}

/// Return the help text for the `/models` command as a `String`.
///
/// Contains usage, flags, and examples for `/models` subcommands such as
/// `/models list` and `/models info <name>`. Use this function when the
/// text needs to be captured or tested; use `print_models_help` when the
/// text should be written directly to stdout.
///
/// # Returns
///
/// A `String` containing the full models help text.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_models_help_text;
///
/// let text = format_models_help_text();
/// assert!(!text.is_empty());
/// ```
pub fn format_models_help_text() -> String {
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
    .to_string()
}

/// Display detailed help for the `/models` command.
///
/// Writes the output of `format_models_help_text` to stdout. Shows usage,
/// flags, and examples for `/models` subcommands such as `/models list`
/// and `/models info <name>`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::print_models_help;
///
/// print_models_help();
/// ```
pub fn print_models_help() {
    println!("{}", format_models_help_text());
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
    println!("{}", format_mention_help_text());
}

/// Build the context-mention help text as an owned string.
///
/// This is the string-returning equivalent of [`print_mention_help`], used
/// by callers such as the stdio ACP agent that must not write directly to
/// stdout (stdout is the JSON-RPC wire channel in that context).
///
/// # Returns
///
/// Returns the full context-mention help text.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_mention_help_text;
///
/// let text = format_mention_help_text();
/// assert!(text.contains("@file"));
/// ```
pub fn format_mention_help_text() -> String {
    r#"
Context Mentions for XZatoma
=============================

Context mentions let you include file contents, directory listings, search results,
and web content in your prompts. Use @mention syntax to reference relevant information.

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

DIRECTORY MENTIONS
==================
Include a recursive listing of a directory's contents.

Syntax:
  @path/to/dir              - List all files and subdirectories
  @src                      - List the src/ directory
  @tmp/output               - List an output directory (or note it is absent)

Examples:
  Write output files to @tmp/output
  What is in @src/tools?
  Summarise the project layout under @docs

Behaviour:
  - If the directory exists: lists all files and subdirectories recursively
    (up to 200 entries), showing file sizes
  - If the directory does not yet exist: injects a note that it will be
    created when the agent writes files there
  - Directories are never cached (always freshly listed)

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

Directory mention shows no files:
  - The directory may be empty — the agent will see "(empty directory)"
  - If the path does not exist yet, the agent sees a "does not exist" note
    and will create it when writing output files
  - Use full relative path: @tmp/output not @/tmp/output (no absolute paths)

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
    .to_string()
}

/// Return help text for the `/mode` command.
///
/// Describes all valid mode subcommands: the bare command for per-command help,
/// the `status` query, and all mode-switching actions with their aliases.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/mode`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_mode_help_text;
///
/// let text = format_mode_help_text();
/// assert!(text.contains("/mode status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_mode_help_text() -> String {
    r#"
/mode - Chat Mode
=================

Controls which tools and capabilities are available in this chat session.

USAGE:
  /mode              - Show this help (you are here)
  /mode status       - Show the currently active mode
  /mode planning     - Switch to Planning mode (read-only file access)
  /planning          - Shorthand for /mode planning
  /mode write        - Switch to Write mode (file read/write and terminal)
  /write             - Shorthand for /mode write

EXAMPLES:
  /mode status
  /mode planning
  /mode write

NOTE: Type /mode alone for this help. Type /mode status to see the active mode.
      Switching to Write mode gives the agent access to file and terminal tools.
"#
    .to_string()
}

/// Return help text for the `/safety` command.
///
/// Describes all valid safety subcommands: the bare command for per-command help,
/// the `status` query, and all policy-switching actions with their aliases.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/safety`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_safety_help_text;
///
/// let text = format_safety_help_text();
/// assert!(text.contains("/safety status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_safety_help_text() -> String {
    r#"
/safety - Safety Confirmation Policy
=====================================

Controls whether the agent requests confirmation before executing
dangerous operations such as terminal commands.

USAGE:
  /safety            - Show this help (you are here)
  /safety status     - Show the currently active safety policy
  /safety on         - Enable safety mode (require confirmation for dangerous ops)
  /safe              - Shorthand for /safety on
  /safety off        - Disable safety mode (no confirmation required)
  /yolo              - Shorthand for /safety off

EXAMPLES:
  /safety status
  /safety on
  /yolo

NOTE: Type /safety alone for this help. Type /safety status to see the active
      policy. Use /safe in Write mode to require confirmation before terminal
      commands.
"#
    .to_string()
}

/// Return help text for the `/model` command.
///
/// Describes all valid model subcommands: the bare command for per-command help,
/// the `status` query, and the model-switching syntax.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/model`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_model_help_text;
///
/// let text = format_model_help_text();
/// assert!(text.contains("/model status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_model_help_text() -> String {
    r#"
/model - Active Model
======================

Shows or changes the AI provider model used for this session.

USAGE:
  /model                  - Show this help (you are here)
  /model status           - Show the currently active model name
  /model <name>           - Switch to a different model
  /models                 - Show help for model-management subcommands
  /models list            - List all available models from the current provider
  /models info <name>     - Show detailed information about a specific model

EXAMPLES:
  /model status
  /model gpt-4o
  /model llama3.2:latest
  /models list

NOTE: Type /model alone for this help. Type /model status to see the active
      model. Use /models list to discover available model names before
      switching.
"#
    .to_string()
}

/// Return help text for the `/streaming` command.
///
/// Describes all valid streaming subcommands: the bare command for per-command
/// help, the `status` query, and all toggle actions with their aliases.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/streaming`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_streaming_help_text;
///
/// let text = format_streaming_help_text();
/// assert!(text.contains("/streaming status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_streaming_help_text() -> String {
    r#"
/streaming - Token Streaming
=============================

Controls whether model response tokens are printed as they arrive or
all at once after the model finishes.

USAGE:
  /streaming             - Show this help (you are here)
  /streaming status      - Show the current streaming setting
  /streaming on          - Enable live token streaming to terminal
  /streaming enable      - Shorthand for /streaming on
  /streaming off         - Disable streaming (print full response when complete)
  /streaming disable     - Shorthand for /streaming off

EXAMPLES:
  /streaming status
  /streaming on
  /streaming off

NOTE: Type /streaming alone for this help. Type /streaming status for the
      current setting. In Zed (ACP mode), streaming is controlled by the Zed
      client; /streaming on|off has no effect in that environment. In terminal
      chat mode, streaming shows tokens in real time as the model generates
      them.
"#
    .to_string()
}

/// Return help text for the `/system` command.
///
/// Describes all valid system subcommands: the bare command for per-command
/// help, the `status` query, and the system-prompt replacement syntax.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/system`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_system_help_text;
///
/// let text = format_system_help_text();
/// assert!(text.contains("/system status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_system_help_text() -> String {
    r#"
/system - System Prompt
========================

Shows or replaces the active system prompt for this session.

USAGE:
  /system                - Show this help (you are here)
  /system status         - Show the currently active system prompt text
  /system <text>         - Replace the active system prompt with <text>

EXAMPLES:
  /system status
  /system You are a concise code reviewer. Be brief.
  /system Act as a senior Rust engineer reviewing pull requests.

NOTE: Type /system alone for this help. Type /system status to see the
      active prompt without replacing it. The command replaces the first
      system message in the conversation history; skill disclosure messages
      that follow it are preserved unchanged.
"#
    .to_string()
}

/// Return help text for the `/subagents` command.
///
/// Describes all valid subagents subcommands: the bare command for per-command
/// help, the `status` query, and all delegation-toggle actions with aliases.
///
/// # Returns
///
/// A `String` containing formatted usage instructions for `/subagents`.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::special_commands::format_subagents_help_text;
///
/// let text = format_subagents_help_text();
/// assert!(text.contains("/subagents status"));
/// assert!(text.contains("USAGE:"));
/// ```
pub fn format_subagents_help_text() -> String {
    r#"
/subagents - Subagent Delegation
==================================

Controls whether the agent can delegate tasks to separate agent instances.

USAGE:
  /subagents            - Show this help (you are here)
  /subagents status     - Show the current subagent delegation state
  /subagents on         - Enable subagent delegation
  /subagents enable     - Shorthand for /subagents on
  /subagents off        - Disable subagent delegation
  /subagents disable    - Shorthand for /subagents off

EXAMPLES:
  /subagents status
  /subagents on
  /subagents off

NOTE: Type /subagents alone for this help. Type /subagents status for the
      current state. When enabled, mentioning "subagent", "delegate", or
      "parallel agent" in your prompt auto-enables delegation for complex
      multi-step tasks.
"#
    .to_string()
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
    fn test_parse_special_command_tools_returns_list_tools() {
        assert_eq!(
            parse_special_command("/tools"),
            Ok(SpecialCommand::ListTools)
        );
    }

    #[test]
    fn test_parse_special_command_skills_returns_list_skills() {
        assert_eq!(
            parse_special_command("/skills"),
            Ok(SpecialCommand::ListSkills)
        );
    }

    #[test]
    fn test_parse_special_command_mcp_returns_show_mcp_status() {
        assert_eq!(
            parse_special_command("/mcp"),
            Ok(SpecialCommand::ShowMcpStatus)
        );
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
    fn test_parse_model_bare_returns_show_model_help() {
        assert_eq!(
            parse_special_command("/model").unwrap(),
            SpecialCommand::ShowModelHelp
        );
    }

    #[test]
    fn test_parse_model_status_returns_show_model_status() {
        assert_eq!(
            parse_special_command("/model status").unwrap(),
            SpecialCommand::ShowModelStatus
        );
    }

    #[test]
    fn test_parse_model_status_not_treated_as_model_name() {
        // "/model status" must route to ShowModelStatus, not SwitchModel("status")
        let result = parse_special_command("/model status").unwrap();
        assert_eq!(result, SpecialCommand::ShowModelStatus);
        assert_ne!(result, SpecialCommand::SwitchModel("status".to_string()));
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
    fn test_parse_mode_bare_returns_show_mode_help() {
        assert_eq!(
            parse_special_command("/mode").unwrap(),
            SpecialCommand::ShowModeHelp
        );
    }

    #[test]
    fn test_parse_mode_status_returns_show_mode_status() {
        assert_eq!(
            parse_special_command("/mode status").unwrap(),
            SpecialCommand::ShowModeStatus
        );
    }

    #[test]
    fn test_parse_safety_bare_returns_show_safety_help() {
        assert_eq!(
            parse_special_command("/safety").unwrap(),
            SpecialCommand::ShowSafetyHelp
        );
    }

    #[test]
    fn test_parse_safety_status_returns_show_safety_status() {
        assert_eq!(
            parse_special_command("/safety status").unwrap(),
            SpecialCommand::ShowSafetyStatus
        );
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
        let cmd = parse_special_command("/models info gpt-5.3-codex").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::ShowModelInfo("gpt-5.3-codex".to_string())
        );
    }

    #[test]
    fn test_parse_subagents_bare_returns_show_subagents_help() {
        assert_eq!(
            parse_special_command("/subagents").unwrap(),
            SpecialCommand::ShowSubagentsHelp
        );
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

    #[test]
    fn test_parse_set_system_prompt_with_text() {
        let cmd = parse_special_command("/system you are a pirate captain").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::SetSystemPrompt("you are a pirate captain".to_string())
        );
    }

    #[test]
    fn test_parse_set_system_prompt_preserves_original_case() {
        let cmd = parse_special_command("/system You Are A Helpful ASSISTANT").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::SetSystemPrompt("You Are A Helpful ASSISTANT".to_string())
        );
    }

    #[test]
    fn test_parse_system_bare_returns_show_system_help() {
        assert_eq!(
            parse_special_command("/system").unwrap(),
            SpecialCommand::ShowSystemHelp
        );
    }

    #[test]
    fn test_parse_system_whitespace_only_after_system_returns_help() {
        // "/system   " trims to "/system" -- must return ShowSystemHelp, not an error
        assert_eq!(
            parse_special_command("/system   ").unwrap(),
            SpecialCommand::ShowSystemHelp
        );
    }

    #[test]
    fn test_parse_set_system_prompt_with_leading_whitespace_after_command() {
        // "/system  text" (two spaces) -- trimmed text is "text"
        let cmd = parse_special_command("/system  only speak in pirate").unwrap();
        // The text after "/system " (one space, 8 chars total) is " only speak in pirate",
        // and .trim() removes the leading space.
        assert_eq!(
            cmd,
            SpecialCommand::SetSystemPrompt("only speak in pirate".to_string())
        );
    }

    #[test]
    fn test_parse_set_system_prompt_multiword_text() {
        let cmd =
            parse_special_command("/system act as a senior Rust engineer reviewing code").unwrap();
        assert_eq!(
            cmd,
            SpecialCommand::SetSystemPrompt(
                "act as a senior Rust engineer reviewing code".to_string()
            )
        );
    }

    #[test]
    fn test_parse_streaming_on_returns_toggle_streaming_true() {
        let result = parse_special_command("/streaming on").unwrap();
        assert_eq!(result, SpecialCommand::ToggleStreaming(true));
    }

    #[test]
    fn test_parse_streaming_off_returns_toggle_streaming_false() {
        let result = parse_special_command("/streaming off").unwrap();
        assert_eq!(result, SpecialCommand::ToggleStreaming(false));
    }

    #[test]
    fn test_parse_streaming_enable_alias() {
        let result = parse_special_command("/streaming enable").unwrap();
        assert_eq!(result, SpecialCommand::ToggleStreaming(true));
    }

    #[test]
    fn test_parse_streaming_disable_alias() {
        let result = parse_special_command("/streaming disable").unwrap();
        assert_eq!(result, SpecialCommand::ToggleStreaming(false));
    }

    #[test]
    fn test_parse_streaming_bare_returns_show_streaming_help() {
        assert_eq!(
            parse_special_command("/streaming").unwrap(),
            SpecialCommand::ShowStreamingHelp
        );
    }

    #[test]
    fn test_parse_streaming_invalid_arg_returns_unsupported_argument_error() {
        let result = parse_special_command("/streaming maybe");
        if let Err(CommandError::UnsupportedArgument { command, arg }) = result {
            assert_eq!(command, "/streaming");
            assert_eq!(arg, "maybe");
        } else {
            panic!(
                "expected UnsupportedArgument {{ command: \"/streaming\", arg: \"maybe\" }}, got {:?}",
                result
            );
        }
    }

    #[test]
    fn test_format_help_text_matches_print_help_content() {
        let text = format_help_text();
        assert!(!text.is_empty());
        assert!(text.contains("Special Commands for Interactive Chat Mode"));
        assert!(text.contains("CHAT MODE SWITCHING"));
        assert!(text.contains("SESSION INFORMATION"));
        assert!(text.contains("/help"));
        assert!(text.contains("/mentions"));
    }

    #[test]
    fn test_format_mention_help_text_contains_mention_syntax() {
        let text = format_mention_help_text();
        assert!(!text.is_empty());
        assert!(text.contains("Context Mentions for XZatoma"));
        assert!(text.contains("@file"));
        assert!(text.contains("@search"));
        assert!(text.contains("@grep"));
        assert!(text.contains("@url"));
    }

    #[test]
    fn test_parse_streaming_status_returns_show_streaming_status() {
        assert_eq!(
            parse_special_command("/streaming status").unwrap(),
            SpecialCommand::ShowStreamingStatus
        );
    }

    #[test]
    fn test_parse_system_status_returns_show_system_status() {
        assert_eq!(
            parse_special_command("/system status").unwrap(),
            SpecialCommand::ShowSystemStatus
        );
    }

    #[test]
    fn test_parse_system_status_not_treated_as_prompt_text() {
        // "/system status" must route to ShowSystemStatus, not SetSystemPrompt("status")
        let result = parse_special_command("/system status").unwrap();
        assert_eq!(result, SpecialCommand::ShowSystemStatus);
        assert_ne!(
            result,
            SpecialCommand::SetSystemPrompt("status".to_string())
        );
    }

    #[test]
    fn test_parse_subagents_status_returns_show_subagents_status() {
        assert_eq!(
            parse_special_command("/subagents status").unwrap(),
            SpecialCommand::ShowSubagentsStatus
        );
    }

    #[test]
    fn test_format_mode_help_text_contains_status_note() {
        let text = format_mode_help_text();
        assert!(
            text.contains("/mode status"),
            "format_mode_help_text() missing '/mode status': {text}"
        );
        assert!(
            text.contains("USAGE:"),
            "format_mode_help_text() missing 'USAGE:': {text}"
        );
    }

    #[test]
    fn test_format_safety_help_text_contains_status_note() {
        let text = format_safety_help_text();
        assert!(
            text.contains("/safety status"),
            "format_safety_help_text() missing '/safety status': {text}"
        );
        assert!(
            text.contains("USAGE:"),
            "format_safety_help_text() missing 'USAGE:': {text}"
        );
    }

    #[test]
    fn test_format_streaming_help_text_contains_status_note() {
        let text = format_streaming_help_text();
        assert!(
            text.contains("/streaming status"),
            "format_streaming_help_text() missing '/streaming status': {text}"
        );
        assert!(
            text.contains("USAGE:"),
            "format_streaming_help_text() missing 'USAGE:': {text}"
        );
    }

    #[test]
    fn test_format_system_help_text_contains_status_note() {
        let text = format_system_help_text();
        assert!(
            text.contains("/system status"),
            "format_system_help_text() missing '/system status': {text}"
        );
        assert!(
            text.contains("USAGE:"),
            "format_system_help_text() missing 'USAGE:': {text}"
        );
    }

    #[test]
    fn test_format_subagents_help_text_contains_status_note() {
        let text = format_subagents_help_text();
        assert!(
            text.contains("/subagents status"),
            "format_subagents_help_text() missing '/subagents status': {text}"
        );
        assert!(
            text.contains("USAGE:"),
            "format_subagents_help_text() missing 'USAGE:': {text}"
        );
    }
}
