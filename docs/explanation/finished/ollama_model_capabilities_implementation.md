# Ollama Model Capabilities Implementation

## Overview

This change fixes how XZatoma discovers and displays model capabilities and context windows for Ollama-hosted models. Previously we used simple heuristics (model name-based) to determine context window size and capabilities. Ollama's `/api/show` response contains authoritative fields (e.g. per-architecture `*.context_length` and a `capabilities` list) that we now parse and map into XZatoma's `ModelInfo` and capability flags.

Why this matters:
- Some Ollama models (e.g., granite) report very large context windows (e.g., 131072 tokens). Using default heuristics could show incorrect (too small) context windows.
- Ollama exposes a `capabilities` list (e.g., `["completion","tools"]`). We now map these to XZatoma capabilities (e.g., `FunctionCalling` for `tools`, `Completion` for `completion`) so the CLI and other features present accurate model capabilities to users.

## Components Delivered

- `src/providers/ollama.rs` (primary changes)
  - Parse `/api/show` `model_info` to extract architecture-specific `*.context_length`.
  - Parse top-level `capabilities` array and map it to `ModelCapability` flags.
  - Use `/api/show` details when listing models to provide accurate context windows and capabilities. Fall back gracefully to tag data when show fails.
  - Store useful provider-specific metadata (`capabilities`, `architecture`, `parameter_size`, `quantization_level`, `size`).

- `src/providers/base.rs`
  - Added `ModelCapability::Completion`.
  - Updated capability display & tests.

- Tests
  - New test: `test_build_model_info_from_show_response_parses_context_and_capabilities`.
  - Updated tests to include the `Completion` capability in capability display checks and maintain existing expectations.

- This docs file:
  - `docs/explanation/ollama_model_capabilities_implementation.md`

## Implementation Details

High-level approach:
1. Use `/api/tags` to enumerate models (as before).
2. For each model, call `/api/show` to obtain authoritative details. If `/api/show` fails for a model, fall back to the tags-derived heuristics.
3. Parse `model_info` and `details` from the show response:
   - Determine the architecture (via `general.architecture`) and look up `<arch>.context_length` or a key that ends with `.context_length`. Use its numeric value as the model's `context_window`.
   - Read `capabilities` (array of strings) and map them to `ModelCapability`:
     - `tools` → `ModelCapability::FunctionCalling`
     - `completion` → `ModelCapability::Completion`
     - `vision` → `ModelCapability::Vision`
     - `streaming` → `ModelCapability::Streaming`
     - `json`, `json_mode` → `ModelCapability::JsonMode`
     - `long_context` → `ModelCapability::LongContext`
   - Keep the raw `capabilities` list in `ModelInfo.provider_specific` under key `capabilities` for inspection.
4. Ensure existing family-based heuristics (e.g., adding FunctionCalling for `llama3.2`, `granite4`, etc.) still run as a fallback.

Key code excerpts (representative pseudocode):

Parsing context length:
```/dev/null/ollama.rs#L1-20
// Extract architecture and check <arch>.context_length or any key ending with '.context_length'
if let Some(obj) = show.model_info.as_object() {
    if let Some(arch) = obj.get("general.architecture").and_then(|v| v.as_str()) {
        let key = format!("{}.context_length", arch);
        if let Some(val) = obj.get(&key).and_then(|v| v.as_u64()) {
            context_window = val as usize;
        } else {
            // fallback: find any key that ends_with(".context_length")
        }
    }
}
```

Mapping capabilities:
```/dev/null/ollama.rs#L1-20
for cap in &show.capabilities {
    match cap.to_lowercase().as_str() {
        "tools" => model_info.add_capability(ModelCapability::FunctionCalling),
        "completion" => model_info.add_capability(ModelCapability::Completion),
        "vision" => model_info.add_capability(ModelCapability::Vision),
        "streaming" => model_info.add_capability(ModelCapability::Streaming),
        "json" | "json_mode" => model_info.add_capability(ModelCapability::JsonMode),
        "long_context" => model_info.add_capability(ModelCapability::LongContext),
        _ => {
            // unknown capability: preserve the raw list
        }
    }
}
```

Notes on performance:
- Fetching `/api/show` per model can be slower than a single `/api/tags` call. The implementation currently attempts to fetch details for each model (falling back to tag data when `show` fails). If necessary we can later optimize by doing concurrent calls with a configurable concurrency limit or cache the detailed responses.

## Testing

What I changed:
- Added `test_build_model_info_from_show_response_parses_context_and_capabilities` to confirm:
  - The context window is parsed from `granite.context_length` and is set to 131072.
  - The `capabilities` list `["completion","tools"]` maps to `ModelCapability::Completion` and `ModelCapability::FunctionCalling`.
  - Relevant provider metadata is stored (e.g., `capabilities` and `architecture`).

- Updated `ModelCapability` display test to include `Completion`.

Commands used to validate locally:
- Format: `cargo fmt --all` → no formatting changes (clean).
- Compile check: `cargo check --all-targets --all-features` → passed.
- Lint: `cargo clippy --all-targets --all-features -- -D warnings` → passed.
- Tests: `cargo test --all-features` → all tests passed (test suite ran successfully).

Note: Coverage (percentage) was not measured as part of this patch; if you'd like I can run a coverage tool (e.g., `cargo tarpaulin`) and add tests to reach/verify >80% coverage as required.

## Usage Examples

1. Reproduce the raw data from Ollama (example curl):
```/dev/null/commands.sh#L1-3
# Example from the Ollama endpoint:
curl http://localhost:11434/api/show -d '{"model": "granite4:latest"}'
# Look for "granite.context_length": 131072 and "capabilities": ["completion","tools"]
```

2. After the change, `xzatoma` CLI will present accurate model info:
```/dev/null/commands.sh#L1-6
# Before (incorrect): shows 4096 tokens
xzatoma models list --provider ollama

# After (fixed): shows 131072 tokens and completion/tools capabilities
xzatoma models info --model granite4:latest --provider ollama --summary
# Expected output includes:
# Context Window: 131072 tokens
# Capabilities:
#   Tool Calls: Yes
#   Full List: Completion, FunctionCalling
```

3. Developer/maintainer test flow:
```/dev/null/commands.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## References

- Example show response used in testing and reproduction:
  - `xzatoma/testdata/ollama_model_capabilities.json` contains sample `/api/show` JSON including `"granite.context_length": 131072` and `"capabilities": ["completion", "tools"]`.

## Future Improvements / Follow-ups

- Improve throughput by fetching `/api/show` responses concurrently with a bounded worker pool.
- Cache detailed `/api/show` responses for a short TTL (e.g., 5 minutes) to avoid repeated detailed fetches for the listing flow.
- If Ollama introduces richer fields in `/api/tags`, consider using them directly to avoid extra `show` requests.
- Add more tests covering additional architectures (llama3.x, mistral, etc.) and edge cases (missing keys, non-numeric context_length).

---

If you'd like, I can:
- Add an additional test that verifies `xzatoma models list` output formatting matches an expected table for the sample testdata.
- Add a small concurrency improvement to the per-model detail fetch (bounded parallelism) and include a test measuring performance.
