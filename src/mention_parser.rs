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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use tracing::debug;

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

/// Loaded content from a file mention with metadata
///
/// Contains the file contents and metadata like size, line count, and modification time.
#[derive(Debug, Clone)]
pub struct MentionContent {
    /// The resolved file path (used in cache operations)
    #[allow(dead_code)]
    pub path: PathBuf,
    /// The original mention path for display purposes
    pub original_path: String,
    /// File contents
    pub contents: String,
    /// File size in bytes
    #[allow(dead_code)]
    pub size_bytes: u64,
    /// Total number of lines in file
    pub line_count: usize,
    /// Last modification time
    pub mtime: Option<SystemTime>,
}

impl MentionContent {
    /// Create a new MentionContent instance
    ///
    /// # Arguments
    ///
    /// * `path` - The file path
    /// * `original_path` - The original mention path for display
    /// * `contents` - The file contents
    /// * `mtime` - Optional modification time
    pub fn new(
        path: PathBuf,
        original_path: String,
        contents: String,
        mtime: Option<SystemTime>,
    ) -> Self {
        let size_bytes = contents.len() as u64;
        let line_count = contents.lines().count();
        Self {
            path,
            original_path,
            contents,
            size_bytes,
            line_count,
            mtime,
        }
    }

    /// Extract a line range from the content
    ///
    /// # Arguments
    ///
    /// * `start_line` - Starting line (1-based, inclusive)
    /// * `end_line` - Ending line (1-based, inclusive)
    ///
    /// # Returns
    ///
    /// The extracted lines as a string, or error if range is invalid
    pub fn extract_line_range(
        &self,
        start_line: usize,
        end_line: usize,
    ) -> crate::error::Result<String> {
        if start_line == 0 || end_line == 0 || start_line > end_line {
            return Err(anyhow::anyhow!(
                "Invalid line range: {}-{} (must be 1-based and start <= end)",
                start_line,
                end_line
            ));
        }

        if start_line > self.line_count {
            return Err(anyhow::anyhow!(
                "Start line {} exceeds file line count {}",
                start_line,
                self.line_count
            ));
        }

        let end = std::cmp::min(end_line, self.line_count);
        let extracted: String = self
            .contents
            .lines()
            .enumerate()
            .filter(|(idx, _)| {
                let line_num = idx + 1;
                line_num >= start_line && line_num <= end
            })
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(extracted)
    }

    /// Format content with file header and metadata
    ///
    /// # Returns
    ///
    /// Formatted string suitable for inclusion in prompts
    pub fn format_with_header(&self, start_line: Option<usize>, end_line: Option<usize>) -> String {
        let path_str = &self.original_path;
        let line_info = match (start_line, end_line) {
            (Some(s), Some(e)) => format!(" (Lines {}-{})", s, e),
            (Some(s), None) => format!(" (Line {})", s),
            _ => format!(" ({} lines)", self.line_count),
        };

        format!(
            "File: {}{}\n\n```\n{}\n```",
            path_str, line_info, self.contents
        )
    }
}

/// Cache for loaded file contents with mtime-based invalidation
///
/// Stores loaded file contents indexed by path and invalidates entries
/// when files are modified.
#[derive(Debug, Clone)]
pub struct MentionCache {
    cache: HashMap<PathBuf, MentionContent>,
}

