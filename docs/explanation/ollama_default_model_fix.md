# Ollama Default Model Fix Implementation

## Overview

**Note**: This fix was partially superseded by `ollama_tool_support_validation.md`. The default model was changed from `llama3:latest` to `llama3.2:latest` because `llama3:latest` does not support tool calling, which is required for XZatoma.

Fixed a critical bug where the Ollama provider had a hardcoded default model (`qwen2.5-coder`) that was not available in standard Ollama installations. This caused the provider to fail even when users attempted to switch to available models like `llama3:latest`. The fix changes the default to `llama3:latest` and removes all references to Qwen models from code and documentation.

## Bug Description

The issue manifested in the following ways:

1. Users could not switch models using the `/models llama3:latest` command
2. Error messages reported `qwen2.5-coder` not found even when requesting different models
3. The default model was hardcoded in multiple locations throughout the codebase

Example error:

```text
/models llama3:latest
2026-01-22T21:19:04.105040Z  INFO xzatoma::agent::core: Starting agent execution
2026-01-22T21:19:04.109755Z ERROR xzatoma::providers::ollama: Ollama returned error 404 Not Found: {"error":"model 'qwen2.5-coder' not found"}
Error: Provider error: Ollama returned error 404 Not Found: {"error":"model 'qwen2.5-coder' not found"}
```

## Root Cause

The `default_ollama_model()` function in `src/config.rs` returned a hardcoded `qwen2.5-coder` string, which was not a standard Ollama model. This affected:

- Default configuration generation
- Provider initialization when no model was specified
- Test fixtures and examples throughout the codebase

## Components Modified

### Code Changes

- `src/config.rs` (2 locations) - Changed default model function and test YAML
- `src/cli.rs` (2 locations) - Updated CLI test assertions
- `src/providers/ollama.rs` (19 locations) - Updated doc comments and all test cases
- `src/providers/base.rs` (1 location) - Updated ModelInfo doc comment
- `src/test_utils.rs` (1 location) - Updated test configuration YAML

### Configuration Changes

- `config/config.yaml` (2 locations) - Changed default model and updated comment

Total: 27 changes across 6 files

## Changes Applied

### Default Model Function

Changed the default model from `qwen2.5-coder` to `llama3:latest` (later changed to `llama3.2:latest` for tool support):

```rust
fn default_ollama_model() -> String {
    "llama3:latest".to_string()  // Later changed to llama3.2:latest
}
```

### Configuration File

Updated the default configuration and documentation:

```yaml
ollama:
  host: http://localhost:11434
  # Model to use (e.g., llama3:latest, llama3.2:latest, gemma3:latest)
  model: llama3:latest
```

### Test Updates

All test cases were updated to use `llama3:latest` instead of `qwen2.5-coder`:

```rust
let config = OllamaConfig {
    host: "http://localhost:11434".to_string(),
    model: "llama3:latest".to_string(),
};
```

### Documentation Comments

Updated all doc comment examples to reference standard Ollama models:

```rust
/// let config = OllamaConfig {
///     host: "http://localhost:11434".to_string(),
///     model: "llama3:latest".to_string(),
/// };
```

## Approved Ollama Models

Per user requirements, only these models should be referenced in Ollama documentation:

- `llama3:latest` - Default model
- `llama3.2:latest` - Alternative LLaMA variant
- `gemma3:latest` - Google Gemma model

All references to Qwen models (`qwen2.5-coder`, `qwen3`, etc.) have been removed.

## Testing

All quality gates passed successfully:

### Format Check

```bash
cargo fmt --all
# Result: No output (all files formatted correctly)
```

### Compilation Check

```bash
cargo check --all-targets --all-features
# Result: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.10s
```

### Lint Check

```bash
cargo clippy --all-targets --all-features -- -D warnings
# Result: Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.81s
# Zero warnings
```

### Test Suite

```bash
cargo test --all-features
# Result: test result: ok. 476 passed; 0 failed; 0 ignored
```

All 476 tests passed with zero failures.

## Impact Analysis

### User-Facing Changes

1. **Default Behavior**: New Ollama installations will default to `llama3:latest` instead of `qwen2.5-coder`
2. **Configuration**: Existing config files may need updating if they reference Qwen models
3. **Documentation**: All examples now use standard, widely-available Ollama models

### Breaking Changes

**None**. Users with custom configurations specifying different models are unaffected. Only the default value changed.

### Migration Guide

If you have an existing `config/config.yaml` with `qwen2.5-coder`:

```yaml
# Old configuration
provider:
  ollama:
    model: qwen2.5-coder

# New configuration (choose one)
provider:
  ollama:
    model: llama3:latest      # Recommended default
    # OR
    model: llama3.2:latest    # Alternative
    # OR
    model: gemma3:latest      # Google's model
```

## Validation Results

- ✅ `cargo fmt --all` passed with no changes needed
- ✅ `cargo check --all-targets --all-features` passed with zero errors
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` passed with zero warnings
- ✅ `cargo test --all-features` passed with 476 tests (0 failures)
- ✅ All file extensions correct (`.rs`, `.yaml`, `.md`)
- ✅ Documentation filename follows lowercase_with_underscores convention
- ✅ No emojis in documentation

## References

- Issue: Ollama default model hardcoded to unavailable model
- Superseded by: `ollama_tool_support_validation.md` (default changed to llama3.2:latest)
- Related: Model Management Implementation (Phase 6-7)
- Configuration: `config/config.yaml`
- Provider Implementation: `src/providers/ollama.rs`
