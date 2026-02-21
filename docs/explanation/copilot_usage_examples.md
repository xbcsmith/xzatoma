# Copilot Provider Usage Examples

## Overview

This document provides practical examples of using the Copilot provider in XZatoma. Examples progress from basic setup to advanced scenarios including streaming, reasoning, and tool calling.

## Basic Chat Completion

The simplest way to use the Copilot provider is with default configuration:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create provider with default configuration
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    // Create a simple message
    let messages = vec![Message::user("Hello, how are you?")];
    
    // Get completion
    let response = provider.complete(&messages, &[]).await?;

    println!("Assistant: {}", response.message.content());
    
    if let Some(usage) = response.usage {
        println!("Tokens used: {:?}", usage);
    }

    Ok(())
}
```

This example:
- Uses default model (gpt-5-mini)
- Enables streaming by default
- Returns a single completion response
- Works with automatic endpoint selection

## Chat Conversation

Multi-turn conversations are handled by passing message history:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    // Build conversation history
    let mut messages = vec![
        Message::user("What is the capital of France?"),
    ];

    // First turn
    let response1 = provider.complete(&messages, &[]).await?;
    println!("Q1: {}", messages[0].content());
    println!("A1: {}\n", response1.message.content());
    
    // Add assistant response to history
    messages.push(response1.message.clone());
    
    // Second turn - context aware
    messages.push(Message::user("What is its population?"));
    
    let response2 = provider.complete(&messages, &[]).await?;
    println!("Q2: {}", messages.last().unwrap().content());
    println!("A2: {}\n", response2.message.content());

    Ok(())
}
```

Key points:
- Message history is maintained by the caller
- Each response becomes input for the next turn
- The model has full context of the conversation
- Use `Role::User`, `Role::Assistant`, `Role::System` as needed

## Using Reasoning Models

For models with extended thinking capabilities:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure for reasoning model
    let config = CopilotConfig {
        model: "o1-preview".to_string(),
        reasoning_effort: Some("high".to_string()),
        include_reasoning: true,
        ..Default::default()
    };

    let provider = CopilotProvider::new(config)?;

    let messages = vec![
        Message::user("Solve this: If a train leaves station A at 2 PM going 60 mph, and another train leaves station B at 3 PM going 40 mph, when will they meet if they are 200 miles apart?"),
    ];

    let response = provider.complete(&messages, &[]).await?;

    println!("Answer:\n{}\n", response.message.content());

    if let Some(reasoning) = response.reasoning {
        println!("Reasoning process:\n{}\n", reasoning);
    }

    Ok(())
}
```

Configuration details:
- `model: "o1-preview"` - Use reasoning model (also o1, o1-mini)
- `reasoning_effort: "high"` - Control thinking depth (low/medium/high)
- `include_reasoning: true` - Include thinking in output
- Leave `reasoning_effort: None` for default

## Tool Calling Example

Functions can be called by the model:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{
    CopilotProvider, Provider, Message, Tool, ToolFunction,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    // Define available tools
    let tools = vec![
        Tool {
            function: ToolFunction {
                name: "get_weather".to_string(),
                description: "Get weather for a location".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "City name"
                        }
                    },
                    "required": ["location"]
                }),
            },
        },
        Tool {
            function: ToolFunction {
                name: "get_time".to_string(),
                description: "Get current time in a timezone".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "timezone": {
                            "type": "string",
                            "description": "Timezone (e.g., UTC, EST)"
                        }
                    },
                    "required": ["timezone"]
                }),
            },
        },
    ];

    let messages = vec![
        Message::user("What is the weather in New York right now?"),
    ];

    let response = provider.complete(&messages, &tools).await?;

    // Check if model called a tool
    if let Some(tool_calls) = response.message.tool_calls() {
        for tool_call in tool_calls {
            println!("Tool called: {}", tool_call.function);
            println!("Arguments: {}", tool_call.arguments);
        }
    } else {
        println!("Response: {}", response.message.content());
    }

    Ok(())
}
```

This example shows:
- Defining tool schemas with JSON parameters
- Passing tools to the complete method
- Checking for tool calls in the response
- Handling both text responses and tool invocations

## Streaming Disabled

For blocking responses (not recommended for UI):

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Disable streaming for blocking response
    let config = CopilotConfig {
        enable_streaming: false,
        ..Default::default()
    };

    let provider = CopilotProvider::new(config)?;

    let messages = vec![
        Message::user("Write a haiku about Rust programming"),
    ];

    // This will wait for the complete response before returning
    let response = provider.complete(&messages, &[]).await?;

    println!("{}", response.message.content());

    Ok(())
}
```

Notes:
- When `enable_streaming: false`, the entire response is buffered
- Takes longer to get first token
- Useful for non-interactive scenarios
- Default is `true` (streaming enabled)

## Custom API Base (Testing)

For testing with mock servers:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Point to mock server
    let config = CopilotConfig {
        api_base: Some("http://localhost:8080".to_string()),
        enable_streaming: false, // Easier for testing
        ..Default::default()
    };

    let provider = CopilotProvider::new(config)?;

    let messages = vec![
        Message::user("Test message"),
    ];

    let response = provider.complete(&messages, &[]).await?;

    println!("{}", response.message.content());

    Ok(())
}
```

This is useful for:
- Unit testing with mock servers
- Development against local API
- Testing error handling

## Endpoint Fallback Behavior

