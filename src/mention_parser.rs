//! File mention parser for extracting and resolving @mentions in user input.
//!
//! This module provides functionality to parse mentions from user input strings,
//! supporting various mention types: files, search queries, grep patterns, and URLs.
//!
//! # Mention Syntax
//!
//! - Files: `@filename`, `@path/to/file.rs`, `@file.rs#L10-20`
//! - Search: `@search:"pattern"`
//! - Grep: `@grep:"regex pattern"`
//! - URLs: `@url:https://example.com`
//!
//! # Examples
//!
//! ```ignore
//! use xzatoma::mention_parser::{parse_mentions, Mention};
//!
//! let input = "Please check @config.yaml and search for @search:\"function_name\"";
//! let (mentions, _cleaned) = parse_mentions(input)?;
//!
//! assert_eq!(mentions.len(), 2);
//! ```

use std::path::{Path, PathBuf};

/// A mention extracted from user input
///
/// Represents different types of references the user can include in their input,
/// such as files, search queries, and URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mention {
    /// Reference to a file with optional line range
    File(FileMention),
    /// Search pattern query
    Search(SearchMention),
    /// Grep/regex pattern query
    Grep(SearchMention),
    /// URL reference
    Url(UrlMention),
}

/// File mention with path and optional line range
///
/// Represents a reference to a file in the project, potentially with a specific
/// line range (e.g., `@file.rs#L10-20`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMention {
    /// The file path as mentioned (may be relative or partial)
    pub path: String,
    /// Start line number (1-based), if specified
    pub start_line: Option<usize>,
    /// End line number (1-based), if specified
    pub end_line: Option<usize>,
}

/// Search mention with pattern
///
/// Represents a search or grep pattern query that the user wants to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMention {
    /// The search pattern or regex
    pub pattern: String,
}

/// URL mention with web address
///
/// Represents a reference to a web URL that should be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlMention {
    /// The full URL
    pub url: String,
}

