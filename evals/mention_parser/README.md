# Mention Parser Evals

Data-driven evaluation suite for the `xzatoma` mention parser subsystem.

## Overview

This directory contains offline, deterministic eval scenarios that exercise
`parse_mentions()` and `augment_prompt_with_mentions()` without requiring a
network connection or a live AI provider.

Scenarios are split into two test modes:

- **parse_only** -- calls `parse_mentions(prompt)` and asserts the count and
  types of mentions extracted from the input text.
- **augment** -- creates a temporary directory, installs fixture files, calls
  `augment_prompt_with_mentions()`, and asserts the content of the returned
  augmented prompt string.

## Directory Structure

```text
evals/mention_parser/
├── README.md           -- this file
├── scenarios.yaml      -- all scenario definitions
└── files/              -- content fixture files
    ├── sample.rs       -- 20-line Rust source file
    ├── config.yaml     -- small YAML configuration file
    ├── large_binary.bin -- binary file with null bytes
    └── README.md       -- Markdown file for file mention testing
```

The integration test lives at `tests/eval_mention_parser.rs`.

## Fixture Files

| File               | Purpose                                                                         |
| ------------------ | ------------------------------------------------------------------------------- |
| `sample.rs`        | Rust source with a `pub struct` and `pub fn` for content and line-range testing |
| `config.yaml`      | YAML config file for file mention content injection testing                     |
| `large_binary.bin` | File containing null bytes that triggers binary file rejection                  |
| `README.md`        | Markdown file for testing uppercase filename mention resolution                 |

### Binary fixture

`large_binary.bin` contains null bytes (`\x00`). The mention parser's
`load_file_content` function reads the file with `read_to_string` (which
succeeds because null bytes are valid UTF-8) and then detects the null byte with
`contents.contains('\0')`, returning a `FileBinary` error. The test harness
writes `b"test\x00binary"` programmatically when `"__binary__"` is listed as the
source in `working_dir_files`.

## Scenario Schema

```yaml
- id: unique_scenario_id
  description: "Human-readable description"
  test_mode: parse_only # "parse_only" or "augment"
  input:
    prompt: "text with @mention"
    working_dir_files: # augment only; maps dest -> fixture source
      sample.rs: sample.rs # copies evals/mention_parser/files/sample.rs
      binary.bin: __binary__ # writes programmatic binary content
  expect:
    mention_count: 1 # parse_only: expected number of mentions
    mention_types: # parse_only: ordered list of mention types
      - file
    output_contains: # augment: substrings that must be present
      - "pub struct Sample"
    output_not_contains: # augment: substrings that must be absent
      - "// sample.rs"
    errors_nonempty: true # augment: errors list must be non-empty
```

## Mention Types

| YAML value | Mention variant   | Syntax                                 |
| ---------- | ----------------- | -------------------------------------- |
| `file`     | `Mention::File`   | `@path/to/file.rs` or `@file.rs#L5-10` |
| `search`   | `Mention::Search` | `@search:"pattern"`                    |
| `grep`     | `Mention::Grep`   | `@grep:"regex"`                        |
| `url`      | `Mention::Url`    | `@url:https://example.com`             |

## Parse-Only Behavior

`parse_mentions(input)` always returns `Ok`. It never fails for individual
invalid paths. Instead:

- Absolute paths (`@/etc/passwd`) produce zero mentions.
- Directory traversal paths (`@../../etc`) produce zero mentions.
- Backslash-escaped at signs (`\@`) produce zero mentions.
- At signs not preceded by whitespace or the start of input produce zero
  mentions.

## Augment Behavior

`augment_prompt_with_mentions()` always returns a
`(String, Vec<LoadError>, Vec<String>)` tuple and never panics. Error conditions
are recorded in the `LoadError` list and a placeholder is inserted into the
augmented prompt:

- Missing file: `"Failed to include file <name>"` appears in the prompt.
- Binary file: `"Failed to include file <name>"` appears in the prompt.
- Invalid path: `"Failed to include file <name>"` appears in the prompt.

Search and grep mentions execute `GrepTool::search` against the temporary
working directory and inject formatted results into the prompt.

## Running the Evals

Run the full eval suite:

```sh
cargo test --test eval_mention_parser -- --nocapture
```

Run a single scenario by filtering the test output name:

```sh
cargo test --test eval_mention_parser eval_mention_parser_scenarios -- --nocapture
```

## Adding a New Scenario

1. If the scenario requires a new fixture file, add it to
   `evals/mention_parser/files/`.

2. Add a new entry to `scenarios.yaml` following the schema above.

   - For `parse_only` scenarios, set `mention_count` and optionally
     `mention_types` in the `expect` block.
   - For `augment` scenarios, list any required files in `working_dir_files` and
     set `output_contains` assertions.

3. Run the eval suite and confirm the new scenario passes.

4. No changes to `tests/eval_mention_parser.rs` are required unless you need a
   new `test_mode`.

## Covered Branches

| Branch                  | Scenarios                                                 |
| ----------------------- | --------------------------------------------------------- |
| File mention parsing    | `parse_simple_file_mention`, `parse_file_with_line_range` |
| Search mention parsing  | `parse_search_mention`                                    |
| Grep mention parsing    | `parse_grep_mention`                                      |
| URL mention parsing     | `parse_url_mention`                                       |
| Multiple mentions       | `parse_multiple_mentions`                                 |
| Escaped at sign         | `parse_escaped_at_symbol`                                 |
| No mentions             | `parse_no_mentions_in_plain_text`                         |
| Absolute path rejected  | `parse_absolute_path_not_parsed`                          |
| Traversal path rejected | `parse_path_traversal_not_parsed`                         |
| File content injection  | `augment_file_content_loaded`                             |
| Line range extraction   | `augment_file_with_line_range`                            |
| Markdown file injection | `augment_readme_file_loaded`                              |
| YAML file injection     | `augment_config_file_loaded`                              |
| Missing file error      | `augment_missing_file_error`                              |
| Binary file error       | `augment_binary_file_error`                               |
| Search mention results  | `augment_search_mention_injects_results`                  |
| Grep mention results    | `augment_grep_mention_injects_results`                    |
