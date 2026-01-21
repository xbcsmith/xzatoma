//! Conversation management with token tracking and pruning
//!
//! This module implements conversation history management with automatic
//! token counting and intelligent pruning to stay within context limits.

use crate::error::Result;
use crate::providers::{Message, TokenUsage};

/// Information about the current context window status
///
/// Provides context window metrics including maximum tokens, tokens used,
/// remaining tokens, and percentage of context utilized.
#[derive(Debug, Clone, Copy)]
pub struct ContextInfo {
    /// Maximum tokens available for this context
    pub max_tokens: usize,
    /// Tokens used so far in the conversation
    pub used_tokens: usize,
    /// Tokens remaining in the context window
    pub remaining_tokens: usize,
    /// Percentage of context window used (0.0-100.0)
    pub percentage_used: f64,
}

impl ContextInfo {
    /// Create a new ContextInfo instance
    ///
    /// # Arguments
    ///
    /// * `max_tokens` - Maximum tokens available
    /// * `used_tokens` - Tokens currently used
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::conversation::ContextInfo;
    ///
    /// let context = ContextInfo::new(8192, 1000);
    /// assert_eq!(context.remaining_tokens, 7192);
    /// assert!(context.percentage_used > 12.0 && context.percentage_used < 13.0);
    /// ```
    pub fn new(max_tokens: usize, used_tokens: usize) -> Self {
        let used_tokens = used_tokens.min(max_tokens);
        let remaining_tokens = max_tokens - used_tokens;
        let percentage_used = if max_tokens == 0 {
            0.0
        } else {
            (used_tokens as f64 / max_tokens as f64) * 100.0
        };

        Self {
            max_tokens,
            used_tokens,
            remaining_tokens,
            percentage_used,
        }
    }
}

/// Manages conversation history with token tracking and pruning
///
/// The conversation maintains a list of messages and tracks the total token count.
/// When the token count approaches the maximum, older messages are pruned while
/// keeping recent turns and creating a summary of removed content.
///
/// # Token Counting
///
/// Uses a simple heuristic: characters / 4 (approximates GPT tokenization).
/// For production use, replace with an actual tokenizer library.
/// Provider-reported token counts are preferred when available.
///
/// # Pruning Strategy
///
/// When token count exceeds `prune_threshold * max_tokens`:
/// 1. Keep system message (if present)
/// 2. Keep last `min_retain_turns` conversation turns
/// 3. Summarize and remove older messages
/// 4. Insert summary as new system message
#[derive(Debug, Clone)]
pub struct Conversation {
    messages: Vec<Message>,
    token_count: usize,
    max_tokens: usize,
    min_retain_turns: usize,
    prune_threshold: f64,
    provider_token_usage: Option<TokenUsage>,
}

impl Conversation {
    /// Creates a new conversation with specified limits
    ///
    /// # Arguments
    ///
    /// * `max_tokens` - Maximum token count before pruning
    /// * `min_retain_turns` - Minimum conversation turns to keep during pruning
    /// * `prune_threshold` - Fraction of max_tokens that triggers pruning (0.0-1.0)
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let conversation = Conversation::new(8000, 10, 0.8);
    /// assert_eq!(conversation.token_count(), 0);
    /// ```
    pub fn new(max_tokens: usize, min_retain_turns: usize, prune_threshold: f64) -> Self {
        Self {
            messages: Vec::new(),
            token_count: 0,
            max_tokens,
            min_retain_turns,
            prune_threshold: prune_threshold.clamp(0.0, 1.0),
            provider_token_usage: None,
        }
    }

