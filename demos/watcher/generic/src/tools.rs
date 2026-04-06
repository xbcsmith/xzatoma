//! Tool registry and execution helpers.

/// Represents a single tool available to the agent.
#[derive(Debug, Clone)]
pub struct Tool {
    /// The unique name used to invoke this tool.
    pub name: String,
    /// A short human-readable description of what the tool does.
    pub description: String,
    /// Whether the tool requires explicit user approval before running.
    pub requires_approval: bool,
}

impl Tool {
    /// Creates a new `Tool` with the given name, description, and approval
    /// requirement.
    pub fn new(name: &str, description: &str, requires_approval: bool) -> Self {
        Tool {
            name: name.to_string(),
            description: description.to_string(),
            requires_approval,
        }
    }
}

pub fn list_tools() -> Vec<Tool> {
    vec![
        Tool::new("read_file", "Read the contents of a file from disk.", false),
        Tool::new("write_file", "Write content to a file on disk.", false),
        Tool::new(
            "grep",
            "Search file contents using a regular expression.",
            false,
        ),
        Tool::new(
            "list_directory",
            "List the files and subdirectories at a path.",
            false,
        ),
        Tool::new(
            "terminal",
            "Execute a shell command in the working directory.",
            true,
        ),
        Tool::new(
            "subagent",
            "Delegate a task to an isolated worker subagent.",
            false,
        ),
    ]
}

/// Returns the `Tool` registered under `name`, or `None` when no such tool
/// exists.
///
/// Look up is case-sensitive and matches the exact `name` string used at
/// registration time.
pub fn get_tool(name: &str) -> Option<Tool> {
    list_tools().into_iter().find(|t| t.name == name)
}

pub fn is_tool_allowed(name: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == name)
}

/// Returns `true` when `tool` requires explicit user approval before it may
/// be executed.
///
/// Approval-required tools pause the agent loop and wait for a human
/// confirmation signal before the tool call is dispatched.
pub fn requires_approval(name: &str) -> bool {
    get_tool(name).map(|t| t.requires_approval).unwrap_or(false)
}
