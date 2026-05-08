//! Tag extraction utilities for thinking mode.
//!
//! This module provides [`extract_thinking`], which strips vendor-specific
//! inline thinking blocks from provider response text and returns the
//! extracted reasoning content separately. The cleaned text is safe to store
//! in conversation history; the reasoning content can be forwarded to
//! observers via [`AgentExecutionEvent::ReasoningEmitted`].
//!
//! # Supported Tag Formats
//!
//! | Format   | Open tag          | Close tag          |
//! |----------|-------------------|--------------------|
//! | Standard | `<think>`         | `</think>`         |
//! | XZatoma  | `<\|thinking\|>`  | `<\|/thinking\|>`  |
//! | Block    | `<\|channel>`     | `<channel\|>`      |
//!
//! All tag matching is ASCII case-insensitive.
//!
//! [`AgentExecutionEvent::ReasoningEmitted`]: crate::agent::events::AgentExecutionEvent::ReasoningEmitted
//!
//! # Examples
//!
//! ```
//! use xzatoma::agent::extract_thinking;
//!
//! let (clean, reasoning) = extract_thinking("before<think>thought</think>after");
//! assert_eq!(clean, "beforeafter");
//! assert_eq!(reasoning, Some("thought".to_string()));
//! ```

/// Tag pair definitions: `(open_tag, close_tag)` in lowercase.
const TAGS: &[(&str, &str)] = &[
    ("<think>", "</think>"),
    ("<|thinking|>", "<|/thinking|>"),
    ("<|channel>", "<channel|>"),
];

/// Searches for `needle` in `haystack` using ASCII case-insensitive comparison.
///
/// Returns the byte offset of the first match within `haystack`, or `None`
/// if `needle` is not found. Both the `needle` and any matched portion of
/// `haystack` must be composed of ASCII characters for the comparison to
/// produce correct results.
///
/// # Arguments
///
/// * `haystack` - The string slice to search within.
/// * `needle`   - The ASCII string to search for, matched case-insensitively.
///
/// # Returns
///
/// The byte offset of the first match in `haystack`, or `None`.
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    let needle_len = needle_bytes.len();
    if needle_len == 0 {
        return Some(0);
    }
    if needle_len > haystack_bytes.len() {
        return None;
    }
    for i in 0..=(haystack_bytes.len() - needle_len) {
        if haystack_bytes[i..i + needle_len]
            .iter()
            .zip(needle_bytes.iter())
            .all(|(h, n)| h.eq_ignore_ascii_case(n))
        {
            return Some(i);
        }
    }
    None
}

