# Using Context Mentions

Learn how to use context mentions to include file contents, search results, and web content directly in your agent prompts.

## Overview

Context mentions (or just "mentions") are a powerful way to add relevant information to your agent prompts without manually copying and pasting content. By using special `@` syntax, you can reference:

- **Files** - Include file contents or specific line ranges
- **Search** - Find patterns across your codebase
- **Grep** - Run regex searches through file contents
- **URLs** - Fetch and include web page content

The agent uses this injected context to better understand your code and requirements, making more accurate decisions.

## Quick Start

### Include a File

```
@config.yaml
```

The agent will see the full contents of `config.yaml`.

### Include a Specific File Range

```
@src/main.rs#L10-25
```

Shows lines 10 through 25 of `src/main.rs`.

### Search for a Pattern

```
@search:"function_name"
```

Finds all lines containing "function_name" anywhere in the project.

### Search with Regex

```
@grep:"^pub fn.*\("
```

Finds all lines matching the regex pattern across the project.

### Include Web Content

```
@url:https://example.com/api-docs
```

Fetches and includes the content from the URL.

## File Mentions

File mentions let you include file contents in your prompt for the agent to analyze.

### Basic File Mention

Simply reference a file by name:

```
Please review the configuration in @config.yaml
```

The agent sees the full file contents inserted into the conversation. XZatoma will:

1. Search for the file in your project
2. Load the contents
3. Inject it into the prompt sent to the AI provider
4. Cache the result for subsequent mentions of the same file

### Full Path Specification

For unambiguous references, use the full path:

```
@src/config/production.yaml
```

This is useful when multiple files have the same name in different directories.

### Abbreviations and Smart Expansion

XZatoma supports helpful abbreviations:

- `@main` - Finds `src/main.rs`
- `@lib` - Finds `src/lib.rs`
- `@readme` - Finds `README.md`
- `@readme.rs` - Finds `src/lib.rs` (common variant)
- `@test_foo.rs` - Finds `src/test_foo.rs`, `tests/test_foo.rs`, or `foo/test_foo.rs`

These expansions follow common Rust conventions and try multiple paths before failing.

### Line Range Syntax

Include only specific lines from a file:

```
@src/agent.rs#L50-100
```

This syntax shows:

- `#L10-20` - Lines 10 through 20 (inclusive)
- `#L50` - Just line 50
- `#L100-` - Line 100 to end of file
- `#L-50` - Start of file through line 50

Line numbers are 1-based (as shown in most editors).

### Line Range Examples

```
@config.yaml#L1-10
Show just the beginning of the config file

@src/main.rs#L1-30
Include the module declarations and imports

@src/error.rs#L50-
Include from line 50 to the end of the file
```

### Viewing Specific Functions

To include a specific function or struct definition, estimate the line numbers:

```
Review this error handler: @src/error.rs#L150-175
```

Or use a search mention instead (see below).

### Multiple File Mentions

Include multiple files in one prompt:

```
Please review @config.yaml, @src/main.rs, and @README.md
```

The agent will see all three files' contents in the augmented prompt.

## Search Mentions

Search mentions use literal string matching to find relevant code.

### Basic Search

```
@search:"error_handler"
```

Finds all lines containing "error_handler" anywhere in the project and shows them with context (file name and line number).

### Search is Case-Sensitive

```
@search:"Error"
```

Finds "Error" but not "error". Use a grep mention for case-insensitive searching.

### Special Characters in Search

For patterns with spaces, use quotes:

```
@search:"pub fn"
```

This finds all lines with "pub fn" (public function declarations).

### Practical Search Examples

```
@search:"TODO"
Find all TODO comments

@search:"FIXME:"
Find all FIXME markers

@search:"impl Error"
Find all Error trait implementations

@search:"#[derive"
Find all derive macros
```

### Search Performance

Searches scan the entire project, which can be slow on large codebases. For better performance:

- Use specific patterns that are unlikely to have many matches
- Use grep (regex) for more precise patterns
- Mention specific files or line ranges for known locations

## Grep Mentions

Grep mentions use regular expressions for more powerful pattern matching.

### Basic Grep Pattern

```
@grep:"^pub fn"
```

Finds all lines starting with "pub fn" (anchored at start of line).

### Grep Regex Syntax

XZatoma uses Rust regex syntax. Common patterns:

