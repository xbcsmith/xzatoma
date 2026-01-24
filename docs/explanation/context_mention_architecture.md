# Context Mention Architecture

## Overview

Context mentions are a core feature of XZatoma that allow users to inject relevant file contents, search results, and web content directly into agent prompts. This document explains the architectural design, implementation strategy, and integration points.

## Design Goals

1. **Seamless Context Injection** - Users reference content using natural `@mention` syntax
2. **Multi-Source Support** - Handle files, search results, grep patterns, and URLs
3. **Performance** - Cache content to avoid redundant operations
4. **Error Resilience** - Gracefully handle failures (missing files, network timeouts, etc.)
5. **Security** - Prevent SSRF attacks, validate inputs, respect rate limits
6. **Clarity** - Provide helpful error messages and suggestions to users

## Architecture Diagram

```
User Input with @mentions
     |
     v
  Parse Mentions
  (extract @file, @search, @grep, @url)
     |
     +---> File Mention
     |    |
     |    +---> Path Resolution
     |    +---> Load File Content
     |    +---> Apply Line Range
     |
     +---> Search Mention
     |    |
     |    +---> File System Scan
     |    +---> Pattern Matching
     |    +---> Format Results
     |
     +---> Grep Mention
     |    |
     |    +---> Regex Compilation
     |    +---> Content Scan
     |    +---> Format Results
     |
     +---> URL Mention
     |    |
     |    +---> Validate URL (SSRF check)
     |    +---> HTTP Fetch (with timeout/size limits)
     |    +---> Convert HTML to Text
     |    +---> Cache Result
     |
     v
  Augment Prompt
  (inject content into conversation)
     |
     v
  Send to AI Provider
  (with augmented context)
     |
     v
  Agent Response
  (now has needed context)
```

## Module Structure

### Core Mention Parser (`src/mention_parser.rs`)

The heart of the mention system. Responsibilities:

1. **Mention Parsing** - Extract `@mention` patterns from user input
2. **Type Identification** - Determine mention type (file, search, grep, URL)
3. **Content Loading** - Load content for each mention type
4. **Error Handling** - Provide structured errors with suggestions
5. **Prompt Augmentation** - Inject loaded content into conversation

Key types:

```rust
pub enum Mention {
  File(FileMention),   // @file.rs#L10-20
  Search(SearchMention), // @search:"pattern"
  Grep(SearchMention),  // @grep:"regex"
  Url(UrlMention),    // @url:https://example.com
}

pub struct LoadError {
  pub kind: LoadErrorKind,   // Structured error type
  pub source: String,      // File/URL that failed
  pub message: String,      // Human-readable message
  pub suggestion: Option<String>, // How to fix it
}
```

### File Operations Tool (`src/tools/file_ops.rs`)

Handles filesystem operations:

- List directory contents
- Read file contents
- Write/modify files
- Delete files
- Compare file differences

Used by mention parser for:

- Resolving file paths
- Loading file contents
- Getting file metadata (size, modification time)

### Fetch Tool (`src/tools/fetch.rs`)

Handles HTTP requests:

- URL validation (SSRF prevention)
- HTTP GET requests with timeout
- Response size limiting
- HTML to text conversion
- Response caching

Key features:

- Blocks private/local IP addresses
- Validates redirects at each step
- Per-domain rate limiting
- 24-hour cache TTL
- 60-second timeout per request
- 1 MB content size limit

### Error Types (`src/error.rs`)

Structured error handling:

```rust
pub enum LoadErrorKind {
  FileNotFound,
  FileTooLarge,
  FileBinary,
  UrlInvalid,
  UrlSsrf,
  UrlRateLimited,
  UrlHttpError(u16),
  UrlTimeout,
  ParseError,
  Unknown,
}
```

## Content Injection Pipeline

### Step 1: Mention Parsing

When user input contains `@` symbols, the parser extracts mentions:

```
Input: "Review @config.yaml and search for @search:\"error\""
Output: [
  Mention::File(FileMention { path: "config.yaml", ... }),
  Mention::Search(SearchMention { pattern: "error" }),
]
```

Parser handles:

- File paths with line ranges: `@src/lib.rs#L10-20`
- Search patterns in quotes: `@search:"pattern"`
- Regex patterns in quotes: `@grep:"[Rr]esult"`
- Full URLs: `@url:https://example.com`

### Step 2: Content Loading

For each mention, appropriate content loader is called:

**File Mention Loading:**

1. Resolve path (support full path and abbreviations)
2. Check if file exists
3. Load file contents
4. Validate not binary
5. Apply line range filter (if specified)
6. Return `MentionContent` with metadata

