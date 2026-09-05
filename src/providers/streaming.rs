//! Shared streaming helpers for provider SSE and NDJSON parsing.
//!
//! This module consolidates logic that was previously duplicated across the
//! OpenAI, GitHub Copilot, and Ollama provider implementations. It provides
//! three reusable building blocks:
//!
//! * [`ChatDeltaAccumulator`] - a generic accumulator that assembles a single
//!   [`CompletionResponse`] from a stream of incremental content, reasoning,
//!   tool-call, usage, and finish-reason fragments. It is parameterized over
//!   the tool-call key type `K` so that OpenAI's numeric delta `index`
//!   ordering (`K = u32`) and Copilot's `call_id` string ordering
//!   (`K = String`) are both preserved exactly.
//! * [`LineBuffer`] - a byte buffer that yields complete newline-terminated
//!   lines, correctly handling lines that span multiple network chunks. It is
//!   protocol-agnostic and is used for both SSE (OpenAI, Copilot) and NDJSON
//!   (Ollama) framing.
//! * [`parse_sse_line`] / [`next_sse_data`] - Server-Sent Events line parsing
//!   and an idle-timeout-aware reader that drains complete SSE `data:` payloads
//!   from a byte stream.
//!
//! These helpers introduce no behavior change relative to the per-provider
//! implementations they replace; they exist solely to reduce duplication.

use crate::error::{Result, XzatomaError};
use crate::providers::{
    CompletionResponse, FinishReason, FunctionCall, Message, TokenUsage, ToolCall,
};
use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Generic chat-delta accumulator
// ---------------------------------------------------------------------------

/// A partially assembled tool call built up across streaming fragments.
///
/// The `id` and `name` fields are set once (on the first fragment that
/// supplies a non-empty value) and the `arguments` buffer is appended to on
/// every fragment.
struct PartialChatToolCall {
    /// Stable tool-call identifier.
    id: String,
    /// Function name.
    name: String,
    /// Incrementally appended JSON argument fragments.
    arguments: String,
}

/// Generic accumulator for chat-style streaming completions.
///
/// Collects incremental `content`, optional `reasoning`, `tool_calls` (keyed
/// by `K`), `usage`, and `finish_reason` fragments, then produces a single
/// [`CompletionResponse`] via [`ChatDeltaAccumulator::finalize`].
///
/// The tool-call key type `K` controls the ordering of finalized tool calls:
/// entries are sorted in ascending `K` order. Use `K = u32` to preserve
/// numeric delta-index ordering and `K = String` to preserve identifier
/// ordering.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::streaming::ChatDeltaAccumulator;
///
/// let mut acc = ChatDeltaAccumulator::<u32>::new();
/// acc.push_content("Hello, ");
/// acc.push_content("world");
/// let response = acc.finalize();
/// assert_eq!(response.message.content.as_deref(), Some("Hello, world"));
/// ```
pub struct ChatDeltaAccumulator<K>
where
    K: Ord + Clone + Hash + Eq,
{
    /// Accumulated text content.
    content: String,
    /// Accumulated reasoning content, created on first use.
    reasoning: Option<String>,
    /// Partial tool calls keyed by `K`.
    tool_calls: HashMap<K, PartialChatToolCall>,
    /// Token usage, when the stream provides it.
    usage: Option<TokenUsage>,
    /// Last-seen finish reason; defaults to [`FinishReason::Stop`].
    finish_reason: FinishReason,
}

