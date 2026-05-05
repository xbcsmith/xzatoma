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
//!
//! # Examples
//!
//! ```
//! use xzatoma::acp::available_commands::build_available_commands;
//!
//! let commands = build_available_commands();
//! assert_eq!(commands.len(), 8);
//! assert!(!commands[0].description.is_empty());
//! ```

use acp_sdk::schema as acp;
use agent_client_protocol as acp_sdk;

/// Builds the list of [`acp::AvailableCommand`] entries advertised to Zed.
///
/// Each entry corresponds to a slash command that Zed surfaces in the chat
/// input completion menu. Commands that accept an optional value argument are
/// annotated with an [`acp::AvailableCommandInput::Unstructured`] hint so
/// that Zed can display a descriptive placeholder.
///
/// # Returns
///
/// A `Vec<acp::AvailableCommand>` containing all eight XZatoma slash commands
/// in display order.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::available_commands::build_available_commands;
///
/// let commands = build_available_commands();
/// assert_eq!(commands.len(), 8);
///
/// let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
/// assert!(names.contains(&"/mode"));
/// assert!(names.contains(&"/mcp"));
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_available_commands_returns_eight_entries() {
        let commands = build_available_commands();
        assert_eq!(commands.len(), 8);
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
            ]
        );
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
        let no_input_names = ["/tools", "/context", "/summarize", "/skills", "/mcp"];
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