**Search Mention Loading:**

1. Scan project directory with `walkdir`
2. For each text file, scan for pattern matches
3. Collect matching lines with file/line context
4. Format results as readable text
5. Return search results

**Grep Mention Loading:**

1. Compile regex pattern
2. Scan project directory
3. For each text file, apply regex
4. Collect matches with context
5. Format results as readable text
6. Return grep results

**URL Mention Loading:**

1. Validate URL (security checks)
2. Check cache (avoid redundant fetches)
3. If cached, return cached content
4. Otherwise: Fetch URL with timeout
5. Validate HTTP status code
6. Convert HTML to text (if needed)
7. Enforce size limit
8. Cache result with timestamp
9. Return content

### Step 3: Prompt Augmentation

When all mentions are loaded, content is injected into the augmented prompt:

```
Original: "Review @config.yaml"

Augmented: "Review this file:

File: config.yaml
---
[file contents here]
---

"
```

The agent sees the augmented prompt with full context.

## Component Interactions

### Parser → File Operations

File mention resolution uses file operations:

```
parse_mentions()
 -> load_file_content()
  -> file_ops::read_file()
  -> file_ops::get_file_metadata()
```

### Parser → Fetch Tool

URL mention loading uses fetch tool:

```
parse_mentions()
 -> load_url_content()
  -> fetch_tool::fetch()
  -> convert_html_to_text()
```

### Agent → Augmentation

Agent calls augmentation before sending to provider:

```
agent::chat()
 -> augment_prompt_with_mentions()
  -> parse_mentions()
  -> load_content()
  -> provider::send_message(augmented_prompt)
```

## Caching Strategy

### File Content Cache

File contents are cached during a session:

- **Key**: Resolved file path
- **Value**: `MentionContent` (contents + metadata)
- **TTL**: Session duration
- **Invalidation**: Manual (user can trigger refresh)

Benefits:

- Multiple mentions of same file use cached content
- Avoids redundant file reads

### URL Content Cache

Fetched URL content is cached:

- **Key**: URL string
- **Value**: Content with metadata (size, status, type, truncated flag)
- **TTL**: 24 hours (approximately)
- **Invalidation**: Manual or time-based

Benefits:

- Avoids redundant network requests
- Respects server bandwidth
- Improves performance

### Search/Grep Results Cache

Search and grep results are NOT cached:

- Each search/grep is executed fresh
- Ensures up-to-date results as code changes
- Regex compilation is fast enough

## Error Handling Strategy

### Graceful Degradation

When a mention fails to load:

1. Error is classified into `LoadErrorKind`
2. Clear placeholder is inserted into augmented prompt
3. Error is collected and returned
4. Augmentation continues with remaining mentions
5. Agent execution is NOT halted

Example placeholder for missing file:

```
Failed to include file foo.rs:

<file not found: foo.rs>

Try one of these suggestions:
- Use full path: src/foo.rs
- Check spelling: did you mean src/foo_bar.rs?
```

### User Feedback

CLI displays structured error information:

```
Mention Loading Summary
=======================
Loaded: config.yaml, src/main.rs
Failed: @nonexistent.rs (File not found)
     @url:https://localhost:8080 (SSRF blocked)

Suggestions:
- For nonexistent.rs: Check if file exists with 'ls'
- For localhost: Use public URL instead
```

## Security Architecture

### SSRF Prevention (Server-Side Request Forgery)

URL validation blocks:

- **Private IPs**: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
- **Loopback**: 127.0.0.1, ::1, localhost
- **Link-local**: 169.254.0.0/16
- **Multicast**: 224.0.0.0/4

### Rate Limiting

Per-domain rate limits prevent abuse:

- **Max 10 URLs per mention request**
- **60-second timeout per request**
- **Exponential backoff on rate limit (429) responses**

### Input Validation

File paths validated to prevent traversal:

```rust
// Block attempts to escape project directory
if path.contains("..") || path.starts_with("/") {
  return Err(LoadError::new(FileNotFound, ...));
}
```

### Content Type Safety

Only text content is processed:

- **Allowed**: HTML, JSON, XML, plain text, Markdown
- **Rejected**: Binary files, images, executables

## Performance Considerations

### File Scanning

Searches and greps scan the entire project:

- Uses `walkdir` for efficient directory traversal
- Filters binary files early
- Regex compilation cached if possible

**Performance Impact**: Proportional to project size

**Optimization**: Users should provide specific patterns to narrow results

### URL Fetching

Network requests are the slowest operation:

- **Typical**: 100ms - 5000ms depending on URL
- **Timeout**: 60 seconds
- **Size limit**: 1 MB

