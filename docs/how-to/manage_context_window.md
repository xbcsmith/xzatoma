# How to Manage Context Window

## Understanding Context Window Limits

Every AI model has a context window limit—a maximum number of tokens it can process in a single conversation. For example:

- GPT-4o has a 128,000 token context window
- GPT-4o-mini has a 128,000 token context window
- Llama 3.2 has a 8,000 token context window
- Mistral has variable windows depending on the variant

When your conversation approaches this limit, XZatoma can:

1. **Warn you** (in chat mode) that context is running low
2. **Automatically summarize** (in run mode) old conversation turns to preserve space
3. **Prune old turns** to reduce context before summarizing

This guide explains how to configure and use these features.

## Monitoring Context Usage

### In Chat Mode

Use the `/context info` command to check your current context usage:

```bash
/context info
```

This displays:

- Total tokens available in context window
- Tokens currently used by conversation history
- Percentage of context window filled
- Remaining tokens available

### Understanding the Status Indicators

The status shows three states:

- **Normal**: Conversation is using less than the warning threshold (typically 85%)
- **Warning**: Conversation exceeds the warning threshold but not the auto-summarize threshold
- **Critical**: Conversation exceeds the auto-summarize threshold

When you reach the **Warning** status in chat mode, XZatoma displays a notification showing your context usage percentage and available tokens.

## Manual Summarization in Chat Mode

When you see the context window warning in chat mode, you can manually summarize and reset the conversation to make room for new content.

### Basic Summarization

Use the `/context summary` command:

```bash
/context summary
```

This:

1. Creates a summary of the entire conversation so far
2. Replaces the full conversation history with the summary
3. Resets your token count, freeing up context space
4. Continues the conversation from the summary

### Summarization with a Specific Model

By default, summarization uses the same model as your main conversation. To use a cheaper or faster model for summarization:

```bash
/context summary -m gpt-4o-mini
```

or for Ollama:

```bash
/context summary -m mistral:latest
```

This is useful for cost optimization—use an expensive model for interactions but a cheaper model for generating summaries.

## Automatic Summarization in Run Mode

In run mode (executing a plan), XZatoma automatically handles context management:

1. **Token Monitoring**: As the agent executes turns, XZatoma continuously monitors token usage
2. **Warning Threshold**: When conversation exceeds 85% of the context window, the agent logs a warning
3. **Auto-Summarization**: When conversation exceeds 90% of the context window:
   - Old conversation turns are pruned (except the minimum configured)
   - Remaining conversation is summarized
   - Summary replaces the full history
   - Execution continues

This happens automatically without user intervention, keeping the agent running even in very long executions.

## Configuring Context Management

Edit your `config/config.yaml` to customize context behavior:

```yaml
agent:
  conversation:
    # Maximum tokens allowed in conversation context
    max_tokens: 100000

    # Minimum number of turns to retain when pruning
    min_retain_turns: 5

    # Token threshold to trigger pruning (0.0-1.0)
    prune_threshold: 0.8

    # Warn at this token usage percentage (0.0-1.0)
    warning_threshold: 0.85

    # Auto-summarize at this token usage percentage (0.0-1.0)
    auto_summary_threshold: 0.90

    # Optional: Use cheaper model for summaries
    # summary_model: "gpt-4o-mini"
```

### Understanding Configuration Options

- `max_tokens`: The absolute limit—conversation will be summarized if it approaches this
- `min_retain_turns`: Always keep at least this many recent turns during summarization
- `prune_threshold`: Before summarizing, trim old turns if over this percentage
- `warning_threshold`: Show warning when over this percentage (chat mode only)
- `auto_summary_threshold`: Automatically summarize when over this percentage (run mode only)
- `summary_model`: Optional model override for cost savings

### Example: Large Context Window for Long Conversations

If you're working on a long project and want minimal interruptions:

```yaml
agent:
  conversation:
    max_tokens: 200000
    min_retain_turns: 10
    prune_threshold: 0.75
    warning_threshold: 0.85
    auto_summary_threshold: 0.95
    summary_model: "gpt-4o-mini"  # Use cheaper model for summaries
```

### Example: Conservative Settings for Cost Optimization

If you want aggressive summarization to minimize token usage:

```yaml
agent:
  conversation:
    max_tokens: 50000
    min_retain_turns: 3
    prune_threshold: 0.7
    warning_threshold: 0.75
    auto_summary_threshold: 0.85
    summary_model: "gpt-4o-mini"
```

## Using Environment Variables to Override Configuration

Override context settings without editing the config file:

```bash
# Set maximum context tokens
export XZATOMA_CONTEXT_MAX_TOKENS=150000

# Set warning threshold to 80%
export XZATOMA_CONTEXT_WARNING_THRESHOLD=0.80

# Set auto-summarize threshold to 85%
export XZATOMA_CONTEXT_AUTO_SUMMARY_THRESHOLD=0.85

# Use specific model for summaries
export XZATOMA_CONTEXT_SUMMARY_MODEL=gpt-4o-mini

xzatoma run --plan my_plan.yaml
```

This is useful for temporary adjustments or testing different configurations.

## Best Practices

### For Chat Mode

1. **Monitor regularly**: Use `/context info` periodically, especially for long conversations
2. **Summarize proactively**: Don't wait until critical—summarize at the warning stage
3. **Review summaries**: After summarizing, review the summary to ensure important context is preserved
4. **Use cheaper models for summaries**: Configure `summary_model` to optimize costs

### For Run Mode

1. **Conservative thresholds**: Set `warning_threshold` low (0.75-0.80) to start summarizing early
2. **Preserve recent context**: Set `min_retain_turns` high enough to keep recent interactions
3. **Use cost optimization**: Always configure `summary_model` for long runs
4. **Monitor logs**: Watch agent logs to see when automatic summarization occurs

### For Both Modes

1. **Adjust based on model**: Larger context windows (GPT-4o) allow higher thresholds
2. **Test your settings**: Run a test conversation/plan to validate your configuration
3. **Consider complexity**: Complex tasks benefit from higher `min_retain_turns`
4. **Balance cost vs. quality**: Summarization loses some nuance—adjust thresholds accordingly

## Troubleshooting

### "Context window approaching limit" warning appears frequently

**Solution**: Increase `max_tokens` or lower `warning_threshold` to summarize more aggressively

```yaml
agent:
  conversation:
    max_tokens: 150000
    warning_threshold: 0.75  # Warn earlier
```

### Summaries are losing important information

**Solution**: Increase `min_retain_turns` to preserve more recent context

```yaml
agent:
  conversation:
    min_retain_turns: 10  # Keep more recent turns
```

### Summarization is happening too frequently in run mode

**Solution**: Lower `auto_summary_threshold` or increase `max_tokens`

```yaml
agent:
  conversation:
    max_tokens: 150000  # Increase available space
    auto_summary_threshold: 0.95  # Summarize less frequently
```

### Summarization is too expensive in run mode

**Solution**: Configure a cheaper model for summarization

```yaml
agent:
  conversation:
    summary_model: "gpt-4o-mini"  # Cheaper alternative
```

## Related Topics

- Configuration Reference: `../reference/configuration.md`
- Chat Mode Guide: `use_chat_modes.md`
- Conversation History: `manage_conversation_history.md`
- Configuration: `configure_providers.md`
