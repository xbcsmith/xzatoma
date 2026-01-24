# File Mention Feature Implementation Plan

## Overview

Add comprehensive context injection capabilities to interactive chat mode through @ mention syntax. This includes:

- **File mentions**: `@filename` or `@path/to/file.rs#L10-20` to include file contents
- **Code search mentions**: `@search:"pattern"` or `@grep:"regex"` to find and include matching code
- **Web content mentions**: `@url:https://example.com` to fetch and include external content

The agent will automatically resolve mentions, fetch content, and include it in the conversation context, enabling efficient multi-source discussions without manual tool invocations.

## Current State Analysis

### Existing Infrastructure

The interactive chat mode (`src/commands/mod.rs`) currently provides:

- Rustyline-based readline loop for user input
- Special command parser (`src/commands/special_commands.rs`) for mode switching and control commands
- FileOpsTool (`src/tools/file_ops.rs`) with read capabilities and path validation
- Agent execution pipeline that processes user input and tool calls
- Conversation history management in the agent
- Two chat modes: Planning (read-only) and Write (read-write)

XZatoma currently lacks:

- Advanced code search capabilities (regex-based grep)
- Web content retrieval (URL fetching)
- Unified context injection from multiple sources

### Identified Issues

The current workflow requires users to:

- Explicitly ask the agent to read files
- Wait for the agent to decide to use the read tool
- Potentially make multiple round trips for multi-file discussions
- Manually specify file paths in natural language
- Cannot search codebase for patterns or symbols
- Cannot include external documentation or web content
- No unified way to inject context from multiple sources

This creates friction when users want to discuss specific files, find code patterns, or reference external resources in a single prompt.

## Implementation Phases

### Phase 1: Core Mention Parser

#### Task 1.1: Create Mention Parser Module

Create `src/mention_parser.rs` with core parsing logic:

- Define `Mention` enum with variants: File, Search, Grep, Url
- Define `FileMention` struct containing file path and optional line range
- Define `SearchMention` struct for search queries
- Define `UrlMention` struct for web URLs
- Implement `parse_mentions()` function to extract all mention types from input strings
- Support syntax patterns:
 - Files: `@filename`, `@path/to/file.rs`, `@file.rs#L10-20`
 - Search: `@search:"pattern"`, `@grep:"regex pattern"`
 - URLs: `@url:https://example.com`
- Handle edge cases: escaped @, @ in middle of words, invalid characters
- Return both parsed mentions and cleaned input text

#### Task 1.2: Add Path Resolution Logic

Extend mention parser with path resolution:

- Implement `resolve_mention_path()` to convert mention strings to valid paths
- Support relative paths from working directory
- Integrate with FileOpsTool path validation logic
- Handle common patterns: partial matches, case-insensitive matching
- Return resolved absolute paths or clear error messages

#### Task 1.3: Integrate Parser into Chat Loop

Modify `src/commands/mod.rs` chat loop:

- Parse mentions from user input before processing special commands
- Extract FileMention list from each input line
- Preserve original input for history but use cleaned version for display
- Add tracing/logging for mention detection

#### Task 1.4: Testing Requirements

Unit tests for mention parser:

- Test single file mention: `@config.yaml`
- Test multiple mentions: `@src/main.rs and @README.md`
- Test line ranges: `@file.rs#L10-20`
- Test escaped @ symbols: `email\@example.com`
- Test invalid paths and error cases
- Test path traversal prevention: `@../../../etc/passwd`
- Achieve 80%+ coverage on parser module

#### Task 1.5: Deliverables

- `src/mention_parser.rs` (200-300 lines)
- Unit tests in same file (150-200 lines)
- Integration into chat loop (50-100 lines modified)
- Documentation comments for public API

#### Task 1.6: Success Criteria

- Parser correctly extracts @ mentions from various input patterns
- Path validation prevents directory traversal and absolute paths
- Tests pass with 80%+ coverage
- `cargo fmt`, `cargo clippy`, `cargo check` all pass

### Phase 2: File Content Injection

#### Task 2.1: Create Content Loader

Implement file content loading in `src/mention_parser.rs`:

