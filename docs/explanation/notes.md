# Notes


## Ollama default model

Trying to switch models in Ollama provider to a model exists and I got this error.

```bash
/models llama3:latest
2026-01-22T21:19:04.105040Z  INFO xzatoma::agent::core: Starting agent execution
2026-01-22T21:19:04.109755Z ERROR xzatoma::providers::ollama: Ollama returned error 404 Not Found: {"error":"model 'qwen2.5-coder' not found"}
Error: Provider error: Ollama returned error 404 Not Found: {"error":"model 'qwen2.5-coder' not found"}
```

First observation is the model did not change. Second observation is the error message says `qwen2.5-coder` not found, which is strange because I did not request that model.

It seems we have hardcoded `qwen2.5-coder` as default model in Ollama provider. Change the default to `llama3:latest`. Remove all references to `qwen2.5-coder` or any model that has `qwen` in its name.

For Ollama documentation the only models that should be referenced are:

- `llama3.2:latest`
- `granite4:latest`

/models granite4:latest
2026-01-22T21:46:51.843590Z  INFO xzatoma::agent::core: Starting agent execution
2026-01-22T21:46:51.914299Z ERROR xzatoma::providers::ollama: Ollama returned error 400 Bad Request: {"error":"registry.ollama.ai/library/llama3:latest does not support tools"}
Error: Provider error: Ollama returned error 400 Bad Request: {"error":"registry.ollama.ai/library/llama3:latest does not support tools"}

## Copilot Models

Where are you getting this list of copilot models? This is not the list of models available in Copilot.

+----------------------------------------------------------------------------------------------+
| Model Name       Display Name     Context Window  Capabilities                       |
| gpt-4              GPT-4              8192 tokens       FunctionCalling                      |
| gpt-4-turbo        GPT-4 Turbo        128000 tokens     FunctionCalling, LongContext         |
| gpt-3.5-turbo      GPT-3.5 Turbo      4096 tokens       FunctionCalling                      |
| claude-3.5-sonnet  Claude 3.5 Sonnet  200000 tokens     FunctionCalling, LongContext, Vision |
| claude-sonnet-4.5  Claude Sonnet 4.5  200000 tokens     FunctionCalling, LongContext, Vision |
| o1-preview         OpenAI o1 Preview  128000 tokens     FunctionCalling, LongContext         |
| o1-mini            OpenAI o1 Mini     128000 tokens     FunctionCalling, LongContext         |
+----------------------------------------------------------------------------------------------+

Actual models response from Copilot:

@testdata/models.json

THis is how you do it in python:

```python
# Copilot API endpoints
COPILOT_API_BASE = "https://api.githubcopilot.com"
COPILOT_CHAT_COMPLETIONS_URL = "/chat/completions"
COPILOT_MODELS_URL = "/models"

# Default model configuration
DEFAULT_COPILOT_MODEL = "gpt-5-mini"

client = await self._get_client()
response = await client.get(COPILOT_MODELS_URL)
response.raise_for_status()
data = response.json()

models_dict = {}
for model in data.get("data", []):
    if model.get("policy", {}).get("state") == "enabled":
        model_id = model.get("id")
        if model_id:
            models_dict[model_id] = model

return models_dict
except httpx.RequestError as e:
raise ProviderError(f"Failed to get model details: {e}") from e
```

```text
/models
2026-01-22T22:17:51.972639Z  INFO xzatoma::agent::core: Starting agent execution
2026-01-22T22:17:59.215180Z  INFO xzatoma::providers::copilot: Starting GitHub OAuth device flow

GitHub Authentication Required:
  1. Visit: https://github.com/login/device
  2. Enter code: D423-9B55

Waiting for authorization...
Authorization successful!
2026-01-22T22:18:29.754700Z  INFO xzatoma::providers::copilot: GitHub OAuth device flow completed successfully
2026-01-22T22:18:32.064059Z ERROR xzatoma::providers::copilot: Failed to parse Copilot response: error decoding response body: missing field `content` at line 1 column 319
Error: Provider error: Failed to parse Copilot response: error decoding response body: missing field `content` at line 1 column 319
```

## Display what model is being used

When starting an agent execution, display what model is being used for the provider. For example:

```text
[PLANNING][SAFE] >>
```

Should become:

```text
[PLANNING][SAFE][Copilot: gpt-5-mini] >>>
```
