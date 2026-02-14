//! Mode-aware tool registry builder
//!
//! This module provides a builder for constructing tool registries that are
//! filtered based on the current chat mode (Planning or Write) and safety mode.
//!
//! In Planning mode, only read-only tools are registered.
//! In Write mode, all tools are registered.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::debug;

use crate::chat_mode::{ChatMode, SafetyMode};
use crate::config::{TerminalConfig, ToolsConfig};
use crate::error::Result;
use crate::tools::copy_path::CopyPathTool;
use crate::tools::create_directory::CreateDirectoryTool;
use crate::tools::delete_path::DeletePathTool;
use crate::tools::edit_file::EditFileTool;
use crate::tools::find_path::FindPathTool;
use crate::tools::list_directory::ListDirectoryTool;
use crate::tools::move_path::MovePathTool;
use crate::tools::read_file::ReadFileTool;
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::tools::write_file::WriteFileTool;
use crate::tools::{ToolExecutor, ToolRegistry};

/// Builder for mode-aware tool registries
///
/// Constructs a `ToolRegistry` filtered by chat mode and safety settings.
/// Planning mode registers only read-only tools, while Write mode registers all tools.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::registry_builder::ToolRegistryBuilder;
/// use xzatoma::chat_mode::{ChatMode, SafetyMode};
/// use std::path::PathBuf;
///
/// let builder = ToolRegistryBuilder::new(
///     ChatMode::Planning,
///     SafetyMode::AlwaysConfirm,
///     PathBuf::from("."),
/// );
///
/// let registry = builder.build_for_planning().expect("Failed to build registry");
/// assert_eq!(registry.len(), 3); // read_file, list_directory, find_path
/// ```
pub struct ToolRegistryBuilder {
    /// The chat mode (Planning or Write)
    mode: ChatMode,
    /// The safety mode (AlwaysConfirm or NeverConfirm)
    safety_mode: SafetyMode,
    /// Working directory for tool operations
    working_dir: PathBuf,
    /// Tools configuration
    tools_config: ToolsConfig,
    /// Terminal configuration
    terminal_config: TerminalConfig,
}

impl ToolRegistryBuilder {
    /// Create a new tool registry builder
    ///
    /// # Arguments
    ///
    /// * `mode` - The chat mode (Planning or Write)
    /// * `safety_mode` - The safety mode (AlwaysConfirm or NeverConfirm)
    /// * `working_dir` - Working directory for tool operations
    ///
    /// # Returns
    ///
    /// Returns a new ToolRegistryBuilder instance
    pub fn new(mode: ChatMode, safety_mode: SafetyMode, working_dir: PathBuf) -> Self {
        Self {
            mode,
            safety_mode,
            working_dir,
            tools_config: ToolsConfig::default(),
            terminal_config: TerminalConfig::default(),
        }
    }

    /// Set the tools configuration
    ///
    /// # Arguments
    ///
    /// * `config` - The tools configuration
    ///
    /// # Returns
    ///
    /// Returns self for method chaining
    pub fn with_tools_config(mut self, config: ToolsConfig) -> Self {
        self.tools_config = config;
        self
    }

    /// Set the terminal configuration
    ///
    /// # Arguments
    ///
    /// * `config` - The terminal configuration
    ///
    /// # Returns
    ///
    /// Returns self for method chaining
    pub fn with_terminal_config(mut self, config: TerminalConfig) -> Self {
        self.terminal_config = config;
        self
    }

    /// Build a tool registry for the current mode
    ///
    /// Automatically selects the appropriate registry based on `mode`.
    /// - Planning: read-only tools only (read_file, list_directory, find_path)
    /// - Write: all tools
    ///
    /// # Returns
    ///
    /// Returns a configured ToolRegistry
    ///
    /// # Errors
    ///
    /// Returns error if tool initialization fails
    pub fn build(&self) -> Result<ToolRegistry> {
        match self.mode {
            ChatMode::Planning => self.build_for_planning(),
            ChatMode::Write => self.build_for_write(),
        }
    }