/// Strips inline thinking blocks from `input` and returns the cleaned text
/// together with any extracted reasoning content.
///
/// The function scans `input` for all three supported tag formats in a single
/// left-to-right pass. Each complete `<open_tag>content</close_tag>` block is
/// removed from the returned text and its content is collected as reasoning.
/// When multiple blocks are found their contents are joined with a single
/// newline character.
///
/// Nesting rules: identical open/close pairs are not counted; the first
/// closing tag encountered after an opening tag terminates that block.
///
/// If no tags are present the function returns `(input.to_string(), None)`.
/// The text returned is always safe to store in conversation history because
/// no thinking tags remain.
///
/// # Arguments
///
/// * `input` - The raw provider response text, possibly containing thinking
///   blocks.
///
/// # Returns
///
/// A tuple `(clean_text, reasoning)` where:
/// - `clean_text` is the input with all thinking blocks removed.
/// - `reasoning` is `Some(concatenated_reasoning)` when at least one
///   non-empty block was found, or `None` otherwise.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::extract_thinking;
///
/// // No tags: original text is returned unchanged.
/// let (clean, reasoning) = extract_thinking("no tags here");
/// assert_eq!(clean, "no tags here");
/// assert!(reasoning.is_none());
///
/// // Standard <think> tag.
/// let (clean, reasoning) = extract_thinking("<think>chain of thought</think>answer");
/// assert_eq!(clean, "answer");
/// assert_eq!(reasoning, Some("chain of thought".to_string()));
///
/// // Multiple blocks are concatenated.
/// let (clean, reasoning) = extract_thinking("<think>a</think>x<think>b</think>y");
/// assert_eq!(clean, "xy");
/// assert_eq!(reasoning, Some("a\nb".to_string()));
/// ```
pub fn extract_thinking(input: &str) -> (String, Option<String>) {
    // Fast path: no '<' means no tags can be present.
    if !input.contains('<') {
        return (input.to_string(), None);
    }

    // Avoid allocation if none of the open tags appear in the input.
    if !TAGS
        .iter()
        .any(|(open, _)| find_ascii_case_insensitive(input, open).is_some())
    {
        return (input.to_string(), None);
    }

    let mut clean = String::with_capacity(input.len());
    let mut reasoning_parts: Vec<String> = Vec::new();
    let mut pos = 0usize;

    while pos < input.len() {
        // Find the earliest opening tag at or after `pos`.
        let mut earliest: Option<(usize, usize, &str)> = None; // (abs_start, open_len, close_tag)

        for (open, close) in TAGS {
            if let Some(rel) = find_ascii_case_insensitive(&input[pos..], open) {
                let abs = pos + rel;
                let is_earlier = earliest.map_or(true, |(best, _, _)| abs < best);
                if is_earlier {
                    earliest = Some((abs, open.len(), close));
                }
            }
        }

        match earliest {
            None => {
                // No more opening tags; append the remainder and stop.
                clean.push_str(&input[pos..]);
                break;
            }
            Some((tag_start, open_len, close_tag)) => {
                // Append any text that precedes the opening tag.
                clean.push_str(&input[pos..tag_start]);

                let content_start = tag_start + open_len;

                // Find the first closing tag after the opening tag (case-insensitive).
                match find_ascii_case_insensitive(&input[content_start..], close_tag) {
                    Some(rel_close) => {
                        let abs_close = content_start + rel_close;
                        let content = &input[content_start..abs_close];
                        // Only record non-whitespace reasoning content.
                        if !content.trim().is_empty() {
                            reasoning_parts.push(content.to_string());
                        }
                        pos = abs_close + close_tag.len();
                    }
                    None => {
                        // No closing tag found; treat the open tag as literal text.
                        clean.push_str(&input[tag_start..tag_start + open_len]);
                        pos = tag_start + open_len;
                    }
                }
            }
        }
    }

    let reasoning = if reasoning_parts.is_empty() {
        None
    } else {
        Some(reasoning_parts.join("\n"))
    };

    (clean, reasoning)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_thinking_with_no_tags_returns_original_unchanged() {
        let input = "hello world, no tags here";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, input);
        assert!(reasoning.is_none());
    }

    #[test]
    fn test_extract_thinking_strips_standard_think_tags() {
        let input = "before<think>reasoning content</think>after";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "beforeafter");
        assert_eq!(reasoning, Some("reasoning content".to_string()));
    }

    #[test]
    fn test_extract_thinking_strips_xzatoma_thinking_tags() {
        let input = "text<|thinking|>thought content<|/thinking|>more text";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "textmore text");
        assert_eq!(reasoning, Some("thought content".to_string()));
    }

    #[test]
    fn test_extract_thinking_strips_channel_block_tags() {
        let input = "prefix<|channel>channel reasoning<channel|>suffix";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "prefixsuffix");
        assert_eq!(reasoning, Some("channel reasoning".to_string()));
    }

    #[test]
    fn test_extract_thinking_strips_multiple_blocks_and_concatenates_reasoning() {
        let input = "a<think>r1</think>b<think>r2</think>c";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "abc");
        assert_eq!(reasoning, Some("r1\nr2".to_string()));
    }

    #[test]
    fn test_extract_thinking_is_case_insensitive_for_open_tags() {
        let input = "pre<THINK>uppercase thought</THINK>post";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "prepost");
        assert_eq!(reasoning, Some("uppercase thought".to_string()));
    }

    #[test]
    fn test_extract_thinking_strips_tags_preserves_surrounding_text() {
        let input = "hello <think>reasoning here</think> world";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "hello  world");
        assert_eq!(reasoning, Some("reasoning here".to_string()));
    }

    #[test]
    fn test_extract_thinking_empty_tag_content_yields_none_reasoning() {
        let input = "<think></think>";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "");
        assert!(reasoning.is_none());
    }

    #[test]
    fn test_extract_thinking_all_three_formats_in_single_string() {
        let input =
            "<think>std</think> and <|thinking|>xzat<|/thinking|> and <|channel>chan<channel|>";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, " and  and ");
        assert_eq!(reasoning, Some("std\nxzat\nchan".to_string()));
    }

    #[test]
    fn test_extract_thinking_unclosed_open_tag_treated_as_literal_text() {
        // An unclosed open tag must not silently drop text.
        let input = "before<think>no close tag here";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "before<think>no close tag here");
        assert!(reasoning.is_none());
    }

    #[test]
    fn test_extract_thinking_whitespace_only_content_yields_none_reasoning() {
        let input = "<think>   \n   </think>answer";
        let (clean, reasoning) = extract_thinking(input);
        assert_eq!(clean, "answer");
        assert!(reasoning.is_none());
    }

    #[test]
    fn test_find_ascii_case_insensitive_finds_exact_match() {
        assert_eq!(find_ascii_case_insensitive("hello world", "world"), Some(6));
    }

    #[test]
    fn test_find_ascii_case_insensitive_finds_uppercase_needle_in_lowercase_haystack() {
        assert_eq!(find_ascii_case_insensitive("<think>", "<THINK>"), Some(0));
    }

    #[test]
    fn test_find_ascii_case_insensitive_returns_none_when_absent() {
        assert_eq!(
            find_ascii_case_insensitive("no match here", "<think>"),
            None
        );
    }

    #[test]
    fn test_find_ascii_case_insensitive_empty_needle_returns_zero() {
        assert_eq!(find_ascii_case_insensitive("anything", ""), Some(0));
    }
}