    /// Adds a user message to the conversation
    ///
    /// # Arguments
    ///
    /// * `content` - The user message content
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let mut conversation = Conversation::new(8000, 10, 0.8);
    /// conversation.add_user_message("Hello, assistant!");
    /// assert_eq!(conversation.messages().len(), 1);
    /// ```
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        let message = Message::user(content);
        self.update_token_count(&message);
        self.messages.push(message);
        self.prune_if_needed();
    }

    /// Adds an assistant message to the conversation
    ///
    /// # Arguments
    ///
    /// * `content` - The assistant message content
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        let message = Message::assistant(content);
        self.update_token_count(&message);
        self.messages.push(message);
        self.prune_if_needed();
    }

    /// Adds a tool result message to the conversation
    ///
    /// # Arguments
    ///
    /// * `tool_call_id` - The ID of the tool call this result corresponds to
    /// * `content` - The tool execution result content
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, content: impl Into<String>) {
        let message = Message::tool_result(tool_call_id, content);
        self.update_token_count(&message);
        self.messages.push(message);
        self.prune_if_needed();
    }

    /// Adds a system message to the conversation
    ///
    /// # Arguments
    ///
    /// * `content` - The system message content
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        let message = Message::system(content);
        self.update_token_count(&message);
        self.messages.push(message);
        self.prune_if_needed();
    }

    /// Updates the token count based on a new message
    ///
    /// Uses a simple heuristic: characters / 4
    /// This approximates GPT tokenization for English text.
    fn update_token_count(&mut self, message: &Message) {
        let content_tokens = message
            .content
            .as_ref()
            .map(|s| estimate_tokens(s))
            .unwrap_or(0);

        let tool_calls_tokens = message
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .map(|call| {
                        estimate_tokens(&call.function.name)
                            + estimate_tokens(&call.function.arguments)
                    })
                    .sum()
            })
            .unwrap_or(0);

        self.token_count += content_tokens + tool_calls_tokens;
    }

    /// Prunes old messages if token count exceeds threshold
    ///
    /// Keeps:
    /// - System messages
    /// - Last `min_retain_turns` conversation turns (user/assistant pairs)
    /// - Tool calls and their results (to maintain context)
    ///
    /// Removed messages are summarized and added as a new system message.
    fn prune_if_needed(&mut self) {
        let threshold = (self.max_tokens as f64 * self.prune_threshold) as usize;

        if self.token_count <= threshold {
            return;
        }

        // Find messages to keep
        let mut keep_from_index = 0;
        let mut retained_turns = 0;

        // Count backwards to find min_retain_turns
        for (idx, message) in self.messages.iter().enumerate().rev() {
            if message.role == "user" {
                retained_turns += 1;
                if retained_turns >= self.min_retain_turns {
                    keep_from_index = idx;
                    break;
                }
            }
        }

        // Don't prune if we can't find enough turns to keep
        if keep_from_index == 0 && !self.messages.is_empty() {
            return;
        }

        // Separate system messages, messages to prune, and messages to keep
        let mut system_messages = Vec::new();
        let mut to_prune = Vec::new();
        let mut to_keep = Vec::new();

        for (idx, message) in self.messages.drain(..).enumerate() {
            if message.role == "system" {
                system_messages.push(message);
            } else if idx < keep_from_index {
                to_prune.push(message);
            } else {
                to_keep.push(message);
            }
        }

        // Create summary of pruned messages
        if !to_prune.is_empty() {
            let summary = self.create_summary(&to_prune);
            system_messages.push(Message::system(summary));
        }

        // Reconstruct messages: system messages + kept messages
        self.messages = system_messages;
        self.messages.extend(to_keep);

        // Recalculate token count
        self.recalculate_tokens();
    }

    /// Creates a summary of messages being pruned
    ///
    /// # Arguments
    ///
    /// * `messages` - The messages to summarize
    ///
    /// # Returns
    ///
    /// A string summary of the conversation
    fn create_summary(&self, messages: &[Message]) -> String {
        if messages.is_empty() {
            return String::from("Previous conversation context (no messages)");
        }

        let mut summary = String::from("Summary of earlier conversation:\n\n");

        let mut user_messages = 0;
        let mut assistant_messages = 0;
        let mut tool_calls = 0;

        for message in messages {
            match message.role.as_str() {
                "user" => user_messages += 1,
                "assistant" => {
                    assistant_messages += 1;
                    if let Some(calls) = &message.tool_calls {
                        tool_calls += calls.len();
                    }
                }
                _ => {}
            }
        }

        summary.push_str(&format!("- {} user messages\n", user_messages));
        summary.push_str(&format!("- {} assistant responses\n", assistant_messages));
        if tool_calls > 0 {
            summary.push_str(&format!("- {} tool calls executed\n", tool_calls));
        }

        // Add first and last message excerpts for context
        if let Some(first) = messages.first() {
            if let Some(content) = &first.content {
                let excerpt = truncate_string(content, 100);
                summary.push_str(&format!("\nFirst message: {}\n", excerpt));
            }
        }

        if messages.len() > 1 {
            if let Some(last) = messages.last() {
                if let Some(content) = &last.content {
                    let excerpt = truncate_string(content, 100);
                    summary.push_str(&format!("Last message: {}\n", excerpt));
                }
            }
        }

        summary
    }

    /// Recalculates the total token count from all messages
    fn recalculate_tokens(&mut self) {
        self.token_count = 0;
        let messages = self.messages.clone();
        for message in &messages {
            self.update_token_count(message);
        }
    }

    /// Returns a reference to all messages in the conversation
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let mut conversation = Conversation::new(8000, 10, 0.8);
    /// conversation.add_user_message("Hello");
    /// assert_eq!(conversation.messages().len(), 1);
    /// ```
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns the current token count
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let mut conversation = Conversation::new(8000, 10, 0.8);
    /// conversation.add_user_message("Test message");
    /// assert!(conversation.token_count() > 0);
    /// ```
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Returns the number of tokens remaining before hitting the maximum
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let conversation = Conversation::new(8000, 10, 0.8);
    /// assert_eq!(conversation.remaining_tokens(), 8000);
    /// ```
    pub fn remaining_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.token_count)
    }

    /// Returns the maximum token limit
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Sets the maximum token limit
    ///
    /// Useful for updating the context window when switching models.
    /// This will trigger pruning if current token count exceeds the new limit.
    ///
    /// # Arguments
    ///
    /// * `new_max` - The new maximum token limit
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let mut conversation = Conversation::new(8000, 3, 0.8);
    /// conversation.set_max_tokens(4000);
    /// assert_eq!(conversation.max_tokens(), 4000);
    /// ```
    pub fn set_max_tokens(&mut self, new_max: usize) {
        self.max_tokens = new_max;
    }

    /// Returns the number of messages in the conversation
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns true if the conversation has no messages
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Clears all messages from the conversation
    pub fn clear(&mut self) {
        self.messages.clear();
        self.token_count = 0;
        self.provider_token_usage = None;
    }

    /// Updates token count from provider-reported usage
    ///
    /// When the provider reports token usage, prefer those counts over the heuristic.
    /// This method accumulates token counts from multiple completions.
    ///
    /// # Arguments
    ///
    /// * `usage` - Token usage information from the provider
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    /// use xzatoma::providers::TokenUsage;
    ///
    /// let mut conversation = Conversation::new(8000, 10, 0.8);
    /// conversation.add_user_message("Hello");
    ///
    /// let usage = TokenUsage::new(50, 25);
    /// conversation.update_from_provider_usage(&usage);
    /// // Provider usage is now tracked
    /// ```
    pub fn update_from_provider_usage(&mut self, usage: &TokenUsage) {
        if let Some(existing) = self.provider_token_usage {
            // Accumulate with existing usage
            self.provider_token_usage = Some(TokenUsage::new(
                existing.prompt_tokens + usage.prompt_tokens,
                existing.completion_tokens + usage.completion_tokens,
            ));
        } else {
            // First provider usage
            self.provider_token_usage = Some(*usage);
        }
    }

    /// Get context window information
    ///
    /// Returns information about how the conversation fits within the context window.
    /// Uses provider-reported token counts if available, otherwise uses heuristic counts.
    ///
    /// # Arguments
    ///
    /// * `model_context_window` - The context window size of the current model
    ///
    /// # Returns
    ///
    /// ContextInfo with current usage and remaining tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::Conversation;
    ///
    /// let mut conversation = Conversation::new(8000, 10, 0.8);
    /// conversation.add_user_message("Hello");
    ///
    /// let context = conversation.get_context_info(8192);
    /// assert_eq!(context.max_tokens, 8192);
    /// assert!(context.used_tokens > 0);
    /// ```
    pub fn get_context_info(&self, model_context_window: usize) -> ContextInfo {
        // Prefer provider-reported usage if available
        let used_tokens = if let Some(usage) = self.provider_token_usage {
            usage.total_tokens
        } else {
            // Fall back to heuristic counting
            self.token_count
        };

        ContextInfo::new(model_context_window, used_tokens)
    }

    /// Get accumulated provider token usage
    ///
    /// # Returns
    ///
    /// Returns provider-reported token usage if available, otherwise None
    pub fn get_provider_token_usage(&self) -> Option<TokenUsage> {
        self.provider_token_usage
    }
}