impl<K> ChatDeltaAccumulator<K>
where
    K: Ord + Clone + Hash + Eq,
{
    /// Create a new, empty accumulator.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::streaming::ChatDeltaAccumulator;
    ///
    /// let acc = ChatDeltaAccumulator::<u32>::new();
    /// assert!(!acc.has_tool_calls());
    /// ```
    pub fn new() -> Self {
        Self {
            content: String::new(),
            reasoning: None,
            tool_calls: HashMap::new(),
            usage: None,
            finish_reason: FinishReason::Stop,
        }
    }

    /// Append a fragment to the accumulated text content.
    ///
    /// # Arguments
    ///
    /// * `text` - The content fragment to append
    pub fn push_content(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Append a fragment to the accumulated reasoning content.
    ///
    /// The reasoning buffer is created on first use, so calling this method
    /// (even with an empty fragment) marks the response as carrying reasoning.
    ///
    /// # Arguments
    ///
    /// * `text` - The reasoning fragment to append
    pub fn push_reasoning(&mut self, text: &str) {
        self.reasoning
            .get_or_insert_with(String::new)
            .push_str(text);
    }

    /// Apply an incremental tool-call fragment.
    ///
    /// The entry for `key` is created if it does not yet exist. The `id` is set
    /// only when currently empty, the `name` is set only when currently empty,
    /// and `arguments` is always appended.
    ///
    /// # Arguments
    ///
    /// * `key` - The tool-call key used for grouping and ordering
    /// * `id` - Optional tool-call identifier fragment
    /// * `name` - Optional function name fragment
    /// * `arguments` - Argument fragment to append (may be empty)
    pub fn apply_tool_call(
        &mut self,
        key: K,
        id: Option<&str>,
        name: Option<&str>,
        arguments: &str,
    ) {
        let entry = self
            .tool_calls
            .entry(key)
            .or_insert_with(|| PartialChatToolCall {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });

        if let Some(id) = id
            && entry.id.is_empty()
        {
            entry.id = id.to_string();
        }

        if let Some(name) = name
            && entry.name.is_empty()
        {
            entry.name = name.to_string();
        }

        entry.arguments.push_str(arguments);
    }

    /// Record the finish reason for the completion.
    ///
    /// # Arguments
    ///
    /// * `reason` - The finish reason to store
    pub fn set_finish_reason(&mut self, reason: FinishReason) {
        self.finish_reason = reason;
    }

    /// Record token usage for the completion.
    ///
    /// # Arguments
    ///
    /// * `usage` - The token usage to store
    pub fn set_usage(&mut self, usage: TokenUsage) {
        self.usage = Some(usage);
    }

    /// Return `true` when at least one tool-call fragment has been applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::streaming::ChatDeltaAccumulator;
    ///
    /// let mut acc = ChatDeltaAccumulator::<u32>::new();
    /// assert!(!acc.has_tool_calls());
    /// acc.apply_tool_call(0, Some("call_1"), Some("do_it"), "{}");
    /// assert!(acc.has_tool_calls());
    /// ```
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Consume the accumulator and produce a [`CompletionResponse`].
    ///
    /// When any tool calls were accumulated, the message is built via
    /// [`Message::assistant_with_tools`] with tool calls sorted in ascending
    /// `K` order; otherwise the accumulated text content is used. Token usage
    /// and the finish reason are always applied, and reasoning is set on the
    /// response when it was captured.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::streaming::ChatDeltaAccumulator;
    ///
    /// // Tool calls are ordered by key, not arrival order.
    /// let mut acc = ChatDeltaAccumulator::<u32>::new();
    /// acc.apply_tool_call(10, Some("b"), Some("second"), "{}");
    /// acc.apply_tool_call(2, Some("a"), Some("first"), "{}");
    /// let response = acc.finalize();
    /// let calls = response.message.tool_calls.unwrap();
    /// assert_eq!(calls[0].id, "a");
    /// assert_eq!(calls[1].id, "b");
    /// ```
    pub fn finalize(self) -> CompletionResponse {
        let message = if !self.tool_calls.is_empty() {
            let mut entries: Vec<(K, PartialChatToolCall)> = self.tool_calls.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            let tool_calls: Vec<ToolCall> = entries
                .into_iter()
                .map(|(_, p)| ToolCall {
                    id: p.id,
                    function: FunctionCall {
                        name: p.name,
                        arguments: p.arguments,
                    },
                })
                .collect();
            Message::assistant_with_tools(tool_calls)
        } else {
            Message::assistant(self.content)
        };

        let base = if let Some(usage) = self.usage {
            CompletionResponse::with_usage(message, usage)
        } else {
            CompletionResponse::new(message)
        };

        let base = base.with_finish_reason(self.finish_reason);

        if let Some(reasoning) = self.reasoning {
            base.set_reasoning(reasoning)
        } else {
            base
        }
    }
}

impl<K> Default for ChatDeltaAccumulator<K>
where
    K: Ord + Clone + Hash + Eq,
{
    /// Create an empty accumulator, equivalent to [`ChatDeltaAccumulator::new`].
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Line buffering
// ---------------------------------------------------------------------------

/// A byte buffer that yields complete newline-terminated lines.
///
/// Bytes are appended with [`LineBuffer::push_bytes`] and complete lines are
/// drained with [`LineBuffer::next_line`]. A line is considered complete when
/// a `\n` byte is present; any trailing bytes after the last `\n` remain
/// buffered until more bytes arrive. This makes the buffer safe to use across
/// arbitrary network chunk boundaries, including lines split mid-way between
/// two chunks.
///
/// The returned line excludes the trailing `\n` but preserves any other
/// characters (such as a trailing `\r`); callers that require trimming should
/// trim the returned string.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::streaming::LineBuffer;
///
/// let mut buffer = LineBuffer::new();
/// // A single line split across two pushes is only yielded once complete.
/// buffer.push_bytes(b"hel");
/// assert_eq!(buffer.next_line(), None);
/// buffer.push_bytes(b"lo\nworld");
/// assert_eq!(buffer.next_line().as_deref(), Some("hello"));
/// assert_eq!(buffer.next_line(), None);
/// ```
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    /// Create a new, empty line buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::streaming::LineBuffer;
    ///
    /// let mut buffer = LineBuffer::new();
    /// assert_eq!(buffer.next_line(), None);
    /// ```
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append bytes to the buffer.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The bytes to append
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Remove and return the next complete line, without its trailing `\n`.
    ///
    /// Returns `None` when the buffer does not yet contain a complete line.
    /// Invalid UTF-8 is replaced lossily, matching the behavior of the
    /// per-provider line handling this buffer replaces.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::providers::streaming::LineBuffer;
    ///
    /// let mut buffer = LineBuffer::new();
    /// buffer.push_bytes(b"a\nb\n");
    /// assert_eq!(buffer.next_line().as_deref(), Some("a"));
    /// assert_eq!(buffer.next_line().as_deref(), Some("b"));
    /// assert_eq!(buffer.next_line(), None);
    /// ```
    pub fn next_line(&mut self) -> Option<String> {
        let pos = self.buf.iter().position(|&b| b == b'\n')?;
        let line_bytes: Vec<u8> = self.buf.drain(..=pos).collect();
        // Exclude the trailing '\n' byte from the returned line.
        let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]).to_string();
        Some(line)
    }
}