/// Parse mentions from user input
///
/// Extracts all mention types (@-prefixed references) from the input string,
/// returning both the parsed mentions and a cleaned version of the input.
///
/// # Arguments
///
/// * `input` - The user input string potentially containing mentions
///
/// # Returns
///
/// A tuple containing:
/// - `Vec<Mention>` - All extracted mentions
/// - `String` - The input text with mentions still included (for context)
///
/// # Errors
///
/// Returns an error if parsing fails.
///
/// # Examples
///
/// ```ignore
/// use xzatoma::mention_parser::parse_mentions;
///
/// let input = "Check @src/main.rs#L1-10 for details";
/// let (mentions, cleaned) = parse_mentions(input)?;
/// assert_eq!(mentions.len(), 1);
/// ```
pub fn parse_mentions(input: &str) -> crate::error::Result<(Vec<Mention>, String)> {
    let mut mentions = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    while i < len {
        if chars[i] == '@' {
            // Check if this @ is escaped
            if i > 0 && chars[i - 1] == '\\' {
                i += 1;
                continue;
            }

            // Check if @ is at start or preceded by whitespace
            let valid_start = i == 0 || chars[i - 1].is_whitespace();
            if !valid_start {
                i += 1;
                continue;
            }

            // Try to parse a mention starting at position i+1
            if let Some((mention, consumed)) = try_parse_mention_at(&chars, i + 1) {
                mentions.push(mention);
                i += 1 + consumed;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    Ok((mentions, input.to_string()))
}

/// Try to parse a mention starting at the given position
///
/// Returns (Mention, number of characters consumed) if successful.
fn try_parse_mention_at(chars: &[char], start: usize) -> Option<(Mention, usize)> {
    if start >= chars.len() {
        return None;
    }

    let remaining: String = chars[start..].iter().collect();

    // Try URL mention: url:https://...
    if let Some(rest) = remaining.strip_prefix("url:") {
        if let Some(url_end) = find_url_end(rest) {
            let url = rest[..url_end].to_string();
            if is_valid_url(&url) {
                return Some((Mention::Url(UrlMention { url }), 4 + url_end));
            }
        }
    }

    // Try search mention: search:"pattern"
    if let Some(rest) = remaining.strip_prefix("search:\"") {
        if let Some(quote_pos) = rest.find('"') {
            let pattern = rest[..quote_pos].to_string();
            return Some((
                Mention::Search(SearchMention { pattern }),
                8 + quote_pos + 1,
            ));
        }
    }

    // Try grep mention: grep:"pattern"
    if let Some(rest) = remaining.strip_prefix("grep:\"") {
        if let Some(quote_pos) = rest.find('"') {
            let pattern = rest[..quote_pos].to_string();
            return Some((Mention::Grep(SearchMention { pattern }), 6 + quote_pos + 1));
        }
    }

    // Try file mention: path[#L...[-...]]
    if let Some(file_end) = find_file_mention_end(&remaining) {
        let mention_str = &remaining[..file_end];

        // Parse line range if present
        let (file_path, consumed) = if let Some(hash_pos) = mention_str.find('#') {
            let path = &mention_str[..hash_pos];
            let range_str = &mention_str[hash_pos + 1..];

            match parse_line_range(range_str) {
                Some((start_line, end_line)) => (
                    path,
                    hash_pos + 1 + count_line_range_chars(start_line, end_line),
                ),
                None => (mention_str, mention_str.len()),
            }
        } else {
            (mention_str, mention_str.len())
        };

        // Validate path
        if is_valid_file_path(file_path) {
            let (start_line, end_line) = if let Some(hash_pos) = mention_str.find('#') {
                parse_line_range(&mention_str[hash_pos + 1..]).unwrap_or((None, None))
            } else {
                (None, None)
            };

            return Some((
                Mention::File(FileMention {
                    path: file_path.to_string(),
                    start_line,
                    end_line,
                }),
                consumed,
            ));
        }
    }

    None
}

/// Find the end of a file mention in a string
fn find_file_mention_end(s: &str) -> Option<usize> {
    let mut end = 0;
    for (i, ch) in s.chars().enumerate() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-' | '#' => {
                end = i + 1;
            }
            _ if ch.is_whitespace() => {
                return if end > 0 { Some(end) } else { None };
            }
            ',' | ';' | ')' | ']' | '}' | '|' | '&' | '"' | '\'' => {
                return if end > 0 { Some(end) } else { None };
            }
            _ => {
                return if end > 0 { Some(end) } else { None };
            }
        }
    }
    if end > 0 {
        Some(end)
    } else {
        None
    }
}

/// Find the end of a URL mention
fn find_url_end(s: &str) -> Option<usize> {
    let mut end = 0;
    for (i, ch) in s.chars().enumerate() {
        match ch {
            'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | ':'
            | '/'
            | '.'
            | '_'
            | '-'
            | '?'
            | '&'
            | '='
            | '#' => {
                end = i + 1;
            }
            _ if ch.is_whitespace() => {
                return if end > 0 { Some(end) } else { None };
            }
            _ => {
                return if end > 0 { Some(end) } else { None };
            }
        }
    }
    if end > 0 {
        Some(end)
    } else {
        None
    }
}

/// Parse line range from format like "L10-20" or "L10"
fn parse_line_range(range_str: &str) -> Option<(Option<usize>, Option<usize>)> {
    if !range_str.starts_with('L') {
        return None;
    }

    let rest = &range_str[1..];
    if let Some(dash_pos) = rest.find('-') {
        let start = rest[..dash_pos].parse::<usize>().ok()?;
        let end_str = &rest[dash_pos + 1..];
        let end = end_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<usize>().ok())?;

        if start > 0 && end > 0 && start <= end {
            return Some((Some(start), Some(end)));
        }
    } else {
        let line_str = rest.split_whitespace().next()?;
        let line = line_str.parse::<usize>().ok()?;
        if line > 0 {
            return Some((Some(line), None));
        }
    }
    None
}

/// Count characters needed to represent a line range
fn count_line_range_chars(start: Option<usize>, end: Option<usize>) -> usize {
    match (start, end) {
        (Some(s), Some(e)) => format!("L{}-{}", s, e).len(),
        (Some(s), None) => format!("L{}", s).len(),
        _ => 0,
    }
}

