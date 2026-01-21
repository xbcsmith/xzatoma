//! Grep tool for regex-based code search
//!
//! This module provides a grep-like search tool that supports regex pattern matching,
//! file filtering, case sensitivity control, and pagination.

use crate::error::Result;
use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;
use walkdir::WalkDir;

/// Search result with file location and context
///
/// Represents a single match found during a grep search, including
/// the file location, line number, and surrounding context lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    /// File path of the match
    pub file: PathBuf,
    /// Line number (1-based)
    pub line_number: usize,
    /// Full line content
    pub line: String,
    /// Lines before match (for context)
    pub context_before: Vec<String>,
    /// Lines after match (for context)
    pub context_after: Vec<String>,
}

impl SearchMatch {
    /// Format the match with context for display
    ///
    /// # Arguments
    ///
    /// * `max_width` - Maximum width for line display
    ///
    /// # Returns
    ///
    /// Formatted string with context
    pub fn format_with_context(&self, max_width: usize) -> String {
        let mut output = String::new();
        let file_display = self.file.display().to_string();

        // Add file and line info
        output.push_str(&format!("{}:{}\n", file_display, self.line_number));

        // Add context before
        let start_line = self.line_number.saturating_sub(self.context_before.len());
        for (i, context) in self.context_before.iter().enumerate() {
            let line_num = start_line + i;
            let display_line = if context.len() > max_width {
                format!("{}...", &context[..max_width])
            } else {
                context.clone()
            };
            output.push_str(&format!("  {} | {}\n", line_num, display_line));
        }

        // Add the match line (highlighted conceptually)
        let display_line = if self.line.len() > max_width {
            format!("{}...", &self.line[..max_width])
        } else {
            self.line.clone()
        };
        output.push_str(&format!(">{} | {}\n", self.line_number, display_line));

        // Add context after
        for (i, context) in self.context_after.iter().enumerate() {
            let line_num = self.line_number + i + 1;
            let display_line = if context.len() > max_width {
                format!("{}...", &context[..max_width])
            } else {
                context.clone()
            };
            output.push_str(&format!("  {} | {}\n", line_num, display_line));
        }

        output
    }
}

/// Grep tool for regex-based code search
///
/// Supports searching through files with regex patterns, file filtering,
/// case sensitivity control, and pagination.
#[derive(Clone)]
pub struct GrepTool {
    /// Working directory for relative paths
    working_dir: PathBuf,
    /// Maximum results per page
    max_results_per_page: usize,
    /// Context lines to show around matches
    context_lines: usize,
    /// Maximum file size to search (bytes)
    max_file_size: u64,
    /// File patterns to exclude from search
    excluded_patterns: Vec<String>,
}

impl GrepTool {
    /// Create a new GrepTool instance
    ///
    /// # Arguments
    ///
    /// * `working_dir` - Directory to search within
    /// * `max_results_per_page` - Results per page for pagination
    /// * `context_lines` - Lines of context around matches
    /// * `max_file_size` - Maximum file size to search
    /// * `excluded_patterns` - Glob patterns to exclude
    pub fn new(
        working_dir: PathBuf,
        max_results_per_page: usize,
        context_lines: usize,
        max_file_size: u64,
        excluded_patterns: Vec<String>,
    ) -> Self {
        Self {
            working_dir,
            max_results_per_page,
            context_lines,
            max_file_size,
            excluded_patterns,
        }
    }

