# File Mention Feature Implementation Plan

## Overview

Add support for @ file mentions in interactive chat mode, allowing users to reference files in their prompts using `@filename` syntax. The agent will automatically read mentioned files and include their contents in the conversation context, enabling more efficient file-based discussions without manual read commands.

## Current State Analysis

### Existing Infrastructure

The interactive chat mode (`src/commands/mod.rs`) currently provides:

- Rustyline-based readline loop for user input
- Special command parser (`src/commands/special_commands.rs`) for mode switching and control commands
- FileOpsTool (`src/tools/file_ops.rs`) with read capabilities and path validation
- Agent execution pipeline that processes user input and tool calls
- Conversation history management in the agent
- Two chat modes: Planning (read-only) and Write (read-write)

### Identified Issues

The current workflow requires users to:

- Explicitly ask the agent to read files
- Wait for the agent to decide to use the read tool
- Potentially make multiple round trips for multi-file discussions
- Manually specify file paths in natural language

This creates friction when users want to discuss specific files or provide context from multiple files in a single prompt.

## Implementation Phases

### Phase 1: Core Mention Parser

#### Task 1.1: Create Mention Parser Module

Create `src/mention_parser.rs` with core parsing logic:

- Define `FileMention` struct containing file path and optional line range
- Implement `parse_mentions()` function to extract @ mentions from input strings
- Support syntax patterns: `@filename`, `@path/to/file.rs`, `@file.rs#L10-20`
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

### Phase 3: Error Handling and User Feedback

#### Task 3.1: Comprehensive Error Types

Extend error handling in `src/error.rs`:

- Add `MentionError` variants: FileNotFound, PermissionDenied, FileTooLarge, InvalidPath, BinaryFile
- Implement user-friendly error messages with file size and limit information
- Add suggestions for common mistakes (typos, wrong paths)
- Include context in errors (which file, what went wrong, actual vs expected)
- Format file size errors: "Error: @large_file.txt is too large (5.2 MB). Maximum file size is 1 MB."

#### Task 3.2: Graceful Degradation

Implement partial success handling:

- When some mentions fail, load successful ones and print errors to chat
- Allow user to continue with partial context, prompt still executes
- Display clear summary: "Loaded 2 of 3 files (1 cached). Failed: @missing.txt"
- Print detailed error messages for each failed mention
- Log failures for debugging without blocking execution

#### Task 3.3: User Feedback Display

Add visual feedback in chat loop:

- Show colored status for mention processing: "Loading @file.rs..." (cyan)
- Indicate cached files: "Using cached @config.yaml" (green)
- Display file metadata: size, line count, cache status
- Use existing colored tag infrastructure from chat modes
- Show success: "Loaded @config.yaml (42 lines, 1.2 KB)" (green)
- Show cached: "Cached @main.rs (150 lines)" (green)
- Show errors: "Error: @missing.txt - File not found. Did you mean @main.txt?" (red)
- Show size errors: "Error: @large_file.txt is too large (5.2 MB). Maximum file size is 1 MB." (red)

#### Task 3.4: Testing Requirements

Error handling tests:

- Test file not found with helpful message and suggestions
- Test permission denied scenarios with clear error
- Test file too large rejection with size details in error message
- Test partial success (some files load, some fail, prompt still executes)
- Test path validation errors (absolute paths, traversal attempts)
- Test graceful degradation flow with multiple error types
- Test binary file detection and error handling
- Test cache-related errors (permission changes, file deletion)
- Achieve 80%+ coverage on error paths

#### Task 3.5: Deliverables

- Error types in `src/error.rs` with detailed messages (75-100 lines)
- Error handling logic in mention parser with graceful degradation (125-150 lines)
- User feedback display functions with colored output (100-125 lines)
- Error handling tests covering all scenarios (200-250 lines)

#### Task 3.6: Success Criteria

- All error cases have clear, actionable messages with specific details
- File size errors include actual size and limit
- Partial failures print errors to chat but do not block prompt execution
- User feedback is informative and colored appropriately (green=success/cached, red=error, cyan=loading)
- Cache status is clearly communicated to user
- Tests cover all error paths with 80%+ coverage
- All quality checks pass

### Phase 4: User Experience Enhancements