impl MentionCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Get cached content if valid (not modified since cached)
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to look up
    ///
    /// # Returns
    ///
    /// The cached content if valid, None if not in cache or stale
    pub fn get(&self, path: &Path) -> Option<MentionContent> {
        let cached = self.cache.get(path)?;

        // Check if file was modified since we cached it
        if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(current_mtime) = metadata.modified() {
                if let Some(cached_mtime) = cached.mtime {
                    if current_mtime <= cached_mtime {
                        debug!("Cache hit for {}", path.display());
                        return Some(cached.clone());
                    }
                }
            }
        }

        // Cache is stale
        None
    }

    /// Store content in cache
    ///
    /// # Arguments
    ///
    /// * `path` - The file path
    /// * `content` - The content to cache
    pub fn insert(&mut self, path: PathBuf, content: MentionContent) {
        debug!("Cached content for {}", path.display());
        self.cache.insert(path, content);
    }

    /// Clear the entire cache
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get number of entries in cache
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for MentionCache {
    fn default() -> Self {
        Self::new()
    }
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

/// Load file content for a file mention
///
/// # Arguments
///
/// * `mention` - The file mention to load
/// * `working_dir` - The working directory for path resolution
/// * `max_size_bytes` - Maximum file size to load (from ToolsConfig)
///
/// # Returns
///
/// The loaded MentionContent or an error
///
/// # Errors
///
/// Returns error if:
/// - File cannot be resolved or is outside working directory
/// - File doesn't exist or is not readable
/// - File exceeds size limit
/// - File is binary
pub async fn load_file_content(
    mention: &FileMention,
    working_dir: &Path,
    max_size_bytes: u64,
) -> crate::error::Result<MentionContent> {
    // Resolve the file path
    let file_path = resolve_mention_path(&mention.path, working_dir)?;

    // Check file exists
    if !file_path.exists() {
        return Err(anyhow::anyhow!("File not found: {}", mention.path));
    }

    // Get file metadata
    let metadata = fs::metadata(&file_path).await?;

    if !metadata.is_file() {
        return Err(anyhow::anyhow!("Not a file: {}", mention.path));
    }

    let file_size = metadata.len();

    // Check size limit
    if file_size > max_size_bytes {
        return Err(anyhow::anyhow!(
            "File too large: {} bytes exceeds limit of {} bytes",
            file_size,
            max_size_bytes
        ));
    }

    // Read file contents
    let contents = fs::read_to_string(&file_path).await?;

    // Check for binary file (simple heuristic: contains null bytes)
    if contents.contains('\0') {
        return Err(anyhow::anyhow!(
            "Binary file cannot be loaded: {}",
            mention.path
        ));
    }

    // Get modification time
    let mtime = metadata.modified().ok();

    Ok(MentionContent::new(
        file_path,
        mention.path.clone(),
        contents,
        mtime,
    ))
}

/// Format search results for display in prompts
///
/// # Arguments
///
/// * `matches` - Vector of search matches from grep tool
/// * `pattern` - The search pattern that was executed
///
/// # Returns
///
/// Formatted string with search results suitable for prompt inclusion
#[allow(dead_code)]
pub fn format_search_results(matches: &[crate::tools::SearchMatch], pattern: &str) -> String {
    if matches.is_empty() {
        return format!("Search results for '{}': No matches found", pattern);
    }

    let mut output = format!(
        "Search results for '{}': {} match(es)\n\n",
        pattern,
        matches.len()
    );

    for m in matches {
        output.push_str(&m.format_with_context(120));
        output.push_str("\n---\n");
    }

    output
}

/// Cache entry for URL content with TTL
///
/// Stores both the formatted content and a small set of metadata so the
/// mention pipeline can present concise success messages (size, type, status).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UrlContentCache {
    pub url: String,
    pub content: String,
    pub timestamp: std::time::SystemTime,
    /// Content-Type header as reported by the remote
    pub content_type: Option<String>,
    /// Size in bytes of the fetched content (may be truncated to max size)
    pub size_bytes: Option<usize>,
    /// HTTP status code reported by the fetch
    pub status_code: Option<u16>,
    /// Whether the content stored here was truncated due to size limits
    pub truncated: bool,
}

impl UrlContentCache {
    /// Check if cache entry has expired (5 minute TTL)
    fn is_expired(&self) -> bool {
        self.timestamp
            .elapsed()
            .map(|elapsed| elapsed > std::time::Duration::from_secs(5 * 60))
            .unwrap_or(true)
    }
}

/// Cache entry for search results
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SearchResultsCache {
    pattern: String,
    matches: Vec<crate::tools::SearchMatch>,
    timestamp: std::time::SystemTime,
}

/// Expand a short or abbreviated mention into a likely path in the repository.
///
/// This tries a small set of common expansions:
///  - If the name has no extension, try common extensions like `.rs`, `.md`, `.yaml`, `.toml`, `.json`
///  - Try `src/{name}.rs` if a `src` directory exists
///  - Try exact filename matches anywhere in the tree
///
/// Returns `Some(PathBuf)` when a good direct expansion is found.
pub fn expand_common_abbreviations(
    mention_path: &str,
    working_dir: &std::path::Path,
) -> Option<std::path::PathBuf> {
    use walkdir::WalkDir;

    // 1) If it already exists as typed (relative to working dir), return it
    let candidate = working_dir.join(mention_path);
    if candidate.exists() {
        return Some(candidate);
    }

    // 2) If no extension, try common extensions
    if !mention_path.contains('.') {
        let exts = ["rs", "md", "yaml", "toml", "json"];
        for ext in &exts {
            let try_path = working_dir.join(format!("{}.{}", mention_path, ext));
            if try_path.exists() {
                return Some(try_path);
            }
        }

        // try src/<name>.rs
        let src_try = working_dir.join("src").join(format!("{}.rs", mention_path));
        if src_try.exists() {
            return Some(src_try);
        }
    }

    // 3) Try to find exact filename match anywhere under working dir
    let target = mention_path.to_lowercase();
    for entry in WalkDir::new(working_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(fname) = entry.path().file_name().and_then(|s| s.to_str()) {
            if fname.to_lowercase() == target {
                return Some(entry.path().to_path_buf());
            }
        }
    }

    None
}

