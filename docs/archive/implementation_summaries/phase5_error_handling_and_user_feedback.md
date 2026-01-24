# Phase 5 - Error Handling and User Feedback

## Overview

Phase 5 improves robustness and the user experience around mention-based content loading
(`@file`, `@url`, etc.). The focus here is:

- Introduce structured error types for loading problems (files, URLs, parsing).
- Provide graceful degradation when loads fail (insert clear placeholders into prompts).
- Surface concise, actionable feedback to the user in the CLI and to the agent in prompts.
- Add comprehensive tests and documentation.

This work complements the fetch tool and mention parsing implemented in earlier phases by
adding predictable error semantics and making failures informative instead of opaque.

## Components Delivered

- `src/mention_parser.rs` — Added:
 - `LoadErrorKind` enum for classifying load failures.
 - `LoadError` struct (with `Display`) to convey structured failures and optional suggestions.
 - Heuristic classifiers for file/URL errors.
 - Updated `augment_prompt_with_mentions` to:
  - Return structured `LoadError`s.
  - Insert clear placeholders into the augmented prompt when content cannot be included.
- `src/error.rs` — Added new high-level `XzatomaError` variants to make fetch/search/file errors more explicit (e.g., `Fetch`, `FileLoad`, `MentionParse`, `Search`, `RateLimitExceeded`).
- `src/lib.rs` — Re-exported `LoadError` and `LoadErrorKind` so libraries can pattern-match on error kinds.
- Tests updated / added:
 - Unit tests for `LoadError` usage and classification.
 - Integration-style tests validating graceful degradation and user-facing placeholders when loads fail (missing files, SSRF blocked URLs, oversized files).
- Documentation:
 - `docs/explanation/phase5_error_handling_and_user_feedback.md` (this file) describing behavior, implementation, and tests.

## Implementation Details

Goals and design choices:

- Represent load failures in a structured way:
 - `LoadErrorKind` is a small enum (e.g., `FileNotFound`, `FileTooLarge`, `FileBinary`, `UrlSsrf`, `UrlRateLimited`, `UrlHttpError`, `ParseError`, `Unknown`).
 - `LoadError` contains `kind`, `source` (file path or URL), `message`, and optional `suggestion`.
 - `LoadError` implements `Display` (concise, user-oriented) so CLI printing remains simple.

- Graceful degradation:
 - When loading a file or URL fails, we:
  1. Add a `LoadError` to the returned error list.
  2. Insert a prompt placeholder that clearly indicates the content was not included and shows the failure reason. Example placeholder:
    ```
    Failed to include file foo.rs:

    ```text
    File not found: foo.rs (File not found)
    ```
    ```
  - This ensures that the agent receives explicit context that the content is missing rather than silently proceeding.

- Error classification:
 - Classification is heuristic-based, using the underlying error messages to choose `LoadErrorKind`.
 - The heuristics cover common cases (missing file, permission, binary file detection, file too large, SSRF blocks, timeouts, HTTP errors, rate limiting).
 - Where appropriate, helpful `suggestion`s are attached (e.g., for `FileTooLarge`, a tip to increase `max_file_read_size`).

- Mapping to top-level error types:
 - We added explicit variants in `XzatomaError` for fetch and load related errors so higher-level modules can return typed errors when needed.

- Backward compatibility for CLI:
 - `augment_prompt_with_mentions` now returns `(String, Vec<LoadError>)`.
 - The `run_chat` command prints each load error using `eprintln!("Warning: {}", error)`, which leverages `LoadError`'s `Display` output to present a compact message and any suggestion.

- Example snippets from the implementation:
```xzatoma/src/mention_parser.rs#L742-820
// (excerpt showing LoadErrorKind and LoadError signatures and Display)
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

pub struct LoadError {
  pub kind: LoadErrorKind,
  pub source: String,
  pub message: String,
  pub suggestion: Option<String>,
}
```

