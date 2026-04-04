# Phase 7: Mention Parser Evals Implementation

## Overview

Phase 7 adds a data-driven, offline evaluation suite for the mention parser
subsystem. It covers `parse_mentions()` and `augment_prompt_with_mentions()`
with two test modes: pure parsing assertions and full augmentation assertions
using temporary file fixtures.

## Deliverables

| Deliverable               | Path                                                             |
| ------------------------- | ---------------------------------------------------------------- |
| Content fixture files (4) | `evals/mention_parser/files/`                                    |
| Scenario definitions      | `evals/mention_parser/scenarios.yaml`                            |
| Eval README               | `evals/mention_parser/README.md`                                 |
| Integration test          | `tests/eval_mention_parser.rs`                                   |
| Implementation doc        | `docs/explanation/phase7_mention_parser_evals_implementation.md` |

## Fixture Files

Four fixture files were created under `evals/mention_parser/files/`.

| File               | Purpose                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------ |
| `sample.rs`        | 20-line Rust source file with a `pub struct` and `pub fn new` for content and line-range testing |
| `config.yaml`      | Small YAML configuration file for file content injection testing                                 |
| `large_binary.bin` | File containing null bytes that triggers the binary-detection rejection path                     |
| `README.md`        | Markdown file for testing uppercase-filename file mention resolution                             |

### Binary fixture details

`large_binary.bin` was created with `printf 'test\x00binary\x00content'`. It
contains null bytes (0x00 at positions 4 and 11). When `load_file_content` reads
the file with `fs::read_to_string`, the call succeeds because null bytes are
valid UTF-8. The subsequent check `contents.contains('\0')` returns `true` and
the function returns
`Err(XzatomaError::FileLoad("Binary file cannot be loaded: ..."))`. The test
harness writes `b"test\x00binary"` programmatically when `"__binary__"` is the
source value in a scenario's `working_dir_files` map, keeping the binary content
out of the text-based fixture directory.

## Scenarios

Eighteen scenarios are defined in `evals/mention_parser/scenarios.yaml`. The
scenarios are split into ten parse-only and eight augmentation scenarios.

### Parse-only scenarios

These scenarios call `parse_mentions(prompt)` and assert `mention_count` and
`mention_types`.

| Scenario ID                       | What it tests                                         |
| --------------------------------- | ----------------------------------------------------- |
| `parse_simple_file_mention`       | `@sample.rs` parses as one File mention               |
| `parse_file_with_line_range`      | `@sample.rs#L5-10` parses as one File mention         |
| `parse_search_mention`            | `@search:"fn main"` parses as one Search mention      |
| `parse_grep_mention`              | `@grep:"^pub fn"` parses as one Grep mention          |
| `parse_url_mention`               | `@url:https://example.com` parses as one Url mention  |
| `parse_multiple_mentions`         | File and Search mentions in one prompt parse in order |
| `parse_escaped_at_symbol`         | `\@` produces zero mentions                           |
| `parse_no_mentions_in_plain_text` | Plain text produces zero mentions                     |
| `parse_absolute_path_not_parsed`  | `@/etc/passwd` produces zero mentions                 |
| `parse_path_traversal_not_parsed` | `@../../etc/passwd` produces zero mentions            |

### Augment scenarios

These scenarios create a temporary directory, install fixture files, parse the
prompt, call `augment_prompt_with_mentions()`, and assert the augmented output.

| Scenario ID                              | What it tests                                              |
| ---------------------------------------- | ---------------------------------------------------------- |
| `augment_file_content_loaded`            | File content is injected into the augmented prompt         |
| `augment_file_with_line_range`           | Only the specified line range appears in the output        |
| `augment_readme_file_loaded`             | An uppercase-named Markdown file is loaded correctly       |
| `augment_config_file_loaded`             | A YAML configuration file is loaded and injected           |
| `augment_missing_file_error`             | A missing file produces an error placeholder in the output |
| `augment_binary_file_error`              | A binary file produces an error placeholder in the output  |
| `augment_search_mention_injects_results` | Search results are injected into the augmented prompt      |
| `augment_grep_mention_injects_results`   | Grep results are injected into the augmented prompt        |

## Test Architecture

### Test file: `tests/eval_mention_parser.rs`

The test is a single `#[tokio::test]` function `eval_mention_parser_scenarios`
that:

1. Reads `evals/mention_parser/scenarios.yaml`.
2. Iterates over every scenario, calling `run_scenario` for each.
3. Collects pass/fail counts and reports a summary.
4. Panics if any scenario fails.

The top-level function is async because `augment_prompt_with_mentions` is async.
Parse-only scenarios are handled by the synchronous helper `run_parse_only`.

