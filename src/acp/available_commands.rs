//! XZatoma ACP available command definitions for the Zed chat window.
//!
//! This module defines the slash commands that XZatoma advertises to Zed via
//! the Agent-Client Protocol. Zed displays these commands in the chat input
//! completion menu when the user types `/`.
//!
//! # Command Overview
//!
//! | Command      | Input       | Purpose                                        |
//! |--------------|-------------|------------------------------------------------|
//! | `/mode`      | Optional    | Show or change the XZatoma operation mode      |
//! | `/model`     | Optional    | Show or change the current provider model      |
//! | `/safety`    | Optional    | Show or change the safety confirmation policy  |
//! | `/tools`     | None        | Summarize available XZatoma and IDE tools      |
//! | `/context`   | None        | Show current conversation context usage        |
//! | `/summarize` | None        | Summarize and compact conversation history     |
//! | `/skills`    | None        | List active skills for the current workspace   |
//! | `/mcp`       | None        | List connected MCP servers and tools           |
//! | `/help`      | None        | Show available special commands                |
//! | `/status`    | None        | Show current mode, safety policy, and model    |
//! | `/subagents` | Optional    | Enable or disable subagent delegation          |
//! | `/system`    | Required    | Set or replace the active system prompt        |
//!
//! # Examples
//!
//! ```
//! use xzatoma::acp::available_commands::build_available_commands;
//!
//! let commands = build_available_commands();
//! assert_eq!(commands.len(), 12);
//! assert!(!commands[0].description.is_empty());
//! ```

use acp_sdk::schema::v1 as acp;
use agent_client_protocol as acp_sdk;

/// Builds the list of [`acp::AvailableCommand`] entries advertised to Zed.
///
/// Each entry corresponds to a slash command that Zed surfaces in the chat
/// input completion menu. Commands that accept an optional value argument are
/// annotated with an [`acp::AvailableCommandInput::Unstructured`] hint so
/// that Zed can display a descriptive display hint.
///
/// # Returns
///
/// A `Vec<acp::AvailableCommand>` containing all twelve XZatoma slash commands
/// in display order.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::available_commands::build_available_commands;
///
/// let commands = build_available_commands();
/// assert_eq!(commands.len(), 12);
///
/// let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
/// assert!(names.contains(&"/mode"));
/// assert!(names.contains(&"/mcp"));
/// assert!(names.contains(&"/help"));
/// assert!(names.contains(&"/system"));
/// ```
pub fn build_available_commands() -> Vec<acp::AvailableCommand> {
    vec![
        build_mode_command(),
        build_model_command(),
        build_safety_command(),
        build_tools_command(),
        build_context_command(),
        build_summarize_command(),
        build_skills_command(),
        build_mcp_command(),
        build_help_command(),
        build_status_command(),
        build_subagents_command(),
        build_system_command(),
    ]
}

// ---------------------------------------------------------------------------
// Private command builders
// ---------------------------------------------------------------------------

/// Builds the `/mode` command definition.
///
/// Accepts an optional mode ID argument. When no argument is provided the
/// agent reports the current mode; when an ID is provided the agent switches
/// to that mode.
fn build_mode_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/mode",
        "Show or change the XZatoma operation mode. \
         Available modes: planning, write, safe, full_autonomous.",
    )
    .input(Some(acp::AvailableCommandInput::Unstructured(
        acp::UnstructuredCommandInput::new(
            "Optional mode ID: planning | write | safe | full_autonomous",
        ),
    )))
}

/// Builds the `/model` command definition.
///
/// Accepts an optional model name argument. When no argument is provided the
/// agent reports the active provider model; when a name is provided the agent
/// requests a model switch.
fn build_model_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/model",
        "Show or change the current AI provider model. \
         Pass a model name to switch, or omit to see the active model.",
    )
    .input(Some(acp::AvailableCommandInput::Unstructured(
        acp::UnstructuredCommandInput::new("Optional model name, e.g. gpt-4o or llama3.2:latest"),
    )))
}

