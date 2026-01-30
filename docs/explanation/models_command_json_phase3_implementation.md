# models command JSON output (phase 3) implementation

## Overview

Phase 3 implements machine-readable JSON output for the `models` command. It adds:
- `--json` output for both `models list` and `models info`
- Pretty-printed JSON (user-facing readability)
- JSON support for both basic `ModelInfo` objects and extended `ModelInfoSummary` objects (when `--summary` is used)

This enables automation, piping to tools like `jq`, and easier analysis of provider data.

## Components Delivered

- Modified
  - `src/commands/models.rs` — Branching for `--json` and `--summary`, JSON helpers, table helpers, and refactored detailed output.
- Added
  - `tests/models_json_output.rs` — Integration tests ensuring JSON outputs are valid and parseable.
  - This documentation file: `docs/explanation/models_command_json_phase3_implementation.md`
- Tests added (unit & integration)
  - Unit tests in `src/commands/models.rs` for serialization and utility helpers
  - Integration tests in `tests/models_json_output.rs` to validate parseability of produced JSON

## Implementation Details

Design choices:
- Use pretty JSON (`serde_json::to_string_pretty`) for readability in the CLI.
- Keep the default (no flags) behavior unchanged (human-readable tables / detailed text).
- Use existing types:
  - `ModelInfo` (basic model metadata)
  - `ModelInfoSummary` (extensive provider API data, nested `info` field)
- Errors during serialization propagate as `XzatomaError::Serialization` (the existing serialization variant for provider-related JSON errors).

Key behaviors:
- `xzatoma models list`
  - Default: human-readable table (unchanged)
  - `--json` + no `--summary`: outputs JSON array of `ModelInfo`
  - `--json --summary`: outputs JSON array of `ModelInfoSummary`
  - Empty results in `--json` mode print `[]`
- `xzatoma models info --model <name>`
  - Default: human-readable detailed output (unchanged)
  - `--json` + no `--summary`: prints JSON `ModelInfo` object
  - `--json --summary`: prints JSON `ModelInfoSummary` object

Refactor & helper functions:
- `serialize_pretty<T: serde::Serialize + ?Sized>(&T) -> Result<String, serde_json::Error>`
- `output_models_json(models: &[ModelInfo]) -> Result<()>`
- `output_models_summary_json(models: &[ModelInfoSummary]) -> Result<()>`
- `output_model_info_json(model: &ModelInfo) -> Result<()>`
- `output_model_summary_json(model: &ModelInfoSummary) -> Result<()>`
- Table/detailed output helpers were extracted for clarity:
  - `output_models_table`, `output_models_summary_table`,
  - `output_model_info_detailed`, `output_model_summary_detailed`
- Utility:
  - `format_optional_bool(Option<bool>) -> String`

Notes:
- Serialization errors are surfaced via `XzatomaError::Serialization` and bubble up through the normal `Result` types.
- JSON is intentionally pretty-printed (readability prioritized over compactness for CLI use).

## Testing

Automated tests added:

Unit tests in `src/commands/models.rs`:
- `test_output_models_json_empty_array`
- `test_output_models_json_single_model`
- `test_output_models_json_multiple_models`
- `test_output_models_summary_json_with_full_data`
- `test_output_model_info_json_basic_fields`
- `test_output_model_summary_json_all_fields`
- `test_format_optional_bool_values`

Integration tests in `tests/models_json_output.rs`:
- `test_list_models_json_output_parseable`
- `test_list_models_summary_json_output_parseable`
- `test_model_info_json_output_parseable`
- `test_model_summary_json_output_parseable`
- `test_empty_model_list_json_is_array`

How to run tests (validation checklist):
- Format:
  - `cargo fmt --all` (no changes expected after running)
- Compile check:
  - `cargo check --all-targets --all-features` → should finish with "Finished"
- Lint:
  - `cargo clippy --all-targets --all-features -- -D warnings` → zero warnings
- Tests:
  - `cargo test --all-features` → all tests pass

Test expectations:
- JSON outputs can be parsed back into the corresponding Rust types (using `serde_json::from_str`)
- Empty model list serializes to `[]`
- Pretty-printed JSON is valid and human-readable

## Usage Examples

CLI examples:

- List models as JSON (basic info):
```bash
xzatoma models list --json
```

- List models with extended summary (JSON):
```bash
xzatoma models list --json --summary
```

- Show model info as JSON:
```bash
xzatoma models info --model gpt-4 --json
```

- Show model summary as JSON:
```bash
xzatoma models info --model gpt-4 --json --summary
```

Programmatic (Rust) example:

```rust
/// No-run example illustrating usage programmatically
///
/// ```no_run
/// use xzatoma::config::Config;
/// use xzatoma::commands::models::list_models;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = Config::load("config/config.yaml", &Default::default())?;
/// // Print JSON list of models (using configured provider)
/// list_models(&config, None, true, false).await?;
/// # Ok(())
/// # }
/// ```
```

Example of JSON output (basic ModelInfo array):

```json
[
  {
    "name": "gpt-4",
    "display_name": "GPT-4",
    "context_window": 8192,
    "capabilities": ["FunctionCalling"],
    "provider_specific": {
      "version": "2024-01"
    }
  }
]
```

Example of JSON output (ModelInfoSummary):

```json
{
  "info": {
    "name": "gpt-4",
    "display_name": "GPT-4",
    "context_window": 8192,
    "capabilities": ["FunctionCalling"],
    "provider_specific": {"version": "2024-01"}
  },
  "state": "enabled",
  "max_prompt_tokens": 6144,
  "max_completion_tokens": 2048,
  "supports_tool_calls": true,
  "supports_vision": false,
  "raw_data": { "api_field": "value" }
}
```

## Validation Results

- `cargo fmt --all` — OK
- `cargo check --all-targets --all-features` — OK
- `cargo clippy --all-targets --all-features -- -D warnings` — OK
- `cargo test --all-features` — OK

Manual checks:
- `xzatoma models list --json | jq .` — prints valid JSON and `jq` succeeds
- `xzatoma models list --json` (no models) — prints `[]`

## References

- Implementation:
  - `src/commands/models.rs` — JSON and summary branching + helper functions
  - `tests/models_json_output.rs` — integration tests verifying parseability
- Types:
  - `src/providers/base.rs` — `ModelInfo` and `ModelInfoSummary` definitions

## Notes & Future Work

- The choice to pretty-print JSON prioritizes human readability for CLI users and scripting ease (tools like `jq` still work well with pretty output).
- If compact output is desired for machine-only pipelines, add a `--compact` flag in a future phase or an environment variable to switch to a compact serializer.
- Summary output is intentionally conservative: providers may expose additional `raw_data` fields which are preserved in `ModelInfoSummary.raw_data`.
