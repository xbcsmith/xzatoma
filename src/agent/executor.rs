//! Tool executor trait for XZatoma
//!
//! This module defines the ToolExecutor trait that all tools must implement.
//! It provides a common interface for tool registration and execution.

use crate::error::Result;
use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::Value;

/// Tool executor trait
///
/// All tools must implement this trait to be registered with the agent.
/// The trait provides methods for tool definition and execution.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Get the tool definition for this executor
    ///
    /// # Returns
    ///
    /// Returns the Tool definition including name, description, and parameters
    fn tool_definition(&self) -> Tool;

    /// Execute the tool with the given arguments
    ///
    /// # Arguments
    ///
    /// * `args` - Tool arguments as JSON value
    ///
    /// # Returns
    ///
    /// Returns the tool execution result as a string
    ///
    /// # Errors
    ///
    /// Returns error if tool execution fails
    async fn execute(&self, args: Value) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockToolExecutor;

    #[async_trait]
    impl ToolExecutor for MockToolExecutor {
        fn tool_definition(&self) -> Tool {
            Tool {
                name: "mock_tool".to_string(),
                description: "A mock tool for testing".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                }),
            }
        }

        async fn execute(&self, _args: Value) -> Result<String> {
            Ok("mock result".to_string())
        }
    }

    #[test]
    fn test_mock_tool_definition() {
        let executor = MockToolExecutor;
        let tool = executor.tool_definition();
        assert_eq!(tool.name, "mock_tool");
        assert_eq!(tool.description, "A mock tool for testing");
    }

    #[tokio::test]
    async fn test_mock_tool_execute() {
        let executor = MockToolExecutor;
        let result = executor.execute(serde_json::json!({})).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock result");
    }
}