/// Find fuzzy file matches for a given path-like string.
///
/// Scans the repository under `working_dir` and uses Jaro-Winkler similarity
/// to rank candidate file paths by similarity to `name`. Returns at most
/// `max_results` paths whose score is >= `threshold` (0.0..=1.0).
pub fn find_fuzzy_file_matches(
    name: &str,
    working_dir: &std::path::Path,
    max_results: usize,
    threshold: f64,
) -> crate::error::Result<Vec<std::path::PathBuf>> {
    use std::cmp::Reverse;
    use strsim::jaro_winkler;
    use walkdir::WalkDir;

    let mut scores: Vec<(f64, std::path::PathBuf)> = Vec::new();
    let name_lc = name.to_lowercase();

    for entry in WalkDir::new(working_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path().to_path_buf();
        let candidate = path.to_string_lossy().to_lowercase();

        // Compute similarity against both the basename and full path
        let basename = entry
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let score = jaro_winkler(&name_lc, &basename).max(jaro_winkler(&name_lc, &candidate));
        if score >= threshold {
            scores.push((score, path));
        }
    }

    // sort by descending score and return top N
    scores.sort_by_key(|(score, _)| Reverse((*score * 1_000_000.0) as i64));
    let results = scores
        .into_iter()
        .take(max_results)
        .map(|(_, p)| p)
        .collect();

    Ok(results)
}

/// Kind of error encountered while loading a mention (file or URL)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadErrorKind {
    FileNotFound,
    NotAFile,
    FileTooLarge,
    FileBinary,
    PathOutsideWorkingDirectory,
    PermissionDenied,
    UrlSsrf,
    UrlRateLimited,
    UrlTimeout,
    UrlHttpError,
    UrlOther,
    ParseError,
    Unknown,
}

/// Structured error returned when loading mention content fails.
///
/// - `kind` is machine-friendly and useful for testing/handling
/// - `source` is the original path or URL that was being loaded
/// - `message` is a human-readable explanation
/// - `suggestion` is an optional remediation hint for the user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub kind: LoadErrorKind,
    pub source: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl LoadError {
    /// Create a new `LoadError`
    pub fn new(
        kind: LoadErrorKind,
        source: impl Into<String>,
        message: impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        Self {
            kind,
            source: source.into(),
            message: message.into(),
            suggestion,
        }
    }
}

impl std::fmt::Display for LoadErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LoadErrorKind::FileNotFound => "File not found",
            LoadErrorKind::NotAFile => "Not a file",
            LoadErrorKind::FileTooLarge => "File too large",
            LoadErrorKind::FileBinary => "Binary file",
            LoadErrorKind::PathOutsideWorkingDirectory => "Path outside working directory",
            LoadErrorKind::PermissionDenied => "Permission denied",
            LoadErrorKind::UrlSsrf => "URL blocked by SSRF protections",
            LoadErrorKind::UrlRateLimited => "Rate limited",
            LoadErrorKind::UrlTimeout => "Timed out",
            LoadErrorKind::UrlHttpError => "HTTP error",
            LoadErrorKind::UrlOther => "URL fetch error",
            LoadErrorKind::ParseError => "Parse error",
            LoadErrorKind::Unknown => "Unknown error",
        };
        write!(f, "{}", s)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(suggestion) = &self.suggestion {
            write!(
                f,
                "{}: {} ({}) — suggestion: {}",
                self.source, self.message, self.kind, suggestion
            )
        } else {
            write!(f, "{}: {} ({})", self.source, self.message, self.kind)
        }
    }
}

/// Heuristic classification for file loading errors
fn classify_file_error(e: &anyhow::Error) -> LoadErrorKind {
    let s = e.to_string().to_lowercase();
    if s.contains("file not found") || s.contains("no such file") {
        LoadErrorKind::FileNotFound
    } else if s.contains("not a file") {
        LoadErrorKind::NotAFile
    } else if s.contains("too large") || s.contains("exceeds limit") || s.contains("file too large")
    {
        LoadErrorKind::FileTooLarge
    } else if s.contains("binary") || s.contains("null byte") || s.contains("binary file") {
        LoadErrorKind::FileBinary
    } else if s.contains("directory traversal")
        || s.contains("path escapes")
        || s.contains("absolute paths")
    {
        LoadErrorKind::PathOutsideWorkingDirectory
    } else if s.contains("permission denied") {
        LoadErrorKind::PermissionDenied
    } else {
        LoadErrorKind::Unknown
    }
}

