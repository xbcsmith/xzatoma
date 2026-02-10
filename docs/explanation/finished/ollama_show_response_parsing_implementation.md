# Ollama Show Response Parsing Implementation

## Overview

This document describes the change that makes the Ollama provider tolerant to variations in the `/api/show` response shape, particularly when the top-level `name` field is missing. Previously, a missing `name` field caused a deserialization failure such as:

```
Error: Failed to parse Ollama show response: error decoding response body: missing field `name` at line 1 column ...
```

We made the parsing robust by:
- Treating `name` as optional in the response type,
- Reading the response body as text and deserializing from that text (so we can inspect and log the raw body if needed),
- Falling back to the requested model name when the response does not include `name`,
- Prefer `details.family` (when present) to determine model family/capabilities,
- Adding unit tests to cover these scenarios.

## Components Delivered

- `src/providers/ollama.rs` (modified)
  - `OllamaShowResponse` now allows `name` to be optional.
  - `fetch_model_details` now reads response text and falls back to the requested model name on missing `name`.
  - Added helper: `build_model_info_from_show_response(...)` for isolated, testable logic.
  - Added tests:
    - `test_parse_show_response_missing_name`
    - `test_build_model_info_from_show_response_missing_name`
- `docs/explanation/ollama_show_response_parsing_implementation.md` (this document)

## Implementation Details

Root cause:
- Some Ollama server responses do not include a `name` field at the top level of the `/api/show` response.
- The prior code did direct deserialization into a struct with a required `name` field which produced an error when the field was absent.

Key changes:
1. Make `name` optional on the show-response struct:
   - `OllamaShowResponse.name` is now `Option<String>` (with serde default behavior).
2. Read the response body as text first:
   - This supports better diagnostics and avoids consuming the response implicitly during a failing `.json().await` call.
3. Fallback behavior:
   - If the response `name` is missing, fall back to the `model_name` that was requested.
   - When available, prefer `details.family` to determine the model family (used to add capabilities).
4. Logging:
   - When `name` is absent, we log a debug-level message noting the fallback.
   - When the response body can't be read or deserialized, we log an error (as before) and propagate an appropriate provider error.

Example (core logic excerpt):
```xzatoma/src/providers/ollama.rs#L440-488
// Read the response body as text first so we can handle varying response shapes
let body = response.text().await.map_err(|e| {
    tracing::error!("Failed to read Ollama show response body: {}", e);
    XzatomaError::Provider(format!("Failed to read model details: {}", e))
})?;

let show_response: OllamaShowResponse = serde_json::from_str(&body).map_err(|e| {
    tracing::error!("Failed to parse Ollama show response: {}", e);
    XzatomaError::Provider(format!("Failed to parse model details: {}", e))
})?;

// Use the name from the response when present; otherwise fall back to the requested model name
let name = show_response.name.clone().unwrap_or_else(|| model_name.to_string());

if show_response.name.is_none() {
    tracing::debug!(
        "Ollama show response missing 'name' field, falling back to requested model name: {}",
        name
    );
}

let mut model_info = ModelInfo::new(&name, &name, get_context_window_for_model(&name));

// Prefer the family reported in details if present, otherwise derive from the name
let family = if !show_response.details.family.is_empty() {
    show_response.details.family.clone()
} else {
    name.split(':').next().unwrap_or(&name).to_string()
};
add_model_capabilities(&mut model_info, &family);
```

Testable helper:
```/dev/null/ollama_show_tests.rs#L1-40
// build_model_info_from_show_response(show: &OllamaShowResponse, requested_name: &str) -> ModelInfo
// - Encapsulates fallback and family extraction logic
// - Covered by unit tests (see Testing section)
```

## Testing

Unit tests added to `src/providers/ollama.rs` (under the `mod tests` area):

- `test_parse_show_response_missing_name`
  - Ensures that deserializing a show response JSON that does not have `name` succeeds and `name` is `None`.
- `test_build_model_info_from_show_response_missing_name`
  - Ensures that building `ModelInfo` from a show response without `name` uses the provided requested model name and sets capabilities appropriately (e.g., `granite4` supports function calling).

How to run locally:

- Format:
  - `cargo fmt --all` (should not change files)
- Compile check:
  - `cargo check --all-targets --all-features`
- Lint (treat warnings as errors):
  - `cargo clippy --all-targets --all-features -- -D warnings`
- Tests:
  - `cargo test --all-features`
  - Or run the specific tests:
    - `cargo test test_parse_show_response_missing_name`
    - `cargo test test_build_model_info_from_show_response_missing_name`

Validation results (local):
- `cargo fmt --all`: no diffs
- `cargo check --all-targets --all-features`: passes
- `cargo clippy --all-targets --all-features -- -D warnings`: passes (no warnings)
- `cargo test --all-features`: passes (all tests; new tests included)

## Usage Examples

Example input: a show response that does not include a `name` field:

```/dev/null/ollama_show_response_example.json#L1-20
{
  "model_info": {
    "description": "A model whose show response omits a top-level name"
  },
  "details": {
    "family": "granite4",
    "parameter_size": "placeholder",
    "quantization_level": "placeholder"
  }
}
```

Behavior after this change:
- The provider no longer fails to parse the response.
- The provider uses the requested model name as the model `name`.
- If `details.family` contains a known family (e.g., `granite4`) the model's capabilities (such as function-calling) are set accordingly.

## Notes & Future Work

- This solution is intentionally conservative: malformed JSON bodies still cause a parse error; we only relax the requirement for the top-level `name` field.
- We may consider adding additional heuristics in the future to extract `name` from nested fields (if there are consistent alternative fields in future Ollama releases).
- Integration-level tests that hit a running Ollama instance (or simulate it with a mock HTTP server) could further validate the end-to-end behavior.

## References

- Modified code: `src/providers/ollama.rs`
- Tests added in same module (see test names above)
- Observed log that motivated this change:
  - `Failed to parse Ollama show response: error decoding response body: missing field 'name' ...`

---

If you want, I can:
- Add an integration test using a local HTTP mock to simulate various `/api/show` shapes (recommended for regression protection).
- Expand heuristics to extract `name` from other potential response fields if you have sample responses from other Ollama versions.