### Parse-only runner

`run_parse_only` calls `parse_mentions(&scenario.input.prompt)` and asserts:

- `mention_count`: the length of the returned `Vec<Mention>` equals the expected
  value.
- `mention_types`: each mention in the returned vector has the expected type
  label (`"file"`, `"search"`, `"grep"`, `"url"`) in order.

`parse_mentions` always returns `Ok`. Invalid paths (absolute, traversal) and
escaped at signs simply produce zero mentions rather than errors.

### Augment runner

`run_augment`:

1. Creates a `tempfile::TempDir` for the scenario.
2. For each entry in `scenario.input.working_dir_files`:
   - If the source is `"__binary__"`, writes `b"test\x00binary"` to the
     destination path.
   - Otherwise, calls `fs::copy` to copy the fixture byte-for-byte to the
     destination path (preserving binary content).
3. Calls `parse_mentions` on the prompt to obtain the mention list.
4. Calls
   `augment_prompt_with_mentions(mentions, prompt, temp_dir, 1 MiB, cache)`.
5. Asserts:
   - `output_contains`: each string in the list is a substring of the augmented
     output.
   - `output_not_contains`: each string in the list is absent from the augmented
     output.
   - `errors_nonempty`: when `true`, the `Vec<LoadError>` returned by
     augmentation must be non-empty.

### Why tokio::test

`augment_prompt_with_mentions` is an async function that drives
`load_file_content` (which uses `tokio::fs`) and `GrepTool::search`. Wrapping
the test harness with `#[tokio::test]` avoids manually constructing a Tokio
runtime and is consistent with the pattern used in `tests/eval_run_command.rs`.

### Why fs::copy for fixture installation

Using `fs::copy` rather than `fs::read_to_string` plus `fs::write` preserves
binary content byte-for-byte. The `large_binary.bin` fixture contains null
bytes. If `read_to_string` were used to copy it, the null byte would survive
(null bytes are valid UTF-8) but the pattern is misleading. Using `fs::copy`
makes the intent clear: the fixture is copied verbatim, regardless of content.

## Behaviors Confirmed

The following behaviors were confirmed by running the eval suite:

### parse_mentions never returns Err

`parse_mentions` always returns `Ok((Vec<Mention>, String))`. Invalid mention
paths (absolute, traversal, bad characters) cause `try_parse_mention_at` to
return `None`, leaving the `@` character in the cleaned output. No error is
propagated to the caller.

### Backslash-escaped at signs produce zero mentions

When `@` is preceded by a backslash (`\@`), the parser recognises it as escaped
and pushes `@` into the cleaned output without attempting to parse a mention.

### At signs not at word boundaries produce zero mentions

The `valid_start` guard requires `@` to appear at position 0 or after
whitespace. `test@example.com` produces zero mentions because `@` is preceded by
`t`.

### Augmented prompt structure

`augment_prompt_with_mentions` appends the original prompt after all injected
file/search/URL content, separated by `\n---\n\n`. For a single file mention the
output is:

```text
File: sample.rs (N lines)

```

<file content>
```
---

<original prompt>
```

The original prompt appears verbatim, including any mention syntax that was in
the input.

### Line range extraction is 1-based and inclusive

`extract_line_range(5, 10)` returns lines 5 through 10 inclusive (1-based
indexing). Content from lines outside that range does not appear in the output.

### Binary file detection uses null-byte check

`load_file_content` reads the file with `fs::read_to_string`. If the resulting
string contains `'\0'`, the function returns a `FileBinary` error. This means:

- Files with truly non-UTF-8 bytes (e.g. high bytes) would fail `read_to_string`
  with an IO error before the binary check.
- Files with null bytes are valid UTF-8 and are caught by the binary check.
- The `large_binary.bin` fixture uses null bytes to exercise the intended code
  path.

### Search and grep results use format_search_results

Both `@search:"..."` and `@grep:"..."` produce output via
`format_search_results`, which formats the header as
`Search results for '<pattern>': N match(es)`. The eval test asserts this prefix
string appears in the augmented output.

## Quality Gates

All gates passed before marking Phase 7 complete:

```sh
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test eval_mention_parser -- --nocapture
```

Test results: 18 passed, 0 failed out of 18 scenarios.

All previous eval suites were confirmed passing:

- `cargo test --test eval_skills_command` -- 16 passed, 0 failed
- `cargo test --test eval_config_validation` -- 28 passed, 0 failed
- `cargo test --test eval_run_command` -- 19 passed, 0 failed

All Markdown files were linted and formatted:

```sh
markdownlint --fix --config .markdownlint.json <file>
prettier --write --parser markdown --prose-wrap always <file>
```
