//! Base provider trait and common types for XZatoma
//!
//! This module defines the Provider trait that all AI providers must implement,
//! along with common message types and response structures.

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Message structure for conversation
///
/// Represents a message in the conversation with the AI provider.
/// Messages can be from the user, assistant, system, or tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender (user, assistant, system, tool)
    pub role: String,
    /// Content of the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Optional tool calls in the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Optional tool call ID (for tool result messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Creates a new user message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::user("Hello, assistant!");
    /// assert_eq!(msg.role, "user");
    /// ```
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new assistant message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::assistant("Hello, user!");
    /// assert_eq!(msg.role, "assistant");
    /// ```
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new system message
    ///
    /// # Arguments
    ///
    /// * `content` - The message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::system("You are a helpful assistant");
    /// assert_eq!(msg.role, "system");
    /// ```
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Creates a new tool result message
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - The ID of the tool call this result corresponds to
    /// * `content` - The tool execution result content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::Message;
    ///
    /// let msg = Message::tool_result("call_123", "File contents...");
    /// assert_eq!(msg.role, "tool");
    /// assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    /// ```
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Creates an assistant message with tool calls
    ///
    /// # Arguments
    ///
    /// * `tool_calls` - The tool calls to include
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::{Message, ToolCall, FunctionCall};
    ///
    /// let tool_call = ToolCall {
    ///     id: "call_123".to_string(),
    ///     function: FunctionCall {
    ///         name: "read_file".to_string(),
    ///         arguments: r#"{"path":"test.txt"}"#.to_string(),
    ///     },
    /// };
    /// let msg = Message::assistant_with_tools(vec![tool_call]);
    /// assert_eq!(msg.role, "assistant");
    /// assert!(msg.tool_calls.is_some());
    /// ```
    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }
}

/// Function call information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function/tool to call
    pub name: String,
    /// Arguments for the function (as JSON string)
    pub arguments: String,
}

/// Tool call structure
///
/// Represents a request from the AI to execute a tool with specific arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub id: String,
    /// Function call details
    pub function: FunctionCall,
}

/// Provider trait for AI providers
///
/// All AI providers (Copilot, Ollama, etc.) must implement this trait.
/// The trait provides a common interface for completing conversations
/// with tool support.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::{Provider, Message};
/// use xzatoma::error::Result;
/// use async_trait::async_trait;
///
/// struct MyProvider;
///
/// #[async_trait]
/// impl Provider for MyProvider {
///     async fn complete(
///         &self,
///         messages: &[Message],
///         tools: &[serde_json::Value],
///     ) -> Result<Message> {
///         // Implementation here
///         Ok(Message::assistant("Response"))
///     }
/// }
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
    /// Completes a conversation with the given messages and available tools
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools for the assistant to use (as JSON schemas)
    ///
    /// # Returns
    ///
    /// Returns the assistant's response message
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails or response is invalid
    async fn complete(&self, messages: &[Message], tools: &[serde_json::Value]) -> Result<Message>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_user_with_string() {
        let msg = Message::user(String::from("Hello"));
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, Some("Hello".to_string()));
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hi there".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("System prompt");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, Some("System prompt".to_string()));
        assert!(msg.tool_calls.is_none());
        assert!(msg.tool_call_id.is_none());
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("call_123", "result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.content, Some("result".to_string()));
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_message_assistant_with_tools() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let msg = Message::assistant_with_tools(vec![tool_call]);
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.is_none());
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Test\""));
    }

    #[test]
    fn test_tool_call_serialization() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "test_tool".to_string(),
                arguments: r#"{"arg":"value"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&tool_call).unwrap();
        assert!(json.contains("\"id\":\"call_123\""));
        assert!(json.contains("\"name\":\"test_tool\""));
        assert!(json.contains("\"arguments\""));
    }

    #[test]
    fn test_function_call() {
        let func_call = FunctionCall {
            name: "read_file".to_string(),
            arguments: r#"{"path":"test.txt"}"#.to_string(),
        };
        assert_eq!(func_call.name, "read_file");
        assert!(func_call.arguments.contains("path"));
    }
}
