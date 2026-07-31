# How to Switch Models

## Overview

This guide shows you how to switch between different AI models in XZatoma. You
can switch models via CLI configuration, during interactive chat sessions, or
programmatically through the API.

## Prerequisites

- XZatoma installed and configured
- At least one provider configured (GitHub Copilot, Ollama, or OpenAI)
- Multiple models available (verify with `xzatoma models list`)

## Quick Start

### Switch Model in Chat Mode

During an interactive chat session:

```bash
/model gpt-4o
```

### Switch Default Model in Configuration

Edit `config/config.yaml`:

```yaml
provider:
  type: ollama
  ollama:
    model: qwen2.5-coder:14b # Change this line
```

## Switching Models in Interactive Chat

### Step 1: Start Chat Session

```bash
xzatoma chat
```

### Step 2: List Available Models

```bash
/models list
```

Example output:

```text
Model Name      Display Name     Context Window  Capabilities
qwen2.5-coder:7b   Qwen 2.5 Coder 7B   32768 tokens   FunctionCalling
qwen2.5-coder:14b   Qwen 2.5 Coder 14B  32768 tokens   FunctionCalling
llama3.2:3b      Llama 3.2 3B     8192 tokens    FunctionCalling

Note: Current model is highlighted in green
```

### Step 3: Switch to Different Model

```bash
/model qwen2.5-coder:14b
```

Expected response:

```text
Switched to model: qwen2.5-coder:14b (32768 token context)
```

### Step 4: Verify Switch

```bash
/context
```

This shows the current model and context information.

## Understanding Model Switching Behavior

### Conversation Persistence

When you switch models, your conversation history is preserved:

- All previous messages remain in the conversation
- The agent reconstructs with the new model
- Context window limits are updated

### Context Window Warnings

If your current conversation is larger than the new model's context window,
you'll see a warning:

```text
WARNING: Current conversation (45000 tokens) exceeds new model context (32768 tokens)
Messages will be pruned to fit the new context window.

Continue with model switch? [y/N]:
```

XZatoma will automatically prune older messages to fit the new context.

### Agent Reconstruction

Switching models creates a new agent instance:

1. Validates the target model exists
2. Creates a new provider with the selected model
3. Preserves conversation history
4. Updates context window settings
5. Replaces the current agent

## Switching Models via Configuration

### Temporary Switch (Current Session)

Use the `--provider` flag to override config:

```bash
xzatoma chat --provider copilot
```

This uses Copilot instead of the configured provider for this session only.

### Permanent Switch (Update Config)

Edit `config/config.yaml`:

```yaml
provider:
  type: ollama # or 'copilot' or 'openai'

  ollama:
    host: http://localhost:11434
    model: qwen2.5-coder:14b # Change default model

  copilot:
    model: gpt-5-mini # Copilot default model

  openai:
    base_url: https://api.openai.com/v1
    model: gpt-4o-mini # OpenAI default model
```

Restart XZatoma to use the new default.

## Switching Between Providers

### From Ollama to Copilot

In chat mode:

```bash
/help
```

Exit and restart with Copilot:

```bash
xzatoma chat --provider copilot
```

Or update config:

```yaml
provider:
  type: copilot
```

### From Copilot to Ollama

Ensure Ollama is running:

```bash
ollama serve
```

Update config:

```yaml
provider:
  type: ollama
```

Or use override:

```bash
xzatoma chat --provider ollama
```

## Advanced Model Switching

### Switch Based on Context Needs

Monitor context usage and switch proactively:

```bash
/context
```

If approaching the limit:

```text
Context usage: 85.3%

Usage Level: 85.3%
```

Switch to a model with larger context:

```bash
/model claude-3.5-sonnet
```

### Switch Based on Task Type

For code-heavy tasks:

```bash
/model qwen2.5-coder:14b
```

For general conversation:

```bash
/model llama3.2:3b
```

For tasks requiring large context:

```bash
/model gemini-2.0-flash-exp
```

### Programmatic Model Switching

In Rust code:

```rust
use xzatoma::providers::{create_provider, Provider};
use xzatoma::config::Config;

let config = Config::load("config/config.yaml", &Default::default())?;
let mut provider = create_provider("ollama", &config.provider)?;

// Switch model
provider.set_model("qwen2.5-coder:14b".to_string()).await?;

// Verify
let current = provider.get_current_model()?;
println!("Now using: {}", current);
```

## Model Switching Strategies

### Performance Optimization

Use smaller models for simple tasks:

```bash
/model llama3.2:3b
```

Switch to larger models when needed:

```bash
/model qwen2.5-coder:14b
```

### Cost Management (Copilot)

Start with efficient models:

```bash
/model gpt-4-turbo
```

Upgrade for complex tasks:

```bash
/model o1-preview
```

### Context Management

Small context for focused tasks:

```bash
/model gpt-4 # 8k context
```

Large context for comprehensive analysis:

```bash
/model gpt-4o # 128k context
```

## Troubleshooting

### Model Not Found Error

**Problem**: `Model 'xyz' not found`

**Solution**:

1. List available models:

```bash
/models list
```

1. Use exact model name (case-sensitive):

```bash
/model qwen2.5-coder:7b
```

1. For Ollama, ensure model is installed:

```bash
ollama pull qwen2.5-coder:7b
```

### Model Switch Fails Silently

**Problem**: Command accepted but model doesn't change

**Solution**:

1. Check provider capabilities:

```bash
xzatoma models list
```

1. Verify you have permission to switch models

2. Check logs for errors:

```bash
RUST_LOG=debug xzatoma chat
```

### Context Pruning After Switch

**Problem**: Messages disappear after switching to smaller context model

**Explanation**: This is expected behavior. When switching to a model with
smaller context, older messages are automatically pruned to fit.

**Prevention**:

1. Check new model context before switching:

```bash
xzatoma models info --model target-model
```

1. Save important context before switching:

- Copy important messages
- Export conversation if needed

1. Choose models with adequate context for your needs

### Provider Switch Not Working

**Problem**: Chat mode doesn't switch providers

**Explanation**: Provider switching requires restarting the session. Use `/exit`
and start a new session.

**Solution**:

1. Exit current session:

```bash
/exit
```

1. Start new session with different provider:

```bash
xzatoma chat --provider copilot
```

Or update config permanently.

## Best Practices

### 1. Check Model Capabilities First

Before switching, verify the model supports required features:

```bash
xzatoma models info --model target-model
```

Look for:

- `FunctionCalling` capability (required for XZatoma tools)
- Adequate context window
- Appropriate capabilities for your task

### 2. Monitor Context Before Switching

Check current usage:

```bash
/context
```

Ensure new model has sufficient context:

```bash
xzatoma models info --model new-model | grep "Context Window"
```

### 3. Switch Proactively

Don't wait until you hit context limits:

- Switch at 60-70% usage for safety
- Plan model choice based on expected conversation length
- Use larger context models for exploratory tasks

### 4. Test Model Performance

Try different models for your use case:

```bash
/model model-a
How do I implement a binary tree?

/model model-b
How do I implement a binary tree?
```

Compare quality and speed.

### 5. Keep Track of Model Changes

In long sessions, note which model produced which results:

```bash
/context # Shows current model
```

This helps when reviewing conversation history.

## Examples

### Example 1: Switch to Larger Context

```text
User: I need to analyze a large codebase
```
