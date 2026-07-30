//! Shared message-conversion helpers for OpenAI-style chat providers.
//!
//! The Copilot and OpenAI providers translate domain [`Message`] tool calls to
//! and from an identical OpenAI-style chat wire shape ([`ChatToolCall`]). These
//! helpers centralize that translation so each provider only keeps its
//! genuinely divergent branches (Copilot folds text-only multimodal content and
//! drops images with a warning; OpenAI serializes multimodal content parts and
//! reasoning effort) inline.

use crate::providers::types::{ChatFunctionCall, ChatToolCall, FunctionCall, Message, ToolCall};

/// Convert a domain message's tool calls into OpenAI-style chat wire tool calls.
///
/// Returns `None` when the message carries no tool calls, mirroring the
/// `Option<Vec<..>>` field shape used by the Copilot and OpenAI wire message
/// structs. The `type` field is always `"function"` and `arguments` is copied
/// verbatim as a serialized JSON string.
///
/// # Arguments
///
/// * `message` - The domain message whose tool calls should be converted.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::conversion::chat_tool_calls_from_message;
/// use xzatoma::providers::{Message, ToolCall, FunctionCall};
///
/// let message = Message::assistant_with_tools(vec![ToolCall {
///     id: "call_1".to_string(),
///     function: FunctionCall {
///         name: "read_file".to_string(),
///         arguments: r#"{"path":"a.rs"}"#.to_string(),
///     },
/// }]);
/// let wire = chat_tool_calls_from_message(&message).unwrap();
/// assert_eq!(wire[0].function.name, "read_file");
/// assert_eq!(wire[0].r#type, "function");
/// ```
pub fn chat_tool_calls_from_message(message: &Message) -> Option<Vec<ChatToolCall>> {
    message.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|tc| ChatToolCall {
                id: tc.id.clone(),
                r#type: "function".to_string(),
                function: ChatFunctionCall {
                    name: tc.function.name.clone(),
                    arguments: tc.function.arguments.clone(),
                },
            })
            .collect()
    })
}

/// Convert OpenAI-style chat wire tool calls back into domain tool calls.
///
/// The wire `type` field is dropped because domain [`ToolCall`]s do not carry
/// it; `id`, function `name`, and serialized `arguments` are moved through
/// unchanged.
///
/// # Arguments
///
/// * `tool_calls` - The wire tool calls parsed from a provider response.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::conversion::chat_tool_calls_to_domain;
/// use xzatoma::providers::{ChatToolCall, ChatFunctionCall};
///
/// let domain = chat_tool_calls_to_domain(vec![ChatToolCall {
///     id: "call_1".to_string(),
///     r#type: "function".to_string(),
///     function: ChatFunctionCall {
///         name: "write_file".to_string(),
///         arguments: r#"{"path":"out.txt"}"#.to_string(),
///     },
/// }]);
/// assert_eq!(domain[0].id, "call_1");
/// assert_eq!(domain[0].function.name, "write_file");
/// ```
pub fn chat_tool_calls_to_domain(tool_calls: Vec<ChatToolCall>) -> Vec<ToolCall> {
    tool_calls
        .into_iter()
        .map(|tc| ToolCall {
            id: tc.id,
            function: FunctionCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_tool_calls_from_message_none_without_tools() {
        let message = Message::user("hi");
        assert!(chat_tool_calls_from_message(&message).is_none());
    }

    #[test]
    fn test_chat_tool_calls_from_message_maps_fields() {
        let message = Message::assistant_with_tools(vec![ToolCall {
            id: "call_1".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"a.rs"}"#.to_string(),
            },
        }]);
        let wire = chat_tool_calls_from_message(&message).expect("tool calls present");
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].id, "call_1");
        assert_eq!(wire[0].r#type, "function");
        assert_eq!(wire[0].function.arguments, r#"{"path":"a.rs"}"#);
    }

    #[test]
    fn test_chat_tool_calls_round_trip_preserves_data() {
        let message = Message::assistant_with_tools(vec![ToolCall {
            id: "call_9".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: r#"{"query":"foo"}"#.to_string(),
            },
        }]);
        let wire = chat_tool_calls_from_message(&message).expect("tool calls present");
        let domain = chat_tool_calls_to_domain(wire);
        assert_eq!(domain[0].id, "call_9");
        assert_eq!(domain[0].function.name, "search");
        assert_eq!(domain[0].function.arguments, r#"{"query":"foo"}"#);
    }
}
