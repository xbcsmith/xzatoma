# Mention Syntax Reference

## Overview

Context mentions inject external content into agent prompts using `@`-prefixed
syntax. They allow you to include file contents, search results, and web content
without manual copy-paste.

This document is a concise syntax reference. For detailed usage patterns and
examples, see the [how-to guide](../how-to/use_context_mentions.md).

## Mention Types

| Mention Type        | Syntax                     | Description                     | Case Sensitivity     |
| ------------------- | -------------------------- | ------------------------------- | -------------------- |
| File                | `@path/to/file.rs`         | Include file contents           | Filesystem-dependent |
| File Range          | `@path/to/file.rs#L10-25`  | Include specific line range     | Filesystem-dependent |
| File (abbreviation) | `@main`, `@lib`, `@readme` | Smart path expansion            | Filesystem-dependent |
| Search              | `@search:"pattern"`        | Case-insensitive literal search | Case-insensitive     |
| Grep                | `@grep:"regex"`            | Case-sensitive regex search     | Case-sensitive       |
| URL                 | `@url:https://example.com` | Fetch and include web content   | N/A                  |

## Line Range Syntax

Line ranges are appended to file mentions with a `#L` prefix. Line numbers are
1-based.

| Syntax    | Meaning                         |
| --------- | ------------------------------- |
| `#L10-20` | Lines 10 through 20 (inclusive) |
| `#L50`    | Just line 50                    |
| `#L100-`  | Line 100 to end of file         |
| `#L-50`   | Start of file through line 50   |

Examples:

```text
@src/main.rs#L1-30       Include the first 30 lines
@src/error.rs#L50        Include only line 50
@src/config.rs#L100-     Include from line 100 to the end
@src/lib.rs#L-20         Include from the start through line 20
```

## File Abbreviations

XZatoma supports smart path expansion for common files:

| Abbreviation | Expanded Path |
| ------------ | ------------- |
| `@main`      | `src/main.rs` |
| `@lib`       | `src/lib.rs`  |
| `@readme`    | `README.md`   |

Smart expansion tries multiple candidate paths in order and uses the first
match.

## Search and Grep Behavior

### `@search:"pattern"`

- Invokes GrepTool with `case_sensitive=false`
- Performs literal string matching (not regex)
- Scans all files in the project

### `@grep:"regex"`

- Invokes GrepTool with `case_sensitive=true`
- Supports full Rust regex syntax
- Scans all files in the project

Both mention types produce results formatted as:

```text
file_path:line_number: matching line content
```

Results are injected as context into the augmented prompt sent to the AI
provider.

## URL Mention Constraints

### Protocol Requirement

URLs must include the full protocol prefix:

- `http://` or `https://`

Bare hostnames (e.g., `@url:example.com`) are not supported.

### SSRF Protection

The following targets are blocked to prevent server-side request forgery:

- Private IP ranges: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
- Loopback addresses: `127.0.0.1`, `::1`
- Localhost: `localhost`, `localhost:*`
- Link-local addresses: `169.254.0.0/16`

### Limits

| Constraint       | Value         |
| ---------------- | ------------- |
| Max URLs         | 10 per prompt |
| Timeout          | 60 seconds    |
| Max content size | 1 MB          |

### Supported Content Types

| Content Type | Handling                   |
| ------------ | -------------------------- |
| HTML         | Converted to readable text |
| JSON         | Formatted and displayed    |
| XML          | Formatted and displayed    |
| Plain text   | Displayed as-is            |
| Other types  | Rejected with an error     |

## Resolution Behavior

- File mentions are resolved relative to the project root directory.
- Smart expansion tries multiple candidate paths (e.g., `@main` tries
  `src/main.rs`).
- Resolved content is cached and reused for repeated mentions within a session.
- Failed mentions produce clear error placeholders in the augmented prompt
  rather than silently dropping.

## See Also

- [How to use context mentions](../how-to/use_context_mentions.md) -- detailed
  usage patterns, examples, and troubleshooting