    /// Search for a pattern in files
    ///
    /// # Arguments
    ///
    /// * `regex` - Regex pattern to search for
    /// * `include_pattern` - Optional glob pattern to include files
    /// * `case_sensitive` - Whether search is case-sensitive
    /// * `offset` - Starting result number for pagination
    ///
    /// # Returns
    ///
    /// Tuple of (matched results, total match count)
    ///
    /// # Errors
    ///
    /// Returns error if regex is invalid or file operations fail
    pub async fn search(
        &self,
        regex: &str,
        include_pattern: Option<&str>,
        case_sensitive: bool,
        offset: usize,
    ) -> Result<(Vec<SearchMatch>, usize)> {
        // Compile regex pattern
        let regex_str = if case_sensitive {
            regex.to_string()
        } else {
            format!("(?i){}", regex)
        };

        let pattern =
            Regex::new(&regex_str).map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

        let mut all_matches = Vec::new();

        // Walk directory tree
        for entry in WalkDir::new(&self.working_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();

            // Check if file matches include pattern
            if let Some(include) = include_pattern {
                let path_str = path.display().to_string();
                if !self.glob_match(&path_str, include) {
                    continue;
                }
            }

            // Check if file is excluded
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if self.should_exclude(file_name, path) {
                    continue;
                }
            }

            // Check file size
            if let Ok(metadata) = fs::metadata(path) {
                if metadata.len() > self.max_file_size {
                    debug!("Skipping {} (too large)", path.display());
                    continue;
                }
            }

            // Read and search file
            if let Ok(content) = fs::read_to_string(path) {
                // Skip binary files (simple heuristic)
                if content.contains('\0') {
                    continue;
                }

                let lines: Vec<&str> = content.lines().collect();
                for (line_idx, line) in lines.iter().enumerate() {
                    if pattern.is_match(line) {
                        let line_number = line_idx + 1;

                        // Extract context
                        let context_start = line_idx.saturating_sub(self.context_lines);
                        let context_end = std::cmp::min(
                            line_idx + self.context_lines,
                            if lines.is_empty() { 0 } else { lines.len() - 1 },
                        );

                        let context_before = lines[context_start..line_idx]
                            .iter()
                            .map(|s| s.to_string())
                            .collect();

                        let context_after = if line_idx < lines.len() - 1 {
                            lines[(line_idx + 1)..=context_end]
                                .iter()
                                .map(|s| s.to_string())
                                .collect()
                        } else {
                            Vec::new()
                        };

                        all_matches.push(SearchMatch {
                            file: path.to_path_buf(),
                            line_number,
                            line: line.to_string(),
                            context_before,
                            context_after,
                        });
                    }
                }
            }
        }

        let total_matches = all_matches.len();

        // Apply pagination
        let start = offset;
        let end = std::cmp::min(offset + self.max_results_per_page, total_matches);

        let paginated = if start < total_matches {
            all_matches[start..end].to_vec()
        } else {
            Vec::new()
        };

        Ok((paginated, total_matches))
    }

    /// Check if path should be excluded based on patterns
    fn should_exclude(&self, file_name: &str, path: &Path) -> bool {
        for pattern in &self.excluded_patterns {
            if self.glob_match(file_name, pattern)
                || self.glob_match(&path.display().to_string(), pattern)
            {
                return true;
            }
        }
        false
    }

    /// Simple glob pattern matching
    ///
    /// # Arguments
    ///
    /// * `text` - Text to match against
    /// * `pattern` - Glob pattern with * and ? wildcards
    ///
    /// # Returns
    ///
    /// True if text matches the pattern
    fn glob_match(&self, text: &str, pattern: &str) -> bool {
        self.glob_match_inner(text, pattern)
    }

    /// Inner recursive glob matching implementation
    fn glob_match_inner(&self, text: &str, pattern: &str) -> bool {
        let text_chars: Vec<char> = text.chars().collect();
        let pattern_chars: Vec<char> = pattern.chars().collect();
        self.glob_match_recursive(&text_chars, 0, &pattern_chars, 0)
    }

    /// Recursive glob matching
    #[allow(clippy::only_used_in_recursion)]
    fn glob_match_recursive(
        &self,
        text: &[char],
        text_idx: usize,
        pattern: &[char],
        pattern_idx: usize,
    ) -> bool {
        // Both exhausted - match
        if text_idx == text.len() && pattern_idx == pattern.len() {
            return true;
        }

        // Pattern exhausted but text remains - no match (unless only * remaining)
        if pattern_idx == pattern.len() {
            return text_idx == text.len();
        }

        let current_pattern = pattern[pattern_idx];

        match current_pattern {
            '*' => {
                // * can match zero characters
                if self.glob_match_recursive(text, text_idx, pattern, pattern_idx + 1) {
                    return true;
                }
                // Or match one character and recurse
                if text_idx < text.len() {
                    return self.glob_match_recursive(text, text_idx + 1, pattern, pattern_idx);
                }
                false
            }
            '?' => {
                // ? matches exactly one character
                if text_idx < text.len() {
                    self.glob_match_recursive(text, text_idx + 1, pattern, pattern_idx + 1)
                } else {
                    false
                }
            }
            c => {
                // Exact character match
                if text_idx < text.len() && text[text_idx] == c {
                    self.glob_match_recursive(text, text_idx + 1, pattern, pattern_idx + 1)
                } else {
                    false
                }
            }
        }
    }
}

