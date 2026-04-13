# Directory Mention Support Implementation

## Overview

This document explains the implementation of directory mention support in the
XZatoma chat interface. Two related bugs were fixed:

1. Using `@path/to/dir` failed with a hard error instead of injecting a
   directory listing.
2. Even after that fix, the LLM wrote files to the wrong location because the
   path was stripped from the instruction text.

Both fixes are described below.

## Problem Statement

### Bug 1: directory paths caused a hard error

The `@mention` system in XZatoma was designed to inject file contents into the
agent prompt. The parser accepted any valid relative path (letters, digits, `/`,
`.`, `_`, `-`) without distinguishing between files and directories. Path
resolution (`resolve_mention_path`) succeeded for both kinds of paths, but
`load_file_content` always rejected non-file paths with a `Not a file` error:

```text
Error: tmp/output: File load error: Not a file: tmp/output (Not a file)
```

This was particularly disruptive for the common use case of telling an agent
_where_ to write its output. For example:

```text
Write all generated reports to @tmp/output
```

The user's intent is clear, but the system produced an error and discarded the
mention entirely, leaving the agent without the context it needed.

### Bug 2: path stripped from instruction after the first fix

After Bug 1 was fixed (directory listing injected as context), the LLM still
wrote files to the wrong location. The root cause was in `parse_mentions`: when
a mention is recognised, the `@mention` token is removed from the cleaned text
that becomes the instruction. So the prompt the LLM received was:

```text
Directory listing: tmp/output
Contents: 0 file(s), 0 subdirectories

  (empty directory)

---

Answer the questions … Use subagents to write each question and answer to
individual files in
```

The path `tmp/output` appears in the prepended listing header but is completely
absent from the instruction. The LLM guessed a location (`input/questions/`)
rather than using the intended one.

## Design Decisions

### Fix 1: directory check in `augment_prompt_with_mentions`

The fix was placed in `augment_prompt_with_mentions` rather than inside
`load_file_content`. This keeps the two functions' responsibilities clean:

- `load_file_content` remains a focused, synchronous-style helper that loads a
  single text file and enforces size and binary constraints.
- `augment_prompt_with_mentions` is already the coordinator that decides which
  loading strategy to use for each mention type. Adding a `is_dir()` branch
  there follows the existing pattern for URL and search/grep dispatch.

The directory check runs immediately after `resolve_mention_path` succeeds,
before the file cache is consulted, so directory listings bypass the
`MentionCache` entirely. Caching directory listings would be incorrect because
the directory contents can change between turns.

### Fix 2: preserve file/directory path in cleaned text

`parse_mentions` builds a `cleaned` string by walking the input and skipping
over every recognised mention. After Fix 1 the listing was injected, but the
path disappeared from the instruction because the `@tmp/output` token was
consumed and nothing was written to `cleaned`.

The fix adds a single branch in the `@` handler: when the parsed mention is a
`Mention::File` (which covers both files and directories), the bare path — the
mention text without the `@` prefix and without any `#L…` line-range specifier —
is written into `cleaned`. Search, grep, and URL mentions continue to be
stripped because they are pure content injections with no path the LLM needs to
reference in its instruction.

The effect on the example prompt:

```text
Before fix:
  "…write each question and answer to individual files in "

After fix:
  "…write each question and answer to individual files in tmp/output"
```

For file mentions with a line-range specifier, only the path is preserved; the
`#L10-20` part is consumed but not emitted, which keeps the instruction
readable:

```text
Before: "Review @file.rs#L10-20 for issues"
After:  "Review file.rs for issues"
```

### Recursive listing via `walkdir`

The project already depends on `walkdir` (used by `ListDirectoryTool`), so no
new dependency was introduced. The walker is configured with:

| Setting       | Value | Reason                                              |
| ------------- | ----- | --------------------------------------------------- |
| `min_depth`   | 1     | Exclude the root directory entry itself             |
| `max_depth`   | 10    | Prevent runaway recursion in deeply nested trees    |
| `max_entries` | 200   | Keep prompt injection bounded for large directories |
| Sort by name  | yes   | Produce stable, human-readable output across runs   |

When the entry limit is reached a truncation note is appended so the agent knows
the listing is incomplete.

### Non-existent directories

A directory that does not yet exist is a common and legitimate use case: the
user wants the agent to create a directory and write files into it. Rather than
silently failing, `load_directory_content` detects the absent path via
`Path::exists()` and injects a clear note:

```text
Directory listing: tmp/output
Status: Does not exist (target output directory — will be created when files are
written here)
```

This gives the agent the context it needs without producing an error.

### `load_file_content` fallback improvement

The `Not a file` error message inside `load_file_content` was extended to hint
at the directory listing feature when the path is a directory:

```text
Not a file: tmp/output (is a directory — use @directory/path to get a listing)
```