- Add `MentionContent` struct with path, contents, metadata (size, lines, mtime)
- Implement async `load_mention_content()` using FileOpsTool read logic
- Handle line range extraction for `#L` syntax with inclusive bounds on both ends
- Apply size limits from ToolsConfig, return clear error for oversized files
- Format content with clear delimiters and file information
- Handle binary files gracefully (detect and skip or error)
- Add `MentionCache` struct to store loaded file contents keyed by path
- Check file modification time (mtime) to invalidate stale cache entries

#### Task 2.2: Build Context Augmentation

Create context injection logic:

- Implement `augment_prompt_with_mentions()` function
- Format file contents with clear headers: `File: path/to/file.rs (Lines 1-50)`
- Prepend file contents to user prompt in structured format
- Preserve conversational flow with separator formatting
- Handle multiple files in consistent order
- Check cache before loading files, use cached contents when valid
- Store newly loaded contents in cache for future use

#### Task 2.3: Integrate into Agent Execution

Modify chat loop in `src/commands/mod.rs`:

- Initialize `MentionCache` at session start, persist across conversation
- After parsing mentions, load file contents asynchronously
- Check cache first, only read from disk if needed or if file was modified
- Augment user prompt before passing to `agent.execute()`
- Display loading indicator for large files, show "cached" for reused content
- Show user which files were included and their source (disk vs cache)
- Preserve original @ mentions in conversation history via rustyline
- Add file mention summary to conversation context

#### Task 2.4: Testing Requirements

Integration tests for content injection:

- Test single file content injection
- Test multiple files in one prompt
- Test line range selection with inclusive bounds (L10-20 includes both 10 and 20)
- Test large file error message display
- Test missing file error handling
- Test binary file detection
- Test permission denied scenarios
- Test cache hit and cache miss scenarios
- Test cache invalidation on file modification
- Test repeated mentions use cached content
- Achieve 80%+ coverage

#### Task 2.5: Deliverables

- Content loading functions in `src/mention_parser.rs` (150-200 lines)
- Cache implementation with mtime-based invalidation (100-150 lines)
- Context augmentation logic (100-150 lines)
- Modified chat loop integration with cache initialization (100-125 lines)
- Integration tests including cache behavior (250-300 lines)
- Error handling for all failure modes

#### Task 2.6: Success Criteria

- File contents correctly injected into prompts
- Agent receives proper context from mentioned files
- Line ranges use inclusive bounds on both ends
- Size limits respected per ToolsConfig, errors printed to chat
- Cache improves performance for repeated mentions
- Cache invalidation works correctly when files are modified
- Original @ mentions preserved in conversation history
- All error cases handled gracefully
- Tests pass with 80%+ coverage
- Zero clippy warnings

### Phase 3: Advanced Code Search Integration (Grep Tool)

#### Task 3.1: Implement Grep Tool

Create `src/tools/grep.rs` with regex-based code search:

- Define `GrepTool` struct with working directory and config
- Implement async `search()` method with parameters:
 - `regex` pattern (string)
 - `include_pattern` optional glob filter
 - `case_sensitive` boolean flag
 - `offset` for pagination (u32)
- Use `regex` crate for pattern matching
- Use `walkdir` crate for file traversal (already in dependencies)
- Return paginated results (20 matches per page)
- Include context lines around matches (configurable, default 2)
- Respect `.gitignore` and common exclusions

#### Task 3.2: Register Grep Tool in Tool Registry

Modify `src/tools/mod.rs` and tool registry:

- Add `grep` module export
- Register `GrepTool` in `ToolRegistryBuilder`
- Available in both Planning and Write modes
- Add grep-specific configuration to `ToolsConfig`:
 - `max_results_per_page` (default: 20)
 - `context_lines` (default: 2)
 - `max_file_size_for_grep` (default: 1 MB)
 - `excluded_patterns` (default: `["*.lock", "target/**", "node_modules/**"]`)

#### Task 3.3: Integrate Search Mentions into Parser

Extend `src/mention_parser.rs` with search support:

- Parse `@search:"pattern"` and `@grep:"regex"` syntax
- Differentiate between simple search (literal) and grep (regex)
- Validate regex patterns, return clear errors for invalid syntax
- Store search queries in `SearchMention` struct with metadata
- Handle escaped quotes in search patterns

#### Task 3.4: Implement Search Content Loader