/// Builds the `/safety` command definition.
///
/// Accepts an optional safety policy argument. When no argument is provided
/// the agent reports the current policy; when a value is provided the agent
/// applies the new policy.
fn build_safety_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/safety",
        "Show or change the safety confirmation policy. \
         Controls when XZatoma requests confirmation before executing operations.",
    )
    .input(Some(acp::AvailableCommandInput::Unstructured(
        acp::UnstructuredCommandInput::new(
            "Optional policy: always_confirm | confirm_dangerous | never_confirm",
        ),
    )))
}

/// Builds the `/tools` command definition.
///
/// Takes no arguments. The agent responds with a summary of all tools
/// registered in the current session, including both local XZatoma tools and
/// any tools exposed via the IDE integration.
fn build_tools_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/tools",
        "Summarize all available XZatoma and IDE tools for the current session.",
    )
}

/// Builds the `/context` command definition.
///
/// Takes no arguments. The agent responds with a summary of the current
/// conversation context window usage, including token counts and the
/// percentage of the context budget consumed.
fn build_context_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/context",
        "Show current conversation context usage including token counts and context budget.",
    )
}

/// Builds the `/summarize` command definition.
///
/// Takes no arguments. The agent compacts the conversation history by
/// replacing earlier turns with a concise summary, freeing context budget
/// for future work.
fn build_summarize_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/summarize",
        "Summarize and compact the conversation history to free context budget.",
    )
}

/// Builds the `/skills` command definition.
///
/// Takes no arguments. The agent lists all skills discovered and activated
/// for the current workspace, including their source paths and activation
/// conditions.
fn build_skills_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/skills",
        "List active skills discovered for the current workspace.",
    )
}

/// Builds the `/mcp` command definition.
///
/// Takes no arguments. The agent lists all connected MCP servers and the
/// tools they expose, along with their connection status.
fn build_mcp_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/mcp",
        "List connected MCP servers and the tools they expose.",
    )
}

/// Builds the `/help` command definition.
///
/// Takes no arguments. The agent responds with a summary of all available
/// special commands and how to use them.
fn build_help_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new("/help", "Show available special commands.")
}

/// Builds the `/status` command definition.
///
/// Takes no arguments. The agent responds with the current operation mode,
/// safety confirmation policy, and active provider model.
fn build_status_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new("/status", "Show current mode, safety policy, and model.")
}

/// Builds the `/subagents` command definition.
///
/// Accepts an optional toggle argument. When no argument is provided the
/// agent reports whether subagent delegation is currently enabled; when a
/// value is provided the agent enables or disables subagent delegation
/// accordingly.
fn build_subagents_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new("/subagents", "Enable or disable subagent delegation.").input(Some(
        acp::AvailableCommandInput::Unstructured(acp::UnstructuredCommandInput::new(
            "Optional toggle: on | off | enable | disable",
        )),
    ))
}