```xzatoma/src/mention_parser.rs#L900-1020
// (excerpt showing handling of missing files and URL fetch failures
// - placeholder inserted into prompt
// - LoadError recorded)
let load_err = LoadError::new(
  LoadErrorKind::PathOutsideWorkingDirectory,
  file_mention.path.clone(),
  e.to_string(),
  Some("Ensure the path is relative and inside the working directory".to_string()),
);
errors.push(load_err.clone());
file_contents.push(format!(
  "Failed to include file {}:\n\n```text\n{}\n```",
  file_mention.path, load_err.message
));
```

## Testing

- Added tests for:
 - `LoadError` creation and `Display`.
 - `augment_prompt_with_mentions` behavior for:
  - Missing files — confirm a `LoadError::FileNotFound` and placeholder in the augmented prompt.
  - SSRF-protected URLs — confirm `LoadError::UrlSsrf`.
  - Oversized files — confirm `LoadError::FileTooLarge` and presence of a suggestion.
 - New `XzatomaError` variants display.
 - Existing coverage (file loading, line range extraction, mention parsing) was preserved.

- Test commands executed during validation:
 - `cargo fmt --all`
 - `cargo check --all-targets --all-features`
 - `cargo clippy --all-targets --all-features -- -D warnings`
 - `cargo test --all-features`

- Validation results (at time of implementation):
 - All checks passed.
 - Unit tests: 377 passed; 0 failed (run via `cargo test --all-features`).

## Usage Examples

- How to call the augment function and handle structured errors:
```xzatoma/src/mention_parser.rs#L1030-1060
// Example (conceptual):
let mentions = vec![
  Mention::File(FileMention { path: "missing.rs".into(), start_line: None, end_line: None }),
  Mention::Url(UrlMention { url: "http://127.0.0.1".into() }),
];
let mut cache = MentionCache::new();
let (augmented_prompt, load_errors) =
  augment_prompt_with_mentions(&mentions, "Please review", Path::new("."), 1024, &mut cache).await;

for err in &load_errors {
  // Display to user (CLI will call `eprintln!("Warning: {}", err)`).
  println!("Warning: {}", err);
}

// The `augmented_prompt` contains placeholders for failed inclusions so the agent is aware.
```

- In interactive chat (`run_chat`), load errors are printed as concise warnings and
 the agent still receives an augmented prompt with placeholders, enabling the agent to continue
 without crashing.

## Implementation Notes & Future Work

- Classification is currently heuristic-driven (string matching on errors). This is pragmatic and
 extensible; if more precise error typing is desired we can:
 - Return typed errors from lower-level functions (e.g., `load_file_content` and the fetch tool)
  so classification becomes deterministic (no string matching).
 - Perform DNS-based SSRF checks and follow redirect-safe resolution for stricter protections.
 - Respect HTTP caching headers (`Cache-Control`) when storing fetch cache entries.
 - Expose per-domain rate limiting configuration.
- Improve HTML->Markdown conversion by integrating a more robust library if needed (e.g., `html2md`) instead of regex-based transformations.
- Surface "cached" vs "fresh" state more explicitly in CLI output for transparency.

## References

- File Mention Feature Implementation Plan: `docs/explanation/file_mention_feature_implementation_plan.md`
- Phase 4 Fetch Tool docs: `docs/explanation/phase4_fetch_tool_implementation.md`
- Agent & Development guidelines: `AGENTS.md`

## Validation Summary

- Code formatted and linted (`cargo fmt`, `cargo clippy`) with zero warnings.
- Compilation and tests passed (`cargo check`, `cargo test`) — tests were green.
- Documentation added to `docs/explanation/` as required by the project guidelines.

---

If you'd like, I can:
- Make the classifiers stricter by returning typed errors from `load_file_content` and `FetchTool::fetch`.
- Add more tests that mock HTTP responses for deterministic testing of timeouts, large responses, and varied content-types.
- Add CLI flags to control display verbosity for load errors (brief vs. detailed).