- `^` - Start of line
- `$` - End of line
- `.` - Any character
- `*` - Zero or more of previous
- `+` - One or more of previous
- `\w` - Word character (letter, digit, underscore)
- `\d` - Digit
- `\s` - Whitespace
- `[abc]` - Any of a, b, or c
- `[^abc]` - Any except a, b, or c
- `(pattern)` - Group

### Practical Grep Examples

```
@grep:"^impl\s+\w+\s+for"
Find all trait implementations

@grep:"fn\s+\w+\([^)]*\)\s*->"
Find all function declarations with return types

@grep:"#\[test\]"
Find all test functions

@grep:"\.unwrap\(\)"
Find all unwrap calls (potential panics)

@grep:"TODO|FIXME|XXX"
Find all common comment markers
```

### Escaping in Grep Patterns

For literal special characters, use backslash:

```
@grep:"version = \"[0-9]+\.[0-9]+\""
Find semantic version patterns

@grep:"https?://\S+"
Find all URLs
```

### Grep Case-Insensitive Matching

Wrap your pattern with `(?i)`:

```
@grep:"(?i)error"
Matches "error", "Error", "ERROR", etc.
```

## URL Mentions

URL mentions fetch web content and include it in your prompt.

### Basic URL Mention

```
@url:https://docs.rust-lang.org/std/result/enum.Result.html
```

The agent will see the HTML content converted to readable text.

### URLs Must Be Complete

Always use the full URL with protocol:

```
@url:https://example.com/page
```

Not:

```
@url:example.com/page
```

### Supported Protocols

- `http://` - Standard web
- `https://` - Secure web (recommended)

Internal protocols like `file://` are not supported.

### Security Considerations

URL fetching has built-in safety protections:

**Server-Side Request Forgery (SSRF) Prevention**

The following URLs are blocked to prevent attacks:

- Private IP addresses: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
- Loopback: `127.0.0.1`, `::1`
- Link-local: `169.254.0.0/16`
- Localhost: `localhost`, `localhost:*`

**Rate Limiting**

- Maximum 10 URLs per prompt
- 60-second timeout per URL
- Content size limit: 1 MB

**Content Type Handling**

Only text-based content is processed:

- HTML → converted to readable text
- JSON → formatted and displayed
- XML → formatted and displayed
- Plain text → displayed as-is
- Other types → rejected with an error

### Practical URL Mention Examples

```
@url:https://raw.githubusercontent.com/rust-lang/rust/master/RELEASES.md
Include release notes

@url:https://docs.rs/tokio/latest/tokio/
Include Tokio documentation

@url:https://api.github.com/repos/rust-lang/rust
Include GitHub API response
```

### URL Caching

Fetched URL content is cached for 24 hours. If you mention the same URL again, the cached version is used without a new fetch. This improves performance and respects server bandwidth.

### Handling Large Pages

If a page is larger than 1 MB, only the first 1 MB is included and you'll see a message indicating the content was truncated. Try:

- Mentioning a more specific page
- Using a search mention instead if the content is code
- Asking the agent to fetch a different format (like JSON API instead of HTML)

## Common Patterns and Best Practices

### Code Review Pattern

```
Please review @src/feature.rs#L1-50 for correctness and style issues
```

This combines file mentions with line ranges for focused code review.

### Architecture Understanding Pattern

```
Help me understand the architecture. Here are the main components:
@src/agent/mod.rs
@src/providers/mod.rs
@src/tools/mod.rs
```

Multiple file mentions help the agent understand your project structure.

### Documentation Context Pattern

```
Based on @README.md, help me implement the missing feature from:
@url:https://example.com/specification
```

Mix file and URL mentions to relate project docs with external specifications.

### Bug Investigation Pattern

```
I'm getting this error. Please check:
@grep:"error_code_123"
@search:"handle_error"

The error occurs in @src/main.rs#L100-120
```

Use search and file mentions together to investigate issues.

### Feature Implementation Pattern

```
Implement the API from @url:https://api.example.com/docs
following our patterns in @src/api/handlers.rs
```

Reference external requirements alongside similar code patterns.

## Performance Tips

### Minimize Search Scope

Searches scan the entire project. For better performance:

- Use specific search terms unlikely to have many matches
- Use grep with anchors like `^` to narrow results
- Mention specific files for known locations

### Use Line Ranges

For large files, include specific line ranges instead of the whole file:

```
@src/large_file.rs#L100-150
```

