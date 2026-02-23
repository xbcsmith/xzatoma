# Configuring Subagents

Subagents are separate AI agent instances that can be delegated tasks to handle parallel work, specialized operations, or cost optimization. This guide covers how to configure subagent providers, models, and behavior.

## Overview

XZatoma supports configuring subagents with different providers and models than your main agent, enabling powerful scenarios:

- **Cost Optimization**: Use cheaper models for subagent work
- **Provider Mixing**: Copilot for main tasks, local Ollama for subagents
- **Speed Optimization**: Fast local models for quick subagent operations
- **Specialization**: Different models for different task types

## Configuration Structure

Subagent configuration lives under `agent.subagent` in your YAML config file:

```yaml
agent:
  subagent:
    provider: "ollama" # Optional: override main provider
    model: "llama3.2:3b" # Optional: override main model
    chat_enabled: true # Optional: enable subagents in chat mode
    max_executions: 5 # Optional: limit concurrent subagents
    max_depth: 3 # Optional: limit delegation depth
    max_total_tokens: 50000 # Optional: token budget
    default_max_turns: 10 # Optional: conversation turns per subagent
```

### Configuration Fields

#### `provider` (Optional)

Override the provider used for subagent instances. If not specified, subagents use the main provider.

**Valid values**: `copilot`, `ollama`

**Example**:

```yaml
agent:
  subagent:
    provider: ollama
```

#### `model` (Optional)

Override the model for subagent instances. Must be available in the configured provider.

**Example**:

```yaml
agent:
  subagent:
<<<<<<< Updated upstream
    model: "gpt-5-mini"  # Cheaper model for cost optimization
=======
    model: "gpt-5.1-codex-mini" # Cheaper model for cost optimization
>>>>>>> Stashed changes
```

#### `chat_enabled` (Optional)

Enable subagent functionality in interactive chat mode. Default is `false` (disabled).

When enabled, users can:

- Use `/subagents on|off` to toggle subagent delegation
- Mention subagent keywords to auto-enable (`/subagents`, `delegate`, `spawn agent`, etc.)

**Example**:

```yaml
agent:
  subagent:
    chat_enabled: true
```

#### `max_executions` (Optional)

Maximum number of concurrent subagent instances. Prevents resource exhaustion.

**Default**: `5`

**Example**:

```yaml
agent:
  subagent:
    max_executions: 3
```

#### `max_depth` (Optional)

Maximum delegation nesting level. Prevents infinite recursion where subagents spawn subagents.

**Default**: `3`

**Example**:

```yaml
agent:
  subagent:
    max_depth: 2
```

#### `max_total_tokens` (Optional)

Total token budget for all active subagents. Helps control API costs.

**Example**:

```yaml
agent:
  subagent:
    max_total_tokens: 100000
```

#### `default_max_turns` (Optional)

Maximum conversation turns each subagent can take before returning results.

**Default**: `10`

**Example**:

```yaml
agent:
  subagent:
    default_max_turns: 5
```

## Use Cases and Examples

### Cost Optimization: Cheap Model for Subagents

Use an expensive model (GPT-4) for main tasks but delegate heavy lifting to cheaper models:

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  subagent:
<<<<<<< Updated upstream
    model: gpt-5-mini        # 10x cheaper
=======
    model: gpt-5.1-codex-mini # cheaper model for subagents
>>>>>>> Stashed changes
    chat_enabled: false
    max_executions: 10
```

**When to use**: Large document processing, bulk analysis, data transformation

**Cost impact**: 70-80% reduction for subagent-heavy workloads

### Provider Mixing: Copilot + Ollama Subagents

Use cloud-based Copilot for reasoning, local Ollama for cost-free subagents:

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  subagent:
    provider: ollama # Use local model for subagents
    model: llama3.2:3b
    chat_enabled: true
    max_executions: 5
```

**When to use**: Hybrid workflows with local reasoning resources

**Cost impact**: Zero cost for subagent operations (local only)

### Speed Optimization: Fast Local Model

Delegate quickly with small, fast local models:

```yaml
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  subagent:
<<<<<<< Updated upstream
    model: granite3.2:2b              # Smaller, faster model
=======
    model: granite3.2:2b # Smaller, faster model
>>>>>>> Stashed changes
    chat_enabled: true
    default_max_turns: 3 # Quick operations only
    max_depth: 1 # Keep it simple
```

**When to use**: Real-time operations, quick parallel tasks

**Performance**: 5-10x faster responses on local hardware

## Configuration Validation

XZatoma validates subagent configuration on startup:

### Valid Configuration

```yaml
agent:
  subagent:
    provider: ollama
    model: llama3.2:3b
    chat_enabled: true
```

### Invalid Configuration Examples

**Missing provider model**:

```yaml
agent:
  subagent:
    provider: ollama # ERROR: model not specified
    chat_enabled: true
```

**Unknown provider**:

```yaml
agent:
  subagent:
    provider: unknown-provider # ERROR: not "copilot" or "ollama"
    model: some-model
```

**Model not available**:

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-4

agent:
  subagent:
    provider: copilot
    model: nonexistent-model # ERROR: not available in provider
```

## Troubleshooting

### "Subagent provider not configured"

**Cause**: Subagent config exists but provider field is missing while trying to use subagents.

**Solution**: Specify `provider` field or leave it unset to use the main provider:

```yaml
agent:
  subagent:
    # provider: ollama   <- Remove if using main provider
<<<<<<< Updated upstream
    model: gpt-5-mini
=======
    model: gpt-5.1-codex-mini
>>>>>>> Stashed changes
    chat_enabled: true
```

### "Model not available in provider"

**Cause**: Requested model doesn't exist in the configured provider.

**Solution**: Check available models:

```bash
# List available models
xzatoma /models list