Control how endpoints are selected:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable fallback (default)
    let config_with_fallback = CopilotConfig {
        enable_endpoint_fallback: true,
        ..Default::default()
    };

    // Disable fallback (strict endpoint requirement)
    let config_no_fallback = CopilotConfig {
        enable_endpoint_fallback: false,
        ..Default::default()
    };

    let provider = CopilotProvider::new(config_with_fallback)?;

    let messages = vec![
        Message::user("Hello"),
    ];

    // With fallback enabled:
    // - Tries /responses endpoint first
    // - If that fails, automatically tries /chat/completions
    // - This is more resilient but may not use preferred endpoint

    // With fallback disabled:
    // - Uses /responses if model supports it
    // - Fails if /responses not available
    // - Stricter but ensures specific endpoint usage

    let response = provider.complete(&messages, &[]).await?;
    println!("{}", response.message.content());

    Ok(())
}
```

## Error Handling

Proper error handling patterns:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::error::XzatomaError;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn main() {
    let config = CopilotConfig::default();
    
    match CopilotProvider::new(config) {
        Ok(provider) => {
            let messages = vec![Message::user("Hello")];
            
            match provider.complete(&messages, &[]).await {
                Ok(response) => {
                    println!("Success: {}", response.message.content());
                }
                Err(XzatomaError::UnsupportedEndpoint(model, endpoint)) => {
                    eprintln!("Model {} does not support {} endpoint", model, endpoint);
                }
                Err(XzatomaError::EndpointFallbackFailed) => {
                    eprintln!("Both endpoints failed and no fallback available");
                }
                Err(XzatomaError::SseParseError(msg)) => {
                    eprintln!("Failed to parse streaming response: {}", msg);
                }
                Err(XzatomaError::MessageConversionError(msg)) => {
                    eprintln!("Failed to convert message format: {}", msg);
                }
                Err(e) => {
                    eprintln!("API error: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to create provider: {}", e);
        }
    }
}
```

## Configuration from YAML

Loading configuration from file:

```rust
use xzatoma::config::{Config, CopilotConfig};
use xzatoma::providers::CopilotProvider;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read YAML configuration
    let config_str = fs::read_to_string("xzatoma.yaml")?;
    let config: Config = serde_yaml::from_str(&config_str)?;

    // Create provider from loaded config
    let provider = CopilotProvider::new(config.provider.copilot)?;

    Ok(())
}
```

Example `xzatoma.yaml`:

```yaml
provider:
  type: "copilot"
  copilot:
    model: "gpt-5-mini"
    enable_streaming: true
    enable_endpoint_fallback: true
    reasoning_effort: "medium"
    include_reasoning: false
```

## Configuration from Environment

Using environment variables:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::CopilotProvider;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build config from environment
    let mut config = CopilotConfig::default();

    if let Ok(model) = env::var("XZATOMA_COPILOT_MODEL") {
        config.model = model;
    }

    if let Ok(streaming) = env::var("XZATOMA_COPILOT_ENABLE_STREAMING") {
        config.enable_streaming = streaming.parse().unwrap_or(true);
    }

    let provider = CopilotProvider::new(config)?;

    Ok(())
}
```

Set environment variables:

```bash
export XZATOMA_COPILOT_MODEL=gpt-5-mini
export XZATOMA_COPILOT_ENABLE_STREAMING=true
export XZATOMA_COPILOT_ENABLE_ENDPOINT_FALLBACK=true
export XZATOMA_COPILOT_REASONING_EFFORT=medium
export XZATOMA_COPILOT_INCLUDE_REASONING=false
```

## Listing Available Models

Query what models are available:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    // Get list of available models
    let models = provider.list_models().await?;

    for model in models {
        println!("Model: {}", model.id);
        println!("  Capabilities: {:?}", model.capabilities);
    }

    Ok(())
}
```

## System Message Context

Include system context for the model:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CopilotConfig::default();
    let provider = CopilotProvider::new(config)?;

    let messages = vec![
        Message {
            role: Role::System,
            content: "You are a helpful Rust programming assistant. Provide concise, practical advice.".to_string(),
            tool_calls: None,
        },
        Message::user("How do I handle errors in Rust?"),
    ];

    let response = provider.complete(&messages, &[]).await?;

    println!("{}", response.message.content());

    Ok(())
}
```

System messages:
- Set tone and behavior for the model
- Should be the first message in conversations
- Often more effective than user instructions
- Works with all endpoints and models

## Complete Feature Matrix

| Feature | Endpoint | Streaming | Tools | Reasoning | Status |
|---------|----------|-----------|-------|-----------|--------|
| Basic completion | Both | Yes/No | Yes | No | Fully supported |
| Tool calling | Both | Yes | Yes | Yes | Fully supported |
| Streaming | /responses | Yes | Yes | Yes | Fully supported |
| Streaming | /chat/completions | Yes | Yes | No | Fully supported |
| Reasoning | /responses | Yes | Yes | Yes | Supported on o1 family |
| Reasoning | /chat/completions | Yes | Yes | No | Not applicable |
| Fallback | Both | Yes | Yes | Yes | Configurable |

## Migration Guide

If upgrading from earlier versions:

```rust
// Old way (still works):
let config = CopilotConfig {
    model: "gpt-5-mini".to_string(),
    api_base: None,
};

// New way (with Phase 5 features):
let config = CopilotConfig {
    model: "gpt-5-mini".to_string(),
    api_base: None,
    enable_streaming: true,           // NEW - default true
    enable_endpoint_fallback: true,   // NEW - default true
    reasoning_effort: None,            // NEW - optional
    include_reasoning: false,          // NEW - default false
};

// Or just use defaults:
let config = CopilotConfig::default();
```

All new fields have sensible defaults, so existing code continues to work without changes.