#### Task 4.1: Fuzzy Path Matching

Implement smart path resolution:

- When exact match fails, search for similar filenames
- Use `similar` crate (already in dependencies) for fuzzy matching
- Suggest alternatives: "Did you mean @src/main.rs?"
- Limit to top 3 suggestions
- Allow configurable similarity threshold

#### Task 4.2: Common Abbreviations

Support convenient shortcuts:

- `@README` resolves to `README.md`
- `@Cargo` resolves to `Cargo.toml`
- `@main` searches for `main.rs`, `main.go`, etc.
- Configurable abbreviation map in ToolsConfig
- Clear indication when abbreviation is used

#### Task 4.3: Visual Feedback Improvements

Enhance display formatting:

- Syntax highlighting for file type in mention summary
- Color-coded file status (green=loaded, red=failed, yellow=warning)
- Progress indicator for multiple files
- File size warnings before loading large files
- Consistent formatting with existing colored tags

#### Task 4.4: Testing Requirements

UX feature tests:

- Test fuzzy matching with typos
- Test abbreviation expansion
- Test suggestion display
- Test colored output formatting
- Test multi-file progress display
- Achieve 80%+ coverage on new features

#### Task 4.5: Deliverables

- Fuzzy matching logic (100-150 lines)
- Abbreviation resolver (75-100 lines)
- Enhanced display functions (100-125 lines)
- Configuration additions (25-50 lines)
- UX tests (150-200 lines)

#### Task 4.6: Success Criteria

- Fuzzy matching correctly suggests alternatives
- Abbreviations work for common files
- Visual feedback is clear and helpful
- No performance degradation with large file sets
- Tests pass with 80%+ coverage
- All quality checks pass

### Phase 5: Documentation and Polish

#### Task 5.1: User Documentation

Create user-facing documentation in `docs/how-to/use_file_mentions.md`:

- Quick start examples
- Syntax reference for @ mentions
- Line range syntax examples
- Common patterns and best practices
- Troubleshooting guide
- FAQ section

#### Task 5.2: Architecture Documentation

Create technical documentation in `docs/explanation/file_mention_architecture.md`:

- Design decisions and rationale
- Component interaction diagram
- Parser implementation details
- Error handling strategy
- Performance considerations
- Future enhancement ideas

#### Task 5.3: Implementation Summary

Create implementation summary in `docs/explanation/file_mention_feature_implementation.md`:

- Overview of delivered features
- Components created and modified
- Implementation details with code examples
- Testing results and coverage metrics
- Validation checklist results
- Usage examples

#### Task 5.4: Help Text Integration

Update interactive help:

- Add file mention syntax to `/help` output in `src/commands/special_commands.rs`
- Include examples in help text
- Update welcome banner if needed
- Add mention syntax to error messages

#### Task 5.5: Testing Requirements

Documentation validation:

- Verify all code examples are runnable
- Test all documented commands
- Ensure links are valid
- Run markdown linter
- Verify examples match actual behavior

#### Task 5.6: Deliverables

- `docs/how-to/use_file_mentions.md` (300-400 lines)
- `docs/explanation/file_mention_architecture.md` (250-350 lines)
- `docs/explanation/file_mention_feature_implementation.md` (400-500 lines)
- Updated help text (50-75 lines modified)
- README.md updates (25-50 lines)

#### Task 5.7: Success Criteria

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

## Success Metrics

- Feature adoption: percentage of interactive sessions using @ mentions
- Error rate: percentage of mentions that fail to resolve
- Performance: time to load and inject file contents (target: under 100ms for typical files)
- User satisfaction: reduction in explicit file read requests
- Test coverage: maintain 80%+ coverage across all phases

## Dependencies

- No new external dependencies required
- Uses existing: tokio, rustyline, walkdir, similar, colored
- Requires FileOpsTool infrastructure (already exists)
- Builds on special command parser pattern (already exists)

## Risk Mitigation

- Large file handling: enforce size limits from ToolsConfig, warn before loading
- Path traversal: reuse existing FileOpsTool validation logic
- Performance: async file loading, progress indicators for multiple files
- Backward compatibility: feature is purely additive, no breaking changes
- Security: validate all paths, prevent access outside working directory