Add search loading to mention parser:

- Implement async `load_search_content()` using GrepTool
- Execute grep search for mentioned patterns
- Format results with file paths, line numbers, and context
- Limit total content size (apply same limits as file mentions)
- Handle case: no results found (clear message, don't error)
- Handle case: too many results (show count, include first N)
- Cache search results by query string + working directory

#### Task 3.5: Augment Prompts with Search Results

Extend context augmentation for search mentions:

- Format search results with clear structure:

 ```
 Search results for "pattern" (15 matches in 5 files):

 File: src/main.rs (Line 42)
 40: fn main() {
 41:   let config = load_config();
 42:   let pattern = "example"; // Match here
 43:   process_pattern(&pattern);
 44: }
 ```

- Show summary: total matches, number of files
- Include snippet context around each match
- Highlight match location in context
- Order results by relevance (file then line number)

#### Task 3.6: Testing Requirements

Grep tool and integration tests:

- Test regex pattern matching with valid patterns
- Test literal search vs regex search
- Test file filtering with glob patterns
- Test pagination with offset parameter
- Test case-sensitive and case-insensitive modes
- Test excluded patterns respected
- Test large codebase performance
- Test search mention parsing: `@search:"TODO"`, `@grep:"fn\\s+\\w+"`
- Test search result formatting and display
- Test search cache hit/miss scenarios
- Test "no results found" handling
- Achieve 80%+ coverage

#### Task 3.7: Deliverables

- `src/tools/grep.rs` (300-400 lines)
- Grep tool registration and config (50-75 lines)
- Search mention parsing (100-150 lines)
- Search content loader with caching (150-200 lines)
- Search result formatting (75-100 lines)
- Comprehensive tests (300-400 lines)

#### Task 3.8: Success Criteria

- Grep tool correctly searches codebase with regex patterns
- Pagination works for large result sets
- File filtering excludes irrelevant files
- Search mentions parsed and resolved correctly
- Results formatted clearly with context
- Performance acceptable for large codebases (under 2 seconds for typical search)
- Cache improves repeated search performance
- Tests pass with 80%+ coverage
- All quality checks pass

### Phase 4: Web Content Retrieval Integration (Fetch Tool)

#### Task 4.1: Implement Fetch Tool

Create `src/tools/fetch.rs` with web content retrieval:

- Define `FetchTool` struct with HTTP client configuration
- Implement async `fetch()` method with URL parameter
- Use `reqwest` crate for HTTP requests (already in dependencies)
- Convert HTML to Markdown using `html2md` crate (new dependency)
- Support content types: HTML, Markdown, plain text, JSON
- Implement timeout (default: 30 seconds)
- Implement size limits (default: 5 MB)
- Return content as Markdown string

#### Task 4.2: Add Security Validation

Implement SSRF prevention and security checks:

- Validate URL scheme (allow: https, http; deny: file, ftp, etc.)
- Block private IP ranges (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- Block localhost and link-local addresses
- Implement URL allowlist/denylist in config
- Validate content-type headers
- Sanitize content before returning
- Add rate limiting (max requests per minute)

#### Task 4.3: Register Fetch Tool

Modify tool registry for fetch tool:

- Add `fetch` module export in `src/tools/mod.rs`
- Register `FetchTool` in both Planning and Write modes
- Add fetch-specific configuration to `ToolsConfig`:
 - `fetch_timeout_seconds` (default: 30)
 - `max_fetch_size_bytes` (default: 5 MB)
 - `allowed_domains` (optional allowlist)
 - `blocked_domains` (optional denylist)
 - `max_fetches_per_minute` (default: 10)

#### Task 4.4: Integrate URL Mentions into Parser

Extend `src/mention_parser.rs` with URL support:

- Parse `@url:https://example.com` syntax
- Validate URL format during parsing
- Store URLs in `UrlMention` struct with metadata
- Handle various URL formats (with/without protocol, query params, fragments)
- Support URL shorthand: `@url:docs.rs/tokio` expands to `https://docs.rs/tokio`

#### Task 4.5: Implement URL Content Loader

Add URL loading to mention parser:

- Implement async `load_url_content()` using FetchTool
- Fetch and convert content to Markdown
- Apply size limits, timeout on slow requests
- Format content with clear URL source header
- Handle HTTP errors gracefully (404, 500, timeout, etc.)
- Cache fetched content by URL for session duration
- Implement cache TTL (default: 5 minutes for web content)

#### Task 4.6: Augment Prompts with Web Content

Extend context augmentation for URL mentions:

- Format fetched content with clear structure:

 ```
 Web content from https://docs.rs/tokio (fetched 2024-01-15 10:30:00):

 # Tokio Documentation

 [Converted Markdown content here...]

 [Content truncated at 5 MB limit]
 ```

- Show source URL and fetch timestamp
- Include content-type and size metadata
- Indicate if content was truncated
- Mark cached vs freshly fetched

#### Task 4.7: Testing Requirements

Fetch tool and integration tests:

- Test successful URL fetching with various content types
- Test HTML to Markdown conversion
- Test timeout handling for slow servers
- Test size limit enforcement
- Test SSRF prevention (private IPs, localhost, file:// protocol)
- Test URL allowlist/denylist functionality
- Test rate limiting
- Test URL mention parsing with various formats
- Test URL content caching and TTL
- Test HTTP error handling (404, 500, timeout)
- Mock HTTP requests for deterministic testing
- Achieve 80%+ coverage

#### Task 4.8: Deliverables

- `src/tools/fetch.rs` with security validation (250-350 lines)
- Fetch tool registration and config (50-75 lines)
- URL mention parsing (75-100 lines)
- URL content loader with caching and TTL (150-200 lines)
- Web content formatting (75-100 lines)
- Security tests and integration tests (300-400 lines)

#### Task 4.9: Success Criteria

- Fetch tool retrieves web content securely
- SSRF prevention blocks dangerous requests
- HTML correctly converted to readable Markdown
- Size and timeout limits enforced
- URL mentions parsed and resolved correctly
- Rate limiting prevents abuse
- Cache with TTL reduces redundant requests
- All security tests pass
- Tests pass with 80%+ coverage
- All quality checks pass

### Phase 5: Error Handling and User Feedback

#### Task 5.1: Comprehensive Error Types

Extend error handling in `src/error.rs`:

- Add `MentionError` variants:
 - File errors: FileNotFound, PermissionDenied, FileTooLarge, InvalidPath, BinaryFile
 - Search errors: InvalidRegex, SearchTimeout, TooManyResults
 - URL errors: InvalidUrl, FetchFailed, SsrfBlocked, ContentTooLarge, RateLimited
- Implement user-friendly error messages with context
- Add suggestions for common mistakes (typos, wrong paths, regex syntax)
- Include specific details in errors (file, URL, pattern, actual vs expected)
- Format errors appropriately:
 - "Error: @large_file.txt is too large (5.2 MB). Maximum file size is 1 MB."
 - "Error: @grep:\"[invalid\" - Invalid regex: unclosed character class"
 - "Error: @url:http://192.168.1.1 - Blocked: private IP address (SSRF prevention)"

#### Task 5.2: Graceful Degradation

Implement partial success handling for all mention types:

- When some mentions fail, load successful ones and print errors to chat
- Allow user to continue with partial context, prompt still executes
- Display clear summary: "Loaded 2 files, 1 search (15 results), 0 URLs. Failed: @missing.txt, @url:timeout.com"
- Print detailed error messages for each failed mention
- Log failures for debugging without blocking execution
- Categorize errors by type (file, search, URL) in summary

#### Task 5.3: User Feedback Display

Add visual feedback in chat loop for all mention types:

- Show colored status for mention processing:
 - Files: "Loading @file.rs..." (cyan)
 - Search: "Searching @grep:\"pattern\"..." (cyan)
 - URLs: "Fetching @url:https://example.com..." (cyan)
- Indicate cached content: "Using cached @config.yaml" (green)
- Display metadata:
 - Files: size, line count, cache status
 - Search: match count, file count
 - URLs: content size, fetch time, cache status
- Use existing colored tag infrastructure from chat modes
- Show success examples:
 - "Loaded @config.yaml (42 lines, 1.2 KB)" (green)
 - "Search @grep:\"TODO\" found 23 matches in 8 files" (green)
 - "Fetched @url:https://docs.rs/tokio (45 KB, cached)" (green)
- Show errors with appropriate context (red):
 - "Error: @missing.txt - File not found. Did you mean @main.txt?"
 - "Error: @grep:\"[invalid\" - Invalid regex: unclosed character class"
 - "Error: @url:http://localhost - Blocked: SSRF prevention"

#### Task 5.4: Testing Requirements

Error handling tests for all mention types:

- File errors: not found, permission denied, too large, binary files, cache errors
- Search errors: invalid regex, search timeout, too many results
- URL errors: invalid URL, fetch failed, SSRF blocked, content too large, rate limited
- Test helpful suggestions for file not found
- Test partial success with mixed mention types (some succeed, some fail)
- Test graceful degradation flow with multiple error types
- Test error message clarity and actionable information
- Test colored output for different error types
- Achieve 80%+ coverage on all error paths

#### Task 5.5: Deliverables

- Error types in `src/error.rs` with detailed messages (125-175 lines)
- Error handling logic in mention parser with graceful degradation (175-225 lines)
- User feedback display functions with colored output (150-200 lines)
- Error handling tests covering all scenarios (350-450 lines)

#### Task 5.6: Success Criteria

- All error cases have clear, actionable messages with specific details
- Error messages include context (file size limits, regex syntax help, SSRF explanation)
- Partial failures print errors to chat but do not block prompt execution
- User feedback is informative and colored appropriately (green=success/cached, red=error, cyan=loading)
- Cache status clearly communicated for all mention types
- Search and URL errors provide helpful guidance
- Tests cover all error paths with 80%+ coverage
- All quality checks pass

### Phase 6: User Experience Enhancements

#### Task 6.1: Fuzzy Path Matching

Implement smart path resolution:

- When exact match fails, search for similar filenames
- Use `similar` crate (already in dependencies) for fuzzy matching
- Suggest alternatives: "Did you mean @src/main.rs?"
- Limit to top 3 suggestions
- Allow configurable similarity threshold

#### Task 6.2: Common Abbreviations and Smart Patterns

Support convenient shortcuts and smart patterns:

- File abbreviations:
 - `@README` resolves to `README.md`
 - `@Cargo` resolves to `Cargo.toml`
 - `@main` searches for `main.rs`, `main.go`, etc.
- Search shortcuts:
 - `@todo` expands to `@search:"TODO"`
 - `@fixme` expands to `@search:"FIXME"`
 - `@impl:StructName` expands to `@grep:"impl.*StructName"`
- URL shortcuts:
 - `@docs:tokio` expands to `@url:https://docs.rs/tokio`
 - `@gh:user/repo` expands to `@url:https://github.com/user/repo`
- Configurable abbreviation map in ToolsConfig
- Clear indication when abbreviation is expanded

#### Task 6.3: Visual Feedback Improvements

Enhance display formatting for all mention types:

- Syntax highlighting for file type in mention summary
- Color-coded status (green=loaded, red=failed, yellow=warning, cyan=loading)
- Progress indicator for multiple mentions:
 - "Processing mentions: 2/5 complete (files: 2, searches: 1, URLs: 2)"
- Smart warnings:
 - File size warnings before loading large files
 - Search result count warnings (e.g., "500+ results, showing first 20")
 - URL fetch time warnings for slow responses
- Consistent formatting with existing colored tags
- Summary table for complex mention groups

#### Task 6.4: Context-Aware Suggestions

Implement intelligent suggestion system:

- When file not found, use grep to search for similar content
- Suggest files based on recently mentioned files in conversation
- When search returns no results, suggest related patterns
- When URL fetch fails, suggest cached alternatives
- Use conversation history to improve suggestions
- Machine learning optional: rank suggestions by relevance

#### Task 6.5: Testing Requirements

UX feature tests:

- Test fuzzy matching with typos for files
- Test abbreviation expansion for all mention types
- Test suggestion display and ranking
- Test colored output formatting for mixed mentions
- Test progress indicators for multi-mention prompts
- Test context-aware suggestions using grep
- Test smart warnings for large results
- Achieve 80%+ coverage on new features

#### Task 6.6: Deliverables

- Fuzzy matching logic (100-150 lines)
- Abbreviation resolver for all mention types (125-175 lines)
- Enhanced display functions with progress tracking (150-200 lines)
- Context-aware suggestion system (100-150 lines)
- Configuration additions (50-75 lines)
- UX tests (250-350 lines)

#### Task 6.7: Success Criteria

- Fuzzy matching correctly suggests alternatives for all mention types
- Abbreviations work for common files, searches, and URLs
- Visual feedback is clear and helpful for complex mention groups
- Context-aware suggestions improve user experience
- Progress indicators work smoothly for multiple mentions
- No performance degradation with large result sets
- Tests pass with 80%+ coverage
- All quality checks pass

### Phase 7: Documentation and Polish

#### Task 7.1: User Documentation

Create user-facing documentation in `docs/how-to/use_context_mentions.md`:

- Quick start examples for all mention types
- Syntax reference:
 - File mentions: `@file.rs`, `@path/to/file#L10-20`
 - Search mentions: `@search:"pattern"`, `@grep:"regex"`
 - URL mentions: `@url:https://example.com`
 - Abbreviations and shortcuts
- Line range syntax examples
- Search pattern examples (literal vs regex)
- Common patterns and best practices
- Security considerations for URL mentions
- Performance tips for large searches
- Troubleshooting guide
- FAQ section

#### Task 7.2: Architecture Documentation

Create technical documentation in `docs/explanation/context_mention_architecture.md`:

- Design decisions and rationale
- Multi-source context injection architecture
- Component interaction diagram
- Parser implementation details (all mention types)
- Tool integration (FileOps, Grep, Fetch)
- Caching strategy and TTL management
- Security architecture (SSRF prevention, validation)
- Error handling strategy
- Performance considerations
- Future enhancement ideas

#### Task 7.3: Implementation Summary

Create implementation summary in `docs/explanation/context_mention_implementation.md`:

- Overview of delivered features (files, search, URLs)
- Components created and modified
- Implementation details with code examples for each mention type
- Tool implementations (Grep, Fetch)
- Security measures and validation
- Testing results and coverage metrics
- Validation checklist results
- Usage examples for all mention types

#### Task 7.4: Help Text Integration

Update interactive help:

- Add all mention syntax to `/help` output in `src/commands/special_commands.rs`
- Include examples for files, search, and URL mentions
- Document abbreviations and shortcuts
- Update welcome banner with mention capabilities
- Add mention syntax hints to error messages
- Create `/mentions` special command to show mention help

#### Task 7.5: Testing Requirements

Documentation validation:

- Verify all code examples are runnable
- Test all documented commands
- Ensure links are valid
- Run markdown linter
- Verify examples match actual behavior

#### Task 7.6: Deliverables

- `docs/how-to/use_context_mentions.md` (500-700 lines)
- `docs/explanation/context_mention_architecture.md` (400-600 lines)
- `docs/explanation/context_mention_implementation.md` (700-900 lines)
- Updated help text with `/mentions` command (100-150 lines modified)
- README.md updates with all features (75-100 lines)
- Security documentation for URL fetching (100-150 lines)

#### Task 7.7: Success Criteria

- All documentation follows Diataxis framework
- Code examples are tested and accurate
- Help text is clear and complete
- No emojis in documentation
- All filenames are lowercase with underscores
- Markdown passes linting
- All quality checks pass

## Design Decisions

1. **Mode Support**: @ mentions work in both Planning and Write modes. Users can reference files for context regardless of which mode they are in.

2. **Large File Handling**: When a file exceeds the configured size limit, print a clear error message to the chat without blocking the prompt. For example: "Error: @large_file.txt is too large (5.2 MB). Maximum file size is 1 MB."

3. **Wildcard Support**: No wildcard support in initial implementation. Users must mention files explicitly. This keeps the parser simple and predictable.

4. **Line Range Behavior**: Line ranges use inclusive bounds on both ends. The syntax `@file.rs#L10-20` includes both line 10 and line 20. This matches common editor behavior and user expectations. For example:

  - `#L10-20` includes lines 10, 11, 12, ..., 19, 20 (11 lines total)
  - `#L15` includes only line 15 (single line)
  - `#L1-3` includes lines 1, 2, 3 (first three lines)

5. **Content Caching**: Implement session-level caching of file contents. When a file is mentioned multiple times in the same session, read it once and cache the contents. Cache is invalidated if the file is modified (check mtime). This improves performance for repeated references.

6. **History Preservation**: Preserve the original @ mentions in conversation history. This maintains the user's intent and allows the agent to understand the context structure. The file contents are injected only into the prompt sent to the provider, not stored permanently in history.

7. **Search Tool Implementation**: Implement grep tool as part of this plan to enable `@search` and `@grep` mentions. This is integrated into Phase 3 rather than deferred to future work.

8. **Web Fetch Implementation**: Implement fetch tool as part of this plan to enable `@url` mentions. This is integrated into Phase 4 with comprehensive security validation.

9. **Unified Context Injection**: All mention types (files, search results, web content) use the same augmentation framework for consistency.

## Success Metrics

- Feature adoption: percentage of interactive sessions using @ mentions (target: 60%+)
- Mention type usage: distribution of file vs search vs URL mentions
- Error rate: percentage of mentions that fail to resolve (target: under 5%)
- Performance targets:
 - File loading: under 100ms for typical files
 - Search execution: under 2 seconds for typical codebase
 - URL fetching: under 5 seconds for typical pages
- User satisfaction: reduction in explicit tool invocations
- Security: zero SSRF incidents, zero path traversal incidents
- Test coverage: maintain 80%+ coverage across all phases and tools

## Dependencies

### New External Dependencies Required

- `html2md = "0.2"` - HTML to Markdown conversion for web content
- `regex = "1.10"` - Regex pattern matching for grep tool (may already exist)

### Existing Dependencies Used

- `tokio` - Async runtime for file I/O, HTTP requests, search
- `rustyline` - Readline for interactive input
- `walkdir` - File tree traversal for grep
- `similar` - Fuzzy matching for suggestions
- `colored` - Colored terminal output
- `reqwest` - HTTP client for URL fetching (already in dependencies)
- `anyhow` - Error handling
- `thiserror` - Custom error types

### Infrastructure Requirements

- FileOpsTool infrastructure (already exists)
- Special command parser pattern (already exists)
- Tool registry system (already exists)
- Provider abstraction (already exists)

## Integration with Gap Analysis Roadmap

This implementation plan addresses multiple critical gaps identified in the XZatoma vs Zed Agent comparison:

### Gaps Addressed by This Plan

1. **Advanced Code Search (Grep Tool)** - Phase 3

  - Regex-based search with pagination
  - File filtering with glob patterns
  - Context lines around matches
  - Integrated with `@search` and `@grep` mention syntax

2. **Web Content Retrieval (Fetch Tool)** - Phase 4

  - URL fetching with HTML-to-Markdown conversion
  - SSRF prevention and security validation
  - Integrated with `@url` mention syntax
  - Rate limiting and size controls

3. **Unified Context Injection** - Phases 1-2
  - Single framework for files, search, and web content
  - Consistent caching and error handling
  - Conversation history preservation

### Gaps NOT Addressed (Separate Implementation)

1. **Web Search Tool** - Requires API integration (Exa, Google)

  - Could add `@websearch:"query"` syntax in future extension
  - Separate implementation plan recommended

2. **Token Usage Tracking** - Provider-level feature

  - Unrelated to context mentions
  - Should be implemented in provider abstraction layer

3. **Conversation Persistence** - Agent-level feature

  - Separate from context injection
  - Requires storage layer design

4. **Subagent/Multi-Agent** - Advanced coordination
  - Future enhancement after core features stabilize

### Architecture Benefits

By implementing grep and fetch tools as part of the mention system:

- **Unified User Experience**: Single @ syntax for all context types
- **Consistent Error Handling**: Same patterns across files, search, URLs
- **Shared Infrastructure**: Caching, validation, display formatting
- **Security Foundation**: SSRF prevention, path validation, rate limiting
- **Performance Optimization**: Unified caching strategy with TTLs

## Risk Mitigation

- Large file handling: enforce size limits from ToolsConfig, warn before loading
- Path traversal: reuse existing FileOpsTool validation logic
- Performance: async file loading, progress indicators for multiple files
- Backward compatibility: feature is purely additive, no breaking changes
- Security: validate all paths, prevent access outside working directory