Not:

```
@src/large_file.rs
```

### Avoid Redundant Mentions

The agent keeps track of what's been included. Avoid mentioning the same file twice:

```
@config.yaml
Check this config for issues

Can you update @config.yaml to add a new field?
```

The second mention will use the cached content.

### URL Fetching is Expensive

Each URL requires a network request. Minimize mentions:

- Use search for code patterns instead of fetching from GitHub
- Cache important documentation locally
- Mention specific sections instead of entire pages

## Troubleshooting

### "File not found"

The file couldn't be located. Solutions:

- Use the full path: `@src/path/to/file.rs`
- Check the spelling and capitalization
- Use the fuzzy matcher: XZatoma will suggest similar filenames

### "SSRF protection blocked this URL"

The URL is considered unsafe. Common causes:

- Private IP addresses (use public URLs instead)
- Localhost references (access the service locally instead)

### "Content too large"

The fetched content exceeds 1 MB. Solutions:

- Try fetching a specific page instead of the homepage
- Use search mentions for code patterns
- Ask the agent to fetch as JSON if available

### "Search returned too many results"

The pattern matched too many lines. Solutions:

- Use a more specific search term
- Use grep with anchors to narrow results
- Mention specific files instead

### "Timeout fetching URL"

The URL took longer than 60 seconds. Solutions:

- Try again (might be temporary)
- Use a different URL or mirror
- Ask the agent to proceed without it

## FAQ

### Can I mention directories?

Not directly. Mention specific files or use search/grep to find contents. This keeps prompts focused and manageable.

### Are mentions case-sensitive?

File mentions are case-sensitive (matches your filesystem). Search is case-sensitive, but grep with `(?i)` is case-insensitive.

### How large can an included file be?

Individual files are included in full (no size limit), but the total augmented prompt should stay reasonable. Very large files (>100 KB of raw content) may impact token usage.

### Do mentions work in plan files?

Yes, mention syntax works in both interactive prompts and plan files. The agent will include the mentioned content when processing your goals.

### Can I nest mentions?

No, you cannot mention a search result or URL content directly. But the agent can execute a search and then ask you to mention the results.

### What happens if a mention fails?

If a file, search, or URL fails to load:

- The agent is informed via a clear placeholder in the augmented prompt
- A human-readable error message is displayed
- Execution continues with the remaining mentions
- Suggestions for fixing common issues are provided

### Can I use mentions to share secrets?

Not recommended. Avoid mentioning files with credentials, API keys, or other secrets. If you accidentally mention sensitive data, the content was already sent to the AI provider.

### How are mentions different from normal chat?

Mentions pre-load content into the augmented prompt before sending to the AI provider. Regular chat forces the agent to use tools to discover content. Mentions are faster and save tokens.

### Can the agent modify mentioned content?

The agent sees mention content as input context. It cannot directly modify mentioned files through the mention mechanism. The agent uses separate write tools to make changes.

### Do mentions work with all providers?

Yes, mentions are a client-side feature that work with any AI provider (Copilot, Ollama, etc.).

## Advanced Examples

### Full Architecture Analysis

```
Analyze our architecture and recommend improvements. Review:

@src/agent/mod.rs
@src/providers/base.rs
@src/tools/mod.rs
@README.md
```

### Implementing From Specification

```
Implement the logging feature according to this spec:
@url:https://example.com/logging-spec

Avoid these patterns we discussed:
@grep:"unwrap\(\)"

Follow our patterns in:
@src/error.rs#L1-50
@src/lib.rs#L1-30
```

### Debugging with Search and Files

```
I'm getting panic: "message not found". Debug this:

Look for where we handle this error:
@search:"message not found"

And check the surrounding code:
@src/handlers.rs#L100-150

Plus our error types:
@src/error.rs
```

### Multi-File Refactoring

```
Refactor these modules to use a common trait. Current implementations:

@src/handler_a.rs#L50-100
@src/handler_b.rs#L40-90
@src/handler_c.rs#L30-80

Use this pattern:
@src/traits.rs
```

## Getting Help

If you need help with mentions:

- Type `/help` in interactive mode to see all commands
- Type `/mentions` to see mention-specific help (if available)
- Use `/status` to see your current mode
- Ask the agent directly: "How should I reference this file?"

Remember: the agent is there to help! If you're unsure about the right mention syntax, just ask and it will guide you.
