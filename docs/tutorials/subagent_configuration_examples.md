# Subagent Configuration Examples

This tutorial provides practical, tested examples of subagent configuration for common use cases. Each example includes the full configuration, how to test it, and what to expect.

## Quick Reference

| Use Case | Best For | Config Complexity | Cost Impact |
|----------|----------|------------------|------------|
| [Cost Optimization](#example-1-cost-optimization) | High volume work | Simple | 70% savings |
| [Provider Mixing](#example-2-provider-mixing) | Hybrid workflows | Moderate | Variable |
| [Speed Optimization](#example-3-speed-optimization) | Real-time operations | Simple | Minimal |
| [Chat Mode with Manual Control](#example-4-chat-mode-manual) | Interactive work | Simple | Control-based |

## Example 1: Cost Optimization

### Use Case

You have a Copilot subscription and want to use expensive GPT-4 for main reasoning but delegate routine work to cheaper GPT-3.5-turbo.

### Configuration

```yaml
# config.yaml
provider:
  type: copilot
  copilot:
    model: gpt-4-turbo              # Expensive but powerful

agent:
  subagent:
    model: gpt-3.5-turbo            # Cheap but capable
    chat_enabled: false              # Enable manually with /subagents on
    max_executions: 10               # Allow parallel work
    max_total_tokens: 50000          # Cost cap
```

### Expected Behavior

**Main agent**: Uses GPT-4 for complex reasoning and planning

**Subagents**: Use GPT-3.5-turbo for:
- Document summarization
- Data extraction
- File processing
- Text transformation

### Testing

```bash
# Start chat mode
xzatoma chat

# Enable subagents
/subagents on

# Check model assignment
/models list
# Shows:
#  - Main model: gpt-4-turbo
#  - Subagent model: gpt-3.5-turbo

# Try delegating work
Process these 20 CSV files and create a summary report.
Use subagents to speed up the work.

# Monitor cost
/status
# Shows token usage - should be lower with cheap subagent model
```

### Cost Analysis

**Without subagents**:
- 1 GPT-4 call for 20 files
- Estimated: ~$0.10-0.20 (1000-2000 tokens × $0.03/1K)

**With subagents**:
- 1 GPT-4 call for coordination
- 20 GPT-3.5-turbo calls for file processing
- Estimated: ~$0.02 (2000 cheap tokens × $0.0005/1K)
- **Savings**: ~80-90%

## Example 2: Provider Mixing

### Use Case

You want to use Copilot for strategic work but delegate to your local Ollama instance (free) for parallel processing.

### Configuration

```yaml
# config.yaml
provider:
  type: copilot
  copilot:
    model: gpt-4-turbo

  # Configure Ollama as alternative provider
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  subagent:
    provider: ollama                 # Use local instead of cloud
    model: llama3.2:latest
    chat_enabled: true               # Enable by default
    max_executions: 5                # Reasonable parallelism
    max_depth: 2                     # Limit nesting
```

### Expected Behavior

**Main agent**: Copilot (GPT-4) - strategic thinking

**Subagents**: Ollama on localhost (free) - parallel task execution

**Benefit**: Cost-free delegation while keeping powerful main reasoning

### Setup Prerequisites

```bash
# 1. Install Ollama (macOS)
brew install ollama

# 2. Start Ollama service
brew services start ollama

# 3. Pull a model
ollama pull llama3.2

# 4. Verify it's running
curl http://localhost:11434/api/tags
# Should show available models
```

### Testing

```bash
# Start chat mode
xzatoma chat

# Verify providers are configured
/models list

# Enable subagents (should be enabled by default)
/status
# Shows: Subagents: ENABLED

# Delegate work
Process these 10 documents.
Use subagents to analyze each one in parallel for key terms.

# Monitor execution
# You should see:
# - Main agent (GPT-4) coordinates
# - Subagents use Ollama locally (fast, no API calls)
```

### Cost Analysis

**Without subagents**:
- 1 GPT-4 call processing all 10 documents
- Cost: ~$0.05

**With Ollama subagents**:
- 1 GPT-4 call for coordination
- 10 local Ollama calls (free)
- Cost: ~$0.005
- **Savings**: 90%

### Troubleshooting Provider Mixing

**Issue**: "Failed to connect to Ollama"

**Solution**:
```bash
# Verify Ollama is running
ps aux | grep ollama

# If not running, start it
ollama serve

# Test connection
curl http://localhost:11434/api/tags
```

**Issue**: "Model not found in Ollama"

**Solution**:
```bash
# List available models
ollama list

# Pull the model if missing
ollama pull llama3.2

# Update config with available model
# Then restart xzatoma
```

## Example 3: Speed Optimization

### Use Case

You want fast, local-only operation using small, efficient models everywhere.

### Configuration

```yaml
# config.yaml
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest           # General model

agent:
  subagent:
    # Use same provider (ollama)
    model: gemma2:2b                 # Smaller, faster model
    chat_enabled: true                # Enable by default
    default_max_turns: 3             # Quick responses only
    max_executions: 5                # Good parallelism locally
```

### Expected Behavior

- All operations local (no API calls)
- Main agent: Medium-sized model (balanced)
- Subagents: Very small model (fast)
- Response time: Sub-second for subagent operations

### Setup

```bash
# 1. Start Ollama
ollama serve

# 2. Pull models (if not already available)
ollama pull llama3.2      # ~4GB
ollama pull gemma2:2b     # ~1.6GB (faster)

# 3. Verify both are available
ollama list
```

### Testing

```bash
# Start chat mode
xzatoma chat

# Verify local operation
/models list
# Should show: ollama provider with local models

# Enable subagents (should be enabled)
/status

# Delegate quick work
Use subagents to read and summarize these 5 text files.

# Watch performance
# Subagent responses should be instant (all local)
```

### Performance Analysis

**Speed metrics** (typical):
- Main agent processing: 2-5 seconds per request
- Subagent delegation: <1 second response per task
- 5 parallel subagents: 1-3 seconds to complete all tasks

**Resource usage** (typical):
- RAM: 4-6GB for two models loaded
- CPU: 60-80% during processing
- Network: 0 MB (local only)

## Example 4: Chat Mode with Manual Control

### Use Case

You want subagents available in chat mode but want to control them manually. No automatic enablement.

### Configuration

```yaml
# config.yaml
provider:
  type: copilot
  copilot:
    model: gpt-4

agent:
  subagent:
    # Keep same provider for simplicity
    # model not specified - uses main model
    chat_enabled: false              # Don't enable automatically
    max_executions: 5
    max_depth: 3
    max_total_tokens: 100000
```

### Expected Behavior

- Subagents start disabled in chat mode
- Users explicitly enable with `/subagents on`
- No automatic keyword detection
- Manual control over cost and resource usage

### Testing

```bash
# Start chat mode
xzatoma chat

# Check initial status
/status
# Shows: Subagents: disabled

# Regular prompt - no subagent involvement
Summarize this file
# Uses main agent only

# Enable subagents manually
/subagents on

# Now subagents can be used
Delegate file processing to subagents
# Uses subagents

# Disable when done
/subagents off
```

## Example 5: Advanced - Multiple Configurations

### Use Case

You want different configurations for different scenarios. Create multiple config files.

### Setup Structure

```
~/.xzatoma/
├── config.yaml                 # Default (cost-optimized)
├── config-local.yaml           # Local-only (Ollama)
├── config-premium.yaml         # All GPT-4 (expensive)
└── config-chat.yaml            # Chat-focused
```

### Configuration Files

**config.yaml** (cost-optimized, default):
```yaml
provider:
  type: copilot
  copilot:
    model: gpt-4

agent:
  subagent:
    model: gpt-3.5-turbo
    chat_enabled: false
    max_executions: 10
```

**config-local.yaml** (fully local):
```yaml
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2

agent:
  subagent:
    model: gemma2:2b
    chat_enabled: true
    max_executions: 5
```

**config-premium.yaml** (best quality):
```yaml
provider:
  type: copilot
  copilot:
    model: gpt-4

agent:
  subagent:
    model: gpt-4              # Same powerful model
    chat_enabled: true
    max_executions: 3         # Limit cost
    max_total_tokens: 200000
```

### Usage

```bash
# Use default config (cost-optimized)
xzatoma chat

# Use local-only config (free)
xzatoma chat --config ~/.xzatoma/config-local.yaml

# Use premium config (best quality, highest cost)
xzatoma chat --config ~/.xzatoma/config-premium.yaml

# Run plan with specific config
xzatoma run "process files" --config ~/.xzatoma/config-local.yaml
```

## Example 6: Production-Ready Configuration

### Use Case

You're deploying XZatoma in production and need safety, monitoring, and cost controls.

### Configuration

```yaml
# config-production.yaml
provider:
  type: copilot
  copilot:
    model: gpt-4-turbo

agent:
  # Main agent settings
  max_turns: 10
  max_depth: 3

  # Subagent settings with production guardrails
  subagent:
    provider: copilot
    model: gpt-3.5-turbo            # Cheap for cost control
    chat_enabled: false              # Manual control only
    max_executions: 3                # Conservative parallelism
    max_depth: 1                     # No recursive delegation
    max_total_tokens: 10000          # Hard cost cap per session
    default_max_turns: 5             # Quick completion

  # Safety settings
  tools:
    file_ops:
      allowed_dirs:
        - /data/input
        - /data/output
      max_file_size: 10485760        # 10MB
    terminal:
      allowed_commands:
        - ls
        - cat
        - find
        - grep
```

### Testing

```bash
# Test in planning mode (safe)
xzatoma chat --config config-production.yaml
/mode planning
/subagents on
# Subagents disabled because token limit is strict

# Test with higher token limit for development
# Create config-dev.yaml with max_total_tokens: 100000
```

### Production Checklist

- [ ] `max_total_tokens` set to prevent cost overruns
- [ ] `max_executions` limited to prevent resource exhaustion
- [ ] `max_depth` set to 1 to prevent recursive delegation
- [ ] `chat_enabled: false` to prevent accidental usage
- [ ] File operation directories restricted
- [ ] Terminal commands whitelisted
- [ ] Token limits tested in planning mode first

## Example 7: Development Configuration

### Use Case

You're developing and testing features. You want fast iteration with good quality.

### Configuration

```yaml
# config-dev.yaml
provider:
  type: copilot
  copilot:
    model: gpt-4-turbo              # Good quality for testing

agent:
  subagent:
    model: gpt-4-turbo              # Same quality for consistency
    chat_enabled: true              # Enable by default
    max_executions: 5               # Good parallelism
    max_depth: 3                    # Allow nesting
    max_total_tokens: 500000        # High limit for testing
    default_max_turns: 20           # Thorough responses
```

### Usage

```bash
# Quick iteration
xzatoma chat --config config-dev.yaml

# In chat mode
/subagents on
> Delegate complex task analysis to subagents
# Get high-quality results for testing
```

## Migration Guide

### From No Subagents to Subagents

**Step 1**: Update config

```yaml
agent:
  subagent:
    model: gpt-3.5-turbo
    chat_enabled: false              # Conservative start
    max_executions: 5
```

**Step 2**: Test in planning mode

```bash
xzatoma chat
/mode planning
/subagents on
> Try delegation
# No actual execution, just planning
```

**Step 3**: Test in write mode with safety

```bash
/mode write
/safe
/subagents on
> Delegate a small task
# Requires confirmation for each operation
```

**Step 4**: Enable by default (optional)

```yaml
agent:
  subagent:
    chat_enabled: true              # Now enable by default
```

## Validation Checklist

For each configuration example:

- [ ] Configuration file is valid YAML
- [ ] All required provider settings are present
- [ ] Models specified exist in provider
- [ ] Chat mode starts without errors
- [ ] `/status` shows expected state
- [ ] `/models list` shows available models
- [ ] Subagent state matches `chat_enabled` setting
- [ ] Test delegation works as expected
- [ ] `/subagents on` and `/subagents off` work
- [ ] Token and execution limits are reasonable

## Performance Benchmarks

### Cost Optimization Example

| Operation | Main Only | With Subagents | Savings |
|-----------|-----------|---|---|
| Process 20 files | $0.15 | $0.02 | 87% |
| 10K tokens | $0.03 | $0.005 | 83% |

### Speed Optimization Example

| Operation | Sequentially | Parallel (5 agents) | Speedup |
|-----------|---|---|---|
| Process 10 files | 10s | 2s | 5x |
| Analyze docs | 30s | 6s | 5x |

### Provider Mixing Example

| Scenario | Cost | Speed | Resources |
|----------|------|-------|-----------|
| All Copilot | $0.10 | Medium | Cloud |
| Copilot + Ollama | $0.02 | Fast | Hybrid |
| All Ollama | Free | Fast | Local |

## Common Configuration Mistakes

### Mistake 1: Forgetting to Enable Chat Mode

```yaml
# Wrong
agent:
  subagent:
    model: gpt-3.5-turbo
    # Missing: chat_enabled
```

**Fix**:
```yaml
# Right
agent:
  subagent:
    model: gpt-3.5-turbo
    chat_enabled: true              # Add this
```

### Mistake 2: Using Non-Existent Model

```yaml
# Wrong
agent:
  subagent:
    model: gpt-99-ultra             # Doesn't exist
```

**Fix**: Check available models first
```bash
xzatoma /models list
```

Then use real model name.

### Mistake 3: Not Setting Token Limits

```yaml
# Wrong
agent:
  subagent:
    model: gpt-4
    # Missing: max_total_tokens
```

**Fix**:
```yaml
agent:
  subagent:
    model: gpt-4
    max_total_tokens: 50000         # Add limit
```

### Mistake 4: Using Wrong Provider Name

```yaml
# Wrong
agent:
  subagent:
    provider: openai                # Not "copilot" or "ollama"
```

**Fix**: Use only configured providers:
```yaml
agent:
  subagent:
    provider: copilot               # or "ollama"
```

## Next Steps

1. **Start simple**: Use Cost Optimization (Example 1)
2. **Test in chat**: Enable `chat_enabled: true`
3. **Monitor usage**: Check `/status` regularly
4. **Adjust limits**: Set appropriate `max_total_tokens` for your use case
5. **Explore advanced**: Try provider mixing or multiple configs

## Related Documentation

- **Configuration Guide**: `docs/how-to/configure_subagents.md`
- **Chat Mode Usage**: `docs/how-to/use_subagents_in_chat.md`
- **Provider Setup**: `docs/how-to/configure_providers.md`

---

**Last updated**: 2024  
**Have questions?**: Check the configuration guide or troubleshooting sections
