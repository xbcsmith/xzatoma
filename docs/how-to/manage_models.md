# How to Manage AI Models

## Overview

This guide shows you how to discover, inspect, and manage AI models in XZatoma. You'll learn how to list available models, view detailed model information, and check which model is currently active.

## Prerequisites

- XZatoma installed and configured
- At least one provider configured (GitHub Copilot or Ollama)
- For Ollama: Ollama service running with at least one model installed

## Listing Available Models

### Using the CLI

To see all available models from your configured provider:

```bash
xzatoma models list
```

Example output:

```
Available models from ollama:

+-------------------+-------------------+----------------+-------------------+
| Model Name    | Display Name   | Context Window | Capabilities   |
+-------------------+-------------------+----------------+-------------------+
| granite4:3b   | Llama 3.2 13B   | 8192 tokens  | FunctionCalling  |
| llama3.2:3b    | Llama 3.2 3B   | 8192 tokens  | FunctionCalling  |
| gemma2:9b     | Gemma 2 9B    | 8192 tokens  | FunctionCalling  |
+-------------------+-------------------+----------------+-------------------+
```

### Listing Models from a Specific Provider

Override the configured provider using the `--provider` flag:

```bash
xzatoma models list --provider copilot
```

This shows GitHub Copilot's available models even if your config uses Ollama.

### In Interactive Chat Mode

During a chat session, use the special command:

```
/models list
```

The current model will be highlighted in green.

## Viewing Model Details

### Using the CLI

To see detailed information about a specific model:

```bash
xzatoma models info --model gpt-4o
```

Example output:

```
Model Information (GPT-4o)

Name:      gpt-4o
Display Name:  GPT-4o
Context Window: 128000 tokens
Capabilities:  FunctionCalling, LongContext
```

### For Ollama Models

Ollama provides additional metadata:

```bash
xzatoma models info --model granite4:3b --provider ollama
```

Example output:

```
Model Information (Llama 3.2 13B)

Name:      granite4:3b
Display Name:  Llama 3.2 13B
Context Window: 8192 tokens
Capabilities:  FunctionCalling

Provider-Specific Metadata:
 family: llama3.2
 parameter_size: 13B
 quantization: Q4_0
```

## Checking the Current Model

### Using the CLI

To see which model is currently active:

```bash
xzatoma models current
```

Example output:

```
Current Model Information

Provider:    ollama
Active Model:  granite4:3b
```

### With Provider Override

```bash
xzatoma models current --provider copilot
```

### In Interactive Chat Mode

Use the `/context` command to see the current model along with context usage:

```
/context
```

Example output:

```
╔════════════════════════════════════╗
║   Context Window Information   ║
╚════════════════════════════════════╝

Current Model:   granite4:3b
Context Window:  32768 tokens
Tokens Used:    2450 tokens
Remaining:     30318 tokens
Usage:       7.5%

Usage Level:    7.5%
```

## Understanding Model Capabilities

Models can have different capabilities:

- **FunctionCalling**: Model supports tool use and function calling
- **LongContext**: Model has extended context window (typically 100k+ tokens)
- **Vision**: Model can process and understand images
- **Streaming**: Model supports streaming responses
- **JsonMode**: Model can output structured JSON

### Checking if a Model Supports a Feature

When listing models, check the Capabilities column:

```bash
xzatoma models list
```

Models with `FunctionCalling` can use XZatoma's tools (file operations, terminal commands, etc.).

## Choosing the Right Model

### For Code Generation

Use models optimized for code:

- `llama3.2:3b` (Ollama) - Good balance of speed and quality
- `granite4:3b` (Ollama) - Higher quality, slower
- `gpt-4o` (Copilot) - Excellent quality, cloud-based

### For Large Context Tasks

Use models with large context windows:

- `gemini-2.0-flash-exp` (Copilot) - 1M token context
- `claude-3.5-sonnet` (Copilot) - 200k token context
- `gpt-4o` (Copilot) - 128k token context

### For Quick Tasks

Use smaller, faster models:

- `llama3.2:3b` (Ollama) - Fast local inference
- `gpt-4-turbo` (Copilot) - Fast cloud inference

## Provider Differences

### GitHub Copilot

- Models are discovered dynamically via Copilot's `/models` endpoint
- No token usage reporting
- Always available (cloud-based)
- Requires authentication

Available models:

- gpt-4o
- gpt-4-turbo
- gpt-4
- o1-preview
- o1-mini
- claude-3.5-sonnet
- gemini-2.0-flash-exp

### Ollama

- Models discovered dynamically from local installation
- Full model metadata available
- Reports accurate token usage
- Requires Ollama service running
- Models must be installed locally

Install models with:

```bash
ollama pull granite4:3b
ollama pull llama3.2:3b
```

## Troubleshooting

### No Models Available

**Problem**: `xzatoma models list` shows no models

**For Ollama**:

1. Check if Ollama is running:

```bash
curl http://localhost:11434/api/tags
```

2. Install a model if none are available:

```bash
ollama pull granite4:3b
```

3. Check Ollama host in config:

```yaml
provider:
  ollama:
    host: http://localhost:11434
```

**For Copilot**:

1. Verify authentication:

```bash
xzatoma auth --provider copilot
```

2. Check provider configuration:

```yaml
provider:
  provider_type: copilot
```

### Model Not Found

**Problem**: `Model 'xyz' not found` error

**Solution**:

1. List available models first:

```bash
xzatoma models list
```

2. Use exact model name from the list
3. Model names are case-sensitive

### Connection Errors

**Problem**: `Failed to connect to Ollama` or similar errors

**For Ollama**:

1. Ensure Ollama is running:

```bash
ollama list
```

2. Start Ollama if needed:

```bash
ollama serve
```

3. Check firewall settings if using custom host

**For Copilot**:

1. Check internet connection
2. Verify authentication token is valid
3. Re-authenticate if needed:

```bash
xzatoma auth --provider copilot
```

## Configuration

### Setting Default Model

Edit your `config/config.yaml`:

```yaml
provider:
  provider_type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest # Default model
```

### Multiple Providers

You can configure multiple providers and switch between them:

```yaml
provider:
  provider_type: ollama # Active provider

  copilot:
    model: gpt-4o

  ollama:
    host: http://localhost:11434
    model: llama3.2:latest
```

Use `--provider` flag to override:

```bash
xzatoma models list --provider copilot
```

## Best Practices

1. **Check Available Models First**: Always list models before trying to use a specific one

2. **Use Model Info for Planning**: Check context window size before starting large tasks

3. **Monitor Context Usage**: Use `/context` in chat mode to track token usage

4. **Match Model to Task**: Use appropriate models for different task types

5. **Keep Ollama Models Updated**: Regularly update local models for improvements

6. **Verify Capabilities**: Check that the model supports required features (function calling, etc.)

## Examples

### Explore All Available Models

```bash
# List models from configured provider
xzatoma models list

# List models from Copilot
xzatoma models list --provider copilot

# List models from Ollama
xzatoma models list --provider ollama
```

### Inspect Model Before Using

```bash
# Get details
xzatoma models info --model granite4:3b

# Check current model
xzatoma models current

# Start chat and check context
xzatoma chat
> /context
```

### Exporting Model Data

You can export model data for analysis, reporting, or integration with other tools using the `--json` and `--summary` flags.

- Export all models to pretty JSON:

```bash
xzatoma models list --json > all_models.json
```

- Export all models including summary fields (state, limits, capabilities, raw provider data when available):

```bash
xzatoma models list --json --summary > all_models_with_summary.json
```

- Export details for a single model:

```bash
xzatoma models info --model granite4:3b --json > granite4:3bb.json
```

- Export a model's summary (compact, but serialized to JSON):

```bash
xzatoma models info --model gpt-4 --json --summary > gpt4_summary.json
```

Tips:

- The JSON output is pretty-printed and intended for human readability or piping into tools like `jq`.
- The `--summary` flag augments outputs with additional fields such as `state`, `max_prompt_tokens`, `max_completion_tokens`, `supports_tool_calls`, `supports_vision`, and a `raw_data` field that mirrors provider API payloads when present.

---

### Comparing Models

There are two common approaches to comparing models: manual inspection and script-based comparison.

1. View a side-by-side detailed comparison (manual)

```bash
# Dump model info to JSON for manual inspection
xzatoma models info --model gpt-4 --json > gpt4.json
xzatoma models info --model granite4:3b --json > granite4:3b.json

# Inspect specific fields
jq '.context_window, .capabilities, .provider_specific' gpt4.json
jq '.context_window, .capabilities, .provider_specific' granite4:3b.json
```

2. Script-based comparison using `jq`

- List models and show name + context window (sorted by context window):

```bash
xzatoma models list --json | jq -r '.[] | "\(.display_name)\t\(.context_window)"' | sort -k2 -n -r
```

- Compare support for tool calls across models (uses `--summary` to expose `supports_tool_calls`):

```bash
xzatoma models list --json --summary \
  | jq -r '.[] | "\(.info.name)\t\(if .supports_tool_calls == true then \"yes\" elif .supports_tool_calls == false then \"no\" else \"unknown\" end)"'
```

- Create a tabular view for quick comparison (requires `column`):

```bash
xzatoma models list --json \
  | jq -r '.[] | "\(.name)\t\(.display_name)\t\(.context_window)\t\(.capabilities|join(\",\"))"' \
  | column -t -s $'\t'
```

3. Tips for robust comparisons

- Use `--json --summary` to ensure summary fields (like `supports_tool_calls` and `supports_vision`) are available in the exported data.
- When comparing across providers, include the `--provider` flag or filter the JSON output by provider-specific identifiers.
- For automated scripts, rely on JSON fields rather than human-oriented tables (tables are for quick manual inspection).

---

These short recipes cover exporting and comparing model metadata. If you'd like, I can add a small example script in `examples/` that demonstrates the export + `jq` comparison workflow.
xzatoma models list --provider ollama | grep "Context Window"

# View specific model details

xzatoma models info --model gpt-4o --provider copilot
xzatoma models info --model granite4:3b --provider ollama

```

## Next Steps

- Learn how to switch models: See [How to Switch Models](switch_models.md)
- Understand the API: See [Model Management API Reference](../reference/model_management.md)
- Learn about providers: See [Provider API Comparison](../reference/provider_api_comparison.md)

## Related Commands

- `xzatoma chat` - Start interactive chat mode
- `xzatoma auth` - Authenticate with a provider
- `xzatoma run` - Execute a plan file

## See Also

- Model Management API Reference: `docs/reference/model_management.md`
- Configuration Guide: `docs/reference/quick_reference.md`
- Chat Modes Guide: `docs/how-to/use_chat_modes.md`
```