impl Default for LineBuffer {
    /// Create an empty line buffer, equivalent to [`LineBuffer::new`].
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SSE line parsing
// ---------------------------------------------------------------------------

/// The classification of a single Server-Sent Events line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseLine {
    /// A comment line (starts with `:`) or an ignored SSE field such as
    /// `event:` or `id:`.
    Comment,
    /// An empty line (blank after trimming).
    Empty,
    /// A `data:` line carrying a JSON payload.
    Data(String),
    /// The `[DONE]` sentinel that terminates an SSE stream.
    Done,
}

/// Parse a single Server-Sent Events line into an [`SseLine`].
///
/// The line is trimmed first. An empty line yields [`SseLine::Empty`]. A line
/// beginning with `data: ` (the SSE data prefix, including its trailing space)
/// yields [`SseLine::Done`] for the `[DONE]` sentinel or [`SseLine::Data`]
/// with the remaining payload otherwise. Every other line -- SSE metadata
/// fields (`event:`, `id:`), comment lines (`:`), and any unrecognized line --
/// yields [`SseLine::Comment`] and is intended to be ignored.
///
/// This preserves the exact contract of the previous per-provider SSE line
/// parser, including requiring the space in the `data: ` prefix.
///
/// # Arguments
///
/// * `line` - A single raw line from the SSE stream
///
/// # Examples
///
/// ```
/// use xzatoma::providers::streaming::{parse_sse_line, SseLine};
///
/// assert_eq!(
///     parse_sse_line("data: {\"k\":1}"),
///     SseLine::Data("{\"k\":1}".to_string())
/// );
/// assert_eq!(parse_sse_line("data: [DONE]"), SseLine::Done);
/// assert_eq!(parse_sse_line(": keepalive"), SseLine::Comment);
/// assert_eq!(parse_sse_line("event: message"), SseLine::Comment);
/// assert_eq!(parse_sse_line("   "), SseLine::Empty);
/// ```
pub fn parse_sse_line(line: &str) -> SseLine {
    let line = line.trim();

    if line.is_empty() {
        return SseLine::Empty;
    }

    if let Some(data) = line.strip_prefix("data: ") {
        if data.trim() == "[DONE]" {
            return SseLine::Done;
        }
        return SseLine::Data(data.to_string());
    }

    // Comment lines and unrecognized SSE fields (event:, id:, etc.) are ignored.
    SseLine::Comment
}