/// Heuristic classification for URL loading errors
fn classify_url_error(e: &anyhow::Error) -> LoadErrorKind {
    let s = e.to_string().to_lowercase();
    if s.contains("not allowed")
        && (s.contains("private") || s.contains("localhost") || s.contains("loopback"))
    {
        LoadErrorKind::UrlSsrf
    } else if s.contains("rate limit") || s.contains("rate limit exceeded") {
        LoadErrorKind::UrlRateLimited
    } else if s.contains("http") && s.contains("for") {
        LoadErrorKind::UrlHttpError
    } else if s.contains("timeout") || s.contains("timed out") {
        LoadErrorKind::UrlTimeout
    } else if s.contains("failed to fetch") || s.contains("failed to read") {
        LoadErrorKind::UrlOther
    } else {
        LoadErrorKind::Unknown
    }
}

/// Load content from a URL mention
///
/// Fetches web content from the specified URL, converts HTML to plain text,
/// and applies size limits. Content is cached with a 5-minute TTL.
///
/// # Arguments
///
/// * `url_mention` - The URL mention to load
/// * `max_size_bytes` - Maximum size to fetch in bytes
/// * `url_cache` - Optional cache for URL contents (URL -> content)
///
/// # Returns
///
/// Returns the fetched content as a formatted string
///
/// # Errors
///
/// Returns error if URL is invalid or fetch fails
pub async fn load_url_content(
    url_mention: &UrlMention,
    max_size_bytes: u64,
    url_cache: &std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<String, UrlContentCache>>,
    >,
) -> crate::error::Result<String> {
    // Check cache first
    {
        let cache = url_cache.read().await;
        if let Some(cached) = cache.get(&url_mention.url) {
            if !cached.is_expired() {
                debug!("Using cached URL content for {}", url_mention.url);
                return Ok(format!(
                    "Web content from {} (cached):\n\n{}",
                    url_mention.url, cached.content
                ));
            }
        }
    }

    // Create fetch tool
    let fetch_tool =
        crate::tools::FetchTool::new(std::time::Duration::from_secs(30), max_size_bytes as usize);

    // Fetch the URL
    match fetch_tool.fetch(&url_mention.url).await {
        Ok(fetched) => {
            let formatted = fetched.format_with_header(None);

            // Cache the result (store metadata to enable richer UX)
            {
                let mut cache = url_cache.write().await;
                cache.insert(
                    url_mention.url.clone(),
                    UrlContentCache {
                        url: url_mention.url.clone(),
                        content: formatted.clone(),
                        timestamp: std::time::SystemTime::now(),
                        content_type: Some(fetched.content_type.clone()),
                        size_bytes: Some(fetched.size_bytes),
                        status_code: Some(fetched.status_code),
                        truncated: fetched.truncated,
                    },
                );
            }

            Ok(formatted)
        }
        Err(e) => {
            debug!("Failed to fetch URL {}: {}", url_mention.url, e);
            Err(anyhow::anyhow!(
                "Failed to fetch URL {}: {}",
                url_mention.url,
                e
            ))
        }
    }
}

