//! Shared message-conversion helpers for OpenAI-style chat providers.
//!
//! The Copilot and OpenAI providers translate domain [`Message`] tool calls to
//! and from an identical OpenAI-style chat wire shape ([`ChatToolCall`]). These
//! helpers centralize that translation so each provider only keeps its
//! genuinely divergent branches (Copilot folds text-only multimodal content and
//! drops images with a warning; OpenAI serializes multimodal content parts and
//! reasoning effort) inline.
//!
//! [`assistant_message_from_wire`] additionally centralizes the response-message
//! assembly the two providers share (branch on the presence of tool calls,
//! otherwise wrap fallback text). The divergent parts of the response path stay
//! at the call site: Copilot forwards an empty `Some` tool-call vector as a
//! tool-call message while OpenAI filters an empty vector down to plain text,
//! and each provider extracts its fallback text from its own wire content shape.

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

/// Assemble a domain assistant [`Message`] from an OpenAI-style wire response.
///
/// This centralizes the response-message assembly shared by the Copilot and
/// OpenAI providers. When `tool_calls` is `Some`, the wire tool calls are
/// converted with [`chat_tool_calls_to_domain`] and wrapped in
/// [`Message::assistant_with_tools`]; otherwise `content_text` is evaluated and
/// wrapped in [`Message::assistant`].
///
/// The `content_text` closure is evaluated lazily, and only when no tool calls
/// are present. This mirrors the original per-provider control flow so callers
/// never fold response content that would be discarded when tool calls are
/// present.
///
/// Provider-specific divergences are intentionally left at the call site: the
/// Copilot provider forwards its `Option<Vec<ChatToolCall>>` unchanged, so an
/// empty `Some` still produces a tool-call message, whereas the OpenAI provider
/// filters an empty tool-call vector down to `None` (a plain assistant message)
/// before calling this helper. Each provider also supplies its own closure to
/// extract fallback text from its distinct wire content shape.
///
/// # Arguments
///
/// * `tool_calls` - Optional wire tool calls whose emptiness has already been
///   decided by the caller.
/// * `content_text` - Lazily evaluated fallback text used when no tool calls
///   are present.
pub(crate) fn assistant_message_from_wire(
    tool_calls: Option<Vec<ChatToolCall>>,
    content_text: impl FnOnce() -> String,
) -> Message {
    match tool_calls {
        Some(tool_calls) => Message::assistant_with_tools(chat_tool_calls_to_domain(tool_calls)),
        None => Message::assistant(content_text()),
    }
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

    fn wire_tool_call(id: &str, name: &str) -> ChatToolCall {
        ChatToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: ChatFunctionCall {
                name: name.to_string(),
                arguments: "{}".to_string(),
            },
        }
    }

    #[test]
    fn test_assistant_message_from_wire_with_tool_calls_builds_tool_message() {
        let message =
            assistant_message_from_wire(Some(vec![wire_tool_call("call_1", "read_file")]), || {
                panic!("content_text must not be evaluated when tool calls are present")
            });
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content, None);
        let calls = message.tool_calls.expect("tool calls present");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "read_file");
    }

    #[test]
    fn test_assistant_message_from_wire_without_tool_calls_uses_text() {
        let message = assistant_message_from_wire(None, || "hello there".to_string());
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content.as_deref(), Some("hello there"));
        assert!(message.tool_calls.is_none());
    }

    #[test]
    fn test_assistant_message_from_wire_evaluates_text_lazily_only_without_tools() {
        let mut evaluated = false;
        let _ = assistant_message_from_wire(Some(vec![wire_tool_call("call_1", "noop")]), || {
            evaluated = true;
            String::new()
        });
        assert!(
            !evaluated,
            "fallback text closure should not run when tool calls are present"
        );
    }

    /// Parity: the Copilot call convention forwards an empty `Some` unchanged,
    /// so an empty tool-call vector still yields a tool-call message. This
    /// preserves the divergent Copilot branch at the call site.
    #[test]
    fn test_assistant_message_from_wire_copilot_convention_keeps_empty_some() {
        let message = assistant_message_from_wire(Some(vec![]), || "unused".to_string());
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content, None);
        let calls = message
            .tool_calls
            .expect("empty tool-call vector still present");
        assert!(calls.is_empty());
    }

    /// Parity: the OpenAI call convention filters an empty tool-call vector to
    /// `None` before calling the helper, producing a plain assistant message.
    /// This preserves the divergent OpenAI branch at the call site.
    #[test]
    fn test_assistant_message_from_wire_openai_convention_filters_empty_some() {
        let openai_style: Option<Vec<ChatToolCall>> =
            Some(Vec::new()).filter(|tc: &Vec<ChatToolCall>| !tc.is_empty());
        let message = assistant_message_from_wire(openai_style, || "folded text".to_string());
        assert_eq!(message.role, "assistant");
        assert_eq!(message.content.as_deref(), Some("folded text"));
        assert!(message.tool_calls.is_none());
    }
}