/// Builds the `/system` command definition.
///
/// Requires a text argument. The agent replaces the active system prompt for
/// the remainder of the session with the provided text.
fn build_system_command() -> acp::AvailableCommand {
    acp::AvailableCommand::new(
        "/system",
        "Set or replace the active system prompt for this session.",
    )
    .input(Some(acp::AvailableCommandInput::Unstructured(
        acp::UnstructuredCommandInput::new("Required text: the new system prompt"),
    )))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_available_commands_returns_twelve_entries() {
        let commands = build_available_commands();
        assert_eq!(commands.len(), 12);
    }

    #[test]
    fn test_build_available_commands_names_are_correct() {
        let commands = build_available_commands();
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "/mode",
                "/model",
                "/safety",
                "/tools",
                "/context",
                "/summarize",
                "/skills",
                "/mcp",
                "/help",
                "/status",
                "/subagents",
                "/system",
            ]
        );
    }

    #[test]
    fn test_build_available_commands_includes_new_commands() {
        let commands = build_available_commands();
        for name in ["/help", "/status", "/subagents", "/system"] {
            assert!(
                commands.iter().any(|c| c.name == name),
                "expected command '{}' to be present",
                name
            );
        }
    }

    #[test]
    fn test_build_available_commands_no_empty_descriptions() {
        for command in build_available_commands() {
            assert!(
                !command.description.is_empty(),
                "command '{}' has an empty description",
                command.name
            );
        }
    }

    #[test]
    fn test_build_available_commands_names_start_with_slash() {
        for command in build_available_commands() {
            assert!(
                command.name.starts_with('/'),
                "command name '{}' must start with '/'",
                command.name
            );
        }
    }

    #[test]
    fn test_mode_command_has_unstructured_input() {
        let commands = build_available_commands();
        let mode = commands.iter().find(|c| c.name == "/mode").unwrap();
        assert!(
            mode.input.is_some(),
            "/mode must have an input specification"
        );
        match mode.input.as_ref().unwrap() {
            acp::AvailableCommandInput::Unstructured(input) => {
                assert!(!input.hint.is_empty(), "/mode input hint must not be empty");
            }
            _ => panic!("/mode input must be Unstructured"),
        }
    }

    #[test]
    fn test_model_command_has_unstructured_input() {
        let commands = build_available_commands();
        let model = commands.iter().find(|c| c.name == "/model").unwrap();
        assert!(
            model.input.is_some(),
            "/model must have an input specification"
        );
        match model.input.as_ref().unwrap() {
            acp::AvailableCommandInput::Unstructured(input) => {
                assert!(
                    !input.hint.is_empty(),
                    "/model input hint must not be empty"
                );
            }
            _ => panic!("/model input must be Unstructured"),
        }
    }

    #[test]
    fn test_safety_command_has_unstructured_input() {
        let commands = build_available_commands();
        let safety = commands.iter().find(|c| c.name == "/safety").unwrap();
        assert!(
            safety.input.is_some(),
            "/safety must have an input specification"
        );
        match safety.input.as_ref().unwrap() {
            acp::AvailableCommandInput::Unstructured(input) => {
                assert!(
                    !input.hint.is_empty(),
                    "/safety input hint must not be empty"
                );
            }
            _ => panic!("/safety input must be Unstructured"),
        }
    }

    #[test]
    fn test_no_arg_commands_have_no_input() {
        let commands = build_available_commands();
        let no_input_names = [
            "/tools",
            "/context",
            "/summarize",
            "/skills",
            "/mcp",
            "/help",
            "/status",
        ];
        for name in no_input_names {
            let command = commands.iter().find(|c| c.name == name).unwrap();
            assert!(
                command.input.is_none(),
                "command '{}' should have no input specification",
                name
            );
        }
    }

    #[test]
    fn test_subagents_command_has_unstructured_input() {
        let commands = build_available_commands();
        let subagents = commands.iter().find(|c| c.name == "/subagents").unwrap();
        assert!(
            subagents.input.is_some(),
            "/subagents must have an input specification"
        );
        match subagents.input.as_ref().unwrap() {
            acp::AvailableCommandInput::Unstructured(input) => {
                assert!(
                    !input.hint.is_empty(),
                    "/subagents input hint must not be empty"
                );
            }
            _ => panic!("/subagents input must be Unstructured"),
        }
    }

    #[test]
    fn test_system_command_has_unstructured_input() {
        let commands = build_available_commands();
        let system = commands.iter().find(|c| c.name == "/system").unwrap();
        assert!(
            system.input.is_some(),
            "/system must have an input specification"
        );
        match system.input.as_ref().unwrap() {
            acp::AvailableCommandInput::Unstructured(input) => {
                assert!(
                    !input.hint.is_empty(),
                    "/system input hint must not be empty"
                );
            }
            _ => panic!("/system input must be Unstructured"),
        }
    }

    #[test]
    fn test_build_available_commands_no_duplicate_names() {
        let commands = build_available_commands();
        let mut names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        let original_len = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "duplicate command names detected"
        );
    }

    #[test]
    fn test_build_available_commands_is_deterministic() {
        let first = build_available_commands();
        let second = build_available_commands();
        let first_names: Vec<&str> = first.iter().map(|c| c.name.as_str()).collect();
        let second_names: Vec<&str> = second.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(first_names, second_names);
    }
}
