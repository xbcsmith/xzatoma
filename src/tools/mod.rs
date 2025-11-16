//! Tools module for XZatoma
//!
//! This module contains tool definitions, tool registry, and tool implementations
//! for file operations, terminal execution, and plan parsing.

// Phase 1: Allow unused code for placeholder implementations
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod file_ops;
pub mod plan;
pub mod terminal;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Tool definition structure
///
/// Represents a tool that can be called by the AI provider.
/// Follows the OpenAI function calling format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Name of the tool
    pub name: String,
    /// Description of what the tool does
    pub description: String,
    /// JSON schema for the tool's parameters
    pub parameters: serde_json::Value,
}

impl Tool {
    /// Create a new tool definition
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name
    /// * `description` - Tool description
    /// * `parameters` - JSON schema for parameters
    ///
    /// # Returns
    ///
    /// Returns a new Tool instance
    pub fn new(name: String, description: String, parameters: serde_json::Value) -> Self {
        Self {
            name,
            description,
            parameters,
        }
    }
}

/// Tool result structure
///
/// Represents the result of a tool execution with metadata
/// and truncation support.
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether the tool execution succeeded
    pub success: bool,
    /// Output from the tool
    pub output: String,
    /// Error message if execution failed
    pub error: Option<String>,
    /// Whether the output was truncated
    pub truncated: bool,
    /// Additional metadata about the execution
    pub metadata: HashMap<String, String>,
}

impl ToolResult {
    /// Create a successful tool result
    ///
    /// # Arguments
    ///
    /// * `output` - Tool output
    ///
    /// # Returns
    ///
    /// Returns a successful ToolResult
    pub fn success(output: String) -> Self {
        Self {
            success: true,
            output,
            error: None,
            truncated: false,
            metadata: HashMap::new(),
        }
    }

    /// Create a failed tool result
    ///
    /// # Arguments
    ///
    /// * `error` - Error message
    ///
    /// # Returns
    ///
    /// Returns a failed ToolResult
    pub fn error(error: String) -> Self {
        Self {
            success: false,
            output: String::new(),
            error: Some(error),
            truncated: false,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the result
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Returns
    ///
    /// Returns self for chaining
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Truncate output if it exceeds the maximum size
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum size in bytes
    ///
    /// # Returns
    ///
    /// Returns self with potentially truncated output
    pub fn truncate_if_needed(mut self, max_size: usize) -> Self {
        if self.output.len() > max_size {
            self.output.truncate(max_size);
            self.output.push_str("\n... (truncated)");
            self.truncated = true;
        }
        self
    }

    /// Convert to a message string for the conversation
    ///
    /// # Returns
    ///
    /// Returns a formatted string representation
    pub fn to_message(&self) -> String {
        if self.success {
            if self.truncated {
                format!("{}\n(Output truncated to fit context window)", self.output)
            } else {
                self.output.clone()
            }
        } else {
            format!(
                "Error: {}",
                self.error.as_ref().unwrap_or(&"Unknown error".to_string())
            )
        }
    }
}

/// Tool executor trait for implementing tool execution logic
///
/// Each tool must implement this trait to provide execution logic
/// that can be called by the agent.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::ToolExecutor;
/// use xzatoma::error::Result;
/// use async_trait::async_trait;
/// use serde_json::Value;
///
/// struct MyTool;
///
/// #[async_trait]
/// impl ToolExecutor for MyTool {
///     fn tool_definition(&self) -> Value {
///         serde_json::json!({
///             "name": "my_tool",
///             "description": "Does something useful",
///             "parameters": {
///                 "type": "object",
///                 "properties": {}
///             }
///         })
///     }
///
///     async fn execute(&self, args: Value) -> Result<xzatoma::tools::ToolResult> {
///         Ok(xzatoma::tools::ToolResult::success("Success".to_string()))
///     }
/// }
/// ```
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Returns the tool definition as a JSON value
    ///
    /// The definition should follow the OpenAI function calling format:
    /// ```json
    /// {
    ///   "name": "tool_name",
    ///   "description": "Tool description",
    ///   "parameters": {
    ///     "type": "object",
    ///     "properties": {
    ///       "param1": {"type": "string", "description": "..."}
    ///     },
    ///     "required": ["param1"]
    ///   }
    /// }
    /// ```
    fn tool_definition(&self) -> serde_json::Value;

    /// Executes the tool with the given arguments
    ///
    /// # Arguments
    ///
    /// * `args` - Tool arguments as a JSON value
    ///
    /// # Returns
    ///
    /// Returns a ToolResult with the execution outcome
    ///
    /// # Errors
    ///
    /// Returns error if execution fails
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult>;
}

/// Tool registry for managing available tools
///
/// The registry maintains a collection of tools that can be executed
/// by the agent during conversation.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry
    ///
    /// # Returns
    ///
    /// Returns a new ToolRegistry instance
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool executor in the registry
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name
    /// * `executor` - Tool executor implementation
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::tools::ToolRegistry;
    /// // let mut registry = ToolRegistry::new();
    /// // registry.register("my_tool", Box::new(MyToolExecutor));
    /// ```
    pub fn register(&mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) {
        self.tools.insert(name.into(), executor);
    }

