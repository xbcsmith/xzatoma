# Ollama Tool Support Validation Implementation

## Overview

Implemented proper tool support detection and validation for Ollama models to prevent runtime errors when switching to models that do not support function calling. This fix ensures XZatoma only operates with models that have tool/function calling capabilities, which are required for the agent's autonomous operation.

## Problem Description

The original implementation incorrectly assumed all Ollama models support tool calling (function calling). This caused several issues:

1. **Default model lacked tool support**: The initial default (`llama3:latest`) does not support tools
2. **Silent failures**: Users could switch to models without tool support, causing cryptic runtime errors
3. **Misleading capability reporting**: All models were marked as supporting `FunctionCalling` regardless of actual capabilities
4. **Poor user experience**: No guidance on which models work with XZatoma

### User-Reported Issue

```text
User attempted to switch models but encountered errors because llama3:latest
does not support tools. User had to manually discover that llama3.2:latest
was needed for tool support.
```

## Root Cause

The `add_model_capabilities()` function unconditionally added `FunctionCalling` capability:

```rust
// INCORRECT - Old implementation
fn add_model_capabilities(model: &mut ModelInfo, family: &str) {
  // Most Ollama models support function calling
  model.add_capability(ModelCapability::FunctionCalling); // WRONG!

  // ... other capabilities
}
```

This was based on an incorrect assumption. In reality, only specific newer models support tool calling.

## Solution Implemented

### 1. Updated Default Model

Changed default from `llama3:latest` to `llama3.2:latest`:

```rust
fn default_ollama_model() -> String {
  "llama3.2:latest".to_string()
}
```

**Rationale**: `llama3.2:latest` is the most widely available Ollama model that supports tool calling.

### 2. Accurate Capability Detection

Rewrote `add_model_capabilities()` to only mark models that actually support tools:

```rust
fn add_model_capabilities(model: &mut ModelInfo, family: &str) {
  // Only specific Ollama models support function calling (tool use)
  // Based on Ollama documentation and testing
  match family.to_lowercase().as_str() {
    // Models that support tool calling
    "llama3.2" | "llama3.3" | "mistral" | "mistral-nemo" | "firefunction"
    | "command-r" | "command-r-plus" => {
      model.add_capability(ModelCapability::FunctionCalling);
    }
    _ => {
      // Most other models do NOT support tool calling
      // Including: llama3, llama2, gemma, qwen, codellama, etc.
    }
  }

  // Add other capabilities based on model family
  match family.to_lowercase().as_str() {
    "mistral" | "mistral-nemo" | "neural-chat" => {
      model.add_capability(ModelCapability::LongContext);
    }
    "llava" => {
      model.add_capability(ModelCapability::Vision);
    }
    _ => {}
  }
}
```

**Models that support tool calling**:
- `llama3.2:*` - Default, widely available
- `llama3.3:*` - Newer version with tool support
- `mistral:*` - Mistral AI models
- `mistral-nemo:*` - Compact Mistral variant
- `firefunction:*` - Specialized function calling model
- `command-r:*` - Cohere Command R series
- `command-r-plus:*` - Cohere Command R Plus

**Models that do NOT support tool calling**:
- `llama3:*` - Original Llama 3 (no tools)
- `llama2:*` - Llama 2 series
- `gemma:*` - Google Gemma series
- `codellama:*` - Code-focused, no tools
- `llava:*` - Vision-focused, no tools
- Most other models

### 3. Model Switch Validation

Added validation in `set_model()` to reject models without tool support:

```rust
async fn set_model(&mut self, model_name: String) -> Result<()> {
  // Validate that the model exists by fetching the list
  let models = self.list_models().await?;

  let model_info = models.iter().find(|m| m.name == model_name);

  if model_info.is_none() {
    return Err(XzatomaError::Provider(
      format!("Model not found: {}", model_name)
    ).into());
  }

  // Check if the model supports tool calling (required for XZatoma)
  let model = model_info.unwrap();
  if !model.supports_capability(ModelCapability::FunctionCalling) {
    return Err(XzatomaError::Provider(format!(
      "Model '{}' does not support tool calling. XZatoma requires models with tool/function calling support. Try llama3.2:latest, llama3.3:latest, or mistral:latest instead.",
      model_name
    )).into());
  }

  // Update the model in the config
  let mut config = self.config.write().map_err(|_| {
    XzatomaError::Provider("Failed to acquire write lock on config".to_string())
  })?;
  config.model = model_name.clone();
  drop(config);

  // Invalidate cache to ensure fresh model list next time
  self.invalidate_cache();

  tracing::info!("Switched Ollama model to: {}", model_name);
  Ok(())
}
```

**Error message provides clear guidance**:
```text
Model 'llama3:latest' does not support tool calling. XZatoma requires models
with tool/function calling support. Try llama3.2:latest, llama3.3:latest, or
mistral:latest instead.
```

### 4. Enhanced Model Listing

The existing model listing commands already display capabilities, so users can now see which models support `FunctionCalling`:

```text
/models list

+------------------+----------------------+-----------------+--------------------+
| Model Name    | Display Name     | Context Window | Capabilities    |
+------------------+----------------------+-----------------+--------------------+
| llama3.2:latest | llama3.2 (4.7GB)   | 4096 tokens   | FunctionCalling  |
| llama3:latest  | llama3 (4.7GB)    | 4096 tokens   |          |
| mistral:latest  | mistral (4.1GB)   | 8192 tokens   | FunctionCalling,  |
|         |           |         | LongContext    |
+------------------+----------------------+-----------------+--------------------+
```

Models without `FunctionCalling` capability cannot be used with XZatoma.

## Components Modified

### Code Changes

- `src/providers/ollama.rs` (3 functions modified, 3 tests updated)
 - `add_model_capabilities()` - Accurate tool support detection
 - `set_model()` - Validation before allowing switch
 - Tests updated to reflect new behavior

### Configuration Changes

- `config/config.yaml` - Default model changed to `llama3.2:latest`
- `src/config.rs` - Default function returns `llama3.2:latest`

Total: 5 changes across 2 files

## Testing

### Unit Tests Updated

Three test functions were updated to reflect the new capability logic:

1. `test_add_model_capabilities_function_calling` - Now tests both positive (llama3.2) and negative (llama3) cases
2. `test_add_model_capabilities_long_context` - Tests mistral and mistral-nemo
3. `test_add_model_capabilities_vision` - Now correctly expects llava to NOT have function calling

```rust
#[test]
fn test_add_model_capabilities_function_calling() {
  // Test model that supports function calling
  let mut model = ModelInfo::new("llama3.2", "Llama 3.2", 4096);
  add_model_capabilities(&mut model, "llama3.2");
  assert!(model.supports_capability(ModelCapability::FunctionCalling));

  // Test model that does NOT support function calling
  let mut model_no_tools = ModelInfo::new("llama3", "Llama 3", 4096);
  add_model_capabilities(&mut model_no_tools, "llama3");
  assert!(!model_no_tools.supports_capability(ModelCapability::FunctionCalling));
}
```

### Test Results

All quality gates passed:

```bash
cargo test --all-features
# Result: test result: ok. 476 passed; 0 failed; 0 ignored
```

Specific Ollama provider tests:
```bash
cargo test --lib providers::ollama::tests
# Result: test result: ok. 18 passed; 0 failed; 0 ignored
```

### Manual Testing Scenarios

#### Scenario 1: Valid Model Switch
```bash
/model llama3.2:latest
# Expected: Success - model supports tools
# Result: Switched to llama3.2:latest
```

#### Scenario 2: Invalid Model Switch
```bash
/model llama3:latest
# Expected: Error with helpful message
# Result: Error: Model 'llama3:latest' does not support tool calling...
```

#### Scenario 3: List Models Shows Capabilities
```bash
/models list
# Expected: FunctionCalling shown for supported models only
# Result: Capabilities column accurately reflects support
```