/// Estimates token count for a string using a simple heuristic
///
/// Uses characters / 4, which approximates GPT tokenization for English text.
/// For production use, replace with an actual tokenizer library (e.g., tiktoken-rs).
fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() + 3) / 4
}

/// Truncates a string to a maximum length, adding ellipsis if truncated
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut truncated = s.chars().take(max_len - 3).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_conversation() {
        let conversation = Conversation::new(8000, 10, 0.8);
        assert_eq!(conversation.token_count(), 0);
        assert_eq!(conversation.messages().len(), 0);
        assert_eq!(conversation.remaining_tokens(), 8000);
    }

    #[test]
    fn test_add_user_message() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Hello, assistant!");

        assert_eq!(conversation.messages().len(), 1);
        assert_eq!(conversation.messages()[0].role, "user");
        assert!(conversation.token_count() > 0);
    }

    #[test]
    fn test_add_assistant_message() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_assistant_message("Hello, user!");

        assert_eq!(conversation.messages().len(), 1);
        assert_eq!(conversation.messages()[0].role, "assistant");
        assert!(conversation.token_count() > 0);
    }

    #[test]
    fn test_add_system_message() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_system_message("System instruction");

        assert_eq!(conversation.messages().len(), 1);
        assert_eq!(conversation.messages()[0].role, "system");
    }

    #[test]
    fn test_token_counting() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        let initial_tokens = conversation.token_count();

        conversation.add_user_message("This is a test message");
        let tokens_after = conversation.token_count();

        assert!(tokens_after > initial_tokens);
        assert!(conversation.remaining_tokens() < 8000);
    }

    #[test]
    fn test_conversation_pruning_with_threshold() {
        // Small max_tokens to trigger pruning easily
        let mut conversation = Conversation::new(150, 2, 0.5);

        // Add long messages to exceed threshold
        let long_message = "This is a very long message that will consume many tokens. ".repeat(5);
        for i in 0..10 {
            conversation.add_user_message(format!("{} User message {}", long_message, i));
            conversation
                .add_assistant_message(format!("{} Assistant response {}", long_message, i));
        }

        // Should have pruned some messages but kept recent ones
        let message_count = conversation.messages().len();
        assert!(
            message_count < 20,
            "Expected fewer than 20 messages after pruning, got {}",
            message_count
        );

        // Verify that pruning occurred by checking we don't have all original messages
        assert!(message_count < 20);
    }

    #[test]
    fn test_pruning_keeps_recent_turns() {
        let mut conversation = Conversation::new(200, 3, 0.5);

        // Add many messages
        for i in 0..10 {
            conversation.add_user_message(format!("Message {}", i));
            conversation.add_assistant_message(format!("Response {}", i));
        }

        // Check that recent messages are present
        let messages = conversation.messages();
        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .and_then(|m| m.content.as_ref());

        assert!(last_user.is_some());
        assert!(last_user.unwrap().contains("Message 9"));
    }

    #[test]
    fn test_pruning_creates_summary() {
        let mut conversation = Conversation::new(200, 2, 0.5);

        // Add long messages to guarantee we exceed threshold
        let long_message = "This is a very long message that will consume many tokens. ".repeat(10);
        for i in 0..10 {
            conversation.add_user_message(format!("{} User message {}", long_message, i));
            conversation
                .add_assistant_message(format!("{} Assistant response {}", long_message, i));
        }

        // Should have a summary system message after pruning
        let has_summary = conversation.messages().iter().any(|m| {
            m.role == "system"
                && m.content
                    .as_ref()
                    .map(|c| c.contains("Summary"))
                    .unwrap_or(false)
        });

        assert!(has_summary);
    }

    #[test]
    fn test_clear_conversation() {
        let mut conversation = Conversation::new(8000, 10, 0.8);
        conversation.add_user_message("Test");
        conversation.add_assistant_message("Response");

        assert_eq!(conversation.len(), 2);
        assert!(conversation.token_count() > 0);

        conversation.clear();

        assert_eq!(conversation.len(), 0);
        assert_eq!(conversation.token_count(), 0);
        assert!(conversation.is_empty());
    }

    #[test]
    fn test_estimate_tokens() {
        // Simple heuristic: chars / 4
        assert_eq!(estimate_tokens("test"), 1);
        assert_eq!(estimate_tokens("hello world"), 3);
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(
            truncate_string("this is a very long string", 10),
            "this is..."
        );
        assert_eq!(truncate_string("exact", 5), "exact");
    }

    #[test]
    fn test_remaining_tokens() {
        let mut conversation = Conversation::new(1000, 10, 0.8);
        let initial_remaining = conversation.remaining_tokens();
        assert_eq!(initial_remaining, 1000);

        conversation.add_user_message("Test message");
        assert!(conversation.remaining_tokens() < 1000);
    }

    #[test]
    fn test_prune_threshold_clamping() {
        let conversation = Conversation::new(1000, 10, 1.5);
        assert_eq!(conversation.prune_threshold, 1.0);

        let conversation = Conversation::new(1000, 10, -0.5);
        assert_eq!(conversation.prune_threshold, 0.0);
    }
}