#[async_trait]
impl ToolExecutor for GrepTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "grep",
            "description": "Search codebase with regex patterns. Returns matching lines with context.",
            "parameters": {
                "type": "object",
                "properties": {
                    "regex": {
                        "type": "string",
                        "description": "Regular expression pattern to search for"
                    },
                    "include_pattern": {
                        "type": "string",
                        "description": "Optional glob pattern to filter files (e.g., '**/*.rs')"
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Whether search is case-sensitive (default: false)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Starting result number for pagination (default: 0)"
                    }
                },
                "required": ["regex"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let regex = args
            .get("regex")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: regex"))?;

        let include_pattern = args.get("include_pattern").and_then(|v| v.as_str());
        let case_sensitive = args
            .get("case_sensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let (matches, total) = self
            .search(regex, include_pattern, case_sensitive, offset)
            .await?;

        if matches.is_empty() && total == 0 {
            return Ok(ToolResult::success(format!(
                "No matches found for pattern: {}",
                regex
            )));
        }

        let mut output = format!("Found {} match(es) total\n\n", total);
        for m in matches {
            output.push_str(&m.format_with_context(120));
            output.push_str("\n---\n");
        }

        if offset + self.max_results_per_page < total {
            output.push_str(&format!(
                "\n... and {} more matches. Use offset={} for next page.",
                total - (offset + self.max_results_per_page),
                offset + self.max_results_per_page
            ));
        }

        Ok(ToolResult::success(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create test files
        fs::write(
            temp_path.join("file1.rs"),
            "fn main() {\n    println!(\"Hello\");\n}\n",
        )
        .unwrap();
        fs::write(
            temp_path.join("file2.rs"),
            "fn test() {\n    assert_eq!(1, 1);\n}\n",
        )
        .unwrap();
        fs::write(
            temp_path.join("file3.txt"),
            "This is a test file\nwith multiple lines\nfor testing\n",
        )
        .unwrap();

        (temp_dir, temp_path)
    }

    #[tokio::test]
    async fn test_grep_tool_simple_search() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (matches, total) = tool.search("fn", None, false, 0).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn test_grep_tool_case_sensitive() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (_matches, total) = tool.search("FN", None, true, 0).await.unwrap();
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_grep_tool_case_insensitive() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (_matches, total) = tool.search("FN", None, false, 0).await.unwrap();
        assert!(total > 0);
    }

    #[tokio::test]
    async fn test_grep_tool_no_matches() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (matches, total) = tool
            .search("nonexistent_pattern_xyz", None, false, 0)
            .await
            .unwrap();
        assert_eq!(total, 0);
        assert_eq!(matches.len(), 0);
    }

    #[tokio::test]
    async fn test_grep_tool_pagination() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create a file with exactly 50 lines containing "test"
        fs::write(
            temp_path.join("pagination.txt"),
            (0..50)
                .map(|i| format!("line with test on line {}\n", i))
                .collect::<String>(),
        )
        .unwrap();

        let tool = GrepTool::new(temp_path, 10, 0, 1_000_000, vec![]);

        let (matches, total) = tool.search("test", None, false, 0).await.unwrap();
        assert_eq!(total, 50);
        assert_eq!(matches.len(), 10);

        let (matches2, total2) = tool.search("test", None, false, 10).await.unwrap();
        assert_eq!(total2, 50);
        assert_eq!(matches2.len(), 10);
        assert_ne!(matches[0].line_number, matches2[0].line_number);
    }

    #[tokio::test]
    async fn test_grep_tool_include_pattern() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (matches, total) = tool.search("test", Some("*.txt"), false, 0).await.unwrap();
        assert!(total > 0);
        for m in &matches {
            assert_eq!(m.file.extension().and_then(|s| s.to_str()), Some("txt"));
        }
    }

    #[tokio::test]
    async fn test_grep_tool_excluded_patterns() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec!["*.txt".to_string()]);

        let (matches, _total) = tool.search("test", None, false, 0).await.unwrap();
        for m in &matches {
            assert_ne!(m.file.extension().and_then(|s| s.to_str()), Some("txt"));
        }
    }

    #[tokio::test]
    async fn test_grep_tool_context() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let (matches, _) = tool.search("println", None, false, 0).await.unwrap();
        assert!(!matches.is_empty());
        let m = &matches[0];
        assert!(!m.context_before.is_empty() || !m.context_after.is_empty() || m.line_number == 1);
    }

    #[test]
    fn test_search_match_format_with_context() {
        let m = SearchMatch {
            file: PathBuf::from("test.rs"),
            line_number: 2,
            line: "    println!(\"Hello\");".to_string(),
            context_before: vec!["fn main() {".to_string()],
            context_after: vec!["}".to_string()],
        };

        let formatted = m.format_with_context(120);
        assert!(formatted.contains("test.rs:2"));
        assert!(formatted.contains("println"));
        assert!(formatted.contains("main"));
    }

    #[test]
    fn test_grep_tool_glob_match_simple() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        assert!(tool.glob_match("test.rs", "*.rs"));
        assert!(!tool.glob_match("test.txt", "*.rs"));
    }

    #[test]
    fn test_grep_tool_glob_match_star() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        assert!(tool.glob_match("anything", "*"));
        assert!(tool.glob_match("test.rs", "test*"));
    }

    #[test]
    fn test_grep_tool_glob_match_question() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        assert!(tool.glob_match("test.rs", "test.??"));
        assert!(!tool.glob_match("test.rs", "test.?"));
    }

    #[tokio::test]
    async fn test_grep_tool_invalid_regex() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 2, 1_000_000, vec![]);

        let result = tool.search("[invalid", None, false, 0).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_grep_tool_definition() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        let def = tool.tool_definition();
        assert_eq!(def["name"], "grep");
        assert!(def["description"].as_str().unwrap().contains("regex"));
        assert!(def["parameters"]["properties"].get("regex").is_some());
    }

    #[tokio::test]
    async fn test_grep_tool_with_context_lines() {
        let (_temp_dir, temp_path) = setup_test_dir();
        let tool = GrepTool::new(temp_path, 20, 3, 1_000_000, vec![]);

        let (matches, _) = tool.search("println", None, false, 0).await.unwrap();
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_glob_match_exact() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        assert!(tool.glob_match("file.rs", "file.rs"));
        assert!(!tool.glob_match("file.rs", "file.txt"));
    }

    #[test]
    fn test_glob_match_complex() {
        let tool = GrepTool::new(PathBuf::from("."), 20, 2, 1_000_000, vec![]);
        assert!(tool.glob_match("target/", "target/*"));
        assert!(tool.glob_match("test123.lock", "*.lock"));
        assert!(tool.glob_match("path/to/file.rs", "*.rs"));
    }
}