// ---------------------------------------------------------------------------
// Idle-timeout-aware SSE reader
// ---------------------------------------------------------------------------

/// A meaningful SSE event drained from a byte stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseDataEvent {
    /// A `data:` payload ready to be deserialized by the caller.
    Payload(String),
    /// The `[DONE]` sentinel; the caller should stop reading.
    Done,
}

/// Read the next meaningful SSE event from a byte stream, honoring an idle
/// timeout.
///
/// This function first drains any complete lines already present in `buffer`,
/// returning the first that classifies as a [`SseLine::Data`] (as
/// [`SseDataEvent::Payload`]) or [`SseLine::Done`] (as [`SseDataEvent::Done`]);
/// comment and empty lines are skipped. When the buffer holds no complete line,
/// it awaits the next chunk from `stream` under a [`tokio::time::timeout`] of
/// `idle_timeout` and repeats.
///
/// # Arguments
///
/// * `stream` - The byte stream to read from
/// * `buffer` - A [`LineBuffer`] carrying any partially received line
/// * `idle_timeout` - Maximum time to wait for the next chunk before failing
/// * `idle_error` - Constructs the error returned on idle timeout, given the
///   timeout in whole seconds
///
/// # Returns
///
/// * `Ok(Some(event))` for the next data payload or the done sentinel
/// * `Ok(None)` when the stream ends without further events
///
/// # Errors
///
/// Returns the error produced by `idle_error` when no chunk arrives within
/// `idle_timeout`, or an [`XzatomaError::Provider`] wrapping a stream read
/// error.
///
/// # Examples
///
/// ```no_run
/// use std::time::Duration;
/// use xzatoma::error::XzatomaError;
/// use xzatoma::providers::streaming::{next_sse_data, LineBuffer, SseDataEvent};
///
/// # async fn run<S>(stream: &mut S) -> xzatoma::error::Result<()>
/// # where
/// #     S: futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin,
/// # {
/// let mut buffer = LineBuffer::new();
/// let idle = Duration::from_secs(30);
/// while let Some(event) = next_sse_data(stream, &mut buffer, idle, |secs| {
///     XzatomaError::Provider(format!("idle timeout after {}s", secs))
/// })
/// .await?
/// {
///     match event {
///         SseDataEvent::Payload(json) => {
///             // deserialize `json`
///             let _ = json;
///         }
///         SseDataEvent::Done => break,
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub async fn next_sse_data<S>(
    stream: &mut S,
    buffer: &mut LineBuffer,
    idle_timeout: Duration,
    idle_error: impl Fn(u64) -> XzatomaError,
) -> Result<Option<SseDataEvent>>
where
    S: futures::Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin,
{
    use futures::StreamExt;

    loop {
        // Drain any complete lines already buffered before awaiting more bytes.
        while let Some(line) = buffer.next_line() {
            match parse_sse_line(&line) {
                SseLine::Data(payload) => return Ok(Some(SseDataEvent::Payload(payload))),
                SseLine::Done => return Ok(Some(SseDataEvent::Done)),
                SseLine::Comment | SseLine::Empty => continue,
            }
        }

        match tokio::time::timeout(idle_timeout, stream.next()).await {
            Ok(Some(Ok(bytes))) => {
                buffer.push_bytes(&bytes);
                continue;
            }
            Ok(Some(Err(e))) => {
                return Err(XzatomaError::Provider(format!(
                    "Error reading SSE stream: {}",
                    e
                )));
            }
            Ok(None) => return Ok(None),
            Err(_elapsed) => return Err(idle_error(idle_timeout.as_secs())),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulator_concatenates_content() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.push_content("Hello");
        acc.push_content(" ");
        acc.push_content("world");
        let response = acc.finalize();
        assert_eq!(response.message.content.as_deref(), Some("Hello world"));
        assert!(response.message.tool_calls.is_none());
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert!(response.usage.is_none());
        assert!(response.reasoning.is_none());
    }

    #[test]
    fn test_accumulator_concatenates_reasoning() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.push_reasoning("Let me think...");
        acc.push_reasoning(" done.");
        acc.push_content("Answer");
        let response = acc.finalize();
        assert_eq!(response.message.content.as_deref(), Some("Answer"));
        assert_eq!(response.reasoning.as_deref(), Some("Let me think... done."));
    }

    #[test]
    fn test_accumulator_reasoning_absent_when_never_pushed() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.push_content("Result");
        let response = acc.finalize();
        assert!(response.reasoning.is_none());
    }

    #[test]
    fn test_accumulator_multi_fragment_tool_call_arguments() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        // First fragment: id and name, partial arguments.
        acc.apply_tool_call(0, Some("call_abc"), Some("read_file"), "{\"path\"");
        // Second fragment: no id/name, more arguments.
        acc.apply_tool_call(0, None, None, ":\"test.txt\"}");
        acc.set_finish_reason(FinishReason::ToolCalls);
        let response = acc.finalize();

        let calls = response
            .message
            .tool_calls
            .expect("expected tool calls in response");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_abc");
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments, "{\"path\":\"test.txt\"}");
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
    }

    #[test]
    fn test_accumulator_id_and_name_set_only_when_empty() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.apply_tool_call(0, Some("first_id"), Some("first_name"), "");
        // A later fragment must not overwrite an already-populated id or name.
        acc.apply_tool_call(0, Some("second_id"), Some("second_name"), "");
        let response = acc.finalize();
        let calls = response.message.tool_calls.expect("expected tool calls");
        assert_eq!(calls[0].id, "first_id");
        assert_eq!(calls[0].function.name, "first_name");
    }

    #[test]
    fn test_accumulator_orders_tool_calls_by_numeric_key() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        // Insert keys out of order, including one that would sort wrong
        // lexicographically (2 vs 10) to prove numeric ordering.
        acc.apply_tool_call(10, Some("b"), Some("second"), "{}");
        acc.apply_tool_call(2, Some("a"), Some("first"), "{}");
        let response = acc.finalize();
        let calls = response.message.tool_calls.expect("expected tool calls");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "a", "key 2 must come before key 10");
        assert_eq!(calls[1].id, "b");
    }

    #[test]
    fn test_accumulator_orders_tool_calls_by_string_key() {
        let mut acc = ChatDeltaAccumulator::<String>::new();
        acc.apply_tool_call("call_b".to_string(), Some("call_b"), Some("b"), "{}");
        acc.apply_tool_call("call_a".to_string(), Some("call_a"), Some("a"), "{}");
        let response = acc.finalize();
        let calls = response.message.tool_calls.expect("expected tool calls");
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[1].id, "call_b");
    }

    #[test]
    fn test_accumulator_finalize_with_usage() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.push_content("Done");
        acc.set_usage(TokenUsage::new(20, 10));
        let response = acc.finalize();
        let usage = response.usage.expect("expected usage");
        assert_eq!(usage.prompt_tokens, 20);
        assert_eq!(usage.completion_tokens, 10);
    }

    #[test]
    fn test_accumulator_finalize_without_usage() {
        let mut acc = ChatDeltaAccumulator::<u32>::new();
        acc.push_content("Done");
        let response = acc.finalize();
        assert!(response.usage.is_none());
    }

    #[test]
    fn test_accumulator_empty_produces_empty_assistant_message() {
        let acc = ChatDeltaAccumulator::<u32>::new();
        let response = acc.finalize();
        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content.as_deref(), Some(""));
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert!(response.usage.is_none());
        assert!(response.reasoning.is_none());
    }

    #[test]
    fn test_line_buffer_single_complete_line() {
        let mut buffer = LineBuffer::new();
        buffer.push_bytes(b"hello\n");
        assert_eq!(buffer.next_line().as_deref(), Some("hello"));
        assert_eq!(buffer.next_line(), None);
    }

    #[test]
    fn test_line_buffer_multiple_lines_in_one_push() {
        let mut buffer = LineBuffer::new();
        buffer.push_bytes(b"a\nb\nc\n");
        assert_eq!(buffer.next_line().as_deref(), Some("a"));
        assert_eq!(buffer.next_line().as_deref(), Some("b"));
        assert_eq!(buffer.next_line().as_deref(), Some("c"));
        assert_eq!(buffer.next_line(), None);
    }

    #[test]
    fn test_line_buffer_line_split_across_pushes() {
        let mut buffer = LineBuffer::new();
        buffer.push_bytes(b"hel");
        assert_eq!(buffer.next_line(), None, "incomplete line must not yield");
        buffer.push_bytes(b"lo\n");
        assert_eq!(buffer.next_line().as_deref(), Some("hello"));
        assert_eq!(buffer.next_line(), None);
    }

    #[test]
    fn test_line_buffer_retains_incomplete_remainder() {
        let mut buffer = LineBuffer::new();
        buffer.push_bytes(b"first\nsecond-part");
        assert_eq!(buffer.next_line().as_deref(), Some("first"));
        assert_eq!(buffer.next_line(), None);
        buffer.push_bytes(b"-rest\n");
        assert_eq!(buffer.next_line().as_deref(), Some("second-part-rest"));
    }

    #[test]
    fn test_line_buffer_preserves_carriage_return() {
        let mut buffer = LineBuffer::new();
        buffer.push_bytes(b"line\r\n");
        // The trailing \n is stripped but the \r is preserved for the caller.
        assert_eq!(buffer.next_line().as_deref(), Some("line\r"));
    }

    #[test]
    fn test_parse_sse_line_data_payload() {
        assert_eq!(
            parse_sse_line("data: {\"type\":\"message\"}"),
            SseLine::Data("{\"type\":\"message\"}".to_string())
        );
    }

    #[test]
    fn test_parse_sse_line_done_sentinel() {
        assert_eq!(parse_sse_line("data: [DONE]"), SseLine::Done);
    }

    #[test]
    fn test_parse_sse_line_comment_and_metadata() {
        assert_eq!(parse_sse_line(": comment"), SseLine::Comment);
        assert_eq!(parse_sse_line("event: message"), SseLine::Comment);
        assert_eq!(parse_sse_line("id: 123"), SseLine::Comment);
    }

    #[test]
    fn test_parse_sse_line_empty_lines() {
        assert_eq!(parse_sse_line(""), SseLine::Empty);
        assert_eq!(parse_sse_line("   "), SseLine::Empty);
        assert_eq!(parse_sse_line("\n"), SseLine::Empty);
    }

    #[test]
    fn test_parse_sse_line_trims_before_parsing() {
        assert_eq!(
            parse_sse_line("  data: {\"k\":1}  "),
            SseLine::Data("{\"k\":1}".to_string())
        );
    }

    #[tokio::test]
    async fn test_next_sse_data_parses_across_chunk_boundaries_and_stops_at_done() {
        // The payload for a single event is split across three byte chunks,
        // and a trailing [DONE] sentinel terminates the stream.
        let chunks: Vec<reqwest::Result<bytes::Bytes>> = vec![
            Ok(bytes::Bytes::from_static(b"data: {\"a\"")),
            Ok(bytes::Bytes::from_static(b":1}\n\n")),
            Ok(bytes::Bytes::from_static(b"data: [DONE]\n\n")),
        ];
        let mut stream = futures::stream::iter(chunks);
        let mut buffer = LineBuffer::new();
        let idle = Duration::from_secs(5);
        let idle_error = |secs: u64| XzatomaError::Provider(format!("idle {}s", secs));

        let first = next_sse_data(&mut stream, &mut buffer, idle, idle_error)
            .await
            .expect("first read must not error");
        assert_eq!(
            first,
            Some(SseDataEvent::Payload("{\"a\":1}".to_string())),
            "the payload split across chunks must be reassembled"
        );

        let second = next_sse_data(&mut stream, &mut buffer, idle, idle_error)
            .await
            .expect("second read must not error");
        assert_eq!(second, Some(SseDataEvent::Done));
    }

    #[tokio::test]
    async fn test_next_sse_data_returns_none_when_stream_ends() {
        let chunks: Vec<reqwest::Result<bytes::Bytes>> = vec![];
        let mut stream = futures::stream::iter(chunks);
        let mut buffer = LineBuffer::new();
        let result = next_sse_data(&mut stream, &mut buffer, Duration::from_secs(5), |secs| {
            XzatomaError::Provider(format!("idle {}s", secs))
        })
        .await
        .expect("read must not error");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_next_sse_data_triggers_idle_timeout() {
        // A stream that never yields must trigger the idle-timeout error.
        let mut stream = futures::stream::pending::<reqwest::Result<bytes::Bytes>>();
        let mut buffer = LineBuffer::new();
        let result = next_sse_data(
            &mut stream,
            &mut buffer,
            Duration::from_millis(20),
            |secs| XzatomaError::Provider(format!("idle timeout after {}s", secs)),
        )
        .await;

        let err = result.expect_err("a stalled stream must return an error");
        assert!(
            err.to_string().contains("idle timeout"),
            "error must be the idle-timeout error: {}",
            err
        );
    }
}
