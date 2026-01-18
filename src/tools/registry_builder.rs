//! Mode-aware tool registry builder
//!
//! This module provides a builder for constructing tool registries that are
//! filtered based on the current chat mode (Planning or Write) and safety mode.
//!
//! In Planning mode, only read-only tools are registered.
//! In Write mode, all tools are registered.

use std::path::PathBuf;
use std::sync::Arc;

use crate::chat_mode::{ChatMode, SafetyMode};
use crate::config::{TerminalConfig, ToolsConfig};
use crate::error::Result;
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::tools::{FileOpsReadOnlyTool, FileOpsTool, ToolExecutor, ToolRegistry};

/// Builder for mode-aware tool registries
///
/// Constructs a `ToolRegistry` filtered by chat mode and safety settings.
/// Planning mode registers only read-only tools, while Write mode registers all tools.
///
/// # Examples
///
/// ```no_run
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
/// assert_eq!(registry.len(), 1); // Only file_ops_read_only
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
    /// - Planning: read-only tools only
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

    /// Build a tool registry for Planning mode (read-only)
    ///
    /// Planning mode includes:
    /// - `file_ops_read_only` - Read-only file operations (read_file, list_files, search_files)
    ///
    /// Excluded:
    /// - Terminal execution
    /// - File modifications (write_file, delete_file)
    ///
    /// # Returns
    ///
    /// Returns a ToolRegistry with only read-only tools
    pub fn build_for_planning(&self) -> Result<ToolRegistry> {
        let mut registry = ToolRegistry::new();

        // Register read-only file operations tool
        let file_tool_readonly =
            FileOpsReadOnlyTool::new(self.working_dir.clone(), self.tools_config.clone());
        let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool_readonly);
        registry.register("file_ops", file_tool_executor);

        Ok(registry)
    }

    /// Build a tool registry for Write mode (full access)
    ///
    /// Write mode includes:
    /// - `file_ops` - Full file operations (read, write, delete, list, search, diff)
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

        // Register full file operations tool
        let file_tool = FileOpsTool::new(self.working_dir.clone(), self.tools_config.clone());
        let file_tool_executor: Arc<dyn ToolExecutor> = Arc::new(file_tool);
        registry.register("file_ops", file_tool_executor);

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
        assert_eq!(registry.len(), 1);
        assert!(registry.get("file_ops").is_some());
        assert!(registry.get("terminal").is_none());
    }

    #[test]
    fn test_build_for_write() {
        let builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::NeverConfirm,
            PathBuf::from("."),
        );

        let registry = builder.build_for_write().expect("Failed to build registry");
        assert_eq!(registry.len(), 2);
        assert!(registry.get("file_ops").is_some());
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
        assert_eq!(planning_registry.len(), 1);

        let write_builder = ToolRegistryBuilder::new(
            ChatMode::Write,
            SafetyMode::AlwaysConfirm,
            PathBuf::from("."),
        );

        let write_registry = write_builder.build().expect("Failed to build registry");
        assert_eq!(write_registry.len(), 2);
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
}
