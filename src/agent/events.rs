//! Agent execution event model.
//!
//! This module defines the event layer emitted during agent execution. Observers
//! receive events through the [`AgentObserver`] trait. The [`NoOpObserver`] is
//! used by callers that do not need event callbacks.
//!
//! # Examples
//!
//! ```
//! use xzatoma::agent::events::{AgentExecutionEvent, AgentObserver, NoOpObserver};
//!
//! let mut observer = NoOpObserver;
//! observer.on_event(AgentExecutionEvent::PromptStarted);
//! ```

/// Events emitted by the agent execution loop.
///
/// Observers receive these events in the order they are emitted during a single
/// call to `Agent::execute_with_observer` or
/// `Agent::execute_provider_messages_with_observer`.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::events::AgentExecutionEvent;
///
/// let event = AgentExecutionEvent::AssistantTextEmitted {
///     text: "Hello".to_string(),
/// };
/// ```
#[derive(Debug, Clone)]
pub enum AgentExecutionEvent {
    /// The agent is about to start executing a user prompt.
    PromptStarted,

    /// A provider completion request is about to be sent.
    ProviderRequestStarted,

    /// A provider response was received from the model.
    ProviderResponseReceived {
        /// The assistant text content, if any was returned.
        text: Option<String>,
        /// Whether the response included one or more tool calls.
        has_tool_calls: bool,
    },

    /// The provider returned non-empty assistant text content.
    AssistantTextEmitted {
        /// The assistant text returned by the provider.
        text: String,
    },

    /// The provider returned reasoning or chain-of-thought content.
    ///
    /// Reasoning content is extracted from either `CompletionResponse.reasoning`
    /// or from inline thinking tags stripped from the assistant text before it
    /// is stored in conversation history. Observers that do not need reasoning
    /// can ignore this event; the `NoOpObserver` discards it.
    ReasoningEmitted {
        /// The reasoning or chain-of-thought text.
        text: String,
    },

    /// A tool call is about to begin executing.
    ToolCallStarted {
        /// Unique tool call identifier assigned by the provider.
        id: String,
        /// Name of the tool being invoked.
        name: String,
        /// Raw JSON-serialized arguments string.
        arguments: String,
    },

    /// A tool call completed successfully.
    ToolCallCompleted {
        /// Unique tool call identifier.
        id: String,
        /// Name of the tool that was invoked.
        name: String,
        /// Output string returned by the tool executor.
        output: String,
    },

    /// A tool call failed with an error.
    ToolCallFailed {
        /// Unique tool call identifier.
        id: String,
        /// Name of the tool that was invoked.
        name: String,
        /// Human-readable error description.
        error: String,
    },

    /// Vision images were attached to the current prompt.
    VisionInputAttached {
        /// Number of image parts detected in the input.
        count: usize,
    },

    /// Cancellation was detected at a safe execution boundary.
    CancellationRequested,

    /// Execution completed and the final response is available.
    ExecutionCompleted {
        /// The final assistant response text.
        response: String,
    },

    /// Execution failed before producing a final response.
    ExecutionFailed {
        /// Human-readable error description.
        error: String,
    },
}

/// Observer trait for agent execution events.
///
/// Implement this trait to observe events during `Agent::execute_with_observer`.
/// The observer is called synchronously in the execution loop so implementations
/// must not block for extended periods.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::events::{AgentExecutionEvent, AgentObserver};
///
/// struct PrintObserver;
///
/// impl AgentObserver for PrintObserver {
///     fn on_event(&mut self, event: AgentExecutionEvent) {
///         eprintln!("event: {:?}", event);
///     }
/// }
/// ```
pub trait AgentObserver: Send {
    /// Called for each execution event in emission order.
    ///
    /// # Arguments
    ///
    /// * `event` - The execution event that was emitted.
    fn on_event(&mut self, event: AgentExecutionEvent);
}

/// A no-op observer that discards all events.
///
/// Used by legacy callers that call `Agent::execute` or
/// `Agent::execute_provider_messages` without needing event callbacks.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::events::{AgentExecutionEvent, AgentObserver, NoOpObserver};
///
/// let mut observer = NoOpObserver;
/// observer.on_event(AgentExecutionEvent::PromptStarted);
/// // nothing happens
/// ```
pub struct NoOpObserver;

impl AgentObserver for NoOpObserver {
    /// Discards the event without any action.
    ///
    /// # Arguments
    ///
    /// * `_event` - The event to discard.
    fn on_event(&mut self, _event: AgentExecutionEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_op_observer_accepts_all_events() {
        let mut observer = NoOpObserver;
        observer.on_event(AgentExecutionEvent::PromptStarted);
        observer.on_event(AgentExecutionEvent::ProviderRequestStarted);
        observer.on_event(AgentExecutionEvent::ProviderResponseReceived {
            text: Some("hello".to_string()),
            has_tool_calls: false,
        });
        observer.on_event(AgentExecutionEvent::AssistantTextEmitted {
            text: "hello".to_string(),
        });
        observer.on_event(AgentExecutionEvent::ReasoningEmitted {
            text: "thinking...".to_string(),
        });
        observer.on_event(AgentExecutionEvent::ToolCallStarted {
            id: "tc-1".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"foo"}"#.to_string(),
        });
        observer.on_event(AgentExecutionEvent::ToolCallCompleted {
            id: "tc-1".to_string(),
            name: "read_file".to_string(),
            output: "contents".to_string(),
        });
        observer.on_event(AgentExecutionEvent::ToolCallFailed {
            id: "tc-1".to_string(),
            name: "read_file".to_string(),
            error: "not found".to_string(),
        });
        observer.on_event(AgentExecutionEvent::VisionInputAttached { count: 2 });
        observer.on_event(AgentExecutionEvent::CancellationRequested);
        observer.on_event(AgentExecutionEvent::ExecutionCompleted {
            response: "done".to_string(),
        });
        observer.on_event(AgentExecutionEvent::ExecutionFailed {
            error: "boom".to_string(),
        });
    }

    #[test]
    fn test_no_op_observer_accepts_reasoning_emitted_event() {
        let mut observer = NoOpObserver;
        observer.on_event(AgentExecutionEvent::ReasoningEmitted {
            text: "chain-of-thought content".to_string(),
        });
        // NoOpObserver must silently discard the event without panicking.
    }

    #[test]
    fn test_agent_execution_event_is_debug_clone() {
        let event = AgentExecutionEvent::AssistantTextEmitted {
            text: "hello".to_string(),
        };
        let cloned = event.clone();
        let _ = format!("{:?}", cloned);
    }

    #[test]
    fn test_custom_observer_receives_events() {
        struct CountingObserver {
            count: usize,
        }
        impl AgentObserver for CountingObserver {
            fn on_event(&mut self, _event: AgentExecutionEvent) {
                self.count += 1;
            }
        }

        let mut observer = CountingObserver { count: 0 };
        observer.on_event(AgentExecutionEvent::PromptStarted);
        observer.on_event(AgentExecutionEvent::ExecutionCompleted {
            response: "done".to_string(),
        });
        assert_eq!(observer.count, 2);
    }
}
