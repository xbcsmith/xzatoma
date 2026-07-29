# Chat Command Unification Phase 4: `format_models_help_text`

## Summary

Added `format_models_help_text()` to `src/commands/special_commands.rs` and
updated `print_models_help()` to delegate to it. This mirrors the pattern
already established for all other help functions in the module (for example,
`format_help_text`, `format_mode_help_text`, `format_safety_help_text`, and
similar functions).

## Motivation

`print_models_help` previously embedded its raw-string literal directly inside a
`println!` call. That made the text impossible to capture in tests without
redirecting stdout. Extracting it into `format_models_help_text()` makes the
content:

- directly testable with `assert!(!text.is_empty())` or content assertions,
- reusable by callers that need the string rather than side-effecting output,
- consistent with every other help-text function in the module.

## Changes

### `src/commands/special_commands.rs`

- Added `pub fn format_models_help_text() -> String` immediately before
  `print_models_help`. The function owns the raw-string literal previously
  inside the `println!` call and returns it as a `String`.
- Updated `print_models_help` to call
  `println!("{}", format_models_help_text())`.
- Updated the doc comment on `print_models_help` to reference
  `format_models_help_text` as the underlying source.

## Design Decisions

The extracted function follows the established module convention exactly:

- Raw-string literal as the return value (`.to_string()` on a `&str`).
- Full `///` doc comment with `# Returns` and `# Examples` sections.
- The `# Examples` block contains a runnable assertion so `cargo test` verifies
  the doc example compiles and passes.

No other files required changes. The public API surface grows by one function;
the behaviour of `print_models_help` is unchanged.

## Validation

```text
cargo fmt --all                                     -- pass
cargo check --all-targets --all-features            -- pass
cargo clippy --all-targets --all-features -D warnings -- pass
cargo test --all-features -- commands::special_commands -- 90 passed
```