/// Check if a file path is valid (not absolute or traversal)
fn is_valid_file_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }

    // Reject absolute paths
    if path.starts_with('/') {
        return false;
    }

    // Reject directory traversal
    if path.contains("../") || path.ends_with("..") || path.starts_with("..") {
        return false;
    }

    // Check for valid characters (only alphanumeric, /, ., _, -)
    path.chars()
        .all(|ch| matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | '.' | '_' | '-'))
}

/// Check if a URL is valid
fn is_valid_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    url.starts_with("http://") || url.starts_with("https://")
}

/// Resolve a mention path to an absolute path
///
/// Converts relative paths to absolute paths within the given working directory,
/// with validation to prevent directory traversal attacks.
///
/// # Arguments
///
/// * `mention_path` - The path from the mention
/// * `working_dir` - The base working directory
///
/// # Returns
///
/// The resolved absolute path, or an error if validation fails.
///
/// # Errors
///
/// Returns an error if:
/// - The resolved path escapes the working directory
/// - Path contains invalid characters
/// - The mention path is absolute
#[allow(dead_code)]
pub fn resolve_mention_path(
    mention_path: &str,
    working_dir: &Path,
) -> crate::error::Result<PathBuf> {
    // Reject absolute paths
    if mention_path.starts_with('/') {
        return Err(anyhow::anyhow!(
            "Absolute paths are not allowed: {}",
            mention_path
        ));
    }

    // Reject directory traversal
    if mention_path.contains("../")
        || mention_path.ends_with("..")
        || mention_path.starts_with("..")
    {
        return Err(anyhow::anyhow!(
            "Directory traversal is not allowed: {}",
            mention_path
        ));
    }

    let path = working_dir.join(mention_path);

    // Canonicalize and validate that result is still within working dir
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // If canonicalize fails (file doesn't exist), just normalize
            path
        }
    };

    let canonical_wd = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.to_path_buf());

    // Check if the path is within the working directory
    if !canonical.starts_with(&canonical_wd) {
        return Err(anyhow::anyhow!(
            "Path escapes working directory: {}",
            mention_path
        ));
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_file_mention() {
        let input = "@config.yaml";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "config.yaml");
                assert_eq!(fm.start_line, None);
                assert_eq!(fm.end_line, None);
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_parse_file_with_path() {
        let input = "@src/main.rs";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "src/main.rs");
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_parse_file_with_line_range() {
        let input = "@file.rs#L10-20";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "file.rs");
                assert_eq!(fm.start_line, Some(10));
                assert_eq!(fm.end_line, Some(20));
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_parse_file_with_single_line() {
        let input = "@README.md#L5";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "README.md");
                assert_eq!(fm.start_line, Some(5));
                assert_eq!(fm.end_line, None);
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_parse_multiple_mentions() {
        let input = "Check @src/main.rs and @README.md please";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 2);
        match &mentions[0] {
            Mention::File(fm) => assert_eq!(fm.path, "src/main.rs"),
            _ => panic!("Expected file mention"),
        }
        match &mentions[1] {
            Mention::File(fm) => assert_eq!(fm.path, "README.md"),
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_parse_search_mention() {
        let input = "Find @search:\"function_name\"";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Search(sm) => assert_eq!(sm.pattern, "function_name"),
            _ => panic!("Expected search mention"),
        }
    }

    #[test]
    fn test_parse_grep_mention() {
        let input = "Search with @grep:\"^fn \"";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Grep(sm) => assert_eq!(sm.pattern, "^fn "),
            _ => panic!("Expected grep mention"),
        }
    }

    #[test]
    fn test_parse_url_mention() {
        let input = "Check @url:https://example.com";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Url(um) => assert_eq!(um.url, "https://example.com"),
            _ => panic!("Expected URL mention"),
        }
    }

    #[test]
    fn test_escaped_at_symbol() {
        let input = "Email me at test\\@example.com";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_at_in_middle_of_word() {
        let input = "The value is test@value not a mention";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_invalid_path_with_traversal() {
        let input = "@../../../etc/passwd";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_invalid_absolute_path() {
        let input = "@/etc/passwd";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_file_with_underscores_and_dashes() {
        let input = "@my_test-file.rs";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "my_test-file.rs");
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_deeply_nested_path() {
        let input = "@src/module/submodule/deep/file.rs";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "src/module/submodule/deep/file.rs");
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_search_with_special_characters() {
        let input = "@search:\"func_name.*pattern\"";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Search(sm) => {
                assert_eq!(sm.pattern, "func_name.*pattern");
            }
            _ => panic!("Expected search mention"),
        }
    }

    #[test]
    fn test_grep_with_regex_pattern() {
        let input = "@grep:\"^\\s*fn\\s+\\w+\"";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Grep(sm) => {
                assert!(sm.pattern.contains("^"));
            }
            _ => panic!("Expected grep mention"),
        }
    }

    #[test]
    fn test_resolve_mention_path_relative() {
        let wd = std::path::PathBuf::from("/tmp");
        let result = resolve_mention_path("src/main.rs", &wd);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_mention_path_rejects_absolute() {
        let wd = std::path::PathBuf::from("/tmp");
        let result = resolve_mention_path("/etc/passwd", &wd);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_mention_path_rejects_traversal() {
        let wd = std::path::PathBuf::from("/tmp");
        let result = resolve_mention_path("../../../etc/passwd", &wd);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_complex_input() {
        let input = "Review @src/main.rs#L1-50, check @README.md, search for @search:\"TODO\" and fetch @url:https://api.example.com";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 4);
    }

    #[test]
    fn test_file_mention_equality() {
        let fm1 = FileMention {
            path: "test.rs".to_string(),
            start_line: Some(1),
            end_line: Some(10),
        };
        let fm2 = FileMention {
            path: "test.rs".to_string(),
            start_line: Some(1),
            end_line: Some(10),
        };
        assert_eq!(fm1, fm2);
    }

    #[test]
    fn test_search_mention_equality() {
        let sm1 = SearchMention {
            pattern: "test".to_string(),
        };
        let sm2 = SearchMention {
            pattern: "test".to_string(),
        };
        assert_eq!(sm1, sm2);
    }

    #[test]
    fn test_url_mention_equality() {
        let um1 = UrlMention {
            url: "https://example.com".to_string(),
        };
        let um2 = UrlMention {
            url: "https://example.com".to_string(),
        };
        assert_eq!(um1, um2);
    }

    #[test]
    fn test_mention_enum_variants() {
        let _file_mention = Mention::File(FileMention {
            path: "test.rs".to_string(),
            start_line: None,
            end_line: None,
        });
        let _search_mention = Mention::Search(SearchMention {
            pattern: "test".to_string(),
        });
        let _grep_mention = Mention::Grep(SearchMention {
            pattern: "^test".to_string(),
        });
        let _url_mention = Mention::Url(UrlMention {
            url: "https://example.com".to_string(),
        });
    }

    #[test]
    fn test_parse_with_punctuation() {
        let input = "Check @src/main.rs, then review @README.md.";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn test_parse_url_with_path() {
        let input = "@url:https://example.com/api/v1/endpoint";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::Url(um) => {
                assert!(um.url.starts_with("https://example.com"));
            }
            _ => panic!("Expected URL mention"),
        }
    }

    #[test]
    fn test_empty_input() {
        let input = "";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_no_mentions() {
        let input = "This is regular text without mentions";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 0);
    }

    #[test]
    fn test_mention_at_end() {
        let input = "Please check @file.rs";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "file.rs");
            }
            _ => panic!("Expected file mention"),
        }
    }

    #[test]
    fn test_mention_at_start() {
        let input = "@file.rs is important";
        let (mentions, _cleaned) = parse_mentions(input).unwrap();
        assert_eq!(mentions.len(), 1);
        match &mentions[0] {
            Mention::File(fm) => {
                assert_eq!(fm.path, "file.rs");
            }
            _ => panic!("Expected file mention"),
        }
    }
}