In normal usage, `augment_prompt_with_mentions` intercepts directory paths
before they reach `load_file_content`. The improved message is a safety net for
callers that invoke `load_file_content` directly.

## Code Changes

### `src/mention_parser.rs`

#### New public function: `load_directory_content`

```rust
pub async fn load_directory_content(
    mention_path: &str,
    dir_path: &Path,
    max_entries: usize,
) -> String
```

Produces a formatted directory tree string. Returns a "does not exist" notice
for absent paths. Uses `walkdir` for recursive traversal. Called by
`augment_prompt_with_mentions` when a mention resolves to an existing directory.

#### Modified function: `augment_prompt_with_mentions`

A directory check was inserted immediately after the successful
`resolve_mention_path` call, before the `MentionCache` lookup:

```rust
if file_path.is_dir() {
    let dir_listing =
        load_directory_content(&file_mention.path, &file_path, 200).await;
    file_contents.push(dir_listing);
    successes.push(format!("Listed directory @{}", file_mention.path));
    continue;
}
```

#### Modified function: `parse_mentions`

The `@` handler now writes the file path back to `cleaned` for `Mention::File`
variants before advancing past the consumed characters:

```rust
if let Mention::File(ref fm) = mention {
    cleaned.push_str(&fm.path);
}
mentions.push(mention);
i += 1 + consumed;
```

Search, grep, and URL mentions are unaffected and continue to be stripped.

#### Modified function: `load_file_content`

The `Not a file` error message now includes a direction hint when the path is a
directory.

### `src/commands/special_commands.rs`

#### `print_help`

Added `@path/to/dir` to the CONTEXT MENTIONS quick reference table.

#### `print_mention_help`

Added a full DIRECTORY MENTIONS section documenting syntax, examples, and
behaviour. Added a troubleshooting entry covering empty and absent directories.

## Output Format

A directory listing injected into the prompt looks like this:

```text
Directory listing: tmp/output
Contents: 3 file(s), 1 subdirectorie(s)

  archive/
  archive/old.txt  (111 bytes)
  data.json  (890 bytes)
  report.md  (567 bytes)
```

For an absent directory:

```text
Directory listing: tmp/output
Status: Does not exist (target output directory — will be created when files are
written here)
```

The listing is prepended to the user prompt, separated by `---`, in the same way
as file contents and search results.

## Testing

The following tests were added or updated in `mention_parser.rs`:

### New tests for directory listing

| Test name                                                | What it covers                                    |
| -------------------------------------------------------- | ------------------------------------------------- |
| `test_load_directory_content_nonexistent`                | Absent directory produces "Does not exist" notice |
| `test_load_directory_content_empty_directory`            | Empty directory produces "(empty directory)" note |
| `test_load_directory_content_with_files`                 | Files and subdirectories appear in the listing    |
| `test_load_directory_content_truncation`                 | Listing is truncated at `max_entries`             |
| `test_augment_prompt_with_existing_directory_mention`    | Full pipeline: directory injected into prompt     |
| `test_augment_prompt_with_nonexistent_directory_mention` | Non-existent path falls through gracefully        |
| `test_augment_prompt_directory_mention_not_cached`       | Directory listings do not populate the file cache |

### Updated tests for path preservation in cleaned text

Four existing tests that asserted the old "strip everything" behaviour were
renamed and their assertions updated to reflect that file paths are now
preserved:

| Old name                                     | New name                                                        |
| -------------------------------------------- | --------------------------------------------------------------- |
| `test_parse_mentions_cleans_single_file`     | `test_parse_mentions_preserves_file_path_in_cleaned`            |
| `test_parse_mentions_cleans_multiple_files`  | `test_parse_mentions_preserves_multiple_file_paths_in_cleaned`  |
| `test_parse_mentions_cleans_file_with_range` | `test_parse_mentions_preserves_file_path_without_line_range`    |
| `test_parse_mentions_preserves_text`         | `test_parse_mentions_preserves_file_path_with_surrounding_text` |

The URL and search/grep cleaning tests (`test_parse_mentions_cleans_url`,
`test_parse_mentions_cleans_search`) are unchanged — those mention types are
still stripped from the cleaned text.

## Limitations and Future Work

- **No caching**: Directory listings are always re-read. A short-lived cache
  with a per-turn TTL could reduce latency when the same directory is mentioned
  multiple times in one message.
- **Binary files in listings**: Binary files (images, compiled objects) appear
  in directory listings by name and size only. Their contents are never loaded.
- **Line-range syntax on directories**: If a user writes `@src#L1-10`, path
  resolution produces a directory and the line range is silently ignored in
  favour of the full listing. A future improvement could warn the user.
- **Absolute paths remain blocked**: Security constraints that prevent absolute
  paths and directory traversal (`../`) apply equally to directory mentions.
