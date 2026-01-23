# Model Management API Reference

## Overview

This document provides a comprehensive API reference for XZatoma's model management capabilities. The model management system allows users to discover available AI models, inspect model details, switch between models, and monitor context window usage.

## Core Types

### ModelInfo

Represents detailed information about an AI model.

```rust
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub context_window: usize,
    pub capabilities: Vec<ModelCapability>,
    pub provider_specific: HashMap<String, String>,
}
```

**Fields:**

- `name`: Unique identifier for the model (e.g., "gpt-4o", "qwen2.5-coder:7b")
- `display_name`: Human-readable name for UI display (e.g., "GPT-4o", "Qwen 2.5 Coder 7B")
- `context_window`: Maximum number of tokens the model can handle
- `capabilities`: List of supported features (function calling, vision, etc.)
- `provider_specific`: Additional provider-specific metadata

**Methods:**

```rust
pub fn new(
    name: impl Into<String>,
    display_name: impl Into<String>,
    context_window: usize,
) -> Self

pub fn add_capability(&mut self, capability: ModelCapability)

pub fn supports_capability(&self, capability: ModelCapability) -> bool

pub fn set_provider_metadata(&mut self, key: impl Into<String>, value: impl Into<String>)
```

### ModelCapability

Enumeration of model capabilities.

```rust
pub enum ModelCapability {
    LongContext,
    FunctionCalling,
    Vision,
    Streaming,
    JsonMode,
}
```

**Variants:**

- `LongContext`: Model supports extended context windows (typically 100k+ tokens)
- `FunctionCalling`: Model supports tool/function calling
- `Vision`: Model can process and understand images
- `Streaming`: Model supports streaming responses
- `JsonMode`: Model can output structured JSON

### TokenUsage

Tracks token consumption for a request or session.

```rust
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}
```

**Fields:**

- `prompt_tokens`: Number of tokens in the input prompt
- `completion_tokens`: Number of tokens in the generated response
- `total_tokens`: Sum of prompt and completion tokens

**Methods:**

```rust
pub fn new(prompt_tokens: usize, completion_tokens: usize) -> Self
```

### ContextInfo

Provides context window utilization metrics.

```rust
pub struct ContextInfo {
    pub max_tokens: usize,
    pub used_tokens: usize,
    pub remaining_tokens: usize,
    pub percentage_used: f64,
}
```

**Fields:**

- `max_tokens`: Maximum tokens available in the context window
- `used_tokens`: Tokens currently consumed by the conversation
- `remaining_tokens`: Tokens available before hitting the limit
- `percentage_used`: Percentage of context window used (0.0-100.0)

**Methods:**

```rust
pub fn new(max_tokens: usize, used_tokens: usize) -> Self
```

**Overflow Protection:**

The constructor automatically clamps `used_tokens` to `max_tokens` to prevent underflow in `remaining_tokens` calculation.

### ProviderCapabilities

Describes what model management features a provider supports.

```rust
pub struct ProviderCapabilities {
    pub supports_model_listing: bool,
    pub supports_model_details: bool,
    pub supports_model_switching: bool,
    pub supports_token_counts: bool,
    pub supports_streaming: bool,
}
```

**Fields:**

- `supports_model_listing`: Provider can list available models
- `supports_model_details`: Provider can return detailed model information
- `supports_model_switching`: Provider allows changing the active model
- `supports_token_counts`: Provider reports accurate token usage
- `supports_streaming`: Provider supports streaming completions

## Provider Trait Methods

### list_models

Lists all available models from the provider.

```rust
async fn list_models(&self) -> Result<Vec<ModelInfo>>
```

**Returns:**

- `Ok(Vec<ModelInfo>)`: List of available models
- `Err`: Provider doesn't support model listing or API call failed

**Example:**

```rust
let models = provider.list_models().await?;
for model in models {
    println!("{}: {} tokens", model.display_name, model.context_window);
}
```

**Default Implementation:**

Returns an error indicating model listing is not supported.

### get_model_info

Retrieves detailed information about a specific model.

```rust
async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo>
```

**Arguments:**

- `model_name`: Name or identifier of the model

**Returns:**

- `Ok(ModelInfo)`: Detailed model information
- `Err`: Model not found or provider doesn't support this feature

**Example:**

```rust
let model = provider.get_model_info("gpt-4o").await?;
println!("Context: {} tokens", model.context_window);
println!("Supports vision: {}", model.supports_capability(ModelCapability::Vision));
```