# Or check specific provider
xzatoma /models list --provider ollama
```

### Subagents not working in chat mode

**Cause**: `chat_enabled: false` or subagents not enabled at runtime.

**Solution**:

1. Verify config: `chat_enabled: true`
2. In chat, run `/subagents on` to enable
3. Check status: `/status`

### High token usage

**Cause**: Subagents are consuming more tokens than expected.

**Solution**: Set limits:

```yaml
agent:
  subagent:
    max_total_tokens: 50000 # Hard cap
    default_max_turns: 5 # Fewer turns per subagent
    max_executions: 2 # Fewer concurrent subagents
```

### Performance degradation

**Cause**: Too many concurrent subagents or complex models.

**Solution**: Use faster models and reduce concurrency:

```yaml
agent:
  subagent:
<<<<<<< Updated upstream
    model: granite3.2:2b              # Faster, smaller
    max_executions: 2             # Reduce parallelism
    max_depth: 1                  # Simpler delegation
=======
    model: granite3.2:2b # Faster, smaller
    max_executions: 2 # Reduce parallelism
    max_depth: 1 # Simpler delegation
>>>>>>> Stashed changes
```

## Migration from Previous Versions

If you have an existing XZatoma config without subagent settings, subagents are disabled by default. To enable:

1. Add subagent configuration:

```yaml
agent:
  subagent:
    chat_enabled: false # Start disabled
    max_executions: 5
    max_depth: 3
```

2. Test in chat mode: `/subagents on`

3. Adjust provider/model if needed

**Backward compatibility**: Existing configs without subagent sections work unchanged. Subagents are opt-in.

## Performance Considerations

### Provider Initialization

- **Copilot**: First use establishes HTTP connection (100-500ms)
- **Ollama**: Local connection (1-10ms)

Multiple subagent instances reuse the provider connection.

### Concurrent Subagents

Default `max_executions: 5` balances parallelism vs. resource usage:

- **1-2 subagents**: Fast, sequential fallback if one fails
- **3-5 subagents**: Good parallelism, manageable resource use
- **6+ subagents**: High parallelism, higher memory/cost

### Model Selection Impact

<<<<<<< Updated upstream
| Model | Speed | Quality | Cost | Best For |
|-------|-------|---------|------|----------|
| granite3.2:2b | Very Fast | Basic | Free | Quick tasks |
| llama3.2 | Fast | Good | Free | General work |
| gpt-5-mini | Moderate | Excellent | $0.0005/1K tokens | Analysis |
| gpt-4 | Slow | Best | $0.03/1K tokens | Complex reasoning |
=======
| Model            | Speed     | Quality   | Cost     | Best For          |
| ---------------- | --------- | --------- | -------- | ----------------- |
| granite3.2:2b    | Very Fast | Good      | Free     | Quick tasks       |
| llama3.2:3b      | Fast      | Good      | Free     | General work      |
| gpt-5.1-codex-mini | Moderate  | Excellent | Low      | Analysis          |
| gpt-5.3-codex       | Fast      | Best      | Moderate | Complex reasoning |
>>>>>>> Stashed changes

## Best Practices

### 1. Start Conservative

Begin with safe limits:

```yaml
agent:
  subagent:
    chat_enabled: false # Disabled by default
    max_executions: 2 # Conservative parallelism
    max_depth: 2 # Limited nesting
```

### 2. Use Chat Mode for Testing

Enable `chat_enabled: true` and test in interactive mode before production:

```bash
xzatoma chat
# /subagents on
# Try delegating tasks
# Check results and costs
```

### 3. Monitor Token Usage

When using paid providers, track usage:

```bash
xzatoma /status
# Check subagent token consumption
```

### 4. Match Model to Task

- **Quick tasks**: Small, fast models (granite3.2:2b)
<<<<<<< Updated upstream
- **General work**: Medium models (llama3.2, gpt-5-mini)
- **Complex analysis**: Larger models (gpt-4)
=======
- **General work**: Medium models (llama3.2:3b, gpt-5.1-codex-mini)
- **Complex analysis**: Larger models (gpt-5.3-codex, claude-sonnet-4.6)
>>>>>>> Stashed changes

### 5. Set Clear Limits

Always specify cost/resource boundaries:

```yaml
agent:
  subagent:
    max_total_tokens: 50000 # Hard cost cap
    max_executions: 5 # Resource limit
    max_depth: 3 # Nesting limit
```

## Advanced Configuration

### Multi-Provider Setup

Use different providers for different scenarios:

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  subagent:
    provider: ollama # Default to local
    model: llama3.2:3b
```

In code, you can override at runtime:

```bash
# Use Ollama subagents
xzatoma run "delegate this" --subagent-provider ollama

# Use Copilot subagents
xzatoma run "delegate this" --subagent-provider copilot
```

### Environment Variable Overrides

Override configuration via environment variables:

```bash
XZATOMA_SUBAGENT_PROVIDER=ollama \
XZATOMA_SUBAGENT_MODEL=llama3.2:3b \
xzatoma chat
```

## Related Documentation

- **Chat Mode Usage**: See `docs/how-to/use_subagents_in_chat.md` for interactive chat mode subagent control
- **Provider Setup**: See `docs/how-to/configure_providers.md` for configuring main provider
- **Model Management**: See `docs/how-to/manage_models.md` for listing and selecting models
- **Examples**: See `docs/tutorials/subagent_configuration_examples.md` for working examples

---

**Last updated**: 2024
**For questions**: See the troubleshooting section above or check project issues

```

---

<<<<<<< Updated upstream
=======
### Task 4.2: Create Chat Mode Usage Guide
```
>>>>>>> Stashed changes