## User Impact

### Positive Changes

1. **Default works out-of-box**: New installations use `llama3.2:latest` which supports tools
2. **Clear error messages**: Users attempting to switch to unsupported models get helpful guidance
3. **Accurate capability reporting**: Model listings show which models work with XZatoma
4. **Prevents runtime failures**: Validation happens at switch-time, not during agent execution

### Breaking Changes

**None for existing users**. The validation only affects new model switches. Existing configurations with custom models are unaffected unless the user attempts to switch to an unsupported model.

### Migration Guide

If you have `llama3:latest` in your config:

```yaml
# Old (will not work)
provider:
 ollama:
  model: llama3:latest

# New (recommended)
provider:
 ollama:
  model: llama3.2:latest # Default choice
```

Other supported alternatives:
```yaml
model: llama3.3:latest   # Newer version
model: mistral:latest   # Supports long context too
model: command-r:latest  # Cohere's function-calling model
```

## Error Handling

### Before This Fix

Attempting to use a model without tool support would fail during agent execution:

```text
Error: Ollama returned error 404: Tool calling not supported for this model
```

This error occurred after the agent had already started, potentially after multiple conversation turns.

### After This Fix

The error occurs immediately when attempting to switch:

```text
Error: Model 'llama3:latest' does not support tool calling. XZatoma requires
models with tool/function calling support. Try llama3.2:latest, llama3.3:latest,
or mistral:latest instead.
```

This provides:
- **Immediate feedback** - Fail fast at configuration time
- **Clear explanation** - User understands why the switch failed
- **Actionable guidance** - Suggests specific models that will work

## Design Decisions

### Why Whitelist vs. Blacklist?

We chose to whitelist models that support tools rather than blacklist unsupported models because:

1. **Safer default**: New models default to no tool support until explicitly verified
2. **Explicit intent**: Forces maintainers to research and confirm support
3. **Documentation**: The whitelist serves as living documentation of supported models
4. **Future-proof**: New Ollama models won't accidentally be marked as supporting tools

### Why Not Query Ollama API for Tool Support?

The Ollama `/api/show` endpoint does not currently expose whether a model supports tool calling. We must determine this from:

1. Model family name
2. Ollama documentation
3. User testing and feedback

This is why we maintain a curated list in the code.

### Why Block Unsupported Models?

XZatoma is fundamentally built on tool calling. Without it:
- Agent cannot execute terminal commands
- Agent cannot read/write files
- Agent cannot perform any actions

Allowing models without tool support would result in a broken, non-functional agent. Better to fail early with a clear message.

## Validation Results

- `cargo fmt --all` passed with no changes needed
- `cargo check --all-targets --all-features` passed with zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` passed with zero warnings
- `cargo test --all-features` passed with 476 tests (0 failures)
- All file extensions correct (`.rs`, `.yaml`, `.md`)
- Documentation filename follows lowercase_with_underscores convention
- No emojis in documentation

## Future Enhancements

### Potential Improvements

1. **Dynamic capability detection**: If Ollama adds capability metadata to their API, query it instead of hardcoding
2. **Model recommendation system**: Suggest best model based on task type (code vs. chat vs. long context)
3. **Fallback mechanism**: Allow users to override validation with warning (for testing new models)
4. **Capability testing**: Add integration test that verifies tool support by making actual API calls

### Documentation Improvements

1. Add troubleshooting section to README about model compatibility
2. Create FAQ entry about "Model not found" vs. "Model doesn't support tools" errors
3. Document process for adding new models to the whitelist as Ollama releases updates

## References

- Issue: Ollama default model lacked tool support
- Related: Ollama Default Model Fix (`ollama_default_model_fix.md`)
- Related: Model Management Implementation (Phase 6-7)
- Ollama Documentation: https://github.com/ollama/ollama/blob/main/docs/api.md
- Provider Implementation: `src/providers/ollama.rs`
- Configuration: `config/config.yaml`