**Default Implementation:**

Returns an error indicating detailed model information is not supported.

### get_current_model

Returns the name of the currently active model.

```rust
fn get_current_model(&self) -> Result<String>
```

**Returns:**

- `Ok(String)`: Name of the active model
- `Err`: Current model information unavailable

**Example:**

```rust
let current = provider.get_current_model()?;
println!("Using model: {}", current);
```

**Default Implementation:**

Returns an error indicating current model information is unavailable.

### set_model

Changes the active model for subsequent completions.

```rust
async fn set_model(&mut self, model_name: String) -> Result<()>
```

**Arguments:**

- `model_name`: Name of the model to switch to

**Returns:**

- `Ok(())`: Model switched successfully
- `Err`: Model not found or switching not supported

**Example:**

```rust
provider.set_model("gpt-4-turbo".to_string()).await?;
```

**Validation:**

Most implementations validate that the model exists before switching.

**Default Implementation:**

Returns an error indicating model switching is not supported.

### get_provider_capabilities

Returns the capabilities supported by this provider.

```rust
fn get_provider_capabilities(&self) -> ProviderCapabilities
```

**Returns:**

A `ProviderCapabilities` struct describing supported features.

**Example:**

```rust
let caps = provider.get_provider_capabilities();
if caps.supports_model_switching {
    provider.set_model("new-model".to_string()).await?;
}
```

**Default Implementation:**

Returns all capabilities set to `false`.

## Agent Methods

### get_token_usage

Returns accumulated token usage across all agent executions.

```rust
pub fn get_token_usage(&self) -> Option<TokenUsage>
```

**Returns:**

- `Some(TokenUsage)`: Accumulated tokens if provider reports usage
- `None`: No token usage information available

**Example:**

```rust
if let Some(usage) = agent.get_token_usage() {
    println!("Total tokens used: {}", usage.total_tokens);
}
```

### get_context_info

Returns context window utilization metrics.

```rust
pub fn get_context_info(&self, model_context_window: usize) -> ContextInfo
```

**Arguments:**

- `model_context_window`: Maximum context window size for the current model

**Returns:**

A `ContextInfo` struct with usage metrics.

**Example:**

```rust
let context = agent.get_context_info(8192);
println!("{:.1}% of context used", context.percentage_used);

if context.percentage_used > 80.0 {
    println!("Warning: Approaching context limit!");
}
```

**Token Source Priority:**

1. Provider-reported token counts (most accurate)
2. Conversation heuristic (characters / 4)

## CLI Commands

### models list

Lists all available models from the configured provider.

```bash
xzatoma models list [--provider <provider>]
```

**Options:**

- `--provider`, `-p`: Override configured provider (copilot, ollama)

**Example:**

```bash
xzatoma models list
xzatoma models list --provider ollama
```

**Output:**

Displays a table with model name, display name, context window, and capabilities.

### models info

Shows detailed information about a specific model.

```bash
xzatoma models info --model <name> [--provider <provider>]
```

**Options:**

- `--model`, `-m`: Model name or identifier (required)
- `--provider`, `-p`: Override configured provider (copilot, ollama)

**Example:**

```bash
xzatoma models info --model gpt-4o
xzatoma models info --model qwen2.5-coder:7b --provider ollama
```

**Output:**

Displays model name, display name, context window, capabilities, and provider-specific metadata.

### models current

Shows the currently active model.

```bash
xzatoma models current [--provider <provider>]
```

**Options:**

- `--provider`, `-p`: Override configured provider (copilot, ollama)

**Example:**

```bash
xzatoma models current
```

**Output:**

Displays the provider name and currently active model.

## Chat Mode Special Commands

### /models list

Lists available models during an interactive chat session.

```
/models list
```

**Output:**

Table of available models with the current model highlighted in green.

### /model <name>

Switches to a different model during chat.

```
/model <model-name>
```

**Example:**

```
/model gpt-4-turbo
/model qwen2.5-coder:14b
```

**Behavior:**

- Validates that the model exists
- Warns if current conversation exceeds new model's context window
- Preserves conversation history (with automatic pruning if needed)
- Creates a new agent with the updated provider

### /context

Displays context window information.

```
/context
```

**Output:**

Shows current model, context window size, tokens used, remaining tokens, usage percentage, and color-coded usage level.

## Provider-Specific Details

### GitHub Copilot

**Supported Models:**