    /// Build a tool registry for chat mode with optional subagent support
    ///
    /// This method builds a registry appropriate for chat mode, optionally including
    /// subagent tools if `subagents_enabled` is true. The base tools are determined
    /// by the builder's chat mode setting:
    /// - Planning: read-only tools only
    /// - Write: all tools
    ///
    /// When subagents are enabled, this method would register subagent tools.
    /// Currently, this is a placeholder that calls the standard build method,
    /// but can be extended to include subagent-specific tool registration.
    ///
    /// # Arguments
    ///
    /// * `subagents_enabled` - Whether to include subagent tools in the registry
    ///
    /// # Returns
    ///
    /// Returns a configured ToolRegistry
    ///
    /// # Errors
    ///
    /// Returns error if tool initialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::registry_builder::ToolRegistryBuilder;
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode};
    /// use std::path::PathBuf;
    ///
    /// let builder = ToolRegistryBuilder::new(
    ///     ChatMode::Write,
    ///     SafetyMode::NeverConfirm,
    ///     PathBuf::from("."),
    /// );
    ///
    /// let registry = builder.build_for_chat(false).expect("Failed to build registry");
    /// assert_eq!(registry.len(), 10); // All standard Write mode tools
    /// ```
    pub fn build_for_chat(&self, subagents_enabled: bool) -> Result<ToolRegistry> {
        // Build the base registry for the current mode
        let registry = self.build()?;

        // Note: Subagent tool registration would be handled here if needed.
        // Currently, subagent tools are registered separately in the agent/command layer
        // to allow for dynamic enablement/disablement during chat sessions.
        // This method serves as a placeholder for future integration of subagent tools
        // directly into the registry builder.

        if subagents_enabled {
            // Placeholder for future subagent tool registration
            // When implemented, this would register subagent-specific tools
            debug!("Building registry with subagent support enabled");
        }

        Ok(registry)
    }

    /// Build a tool registry for Planning mode (read-only)
    ///
    /// Planning mode includes read-only tools:
    /// - `read_file` - Read file contents with optional line range
    /// - `list_directory` - List directory contents with optional recursion and pattern matching
    /// - `find_path` - Find files by glob pattern
    ///
    /// Excluded:
    /// - Terminal execution
    /// - File modifications (write_file, delete_path, copy_path, move_path, create_directory, edit_file)
    ///
    /// # Returns
    ///
    /// Returns a ToolRegistry with only read-only tools
    pub fn build_for_planning(&self) -> Result<ToolRegistry> {
        let mut registry = ToolRegistry::new();

        // Register read_file tool
        let read_tool = ReadFileTool::new(
            self.working_dir.clone(),
            self.tools_config.max_file_read_size as u64,
            500, // max_outline_lines for file structure display
        );
        let read_tool_executor: Arc<dyn ToolExecutor> = Arc::new(read_tool);
        registry.register("read_file", read_tool_executor);

        // Register list_directory tool
        let list_tool = ListDirectoryTool::new(self.working_dir.clone());
        let list_tool_executor: Arc<dyn ToolExecutor> = Arc::new(list_tool);
        registry.register("list_directory", list_tool_executor);

        // Register find_path tool
        let find_tool = FindPathTool::new(self.working_dir.clone());
        let find_tool_executor: Arc<dyn ToolExecutor> = Arc::new(find_tool);
        registry.register("find_path", find_tool_executor);

        Ok(registry)
    }

    /// Build a tool registry for Write mode (full access)
    ///
    /// Write mode includes all file operation tools:
    /// - `read_file` - Read file contents
    /// - `write_file` - Write or overwrite files
    /// - `delete_path` - Delete files or directories
    /// - `list_directory` - List directory contents
    /// - `copy_path` - Copy files or directories
    /// - `move_path` - Move or rename files or directories
    /// - `create_directory` - Create directories
    /// - `find_path` - Find files by glob pattern
    /// - `edit_file` - Edit files with targeted replacements or create new files
    /// - `terminal` - Terminal command execution with safety validation
    ///
    /// The terminal tool respects the configured safety mode:
    /// - `AlwaysConfirm` - Requires confirmation for dangerous operations
    /// - `NeverConfirm` - Allows all non-blacklisted operations
    ///
    /// # Returns
    ///
    /// Returns a ToolRegistry with all tools
    ///
    /// # Errors
    ///
    /// Returns error if tool initialization fails
    pub fn build_for_write(&self) -> Result<ToolRegistry> {
        let mut registry = ToolRegistry::new();

        // Register read_file tool
        let read_tool = ReadFileTool::new(
            self.working_dir.clone(),
            self.tools_config.max_file_read_size as u64,
            500, // max_outline_lines for file structure display
        );
        let read_tool_executor: Arc<dyn ToolExecutor> = Arc::new(read_tool);
        registry.register("read_file", read_tool_executor);

        // Register write_file tool
        let write_tool = WriteFileTool::new(
            self.working_dir.clone(),
            self.tools_config.max_file_read_size as u64,
        );
        let write_tool_executor: Arc<dyn ToolExecutor> = Arc::new(write_tool);
        registry.register("write_file", write_tool_executor);

        // Register delete_path tool
        let delete_tool = DeletePathTool::new(self.working_dir.clone());
        let delete_tool_executor: Arc<dyn ToolExecutor> = Arc::new(delete_tool);
        registry.register("delete_path", delete_tool_executor);

        // Register list_directory tool
        let list_tool = ListDirectoryTool::new(self.working_dir.clone());
        let list_tool_executor: Arc<dyn ToolExecutor> = Arc::new(list_tool);
        registry.register("list_directory", list_tool_executor);

        // Register copy_path tool
        let copy_tool = CopyPathTool::new(self.working_dir.clone());
        let copy_tool_executor: Arc<dyn ToolExecutor> = Arc::new(copy_tool);
        registry.register("copy_path", copy_tool_executor);

        // Register move_path tool
        let move_tool = MovePathTool::new(self.working_dir.clone());
        let move_tool_executor: Arc<dyn ToolExecutor> = Arc::new(move_tool);
        registry.register("move_path", move_tool_executor);

        // Register create_directory tool
        let create_tool = CreateDirectoryTool::new(self.working_dir.clone());
        let create_tool_executor: Arc<dyn ToolExecutor> = Arc::new(create_tool);
        registry.register("create_directory", create_tool_executor);

        // Register find_path tool
        let find_tool = FindPathTool::new(self.working_dir.clone());
        let find_tool_executor: Arc<dyn ToolExecutor> = Arc::new(find_tool);
        registry.register("find_path", find_tool_executor);

        // Register edit_file tool for targeted edits and diffs
        let edit_tool = EditFileTool::new(
            self.working_dir.clone(),
            self.tools_config.max_file_read_size as u64,
        );
        let edit_tool_executor: Arc<dyn ToolExecutor> = Arc::new(edit_tool);
        registry.register("edit_file", edit_tool_executor);

        // Register terminal tool with safety mode
        let terminal_validator =
            CommandValidator::new(self.terminal_config.default_mode, self.working_dir.clone());
        let terminal_tool = TerminalTool::new(terminal_validator, self.terminal_config.clone())
            .with_safety_mode(self.safety_mode);
        let terminal_tool_executor: Arc<dyn ToolExecutor> = Arc::new(terminal_tool);
        registry.register("terminal", terminal_tool_executor);

        Ok(registry)
    }

    /// Get the current chat mode
    ///
    /// # Returns
    ///
    /// Returns the chat mode
    pub fn mode(&self) -> ChatMode {
        self.mode
    }

    /// Get the current safety mode
    ///
    /// # Returns
    ///
    /// Returns the safety mode
    pub fn safety_mode(&self) -> SafetyMode {
        self.safety_mode
    }

    /// Get the working directory
    ///
    /// # Returns
    ///
    /// Returns the working directory path
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_new() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );
        assert_eq!(builder.mode(), ChatMode::Planning);
        assert_eq!(builder.safety_mode(), SafetyMode::AlwaysConfirm);
    }

    #[test]
    fn test_builder_with_tools_config() {
        let config = ToolsConfig::default();
        let builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        )
        .with_tools_config(config);

        assert_eq!(builder.mode(), ChatMode::Planning);
    }

    #[test]
    fn test_builder_with_terminal_config() {
        let config = TerminalConfig::default();
        let builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        )
        .with_terminal_config(config);

        assert_eq!(builder.mode(), ChatMode::Planning);
    }

    #[test]
    fn test_build_for_planning() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let registry = builder
            .build_for_planning()
            .expect("Failed to build registry");
        assert_eq!(registry.len(), 3);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("list_directory").is_some());
        assert!(registry.get("find_path").is_some());
        assert!(registry.get("terminal").is_none());
        assert!(registry.get("write_file").is_none());
    }

    #[test]
    fn test_build_for_write() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::NeverConfirm,
            PathBuf::from("."),
        );

        let registry = builder.build_for_write().expect("Failed to build registry");
        assert_eq!(registry.len(), 10);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("delete_path").is_some());
        assert!(registry.get("list_directory").is_some());
        assert!(registry.get("copy_path").is_some());
        assert!(registry.get("move_path").is_some());
        assert!(registry.get("create_directory").is_some());
        assert!(registry.get("find_path").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("terminal").is_some());
    }

    #[test]
    fn test_build_delegates_to_mode() {
        let planning_builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let planning_registry = planning_builder.build().expect("Failed to build registry");
        assert_eq!(planning_registry.len(), 3);

        let write_builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let write_registry = write_builder.build().expect("Failed to build registry");
        assert_eq!(write_registry.len(), 10);
    }

    #[test]
    fn test_builder_mode_accessor() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::NeverConfirm,
            PathBuf::from("/test"),
        );

        assert_eq!(builder.mode(), ChatMode::Write);
        assert_eq!(builder.safety_mode(), SafetyMode::NeverConfirm);
        assert_eq!(builder.working_dir(), &PathBuf::from("/test"));
    }

    #[test]
    fn test_build_for_chat_planning_no_subagents() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Planning,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let registry = builder
            .build_for_chat(false)
            .expect("Failed to build registry");
        assert_eq!(registry.len(), 3); // read_file, list_directory, find_path
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("list_directory").is_some());
        assert!(registry.get("find_path").is_some());
    }

    #[test]
    fn test_build_for_chat_write_no_subagents() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::NeverConfirm,
            PathBuf::from("."),
        );

        let registry = builder
            .build_for_chat(false)
            .expect("Failed to build registry");
        assert_eq!(registry.len(), 10); // All standard Write mode tools
    }

    #[test]
    fn test_build_for_chat_write_with_subagents_flag() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let registry = builder
            .build_for_chat(true)
            .expect("Failed to build registry");
        // Currently returns the same tools, but flag is passed and logged
        assert_eq!(registry.len(), 10);
    }
}