    /// Get a tool executor by name
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name
    ///
    /// # Returns
    ///
    /// Returns the tool executor if found
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>> {
        self.tools.get(name).cloned()
    }

    /// Get all tool definitions as JSON values
    ///
    /// # Returns
    ///
    /// Returns a vector of all tool definitions
    pub fn all_definitions(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|executor| executor.tool_definition())
            .collect()
    }

    /// Get the number of registered tools
    ///
    /// # Returns
    ///
    /// Returns the count of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty
    ///
    /// # Returns
    ///
    /// Returns true if no tools are registered
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_creation() {
        let tool = Tool::new(
            "test_tool".to_string(),
            "A test tool".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        );
        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("output".to_string());
        assert!(result.success);
        assert_eq!(result.output, "output");
        assert!(result.error.is_none());
        assert!(!result.truncated);
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("failed".to_string());
        assert!(!result.success);
        assert_eq!(result.error, Some("failed".to_string()));
        assert!(result.output.is_empty());
    }

    #[test]
    fn test_tool_result_with_metadata() {
        let result = ToolResult::success("output".to_string())
            .with_metadata("key".to_string(), "value".to_string());
        assert_eq!(result.metadata.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_tool_result_truncation() {
        let long_output = "a".repeat(1000);
        let result = ToolResult::success(long_output).truncate_if_needed(100);
        assert!(result.truncated);
        assert!(result.output.len() <= 120);
        assert!(result.output.contains("truncated"));
    }

    #[test]
    fn test_tool_result_no_truncation() {
        let short_output = "short".to_string();
        let result = ToolResult::success(short_output.clone()).truncate_if_needed(100);
        assert!(!result.truncated);
        assert_eq!(result.output, short_output);
    }

    #[test]
    fn test_tool_result_to_message_success() {
        let result = ToolResult::success("output".to_string());
        assert_eq!(result.to_message(), "output");
    }

    #[test]
    fn test_tool_result_to_message_truncated() {
        let result = ToolResult::success("output".to_string()).truncate_if_needed(3);
        let message = result.to_message();
        assert!(message.contains("truncated"));
    }

    #[test]
    fn test_tool_result_to_message_error() {
        let result = ToolResult::error("failed".to_string());
        assert_eq!(result.to_message(), "Error: failed");
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    // ToolRegistry tests updated for ToolExecutor trait

    struct MockToolExecutor {
        name: String,
    }

    #[async_trait]
    impl ToolExecutor for MockToolExecutor {
        fn tool_definition(&self) -> serde_json::Value {
            serde_json::json!({
                "name": self.name,
                "description": "Mock tool",
                "parameters": {"type": "object"}
            })
        }

        async fn execute(&self, _args: serde_json::Value) -> crate::error::Result<ToolResult> {
            Ok(ToolResult::success("mock output".to_string()))
        }
    }

    #[test]
    fn test_tool_registry_register() {
        let mut registry = ToolRegistry::new();
        let tool = Arc::new(MockToolExecutor {
            name: "test".to_string(),
        });
        registry.register("test", tool);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_tool_registry_get() {
        let mut registry = ToolRegistry::new();
        let tool = Arc::new(MockToolExecutor {
            name: "test".to_string(),
        });
        registry.register("test", tool);

        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_tool_registry_get_nonexistent() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_tool_registry_all_definitions() {
        let mut registry = ToolRegistry::new();
        let tool1 = Arc::new(MockToolExecutor {
            name: "test1".to_string(),
        });
        let tool2 = Arc::new(MockToolExecutor {
            name: "test2".to_string(),
        });
        registry.register("test1", tool1);
        registry.register("test2", tool2);

        let all = registry.all_definitions();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_tool_executor_execution() {
        let executor = MockToolExecutor {
            name: "test".to_string(),
        };
        let result = executor.execute(serde_json::json!({})).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.is_empty());
    }
}
