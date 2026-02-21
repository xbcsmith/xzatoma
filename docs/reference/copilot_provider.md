# Copilot Provider API Reference

## Overview

The Copilot provider implements the GitHub Copilot API with support for both `/chat/completions` and `/responses` endpoints. It automatically selects the best endpoint based on model capabilities and provides streaming support for improved performance.

## Configuration

### CopilotConfig

Configuration structure for Copilot provider.

```rust
pub struct CopilotConfig {
    pub model: String,
    pub api_base: Option<String>,
    pub enable_streaming: bool,
    pub enable_endpoint_fallback: bool,
    pub reasoning_effort: Option<String>,
    pub include_reasoning: bool,
}
```

#### Fields

| Field                      | Type             | Default        | Description                                     |
| -------------------------- | ---------------- | -------------- | ----------------------------------------------- |
| `model`                    | `String`         | `"gpt-5-mini"` | Model identifier to use                         |
| `api_base`                 | `Option<String>` | `None`         | Custom API base URL (for testing)               |
| `enable_streaming`         | `bool`           | `true`         | Enable SSE streaming for responses              |
| `enable_endpoint_fallback` | `bool`           | `true`         | Auto-fallback to completions if responses fails |
| `reasoning_effort`         | `Option<String>` | `None`         | Reasoning effort: "low", "medium", "high"       |
| `include_reasoning`        | `bool`           | `false`        | Include reasoning output in responses           |

### Environment Variables

```bash
export XZATOMA_COPILOT_MODEL=gpt-5-mini
export XZATOMA_COPILOT_ENABLE_STREAMING=true
export XZATOMA_COPILOT_ENABLE_ENDPOINT_FALLBACK=true
export XZATOMA_COPILOT_REASONING_EFFORT=medium
export XZATOMA_COPILOT_INCLUDE_REASONING=false
```

### YAML Configuration

```yaml
copilot:
  model: "gpt-5-mini"
  enable_streaming: true
  enable_endpoint_fallback: true
  reasoning_effort: "medium"
  include_reasoning: false
```

## Provider Methods

### complete()

Complete a chat conversation with optional tool calling.

```rust
async fn complete(
    &self,
    messages: &[Message],
    tools: &[Tool],
) -> Result<CompletionResponse>
```

#### Parameters

- `messages` - Conversation history
- `tools` - Available tools for function calling

#### Returns

Returns `CompletionResponse` with:

- `message` - Generated response message
- `usage` - Token usage statistics (if available)
- `model` - Model that generated response
- `reasoning` - Reasoning content (if model supports it)

#### Errors

- `UnsupportedEndpoint` - Model doesn't support any compatible endpoint
- `ApiError` - HTTP or API error
- `SseParseError` - Streaming parse error
- `EndpointFallbackFailed` - Both endpoints failed
- `MessageConversionError` - Message format conversion error

#### Example

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;
    
    let messages = vec![Message::user("What is the weather?")];
    let response = provider.complete(&messages, &[]).await?;
    
    println!("{}", response.message.content());
    Ok(())
}
```

### list_models()

List available Copilot models with endpoint support information.

```rust
async fn list_models(&self) -> Result<Vec<ModelInfo>>
```

#### Returns

Vector of `ModelInfo` containing:

- `id` - Model identifier
- `capabilities` - Model capabilities
- `provider_specific` - Metadata including `supported_endpoints`

## Endpoint Selection

The provider automatically selects the best endpoint:

1. **Responses Endpoint** (preferred):

   - Used when model advertises "responses" in `supported_endpoints`
   - Provides streaming, reasoning, and advanced features
   - URL: `https://api.githubcopilot.com/responses`

2. **Chat Completions Endpoint** (fallback):

   - Used when responses not supported
   - Compatible with all models
   - URL: `https://api.githubcopilot.com/chat/completions`

3. **Automatic Fallback**:

   - If responses endpoint fails and `enable_endpoint_fallback` is true
   - Automatically retries with completions endpoint
   - Warning logged to stderr

## Streaming

When `enable_streaming` is true (default):

- Uses Server-Sent Events (SSE) for progressive response
- Reduces time-to-first-token
- Aggregates events into final CompletionResponse
- Handles connection interruptions gracefully

## Reasoning Support

For reasoning-capable models:

- Set `include_reasoning: true` (default false)
- Optionally set `reasoning_effort` to "low", "medium", or "high"
- Reasoning content returned in `CompletionResponse.reasoning` field
- Useful for models like o1, o1-mini, o1-preview

## Authentication

Uses GitHub OAuth device flow:

1. First run prompts for GitHub authentication
2. Tokens cached securely in system keyring
3. Automatic token refresh when expired
4. No manual token management required

## Error Handling

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = CopilotProvider::new(CopilotConfig::default())?;
    let messages = vec![Message::user("Hello")];
    
    match provider.complete(&messages, &[]).await {
        Ok(response) => {
            println!("Success: {}", response.message.content());
            if let Some(reasoning) = response.reasoning {
                println!("Reasoning: {}", reasoning);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
    
    Ok(())
}
```

## Performance

- Model cache TTL: 1 hour (reduces API calls)
- Streaming enabled by default
- Connection pooling via reqwest
- Automatic retry with exponential backoff

## Limitations

- Maximum context length varies by model
- Rate limits apply per GitHub account
- Some models may not support all features
- Reasoning only available on specific models

## See Also

- [GitHub Copilot API Documentation](https://docs.github.com/en/copilot)
- [Provider Abstraction](./provider_abstraction.md)
- [Configuration Reference](./configuration.md)