**Optimization**: Results are cached; subsequent mentions of same URL are instant

### Mention Parsing

Parsing and pattern extraction is fast (O(n) where n = input length):

- Regex matching for `@` patterns
- Type identification (file/search/grep/URL)
- Typical: < 1ms for reasonable input

### Memory Usage

Augmented prompts can get large with many mentions:

- Each file mention adds file size to prompt
- Suggest users limit to essential files
- Large files are warned about

## Future Enhancement Ideas

### 1. Interactive Suggestion Selection

When fuzzy matching finds multiple candidates:

```
Which file did you mean?
1. src/config.rs
2. config/prod.yaml
3. config/dev.yaml

Enter number or 'none' to skip:
```

### 2. Mention Abbreviation System

Allow users to define shortcuts:

```
@docs = src/docs/
@handlers = src/handlers/*.rs

Then use: @docs/api.md
```

### 3. Smarter File Caching

Invalidate cache when file is detected to have changed:

```rust
if file.mtime > cache.cached_at {
  reload_file_content()
}
```

### 4. Search Result Ranking

Rank search results by relevance:

- Exact word matches higher than substring
- More recent modifications ranked higher
- File type affinity (code vs docs)

### 5. HTML to Markdown Conversion

Better HTML → Markdown conversion using libraries like `html2md`:

```
Better formatting and structure preservation
vs current simple HTML stripping
```

### 6. Content Summarization

For large files/pages, optional auto-summarization:

```
@url:https://example.com#summary
Provides key points instead of full content
```

### 7. Mention Validation in Plan Files

Pre-check mentions in plan files before execution:

```
Parse plan.yaml
Check all @mentions exist
Report missing references before starting
```

### 8. Bidirectional Linking

When agent mentions a file it read, track that:

```
Agent: "I reviewed src/main.rs..."
[Implied: @src/main.rs was mentioned]
```

### 9. Mention Coverage Reports

Show user what was included in augmented prompt:

```
Augmented Prompt Coverage:
- 3 files (total 450 lines)
- 2 search results (45 matches)
- 1 URL (8 KB content)
- 0 grep results

Total context: ~25 KB sent to provider
```

### 10. Conditional Mentions

Allow conditional inclusion based on file existence:

```
@if:src/config.dev.yaml
@then:include this for development setup
@else:use default configuration
```

## Integration Points

### With Agent Core

Agent calls `augment_prompt_with_mentions()` before sending message to provider:

```rust
let (augmented_prompt, errors) = augment_prompt_with_mentions(
  user_message,
  &project_root,
).await;

// Errors are collected and reported
// Augmented prompt is sent to provider
provider.send_message(&augmented_prompt).await?
```

### With CLI Chat Loop

Chat loop captures user input, passes to augmentation:

```rust
loop {
  let input = read_user_input();

  if input.contains("@") {
    let (prompt, errors) = augment_prompt_with_mentions(&input, ...);
    display_mention_errors(&errors);
    send_to_agent(&prompt);
  } else {
    send_to_agent(&input);
  }
}
```

### With Special Commands

Mention help is available via special commands:

```
/help    - General help including mentions syntax
/mentions  - Detailed mention help and examples
```

## Testing Strategy

### Unit Tests

- Parse mention patterns correctly
- Load file contents accurately
- Handle line ranges correctly
- Validate URLs for SSRF
- Format search/grep results

### Integration Tests

- End-to-end mention processing
- Error handling and graceful degradation
- Cache invalidation and hits
- URL fetching with mocking
- Large file handling

### Property Tests

- Mention parser handles arbitrary input safely
- Path resolution doesn't escape project
- Regex patterns don't cause DoS (ReDoS)

## Performance Metrics

Typical performance on a medium project (~10K files, ~1M lines):

- **Mention parsing**: < 1ms
- **Single file load**: < 10ms (cached: < 0.1ms)
- **Single file grep**: 50-500ms (depends on pattern)
- **Single file search**: 100-1000ms
- **URL fetch**: 500-5000ms (cached: < 0.1ms)

## References

- **File Mention Feature Plan**: `docs/explanation/file_mention_feature_implementation_plan.md`
- **Phase 4 Fetch Tool**: `docs/explanation/phase4_fetch_tool_implementation.md`
- **Phase 3 Grep Tool**: `docs/explanation/phase3_grep_tool_implementation.md`
- **Phase 5 Error Handling**: `docs/explanation/phase5_error_handling_and_user_feedback.md`
- **Chat Modes**: `docs/explanation/chat_modes_architecture.md`