- gpt-4o (128k context)
- gpt-4-turbo (128k context)
- gpt-4 (8k context)
- o1-preview (128k context)
- o1-mini (128k context)
- claude-3.5-sonnet (200k context)
- gemini-2.0-flash-exp (1M context)

**Capabilities:**

- Model listing: Yes (hardcoded list)
- Model details: Yes (from hardcoded metadata)
- Model switching: Yes (updates config)
- Token counts: No (not provided by Copilot API)
- Streaming: No (not currently implemented)

**Configuration:**

```yaml
provider:
  provider_type: copilot
  copilot:
    model: gpt-4o
```

### Ollama

**Supported Models:**

All models available on the local Ollama instance.

**Capabilities:**

- Model listing: Yes (queries Ollama API)
- Model details: Yes (queries Ollama API with full metadata)
- Model switching: Yes (updates config)
- Token counts: Yes (Ollama reports prompt and completion tokens)
- Streaming: No (not currently implemented)

**Configuration:**

```yaml
provider:
  provider_type: ollama
  ollama:
    host: http://localhost:11434
    model: qwen2.5-coder:7b
```

**Dynamic Discovery:**

Ollama queries the running Ollama instance to discover available models dynamically.

## Error Handling

### Common Errors

**Model Not Found:**

```rust
Err(XzatomaError::Provider("Model not found: invalid-model".to_string()))
```

**Feature Not Supported:**

```rust
Err(XzatomaError::Provider("Model listing is not supported by this provider".to_string()))
```

**API Connection Failed:**

```rust
Err(XzatomaError::Provider("Failed to connect to Ollama: connection refused".to_string()))
```

### Error Recovery

- Validate model existence before switching
- Check provider capabilities before calling feature-specific methods
- Provide fallback behavior for providers without token counting

## Best Practices

### Model Selection

Choose models based on task requirements:

- Large context tasks: Use models with 100k+ context windows
- Function calling: Ensure model supports `FunctionCalling` capability
- Cost-sensitive: Prefer smaller models for simple tasks

### Context Management

Monitor context window usage:

```rust
let context = agent.get_context_info(model_context_window);

if context.percentage_used > 80.0 {
    // Consider switching to a model with larger context
    // Or prune conversation history
}
```

### Token Tracking

Use provider-reported tokens when available:

```rust
if let Some(usage) = agent.get_token_usage() {
    // Accurate counts from provider
    println!("Exact tokens: {}", usage.total_tokens);
} else {
    // Fall back to heuristic
    let context = agent.get_context_info(max_tokens);
    println!("Estimated tokens: {}", context.used_tokens);
}
```

### Model Switching

Preserve conversation when switching models:

1. Check new model's context window
2. Warn if current conversation exceeds new limit
3. Use agent reconstruction pattern to switch cleanly
4. Conversation pruning happens automatically if needed

## Integration Examples

### Programmatic Model Management

```rust
use xzatoma::providers::{create_provider, Provider};
use xzatoma::config::Config;

let config = Config::load("config/config.yaml", &Default::default())?;
let mut provider = create_provider("ollama", &config.provider)?;

// List available models
let models = provider.list_models().await?;
for model in models {
    println!("{}: {} tokens", model.name, model.context_window);
}

// Get current model
let current = provider.get_current_model()?;
println!("Current: {}", current);

// Switch model
provider.set_model("qwen2.5-coder:14b".to_string()).await?;

// Get detailed info
let info = provider.get_model_info("qwen2.5-coder:14b").await?;
println!("Context: {} tokens", info.context_window);
```

### Context Monitoring in Agent Loop

```rust
use xzatoma::agent::Agent;

let agent = Agent::new(provider, tools, config)?;

loop {
    let result = agent.execute(&prompt).await?;

    let context = agent.get_context_info(8192);
    println!("Context usage: {:.1}%", context.percentage_used);

    if context.percentage_used > 90.0 {
        eprintln!("Warning: Context nearly full!");
        break;
    }
}
```

## See Also

- How-To Guide: Managing Models (docs/how-to/manage_models.md)
- How-To Guide: Switching Models (docs/how-to/switch_models.md)
- Architecture: Model Management (docs/explanation/model_management_implementation_plan.md)
- Phase 4 Implementation: Agent Integration (docs/explanation/phase4_agent_integration_implementation.md)
- Phase 6 Implementation: Chat Mode Model Management (docs/explanation/phase6_chat_mode_model_management_implementation.md)