/// Augment user prompt with file contents from mentions
///
/// Loads file contents for all file mentions and search results for all search mentions,
/// prepending them to the user prompt in a structured format. Uses cache to avoid
/// repeated searches and file reads. When individual loads fail we record structured
/// `LoadError` values and insert a clear placeholder into the prompt so the user
/// and agent are both aware of missing content (graceful degradation).
///
/// # Arguments
///
/// * `mentions` - The parsed mentions from user input
/// * `original_prompt` - The original user input
/// * `working_dir` - The working directory for path resolution
/// * `max_size_bytes` - Maximum file size to load
/// * `cache` - Mention cache for storing/retrieving loaded contents
///
/// # Returns
///
/// A tuple of (augmented_prompt, load_errors)
/// - augmented_prompt: The original prompt with file contents and search results prepended
/// - load_errors: Structured, non-fatal errors that occurred during loading (see `LoadError`)
pub async fn augment_prompt_with_mentions(
    mentions: &[Mention],
    original_prompt: &str,
    working_dir: &Path,
    max_size_bytes: u64,
    cache: &mut MentionCache,
) -> (String, Vec<LoadError>, Vec<String>) {
    let mut file_contents: Vec<String> = Vec::new();
    let mut errors: Vec<LoadError> = Vec::new();
    let mut successes: Vec<String> = Vec::new();
    let url_cache = std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::<
        String,
        UrlContentCache,
    >::new()));

    // Process file mentions in order
    for mention in mentions {
        if let Mention::File(file_mention) = mention {
            // Resolve the file path
            let file_path = match resolve_mention_path(&file_mention.path, working_dir) {
                Ok(p) => p,
                Err(e) => {
                    // Try to provide helpful suggestions using common abbreviations and fuzzy matching
                    let mut suggestion: Option<String> = None;
                    if let Some(expanded) =
                        expand_common_abbreviations(&file_mention.path, working_dir)
                    {
                        suggestion = Some(format!("Did you mean: {}?", expanded.to_string_lossy()));
                    } else if let Ok(matches) =
                        find_fuzzy_file_matches(&file_mention.path, working_dir, 5, 0.65)
                    {
                        if !matches.is_empty() {
                            let snippet: Vec<String> = matches
                                .into_iter()
                                .take(3)
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();
                            suggestion = Some(format!("Did you mean: {}?", snippet.join(", ")));
                        }
                    }

                    let load_err = LoadError::new(
                        LoadErrorKind::PathOutsideWorkingDirectory,
                        file_mention.path.clone(),
                        e.to_string(),
                        suggestion.or(Some(
                            "Ensure the path is relative and inside the working directory"
                                .to_string(),
                        )),
                    );
                    errors.push(load_err.clone());
                    // Insert a placeholder into the prompt so the agent knows content was omitted
                    file_contents.push(format!(
                        "Failed to include file {}:\n\n```text\n{}\n```",
                        file_mention.path, load_err
                    ));
                    continue;
                }
            };

            // Try to get from cache first (note whether it's cached for success messaging)
            let (content, was_cached) = if let Some(cached) = cache.get(&file_path) {
                debug!("Using cached content for {}", file_path.display());
                (cached, true)
            } else {
                // Load from disk
                match load_file_content(file_mention, working_dir, max_size_bytes).await {
                    Ok(content) => {
                        cache.insert(file_path.clone(), content.clone());
                        (content, false)
                    }
                    Err(e) => {
                        let kind = classify_file_error(&e);
                        let suggestion = match kind {
                            LoadErrorKind::FileTooLarge => Some("Consider increasing 'max_file_read_size' or requesting a smaller range".to_string()),
                            LoadErrorKind::FileBinary => Some("Binary files cannot be displayed; open locally or use a different tool".to_string()),
                            _ => None,
                        };
                        let load_err = LoadError::new(
                            kind,
                            file_mention.path.clone(),
                            e.to_string(),
                            suggestion,
                        );
                        errors.push(load_err.clone());
                        file_contents.push(format!(
                            "Failed to include file {}:\n\n```text\n{}\n```",
                            file_mention.path, load_err.message
                        ));
                        continue;
                    }
                }
            };

            // Extract line range if specified
            let content_str = match (file_mention.start_line, file_mention.end_line) {
                (Some(start), Some(end)) => match content.extract_line_range(start, end) {
                    Ok(extracted) => content
                        .format_with_header(Some(start), Some(end))
                        .replace(&content.contents, &extracted),
                    Err(e) => {
                        let load_err = LoadError::new(
                            LoadErrorKind::ParseError,
                            file_mention.path.clone(),
                            e.to_string(),
                            Some("Check the requested line range".to_string()),
                        );
                        errors.push(load_err.clone());
                        file_contents.push(format!(
                            "Failed to include file {} lines {}-{}:\n\n```text\n{}\n```",
                            file_mention.path, start, end, load_err.message
                        ));
                        continue;
                    }
                },
                (Some(line), None) => match content.extract_line_range(line, line) {
                    Ok(extracted) => content
                        .format_with_header(Some(line), None)
                        .replace(&content.contents, &extracted),
                    Err(e) => {
                        let load_err = LoadError::new(
                            LoadErrorKind::ParseError,
                            file_mention.path.clone(),
                            e.to_string(),
                            Some("Check the requested line".to_string()),
                        );
                        errors.push(load_err.clone());
                        file_contents.push(format!(
                            "Failed to include file {} line {}:\n\n```text\n{}\n```",
                            file_mention.path, line, load_err.message
                        ));
                        continue;
                    }
                },
                _ => content.format_with_header(None, None),
            };

            file_contents.push(content_str);

            // Build a concise success message for UX (include cached flag)
            let loaded_lines = content.line_count;
            let loaded_bytes = content.size_bytes;
            let cached_note = if was_cached { " (cached)" } else { "" };
            successes.push(format!(
                "Loaded @{} ({} lines, {} bytes{})",
                file_mention.path, loaded_lines, loaded_bytes, cached_note
            ));
        }
    }

    // Process URL mentions
    for mention in mentions {
        if let Mention::Url(url_mention) = mention {
            // Check cache first for a quick success message
            let cached_opt = {
                let cache = url_cache.read().await;
                cache.get(&url_mention.url).cloned()
            };

            if let Some(cached) = cached_opt {
                if !cached.is_expired() {
                    debug!("Using cached URL content for {}", url_mention.url);
                    // Use the cached formatted content
                    file_contents.push(cached.content.clone());
                    let size = cached.size_bytes.unwrap_or(0);
                    let ctype = cached
                        .content_type
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string());
                    let truncated_note = if cached.truncated { " (truncated)" } else { "" };
                    successes.push(format!(
                        "Fetched @{} ({} bytes, {}{}) (cached)",
                        url_mention.url, size, ctype, truncated_note
                    ));
                    continue;
                }
            }

            // Not cached (or expired), attempt to fetch
            match load_url_content(url_mention, max_size_bytes, &url_cache).await {
                Ok(content) => {
                    file_contents.push(content);

                    // Try to read metadata from cache (the fetch function populates it)
                    let meta_opt = {
                        let cache = url_cache.read().await;
                        cache.get(&url_mention.url).cloned()
                    };

                    if let Some(meta) = meta_opt {
                        let size = meta.size_bytes.unwrap_or(0);
                        let ctype = meta.content_type.unwrap_or_else(|| "unknown".to_string());
                        let truncated_note = if meta.truncated { " (truncated)" } else { "" };
                        successes.push(format!(
                            "Fetched @{} ({} bytes, {}{})",
                            url_mention.url, size, ctype, truncated_note
                        ));
                    } else {
                        // Fallback message when metadata not available
                        successes.push(format!("Fetched @{}", url_mention.url));
                    }
                }
                Err(e) => {
                    let kind = classify_url_error(&e);
                    let suggestion = match kind {
                        LoadErrorKind::UrlSsrf => Some("URL blocked due to SSRF protections. Try a public URL or update fetch_allowed_domains in configuration.".to_string()),
                        LoadErrorKind::UrlRateLimited => Some("Rate limit exceeded. Try again later or increase the limit in configuration.".to_string()),
                        LoadErrorKind::UrlTimeout => Some("Request timed out. Consider increasing the fetch timeout in configuration.".to_string()),
                        _ => None,
                    };
                    let load_err =
                        LoadError::new(kind, url_mention.url.clone(), e.to_string(), suggestion);
                    errors.push(load_err.clone());
                    file_contents.push(format!(
                        "Failed to include URL {}:\n\n```text\n{}\n```",
                        url_mention.url, load_err.message
                    ));
                }
            }
        }
    }

    // Process search mentions (note: actual grep execution requires GrepTool)
    for mention in mentions {
        match mention {
            Mention::Search(search_mention) => {
                // Search mentions are logged but not executed without grep tool
                // The grep tool will be integrated in a later phase
                debug!(
                    "Search mention parsed (execution pending grep tool): {:?}",
                    search_mention.pattern
                );
            }
            Mention::Grep(grep_mention) => {
                // Grep mentions are logged but not executed without grep tool
                // The grep tool will be integrated in a later phase
                debug!(
                    "Grep mention parsed (execution pending grep tool): {:?}",
                    grep_mention.pattern
                );
            }
            _ => {
                // File and URL mentions are handled elsewhere
            }
        }
    }

    // Construct augmented prompt
    let augmented = if file_contents.is_empty() {
        original_prompt.to_string()
    } else {
        let separator = "\n---\n\n";
        format!(
            "{}{}{}",
            file_contents.join(separator),
            separator,
            original_prompt
        )
    };

    (augmented, errors, successes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mention_content_new() {
        let path = PathBuf::from("test.rs");
        let contents = "line1\nline2\nline3".to_string();
        let content = MentionContent::new(path.clone(), "test.rs".to_string(), contents, None);

        assert_eq!(content.path, path);
        assert_eq!(content.original_path, "test.rs");
        assert_eq!(content.line_count, 3);
        assert_eq!(content.size_bytes, 17);
    }

    #[test]
    fn test_mention_content_extract_line_range() {
        let path = PathBuf::from("test.rs");
        let contents = "line1\nline2\nline3\nline4".to_string();
        let content = MentionContent::new(path, "test.rs".to_string(), contents, None);

        let extracted = content.extract_line_range(1, 2).unwrap();
        assert_eq!(extracted, "line1\nline2");

        let extracted = content.extract_line_range(2, 3).unwrap();
        assert_eq!(extracted, "line2\nline3");

        let extracted = content.extract_line_range(1, 1).unwrap();
        assert_eq!(extracted, "line1");
    }

    #[test]
    fn test_mention_content_extract_line_range_invalid() {
        let path = PathBuf::from("test.rs");
        let contents = "line1\nline2\nline3".to_string();
        let content = MentionContent::new(path, "test.rs".to_string(), contents, None);

        // Invalid: start > end
        assert!(content.extract_line_range(2, 1).is_err());

        // Invalid: zero line numbers
        assert!(content.extract_line_range(0, 1).is_err());

        // Invalid: start exceeds line count
        assert!(content.extract_line_range(5, 10).is_err());
    }

    #[test]
    fn test_mention_content_extract_line_range_beyond_end() {
        let path = PathBuf::from("test.rs");
        let contents = "line1\nline2\nline3".to_string();
        let content = MentionContent::new(path, "test.rs".to_string(), contents, None);

        // Should clamp to end
        let extracted = content.extract_line_range(2, 10).unwrap();
        assert_eq!(extracted, "line2\nline3");
    }

    #[test]
    fn test_mention_content_format_with_header() {
        let path = PathBuf::from("src/main.rs");
        let contents = "fn main() {}".to_string();
        let content = MentionContent::new(path, "src/main.rs".to_string(), contents, None);

        let formatted = content.format_with_header(None, None);
        assert!(formatted.contains("File: src/main.rs"));
        assert!(formatted.contains("1 lines"));
        assert!(formatted.contains("fn main() {}"));
    }

    #[test]
    fn test_mention_content_format_with_line_range() {
        let path = PathBuf::from("src/main.rs");
        let contents = "line1\nline2\nline3\nline4".to_string();
        let content = MentionContent::new(path, "src/main.rs".to_string(), contents, None);

        let formatted = content.format_with_header(Some(1), Some(2));
        assert!(formatted.contains("File: src/main.rs"));
        assert!(formatted.contains("(Lines 1-2)"));
    }

    #[test]
    fn test_mention_cache_new() {
        let cache = MentionCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_mention_cache_insert_and_get() {
        let mut cache = MentionCache::new();
        let path = PathBuf::from("test.rs");
        let content = MentionContent::new(
            path.clone(),
            "test.rs".to_string(),
            "test".to_string(),
            None,
        );

        cache.insert(path.clone(), content.clone());
        assert_eq!(cache.len(), 1);

        // Note: get may return None if file doesn't actually exist
        // But we can check that it was stored
        assert!(cache.cache.contains_key(&path));
    }

    #[test]
    fn test_mention_cache_clear() {
        let mut cache = MentionCache::new();
        let path = PathBuf::from("test.rs");
        let content = MentionContent::new(path, "test.rs".to_string(), "test".to_string(), None);

        cache.insert(PathBuf::from("test.rs"), content);
        assert!(!cache.is_empty());

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_mention_cache_default() {
        let cache = MentionCache::default();
        assert!(cache.is_empty());
    }

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

    #[tokio::test]
    async fn test_load_file_content_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, "line1\nline2\nline3")
            .await
            .unwrap();

        let mention = FileMention {
            path: "test.txt".to_string(),
            start_line: None,
            end_line: None,
        };

        let content = load_file_content(&mention, temp_dir.path(), 1024).await;
        assert!(content.is_ok());

        let content = content.unwrap();
        assert_eq!(content.line_count, 3);
        assert_eq!(content.size_bytes, 17);
        assert!(content.contents.contains("line1"));
    }

    #[tokio::test]
    async fn test_load_file_content_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mention = FileMention {
            path: "nonexistent.txt".to_string(),
            start_line: None,
            end_line: None,
        };

        let result = load_file_content(&mention, temp_dir.path(), 1024).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_file_content_exceeds_size_limit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("large.txt");
        tokio::fs::write(&file_path, "x".repeat(1000))
            .await
            .unwrap();

        let mention = FileMention {
            path: "large.txt".to_string(),
            start_line: None,
            end_line: None,
        };

        let result = load_file_content(&mention, temp_dir.path(), 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_file_content_binary_detection() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        tokio::fs::write(&file_path, b"test\x00binary")
            .await
            .unwrap();

        let mention = FileMention {
            path: "binary.bin".to_string(),
            start_line: None,
            end_line: None,
        };

        let result = load_file_content(&mention, temp_dir.path(), 1024).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Binary file"));
    }

    #[tokio::test]
    async fn test_augment_prompt_with_single_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        tokio::fs::write(&file_path, "fn main() {}").await.unwrap();

        let mentions = vec![Mention::File(FileMention {
            path: "test.rs".to_string(),
            start_line: None,
            end_line: None,
        })];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Please review this code",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors.is_empty());
        assert!(!successes.is_empty());
        assert!(successes.iter().any(|s| s.contains("test.rs")));
        assert!(augmented.contains("fn main() {}"));
        assert!(augmented.contains("Please review this code"));
        assert!(augmented.contains("File: test.rs"));
    }

    #[tokio::test]
    async fn test_augment_prompt_with_line_range() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        tokio::fs::write(&file_path, "line1\nline2\nline3\nline4")
            .await
            .unwrap();

        let mentions = vec![Mention::File(FileMention {
            path: "test.rs".to_string(),
            start_line: Some(2),
            end_line: Some(3),
        })];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "What about these lines?",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors.is_empty());
        assert!(!successes.is_empty());
        assert!(augmented.contains("line2"));
        assert!(augmented.contains("line3"));
        assert!(!augmented.contains("line1"));
        assert!(!augmented.contains("line4"));
    }

    #[tokio::test]
    async fn test_augment_prompt_cache_hit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        tokio::fs::write(&file_path, "cached content")
            .await
            .unwrap();

        let mentions = vec![Mention::File(FileMention {
            path: "test.rs".to_string(),
            start_line: None,
            end_line: None,
        })];

        let mut cache = MentionCache::new();

        // First call - loads from disk
        let (augmented1, errors1, successes1) = augment_prompt_with_mentions(
            &mentions,
            "First prompt",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors1.is_empty());
        assert_eq!(cache.len(), 1);
        assert!(!successes1.is_empty());

        // Second call - uses cache
        let (augmented2, errors2, successes2) = augment_prompt_with_mentions(
            &mentions,
            "Second prompt",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors2.is_empty());
        assert_eq!(cache.len(), 1); // Still one entry
        assert!(!successes2.is_empty());
        assert!(
            augmented1.contains("cached content")
                || successes1.iter().any(|s| s.contains("cached"))
        );
        assert!(
            augmented2.contains("cached content")
                || successes2.iter().any(|s| s.contains("cached"))
        );
    }

    #[tokio::test]
    async fn test_augment_prompt_with_multiple_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("file1.rs");
        let file2 = temp_dir.path().join("file2.rs");
        tokio::fs::write(&file1, "content1").await.unwrap();
        tokio::fs::write(&file2, "content2").await.unwrap();

        let mentions = vec![
            Mention::File(FileMention {
                path: "file1.rs".to_string(),
                start_line: None,
                end_line: None,
            }),
            Mention::File(FileMention {
                path: "file2.rs".to_string(),
                start_line: None,
                end_line: None,
            }),
        ];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Review both files",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors.is_empty());
        assert!(augmented.contains("content1"));
        assert!(augmented.contains("content2"));
        assert!(!successes.is_empty());
        assert_eq!(cache.len(), 2);
    }

    #[tokio::test]
    async fn test_augment_prompt_with_missing_file() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mentions = vec![Mention::File(FileMention {
            path: "missing.rs".to_string(),
            start_line: None,
            end_line: None,
        })];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Please review",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(!errors.is_empty());
        assert!(successes.is_empty());
        // Ensure the error placeholder is included in the augmented prompt
        assert!(augmented.contains("Failed to include file"));
        assert!(augmented.contains("missing.rs"));
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn test_augment_prompt_with_url_ssrf_error() {
        let mentions = vec![Mention::Url(UrlMention {
            url: "http://127.0.0.1".to_string(),
        })];

        let temp_dir = tempfile::tempdir().unwrap();
        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Check this URL",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(!errors.is_empty());
        assert_eq!(errors[0].kind, LoadErrorKind::UrlSsrf);
        assert!(successes.is_empty());
        assert!(augmented.contains("Failed to include URL http://127.0.0.1"));
    }

    #[tokio::test]
    async fn test_augment_prompt_with_large_file_suggestion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("large.txt");
        tokio::fs::write(&file_path, "x".repeat(2000))
            .await
            .unwrap();

        let mentions = vec![Mention::File(FileMention {
            path: "large.txt".to_string(),
            start_line: None,
            end_line: None,
        })];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Please check this file",
            temp_dir.path(),
            1000, // max_size_bytes smaller than file size
            &mut cache,
        )
        .await;

        // We should receive a structured LoadError and the prompt should contain a clear placeholder
        assert!(!errors.is_empty());
        assert_eq!(errors[0].kind, LoadErrorKind::FileTooLarge);
        assert!(errors[0].suggestion.is_some());
        assert!(successes.is_empty());
        assert!(augmented.contains("Failed to include file large.txt"));
        assert!(errors[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("max_file_read_size"));
        assert!(augmented.contains("Failed to include file large.txt"));
    }

    #[tokio::test]
    async fn test_augment_prompt_no_mentions() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mentions = vec![];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Just a regular prompt",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        assert!(errors.is_empty());
        assert!(successes.is_empty());
        assert_eq!(augmented, "Just a regular prompt");
    }

    #[tokio::test]
    async fn test_augment_prompt_non_file_mentions_ignored() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mentions = vec![
            Mention::Search(SearchMention {
                pattern: "test".to_string(),
            }),
            // URL mentions are now processed, so test search mentions only
        ];

        let mut cache = MentionCache::new();
        let (augmented, errors, successes) = augment_prompt_with_mentions(
            &mentions,
            "Search for patterns",
            temp_dir.path(),
            1024,
            &mut cache,
        )
        .await;

        // No content should have been loaded for pure search mentions
        assert!(errors.is_empty());
        assert!(successes.is_empty());

        assert!(errors.is_empty());
        assert_eq!(augmented, "Search for patterns");
        assert!(cache.is_empty());
    }
}
